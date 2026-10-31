//! The durable boundary: complete, immutable batches, never business-policy reads.
//!
//! Event meaning belongs to each domain interpreter. Storage preserves the
//! sealed envelopes byte for byte, including unknown versions and source times.

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

macro_rules! identity {
    ($($name:ident),+ $(,)?) => {$(
        #[derive(Clone, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
        pub struct $name(pub String);
    )+};
}

identity!(
    ObjectId,
    RecordId,
    RunId,
    SessionCardId,
    MessageId,
    BatchId,
    RequestId
);

/// The family chooses P and its encoding before sealing; a backend decodes neither.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Event<P> {
    pub event_kind: String,
    pub meaning_version: u32,
    pub actor: Option<crate::actor::Actor>,
    pub run: Option<RunId>,
    pub session_card: Option<SessionCardId>,
    pub request: Option<RequestId>,
    pub part: Option<String>,
    pub occurred_at: Option<Timestamp>,
    pub written_at: Option<Timestamp>,
    pub imported_source_time: Option<Timestamp>,
    pub payload: P,
}

/// Zero is the empty prefix. An event occupies one position; a cursor is exclusive.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct Position(pub u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Range {
    pub first: Position,
    pub last: Position,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    pub batch_id: BatchId,
    pub expected_previous_position: Position,
    pub first_position: Position,
    pub event_count: u64,
    pub append_time_utc: Timestamp,
    pub representation_version: u32,
    pub checksum: [u8; 32],
}

/// Each member is an already encoded typed Event. No backend re-encodes it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SealedBatch {
    header: Header,
    members: Vec<Vec<u8>>,
}

impl SealedBatch {
    pub fn seal(
        batch_id: BatchId,
        previous: Position,
        at: Timestamp,
        representation_version: u32,
        members: Vec<Vec<u8>>,
    ) -> Result<Self, LogError> {
        let count = u64::try_from(members.len()).map_err(|_| LogError::InvalidBatch)?;
        if count == 0 || previous.0.checked_add(count).is_none() {
            return Err(LogError::InvalidBatch);
        }
        // Length framing distinguishes [a, bc] from [ab, c], and [] from [empty].
        let mut digest = Sha256::new();
        for member in &members {
            digest.update((member.len() as u64).to_be_bytes());
            digest.update(member);
        }
        Ok(Self {
            header: Header {
                batch_id,
                expected_previous_position: previous,
                first_position: Position(previous.0 + 1),
                event_count: count,
                append_time_utc: at,
                representation_version,
                checksum: digest.finalize().into(),
            },
            members,
        })
    }

    pub fn header(&self) -> &Header {
        &self.header
    }

    pub fn members(&self) -> &[Vec<u8>] {
        &self.members
    }

    pub fn range(&self) -> Range {
        Range {
            first: self.header.first_position,
            last: Position(self.header.expected_previous_position.0 + self.header.event_count),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Append {
    Committed(Range),
    DefinitelyNotCommitted,
    Indeterminate,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum LogError {
    #[error("a batch must contain events and fit in the position space")]
    InvalidBatch,
    #[error("the batch identity or reserved range conflicts with committed bytes")]
    Conflict,
    #[error("the preceding append has no terminal fence yet")]
    Indeterminate,
    #[error("the log store failed: {0}")]
    Store(String),
}

/// Reserved storage shape only. The projection layer validates compatibility,
/// epochs, complete-byte checksum and length before supplying this opaque blob.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedSnapshot {
    pub snapshot_format_version: u32,
    pub projection_version: u32,
    pub log_identity: String,
    pub representation_epoch: u64,
    pub erasure_epoch: u64,
    pub covered_position: Position,
    pub batch_checksum: [u8; 32],
    pub created_at: Timestamp,
    pub checksum: [u8; 32],
    pub serialized_length: u64,
    pub blob: Vec<u8>,
}

/// No maintenance, erasure or Memory methods are available through this port.
#[async_trait::async_trait]
pub trait LogStore: Send + Sync {
    async fn append(
        &self,
        expected_head: Position,
        batch: &SealedBatch,
    ) -> Result<Append, LogError>;

    /// At most bound complete batches whose last position is after position.
    /// An interior cursor returns its whole batch. Unknown commit status is an
    /// error, never an empty tail that a caller could mistake for non-commit.
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError>;

    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError>;
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError>;
}
