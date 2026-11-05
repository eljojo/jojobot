//! **The one-time import of today's claims, fields and identities into the event
//! log.** It reads every row the claims import names ([`DoltMemory::import_rows`]),
//! maps them to events ([`import`]) and appends them as ONE batch, in one
//! transaction, so a log that holds a claims event holds the whole import and a
//! start cut short leaves nothing. A log that already holds a claims event is
//! never imported into again.
//!
//! **A boot step, run after every pass that rewrites this data.** The row repairs
//! that stay until 9.3.10 (rule 706) run first; this reads what they left. It
//! runs as a job of the writer so the log's head is the one every family shares,
//! and it refuses rather than guess: a row the reader could not place, or a set of
//! rows the mapping will not accept, fails the call with nothing appended (a crash
//! loop is the cost of silent damage in the log, and the import is run again at
//! the next start).

use jiff::Timestamp;
use jojobot_domain::log::claims::import::import;
use jojobot_domain::log::{BatchId, Position, SealedBatch};
use sqlx::MySqlPool;

use crate::claims_log::codec;
use crate::dolt::memory::DoltMemory;
use crate::ledger_log::READ_BOUND;
use crate::writer::{Writer, sql};

/// The representation version of a claims batch, the one the claims family's own
/// appends use.
const REPRESENTATION_VERSION: u32 = 1;

/// What one call did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Imported {
    /// The log held no claims event, and now holds this many.
    Done { events: usize },
    /// The log already held a claims event. Nothing was read or written.
    AlreadyDone,
}

/// Why the import appended nothing.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ClaimsImportError {
    /// The store or the log could not be read or written.
    #[error("the claims import could not reach the store: {0}")]
    Store(String),
    /// The log holds a claims event this build cannot read.
    #[error("the log holds a claims event the import cannot read: {0}")]
    Log(String),
    /// The reader found rows it would not place. Each is `table row: what`.
    #[error("the claims import found {count} stored rows it will not guess at: {rows}")]
    Findings { count: usize, rows: String },
    /// The rows do not map to events.
    #[error("the stored rows do not map to claims events: {0}")]
    Unmappable(String),
    /// Whether the batch committed is not known. The next start reads the log.
    #[error("the claims import does not know whether its batch committed")]
    Indeterminate,
}

impl crate::writer::recovery::StoreError for ClaimsImportError {
    fn store(why: String) -> Self {
        Self::Store(why)
    }
    fn indeterminate() -> Self {
        Self::Indeterminate
    }
    fn unknown(&self) -> bool {
        matches!(self, Self::Store(_) | Self::Indeterminate)
    }
}

fn log_failed(error: jojobot_domain::log::LogError) -> ClaimsImportError {
    ClaimsImportError::Store(error.to_string())
}

/// **Import today's claims once.** Appends nothing when the log already holds a
/// claims event.
pub async fn import_claims_once(
    writer: &Writer,
    pool: &MySqlPool,
    at: Timestamp,
) -> Result<Imported, ClaimsImportError> {
    let pool = sql::Pool::from(pool.clone());
    let owner = writer.clone();
    writer
        .run(async move { import_in_job(&owner, &pool, at).await })
        .await
}

async fn import_in_job(
    writer: &Writer,
    pool: &sql::Pool,
    at: Timestamp,
) -> Result<Imported, ClaimsImportError> {
    let log = writer.shared_log(pool);
    // The head is read on the writer's connection, which takes the one-writer
    // lock first, as the ledger seam's open does.
    let mut tx = pool
        .writer()
        .begin()
        .await
        .map_err(|e| ClaimsImportError::Store(e.to_string()))?;
    let settled = log.settle_on(&mut tx).await;
    let _ = tx.rollback().await;
    settled.map_err(log_failed)?;

    // **Does the log hold a claims event?** A member of another family is skipped;
    // one of this family that cannot be read fails the call, because importing
    // over a log this build cannot read would write the rows twice.
    let mut from = Position::default();
    loop {
        let batches = log.read_from(from, READ_BOUND).await.map_err(log_failed)?;
        let Some(last) = batches.last().map(|batch| batch.range().last) else {
            break;
        };
        for batch in &batches {
            for member in batch.members() {
                if codec::decode_own(member)
                    .map_err(|e| ClaimsImportError::Log(e.to_string()))?
                    .is_some()
                {
                    return Ok(Imported::AlreadyDone);
                }
            }
        }
        from = last;
    }

    let read = DoltMemory::open((**pool).clone())
        .import_rows()
        .await
        .map_err(|e| ClaimsImportError::Store(e.to_string()))?;
    if !read.findings.is_empty() {
        return Err(ClaimsImportError::Findings {
            count: read.findings.len(),
            rows: read
                .findings
                .iter()
                .map(|finding| format!("{} {}: {}", finding.table, finding.row, finding.what))
                .collect::<Vec<_>>()
                .join("; "),
        });
    }
    let events = import(&read.rows).map_err(|e| ClaimsImportError::Unmappable(e.to_string()))?;
    if events.is_empty() {
        return Ok(Imported::Done { events: 0 });
    }
    let members: Vec<Vec<u8>> = events.iter().map(codec::encode).collect();

    let mut tx = pool
        .writer()
        .begin()
        .await
        .map_err(|e| ClaimsImportError::Store(e.to_string()))?;
    let head = log.head_in(&mut tx).await.map_err(log_failed)?;
    let batch = SealedBatch::seal(
        BatchId(format!("claims-import:{head}")),
        Position(head),
        at,
        REPRESENTATION_VERSION,
        members,
    )
    .map_err(log_failed)?;
    let staged = log
        .stage(&mut tx, Position(head), &batch)
        .await
        .map_err(log_failed)?;
    match sql::commit(tx).await {
        Ok(()) => {
            staged.committed();
            Ok(Imported::Done {
                events: events.len(),
            })
        }
        Err(error) => {
            staged.unknown();
            // The error goes through the writer's own classification, which marks
            // the native connection uncertain so the job fences it before the next
            // read of the tail.
            Err(sql::store_error::<ClaimsImportError>(error))
        }
    }
}
