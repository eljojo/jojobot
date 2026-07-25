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
    normalize_content, normalize_details, search, validate_content, validate_details, validate_edge,
    validate_entity, validate_subject,
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

}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        validate_entity(&new.id, &new.name, &new.aliases, &new.source, new.crm.as_deref())?;
        if let Decision::Block(candidates) =
            guard::decide(&new.id, &new.labels(), &self.index(), new.create_new)
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
            aliases: new.aliases.iter().map(|a| a.trim().to_string()).collect(),
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
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_subject(handle)?;
        // Taken before the lock: index() locks too.
        let index = self.index();
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == handle) else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, &[], &entities),
            });
        };
        // A rename is an entity-touching write, so it faces the same gate.
        if let Some(new_name) = &patch.name
            && let Decision::Block(candidates) =
                guard::decide_rename(handle, new_name, &entity.name, &index, patch.create_new)
        {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates,
            });
        }
        apply_entity_patch(entity, &patch)?;
        Ok(Guarded::Written(entity.clone()))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        // Same guards the real adapter applies, so the fake can't drift.
        validate_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;
        if let Some(edge) = &fact.edge {
            validate_edge(edge)?;
        }

        // Every entity this write names must already exist — the subject first,
        // then the edge's object. Nothing here provisions.
        let index = self.index();
        if let Decision::Block(candidates) = guard::decide_existing(&fact.subject, &index) {
            return Ok(Guarded::Blocked {
                attempted: fact.subject,
                candidates,
            });
        }
        if let Some(edge) = &fact.edge
            && let Decision::Block(candidates) = guard::decide_existing(&edge.object, &index)
        {
            return Ok(Guarded::Blocked {
                attempted: edge.object.clone(),
                candidates,
            });
        }

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
            edge: fact.edge,
        };
        facts.push(stored.clone());
        Ok(Guarded::Written(stored))
    }

    /// Home-doc membership counts alongside the subject, as it does in the real
    /// store: a row homed here is reachable here, whatever its subject cell says.
    /// The fake cannot produce that disagreement — every capture homes a fact at
    /// its subject — but the two adapters must not differ on the rule.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let facts = self.facts.lock().expect("fake mutex poisoned");
        Ok(facts
            .iter()
            .filter(|f| &f.subject == subject || &f.home == subject)
            .cloned()
            .collect())
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        // An edge's object names an entity, so an edit that attaches one is an
        // entity-touching write and faces the guard — same check, same order:
        // screened before anything is rewritten.
        if let Some(edge) = &patch.edge {
            validate_edge(edge)?;
            if let Decision::Block(candidates) = guard::decide_existing(&edge.object, &self.index())
            {
                return Ok(Guarded::Blocked {
                    attempted: edge.object.clone(),
                    candidates,
                });
            }
        }
        // A miss on the HANDLE is an entity miss, with the near candidates that
        // explain it — not a fact miss trailing an empty address list.
        let index = self.index();
        if !index.iter().any(|e| e.id == address.home) {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        }

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
        Ok(Guarded::Written(fact.clone()))
    }

    /// The fake holds no prose — no verb writes any yet, so a doc here is its
    /// frontmatter plus its facts. The **handle doubles as the doc id**: in the
    /// fake an entity's handle IS the key its facts are filed under, so it is the
    /// honest answer to "which document is this".
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        let facts = self.facts.lock().expect("fake mutex poisoned").clone();
        Ok(self
            .index()
            .into_iter()
            .map(|entity| search::DocScan {
                doc_id: entity.id.to_string(),
                title: entity.name.clone(),
                prose: String::new(),
                facts: facts.iter().filter(|f| f.home == entity.id).cloned().collect(),
                entity: Some(entity),
            })
            .collect())
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
    use crate::memory::search::{EdgeFilter, Hit, Search, SearchQuery};
    use crate::memory::{Boot, Edge, EdgeShape, FactStatus, Provenance};
    use jiff::civil::{Date, date};

    /// Make sure `id` exists, so the write guard's **existence gate** is not
    /// what a spec about something else trips over. Idempotent: the suite runs
    /// against a shared, pre-populated collection as much as an empty fake.
    ///
    /// The gate itself has its own specs below; everywhere else, provisioning is
    /// setup, not the subject under test.
    async fn ensure<M: Memory>(store: &M, id: &EntityId) {
        let known = store.list_entities(None).await.expect("list_entities should succeed");
        if known.iter().any(|e| &e.id == id) {
            return;
        }
        add(store, NewEntity::new(id.clone(), id.slug(), "contract-fixture")).await;
    }

    /// Capture a fact the guard is expected to wave through — provisioning its
    /// subject and any edge object first, because every write that names an
    /// entity now requires one that exists.
    async fn capture<M: Memory>(store: &M, fact: NewFact) -> Fact {
        ensure(store, &fact.subject).await;
        if let Some(edge) = &fact.edge {
            ensure(store, &edge.object).await;
        }
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

    /// Edit a fact the guard is expected to wave through — provisioning any edge
    /// object the patch attaches, for the same reason [`capture`] does.
    async fn edit<M: Memory>(store: &M, address: &FactAddress, patch: FactPatch) -> Fact {
        if let Some(edge) = &patch.edge {
            ensure(store, &edge.object).await;
        }
        store
            .update_fact(address, patch)
            .await
            .expect("update_fact should succeed")
            .written()
            .unwrap_or_else(|| panic!("the guard must not block the edit at {address}"))
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
            edge: None,
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
            .expect("update_entity should succeed")
            .written()
            .expect("the guard must not block an uncontested rename");
        assert_eq!(updated.id, id, "the handle is immutable");
        assert_eq!(updated.source, "user-named", "an omitted field is left alone");

        let seen = read_entity(store, &id).await;
        assert_eq!(seen.name, "Red Bike (the gravel one)");
        assert_eq!(seen.crm.as_deref(), Some("card:551"));
    }

    /// Renaming an entity onto a name the index already holds is screened by the
    /// same guard that screens creation. Otherwise the guard is trivially
    /// side-steppable: create under a throwaway name, then rename onto the
    /// collision — and two people wear one name with no confirmation asked.
    pub async fn update_entity_screens_a_colliding_rename<M: Memory>(store: &M) {
        let first = EntityId::person("contract-renamed-onto");
        let second = EntityId::person("contract-renamer");
        add(store, NewEntity::new(first.clone(), "Renamed Onto", "user-named")).await;
        add(store, NewEntity::new(second.clone(), "Renamer", "user-named")).await;

        let outcome = store
            .update_entity(
                &second,
                EntityPatch { name: Some("Renamed Onto".into()), ..Default::default() },
            )
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a rename onto an existing name must be blocked");
        };
        assert!(
            candidates.iter().any(|m| m.handle == first),
            "the guard must name the entity already wearing it: {candidates:?}"
        );

        let entities = store.list_entities(None).await.expect("list");
        let wearing_the_name: Vec<&EntityId> = entities
            .iter()
            .filter(|e| e.name == "Renamed Onto")
            .map(|e| &e.id)
            .collect();
        assert_eq!(
            wearing_the_name,
            vec![&first],
            "an unconfirmed rename onto an existing name must not land"
        );

        // The same explicit signal that clears a creation clears a rename.
        let forced = store
            .update_entity(
                &second,
                EntityPatch {
                    name: Some("Renamed Onto".into()),
                    create_new: true,
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("an explicit create_new resolves the rename");
        assert_eq!(forced.name, "Renamed Onto");
        assert_eq!(forced.id, second, "the handle is untouched by a rename");
    }

    /// A rename is screened on the **name** channel only. An entity whose handle
    /// is a near-slug of another's — a collision already adjudicated when it was
    /// created — must still be freely renamable: re-screening the immutable
    /// handle turned that one decision into a permanent block on the name field.
    pub async fn update_entity_does_not_re_screen_the_handle<M: Memory>(store: &M) {
        let settled = EntityId::person("contract-nearslug");
        let neighbour = EntityId::person("contract-nearslugg");
        add(store, NewEntity::new(settled, "Nearslug One", "user-named")).await;
        add(
            store,
            NewEntity {
                // The near-slug the guard reported, judged different at creation.
                create_new: true,
                ..NewEntity::new(neighbour.clone(), "Quite Another Two", "user-named")
            },
        )
        .await;

        let renamed = store
            .update_entity(
                &neighbour,
                EntityPatch { name: Some("Quite Another Three".into()), ..Default::default() },
            )
            .await
            .expect("update_entity should succeed")
            .written()
            .expect("a near-slug settled at creation must not block a later name edit");
        assert_eq!(renamed.name, "Quite Another Three");
    }

    /// Editing metadata that isn't the name is never screened — an entity's own
    /// name must not trip the guard against itself.
    pub async fn update_entity_without_a_rename_is_not_screened<M: Memory>(store: &M) {
        let id = EntityId::new(EntityKind::Org, "contract-unscreened");
        add(store, NewEntity::new(id.clone(), "Unscreened Org", "user-named")).await;
        let same_name_again = store
            .update_entity(
                &id,
                EntityPatch {
                    name: Some("Unscreened Org".into()),
                    source: Some("crm-card".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("an entity is not a candidate for its own rename");
        assert_eq!(same_name_again.source, "crm-card");
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

        let updated = edit(
            store,
            &address,
            FactPatch { content: Some("addressed and edited".into()), ..Default::default() },
        )
        .await;
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
        edit(
            store,
            &captured.address(),
            FactPatch {
                content: Some("works at the new place".into()),
                details: Some("changed jobs in July".into()),
                ..Default::default()
            },
        )
        .await;

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

    /// **A refutation is an ordinary content edit**, not a status. It rewrites
    /// the row in place to state the negative truth, keeps its id, and stays
    /// `active` — because "does NOT play the theremin" IS the current truth
    /// about this entity, and the reader must find it on a plain default read.
    ///
    /// The alternative — a `negated` flag beside the disproved claim — was the
    /// "was wrong, see flag" anti-pattern: it left two versions on the page for
    /// the reader to adjudicate, and hid the correction from every default
    /// search, which is precisely where it needed to be.
    pub async fn a_refutation_is_an_ordinary_content_edit<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-refutable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "a close contact of the user", date(2026, 7, 1)),
        )
        .await;
        let refuted = edit(
            store,
            &captured.address(),
            FactPatch {
                content: Some("NOT a close contact — do not re-infer closeness".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(refuted.id, captured.id, "the row is rewritten, not replaced");

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen.status, FactStatus::Active, "the negative truth is the truth");
        assert!(seen.content.starts_with("NOT a close contact"));

        let facts = store.recall(&subject).await.expect("recall");
        assert!(
            !facts.iter().any(|f| f.content == "a close contact of the user"),
            "the refuted claim is gone from the page, not flagged beside it: {facts:?}"
        );
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

        let promoted = edit(
            store,
            &captured.address(),
            FactPatch {
                provenance: Some(Provenance::Testimony),
                confirmed_by_user: true,
                ..Default::default()
            },
        )
        .await;
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
        let demoted = edit(
            store,
            &captured.address(),
            FactPatch { provenance: Some(Provenance::Inference), ..Default::default() },
        )
        .await;
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

    /// **An address that misses on its HANDLE is an entity miss, not a fact
    /// miss.** It came back as "no fact at 'person:zenit#f1'; addresses here:"
    /// — a dangling empty list that named nothing and pointed at nothing, while
    /// the actual mistake was one field to the left. The two misses have
    /// different causes and different fixes, so they say different things: an
    /// unknown handle answers with near misses, exactly as `update_entity`
    /// does, and a known entity that simply holds no rows says so plainly.
    pub async fn update_fact_tells_an_unknown_handle_from_an_empty_entity<M: Memory>(store: &M) {
        let known = EntityId::person("contract-addressee");
        add(store, NewEntity::new(known.clone(), "Addressee", "user-named")).await;

        let nudge = || FactPatch { content: Some("nope".into()), ..Default::default() };

        let typo = EntityId::person("contract-addresse");
        let err = store
            .update_fact(&FactAddress::new(typo, FactId("f1".into())), nudge())
            .await
            .expect_err("an address on an unknown handle must error");
        let MemoryError::UnknownEntity { nearest, .. } = &err else {
            panic!("a handle that names no entity is an entity miss, got {err:?}");
        };
        assert!(
            nearest.iter().any(|m| m.handle == known),
            "…and it names the near miss the caller probably meant: {nearest:?}"
        );

        // The entity is real; it just has nothing in it yet.
        let err = store
            .update_fact(&FactAddress::new(known.clone(), FactId("f1".into())), nudge())
            .await
            .expect_err("an address on an entity with no facts must error");
        let MemoryError::UnknownFact { nearest, .. } = &err else {
            panic!("a real entity with no rows is a fact miss, got {err:?}");
        };
        assert!(nearest.is_empty(), "there are no addresses to list: {nearest:?}");
        assert!(
            !err.to_string().trim_end().ends_with(':'),
            "the message must not trail off into an empty list: {err}"
        );

        // …and once it holds one, the miss lists what does exist.
        let real = capture(
            store,
            NewFact::about(known.clone(), "the only row here", date(2026, 7, 1)),
        )
        .await;
        let err = store
            .update_fact(&FactAddress::new(known, FactId("f999".into())), nudge())
            .await
            .expect_err("an unknown row must still error");
        let MemoryError::UnknownFact { nearest, .. } = &err else {
            panic!("expected UnknownFact, got {err:?}");
        };
        assert!(nearest.contains(&real.address().to_string()), "got {nearest:?}");
    }

    // --- structured edges at capture -----------------------------------------

    /// An edge is written atomically with its fact and comes back on the read
    /// path. This is what makes ask-across an edge walk instead of an AI reading
    /// prose, so it is bound by the same read-back invariant as the row itself.
    pub async fn capture_writes_an_edge_that_reads_back<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-edged");
        let edge = Edge::new(EdgeShape::Location, EntityId::new(EntityKind::Place, "contract-far-country"));
        let captured = capture(
            store,
            NewFact {
                edge: Some(edge.clone()),
                ..NewFact::about(subject.clone(), "spending the winter away", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(captured.edge.as_ref(), Some(&edge));

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured, "the edge must survive read-back byte-identical");
        assert_eq!(seen.edge.map(|e| e.object), Some(edge.object));
    }

    /// Every shape survives the trip, each with an object of the kind it requires.
    pub async fn every_edge_shape_reads_back<M: Memory>(store: &M) {
        let shapes = [
            (EdgeShape::Location, EntityKind::Place),
            (EdgeShape::Membership, EntityKind::Org),
            (EdgeShape::Attendance, EntityKind::Event),
            (EdgeShape::About, EntityKind::Topic),
        ];
        for (shape, kind) in shapes {
            let subject = EntityId::person(format!("contract-shape-{shape}"));
            let object = EntityId::new(kind, format!("contract-object-{shape}"));
            let captured = capture(
                store,
                NewFact {
                    edge: Some(Edge::new(shape, object.clone())),
                    ..NewFact::about(subject.clone(), format!("a {shape} claim"), date(2026, 7, 1))
                },
            )
            .await;
            let seen = read_back(store, &subject, &captured.id).await;
            assert_eq!(
                seen.edge,
                Some(Edge::new(shape, object)),
                "the {shape} edge must read back"
            );
        }
    }

    /// An object of the wrong kind for its shape is refused outright, and the
    /// fact does not land either — the edge is part of the write, not a garnish.
    pub async fn a_wrong_kind_edge_object_is_refused<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-miskinded");
        let err = store
            .capture(NewFact {
                // A `location` must point at a place; this one points at a person.
                edge: Some(Edge::new(EdgeShape::Location, EntityId::person("contract-alpha"))),
                ..NewFact::about(subject.clone(), "should never be stored", date(2026, 7, 1))
            })
            .await
            .expect_err("a wrong-kind edge object must be refused");
        assert!(matches!(err, MemoryError::InvalidEdge(_)), "got {err:?}");
        assert!(
            store.recall(&subject).await.expect("recall").is_empty(),
            "a refused edge must take its fact with it: nothing written"
        );
    }

    /// The **object is screened by the write guard exactly as a subject is.** A
    /// typo'd object is where ask-across quietly rots: the edge points at a node
    /// nobody else references, so the walk comes back empty and nothing looks
    /// wrong. It comes back as candidates instead, and nothing is written.
    pub async fn an_edge_object_is_screened_by_the_guard<M: Memory>(store: &M) {
        let object = EntityId::new(EntityKind::Place, "contract-riverbend");
        add(store, NewEntity::new(object.clone(), "Riverbend", "user-named")).await;

        let subject = EntityId::person("contract-edge-guarded");
        // The subject faces the gate too, so it is provisioned first: this spec
        // is about the object, and the guard reports the first handle it stops.
        add(store, NewEntity::new(subject.clone(), "Edge Guarded", "user-named")).await;

        let typo = EntityId::new(EntityKind::Place, "contract-riverbnd");
        let outcome = store
            .capture(NewFact {
                edge: Some(Edge::new(EdgeShape::Location, typo.clone())),
                ..NewFact::about(subject.clone(), "should not land yet", date(2026, 7, 1))
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { attempted, candidates } = outcome else {
            panic!("a near-miss edge object must be reported");
        };
        assert_eq!(attempted, typo, "the guard names the handle it stopped");
        assert!(
            candidates.iter().any(|m| m.handle == object),
            "the guard must name the place it suspects: {candidates:?}"
        );
        assert!(
            store.recall(&subject).await.expect("recall").is_empty(),
            "a blocked edge object must write no fact"
        );

        // Confirming the existing object is the ordinary path out.
        let landed = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, object.clone())),
                ..NewFact::about(subject.clone(), "now it lands", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(landed.edge.map(|e| e.object), Some(object));
    }

    /// `update_fact` attaches an edge to a fact that didn't have one — the
    /// day-to-day path for an edge realized after the fact was captured.
    pub async fn update_fact_attaches_an_edge<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-edge-later");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "was at the festival", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(captured.edge, None);

        let edge = Edge::new(
            EdgeShape::Attendance,
            EntityId::new(EntityKind::Event, "contract-winter-fest"),
        );
        let updated = edit(
            store,
            &captured.address(),
            FactPatch { edge: Some(edge.clone()), ..Default::default() },
        )
        .await;
        assert_eq!(updated.edge.as_ref(), Some(&edge));
        assert_eq!(
            read_back(store, &subject, &captured.id).await.edge.as_ref(),
            Some(&edge),
            "the attached edge must be on the read path"
        );
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

    /// **Capture's subject must already exist.** Not "must not look like
    /// something else" — must *be* something.
    ///
    /// A novel subject used to self-provision a nameless entity, so every typo
    /// and every plausible-looking handle an AI produced became a permanent
    /// record nobody chose, indistinguishable from a real one at a glance. There
    /// is no create-new escape on this path either: a genuinely new entity is
    /// `add_entity` and then the capture, two deliberate steps, and the second
    /// is what proves the first was meant.
    pub async fn capture_requires_an_existing_subject<M: Memory>(store: &M) {
        let known = EntityId::person("contract-zenith");
        add(store, NewEntity::new(known.clone(), "Zenith", "user-named")).await;

        // A fact about an entity that exists: waved straight through, always —
        // otherwise every second fact about someone would need confirming.
        capture(store, NewFact::about(known.clone(), "likes long walks", date(2026, 7, 1))).await;

        // A near miss comes back with the candidate that explains it…
        let typo = EntityId::person("contract-zenit");
        let outcome = store
            .capture(NewFact::about(typo.clone(), "should not land", date(2026, 7, 1)))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a near-miss subject must be reported, never provisioned");
        };
        assert!(candidates.iter().any(|m| m.handle == known), "got {candidates:?}");

        // …and a handle nothing resembles blocks just the same, with nothing to
        // suggest. This is the case that used to sail through and provision.
        let stranger = EntityId::new(EntityKind::Work, "contract-first-mix");
        let outcome = store
            .capture(NewFact::about(stranger.clone(), "32 tracks", date(2026, 7, 1)))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { attempted, candidates } = outcome else {
            panic!("an unknown subject must block even with no near match");
        };
        assert_eq!(attempted, stranger, "the guard names the handle it stopped");
        assert!(candidates.is_empty(), "nothing resembles it: {candidates:?}");

        for blocked in [&typo, &stranger] {
            assert!(
                store.recall(blocked).await.expect("recall").is_empty(),
                "a blocked capture must write no facts ({blocked})"
            );
            // …and no entity either. Checking only for facts left the guard's
            // "write NOTHING" half-tested: an adapter that provisioned the doc
            // before screening would still show an empty fact table here.
            assert!(
                store
                    .list_entities(None)
                    .await
                    .expect("list")
                    .iter()
                    .all(|e| &e.id != blocked),
                "a blocked capture must not have provisioned the entity either ({blocked})"
            );
        }

        // The way through is to mean it: add the entity, then capture.
        add(store, NewEntity::new(stranger.clone(), "First Mix", "user-named")).await;
        let landed = capture(
            store,
            NewFact::about(stranger.clone(), "32 tracks", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(landed.subject, stranger);
        assert_eq!(read_back(store, &stranger, &landed.id).await.content, "32 tracks");
        assert_eq!(
            read_entity(store, &stranger).await.source,
            "user-named",
            "existence is sourced by whoever asked for it, never by a side effect"
        );
    }

    /// The same gate on an **edge's object**: a handle nothing resembles is
    /// refused rather than quietly becoming a new node. This is where ask-across
    /// rots silently — the edge points at something nobody else references, the
    /// walk comes back empty, and nothing looks wrong.
    pub async fn capture_requires_an_existing_edge_object<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-edge-stranger");
        add(store, NewEntity::new(subject.clone(), "Edge Stranger", "user-named")).await;

        let stranger = EntityId::new(EntityKind::Event, "contract-unheard-of-fest");
        let outcome = store
            .capture(NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
                ..NewFact::about(subject.clone(), "should not land", date(2026, 7, 1))
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { attempted, candidates } = outcome else {
            panic!("an unknown edge object must block even with no near match");
        };
        assert_eq!(attempted, stranger);
        assert!(candidates.is_empty(), "nothing resembles it: {candidates:?}");
        assert!(
            store.recall(&subject).await.expect("recall").is_empty(),
            "a blocked object must take its fact with it"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != stranger),
            "…and must not have provisioned the object either"
        );

        add(store, NewEntity::new(stranger.clone(), "Unheard-of Fest", "user-named")).await;
        let landed = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
                ..NewFact::about(subject.clone(), "went both nights", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(landed.edge.map(|e| e.object), Some(stranger));
    }

    /// The same gate on an **edge attached later**. `capture` has had this spec
    /// since the gate was built; `update_fact` had the code and no spec, so
    /// deleting the check from its path left the whole suite green — and the
    /// hole would have been exactly the interesting one: an edge realized after
    /// the fact is the day-to-day way edges get drawn.
    pub async fn update_fact_requires_an_existing_edge_object<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-late-edge");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "was somewhere that week", date(2026, 7, 1)),
        )
        .await;

        let stranger = EntityId::new(EntityKind::Place, "contract-nowhere-in-particular");
        let outcome = store
            .update_fact(
                &captured.address(),
                FactPatch {
                    edge: Some(Edge::new(EdgeShape::Location, stranger.clone())),
                    ..Default::default()
                },
            )
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { attempted, candidates } = outcome else {
            panic!("an edge object that names no entity must block the edit");
        };
        assert_eq!(attempted, stranger, "the guard names the handle it stopped");
        assert!(candidates.is_empty(), "nothing resembles it: {candidates:?}");

        assert_eq!(
            read_back(store, &subject, &captured.id).await.edge,
            None,
            "a blocked edit must leave the fact exactly as it was"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != stranger),
            "…and must not have provisioned the object either"
        );

        // Two deliberate steps, here as everywhere: add the entity, then write.
        add(store, NewEntity::new(stranger.clone(), "Nowhere In Particular", "user-named")).await;
        let landed = edit(
            store,
            &captured.address(),
            FactPatch {
                edge: Some(Edge::new(EdgeShape::Location, stranger.clone())),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(landed.edge.map(|e| e.object), Some(stranger));
    }

    /// **An entity keeps the other names it answers to**, through the store and
    /// back. A nickname that survives only in the caller's request is a nickname
    /// the next session has never heard of.
    pub async fn add_entity_keeps_its_alternate_names<M: Memory>(store: &M) {
        let id = EntityId::person("contract-many-named");
        let added = add(
            store,
            NewEntity {
                aliases: vec!["Contract Nickname".into(), "C.M.N.".into()],
                ..NewEntity::new(id.clone(), "Contract Many-Named", "user-named")
            },
        )
        .await;
        assert_eq!(added.aliases, vec!["Contract Nickname", "C.M.N."]);
        assert_eq!(read_entity(store, &id).await, added, "…on the read path too");

        // The set is replaced whole, and an omitted field is left alone.
        let renamed = store
            .update_entity(&id, EntityPatch { source: Some("crm-card".into()), ..Default::default() })
            .await
            .expect("update ok")
            .written()
            .expect("not blocked");
        assert_eq!(renamed.aliases, added.aliases, "an omitted alias set is untouched");

        let replaced = store
            .update_entity(
                &id,
                EntityPatch { aliases: Some(vec!["Only This One".into()]), ..Default::default() },
            )
            .await
            .expect("update ok")
            .written()
            .expect("not blocked");
        assert_eq!(replaced.aliases, vec!["Only This One"]);
        assert_eq!(
            read_entity(store, &id).await.aliases,
            vec!["Only This One"],
            "the replacement is what the store holds, not an addendum beside it"
        );

        // And an alias carrying the separator is refused before anything moves.
        let err = store
            .update_entity(
                &id,
                EntityPatch { aliases: Some(vec!["one, two".into()]), ..Default::default() },
            )
            .await
            .expect_err("an alias with a comma in it must be refused");
        assert!(matches!(err, MemoryError::InvalidEntity(_)), "got {err:?}");
        assert_eq!(read_entity(store, &id).await.aliases, vec!["Only This One"]);
    }

    /// **The guard knows every name an entity answers to.** Someone filed under
    /// one name and called another is one entity; a write arriving under the
    /// nickname has to hit the same gate a write under the display name does, or
    /// a second record gets created under the name the user actually says and
    /// the facts split evenly between the two.
    pub async fn add_entity_screens_every_name_an_entity_answers_to<M: Memory>(store: &M) {
        let known = EntityId::person("contract-many-labelled");
        add(
            store,
            NewEntity {
                aliases: vec!["Contract Nickname Only".into()],
                ..NewEntity::new(known.clone(), "Contract Many-Labelled", "user-named")
            },
        )
        .await;

        let under_the_alias = EntityId::person("contract-nickname-only");
        let outcome = store
            .add_entity(NewEntity::new(
                under_the_alias.clone(),
                "Contract Nickname Only",
                "user-named",
            ))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a write under a name the entity already answers to must block");
        };
        assert!(
            candidates.iter().any(|m| m.handle == known),
            "the guard must name the entity that wears it: {candidates:?}"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != under_the_alias),
            "a blocked add writes nothing"
        );
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

    // --- retrieval: the search verb ------------------------------------------
    //
    // These run against a store that also carries the search projection. They
    // are scoped to handles this suite owns, so they hold against a shared,
    // pre-populated collection as much as against an empty fake.

    /// Search the store, expecting the query to be well-formed.
    fn found<S: Search>(store: &S, query: SearchQuery) -> Vec<Hit> {
        store
            .search(&query)
            .unwrap_or_else(|e| panic!("search should succeed: {e}"))
    }

    /// The facts in a result list, by address.
    fn fact_hits(hits: &[Hit]) -> Vec<&Fact> {
        hits.iter()
            .filter_map(|h| match h {
                Hit::Fact { fact } => Some(fact),
                _ => None,
            })
            .collect()
    }

    /// **Read-back extends to the index.** A fact captured a moment ago is
    /// findable by the next search call, with no restart — otherwise "captured"
    /// means "written somewhere the assistant can't look".
    pub async fn search_finds_a_fact_captured_moments_ago<S: Memory + Search>(store: &S) {
        let subject = EntityId::person("contract-searchable");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "keeps a zamboni in the garage", date(2026, 7, 1)),
        )
        .await;

        let hits = found(store, SearchQuery::text("zamboni"));
        let addresses: Vec<String> = fact_hits(&hits).iter().map(|f| f.address().to_string()).collect();
        assert!(
            addresses.contains(&captured.address().to_string()),
            "the fact just captured must be findable without a restart: {hits:?}"
        );
    }

    /// Every fact hit carries the **whole row** — its address and its provenance
    /// included. The address is what an edit needs; the provenance is what keeps a
    /// guess from being read as something the user said.
    pub async fn search_fact_hits_carry_an_address_and_provenance<S: Memory + Search>(store: &S) {
        let subject = EntityId::person("contract-search-fields");
        capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                details: Some("said so twice".into()),
                ..NewFact::about(subject.clone(), "cycles to the velodrome", date(2026, 7, 1))
            },
        )
        .await;

        let hits = found(store, SearchQuery::text("velodrome"));
        let facts = fact_hits(&hits);
        let found_fact = facts
            .iter()
            .find(|f| f.subject == subject)
            .unwrap_or_else(|| panic!("the captured fact must come back: {hits:?}"));
        assert_eq!(found_fact.provenance, Provenance::Testimony);
        assert_eq!(found_fact.details.as_deref(), Some("said so twice"));
        assert_eq!(found_fact.address().home, subject, "the address names its home doc");
        assert_eq!(found_fact.address().local, found_fact.id);
    }

    /// A superseded fact is **out of a default search** — a claim the store has
    /// already moved past coming back as current truth is worse than no memory
    /// at all — and `status: superseded` is how it is reached deliberately, so
    /// nothing is destroyed, only demoted.
    ///
    /// This is the default-exclusion contract; only the negated variant had one
    /// before, and that variant is gone.
    pub async fn search_excludes_superseded_by_default_and_lists_it_on_request<S: Memory + Search>(
        store: &S,
    ) {
        let subject = EntityId::person("contract-search-superseded");
        let live = capture(
            store,
            NewFact::about(subject.clone(), "plays the theremin", date(2026, 7, 1)),
        )
        .await;
        let retired = capture(
            store,
            NewFact::about(subject.clone(), "plays the theremin on Tuesdays", date(2026, 7, 2)),
        )
        .await;
        edit(
            store,
            &retired.address(),
            FactPatch { status: Some(FactStatus::Superseded), ..Default::default() },
        )
        .await;

        let default = found(store, SearchQuery::text("theremin"));
        let addresses: Vec<String> = fact_hits(&default).iter().map(|f| f.address().to_string()).collect();
        assert!(
            addresses.contains(&live.address().to_string()),
            "the active fact must be found: {default:?}"
        );
        assert!(
            !addresses.contains(&retired.address().to_string()),
            "a superseded fact must not come back as current truth: {default:?}"
        );

        let asked = found(
            store,
            SearchQuery {
                status: Some(FactStatus::Superseded),
                ..SearchQuery::text("theremin")
            },
        );
        let asked_addresses: Vec<String> = fact_hits(&asked).iter().map(|f| f.address().to_string()).collect();
        assert!(
            asked_addresses.contains(&retired.address().to_string()),
            "asking for it by name is how a superseded fact is reached: {asked:?}"
        );
        assert!(
            !asked_addresses.contains(&live.address().to_string()),
            "…and that list holds only the superseded ones: {asked:?}"
        );
    }

    /// **Ask-across, the capability this milestone exists for:** one call answers
    /// "which people are in X". The filter walks the typed edges, so a fact that
    /// merely *mentions* X in its text is not an answer — that difference is the
    /// whole reason edges are written at capture instead of inferred later.
    pub async fn search_answers_ask_across_by_kind_and_edge<S: Memory + Search>(store: &S) {
        let far = EntityId::new(EntityKind::Place, "contract-faraway");
        let here = capture_at(store, "contract-away-one", &far, date(2026, 7, 1)).await;
        let there = capture_at(store, "contract-away-two", &far, date(2026, 7, 2)).await;

        // A fact that talks about the place but draws no edge to it.
        let talker = EntityId::person("contract-away-talker");
        capture(
            store,
            NewFact::about(talker.clone(), "keeps talking about contract-faraway", date(2026, 7, 3)),
        )
        .await;
        // …and a place that is edged there but is not a person.
        let project = EntityId::new(EntityKind::Project, "contract-away-project");
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, far.clone())),
                ..NewFact::about(project.clone(), "runs out of contract-faraway", date(2026, 7, 4))
            },
        )
        .await;

        let hits = found(
            store,
            SearchQuery {
                kind: Some(EntityKind::Person),
                edge: Some(EdgeFilter {
                    shape: Some(EdgeShape::Location),
                    object: far.clone(),
                }),
                ..Default::default()
            },
        );
        let mut subjects: Vec<String> = fact_hits(&hits)
            .iter()
            .map(|f| f.subject.to_string())
            .collect();
        subjects.sort();
        subjects.dedup();
        assert_eq!(
            subjects,
            vec![here.subject.to_string(), there.subject.to_string()],
            "exactly the people edged there — not the one who merely mentions it, \
             not the project that is: {hits:?}"
        );
    }

    /// An edge filter with **no shape** answers "what's connected to X" — every
    /// edge pointing at it, whatever its shape.
    pub async fn search_by_edge_object_alone_finds_any_shape<S: Memory + Search>(store: &S) {
        let fest = EntityId::new(EntityKind::Event, "contract-connected-fest");
        let attendee = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, fest.clone())),
                ..NewFact::about(EntityId::person("contract-conn-one"), "went both nights", date(2026, 7, 1))
            },
        )
        .await;
        let about = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::About, fest.clone())),
                ..NewFact::about(
                    EntityId::new(EntityKind::Work, "contract-conn-mix"),
                    "recorded live that weekend",
                    date(2026, 7, 2),
                )
            },
        )
        .await;

        // A fact drawing the same shape at a DIFFERENT event. Without it this
        // spec is a containment assertion, and an `edge` filter that matched
        // everything would satisfy it — "connected to X" has to mean X.
        let elsewhere = capture(
            store,
            NewFact {
                edge: Some(Edge::new(
                    EdgeShape::Attendance,
                    EntityId::new(EntityKind::Event, "contract-connected-other"),
                )),
                ..NewFact::about(EntityId::person("contract-conn-two"), "went to the other one", date(2026, 7, 3))
            },
        )
        .await;

        let hits = found(
            store,
            SearchQuery {
                edge: Some(EdgeFilter { shape: None, object: fest }),
                ..Default::default()
            },
        );
        let addresses: Vec<String> = fact_hits(&hits).iter().map(|f| f.address().to_string()).collect();
        for expected in [attendee.address(), about.address()] {
            assert!(
                addresses.contains(&expected.to_string()),
                "every shape pointing at it must come back, got {addresses:?}"
            );
        }
        assert!(
            !addresses.contains(&elsewhere.address().to_string()),
            "…and only the ones pointing at it: {addresses:?}"
        );
    }

    /// A query that names an entity outright puts **that entity first** — decided
    /// by the write guard's own matcher, so search and the guard can never
    /// disagree about what counts as the same thing.
    pub async fn search_pins_a_named_entity_first<S: Memory + Search>(store: &S) {
        let handle = EntityId::new(EntityKind::Org, "contract-pinnable-guild");
        add(store, NewEntity::new(handle.clone(), "Pinnable Guild", "user-named")).await;
        // Facts that also match the query text, so the pin has something to beat.
        capture(
            store,
            NewFact::about(handle.clone(), "meets at the contract-pinnable-guild hall", date(2026, 7, 1)),
        )
        .await;

        let hits = found(store, SearchQuery::text(handle.as_str()));
        assert!(
            matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == handle),
            "an exact handle query must return that entity first: {hits:?}"
        );
    }

    /// Capture a fact placing `who` at `place`, and return it.
    async fn capture_at<M: Memory>(store: &M, who: &str, place: &EntityId, on: Date) -> Fact {
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, place.clone())),
                ..NewFact::about(EntityId::person(who), "spending the season there", on)
            },
        )
        .await
    }

    /// Run the whole contract, **including retrieval**, against a store that
    /// carries the search projection. The search half can't live in `run_all`:
    /// the bare Memory port has no read side for it.
    pub async fn run_all_searchable<S: Memory + Search>(store: &S) {
        run_all(store).await;

        search_finds_a_fact_captured_moments_ago(store).await;
        search_fact_hits_carry_an_address_and_provenance(store).await;
        search_excludes_superseded_by_default_and_lists_it_on_request(store).await;
        search_answers_ask_across_by_kind_and_edge(store).await;
        search_by_edge_object_alone_finds_any_shape(store).await;
        search_pins_a_named_entity_first(store).await;
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
        update_entity_screens_a_colliding_rename(store).await;
        update_entity_does_not_re_screen_the_handle(store).await;
        update_entity_without_a_rename_is_not_screened(store).await;
        update_entity_unknown_handle_never_creates(store).await;
        add_entity_keeps_its_alternate_names(store).await;
        add_entity_screens_every_name_an_entity_answers_to(store).await;

        capture_writes_an_edge_that_reads_back(store).await;
        every_edge_shape_reads_back(store).await;
        a_wrong_kind_edge_object_is_refused(store).await;
        an_edge_object_is_screened_by_the_guard(store).await;
        update_fact_attaches_an_edge(store).await;

        facts_carry_a_usable_address(store).await;
        update_fact_edits_in_place(store).await;
        a_refutation_is_an_ordinary_content_edit(store).await;
        promotion_to_testimony_needs_confirmation(store).await;
        demotion_to_inference_is_free(store).await;
        update_fact_unknown_address_never_creates(store).await;
        update_fact_tells_an_unknown_handle_from_an_empty_entity(store).await;

        add_entity_blocks_an_existing_handle(store).await;
        add_entity_reports_a_near_miss_then_accepts_create_new(store).await;
        capture_requires_an_existing_subject(store).await;
        capture_requires_an_existing_edge_object(store).await;
        update_fact_requires_an_existing_edge_object(store).await;
        malformed_entity_fields_are_rejected(store).await;
    }
}
