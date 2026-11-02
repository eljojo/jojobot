//! **Teaching served from the event log.** The marks live in a
//! [`State`] held in memory; a contact or a clear is prepared against it,
//! appended to the log as one batch, and applied to the state from the value
//! that was appended. After boot nothing reads the old teaching rows (decision
//! log 292). **The log's own tail is read before every write**, once, to catch up
//! a commit whose answer was lost; no other read of the store is made.
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

use crate::writer::Writer;

use crate::dolt_log::{Companion, DoltLog};
use crate::fold::teachings::{held_after_contact, held_after_forget};

pub mod codec;

/// **The teaching ledger a boot serves.** It is served from the event log. The
/// log's connection holds the one-writer lock, so a second process cannot start
/// beside this one. The old rows seed the first boot's import; and when the
/// shadow names the teaching family they are also kept current and compared,
/// which is the default (see the feature registry). One function for the boot and
/// for the case that boots it with no instance record.
pub async fn serving(
    pool: &sqlx::MySqlPool,
    shadow: Arc<crate::fold::shadow::Shadow>,
    writer: Writer,
) -> Result<LoggedTeachings, TeachingError> {
    let log = Arc::new(
        DoltLog::open(pool.clone())
            .await
            .map_err(|e| TeachingError::Store(format!("the event log is unavailable: {e}")))?,
    );
    let marks = crate::dolt::teaching::DoltTeachings::open(pool.clone())
        .marks()
        .await?;
    let ledger = if shadow.watches(crate::fold::shadow::Family::Teachings) {
        LoggedTeachings::open_shadowed(log, &marks, shadow).await?
    } else {
        LoggedTeachings::open(log, &marks).await?
    };
    Ok(ledger.with_writer(writer))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod witness_tests;

const REPRESENTATION_VERSION: u32 = 1;
const READ_BOUND: usize = 256;
/// Eager recovery after an unknown append: this many tail reads, waiting twice
/// as long before each as before the one ahead of it.
const RECOVERY_ATTEMPTS: u32 = 4;
const RECOVERY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);

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
    /// The runs of a batch whose append outcome is unknown. Their reads refuse
    /// until a tail read the log resolves; every other run serves the held state.
    uncertain: BTreeSet<RunId>,
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

/// The old path beside the log: its held set, run through the old transition
/// code, and the legacy rows its statements keep current in the batch's own
/// transaction.
struct Shadow {
    log: Arc<dyn CompanionLog>,
    legacy: BTreeSet<(Sid, String)>,
    report: Arc<crate::fold::shadow::Shadow>,
}

