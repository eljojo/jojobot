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

use super::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId, FactPatch, Guarded,
    Memory, MemoryError, NewEntity, NewFact, apply_entity_patch, apply_fact_patch,
    normalize_content, normalize_details, validate_content, validate_details, validate_entity,
    validate_subject,
    guard::{self, Decision},
};

/// An in-memory [`Memory`] adapter for tests. Holds entities and facts in `Vec`s
/// behind `Mutex`es; mints fact ids `f1`, `f2`, … per home doc, mirroring the
/// real store's per-doc numbering. A fresh instance starts empty.
#[derive(Default)]
pub struct InMemoryMemory {
    entities: Mutex<Vec<Entity>>,
    facts: Mutex<Vec<Fact>>,
}

impl InMemoryMemory {
    /// A new, empty fake.
    pub fn new() -> Self {
        Self::default()
    }

    /// The entity index the write guard screens against.
    fn index(&self) -> Vec<Entity> {
        self.entities.lock().expect("fake mutex poisoned").clone()
    }

    /// Provision an entity the way a `capture` on an unknown subject does:
    /// existence sourced as `capture`, no name until someone names it.
    fn provision(&self, id: &EntityId) {
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        if entities.iter().any(|e| &e.id == id) {
            return;
        }
        entities.push(Entity {
            kind: id.kind().expect("a validated id has a kind"),
            id: id.clone(),
            name: String::new(),
            source: "capture".into(),
            crm: None,
            boot: Default::default(),
        });
    }
}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        validate_entity(&new.id, &new.name, &new.source, new.crm.as_deref())?;
        if let Decision::Block(candidates) =
            guard::decide(&new.id, Some(&new.name), &self.index(), new.create_new)
        {
            return Ok(Guarded::Blocked {
                attempted: new.id,
                candidates,
            });
        }
        let entity = Entity {
            kind: new.id.kind().expect("a validated id has a kind"),
            id: new.id,
            name: new.name.trim().to_string(),
            source: new.source.trim().to_string(),
            crm: new.crm.map(|c| c.trim().to_string()),
            boot: new.boot,
        };
        self.entities
            .lock()
            .expect("fake mutex poisoned")
            .push(entity.clone());
        Ok(Guarded::Written(entity))
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        Ok(self
            .index()
            .into_iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .collect())
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Entity, MemoryError> {
        validate_subject(handle)?;
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == handle) else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, None, &entities),
            });
        };
        apply_entity_patch(entity, &patch)?;
        Ok(entity.clone())
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        // Same guards the real adapter applies, so the fake can't drift.
        validate_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;

        let known = self.index().iter().any(|e| e.id == fact.subject);
        if !known
            && let Decision::Block(candidates) =
                guard::decide(&fact.subject, None, &self.index(), fact.create_new)
        {
            return Ok(Guarded::Blocked {
                attempted: fact.subject,
                candidates,
            });
        }
        self.provision(&fact.subject);

        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let home = fact.subject.clone();
        let existing: Vec<&Fact> = facts.iter().filter(|f| f.home == home).collect();
        let id = FactId(format!("f{}", existing.len() + 1));
        let stored = Fact {
            id,
            home,
            subject: fact.subject,
            // Edge whitespace doesn't survive a table cell, so it isn't significant.
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        facts.push(stored.clone());
        Ok(Guarded::Written(stored))
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let facts = self.facts.lock().expect("fake mutex poisoned");
        Ok(facts
            .iter()
            .filter(|f| &f.subject == subject)
            .cloned()
            .collect())
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Fact, MemoryError> {
        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let nearest: Vec<String> = facts
            .iter()
            .filter(|f| f.home == address.home)
            .map(|f| f.address().to_string())
            .collect();
        let Some(fact) = facts
            .iter_mut()
            .find(|f| f.home == address.home && f.id == address.local)
        else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        };
        apply_fact_patch(fact, &patch)?;
        Ok(fact.clone())
    }
}

/// The behavioural contract every [`Memory`] adapter must satisfy. Each function
/// is a self-contained spec run against a live store. Assertions are
/// **subset-based** — they check that what was captured comes back, never exact
/// totals — so a shared/pre-populated store (real Outline) passes without a
/// reset, and cross-doc local-id reuse never trips them.
///
/// Handles here are deliberately far apart (≥3 edits): the write guard is on the
/// write path now, so contract entities that looked alike would flag each other.
/// Every fixture is a **synthetic placeholder** — this is user-agnostic software
/// and carries no user PII, not even in test data.
pub mod contract {
    use super::*;
    use crate::memory::{Boot, FactStatus, Provenance};
    use jiff::civil::date;

