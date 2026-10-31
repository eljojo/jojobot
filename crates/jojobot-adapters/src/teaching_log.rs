//! **Teaching served from the event log.** The marks live in a
//! [`State`] held in memory; a contact or a clear is prepared against it,
//! appended to the log as one batch, and applied to the state from the value
//! that was appended. After boot nothing reads the store (decision log 292).
//!
//! **Boot** replays the log. A log with no batch is filled once from the
//! teaching rows ([`LoggedTeachings::open`]), each mark keeping its own source
//! time.

use std::sync::Arc;

use jiff::Timestamp;
use jojobot_domain::log::{Append, BatchId, LogError, LogStore, Position, SealedBatch};
use jojobot_domain::session::Sid;
use jojobot_domain::teaching::events::{RunId, State, TeachingEvent};
use jojobot_domain::teaching::{TeachingError, Teachings};
use std::collections::BTreeSet;
use tokio::sync::Mutex;

use crate::dolt_log::{Companion, DoltLog};
use crate::fold::teachings::{held_after_contact, held_after_forget};

pub mod codec;

#[cfg(test)]
mod tests;

const REPRESENTATION_VERSION: u32 = 1;
const READ_BOUND: usize = 256;

/// One stored teaching mark, as the legacy rows hold it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub sid: Sid,
    pub domain: String,
    pub taught_at: Timestamp,
}

struct Held {
    state: State,
    head: Position,
    shadow: Option<Shadow>,
}

/// A log that commits legacy-row statements with its batches.
#[async_trait::async_trait]
pub trait CompanionLog: LogStore {
    async fn append_with(
        &self,
        head: Position,
        batch: &SealedBatch,
        companions: Vec<Companion>,
    ) -> Result<Append, LogError>;
}

#[async_trait::async_trait]
impl CompanionLog for DoltLog {
    async fn append_with(
        &self,
        head: Position,
        batch: &SealedBatch,
        companions: Vec<Companion>,
    ) -> Result<Append, LogError> {
        DoltLog::append_with(self, head, batch, companions).await
    }
}

/// What the shadow has counted: reads compared against the old held set, and
/// the ones that differed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShadowCounts {
    pub comparisons: u64,
    pub disagreements: u64,
}

/// The old path beside the log: its held set, run through the old transition
/// code, and the legacy rows its statements keep current in the batch's own
/// transaction.
struct Shadow {
    log: Arc<dyn CompanionLog>,
    legacy: BTreeSet<(Sid, String)>,
    counts: ShadowCounts,
}

impl Shadow {
    fn compare(&mut self, sid: &Sid, domain: &str, log_holds: bool) {
        self.counts.comparisons += 1;
        let legacy_holds = self.legacy.contains(&(sid.clone(), domain.to_string()));
        if legacy_holds != log_holds {
            self.counts.disagreements += 1;
            tracing::error!(
                run = sid.as_str(),
                domain,
                log_holds,
                legacy_holds,
                "teaching shadow: the log and the old held set disagree (the log is served)"
            );
        }
    }

    fn statement(event: &TeachingEvent) -> Companion {
        match event {
            TeachingEvent::Recorded {
                run,
                domain,
                taught_at,
            } => Companion {
                sql: "INSERT INTO session_teaching (sid, domain, taught_at) VALUES (?, ?, ?)"
                    .into(),
                binds: vec![run.0.clone(), domain.clone(), taught_at.to_string()],
            },
            TeachingEvent::Cleared { run } => Companion {
                sql: "DELETE FROM session_teaching WHERE sid = ?".into(),
                binds: vec![run.0.clone()],
            },
        }
    }

    fn follow(&mut self, event: &TeachingEvent) {
        match event {
            TeachingEvent::Recorded { run, domain, .. } => {
                held_after_contact(&mut self.legacy, (Sid(run.0.clone()), domain.clone()))
            }
            TeachingEvent::Cleared { run } => {
                held_after_forget(&mut self.legacy, &Sid(run.0.clone()))
            }
        }
    }
}

/// The teaching ledger, answered from the log.
#[derive(Clone)]
pub struct LoggedTeachings {
    log: Arc<dyn LogStore>,
    held: Arc<Mutex<Held>>,
}

fn run(sid: &Sid) -> RunId {
    RunId(sid.as_str().to_string())
}

fn refused(error: LogError) -> TeachingError {
    match error {
        LogError::Indeterminate => TeachingError::Indeterminate,
        LogError::Conflict => TeachingError::Conflict,
        other => TeachingError::Store(other.to_string()),
    }
}

fn unreadable(error: codec::DecodeError) -> TeachingError {
    TeachingError::Store(error.to_string())
}

impl LoggedTeachings {
    /// Replay the log. When it holds no batch, append `marks` as the first
    /// batch, in `(sid, domain)` order, each with its own `taught_at`.
    pub async fn open(log: Arc<dyn LogStore>, marks: &[Mark]) -> Result<Self, TeachingError> {
        let this = Self {
            log,
            held: Arc::new(Mutex::new(Held {
                state: State::default(),
                head: Position::default(),
                shadow: None,
            })),
        };
        this.fill(marks).await?;
        Ok(this)
    }

