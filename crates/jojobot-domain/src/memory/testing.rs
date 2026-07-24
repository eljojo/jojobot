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

use super::{EntityId, Fact, FactId, Memory, MemoryError, NewFact};

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
        if fact.content.trim().is_empty() {
            return Err(MemoryError::InvalidFact("content is empty".into()));
        }
        let mut facts = self.facts.lock().unwrap();
        let id = FactId(format!("f{}", facts.len() + 1));
        let stored = Fact {
            id,
            subject: fact.subject,
            content: fact.content,
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        facts.push(stored.clone());
        Ok(stored)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let facts = self.facts.lock().unwrap();
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
/// reset, and self-cleanup is the integration test's job, not the spec's.
pub mod contract {
    use super::*;
    use crate::memory::{FactStatus, Provenance};
    use jiff::civil::date;

    /// The core invariant: a captured fact is returned by a later recall.
    pub async fn capture_reads_back<M: Memory>(store: &M) {
        let subject = EntityId::self_();
        let captured = store
            .capture(NewFact::about_self("drinks oat milk", date(2026, 7, 24)))
            .await
            .expect("capture should succeed");

        let recalled = store.recall(&subject).await.expect("recall should succeed");
        assert!(
            recalled.iter().any(|f| f.id == captured.id),
            "recall must return the captured fact (id {}); got ids {:?}",
            captured.id,
            recalled.iter().map(|f| f.id.as_str()).collect::<Vec<_>>()
        );
    }

    /// Capture round-trips every field unchanged, and the recalled fact equals
    /// the one capture returned.
    pub async fn preserves_all_fields<M: Memory>(store: &M) {
        let subject = EntityId::self_();
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
        assert_eq!(captured.status, FactStatus::Active);
        assert_eq!(captured.date, date(2026, 3, 9));

        let recalled = store.recall(&subject).await.expect("recall should succeed");
        let found = recalled
            .iter()
            .find(|f| f.id == captured.id)
            .expect("the captured fact must be recallable by id");
        assert_eq!(*found, captured, "recalled fact must equal the captured one");
    }

    /// Two distinct captures are both recallable, each under its own id — no
    /// overwrite, no id collision.
    pub async fn multiple_facts_all_recallable<M: Memory>(store: &M) {
        let subject = EntityId::self_();
        let a = store
            .capture(NewFact::about_self("plays go", date(2026, 7, 1)))
            .await
            .expect("capture a");
        let b = store
            .capture(NewFact::about_self("learning Rust", date(2026, 7, 2)))
            .await
            .expect("capture b");
        assert_ne!(a.id, b.id, "each fact must get its own id");

        let recalled = store.recall(&subject).await.expect("recall should succeed");
        let a_seen = recalled.iter().find(|f| f.id == a.id);
        let b_seen = recalled.iter().find(|f| f.id == b.id);
        assert_eq!(a_seen.map(|f| f.content.as_str()), Some("plays go"));
        assert_eq!(b_seen.map(|f| f.content.as_str()), Some("learning Rust"));
    }

    /// Run the whole contract against one store.
    pub async fn run_all<M: Memory>(store: &M) {
        capture_reads_back(store).await;
        preserves_all_fields(store).await;
        multiple_facts_all_recallable(store).await;
    }
}
