//! Test support for the [`Memory`](super::Memory) port — **never shipped**.
//!
//! Gated behind `feature = "testing"` (and `cfg(test)` in this crate), so it is
//! present for tests here and in downstream crates but absent from every
//! production binary. It holds two things:
//!
//! * [`InMemoryMemory`] — the fake adapter. The fast TDD loop runs against it:
//!   no network, milliseconds.
//! * the **contract** — one behavioural spec (`contract::*`) that every adapter
//!   of the port must satisfy. It runs against the fake (proving the fake
//!   faithful) and against the real Dolt adapter (proving it conforms), so
//!   the two can't drift.

use super::{
    ClaimWrite, Edge, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FactStatus, FieldWrite, FormerHandle, Guarded, MAX_KEY_CHARS, MERGED_FROM, Memory,
    MemoryError, Merge, NewEntity, NewFact, Retraction, Standing, apply_entity_patch,
    apply_fact_patch,
    guard::{self, Decision},
    merge_account, normalize_content, normalize_details, normalize_prose, resolve_handle,
    retraction_of, screen_entity_patch, search, standing_of, validate_content, validate_details,
    validate_edge, validate_entity, validate_fields, validate_prose, validate_provenance_source,
    validate_write_subject,
};

mod fake;
pub use fake::InMemoryMemory;

/// The behavioural contract every [`Memory`] adapter must satisfy. Each function
/// is a self-contained spec run against a live store. Assertions are
/// **subset-based** — they check that what was captured comes back, never exact
/// totals — so a shared/pre-populated store (real Dolt) passes without a
/// reset, and cross-doc local-id reuse never trips them.
///
/// Handles here are deliberately far apart (≥3 edits): the write guard is on the
/// write path now, so contract entities that looked alike would flag each other.
/// Every fixture is a **synthetic placeholder** — this is user-agnostic software
/// and carries no user PII, not even in test data.
pub mod contract;
