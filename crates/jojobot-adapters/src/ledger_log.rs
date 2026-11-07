//! **What the request and app ledgers share with the event log.** The ledgers
//! live in a state held in memory by [`crate::ledger_seam::Seam`], which replays
//! this family's events from the log and stages new ones. This module holds the
//! family's error type, its representation version, the bound on a read, and the
//! codec that spells its events in the log.

use jojobot_domain::log::LogError;

pub mod codec;

#[cfg(test)]
mod tests;

pub(crate) const REPRESENTATION_VERSION: u32 = 1;
pub(crate) const READ_BOUND: usize = 256;

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

impl crate::writer::recovery::StoreError for LedgerError {
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

pub(crate) fn refused(error: LogError) -> LedgerError {
    match error {
        LogError::Indeterminate => LedgerError::Indeterminate,
        LogError::Conflict => LedgerError::Conflict,
        other => LedgerError::Store(other.to_string()),
    }
}

pub(crate) fn unreadable(error: codec::DecodeError) -> LedgerError {
    LedgerError::Store(error.to_string())
}