    /// Capture a fact the guard is expected to wave through.
    async fn capture<M: Memory>(store: &M, fact: NewFact) -> Fact {
        let subject = fact.subject.clone();
        store
            .capture(fact)
            .await
            .expect("capture should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block {subject}"))
    }

    /// Add an entity the guard is expected to wave through.
    async fn add<M: Memory>(store: &M, new: NewEntity) -> Entity {
        let id = new.id.clone();
        store
            .add_entity(new)
            .await
            .expect("add_entity should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block {id}"))
    }

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

    /// One entity out of the store's own listing — the read path for entities.
    async fn read_entity<M: Memory>(store: &M, id: &EntityId) -> Entity {
        store
            .list_entities(None)
            .await
            .expect("list_entities should succeed")
            .into_iter()
            .find(|e| &e.id == id)
            .unwrap_or_else(|| panic!("list_entities must return {id}"))
    }

    // --- facts (slice 1, still binding) --------------------------------------

    /// The core invariant: a captured fact is returned by a later recall.
    pub async fn capture_reads_back<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-readback");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "drinks oat milk", date(2026, 7, 24)),
        )
        .await;
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured, "recalled fact must be byte-identical");
    }

    /// Every field survives capture→recall unchanged and byte-identical.
    pub async fn preserves_all_fields<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-fields");
        let new = NewFact {
            subject: subject.clone(),
            content: "prefers a café table".into(),
            details: Some("mentioned it twice".into()),
            provenance: Provenance::Testimony,
            status: FactStatus::Active,
            date: date(2026, 3, 9),
            create_new: false,
        };
        let captured = capture(store, new).await;
        assert_eq!(captured.subject, subject);
        assert_eq!(captured.content, "prefers a café table");
        assert_eq!(captured.details.as_deref(), Some("mentioned it twice"));
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.date, date(2026, 3, 9));

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured);
    }

    /// A raw pipe in content survives the round-trip (it must be escaped in the
    /// table, not split into extra cells) — byte-identical.
    pub async fn pipe_in_content_round_trips<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-pipe");
        let captured = capture(
            store,
            NewFact {
                details: Some("noted a|b in the margin".into()),
                ..NewFact::about(subject.clone(), "reads a|b|c pipe notation", date(2026, 7, 24))
            },
        )
        .await;
        assert_eq!(captured.content, "reads a|b|c pipe notation");
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured);
    }

    /// Both provenance values survive independently — the regression guard for
    /// the collision that dropped/corrupted facts when provenance was folded
    /// into content. Testimony must come back testimony, inference inference.
    pub async fn both_provenances_survive<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-provenance");
        let testi = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                ..NewFact::about(subject.clone(), "speaks two languages", date(2026, 1, 1))
            },
        )
        .await;
        let infer = capture(
            store,
            NewFact {
                // Content that ends in the human ❓ glyph must NOT be read as
                // inference-by-marker — provenance is its own column now.
                provenance: Provenance::Inference,
                ..NewFact::about(subject.clone(), "might prefer mornings ❓", date(2026, 1, 2))
            },
        )
        .await;

        let seen_testi = read_back(store, &subject, &testi.id).await;
        let seen_infer = read_back(store, &subject, &infer.id).await;
        assert_eq!(seen_testi.provenance, Provenance::Testimony);
        assert_eq!(seen_testi.content, "speaks two languages");
        assert_eq!(seen_infer.provenance, Provenance::Inference);
        assert_eq!(seen_infer.content, "might prefer mornings ❓");
    }

    /// Edge whitespace is not significant: capture normalizes it, and the
    /// returned fact is byte-identical to what recall reads back.
    pub async fn edge_whitespace_is_normalized<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-whitespace");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "   likes espresso   ", date(2026, 7, 24)),
        )
        .await;
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
        let a = capture(store, NewFact::about(subject.clone(), "plays go", date(2026, 7, 1))).await;
        let b = capture(
            store,
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
        )
        .await;
        assert_ne!(a.id, b.id, "each fact must get its own id");
        assert_eq!(read_back(store, &subject, &a.id).await.content, "plays go");
        assert_eq!(read_back(store, &subject, &b.id).await.content, "learning Rust");
    }

    /// Facts about one entity never leak into another's recall — each subject's
    /// facts are isolated (a per-entity doc, in the real adapter).
    pub async fn subjects_are_isolated<M: Memory>(store: &M) {
        let solo = EntityId::person("contract-solo");
        let duet = EntityId::person("contract-duet");
        capture(store, NewFact::about(solo.clone(), "solo fact", date(2026, 7, 1))).await;
        capture(store, NewFact::about(duet.clone(), "duet fact", date(2026, 7, 1))).await;

        let solo_facts = store.recall(&solo).await.expect("recall solo");
        assert!(
            solo_facts.iter().all(|f| f.subject == solo),
            "recall(solo) must only return solo's facts"
        );
        assert!(solo_facts.iter().any(|f| f.content == "solo fact"));
        assert!(
            !solo_facts.iter().any(|f| f.content == "duet fact"),
            "duet's fact must not appear under solo"
        );
    }

    /// An adversarial subject id — one carrying a pipe, a newline, a markdown
    /// header, or a fence — is rejected at capture, never written. This is the
    /// injection guard: a forged subject must not be able to fabricate a fact
    /// row, a table, or a header in someone's doc.
    pub async fn malicious_subjects_are_rejected<M: Memory>(store: &M) {
        for bad in [
            "person:a|b",
            "person:a\nb",
            "person:a\n### forged",
            "person:a`b`",
            "person:a b",
            "receipt:not-a-kind",
        ] {
            let err = store
                .capture(NewFact::about(
                    EntityId(bad.into()),
                    "should never be stored",
                    date(2026, 7, 24),
                ))
                .await
                .expect_err("a malicious subject must be rejected");
            assert!(
                matches!(err, MemoryError::InvalidSubject(_)),
                "expected InvalidSubject for {bad:?}, got {err:?}"
            );
        }
    }

    /// Recalling a subject that has no doc yet returns empty — not an error, and
    /// without creating anything.
    pub async fn recall_unknown_subject_is_empty<M: Memory>(store: &M) {
        let never = EntityId::person("contract-never-captured");
        let facts = store.recall(&never).await.expect("recall should succeed");
        assert!(facts.is_empty(), "unknown subject must recall empty: {facts:?}");
    }

    // --- the entity model ----------------------------------------------------

    /// A fact can be about any of the eight kinds, not just people — and each
    /// lands in its own home, addressable under its own handle.
    pub async fn every_kind_holds_facts<M: Memory>(store: &M) {
        for kind in EntityKind::ALL {
            let subject = EntityId::new(kind, format!("contract-kind-{kind}"));
            let captured = capture(
                store,
                NewFact::about(subject.clone(), format!("a {kind} fact"), date(2026, 7, 24)),
            )
            .await;
            assert_eq!(captured.subject.kind(), Some(kind));
            let seen = read_back(store, &subject, &captured.id).await;
            assert_eq!(seen.content, format!("a {kind} fact"));
        }
    }

    /// `add_entity` writes an entity of any kind, and the read path returns it
    /// with every frontmatter field intact.
    pub async fn add_entity_reads_back<M: Memory>(store: &M) {
        let id = EntityId::new(EntityKind::Project, "contract-atlas");
        let added = add(
            store,
            NewEntity {
                crm: Some("card:874".into()),
                boot: Boot::Always,
                ..NewEntity::new(id.clone(), "Atlas", "user-named")
            },
        )
        .await;
        assert_eq!(added.kind, EntityKind::Project);

        let seen = read_entity(store, &id).await;
        assert_eq!(seen, added, "the listed entity must be byte-identical");
        assert_eq!(seen.name, "Atlas");
        assert_eq!(seen.source, "user-named");
        assert_eq!(seen.crm.as_deref(), Some("card:874"));
        assert_eq!(seen.boot, Boot::Always);
    }

    /// `list_entities(kind)` narrows to one kind and never leaks another's.
    pub async fn list_entities_filters_by_kind<M: Memory>(store: &M) {
        let place = EntityId::new(EntityKind::Place, "contract-north-trail");
        let topic = EntityId::new(EntityKind::Topic, "contract-widgets");
        add(store, NewEntity::new(place.clone(), "North Trail", "user-named")).await;
        add(store, NewEntity::new(topic.clone(), "Widgets", "user-named")).await;

        let places = store
            .list_entities(Some(EntityKind::Place))
            .await
            .expect("list places");
        assert!(places.iter().all(|e| e.kind == EntityKind::Place));
        assert!(places.iter().any(|e| e.id == place));
        assert!(
            !places.iter().any(|e| e.id == topic),
            "a topic must not appear in the place listing"
        );
    }

    /// An entity's metadata edits in place; the handle is untouched.
    pub async fn update_entity_edits_metadata_in_place<M: Memory>(store: &M) {
        let id = EntityId::new(EntityKind::Thing, "contract-red-bike");
        add(store, NewEntity::new(id.clone(), "Red Bike", "user-named")).await;

        let updated = store
            .update_entity(
                &id,
                EntityPatch {
                    name: Some("Red Bike (the gravel one)".into()),
                    crm: Some("card:551".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update_entity should succeed");
        assert_eq!(updated.id, id, "the handle is immutable");
        assert_eq!(updated.source, "user-named", "an omitted field is left alone");

        let seen = read_entity(store, &id).await;
        assert_eq!(seen.name, "Red Bike (the gravel one)");
        assert_eq!(seen.crm.as_deref(), Some("card:551"));
    }

    /// Updating an entity that doesn't exist errors with the nearest candidates
    /// — it never quietly creates one.
    pub async fn update_entity_unknown_handle_never_creates<M: Memory>(store: &M) {
        let ghost = EntityId::new(EntityKind::Thing, "contract-red-bikee");
        let err = store
            .update_entity(&ghost, EntityPatch { name: Some("nope".into()), ..Default::default() })
            .await
            .expect_err("an unknown handle must error");
        let MemoryError::UnknownEntity { nearest, .. } = &err else {
            panic!("expected UnknownEntity, got {err:?}");
        };
        assert!(
            nearest.iter().any(|m| m.handle.slug() == "contract-red-bike"),
            "the error must name the near miss: {nearest:?}"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != ghost),
            "a failed update must not have created the entity"
        );
    }

    // --- addresses and update ------------------------------------------------

    /// Every fact read back carries its global address, and that address is
    /// exactly what `update_fact` accepts — the pairing that makes facts
    /// editable at all.
    pub async fn facts_carry_a_usable_address<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-addressable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "address me", date(2026, 7, 24)),
        )
        .await;
        let seen = read_back(store, &subject, &captured.id).await;
        let address = seen.address();
        assert_eq!(address.home, subject, "a fact's home is the doc it lives in");
        assert_eq!(
            FactAddress::parse(&address.to_string()).expect("the address must round-trip"),
            address
        );

        let updated = store
            .update_fact(
                &address,
                FactPatch { content: Some("addressed and edited".into()), ..Default::default() },
            )
            .await
            .expect("update via the returned address");
        assert_eq!(updated.content, "addressed and edited");
    }

    /// An edit rewrites the row in place — fix-the-source — and the read path
    /// shows the new truth with no second copy left beside it.
    pub async fn update_fact_edits_in_place<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-editable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "works at the old place", date(2026, 7, 1)),
        )
        .await;
        store
            .update_fact(
                &captured.address(),
                FactPatch {
                    content: Some("works at the new place".into()),
                    details: Some("changed jobs in July".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed");

        let facts = store.recall(&subject).await.expect("recall");
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen.content, "works at the new place");
        assert_eq!(seen.details.as_deref(), Some("changed jobs in July"));
        assert_eq!(
            facts.iter().filter(|f| f.id == captured.id).count(),
            1,
            "an edit rewrites the row; it never appends a second one"
        );
        assert!(
            !facts.iter().any(|f| f.content == "works at the old place"),
            "the old claim must be gone, not left beside the new one"
        );
    }

    /// Negating is a status flip: the fact keeps its id and stays readable, so
    /// nothing that referenced it breaks and the claim can't be re-inferred.
    pub async fn negating_is_a_status_flip_not_a_delete<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-negatable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "a close contact of the user", date(2026, 7, 1)),
        )
        .await;
        let negated = store
            .update_fact(
                &captured.address(),
                FactPatch {
                    content: Some("NOT a close friend — do not re-infer closeness".into()),
                    status: Some(FactStatus::Negated),
                    ..Default::default()
                },
            )
            .await
            .expect("negate should succeed");
        assert_eq!(negated.id, captured.id, "a negated fact keeps its id");

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen.status, FactStatus::Negated);
        assert!(seen.content.starts_with("NOT a close friend"));
    }

    /// Promotion to testimony is gated on the user's explicit confirmation —
    /// and a refused promotion leaves the fact exactly as it was.
    pub async fn promotion_to_testimony_needs_confirmation<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-promotable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "prefers mornings", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(captured.provenance, Provenance::Inference);

        let err = store
            .update_fact(
                &captured.address(),
                FactPatch { provenance: Some(Provenance::Testimony), ..Default::default() },
            )
            .await
            .expect_err("an unconfirmed promotion must be refused");
        assert!(
            matches!(err, MemoryError::UnconfirmedPromotion),
            "expected UnconfirmedPromotion, got {err:?}"
        );
        assert_eq!(
            read_back(store, &subject, &captured.id).await.provenance,
            Provenance::Inference,
            "a refused promotion must leave the fact untouched"
        );

        let promoted = store
            .update_fact(
                &captured.address(),
                FactPatch {
                    provenance: Some(Provenance::Testimony),
                    confirmed_by_user: true,
                    ..Default::default()
                },
            )
            .await
            .expect("a confirmed promotion is allowed");
        assert_eq!(promoted.provenance, Provenance::Testimony);
        assert_eq!(
            read_back(store, &subject, &captured.id).await.provenance,
            Provenance::Testimony
        );
    }

    /// Demotion needs no ceremony — only promotion is gated.
    pub async fn demotion_to_inference_is_free<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-demotable");
        let captured = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                ..NewFact::about(subject.clone(), "said to like winter", date(2026, 7, 1))
            },
        )
        .await;
        let demoted = store
            .update_fact(
                &captured.address(),
                FactPatch { provenance: Some(Provenance::Inference), ..Default::default() },
            )
            .await
            .expect("demotion should succeed");
        assert_eq!(demoted.provenance, Provenance::Inference);
    }

    /// An unknown address errors with the addresses that do exist, and writes
    /// nothing — the never-guess rule on the update path.
    pub async fn update_fact_unknown_address_never_creates<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-missing-row");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "the only row here", date(2026, 7, 1)),
        )
        .await;
        let ghost = FactAddress::new(subject.clone(), FactId("f999".into()));
        let err = store
            .update_fact(&ghost, FactPatch { content: Some("nope".into()), ..Default::default() })
            .await
            .expect_err("an unknown address must error");
        let MemoryError::UnknownFact { nearest, .. } = &err else {
            panic!("expected UnknownFact, got {err:?}");
        };
        assert!(
            nearest.contains(&captured.address().to_string()),
            "the error must list the addresses that do exist: {nearest:?}"
        );

        let facts = store.recall(&subject).await.expect("recall");
        assert_eq!(facts.len(), 1, "nothing was created: {facts:?}");
        assert_eq!(facts[0].content, "the only row here");
    }

    // --- the write guard, on the write path ----------------------------------

    /// The golden case: a second entity at an existing handle is blocked, and
    /// `create_new` cannot force it. Two same-named people can never merge into
    /// one portrait silently.
    pub async fn add_entity_blocks_an_existing_handle<M: Memory>(store: &M) {
        let id = EntityId::person("contract-alpha");
        add(store, NewEntity::new(id.clone(), "Alpha", "crm-card")).await;

        for create_new in [false, true] {
            let outcome = store
                .add_entity(NewEntity {
                    create_new,
                    ..NewEntity::new(id.clone(), "Alpha Two", "user-named")
                })
                .await
                .expect("the call itself succeeds; the guard answers in the result");
            let Guarded::Blocked { candidates, .. } = outcome else {
                panic!("a colliding handle must be blocked (create_new={create_new})");
            };
            assert_eq!(candidates[0].reason, guard::MatchReason::ExactHandle);
            assert_eq!(candidates[0].source, "crm-card", "the caller decides on the source");
        }

        let seen = read_entity(store, &id).await;
        assert_eq!(seen.name, "Alpha", "the blocked write must not have overwritten anything");
    }

    /// A near-miss handle is reported, and the explicit create-new signal is
    /// what lets a genuinely different entity through.
    pub async fn add_entity_reports_a_near_miss_then_accepts_create_new<M: Memory>(store: &M) {
        let first = EntityId::new(EntityKind::Org, "contract-riverside");
        add(store, NewEntity::new(first.clone(), "Riverside", "user-named")).await;

        let typo = EntityId::new(EntityKind::Org, "contract-riversid");
        let outcome = store
            .add_entity(NewEntity::new(typo.clone(), "Riversid", "user-named"))
            .await
            .expect("call succeeds");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a one-letter-off handle must be reported");
        };
        assert!(candidates.iter().any(|m| m.handle == first));
        assert!(
            store
                .list_entities(Some(EntityKind::Org))
                .await
                .expect("list orgs")
                .iter()
                .all(|e| e.id != typo),
            "a blocked add must write nothing"
        );

        let forced = add(
            store,
            NewEntity { create_new: true, ..NewEntity::new(typo.clone(), "Riversid", "user-named") },
        )
        .await;
        assert_eq!(forced.id, typo);
    }

    /// Capture guards a subject that doesn't resolve — the same check, on the
    /// path a fact takes. Capturing about an entity that *does* exist is never
    /// guarded, or every fact would need confirming.
    pub async fn capture_guards_only_an_unresolved_subject<M: Memory>(store: &M) {
        let known = EntityId::person("contract-zenith");
        add(store, NewEntity::new(known.clone(), "Zenith", "user-named")).await;

        // Second fact about a known entity: waved straight through.
        capture(store, NewFact::about(known.clone(), "likes long walks", date(2026, 7, 1))).await;

        let typo = EntityId::person("contract-zenit");
        let outcome = store
            .capture(NewFact::about(typo.clone(), "should not land yet", date(2026, 7, 1)))
            .await
            .expect("call succeeds");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a near-miss subject must be reported before a doc is spawned");
        };
        assert!(candidates.iter().any(|m| m.handle == known));
        assert!(
            store.recall(&typo).await.expect("recall").is_empty(),
            "a blocked capture must write no facts"
        );
        // …and no entity either. Checking only for facts left the guard's
        // "write NOTHING" half-tested: an adapter that provisioned the doc
        // before screening would still show an empty fact table here, so the
        // near-duplicate entity it just spawned would pass unnoticed.
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != typo),
            "a blocked capture must not have provisioned the entity either"
        );

        let forced = capture(
            store,
            NewFact { create_new: true, ..NewFact::about(typo.clone(), "now it lands", date(2026, 7, 1)) },
        )
        .await;
        assert_eq!(forced.subject, typo);
        assert_eq!(read_back(store, &typo, &forced.id).await.content, "now it lands");
    }

    /// A `capture` about an entity nobody added self-provisions its home, and
    /// that entity shows up in the listing sourced as `capture` — existence is
    /// always sourced, never invented.
    pub async fn capture_self_provisions_a_sourced_entity<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::Work, "contract-first-mix");
        capture(store, NewFact::about(subject.clone(), "32 tracks", date(2026, 7, 1))).await;
        let seen = read_entity(store, &subject).await;
        assert_eq!(seen.kind, EntityKind::Work);
        assert_eq!(seen.source, "capture", "a self-provisioned entity says where it came from");
    }

    /// An entity write with a malformed field is refused outright — a name that
    /// could break out of its frontmatter line never reaches the store.
    pub async fn malformed_entity_fields_are_rejected<M: Memory>(store: &M) {
        let id = EntityId::person("contract-injector");
        for (name, source, crm) in [
            ("", "user-named", None),
            ("ok", "", None),
            ("bad\nid: person:someone-else", "user-named", None),
            ("bad ```", "user-named", None),
            ("ok", "user-named", Some("card:abc")),
        ] {
            let err = store
                .add_entity(NewEntity {
                    crm: crm.map(str::to_string),
                    ..NewEntity::new(id.clone(), name, source)
                })
                .await
                .expect_err("a malformed entity field must be rejected");
            assert!(
                matches!(err, MemoryError::InvalidEntity(_)),
                "expected InvalidEntity for {name:?}/{source:?}/{crm:?}, got {err:?}"
            );
        }
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
        malicious_subjects_are_rejected(store).await;
        recall_unknown_subject_is_empty(store).await;

        every_kind_holds_facts(store).await;
        add_entity_reads_back(store).await;
        list_entities_filters_by_kind(store).await;
        update_entity_edits_metadata_in_place(store).await;
        update_entity_unknown_handle_never_creates(store).await;

        facts_carry_a_usable_address(store).await;
        update_fact_edits_in_place(store).await;
        negating_is_a_status_flip_not_a_delete(store).await;
        promotion_to_testimony_needs_confirmation(store).await;
        demotion_to_inference_is_free(store).await;
        update_fact_unknown_address_never_creates(store).await;

        add_entity_blocks_an_existing_handle(store).await;
        add_entity_reports_a_near_miss_then_accepts_create_new(store).await;
        capture_guards_only_an_unresolved_subject(store).await;
        capture_self_provisions_a_sourced_entity(store).await;
        malformed_entity_fields_are_rejected(store).await;
    }
}