impl Shadow {
    fn compare(&self, sid: &Sid, domain: &str, log_holds: bool) {
        let old_holds = self.legacy.contains(&(sid.clone(), domain.to_string()));
        self.report
            .compared_teaching(sid.as_str(), domain, log_holds, old_holds);
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
    writer: Writer,
    log: Arc<dyn LogStore>,
    held: Arc<Mutex<Held>>,
    /// **The pairs the ledger holds for runs whose reads are certain**, readable
    /// without the held mutex, which covers log I/O. A pair here is answered
    /// before the writer queue; see [`LoggedTeachings::publish`].
    known: Arc<std::sync::RwLock<Known>>,
    /// The shadow's report, so a contact answered from `known` is compared with
    /// the old path as every other contact is.
    report: Option<Arc<crate::fold::shadow::Shadow>>,
}

/// What a first contact reads without the held mutex.
#[derive(Default)]
struct Known {
    /// The log's pairs for runs whose reads are certain.
    log: BTreeSet<(String, String)>,
    /// The old path's pairs, when a shadow runs.
    legacy: BTreeSet<(String, String)>,
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
            writer: Writer::default(),
            log,
            held: Arc::new(Mutex::new(Held {
                state: State::default(),
                head: Position::default(),
                shadow: None,
                uncertain: BTreeSet::new(),
            })),
            known: Arc::default(),
            report: None,
        };
        this.fill(marks).await?;
        Ok(this)
    }

    /// **Open with the old path as a shadow.** Every batch carries the legacy
    /// row statements, and `marks` seed the old held set. The counts say how
    /// often the two answered and how often they differed, on `report`.
    pub async fn open_shadowed<L: CompanionLog + 'static>(
        log: Arc<L>,
        marks: &[Mark],
        report: Arc<crate::fold::shadow::Shadow>,
    ) -> Result<Self, TeachingError> {
        let this = Self {
            writer: Writer::default(),
            log: log.clone(),
            report: Some(report.clone()),
            held: Arc::new(Mutex::new(Held {
                state: State::default(),
                head: Position::default(),
                uncertain: BTreeSet::new(),
                shadow: Some(Shadow {
                    log,
                    legacy: marks
                        .iter()
                        .map(|m| (m.sid.clone(), m.domain.clone()))
                        .collect(),
                    report,
                }),
            })),
            known: Arc::default(),
        };
        this.fill(marks).await?;
        Ok(this)
    }

    /// Share the serving process's writer with the other rails, so a write
    /// here queues with theirs and finishes even when its caller stops waiting.
    pub fn with_writer(mut self, writer: Writer) -> Self {
        self.writer = writer;
        self
    }

    async fn fill(&self, marks: &[Mark]) -> Result<(), TeachingError> {
        let this = self;
        {
            let mut held = this.held.lock().await;
            this.catch_up(&mut held, false).await?;
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
            } else if held.head != Position::default() && held.shadow.is_some() {
                this.reconcile(&mut held, marks).await?;
            }
        }
        Ok(())
    }

    /// **Make the log hold what the rows hold, for a shadowed boot over a log that
    /// is not empty.** A rollback to the old path writes and clears rows the log
    /// never sees, and the roll forward opens the log as it was. A run whose log
    /// marks are not all in the rows is cleared and recorded again from the rows;
    /// a mark only the rows hold is recorded with its own time. Without this a
    /// first contact on such a pair builds an insert the table refuses, on every
    /// attempt. Runs only with the shadow on: with it off the rows are stale by
    /// design. Nothing is appended when the two agree.
    async fn reconcile(&self, held: &mut Held, marks: &[Mark]) -> Result<(), TeachingError> {
        use std::collections::{BTreeMap, BTreeSet};
        let mut rows: BTreeMap<&str, BTreeMap<&str, Timestamp>> = BTreeMap::new();
        for mark in marks {
            rows.entry(mark.sid.as_str())
                .or_default()
                .insert(mark.domain.as_str(), mark.taught_at);
        }
        let mut logged: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
        for (run, domain) in held.state.taught() {
            logged.entry(run.0.as_str()).or_default().insert(domain);
        }
        let runs: BTreeSet<&str> = rows.keys().chain(logged.keys()).copied().collect();
        let mut events = Vec::new();
        for name in runs {
            let row = rows.get(name);
            let in_log = logged.get(name);
            let log_has_extra = in_log.is_some_and(|domains| {
                domains
                    .iter()
                    .any(|domain| !row.is_some_and(|r| r.contains_key(domain)))
            });
            if log_has_extra {
                events.push(TeachingEvent::Cleared {
                    run: RunId(name.to_string()),
                });
            }
            for (domain, taught_at) in row.into_iter().flatten() {
                if log_has_extra || !in_log.is_some_and(|domains| domains.contains(domain)) {
                    events.push(TeachingEvent::Recorded {
                        run: RunId(name.to_string()),
                        domain: domain.to_string(),
                        taught_at: *taught_at,
                    });
                }
            }
        }
        if events.is_empty() {
            return Ok(());
        }
        let at = events
            .iter()
            .filter_map(|event| match event {
                TeachingEvent::Recorded { taught_at, .. } => Some(*taught_at),
                TeachingEvent::Cleared { .. } => None,
            })
            .max()
            .or_else(|| held.state.history().last().map(|(_, _, at)| *at))
            .unwrap_or_default();
        tracing::warn!(
            events = events.len(),
            "teaching: the old rows and the log disagree at boot (the old path ran in between); \
             the log now follows the rows"
        );
        self.commit(held, events, at, false).await
    }

    /// Apply every committed batch after the held head. A tail whose commit is
    /// unresolved is an error, so a write never prepares on a stale head.
    ///
    /// After boot, `moves_old` also moves the shadow's old held set: a commit
    /// whose answer was lost landed its legacy rows with the batch, so the old
    /// set follows what the log holds. At boot the old set comes from the rows.
    async fn catch_up(&self, held: &mut Held, moves_old: bool) -> Result<(), TeachingError> {
        loop {
            let batches = self
                .log
                .read_from(held.head, READ_BOUND)
                .await
                .map_err(refused)?;
            if batches.is_empty() {
                held.uncertain.clear();
                self.publish(held);
                return Ok(());
            }
            for batch in batches {
                for member in batch.members() {
                    let event = codec::decode(member).map_err(unreadable)?;
                    held.state.apply(&event);
                    if let Some(shadow) = held.shadow.as_mut().filter(|_| moves_old) {
                        shadow.follow(&event);
                    }
                }
                held.head = batch.range().last;
            }
        }
    }

    /// **Publish the pairs a first contact may answer without the writer.** A run
    /// whose append outcome is unknown is left out, so its contact still goes
    /// through the writer and meets the refusal. Called wherever the held state
    /// or the uncertain runs change, with the held mutex taken.
    fn publish(&self, held: &Held) {
        let log = held
            .state
            .taught()
            .filter(|(run, _)| !held.uncertain.contains(run))
            .map(|(run, domain)| (run.0.clone(), domain.clone()))
            .collect();
        let legacy = held
            .shadow
            .iter()
            .flat_map(|shadow| shadow.legacy.iter())
            .map(|(sid, domain)| (sid.as_str().to_string(), domain.clone()))
            .collect();
        *self
            .known
            .write()
            .expect("the known pairs are not poisoned") = Known { log, legacy };
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
        let appended = match appended {
            Err(LogError::Indeterminate) => Ok(Append::Indeterminate),
            other => other,
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
                self.publish(held);
                Ok(())
            }
            Append::DefinitelyNotCommitted => Err(TeachingError::Store(
                "the log did not commit the teaching batch".to_string(),
            )),
            Append::Indeterminate => {
                held.uncertain
                    .extend(events.iter().map(|event| match event {
                        TeachingEvent::Recorded { run, .. } | TeachingEvent::Cleared { run } => {
                            run.clone()
                        }
                    }));
                self.publish(held);
                self.recover(held).await;
                Err(TeachingError::Indeterminate)
            }
        }
    }

    /// **Fenced tail recovery, straight after an unknown append.** The log
    /// answers a tail read only once the outstanding append has an outcome, so a
    /// read that succeeds settles the uncertain runs ([`Self::catch_up`] clears
    /// them). When the log cannot say, the runs keep refusing and the next write
    /// retries; the wait is bounded so the writer's queue is not held for good.
    async fn recover(&self, held: &mut Held) {
        for attempt in 0..RECOVERY_ATTEMPTS {
            if self.catch_up(held, true).await.is_ok() {
                return;
            }
            tokio::time::sleep(RECOVERY_BACKOFF * 2u32.pow(attempt)).await;
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
        // **A pair already held writes nothing**, so it neither waits behind an
        // unrelated write on the shared writer nor reads the log's tail.
        let pair = (sid.as_str().to_string(), domain.to_string());
        {
            let known = self.known.read().expect("the known pairs are not poisoned");
            if known.log.contains(&pair) {
                if let Some(report) = &self.report {
                    report.compared_teaching(
                        sid.as_str(),
                        domain,
                        true,
                        known.legacy.contains(&pair),
                    );
                }
                return Ok(false);
            }
        }
        let this = self.clone();
        let (sid, domain) = (sid.clone(), domain.to_string());
        self.writer
            .run(async move { this.contact(&sid, &domain, at).await })
            .await
    }

    async fn has_contact(&self, sid: &Sid, domain: &str) -> Result<bool, TeachingError> {
        let held = self.held.lock().await;
        if held.uncertain.contains(&run(sid)) {
            return Err(TeachingError::Indeterminate);
        }
        Ok(held.state.has_contact(&run(sid), domain))
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        let this = self.clone();
        let sid = sid.clone();
        self.writer.run(async move { this.clear(&sid).await }).await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        let held = self.held.lock().await;
        if !held.uncertain.is_empty() {
            return Err(TeachingError::Indeterminate);
        }
        Ok(Some(
            held.state
                .taught()
                .map(|(run, domain)| (Sid(run.0.clone()), domain.clone()))
                .collect(),
        ))
    }
}

impl LoggedTeachings {
    async fn contact(&self, sid: &Sid, domain: &str, at: Timestamp) -> Result<bool, TeachingError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held, true).await?;
        let prepared = held
            .state
            .first_contact(&run(sid), domain, at)
            .map_err(|refusal| TeachingError::Refused(refusal.to_string()))?;
        if let Some(shadow) = held.shadow.as_ref() {
            shadow.compare(sid, domain, !prepared.answer);
        }
        if !prepared.events.is_empty() {
            self.commit(&mut held, prepared.events, at, true).await?;
        }
        Ok(prepared.answer)
    }

    async fn clear(&self, sid: &Sid) -> Result<(), TeachingError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held, true).await?;
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
}
