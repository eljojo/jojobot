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
//!   faithful) and against the real Outline adapter (proving it conforms), so
//!   the two can't drift.

use std::sync::Mutex;

use super::{EntityId, Fact, FactId, Memory, MemoryError, NewFact, normalize_content};

/// An in-memory [`Memory`] adapter for tests. Holds facts in a `Vec` behind a
/// `Mutex`; mints ids `f1`, `f2`, … in capture order. A fresh instance starts
/// empty.
#[derive(Default)]
pub struct InMemoryMemory {
    facts: Mutex<Vec<Fact>>,
}

impl InMemoryMemory {
    /// A new, empty fake.
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    async fn capture(&self, fact: NewFact) -> Result<Fact, MemoryError> {
        // Mirror the real adapter's normalization so the fake can't drift: edge
        // whitespace doesn't survive a table cell, so it isn't significant.
        let content = normalize_content(&fact.content);
        if content.is_empty() {
            return Err(MemoryError::InvalidFact("content is empty".into()));
        }
        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let id = FactId(format!("f{}", facts.len() + 1));
        let stored = Fact {
            id,
            subject: fact.subject,
            content,
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        facts.push(stored.clone());
        Ok(stored)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let facts = self.facts.lock().expect("fake mutex poisoned");
        Ok(facts
            .iter()
            .filter(|f| &f.subject == subject)
            .cloned()
            .collect())
    }
}

/// The behavioural contract every [`Memory`] adapter must satisfy. Each function
/// is a self-contained spec run against a live store. Assertions are
/// **subset-based** — they check that what was captured comes back, never exact
/// totals — so a shared/pre-populated store (real Outline) passes without a
/// reset, and cross-doc local-id reuse never trips them.
pub mod contract {
    use super::*;
    use crate::memory::{FactStatus, Provenance};
    use jiff::civil::date;

    /// Fetch the fact the store returned from `capture`, read back by id.
    async fn read_back<M: Memory>(store: &M, subject: &EntityId, id: &FactId) -> Fact {
        store
            .recall(subject)
            .await
            .expect("recall should succeed")
            .into_iter()
            .find(|f| &f.id == id)
            .unwrap_or_else(|| panic!("recall must return the captured fact (id {id})"))
    }

    /// The core invariant: a captured fact is returned by a later recall.
    pub async fn capture_reads_back<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-readback");
        let captured = store
            .capture(NewFact::about(subject.clone(), "drinks oat milk", date(2026, 7, 24)))
            .await
            .expect("capture should succeed");
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured, "recalled fact must be byte-identical");
    }

    /// Every field survives capture→recall unchanged and byte-identical.
    pub async fn preserves_all_fields<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-fields");
        let new = NewFact {
            subject: subject.clone(),
            content: "lived in Montréal".into(),
            provenance: Provenance::Testimony,
            status: FactStatus::Active,
            date: date(2026, 3, 9),
        };
        let captured = store.capture(new).await.expect("capture should succeed");
        assert_eq!(captured.subject, subject);
        assert_eq!(captured.content, "lived in Montréal");
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.date, date(2026, 3, 9));

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured);
    }

    /// A raw pipe in content survives the round-trip (it must be escaped in the
    /// table, not split into extra cells) — byte-identical.
    pub async fn pipe_in_content_round_trips<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-pipe");
        let captured = store
            .capture(NewFact::about(
                subject.clone(),
                "reads a|b|c pipe notation",
                date(2026, 7, 24),
            ))
            .await
            .expect("capture should succeed");
        assert_eq!(captured.content, "reads a|b|c pipe notation");
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured);
    }

    /// Both provenance values survive independently — the regression guard for
    /// the collision that dropped/corrupted facts when provenance was folded
    /// into content. Testimony must come back testimony, inference inference.
    pub async fn both_provenances_survive<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-provenance");
        let testi = store
            .capture(NewFact {
                subject: subject.clone(),
                content: "born in Chile".into(),
                provenance: Provenance::Testimony,
                status: FactStatus::Active,
                date: date(2026, 1, 1),
            })
            .await
            .expect("capture testimony");
        let infer = store
            .capture(NewFact {
                subject: subject.clone(),
                // Content that ends in the human ❓ glyph must NOT be read as
                // inference-by-marker — provenance is its own column now.
                content: "might prefer mornings ❓".into(),
                provenance: Provenance::Inference,
                status: FactStatus::Active,
                date: date(2026, 1, 2),
            })
            .await
            .expect("capture inference");

        let seen_testi = read_back(store, &subject, &testi.id).await;
        let seen_infer = read_back(store, &subject, &infer.id).await;
        assert_eq!(seen_testi.provenance, Provenance::Testimony);
        assert_eq!(seen_testi.content, "born in Chile");
        assert_eq!(seen_infer.provenance, Provenance::Inference);
        assert_eq!(seen_infer.content, "might prefer mornings ❓");
    }

    /// Edge whitespace is not significant: capture normalizes it, and the
    /// returned fact is byte-identical to what recall reads back.
    pub async fn edge_whitespace_is_normalized<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-whitespace");
        let captured = store
            .capture(NewFact::about(
                subject.clone(),
                "   likes espresso   ",
                date(2026, 7, 24),
            ))
            .await
            .expect("capture should succeed");
        assert_eq!(
            captured.content, "likes espresso",
            "capture must normalize edge whitespace"
        );
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured, "recalled fact must be byte-identical");
    }

    /// Two distinct captures are both recallable, each under its own id.
    pub async fn multiple_facts_all_recallable<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-multi");
        let a = store
            .capture(NewFact::about(subject.clone(), "plays go", date(2026, 7, 1)))
            .await
            .expect("capture a");
        let b = store
            .capture(NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)))
            .await
            .expect("capture b");
        assert_ne!(a.id, b.id, "each fact must get its own id");
        assert_eq!(read_back(store, &subject, &a.id).await.content, "plays go");
        assert_eq!(read_back(store, &subject, &b.id).await.content, "learning Rust");
    }

    /// Facts about one entity never leak into another's recall — each subject's
    /// facts are isolated (a per-person doc, in the real adapter).
    pub async fn subjects_are_isolated<M: Memory>(store: &M) {
        let alice = EntityId::person("contract-alice");
        let bob = EntityId::person("contract-bob");
        store
            .capture(NewFact::about(alice.clone(), "alice fact", date(2026, 7, 1)))
            .await
            .expect("capture alice");
        store
            .capture(NewFact::about(bob.clone(), "bob fact", date(2026, 7, 1)))
            .await
            .expect("capture bob");

        let alice_facts = store.recall(&alice).await.expect("recall alice");
        assert!(
            alice_facts.iter().all(|f| f.subject == alice),
            "recall(alice) must only return alice's facts"
        );
        assert!(alice_facts.iter().any(|f| f.content == "alice fact"));
        assert!(
            !alice_facts.iter().any(|f| f.content == "bob fact"),
            "bob's fact must not appear under alice"
        );
    }

    /// Run the whole contract against one store.
    pub async fn run_all<M: Memory>(store: &M) {
        capture_reads_back(store).await;
        preserves_all_fields(store).await;
        pipe_in_content_round_trips(store).await;
        both_provenances_survive(store).await;
        edge_whitespace_is_normalized(store).await;
        multiple_facts_all_recallable(store).await;
        subjects_are_isolated(store).await;
    }
}
