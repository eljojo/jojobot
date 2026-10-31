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
use tokio::sync::Mutex;

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
            })),
        };
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
                this.commit(&mut held, events, at).await?;
            }
        }
        Ok(this)
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
        match self.log.append(held.head, &batch).await.map_err(refused)? {
            Append::Committed(range) => {
                for event in &events {
                    held.state.apply(event);
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
        if !prepared.events.is_empty() {
            self.commit(&mut held, prepared.events, at).await?;
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
            self.commit(&mut held, prepared.events, at).await?;
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
