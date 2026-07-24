//! Memory — facts, portraits, rules-and-receipts.
//!
//! Provenance is a *type*, not a convention: a fact the user stated is
//! testimony; anything derived is inference, and inference is the default.
//! Making this an enum means every place that consumes a fact must decide how
//! it treats the two — the compiler lists the sites.
//!
//! This module carries the **first Memory slice**: the [`Memory`] port and the
//! [`Fact`] it moves. The port has exactly two verbs — [`Memory::capture`] and
//! [`Memory::recall`] — bound by one invariant: a capture succeeds only if a
//! subsequent recall returns the fact. The port is pure (no rmcp, no reqwest);
//! adapters behind it (the in-memory fake, the real Outline store) live outside
//! this crate.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// A noun jojobot knows about — a person, project, place, or the user. Stable;
/// never true/false. Everything points at an entity by its id, so the id is a
/// stable typed string (`self`, later `person:ted`).
///
/// The first slice knows exactly one entity: [`EntityId::SELF`], the user. It is
/// a role, not a name — zero PII — so the engine and its tests reference it
/// generically.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    /// The always-present entity: the user. Seeded at instance config, never
    /// sourced or graduated like a CRM person. The default — and, this slice,
    /// the only — subject of a fact.
    pub const SELF: &'static str = "self";

    /// The `self` entity.
    pub fn self_() -> Self {
        EntityId(Self::SELF.to_string())
    }

    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A fact's light, local id — unique within its home doc. Facts stay
/// light/local until something must point at one directly (supersede/merge);
/// only then do they earn a global typed id. The store mints it on capture.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FactId(pub String);

impl FactId {
    /// Borrow the underlying id string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for FactId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a claim came from. The default is [`Provenance::Inference`]: anything
/// not tied to the user's own words is a hypothesis until confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The user said or confirmed it.
    Testimony,
    /// jojobot (or Claude) derived it. Carries no more authority than a guess.
    #[default]
    Inference,
}

/// A fact's lifecycle state. Blank in the table means [`FactStatus::Active`];
/// superseded/negated facts are kept (their ids may be referenced) and filtered
/// out of a normal read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactStatus {
    /// The current truth.
    #[default]
    Active,
    /// Absorbed by a newer fact; kept so links survive.
    Superseded,
    /// Disproven but retained: its content is rephrased as the thing NOT to
    /// infer. The "anti-fact list" is just a `status = negated` filter.
    Negated,
}

/// A fact about to be captured — everything but the id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    /// The entity this fact is about. This slice: always [`EntityId::self_`].
    pub subject: EntityId,
    /// The crisp claim — what surfaces, like a card title.
    pub content: String,
    /// Testimony vs inference; defaults to inference.
    pub provenance: Provenance,
    /// Lifecycle state; a fresh capture is normally [`FactStatus::Active`].
    pub status: FactStatus,
    /// The fact's own freshness stamp, authoritative in the source.
    pub date: Date,
}

impl NewFact {
    /// A fact about the `self` entity with default provenance (inference) and
    /// active status — the common shape this slice captures.
    pub fn about_self(content: impl Into<String>, date: Date) -> Self {
        NewFact {
            subject: EntityId::self_(),
            content: content.into(),
            provenance: Provenance::default(),
            status: FactStatus::default(),
            date,
        }
    }
}

/// A captured fact — a [`NewFact`] with the id its home assigned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Fact {
    /// Light/local id, unique in its home.
    pub id: FactId,
    /// The entity this fact is about.
    pub subject: EntityId,
    /// The crisp claim.
    pub content: String,
    /// Testimony vs inference.
    pub provenance: Provenance,
    /// Lifecycle state.
    pub status: FactStatus,
    /// The fact's own freshness stamp.
    pub date: Date,
}

/// Why a memory operation failed. Adapters map their transport/parse errors into
/// these; the domain and the MCP layer speak only this vocabulary.
#[derive(Debug, thiserror::Error)]
pub enum MemoryError {
    /// A table cell can't carry a raw newline, and content can't be empty. The
    /// claim is malformed for storage.
    #[error("invalid fact: {0}")]
    InvalidFact(String),
    /// The underlying store (Outline, or its network/parse layer) failed.
    #[error("store error: {0}")]
    Store(String),
    /// The store isn't configured (no token / no target doc). Production fronts
    /// real Outline; until it's wired, the memory verbs refuse rather than lie.
    #[error("memory store not configured: {0}")]
    NotConfigured(String),
}

/// The Memory port: capture a fact, recall an entity's facts. One real adapter
/// stands behind it in production (Outline); a fake stands behind it in tests.
/// The invariant that binds every adapter: **a `capture` succeeds only if a
/// subsequent `recall` of the same subject returns the fact.**
#[async_trait::async_trait]
pub trait Memory: Send + Sync {
    /// Write a fact and return it with the id its home assigned. The returned
    /// fact must be visible to a subsequent [`recall`](Memory::recall) of its
    /// subject.
    async fn capture(&self, fact: NewFact) -> Result<Fact, MemoryError>;

    /// Read back every fact whose subject is `subject`, in an unspecified order.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError>;
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemoryMemory, contract};

    /// The invariant, red→green, in milliseconds against the fake: a capture
    /// succeeds only if a subsequent recall returns the fact.
    #[tokio::test]
    async fn capture_reads_back_against_the_fake() {
        contract::capture_reads_back(&InMemoryMemory::new()).await;
    }

    /// The full behavioural contract holds for the fake — the same suite the
    /// gated integration test runs against real Outline.
    #[tokio::test]
    async fn fake_satisfies_the_contract() {
        contract::run_all(&InMemoryMemory::new()).await;
    }
}
