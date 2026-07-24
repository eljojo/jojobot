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
//! subsequent recall returns the fact. Facts are about **entities** (people);
//! there is no privileged owner. The port is pure (no rmcp, no reqwest);
//! adapters behind it (the in-memory fake, the real Outline store) live outside
//! this crate.

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

#[cfg(any(test, feature = "testing"))]
pub mod testing;

/// A noun jojobot knows about — a person, project, or place. Stable; never
/// true/false. Everything points at an entity by its id, so the id is a stable
/// typed string (`person:jose`). There is no privileged `self`/owner entity:
/// the user is a person like any other.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EntityId(pub String);

impl EntityId {
    /// A person entity id from a bare handle: `person("jose")` → `person:jose`.
    /// If the handle already carries a `kind:` prefix it is used verbatim.
    pub fn person(handle: impl AsRef<str>) -> Self {
        let h = handle.as_ref().trim();
        if h.contains(':') {
            EntityId(h.to_string())
        } else {
            EntityId(format!("person:{h}"))
        }
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
/// not tied to the user's own words is a hypothesis until confirmed. Stored in
/// its **own** table column — never folded into the content — so a claim that
/// happens to end in a marker glyph can't be misread as inference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    /// The user said or confirmed it.
    Testimony,
    /// jojobot (or Claude) derived it. Carries no more authority than a guess.
    #[default]
    Inference,
}

impl Provenance {
    /// The wire token written to the table's `provenance` column.
    pub fn as_token(self) -> &'static str {
        match self {
            Provenance::Testimony => "testimony",
            Provenance::Inference => "inference",
        }
    }

    /// Parse a `provenance` cell. Only the exact `testimony` token yields
    /// testimony; everything else — the default, a blank cell, an unknown
    /// value — is inference. This is deliberately lenient and one-directional:
    /// a garbled cell degrades to the *less*-trusted class, never up to
    /// testimony, and no fact is ever dropped.
    pub fn from_token(cell: &str) -> Self {
        match cell.trim() {
            "testimony" => Provenance::Testimony,
            _ => Provenance::Inference,
        }
    }
}

/// A fact's lifecycle state. This slice implements **[`FactStatus::Active`]
/// only** — no supersede/negate machinery and no status *filter* on recall. The
/// column exists in the table (and the type is an enum) so the schema is stable
/// and the lifecycle states can land later without a migration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FactStatus {
    /// The current truth — the only state this slice writes or reads.
    #[default]
    Active,
}

impl FactStatus {
    /// The wire token written to the table's `status` column.
    pub fn as_token(self) -> &'static str {
        match self {
            FactStatus::Active => "active",
        }
    }
}

/// Validate a subject id before it is written anywhere. Entity ids are
/// **structured** (`kind:slug`), never free text, so a safe charset is enough to
/// keep an adversarial subject out of the markdown: no newline (forge a row or a
/// `###` header), no `|` (forge a cell), no backtick (forge a fence), no space.
/// Anything outside `[a-z0-9:_-]` — or empty, or absurdly long — is rejected.
/// This is the primary defence; escaping-on-write is the belt-and-suspenders.
pub fn validate_subject(subject: &EntityId) -> Result<(), MemoryError> {
    let s = subject.as_str();
    let ok = !s.is_empty()
        && s.len() <= 128
        && s.bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b':' | b'_' | b'-'));
    if ok {
        Ok(())
    } else {
        Err(MemoryError::InvalidSubject(s.to_string()))
    }
}

/// Normalize a fact's content to the form that survives a table round-trip.
///
/// A markdown table cell cannot preserve leading/trailing whitespace, so edge
/// whitespace is not significant and is trimmed here. Both adapters call this on
/// capture, which is what makes the returned fact **byte-identical** to what a
/// later recall reads back — the fake can't preserve whitespace the real store
/// would drop.
pub fn normalize_content(content: &str) -> String {
    content.trim().to_string()
}

/// A fact about to be captured — everything but the id, which the store mints.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewFact {
    /// The entity this fact is about.
    pub subject: EntityId,
    /// The crisp claim — what surfaces, like a card title.
    pub content: String,
    /// Testimony vs inference; defaults to inference.
    pub provenance: Provenance,
    /// Lifecycle state; a fresh capture is [`FactStatus::Active`].
    pub status: FactStatus,
    /// The fact's own freshness stamp, authoritative in the source.
    pub date: Date,
}

impl NewFact {
    /// A fact about `subject` with default provenance (inference) and active
    /// status — the common shape this slice captures.
    pub fn about(subject: EntityId, content: impl Into<String>, date: Date) -> Self {
        NewFact {
            subject,
            content: content.into(),
            provenance: Provenance::default(),
            status: FactStatus::default(),
            date,
        }
    }
}

/// A captured fact — a [`NewFact`] with the id its home assigned and its content
/// normalized to storage form.
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
    /// The claim is malformed for storage (empty, or spans multiple lines).
    #[error("invalid fact: {0}")]
    InvalidFact(String),
    /// The subject id is not a well-formed entity id (see [`validate_subject`]).
    /// Treated as adversarial: it never reaches the store.
    #[error("invalid subject '{0}': entity ids must match [a-z0-9:_-]")]
    InvalidSubject(String),
    /// The underlying store (Outline, or its network/parse layer) failed.
    #[error("store error: {0}")]
    Store(String),
    /// The store isn't configured (no credentials). Production fronts real
    /// Outline; until it's wired, the memory verbs refuse rather than lie.
    #[error("memory store not configured: {0}")]
    NotConfigured(String),
}

/// The Memory port: capture a fact about an entity, recall an entity's facts.
/// One real adapter stands behind it in production (Outline); a fake stands
/// behind it in tests. The invariant that binds every adapter: **a `capture`
/// succeeds only if a subsequent `recall` of the same subject returns the
/// fact**, byte-identical.
#[async_trait::async_trait]
pub trait Memory: Send + Sync {
    /// Write a fact and return it with the id its home assigned, its content
    /// normalized. The returned fact must be visible — byte-identical — to a
    /// subsequent [`recall`](Memory::recall) of its subject.
    async fn capture(&self, fact: NewFact) -> Result<Fact, MemoryError>;

    /// Read back every fact whose subject is `subject`, in an unspecified order.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError>;
}

#[cfg(test)]
mod tests {
    use super::testing::{InMemoryMemory, contract};
    use super::*;

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

    #[test]
    fn person_id_prefixes_a_bare_handle_but_respects_a_typed_one() {
        assert_eq!(EntityId::person("jose").as_str(), "person:jose");
        assert_eq!(EntityId::person("person:jose").as_str(), "person:jose");
    }

    #[test]
    fn validate_subject_accepts_ids_and_rejects_adversarial_ones() {
        assert!(validate_subject(&EntityId::person("jose")).is_ok());
        assert!(validate_subject(&EntityId("project:jojobot-server".into())).is_ok());
        // Injection vectors: newline, pipe, header, fence, space, uppercase, empty.
        for bad in ["person:a|b", "a\nb", "### forged", "a`b", "a b", "Person:Jose", ""] {
            assert!(
                validate_subject(&EntityId(bad.into())).is_err(),
                "must reject {bad:?}"
            );
        }
    }

    #[test]
    fn provenance_tokens_round_trip_and_degrade_to_inference() {
        assert_eq!(Provenance::from_token("testimony"), Provenance::Testimony);
        assert_eq!(Provenance::from_token("inference"), Provenance::Inference);
        assert_eq!(Provenance::from_token(""), Provenance::Inference);
        assert_eq!(Provenance::from_token("garbled"), Provenance::Inference);
    }
}