    /// **Open with the old path as a shadow.** Every batch carries the legacy
    /// row statements, and `marks` seed the old held set. The counts say how
    /// often the two answered and how often they differed.
    pub async fn open_shadowed<L: CompanionLog + 'static>(
        log: Arc<L>,
        marks: &[Mark],
    ) -> Result<Self, TeachingError> {
        let this = Self {
            log: log.clone(),
            held: Arc::new(Mutex::new(Held {
                state: State::default(),
                head: Position::default(),
                shadow: Some(Shadow {
                    log,
                    legacy: marks
                        .iter()
                        .map(|m| (m.sid.clone(), m.domain.clone()))
                        .collect(),
                    counts: ShadowCounts::default(),
                }),
            })),
        };
        this.fill(marks).await?;
        Ok(this)
    }

    /// What the shadow has counted; zero when there is none.
    pub async fn shadow_counts(&self) -> ShadowCounts {
        self.held
            .lock()
            .await
            .shadow
            .as_ref()
            .map(|s| s.counts)
            .unwrap_or_default()
    }

    async fn fill(&self, marks: &[Mark]) -> Result<(), TeachingError> {
        let this = self;
        {
            let mut held = this.held.lock().await;
            this.catch_up(&mut held).await?;
            if held.head == Position::default() && !marks.is_empty() {
                let mut ordered = marks.to_vec();
                ordered.sort_by(|a, b| (&a.sid, &a.domain).cmp(&(&b.sid, &b.domain)));
                let at = ordered
                    .iter()
                    .map(|m| m.taught_at)
                    .max()
                    .unwrap_or_default();
                let events: Vec<TeachingEvent> = ordered
                    .into_iter()
                    .map(|mark| TeachingEvent::Recorded {
                        run: run(&mark.sid),
                        domain: mark.domain,
                        taught_at: mark.taught_at,
                    })
                    .collect();
                this.commit(&mut held, events, at, false).await?;
            }
        }
        Ok(())
    }

    /// Apply every committed batch after the held head. A tail whose commit is
    /// unresolved is an error, so a write never prepares on a stale head.
    async fn catch_up(&self, held: &mut Held) -> Result<(), TeachingError> {
        loop {
            let batches = self
                .log
                .read_from(held.head, READ_BOUND)
                .await
                .map_err(refused)?;
            if batches.is_empty() {
                return Ok(());
            }
            for batch in batches {
                for member in batch.members() {
                    held.state
                        .apply(&codec::decode(member).map_err(unreadable)?);
                }
                held.head = batch.range().last;
            }
        }
    }

    /// Append the events as one batch and apply them from the appended value.
    async fn commit(
        &self,
        held: &mut Held,
        events: Vec<TeachingEvent>,
        at: Timestamp,
        with_rows: bool,
    ) -> Result<(), TeachingError> {
        let members: Vec<Vec<u8>> = events.iter().map(codec::encode).collect();
        let identity = jojobot_domain::request::digest_of(&format!(
            "{}:{}",
            held.head.0,
            members
                .iter()
                .map(|m| String::from_utf8_lossy(m).into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        let batch = SealedBatch::seal(
            BatchId(format!("teaching:{identity}")),
            held.head,
            at,
            REPRESENTATION_VERSION,
            members,
        )
        .map_err(refused)?;
        // The import's rows already exist, so only a live write carries them.
        let appended = match held.shadow.as_ref().filter(|_| with_rows) {
            Some(shadow) => {
                let companions = events.iter().map(Shadow::statement).collect();
                shadow.log.append_with(held.head, &batch, companions).await
            }
            None => self.log.append(held.head, &batch).await,
        };
        match appended.map_err(refused)? {
            Append::Committed(range) => {
                for event in &events {
                    held.state.apply(event);
                    if let Some(shadow) = held.shadow.as_mut() {
                        shadow.follow(event);
                    }
                }
                held.head = range.last;
                Ok(())
            }
            Append::DefinitelyNotCommitted => Err(TeachingError::Store(
                "the log did not commit the teaching batch".to_string(),
            )),
            Append::Indeterminate => Err(TeachingError::Indeterminate),
        }
    }
}

#[async_trait::async_trait]
impl Teachings for LoggedTeachings {
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held).await?;
        let prepared = held
            .state
            .first_contact(&run(sid), domain, at)
            .map_err(|refusal| TeachingError::Refused(refusal.to_string()))?;
        if let Some(shadow) = held.shadow.as_mut() {
            shadow.compare(sid, domain, !prepared.answer);
        }
        if !prepared.events.is_empty() {
            self.commit(&mut held, prepared.events, at, true).await?;
        }
        Ok(prepared.answer)
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        Ok(self.held.lock().await.state.has_contact(&run(sid), domain))
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held).await?;
        let prepared = held.state.forget(&run(sid));
        if !prepared.events.is_empty() {
            // A clear has no source time of its own; the newest mark's time
            // keeps the batch deterministic.
            let at = held
                .state
                .history()
                .last()
                .map(|(_, _, at)| *at)
                .unwrap_or_default();
            self.commit(&mut held, prepared.events, at, true).await?;
        }
        Ok(())
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        Ok(Some(
            self.held
                .lock()
                .await
                .state
                .taught()
                .map(|(run, domain)| (Sid(run.0.clone()), domain.clone()))
                .collect(),
        ))
    }
}
