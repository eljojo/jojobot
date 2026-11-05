//! **The claims family served from the event log.** Claim, field and identity
//! events are stored as the log envelope with a typed payload ([`codec`]). The
//! claims [`State`] is held in memory; a batch of claims events is appended to the
//! log as one batch and applied to the held state from the value that was
//! appended. After open, a read is answered from the held state and touches no
//! store (decision log 292). **The log's own tail is read before every write**,
//! once, to catch up a commit whose answer was lost; no other read of the store is
//! made.
//!
//! **Open** replays the log. No rail calls this type yet: the old tables still
//! serve every claim, and this holds the family ready beside them.

use std::sync::Arc;

use jiff::Timestamp;
use jojobot_domain::log::claims::{ClaimEvent, State};
use jojobot_domain::log::{Append, BatchId, Event, LogError, LogStore, Position, SealedBatch};
use tokio::sync::Mutex;

use crate::writer::Writer;

pub mod codec;

#[cfg(test)]
mod codec_tests;

#[cfg(test)]
mod tests;

const REPRESENTATION_VERSION: u32 = 1;
const READ_BOUND: usize = 256;
/// Eager recovery after an unknown append: this many tail reads, waiting twice
/// as long before each as before the one ahead of it.
const RECOVERY_ATTEMPTS: u32 = 4;
const RECOVERY_BACKOFF: std::time::Duration = std::time::Duration::from_millis(25);

/// Why the claims family could not take or serve a batch.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimsLogError {
    /// The log or a stored event could not be read or written.
    #[error("the claims family could not reach the log: {0}")]
    Store(String),
    /// Whether the batch committed is not known yet. The next write reads the
    /// tail and settles it.
    #[error("the claims family does not know whether the batch committed")]
    Indeterminate,
    /// Another writer took the position.
    #[error("another writer took the log position")]
    Conflict,
}

fn refused(error: LogError) -> ClaimsLogError {
    match error {
        LogError::Indeterminate => ClaimsLogError::Indeterminate,
        LogError::Conflict => ClaimsLogError::Conflict,
        other => ClaimsLogError::Store(other.to_string()),
    }
}

fn unreadable(error: codec::DecodeError) -> ClaimsLogError {
    ClaimsLogError::Store(error.to_string())
}

struct Held {
    state: State,
    head: Position,
}

/// The claims family, answered from the log.
#[derive(Clone)]
pub struct LoggedClaims {
    writer: Writer,
    log: Arc<dyn LogStore>,
    held: Arc<Mutex<Held>>,
}

impl LoggedClaims {
    /// Replay the log into the held state.
    pub async fn open(log: Arc<dyn LogStore>) -> Result<Self, ClaimsLogError> {
        let this = Self {
            writer: Writer::default(),
            log,
            held: Arc::new(Mutex::new(Held {
                state: State::default(),
                head: Position::default(),
            })),
        };
        {
            let mut held = this.held.lock().await;
            this.catch_up(&mut held).await?;
        }
        Ok(this)
    }

    /// Share the serving process's writer with the other rails, so a write here
    /// queues with theirs and finishes even when its caller stops waiting.
    pub fn with_writer(mut self, writer: Writer) -> Self {
        self.writer = writer;
        self
    }

    /// Apply every committed batch after the held head. A tail whose commit is
    /// unresolved is an error, so a write never prepares on a stale head.
    async fn catch_up(&self, held: &mut Held) -> Result<(), ClaimsLogError> {
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
                    held.state.apply(&event);
                }
                held.head = batch.range().last;
            }
        }
    }

    /// **Append the events as one batch and apply them from the appended value.**
    pub async fn append(
        &self,
        events: Vec<Event<ClaimEvent>>,
        at: Timestamp,
    ) -> Result<(), ClaimsLogError> {
        if events.is_empty() {
            return Ok(());
        }
        let this = self.clone();
        self.writer
            .run(async move { this.commit(events, at).await })
            .await
    }

    async fn commit(
        &self,
        events: Vec<Event<ClaimEvent>>,
        at: Timestamp,
    ) -> Result<(), ClaimsLogError> {
        let mut held = self.held.lock().await;
        self.catch_up(&mut held).await?;
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
            BatchId(format!("claims:{identity}")),
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
                // Applied from the value appended: what the log holds is what
                // the state holds, the stored form of each event included.
                for event in &events {
                    let stored = codec::decode(&codec::encode(event)).map_err(unreadable)?;
                    held.state.apply(&stored);
                }
                held.head = range.last;
                Ok(())
            }
            Append::DefinitelyNotCommitted => Err(ClaimsLogError::Store(
                "the log did not commit the claims batch".to_string(),
            )),
            Append::Indeterminate => {
                self.recover(&mut held).await;
                Err(ClaimsLogError::Indeterminate)
            }
        }
    }

    /// **Fenced tail recovery, straight after an unknown append.** The log answers
    /// a tail read only once the outstanding append has an outcome, so a read that
    /// succeeds settles the held state. When the log cannot say, the next write
    /// retries; the wait is bounded so the writer's queue is not held for good.
    async fn recover(&self, held: &mut Held) {
        for attempt in 0..RECOVERY_ATTEMPTS {
            if self.catch_up(held).await.is_ok() {
                return;
            }
            tokio::time::sleep(RECOVERY_BACKOFF * 2u32.pow(attempt)).await;
        }
    }

    /// The held state as it stands, copied out.
    pub async fn state(&self) -> State {
        self.held.lock().await.state.clone()
    }

    /// The log position the held state has applied through.
    pub async fn head(&self) -> Position {
        self.held.lock().await.head
    }
}
