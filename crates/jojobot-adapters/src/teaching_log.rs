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

use crate::writer_log::WriterLog;

pub mod codec;

/// **The teaching ledger a boot serves.** It is served from the event log. The
/// writer's connection holds the one-writer lock, so a second process cannot
/// start beside this one. The old rows seed the first boot's import. One function for the boot and
/// for the case that boots it with no instance record.
pub async fn serving(
    pool: &sqlx::MySqlPool,
    writer: Writer,
) -> Result<LoggedTeachings, TeachingError> {
    let log = Arc::new(
        WriterLog::open(pool.clone(), writer.clone())
            .await
            .map_err(|e| TeachingError::Store(format!("the event log is unavailable: {e}")))?,
    );
    let marks = import_marks(pool).await?;
    let ledger = LoggedTeachings::open(log, &marks).await?;
    Ok(ledger.with_writer(writer))
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod witness_tests;

#[cfg(test)]
mod writer_log_tests;

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

/// TEMPORARY(9.3.10) the one-way import of pre-log teaching marks, retained
/// until the explicit upgrade support boundary. A time that cannot be read
/// stops the import rather than inventing one.
pub async fn import_marks(pool: &sqlx::MySqlPool) -> Result<Vec<Mark>, TeachingError> {
    let rows: Vec<(String, String, String)> =
        sqlx::query_as("SELECT sid, domain, taught_at FROM session_teaching ORDER BY sid, domain")
            .fetch_all(pool)
            .await
            .map_err(|error| TeachingError::Store(error.to_string()))?;
    rows.into_iter().map(|(sid, domain, taught_at)| {
        let taught_at = taught_at.parse().map_err(|e| TeachingError::Store(format!(
            "the stored teaching time of run {sid:?} for domain {domain:?} is unreadable, so the teaching rows cannot be imported: {e}"
        )))?;
        Ok(Mark { sid: Sid(sid), domain, taught_at })
    }).collect()
}

struct Held {
    state: State,
    head: Position,
    /// The runs of a batch whose append outcome is unknown. Their reads refuse
    /// until a tail read the log resolves; every other run serves the held state.
    uncertain: BTreeSet<RunId>,
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
}

/// Publishing uncertainty beside the pairs keeps an unknown new run from
/// reading as absent while recovery holds the state mutex over I/O.
#[derive(Default)]
struct Known {
    /// The log's pairs for runs whose reads are certain.
    log: BTreeSet<(String, String)>,
    uncertain: BTreeSet<RunId>,
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
                uncertain: BTreeSet::new(),
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
            let restored = crate::log_snapshot::boot(this.log.as_ref())
                .await
                .map_err(refused)?;
            held.state = restored.state.teaching;
            held.head = restored.position;
            this.publish(&held);
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
                this.commit(&mut held, events, at).await?;
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
                held.uncertain.clear();
                self.publish(held);
                return Ok(());
            }
            for batch in batches {
                for member in batch.members() {
                    // Another family's member is that family's to read.
                    let Some(event) = codec::decode_own(member).map_err(unreadable)? else {
                        continue;
                    };
                    held.state.apply(&event);
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
        *self
            .known
            .write()
            .expect("the known pairs are not poisoned") = Known {
            log,
            uncertain: held.uncertain.clone(),
        };
    }

    /// Append the events as one batch and apply them from the appended value.
    async fn commit(
        &self,
        held: &mut Held,
        events: Vec<TeachingEvent>,
        at: Timestamp,
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
        let appended = self.log.append(held.head, &batch).await;
        let appended = match appended {
            Err(LogError::Indeterminate) => Ok(Append::Indeterminate),
            other => other,
        };
        match appended.map_err(refused)? {
            Append::Committed(range) => {
                for event in &events {
                    held.state.apply(event);
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
            if self.catch_up(held).await.is_ok() {
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
        let known = self.known.read().expect("the known pairs are not poisoned");
        if known.uncertain.contains(&run(sid)) {
            return Err(TeachingError::Indeterminate);
        }
        Ok(known
            .log
            .contains(&(sid.as_str().to_string(), domain.to_string())))
    }

    async fn forget(&self, sid: &Sid) -> Result<(), TeachingError> {
        let this = self.clone();
        let sid = sid.clone();
        self.writer.run(async move { this.clear(&sid).await }).await
    }

    async fn taught(&self) -> Result<Option<Vec<(Sid, String)>>, TeachingError> {
        let known = self.known.read().expect("the known pairs are not poisoned");
        if !known.uncertain.is_empty() {
            return Err(TeachingError::Indeterminate);
        }
        Ok(Some(
            known
                .log
                .iter()
                .map(|(sid, domain)| (Sid(sid.clone()), domain.clone()))
                .collect(),
        ))
    }
}

impl LoggedTeachings {
    async fn contact(&self, sid: &Sid, domain: &str, at: Timestamp) -> Result<bool, TeachingError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held).await?;
        let prepared = held
            .state
            .first_contact(&run(sid), domain, at)
            .map_err(|refusal| TeachingError::Refused(refusal.to_string()))?;
        if !prepared.events.is_empty() {
            self.commit(&mut held, prepared.events, at).await?;
        }
        Ok(prepared.answer)
    }

    async fn clear(&self, sid: &Sid) -> Result<(), TeachingError> {
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
            self.commit(&mut held, prepared.events, at).await?;
        }
        Ok(())
    }
}
