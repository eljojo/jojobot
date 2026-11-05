//! **The request and app ledgers served from the event log.** The ledgers live in
//! a [`State`] held in memory; a batch of ledger events is checked against a copy
//! of it, appended to the log as one batch, and applied to the held state from the
//! value that was appended. After open, a read is answered from the held state
//! and touches no store (decision log 292). **The log's own tail is read before
//! every write**, once, to catch up a commit whose answer was lost; no other read
//! of the store is made.
//!
//! **Open** replays the log. No rail calls this type yet: the old ledger tables
//! still serve every request, and this holds the family ready beside them.

use std::sync::Arc;

use jiff::Timestamp;
use jojobot_domain::log::{
    Append, BatchId, LogError, LogStore, ObjectId, Position, RequestId, SealedBatch,
};
use jojobot_domain::request::events::{AppAttempt, AppKey, Commit, LedgerEvent, Part, State};
use tokio::sync::Mutex;

use crate::writer::Writer;

pub mod codec;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod writer_log_tests;

const REPRESENTATION_VERSION: u32 = 1;
const READ_BOUND: usize = 256;
/// Eager recovery after an unknown append: this many tail reads, waiting twice
/// as long before each as before the one ahead of it.
const RECOVERY_ATTEMPTS: u32 = 4;
const RECOVERY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);

/// Why the ledger could not take or serve a batch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// The log or a stored event could not be read or written.
    #[error("the ledger could not reach the log: {0}")]
    Store(String),
    /// The batch is not valid against what the ledger holds. Nothing was appended.
    #[error("the ledger refused the batch: {0}")]
    Refused(String),
    /// Whether the batch committed is not known yet. The next write reads the
    /// tail and settles it.
    #[error("the ledger does not know whether the batch committed")]
    Indeterminate,
    /// Another writer took the position.
    #[error("another writer took the log position")]
    Conflict,
}

fn refused(error: LogError) -> LedgerError {
    match error {
        LogError::Indeterminate => LedgerError::Indeterminate,
        LogError::Conflict => LedgerError::Conflict,
        other => LedgerError::Store(other.to_string()),
    }
}

fn unreadable(error: codec::DecodeError) -> LedgerError {
    LedgerError::Store(error.to_string())
}

struct Held {
    state: State,
    head: Position,
}

/// The ledgers, answered from the log.
#[derive(Clone)]
pub struct LoggedLedger {
    writer: Writer,
    log: Arc<dyn LogStore>,
    held: Arc<Mutex<Held>>,
}

impl LoggedLedger {
    /// Replay the log into the held state.
    pub async fn open(log: Arc<dyn LogStore>) -> Result<Self, LedgerError> {
        let restored = crate::log_snapshot::boot(log.as_ref())
            .await
            .map_err(refused)?;
        let this = Self {
            writer: Writer::default(),
            log,
            held: Arc::new(Mutex::new(Held {
                state: restored.state.ledger,
                head: restored.position,
            })),
        };
        Ok(this)
    }

    /// Share the serving process's writer with the other rails, so a write
    /// here queues with theirs and finishes even when its caller stops waiting.
    pub fn with_writer(mut self, writer: Writer) -> Self {
        self.writer = writer;
        self
    }

    /// Apply every committed batch after the held head. A tail whose commit is
    /// unresolved is an error, so a write never prepares on a stale head.
    async fn catch_up(&self, held: &mut Held) -> Result<(), LedgerError> {
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
                    // Another family's member is that family's to read.
                    let Some(event) = codec::decode_own(member).map_err(unreadable)? else {
                        continue;
                    };
                    held.state
                        .apply(&event)
                        .map_err(|error| LedgerError::Store(error.to_string()))?;
                }
                held.head = batch.range().last;
            }
        }
    }

    /// **Append the events as one batch and apply them from the appended value.**
    /// The batch is checked against a copy of the held state first, so an event
    /// the log would hold and the state could not apply never reaches the log.
    /// The copy is the cost of that check; the seam that makes this a hot path
    /// replaces it with a check that does not copy.
    pub async fn append(&self, events: Vec<LedgerEvent>, at: Timestamp) -> Result<(), LedgerError> {
        if events.is_empty() {
            return Ok(());
        }
        let this = self.clone();
        self.writer
            .run(async move { this.commit(events, at).await })
            .await
    }

    async fn commit(&self, events: Vec<LedgerEvent>, at: Timestamp) -> Result<(), LedgerError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held).await?;
        let mut next = held.state.clone();
        for event in &events {
            next.apply(event)
                .map_err(|error| LedgerError::Refused(error.to_string()))?;
        }
        let members: Vec<Vec<u8>> = events.iter().map(codec::encode).collect();
        let identity = jojobot_domain::request::digest_of(&format!(
            "{}:{}",
            held.head.0,
            members
                .iter()
                .map(|member| String::from_utf8_lossy(member).into_owned())
                .collect::<Vec<_>>()
                .join("\n")
        ));
        let batch = SealedBatch::seal(
            BatchId(format!("ledger:{identity}")),
            held.head,
            at,
            REPRESENTATION_VERSION,
            members,
        )
        .map_err(refused)?;
        let appended = match self.log.append(held.head, &batch).await {
            Err(LogError::Indeterminate) => Ok(Append::Indeterminate),
            other => other,
        };
        match appended.map_err(refused)? {
            Append::Committed(range) => {
                held.state = next;
                held.head = range.last;
                Ok(())
            }
            Append::DefinitelyNotCommitted => Err(LedgerError::Store(
                "the log did not commit the ledger batch".to_string(),
            )),
            Append::Indeterminate => {
                self.recover(&mut held).await;
                Err(LedgerError::Indeterminate)
            }
        }
    }

    /// **Fenced tail recovery, straight after an unknown append.** The log
    /// answers a tail read only once the outstanding append has an outcome, so a
    /// read that succeeds settles the held state. When the log cannot say, the
    /// next write retries; the wait is bounded so the writer's queue is not held
    /// for good.
    async fn recover(&self, held: &mut Held) {
        for attempt in 0..RECOVERY_ATTEMPTS {
            if self.catch_up(held).await.is_ok() {
                return;
            }
            tokio::time::sleep(RECOVERY_BACKOFF * 2u32.pow(attempt)).await;
        }
    }

    /// The whole write's commit under an id, while it is held and unexpired.
    pub async fn lookup(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Option<Commit> {
        let held = self.held.lock().await;
        held.state.lookup(actor, id, at).cloned()
    }

    /// The parts of the write under an id, oldest first, while they are held.
    pub async fn parts(&self, actor: &ObjectId, id: &RequestId, at: Timestamp) -> Vec<Part> {
        let held = self.held.lock().await;
        held.state
            .parts(actor, id, at)
            .into_iter()
            .cloned()
            .collect()
    }

    /// An app attempt under its key.
    pub async fn app(&self, key: &AppKey) -> Option<AppAttempt> {
        let held = self.held.lock().await;
        held.state.app(key).cloned()
    }

    /// The log position the held state has applied through.
    pub async fn head(&self) -> Position {
        self.held.lock().await.head
    }
}
