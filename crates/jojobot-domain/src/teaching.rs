//! Teaching — whether a session has already been told how a domain behaves.
//!
//! **The problem this answers.** A rule an agent needs is sometimes true only
//! at the moment it decides whether to write: "a further claim does not
//! destroy the one already there" has to reach a session before it decides
//! not to write a second, contradicting claim — not after, on a receipt the
//! session that needed it will never call. A footer on every read reaches
//! that session too, forever, on every call it ever makes. Neither is right;
//! what is wanted is once, and early enough.
//!
//! **The shape is a fact, not a rule.** A `domain` is a caller-declared
//! string — `"claims"`, and later whatever else needs this — never a
//! compiled enum: this build has already paid once for a fixed set of kinds
//! that had to become data, and a teaching domain is not going to be the
//! second one.
//!
//! **Keyed on the session's handle, not its card.** A session's [`Sid`] exists
//! from the moment it boots; the store row a session eventually gets
//! ([`crate::session::SessionId`]) exists only once something writes. The
//! sessions this exists for are exactly the ones that only ever read — they
//! search, they see a claim, they decide — so keying on the card would mean
//! the read-only case, which is the whole reason this exists, never
//! qualifies for a row at all.
//!
//! **One row per `(sid, domain)`, and existence is the whole answer.**
//! `first_contact` reports whether this call is the one that created the row.
//! There is no separate "has been taught" read: a caller that needs to know
//! calls the same verb it would call to record it, because those are one
//! question, not two.

use jiff::Timestamp;

use crate::session::Sid;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// A port onto the per-session teaching ledger.
///
/// **Records, never gates.** Nothing here withholds anything or answers
/// `blocked` — a caller decides what to do with a `true`; this only tells the
/// truth about whether it has said so before.
#[async_trait::async_trait]
pub trait Teachings: Send + Sync {
    /// Record that `domain` has reached this session, and say whether this is
    /// the first time.
    ///
    /// `true` means no earlier call recorded this `(sid, domain)` pair, and
    /// this one just did — the caller should teach. `false` means an earlier
    /// call already did, on this session or a predecessor it resumed from
    /// (the same `sid` addresses both), and this one changed nothing.
    ///
    /// `at` is passed in rather than read off a clock, so the store stays
    /// deterministic under test — the same convention [`crate::mailbox`]
    /// uses for `sent_at`.
    async fn first_contact(
        &self,
        sid: &Sid,
        domain: &str,
        at: Timestamp,
    ) -> Result<bool, TeachingError>;
}

/// Why a teaching operation failed. Adapters map their transport errors into
/// this; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum TeachingError {
    /// The store could not be reached or answered with something this port
    /// cannot read.
    #[error("teaching store error: {0}")]
    Store(String),
}
