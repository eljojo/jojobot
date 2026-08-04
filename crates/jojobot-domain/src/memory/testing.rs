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

use jiff::civil::Date;

use super::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId, FactPatch, FactStatus,
    Guarded, Memory, MemoryError, NewEntity, NewFact, Retraction, Standing, apply_entity_patch,
    apply_fact_patch,
    guard::{self, Decision},
    normalize_content, normalize_details, normalize_prose, retraction_of, screen_entity_patch,
    search, standing_of, validate_content, validate_details, validate_edge, validate_entity,
    validate_event, validate_prose, validate_subject,
};

/// An in-memory [`Memory`] adapter for tests. Holds entities and facts in `Vec`s
/// behind `Mutex`es; mints fact ids `f1`, `f2`, … per home doc, mirroring the
/// real store's per-doc numbering. A fresh instance starts empty.
#[derive(Default)]
pub struct InMemoryMemory {
    entities: Mutex<Vec<Entity>>,
    facts: Mutex<Vec<Fact>>,
    /// The human half of each entity's doc, keyed by handle — replaced whole by
    /// `set_prose`, exactly as the real store replaces the region.
    prose: Mutex<std::collections::HashMap<EntityId, String>>,
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
        validate_entity(
            &new.id,
            &new.name,
            &new.aliases,
            &new.source,
            new.crm.as_deref(),
            new.parent.as_ref(),
        )?;
        let index = self.index();
        if let Decision::Block(candidates) =
            guard::decide(&new.id, &new.labels(), &index, new.create_new)
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
            parent: new.parent,
            boot: new.boot,
        };
        // The entity this one sits under must already exist, and must not be
        // this one. Screened after the record is assembled because a
        // self-parenting block reports the write itself, and this is where the
        // write's own name and source live.
        if let Some(parent) = &entity.parent
            && let Decision::Block(candidates) = guard::decide_parent(&entity, parent, &index)
        {
            return Ok(Guarded::Blocked {
                attempted: parent.clone(),
                candidates,
            });
        }
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
        // Changing what an entity is CALLED is an entity-touching write, so it
        // faces the same gate — display name and aliases alike. Unconditional
        // on purpose: a patch that moves no label screens against nothing, so
        // there is no "is this a rename?" test to get wrong.
        if let Decision::Block(candidates) = screen_entity_patch(entity, &patch, &index) {
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
        if let Some(event) = &fact.event {
            validate_event(event)?;
        }
        let standing = standing_of(&fact);

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
        // **An event's refs are named entities like any other.** The rule is
        // not about edges, it is about naming: nothing a write mentions is
        // brought into being as a side effect of mentioning it. A ref that
        // provisioned its own entity would make the open hatch the one place on
        // the surface where that stopped being true.
        for object in fact.event.iter().flat_map(|e| &e.refs) {
            validate_subject(object)?;
            if let Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
                    candidates,
                });
            }
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
            standing,
            status: fact.status,
            date: fact.date,
            edge: fact.edge,
            event: fact.event,
            derived_from: fact.derived_from,
        };
        facts.push(stored.clone());
        Ok(Guarded::Written(stored))
    }

    /// Home-doc membership counts alongside the subject, as it does in the real
    /// store: a row homed here is reachable here, whatever its subject cell says.
    /// The fake cannot produce that disagreement — every capture homes a fact at
    /// its subject — but the two adapters must not differ on the rule.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        // An unknown entity is a miss with its near candidates — never an
        // empty page. Empty-but-real and nonexistent are different answers.
        let index = self.index();
        if !index.iter().any(|e| &e.id == subject) {
            return Err(MemoryError::UnknownEntity {
                attempted: subject.to_string(),
                nearest: guard::screen(subject, &[], &index),
            });
        }
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
        // A retracted row is out of reach of an ordinary edit — checked here,
        // beside the real store's copy, because one-way that holds in only one
        // adapter holds until somebody switches adapters.
        // A retracted row is out of reach of an ordinary edit — checked here,
        // beside the real store's copy, because one-way that holds in only one
        // adapter holds until somebody switches adapters.
        if fact.status == FactStatus::Retracted {
            return Err(MemoryError::NotRetractable {
                attempted: address.to_string(),
                why: "it is retracted, and a retracted record is not editable — retraction is \
                      one-way. Capture what is so now as a new record"
                    .to_string(),
            });
        }
        apply_fact_patch(fact, &patch)?;
        Ok(Guarded::Written(fact.clone()))
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let index = self.index();
        if !index.iter().any(|e| e.id == address.home) {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        }

        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was — the same shape `apply_fact_patch` has.
        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let nearest: Vec<String> = facts
            .iter()
            .filter(|f| f.home == address.home)
            .map(|f| f.address().to_string())
            .collect();
        let Some(target) = facts
            .iter()
            .find(|f| f.home == address.home && f.id == address.local)
            .cloned()
        else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        };
        let account = retraction_of(&target, reason, date)?;
        let standing = standing_of(&account);

        let home = target.home.clone();
        let existing = facts.iter().filter(|f| f.home == home).count();
        let record = Fact {
            id: FactId(format!("f{}", existing + 1)),
            home,
            subject: account.subject,
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            date: account.date,
            edge: account.edge,
            event: account.event,
            derived_from: account.derived_from,
        };
        let retracted = Fact {
            status: FactStatus::Retracted,
            ..target
        };
        for fact in facts.iter_mut() {
            if fact.home == address.home && fact.id == address.local {
                *fact = retracted.clone();
            }
        }
        facts.push(record.clone());
        Ok(Retraction { retracted, record })
    }

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        validate_subject(entity)?;
        validate_prose(prose)?;
        // Never creates: a handle that names nothing is a miss with its near
        // candidates, exactly as it is for every other verb here.
        let index = self.index();
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        // Normalized here as the real store normalizes it, so the fake cannot
        // preserve whitespace a markdown round-trip would drop.
        let stored = normalize_prose(prose);
        self.prose
            .lock()
            .expect("fake mutex poisoned")
            .insert(entity.clone(), stored.clone());
        Ok(stored)
    }

    /// A doc here is its frontmatter, whatever prose was written onto it, and
    /// its facts. The **handle doubles as the doc id**: in the fake an entity's
    /// handle IS the key its facts are filed under, so it is the honest answer
    /// to "which document is this".
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        let facts = self.facts.lock().expect("fake mutex poisoned").clone();
        let prose = self.prose.lock().expect("fake mutex poisoned").clone();
        // No Journal document: a wrap publishes nowhere, so the journal stays
        // dark until events land — there is no shared page for `search` to
        // scan here.
        Ok(std::iter::empty()
            .chain(self.index().into_iter().map(|entity| {
                search::DocScan {
                    doc_id: entity.id.to_string(),
                    title: entity.name.clone(),
                    prose: prose.get(&entity.id).cloned().unwrap_or_default(),
                    facts: facts
                        .iter()
                        .filter(|f| f.home == entity.id)
                        .cloned()
                        .collect(),
                    entity: Some(entity),
                }
            }))
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
    use crate::memory::{
        Boot, Edge, EdgeShape, FACTS_HEADER, FactStatus, Provenance, event::Event,
    };
    use jiff::civil::{Date, date};

    /// Make sure `id` exists, so the write guard's **existence gate** is not
    /// what a spec about something else trips over. Idempotent: the suite runs
    /// against a shared, pre-populated collection as much as an empty fake.
    ///
    /// The gate itself has its own specs below; everywhere else, provisioning is
    /// setup, not the subject under test.
    async fn ensure<M: Memory>(store: &M, id: &EntityId) {
        let known = store
            .list_entities(None)
            .await
            .expect("list_entities should succeed");
        if known.iter().any(|e| &e.id == id) {
            return;
        }
        add(
            store,
            NewEntity::new(id.clone(), id.slug(), "contract-fixture"),
        )
        .await;
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

    /// Every field survives capture→recall unchanged and byte-identical —
    /// `derived_from` included, since it is a fact field like any other and
    /// this is the one test that pins ALL of them at once.
    pub async fn preserves_all_fields<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-fields");
        let source = FactAddress::parse("person:contract-fields#f1").expect("well-formed");
        let new = NewFact {
            subject: subject.clone(),
            content: "prefers a café table".into(),
            details: Some("mentioned it twice".into()),
            provenance: Provenance::Testimony,
            standing: Some(Standing::Open),
            status: FactStatus::Active,
            date: date(2026, 3, 9),
            edge: None,
            event: None,
            derived_from: Some(source.clone()),
        };
        let captured = capture(store, new).await;
        assert_eq!(captured.subject, subject);
        assert_eq!(captured.content, "prefers a café table");
        assert_eq!(captured.details.as_deref(), Some("mentioned it twice"));
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.standing, Standing::Open);
        assert_eq!(captured.date, date(2026, 3, 9));
        assert_eq!(captured.derived_from, Some(source));

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
                ..NewFact::about(
                    subject.clone(),
                    "reads a|b|c pipe notation",
                    date(2026, 7, 24),
                )
            },
        )
        .await;
        assert_eq!(captured.content, "reads a|b|c pipe notation");
        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(seen, captured);
    }

    /// A backslash in content or details survives the round-trip — byte-identical.
    ///
    /// A contract case rather than a codec unit test, because it is a claim
    /// about STORAGE: a fake keeping bytes verbatim answers yes whatever the
    /// codec does, and only the real store can say the escape holds.
    pub async fn a_backslash_in_content_round_trips<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-backslash");
        let captured = capture(
            store,
            NewFact {
                details: Some(r#"quoted it as \"exactly this\""#.into()),
                ..NewFact::about(
                    subject.clone(),
                    r"the path is c:\dir\file and a trailing \",
                    date(2026, 7, 24),
                )
            },
        )
        .await;
        assert_eq!(
            captured.content,
            r"the path is c:\dir\file and a trailing \"
        );
        assert_eq!(
            captured.details.as_deref(),
            Some(r#"quoted it as \"exactly this\""#)
        );
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
                ..NewFact::about(
                    subject.clone(),
                    "might prefer mornings ❓",
                    date(2026, 1, 2),
                )
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
        let a = capture(
            store,
            NewFact::about(subject.clone(), "plays go", date(2026, 7, 1)),
        )
        .await;
        let b = capture(
            store,
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
        )
        .await;
        assert_ne!(a.id, b.id, "each fact must get its own id");
        assert_eq!(read_back(store, &subject, &a.id).await.content, "plays go");
        assert_eq!(
            read_back(store, &subject, &b.id).await.content,
            "learning Rust"
        );
    }

    /// Facts about one entity never leak into another's recall — each subject's
    /// facts are isolated (a per-entity doc, in the real adapter).
    pub async fn subjects_are_isolated<M: Memory>(store: &M) {
        let solo = EntityId::person("contract-solo");
        let duet = EntityId::person("contract-duet");
        capture(
            store,
            NewFact::about(solo.clone(), "solo fact", date(2026, 7, 1)),
        )
        .await;
        capture(
            store,
            NewFact::about(duet.clone(), "duet fact", date(2026, 7, 1)),
        )
        .await;

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

    /// Recalling an entity nobody ever created is a MISS — `UnknownEntity`,
    /// naming the attempt with the near candidates that explain it — never an
    /// empty success. An empty page and a nonexistent entity are different
    /// facts, and the production smoke test caught them dressed identically: a
    /// caller told a bad handle "reads fine, no facts" can never repair it.
    /// An entity that EXISTS with no facts still recalls empty, and nothing is
    /// created either way.
    pub async fn recall_unknown_is_a_miss_not_an_empty_page<M: Memory>(store: &M) {
        let never = EntityId::person("contract-never-captured");
        let err = store
            .recall(&never)
            .await
            .expect_err("an unknown entity must not read as an empty page");
        match &err {
            MemoryError::UnknownEntity { attempted, .. } => {
                assert_eq!(attempted, &never.to_string());
            }
            other => panic!("expected UnknownEntity, got {other:?}"),
        }

        // A typo'd handle explains itself: the miss carries its neighbour.
        let real = EntityId::person("contract-orient");
        ensure(store, &real).await;
        let typo = EntityId::person("contract-orjent");
        let err = store
            .recall(&typo)
            .await
            .expect_err("a typo'd handle is a miss");
        match &err {
            MemoryError::UnknownEntity { nearest, .. } => {
                assert!(
                    nearest.iter().any(|m| m.handle == real),
                    "the near candidate must surface: {err:?}"
                );
            }
            other => panic!("expected UnknownEntity, got {other:?}"),
        }

        // Exists-but-empty is the OTHER case, and it still reads fine.
        let facts = store
            .recall(&real)
            .await
            .expect("an existing entity's empty page reads");
        assert!(
            facts.is_empty(),
            "no facts were created along the way: {facts:?}"
        );
    }

    // --- the entity model ----------------------------------------------------

    /// A fact can be about any of the nine kinds, not just people — and each
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

    // --- the tree ------------------------------------------------------------

    /// **An entity can name a parent, and it survives the read path.** A root
    /// names none, which is what most entities are.
    pub async fn a_child_names_its_parent_and_reads_back<M: Memory>(store: &M) {
        let parent = EntityId::new(EntityKind::Project, "contract-monorail");
        let child = EntityId::new(EntityKind::Project, "contract-monorail-funding");

        let root = add(
            store,
            NewEntity::new(parent.clone(), "Contract Monorail", "contract-fixture"),
        )
        .await;
        assert_eq!(root.parent, None, "an entity under nothing is a root");

        let added = add(
            store,
            NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(child.clone(), "Monorail Funding", "contract-fixture")
            },
        )
        .await;
        assert_eq!(added.parent.as_ref(), Some(&parent));

        let seen = read_entity(store, &child).await;
        assert_eq!(
            seen, added,
            "the listed child must be byte-identical, parent included"
        );
        assert_eq!(
            read_entity(store, &parent).await.parent,
            None,
            "…and the parent is still a root"
        );
    }

    /// **Children come back as handles, one level down.** Zooming is the whole
    /// point: a parent read hands back the branch names and nothing else, so
    /// the caller pays only for the branch it descends into. A grandchild is
    /// not a child, and a leaf has none.
    pub async fn children_are_handles_and_one_level_deep<M: Memory>(store: &M) {
        let root = EntityId::new(EntityKind::Project, "contract-springfield");
        let track = EntityId::new(EntityKind::Project, "contract-springfield-track");
        let cars = EntityId::new(EntityKind::Project, "contract-springfield-cars");
        let brakes = EntityId::new(EntityKind::Project, "contract-springfield-brakes");

        add(
            store,
            NewEntity::new(root.clone(), "Contract Springfield", "contract-fixture"),
        )
        .await;
        for (id, name, under) in [
            (&track, "Springfield Track", &root),
            (&cars, "Springfield Cars", &root),
            (&brakes, "Springfield Brakes", &cars),
        ] {
            add(
                store,
                NewEntity {
                    parent: Some(under.clone()),
                    ..NewEntity::new(id.clone(), name, "contract-fixture")
                },
            )
            .await;
        }

        let mut got = store
            .children(&root)
            .await
            .expect("children should succeed");
        got.sort();
        let mut want = vec![track.clone(), cars.clone()];
        want.sort();
        assert_eq!(
            got, want,
            "exactly the direct children — the grandchild is the next level's business"
        );
        assert_eq!(
            store
                .children(&cars)
                .await
                .expect("children should succeed"),
            vec![brakes.clone()],
            "the middle of the tree has children of its own"
        );
        assert_eq!(
            store
                .children(&brakes)
                .await
                .expect("children should succeed"),
            Vec::<EntityId>::new(),
            "a leaf has none, and says so with an empty list"
        );
    }

    /// **Editing a child does not orphan it.** Every write that rewrites a
    /// whole document is a chance to drop a field nobody was thinking about,
    /// and parentage is the newest and least-remembered one. A rename and a
    /// prose replacement both go through here, because both rebuild the page
    /// around the part they came to change.
    pub async fn a_write_that_rewrites_a_child_leaves_it_where_it_was<M: Memory>(store: &M) {
        let parent = EntityId::new(EntityKind::Project, "contract-kwik-e");
        let child = EntityId::new(EntityKind::Project, "contract-kwik-e-squishee");
        add(
            store,
            NewEntity::new(parent.clone(), "Contract Kwik-E", "contract-fixture"),
        )
        .await;
        add(
            store,
            NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(child.clone(), "Squishee Machine", "contract-fixture")
            },
        )
        .await;

        let renamed = store
            .update_entity(
                &child,
                EntityPatch {
                    name: Some("Squishee Machine (the second one)".into()),
                    crm: Some("card:552".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update_entity should succeed")
            .written()
            .expect("the guard must not block an uncontested rename");
        assert_eq!(
            renamed.parent.as_ref(),
            Some(&parent),
            "a metadata edit rebuilds the record; the parent must survive it"
        );

        store
            .set_prose(&child, "The machine, and what it is for.")
            .await
            .expect("set_prose should succeed");
        assert_eq!(
            read_entity(store, &child).await.parent.as_ref(),
            Some(&parent),
            "…and so must a prose write, which rebuilds the page around it"
        );
        assert_eq!(
            store
                .children(&parent)
                .await
                .expect("children should succeed"),
            vec![child.clone()],
            "the parent still has it, which is the only place that would show"
        );
    }

    /// **A malformed parent is malformed, not merely unresolvable.** The two
    /// refusals have different shapes and a caller branches on them
    /// differently: a handle that is not a handle comes back an error, and one
    /// that is a handle but resolves to nothing comes back blocked with
    /// candidates. Collapsing the first into the second would answer "I don't
    /// know that one" to a caller whose real problem is that it never wrote a
    /// handle at all.
    pub async fn a_parent_that_is_not_a_handle_is_refused_before_the_guard<M: Memory>(store: &M) {
        let child = EntityId::new(EntityKind::Project, "contract-bad-parent");
        for bad in ["Some Project", "person:", "notakind:atlas", "person:Alpha"] {
            let err = store
                .add_entity(NewEntity {
                    parent: Some(EntityId(bad.into())),
                    ..NewEntity::new(child.clone(), "Contract Bad Parent", "contract-fixture")
                })
                .await
                .expect_err("a parent that is not a handle is malformed, not a candidate search");
            assert!(
                matches!(err, MemoryError::InvalidSubject(_)),
                "{bad:?} must be refused as a malformed id, got {err:?}"
            );
        }
        assert!(
            !store
                .list_entities(None)
                .await
                .expect("list_entities should succeed")
                .iter()
                .any(|e| e.id == child),
            "a refused write creates nothing"
        );
    }

    /// **A miss is a miss, not a leaf.** Asking for the children of something
    /// that does not exist is an error carrying candidates — never an empty
    /// list, which a caller would read as "this thing has nothing under it".
    pub async fn children_of_an_unknown_entity_is_a_miss<M: Memory>(store: &M) {
        let known = EntityId::new(EntityKind::Project, "contract-ghost-parent");
        add(
            store,
            NewEntity::new(known.clone(), "Contract Ghost Parent", "contract-fixture"),
        )
        .await;

        let typo = EntityId::new(EntityKind::Project, "contract-ghost-parnt");
        let err = store
            .children(&typo)
            .await
            .expect_err("an unknown parent must not read as childless");
        let MemoryError::UnknownEntity { nearest, .. } = &err else {
            panic!("an unknown parent is an unknown entity, got {err:?}");
        };
        assert!(
            nearest.iter().any(|m| m.handle == known),
            "the miss names what it might have meant: {nearest:?}"
        );
    }

    /// **A parent must already exist.** Creating an entity under a handle
    /// nothing resolves is blocked with candidates — and blocked means nothing
    /// is written: not the child, and certainly not the parent it named.
    /// Creation is an intentional act; naming a thing is not creating it.
    pub async fn an_unnamed_parent_is_refused_and_provisions_nothing<M: Memory>(store: &M) {
        let real = EntityId::new(EntityKind::Project, "contract-plant");
        let typo = EntityId::new(EntityKind::Project, "contract-plnt");
        let child = EntityId::new(EntityKind::Project, "contract-plant-shift");
        add(
            store,
            NewEntity::new(real.clone(), "Contract Plant", "contract-fixture"),
        )
        .await;

        let blocked = store
            .add_entity(NewEntity {
                parent: Some(typo.clone()),
                // The same signal that clears a name collision must not
                // conjure a parent: this is a write that NAMES an entity.
                create_new: true,
                ..NewEntity::new(child.clone(), "Plant Shift", "contract-fixture")
            })
            .await
            .expect("an unresolvable parent is an answer, not a failure");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = blocked
        else {
            panic!("a parent that does not exist must block");
        };
        assert_eq!(
            attempted, typo,
            "the block is about the parent, so that is the handle it reports"
        );
        assert!(
            candidates.iter().any(|c| c.handle == real),
            "the answer names what it might have meant: {candidates:?}"
        );

        let known = store
            .list_entities(None)
            .await
            .expect("list_entities should succeed");
        assert!(
            !known.iter().any(|e| e.id == child || e.id == typo),
            "a blocked write creates neither the child nor the parent it named"
        );
    }

    /// **Nothing is its own parent.** Refused in the house shape — a blocked
    /// result naming the offender, not a bare error — and never overridable,
    /// because there is no honest "I checked, they're different" answer when
    /// both handles are the same one.
    pub async fn nothing_may_be_its_own_parent<M: Memory>(store: &M) {
        let ouroboros = EntityId::new(EntityKind::Project, "contract-ouroboros");
        let blocked = store
            .add_entity(NewEntity {
                parent: Some(ouroboros.clone()),
                create_new: true,
                ..NewEntity::new(ouroboros.clone(), "Contract Ouroboros", "contract-fixture")
            })
            .await
            .expect("a self-parenting write is an answer, not a failure");
        let Guarded::Blocked { candidates, .. } = blocked else {
            panic!("an entity naming itself as its parent must block");
        };
        assert!(
            candidates
                .iter()
                .any(|c| c.handle == ouroboros && c.reason == guard::MatchReason::SelfParent),
            "the answer says WHICH refusal this is, or it reads as an unknown handle: {candidates:?}"
        );
        assert!(
            !store
                .list_entities(None)
                .await
                .expect("list_entities should succeed")
                .iter()
                .any(|e| e.id == ouroboros),
            "a blocked write writes nothing at all"
        );
    }

    pub async fn prose_is_replaced_whole_and_reads_back<M: Memory>(store: &M) {
        let bot = EntityId::new(EntityKind::Bot, "contract-epsilon");
        add(
            store,
            NewEntity::new(bot.clone(), "Contract Epsilon", "contract-fixture"),
        )
        .await;
        let fact = capture(
            store,
            NewFact::about(bot.clone(), "answers before noon", date(2026, 7, 25)),
        )
        .await;

        let charter = "Keeps the schedule.\n\nHard line: never writes to the ledger.";
        let stored = store.set_prose(&bot, charter).await.expect("set_prose ok");
        assert_eq!(stored, charter, "the verb returns what a read will return");
        let scanned = store
            .scan_entity(&bot)
            .await
            .expect("scan_entity ok")
            .expect("an entity that exists has a doc");
        assert_eq!(scanned.prose, charter, "the read path returns it");

        // Replaced, never appended: a charter is what is so now, not a trail.
        let rewritten = "Keeps the schedule. Nothing else.";
        store
            .set_prose(&bot, rewritten)
            .await
            .expect("set_prose ok");
        let scanned = store
            .scan_entity(&bot)
            .await
            .expect("scan_entity ok")
            .expect("an entity that exists has a doc");
        assert_eq!(scanned.prose, rewritten);
        assert!(
            !scanned.prose.contains("ledger"),
            "the old prose is gone, not buried under the new: {}",
            scanned.prose
        );

        // …and the facts sharing the page are untouched by any of it.
        let facts = store.recall(&bot).await.expect("recall ok");
        assert!(
            facts
                .iter()
                .any(|f| f.id == fact.id && f.content == "answers before noon"),
            "a prose write must not disturb the facts beside it: {facts:?}"
        );

        // An empty charter is not a charter.
        assert!(
            store.set_prose(&bot, "   ").await.is_err(),
            "blank prose is refused rather than silently clearing the page"
        );

        // **And prose that would forge a document's own structure is refused by
        // EVERY store, not only the one that would be corrupted by it.** A
        // charter carrying the fact-table header moves the boundary between
        // prose and facts, and every fact below it stops being read as a fact.
        // Held here rather than in one adapter's own tests because a fake that
        // waves it through is how a green suite ships a store-corrupting write.
        for forged in [
            format!("a charter\n\n{FACTS_HEADER}\n\n| id | subject |"),
            FACTS_HEADER.to_string(),
            format!("   {FACTS_HEADER}   "),
        ] {
            let err = store
                .set_prose(&bot, &forged)
                .await
                .expect_err("prose carrying a reserved line must be refused");
            assert!(
                matches!(err, MemoryError::InvalidEntity(_)),
                "expected a refusal naming the prose, got {err:?} for {forged:?}"
            );
        }
        // …and the charter that was there is untouched by any refusal.
        let scanned = store
            .scan_entity(&bot)
            .await
            .expect("scan_entity ok")
            .expect("an entity that exists has a doc");
        assert_eq!(scanned.prose, rewritten, "a refused write changes nothing");

        // The words on their own, not on a line of their own, are just words.
        let mentioning = "the facts table is at the bottom of this page";
        assert_eq!(
            store
                .set_prose(&bot, mentioning)
                .await
                .expect("ordinary prose"),
            mentioning,
            "a sentence that merely mentions facts is prose"
        );

        // And a handle that names nothing is a miss — never a doc conjured to
        // hold the prose, the same rule every other verb here follows.
        let ghost = EntityId::new(EntityKind::Bot, "contract-ghost-bot");
        let err = store
            .set_prose(&ghost, "a charter for nobody")
            .await
            .expect_err("an unknown entity must miss");
        assert!(
            matches!(err, MemoryError::UnknownEntity { .. }),
            "expected an entity miss, got {err:?}"
        );
        assert!(
            !store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .any(|e| e.id == ghost),
            "nothing was created along the way"
        );
    }

    /// `list_entities(kind)` narrows to one kind and never leaks another's.
    pub async fn list_entities_filters_by_kind<M: Memory>(store: &M) {
        let place = EntityId::new(EntityKind::Place, "contract-north-trail");
        let topic = EntityId::new(EntityKind::Topic, "contract-widgets");
        add(
            store,
            NewEntity::new(place.clone(), "North Trail", "user-named"),
        )
        .await;
        add(
            store,
            NewEntity::new(topic.clone(), "Widgets", "user-named"),
        )
        .await;

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
        assert_eq!(
            updated.source, "user-named",
            "an omitted field is left alone"
        );

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
        add(
            store,
            NewEntity::new(first.clone(), "Renamed Onto", "user-named"),
        )
        .await;
        add(
            store,
            NewEntity::new(second.clone(), "Renamer", "user-named"),
        )
        .await;

        let outcome = store
            .update_entity(
                &second,
                EntityPatch {
                    name: Some("Renamed Onto".into()),
                    ..Default::default()
                },
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
                EntityPatch {
                    name: Some("Quite Another Three".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update_entity should succeed")
            .written()
            .expect("a near-slug settled at creation must not block a later name edit");
        assert_eq!(renamed.name, "Quite Another Three");
    }

    /// Editing metadata that isn't the name is never screened — an entity's own
    /// name must not trip the guard against itself.
    ///
    /// An alias is a name: claiming one that another entity already answers
    /// to is the same collision a rename is, so it must face the same gate,
    /// even on a patch that renames nothing — otherwise search would index
    /// two entities answering to one word.
    pub async fn update_entity_screens_a_colliding_alias<M: Memory>(store: &M) {
        let owner = EntityId::person("contract-alias-owner");
        add(
            store,
            NewEntity::new(owner.clone(), "Contract Alias Owner", "user-named"),
        )
        .await;
        let borrower = EntityId::person("contract-alias-borrower");
        add(
            store,
            NewEntity::new(borrower.clone(), "Contract Alias Borrower", "user-named"),
        )
        .await;

        let outcome = store
            .update_entity(
                &borrower,
                EntityPatch {
                    aliases: Some(vec!["Contract Alias Owner".into()]),
                    ..Default::default()
                },
            )
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("an alias onto a name another entity wears must be blocked");
        };
        assert!(
            candidates.iter().any(|m| m.handle == owner),
            "the guard must name the entity already wearing it: {candidates:?}"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .filter(|e| e.id == borrower)
                .all(|e| e.aliases.is_empty()),
            "a blocked alias write lands nothing"
        );

        // The same explicit signal that clears a rename clears this: names are
        // not unique, and two entities may legitimately answer to one word.
        let forced = store
            .update_entity(
                &borrower,
                EntityPatch {
                    aliases: Some(vec!["Contract Alias Owner".into()]),
                    create_new: true,
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("an explicit create_new resolves the collision");
        assert_eq!(forced.aliases, vec!["Contract Alias Owner".to_string()]);
    }

    /// An alias the entity **already wears** is not a new claim, so re-sending it
    /// is not a collision with itself. Without this, every later patch to an
    /// entity's alias set comes back blocked by the entity's own name.
    pub async fn update_entity_is_not_blocked_by_its_own_labels<M: Memory>(store: &M) {
        let id = EntityId::new(EntityKind::Org, "contract-self-labelled");
        add(
            store,
            NewEntity {
                aliases: vec!["Contract Self Nickname".into()],
                ..NewEntity::new(id.clone(), "Contract Self-Labelled", "user-named")
            },
        )
        .await;

        let again = store
            .update_entity(
                &id,
                EntityPatch {
                    name: Some("Contract Self-Labelled".into()),
                    aliases: Some(vec!["Contract Self Nickname".into()]),
                    crm: Some("card:552".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("an entity is never a candidate for its own labels");
        assert_eq!(again.crm.as_deref(), Some("card:552"));
        assert_eq!(again.aliases, vec!["Contract Self Nickname".to_string()]);
    }

    /// **Only a change of LABEL is screened.** source, crm and boot say nothing
    /// about what a thing is called, so they can introduce no collision — and a
    /// gate that fired on them would make an already-settled duplicate name
    /// permanently uneditable in every other field.
    pub async fn update_entity_without_a_rename_is_not_screened<M: Memory>(store: &M) {
        let first = EntityId::new(EntityKind::Org, "contract-unscreened");
        add(
            store,
            NewEntity::new(first.clone(), "Unscreened Org", "user-named"),
        )
        .await;
        // A second entity that legitimately shares the name — settled once, at
        // creation, with the explicit signal. That settlement must not be
        // re-litigated by a patch that touches no label at all.
        let twin = EntityId::new(EntityKind::Org, "contract-unscreened-twin");
        add(
            store,
            NewEntity {
                create_new: true,
                ..NewEntity::new(twin.clone(), "Unscreened Org", "user-named")
            },
        )
        .await;

        let metadata_only = store
            .update_entity(
                &twin,
                EntityPatch {
                    source: Some("crm-card".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("a patch that renames nothing is not a rename");
        assert_eq!(metadata_only.source, "crm-card");
        assert_eq!(
            metadata_only.name, "Unscreened Org",
            "and it left the name alone"
        );
    }

    /// Updating an entity that doesn't exist errors with the nearest candidates
    /// — it never quietly creates one.
    pub async fn update_entity_unknown_handle_never_creates<M: Memory>(store: &M) {
        let ghost = EntityId::new(EntityKind::Thing, "contract-red-bikee");
        let err = store
            .update_entity(
                &ghost,
                EntityPatch {
                    name: Some("nope".into()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("an unknown handle must error");
        let MemoryError::UnknownEntity { nearest, .. } = &err else {
            panic!("expected UnknownEntity, got {err:?}");
        };
        assert!(
            nearest
                .iter()
                .any(|m| m.handle.slug() == "contract-red-bike"),
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
        assert_eq!(
            address.home, subject,
            "a fact's home is the doc it lives in"
        );
        assert_eq!(
            FactAddress::parse(&address.to_string()).expect("the address must round-trip"),
            address
        );

        let updated = edit(
            store,
            &address,
            FactPatch {
                content: Some("addressed and edited".into()),
                ..Default::default()
            },
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
            NewFact::about(
                subject.clone(),
                "a close contact of the user",
                date(2026, 7, 1),
            ),
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
        assert_eq!(
            refuted.id, captured.id,
            "the row is rewritten, not replaced"
        );

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen.status,
            FactStatus::Active,
            "the negative truth is the truth"
        );
        assert!(seen.content.starts_with("NOT a close contact"));

        let facts = store.recall(&subject).await.expect("recall");
        assert!(
            !facts
                .iter()
                .any(|f| f.content == "a close contact of the user"),
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
                FactPatch {
                    provenance: Some(Provenance::Testimony),
                    ..Default::default()
                },
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

    /// **A hedge round-trips as itself.** The claim the second field exists
    /// for: the operator says something and says they are not sure of it, so
    /// `testimony` (they back it) and `open` (they are not sure) are both true
    /// and both stored. Before this there was one field for two questions and a
    /// session had to pick which one to be wrong about.
    pub async fn a_hedged_claim_round_trips<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-hedged-word");
        let captured = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                standing: Some(Standing::Open),
                ..NewFact::about(
                    subject.clone(),
                    "thinks the shop shuts early",
                    date(2026, 7, 1),
                )
            },
        )
        .await;
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.standing, Standing::Open);
        assert_eq!(read_back(store, &subject, &captured.id).await, captured);
    }

    /// **A silent standing follows the provenance**, which is what every claim
    /// written before this field meant — so only a hedge has to be asked for,
    /// and no existing row changed meaning when the column arrived.
    pub async fn standing_defaults_to_what_the_provenance_implies<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-silent-standing");
        let said = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                ..NewFact::about(subject.clone(), "opens at seven", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(
            said.standing,
            Standing::Settled,
            "the operator's word is settled unless they hedge it"
        );

        let guessed = capture(
            store,
            NewFact::about(
                subject.clone(),
                "probably busy on Fridays",
                date(2026, 7, 2),
            ),
        )
        .await;
        assert_eq!(
            guessed.standing,
            Standing::Open,
            "a claim nobody confirmed is open"
        );

        // Both survive storage, not just the values the domain computed.
        assert_eq!(
            read_back(store, &subject, &said.id).await.standing,
            Standing::Settled
        );
        assert_eq!(
            read_back(store, &subject, &guessed.id).await.standing,
            Standing::Open
        );
    }

    /// **A capture declares its own standing**, on honour, exactly as it
    /// declares its provenance. A derived claim the operator has since
    /// confirmed is an ordinary row: the axes are independent, and the gate is
    /// on settling an open claim rather than on the pairing.
    pub async fn a_capture_declares_its_own_standing<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-unbacked-guess");
        ensure(store, &subject).await;
        let settled = capture(
            store,
            NewFact {
                provenance: Provenance::Inference,
                standing: Some(Standing::Settled),
                ..NewFact::about(subject.clone(), "certainly shuts at nine", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(settled.provenance, Provenance::Inference);
        assert_eq!(settled.standing, Standing::Settled);

        // Paired with the default it did not take: silence still resolves from
        // the provenance, so declaring a standing is a choice rather than the
        // only way to get one.
        let open = capture(
            store,
            NewFact::about(subject.clone(), "certainly shuts at nine", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(open.standing, Standing::Open);
    }

    /// **Settling is the user's move, and it leaves provenance alone.**
    ///
    /// This is the half the story found by accident: promotion used to "work"
    /// only because a hedge had been mis-stored as inference, so there was a
    /// provenance to promote. Stored honestly, the claim is testimony from the
    /// start — and there must still be something for confirmation to close.
    pub async fn settling_a_hedge_needs_confirmation_and_keeps_its_provenance<M: Memory>(
        store: &M,
    ) {
        let subject = EntityId::person("contract-settle-gate");
        let hedged = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                standing: Some(Standing::Open),
                ..NewFact::about(subject.clone(), "thinks it shuts early", date(2026, 7, 1))
            },
        )
        .await;

        let err = store
            .update_fact(
                &hedged.address(),
                FactPatch {
                    standing: Some(Standing::Settled),
                    ..Default::default()
                },
            )
            .await
            .expect_err("an unconfirmed settling must be refused");
        assert!(
            matches!(err, MemoryError::UnconfirmedSettling),
            "expected UnconfirmedSettling, got {err:?}"
        );
        assert_eq!(
            read_back(store, &subject, &hedged.id).await.standing,
            Standing::Open,
            "a refused settling must leave the claim untouched"
        );

        let settled = edit(
            store,
            &hedged.address(),
            FactPatch {
                standing: Some(Standing::Settled),
                confirmed_by_user: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(settled.standing, Standing::Settled);
        assert_eq!(
            settled.provenance,
            Provenance::Testimony,
            "confirmation closes the hedge; it does not restate who backed the claim"
        );
        let seen = read_back(store, &subject, &hedged.id).await;
        assert_eq!(seen.standing, Standing::Settled);
        assert_eq!(seen.provenance, Provenance::Testimony);
    }

    /// **Reopening is free**, exactly as demotion to inference is free. Nothing
    /// is risked by a claim admitting it might be wrong.
    pub async fn reopening_a_settled_claim_needs_no_ceremony<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-reopening");
        let settled = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                ..NewFact::about(subject.clone(), "opens at seven", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(settled.standing, Standing::Settled);

        let reopened = edit(
            store,
            &settled.address(),
            FactPatch {
                standing: Some(Standing::Open),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(reopened.standing, Standing::Open);
        assert_eq!(
            read_back(store, &subject, &settled.id).await.standing,
            Standing::Open
        );
    }

    /// **One axis moves at a time.** A patch names what it changes: promoting a
    /// guess says who backs it now and nothing about how sure anyone is, so a
    /// caller who means both says both. The alternative is the coupling the
    /// second field exists to remove — move one and the other follows.
    pub async fn a_patch_moves_only_the_axis_it_names<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-confirmed-guess");
        let guess = capture(
            store,
            NewFact::about(subject.clone(), "probably shuts at nine", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(guess.provenance, Provenance::Inference);
        assert_eq!(guess.standing, Standing::Open);

        let promoted = edit(
            store,
            &guess.address(),
            FactPatch {
                provenance: Some(Provenance::Testimony),
                confirmed_by_user: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(promoted.provenance, Provenance::Testimony);
        assert_eq!(
            promoted.standing,
            Standing::Open,
            "a promotion that said nothing about standing moved it anyway"
        );
        assert_eq!(
            read_back(store, &subject, &guess.id).await.standing,
            Standing::Open,
            "…and it reached the store that way"
        );

        // Paired with the positive: naming both is what lands both, so the
        // assertion above is about the silence rather than about a claim that
        // cannot be settled at all.
        let settled = edit(
            store,
            &guess.address(),
            FactPatch {
                standing: Some(Standing::Settled),
                confirmed_by_user: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(settled.provenance, Provenance::Testimony);
        assert_eq!(settled.standing, Standing::Settled);

        // And the reverse silence too: demoting says nothing about standing.
        let demoted = edit(
            store,
            &guess.address(),
            FactPatch {
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(demoted.provenance, Provenance::Inference);
        assert_eq!(demoted.standing, Standing::Settled);
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
            FactPatch {
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
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
            .update_fact(
                &ghost,
                FactPatch {
                    content: Some("nope".into()),
                    ..Default::default()
                },
            )
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
        add(
            store,
            NewEntity::new(known.clone(), "Addressee", "user-named"),
        )
        .await;

        let nudge = || FactPatch {
            content: Some("nope".into()),
            ..Default::default()
        };

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
            .update_fact(
                &FactAddress::new(known.clone(), FactId("f1".into())),
                nudge(),
            )
            .await
            .expect_err("an address on an entity with no facts must error");
        let MemoryError::UnknownFact { nearest, .. } = &err else {
            panic!("a real entity with no rows is a fact miss, got {err:?}");
        };
        assert!(
            nearest.is_empty(),
            "there are no addresses to list: {nearest:?}"
        );
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
        assert!(
            nearest.contains(&real.address().to_string()),
            "got {nearest:?}"
        );
    }

    // --- structured edges at capture -----------------------------------------

    /// An edge is written atomically with its fact and comes back on the read
    /// path. This is what makes ask-across an edge walk instead of an AI reading
    /// prose, so it is bound by the same read-back invariant as the row itself.
    pub async fn capture_writes_an_edge_that_reads_back<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-edged");
        let edge = Edge::new(
            EdgeShape::Location,
            EntityId::new(EntityKind::Place, "contract-far-country"),
        );
        let captured = capture(
            store,
            NewFact {
                edge: Some(edge.clone()),
                event: None,
                ..NewFact::about(
                    subject.clone(),
                    "spending the winter away",
                    date(2026, 7, 1),
                )
            },
        )
        .await;
        assert_eq!(captured.edge.as_ref(), Some(&edge));

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen, captured,
            "the edge must survive read-back byte-identical"
        );
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
                    event: None,
                    ..NewFact::about(
                        subject.clone(),
                        format!("a {shape} claim"),
                        date(2026, 7, 1),
                    )
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

    /// Nothing was recorded for `subject`: either its page reads empty or the
    /// entity never came to exist at all — both prove a refused write did not
    /// land. (Recalling a nonexistent entity is a miss by contract, so a spec
    /// probing a subject it never ensured accepts the miss as its proof.)
    async fn assert_nothing_recorded<M: Memory>(store: &M, subject: &EntityId) {
        match store.recall(subject).await {
            Ok(facts) => assert!(facts.is_empty(), "nothing must be written: {facts:?}"),
            Err(MemoryError::UnknownEntity { .. }) => {}
            Err(other) => panic!("unexpected recall error: {other:?}"),
        }
    }

    /// An object of the wrong kind for its shape is refused outright, and the
    /// fact does not land either — the edge is part of the write, not a garnish.
    pub async fn a_wrong_kind_edge_object_is_refused<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-miskinded");
        let err = store
            .capture(NewFact {
                // A `location` must point at a place; this one points at a person.
                edge: Some(Edge::new(
                    EdgeShape::Location,
                    EntityId::person("contract-alpha"),
                )),
                ..NewFact::about(subject.clone(), "should never be stored", date(2026, 7, 1))
            })
            .await
            .expect_err("a wrong-kind edge object must be refused");
        assert!(matches!(err, MemoryError::InvalidEdge(_)), "got {err:?}");
        assert_nothing_recorded(store, &subject).await;
    }

    /// The **object is screened by the write guard exactly as a subject is.** A
    /// typo'd object is where ask-across quietly rots: the edge points at a node
    /// nobody else references, so the walk comes back empty and nothing looks
    /// wrong. It comes back as candidates instead, and nothing is written.
    pub async fn an_edge_object_is_screened_by_the_guard<M: Memory>(store: &M) {
        let object = EntityId::new(EntityKind::Place, "contract-riverbend");
        add(
            store,
            NewEntity::new(object.clone(), "Riverbend", "user-named"),
        )
        .await;

        let subject = EntityId::person("contract-edge-guarded");
        // The subject faces the gate too, so it is provisioned first: this spec
        // is about the object, and the guard reports the first handle it stops.
        add(
            store,
            NewEntity::new(subject.clone(), "Edge Guarded", "user-named"),
        )
        .await;

        let typo = EntityId::new(EntityKind::Place, "contract-riverbnd");
        let outcome = store
            .capture(NewFact {
                edge: Some(Edge::new(EdgeShape::Location, typo.clone())),
                event: None,
                ..NewFact::about(subject.clone(), "should not land yet", date(2026, 7, 1))
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("a near-miss edge object must be reported");
        };
        assert_eq!(attempted, typo, "the guard names the handle it stopped");
        assert!(
            candidates.iter().any(|m| m.handle == object),
            "the guard must name the place it suspects: {candidates:?}"
        );
        assert_nothing_recorded(store, &subject).await;

        // Confirming the existing object is the ordinary path out.
        let landed = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, object.clone())),
                event: None,
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
            FactPatch {
                edge: Some(edge.clone()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(updated.edge.as_ref(), Some(&edge));
        assert_eq!(
            read_back(store, &subject, &captured.id).await.edge.as_ref(),
            Some(&edge),
            "the attached edge must be on the read path"
        );
    }

    /// **An event survives capture through any store, payload and all.**
    ///
    /// Every other event spec in this workspace runs against something that
    /// holds the record in memory, so all of them can pass while the one store
    /// production actually writes to drops the payload on the floor. That is
    /// not hypothetical: it is what the Outline adapter did — it built its
    /// stored fact field by field and left the event at `None`, so a capture
    /// answered with a record it had not written and a restart read back an
    /// ordinary fact.
    ///
    /// **The read-back guard cannot catch this one**, which is why it needs a
    /// spec of its own. Read-back compares what came back against what the
    /// adapter *believed* it stored, and both halves were missing the payload
    /// in the same way — so a lossy write passed its own invariant. The
    /// comparison a dropped field cannot survive is against the CALLER's
    /// record, and this is the only place that comparison is made.
    pub async fn an_event_survives_capture<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-evented");
        let touched = EntityId::new(EntityKind::Place, "contract-kiln-yard");
        ensure(store, &touched).await;

        let recorded = Event {
            kind: "a-type-nobody-defined".into(),
            metadata: [
                ("mood".to_string(), "delighted".to_string()),
                // A key no build has ever heard of, because the promise is
                // that an unknown field is kept as written rather than that
                // known fields survive.
                (
                    "a-field-from-a-later-build".to_string(),
                    "and its value".to_string(),
                ),
                // **The punctuation battery, and it is not decoration.** A
                // markdown store rewrites markdown, so the payload's own
                // grammar is the thing most likely not to survive being
                // stored — and every character below was mangled by real
                // Outline at some point in this record's short life: a space
                // and an `=` because the grammar escaped them with a
                // backslash and the store re-serialized every backslash it
                // saw, and a `~` because the store INSERTED an escape of its
                // own in front of it. A fake that stores bytes verbatim finds
                // none of this, which is why it rides in the shared contract
                // rather than in an adapter's own tests.
                (
                    "punctuation".to_string(),
                    "a = b, c~d, <e> & \"f\" — 100% ünïcode".to_string(),
                ),
            ]
            .into_iter()
            .collect(),
            refs: vec![touched.clone()],
        };
        let captured = capture(
            store,
            NewFact {
                event: Some(recorded.clone()),
                ..NewFact::about(
                    subject.clone(),
                    "the kiln was finally lit",
                    date(2026, 7, 2),
                )
            },
        )
        .await;
        assert_eq!(
            captured.event.as_ref(),
            Some(&recorded),
            "capture answered with an event it did not store"
        );

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen, captured,
            "the event must survive read-back byte-identical"
        );
        assert_eq!(
            seen.event.as_ref(),
            Some(&recorded),
            "the payload is what makes this an event at all"
        );
        assert!(seen.is_event(), "…and the class filter has to see it");
    }

    /// **An event's refs name entities, so the guard screens them too.**
    ///
    /// The rule is not about edges, it is about naming: nothing a write
    /// mentions is brought into being — or waved through unrecognized — as a
    /// side effect of mentioning it. A store that screened the subject and the
    /// edge object but not the refs would make the open hatch the one door on
    /// this surface where naming a stranger was free, and the hatch is ungated
    /// on its TYPE precisely so that everything else about it stays strict.
    ///
    /// And it takes the whole write with it: an event is one write, so a ref
    /// that cannot be resolved leaves no half-recorded fact behind.
    pub async fn an_events_ref_is_screened_by_the_guard<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-ref-guarded");
        ensure(store, &subject).await;
        let stranger = EntityId::person("contract-nobody-created-this");

        let outcome = store
            .capture(NewFact {
                event: Some(Event {
                    refs: vec![stranger.clone()],
                    ..Event::of("a-thing-that-happened")
                }),
                ..NewFact::about(subject.clone(), "should not land", date(2026, 7, 2))
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { attempted, .. } = outcome else {
            panic!("a ref naming an entity nobody created must be blocked");
        };
        assert_eq!(attempted, stranger, "the guard names the handle it stopped");
        assert_nothing_recorded(store, &subject).await;
    }

    /// **A metadata key the record's own grammar reserves is refused, not
    /// silently eaten.**
    ///
    /// `type` and `ref` are the grammar's own tokens. A caller passing
    /// `type` as metadata rendered two type tokens and the second won, so the
    /// event's actual type was destroyed and the metadata key vanished with
    /// it.
    ///
    /// **The fake and the real store disagreed on this input**, which is why
    /// the spec belongs here: the real store renders and reparses, so the
    /// guard caught a mismatch and refused a legitimate write with an opaque
    /// error, while the fake holds the record in memory, never round-trips it,
    /// and accepted it uncorrupted. A row that did reach disk in that shape
    /// would be reparsed and re-rendered by table migration with nothing
    /// comparing wrote against read, baking the loss in permanently.
    pub async fn a_reserved_metadata_key_is_refused<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-reserved-key");
        ensure(store, &subject).await;

        for reserved in ["type", "ref"] {
            let outcome = store
                .capture(NewFact {
                    event: Some(Event {
                        metadata: [(reserved.to_string(), "something".to_string())]
                            .into_iter()
                            .collect(),
                        ..Event::of("an-appointment")
                    }),
                    ..NewFact::about(subject.clone(), "it happened", date(2026, 7, 3))
                })
                .await;
            assert!(
                matches!(outcome, Err(MemoryError::InvalidFact(_))),
                "a metadata key named {reserved:?} must be refused rather than \
                 silently destroying the event's type: {outcome:?}"
            );
        }

        // …and the ordinary keys beside them are untouched.
        let landed = capture(
            store,
            NewFact {
                event: Some(Event {
                    metadata: [("kind".to_string(), "a value".to_string())]
                        .into_iter()
                        .collect(),
                    ..Event::of("an-appointment")
                }),
                ..NewFact::about(subject.clone(), "it happened later", date(2026, 7, 4))
            },
        )
        .await;
        assert_eq!(
            landed
                .event
                .as_ref()
                .and_then(|e| e.metadata.get("kind"))
                .map(String::as_str),
            Some("a value"),
        );
    }

    /// **Retraction: the record stays, marked, and the reason lands beside
    /// it.** Nothing is removed — that is the no-delete rule, and it is what
    /// makes this different from every store where taking something back means
    /// losing the evidence that it was ever said.
    pub async fn retracting_an_event_marks_it_and_records_why<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-retracted");
        let event = capture(
            store,
            NewFact {
                event: Some(Event::of("an-appointment")),
                ..NewFact::about(subject.clone(), "moved to the 14th", date(2026, 7, 3))
            },
        )
        .await;

        let taken_back = store
            .retract(
                &event.address(),
                Some("it was rebooked twice"),
                date(2026, 7, 4),
            )
            .await
            .expect("retracting an event should succeed");

        // The row it names: same id, same words, same place.
        assert_eq!(taken_back.retracted.id, event.id);
        assert_eq!(taken_back.retracted.content, event.content);
        assert_eq!(taken_back.retracted.event, event.event);
        assert_eq!(
            taken_back.retracted.status,
            FactStatus::Retracted,
            "the row is marked rather than removed"
        );

        // And the account of why, as a record of its own.
        assert_eq!(taken_back.record.content, "it was rebooked twice");
        assert_eq!(taken_back.record.date, date(2026, 7, 4));
        assert_eq!(
            taken_back.record.event.as_ref().and_then(Event::retracts),
            Some(event.address().to_string().as_str()),
            "the retraction names what it takes back, or the two are not one story"
        );

        // Both are on the read path, which is what makes any of it durable.
        let seen = read_back(store, &subject, &event.id).await;
        assert_eq!(seen.status, FactStatus::Retracted);
        assert_eq!(
            seen.content, event.content,
            "the words are untouched: it is marked, not edited"
        );
        let account = read_back(store, &subject, &taken_back.record.id).await;
        assert_eq!(account, taken_back.record);
    }

    /// **A retraction with no reason still records the act, and says the
    /// reason is missing rather than inventing one.**
    ///
    /// The reason became optional when the requirement was cut, and a row
    /// still has to carry content — so the absent case writes a sentence
    /// either way. The risk it leaves behind is that the sentence is jojobot's
    /// and not a caller's: it must state only what happened and that nobody
    /// said why, because a plausible-sounding reason here would be
    /// indistinguishable later from one somebody actually gave.
    pub async fn a_retraction_needs_no_reason<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-unreasoned");
        let event = capture(
            store,
            NewFact {
                event: Some(Event::of("an-appointment")),
                ..NewFact::about(subject.clone(), "it happened", date(2026, 7, 3))
            },
        )
        .await;

        let taken_back = store
            .retract(&event.address(), None, date(2026, 7, 4))
            .await
            .expect("a retraction without a reason is still a retraction");

        // The act landed in full: the row is marked and the account is a real
        // record, linked, dated, and on the read path like any other.
        assert_eq!(taken_back.retracted.status, FactStatus::Retracted);
        assert_eq!(
            taken_back.record.event.as_ref().and_then(Event::retracts),
            Some(event.address().to_string().as_str()),
        );
        assert_eq!(
            read_back(store, &subject, &taken_back.record.id).await,
            taken_back.record,
        );

        // And the content says the reason is absent — it does not stand in for
        // one. Asserted as the two things a later reader needs to be able to
        // tell apart, rather than as an exact string nobody may reword.
        let content = taken_back.record.content.to_lowercase();
        assert!(
            content.contains("retracted"),
            "the record has to say what happened: {content:?}"
        );
        assert!(
            content.contains("no reason"),
            "…and that nobody gave a reason, rather than supplying one: {content:?}"
        );
    }

    /// **One-way, and the three ways of asking for the reverse are all
    /// refused.** Retracting twice, retracting the retraction, and editing the
    /// status back are the same wish wearing three faces — so no single one of
    /// them is the whole test.
    pub async fn a_retraction_is_one_way<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-oneway");
        let event = capture(
            store,
            NewFact {
                event: Some(Event::of("a-thing-that-happened")),
                ..NewFact::about(subject.clone(), "it happened", date(2026, 7, 3))
            },
        )
        .await;
        let taken_back = store
            .retract(
                &event.address(),
                Some("it did not, in fact"),
                date(2026, 7, 4),
            )
            .await
            .expect("the first retraction lands");

        let again = store
            .retract(&event.address(), Some("again"), date(2026, 7, 5))
            .await;
        assert!(
            matches!(again, Err(MemoryError::NotRetractable { .. })),
            "a second retraction must be refused: {again:?}"
        );

        let the_record = store
            .retract(&taken_back.record.address(), Some("undo"), date(2026, 7, 5))
            .await;
        assert!(
            matches!(the_record, Err(MemoryError::NotRetractable { .. })),
            "a retraction is not itself retractable: {the_record:?}"
        );

        // The edit path is the third face, and the one a caller reaches for
        // without meaning anything by it.
        let edited = store
            .update_fact(
                &event.address(),
                FactPatch {
                    status: Some(FactStatus::Active),
                    ..Default::default()
                },
            )
            .await;
        assert!(
            matches!(edited, Err(MemoryError::NotRetractable { .. })),
            "a retracted row is not editable back to active: {edited:?}"
        );
        assert_eq!(
            read_back(store, &subject, &event.id).await.status,
            FactStatus::Retracted,
            "and none of the three moved it"
        );
    }

    /// **A fact is fixed, not retracted** — the boundary the whole model rests
    /// on, and the refusal has to name the way forward or it is a wall.
    pub async fn an_ordinary_fact_is_not_retractable<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-not-an-event");
        let fact = capture(
            store,
            NewFact::about(subject.clone(), "plays the theremin", date(2026, 7, 3)),
        )
        .await;

        let refused = store
            .retract(&fact.address(), Some("turns out not"), date(2026, 7, 4))
            .await;
        let Err(MemoryError::NotRetractable { why, .. }) = refused else {
            panic!("a fact must not be retractable: {refused:?}");
        };
        assert!(
            why.contains("update_fact"),
            "the refusal must name what to do instead: {why}"
        );

        // Untouched, and still the current truth.
        let seen = read_back(store, &subject, &fact.id).await;
        assert_eq!(seen.status, FactStatus::Active);
        assert_eq!(seen.content, "plays the theremin");
    }

    /// An address naming nothing is the same miss an edit's is — never a new
    /// record, and never a silent success.
    pub async fn retracting_an_unknown_address_never_writes<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-retract-miss");
        ensure(store, &subject).await;
        let missed = FactAddress::new(subject.clone(), FactId("f404".into()));

        let refused = store
            .retract(&missed, Some("nothing here"), date(2026, 7, 4))
            .await;
        assert!(
            matches!(refused, Err(MemoryError::UnknownFact { .. })),
            "a missed address is a miss, not a create: {refused:?}"
        );
        assert!(
            !store
                .recall(&subject)
                .await
                .expect("recall should succeed")
                .iter()
                .any(|f| f.id == missed.local),
            "nothing was written at the address that missed"
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
            assert_eq!(
                candidates[0].source, "crm-card",
                "the caller decides on the source"
            );
        }

        let seen = read_entity(store, &id).await;
        assert_eq!(
            seen.name, "Alpha",
            "the blocked write must not have overwritten anything"
        );
    }

    /// A near-miss handle is reported, and the explicit create-new signal is
    /// what lets a genuinely different entity through.
    pub async fn add_entity_reports_a_near_miss_then_accepts_create_new<M: Memory>(store: &M) {
        let first = EntityId::new(EntityKind::Org, "contract-riverside");
        add(
            store,
            NewEntity::new(first.clone(), "Riverside", "user-named"),
        )
        .await;

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
            NewEntity {
                create_new: true,
                ..NewEntity::new(typo.clone(), "Riversid", "user-named")
            },
        )
        .await;
        assert_eq!(forced.id, typo);
    }

    /// Capture's subject must already exist. Not "must not look like
    /// something else" — must *be* something: letting a novel subject
    /// self-provision a nameless entity would turn every typo or
    /// plausible-looking AI handle into a permanent record nobody chose.
    /// There is no create-new escape on this path either: a genuinely new
    /// entity is `add_entity`, then the capture — two deliberate steps.
    pub async fn capture_requires_an_existing_subject<M: Memory>(store: &M) {
        let known = EntityId::person("contract-zenith");
        add(store, NewEntity::new(known.clone(), "Zenith", "user-named")).await;

        // A fact about an entity that exists: waved straight through, always —
        // otherwise every second fact about someone would need confirming.
        capture(
            store,
            NewFact::about(known.clone(), "likes long walks", date(2026, 7, 1)),
        )
        .await;

        // A near miss comes back with the candidate that explains it…
        let typo = EntityId::person("contract-zenit");
        let outcome = store
            .capture(NewFact::about(
                typo.clone(),
                "should not land",
                date(2026, 7, 1),
            ))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = outcome else {
            panic!("a near-miss subject must be reported, never provisioned");
        };
        assert!(
            candidates.iter().any(|m| m.handle == known),
            "got {candidates:?}"
        );

        // …and a handle nothing resembles blocks just the same, with nothing
        // to suggest.
        let stranger = EntityId::new(EntityKind::Work, "contract-first-mix");
        let outcome = store
            .capture(NewFact::about(
                stranger.clone(),
                "32 tracks",
                date(2026, 7, 1),
            ))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("an unknown subject must block even with no near match");
        };
        assert_eq!(attempted, stranger, "the guard names the handle it stopped");
        assert!(
            candidates.is_empty(),
            "nothing resembles it: {candidates:?}"
        );

        for blocked in [&typo, &stranger] {
            assert_nothing_recorded(store, blocked).await;
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
        add(
            store,
            NewEntity::new(stranger.clone(), "First Mix", "user-named"),
        )
        .await;
        let landed = capture(
            store,
            NewFact::about(stranger.clone(), "32 tracks", date(2026, 7, 1)),
        )
        .await;
        assert_eq!(landed.subject, stranger);
        assert_eq!(
            read_back(store, &stranger, &landed.id).await.content,
            "32 tracks"
        );
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
        add(
            store,
            NewEntity::new(subject.clone(), "Edge Stranger", "user-named"),
        )
        .await;

        let stranger = EntityId::new(EntityKind::Event, "contract-unheard-of-fest");
        let outcome = store
            .capture(NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
                event: None,
                ..NewFact::about(subject.clone(), "should not land", date(2026, 7, 1))
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("an unknown edge object must block even with no near match");
        };
        assert_eq!(attempted, stranger);
        assert!(
            candidates.is_empty(),
            "nothing resembles it: {candidates:?}"
        );
        assert_nothing_recorded(store, &subject).await;
        assert!(
            store
                .list_entities(None)
                .await
                .expect("list")
                .iter()
                .all(|e| e.id != stranger),
            "…and must not have provisioned the object either"
        );

        add(
            store,
            NewEntity::new(stranger.clone(), "Unheard-of Fest", "user-named"),
        )
        .await;
        let landed = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, stranger.clone())),
                event: None,
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
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("an edge object that names no entity must block the edit");
        };
        assert_eq!(attempted, stranger, "the guard names the handle it stopped");
        assert!(
            candidates.is_empty(),
            "nothing resembles it: {candidates:?}"
        );

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
        add(
            store,
            NewEntity::new(stranger.clone(), "Nowhere In Particular", "user-named"),
        )
        .await;
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
        assert_eq!(
            read_entity(store, &id).await,
            added,
            "…on the read path too"
        );

        // The set is replaced whole, and an omitted field is left alone.
        let renamed = store
            .update_entity(
                &id,
                EntityPatch {
                    source: Some("crm-card".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            renamed.aliases, added.aliases,
            "an omitted alias set is untouched"
        );

        let replaced = store
            .update_entity(
                &id,
                EntityPatch {
                    aliases: Some(vec!["Only This One".into()]),
                    ..Default::default()
                },
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
                EntityPatch {
                    aliases: Some(vec!["one, two".into()]),
                    ..Default::default()
                },
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
                Hit::Fact { fact, .. } => Some(fact),
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
            NewFact::about(
                subject.clone(),
                "keeps a zamboni in the garage",
                date(2026, 7, 1),
            ),
        )
        .await;

        let hits = found(store, SearchQuery::text("zamboni"));
        let addresses: Vec<String> = fact_hits(&hits)
            .iter()
            .map(|f| f.address().to_string())
            .collect();
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
        assert_eq!(
            found_fact.address().home,
            subject,
            "the address names its home doc"
        );
        assert_eq!(found_fact.address().local, found_fact.id);
    }

    /// A superseded fact is **out of a default search** — a claim the store has
    /// already moved past coming back as current truth is worse than no memory
    /// at all — and `status: superseded` is how it is reached deliberately, so
    /// nothing is destroyed, only demoted.
    ///
    /// This is the default-exclusion contract.
    /// **A retracted record is out of a default search, and reachable when
    /// asked for by name.** The same rule superseded lives under, for a
    /// different reason: superseded says a later claim replaced this one,
    /// retracted says it should not have been recorded — and neither is
    /// something a reader should be handed as current truth.
    ///
    /// The reachable half matters more here than it does for superseded.
    /// Nothing is deleted, so the record has to stay findable by somebody who
    /// goes looking; a mark that hid a record from every possible read would
    /// be a delete with extra steps.
    pub async fn search_excludes_a_retracted_record_by_default<S: Memory + Search>(store: &S) {
        let subject = EntityId::person("contract-search-retracted");
        let live = capture(
            store,
            NewFact {
                event: Some(Event::of("a-rehearsal")),
                ..NewFact::about(subject.clone(), "the quartet rehearsed", date(2026, 7, 1))
            },
        )
        .await;
        let taken_back = capture(
            store,
            NewFact {
                event: Some(Event::of("a-rehearsal")),
                ..NewFact::about(
                    subject.clone(),
                    "the quartet rehearsed twice",
                    date(2026, 7, 2),
                )
            },
        )
        .await;
        store
            .retract(
                &taken_back.address(),
                Some("it never happened"),
                date(2026, 7, 3),
            )
            .await
            .expect("retracting an event should succeed");

        let addresses = |hits: &[Hit]| -> Vec<String> {
            fact_hits(hits)
                .iter()
                .map(|f| f.address().to_string())
                .collect()
        };

        let default = found(store, SearchQuery::text("rehearsed"));
        let seen = addresses(&default);
        // Both halves, because "not in the results" on its own passes just as
        // well when the query matched nothing at all.
        assert!(
            seen.contains(&live.address().to_string()),
            "the record that still stands must be found: {default:?}"
        );
        assert!(
            !seen.contains(&taken_back.address().to_string()),
            "a retracted record must not come back as current: {default:?}"
        );

        let asked = found(
            store,
            SearchQuery {
                status: Some(FactStatus::Retracted),
                ..SearchQuery::text("rehearsed")
            },
        );
        assert!(
            addresses(&asked).contains(&taken_back.address().to_string()),
            "nothing was deleted, so asking for it by name finds it: {asked:?}"
        );
    }

    pub async fn search_excludes_superseded_by_default_and_lists_it_on_request<
        S: Memory + Search,
    >(
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
            NewFact::about(
                subject.clone(),
                "plays the theremin on Tuesdays",
                date(2026, 7, 2),
            ),
        )
        .await;
        edit(
            store,
            &retired.address(),
            FactPatch {
                status: Some(FactStatus::Superseded),
                ..Default::default()
            },
        )
        .await;

        let default = found(store, SearchQuery::text("theremin"));
        let addresses: Vec<String> = fact_hits(&default)
            .iter()
            .map(|f| f.address().to_string())
            .collect();
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
        let asked_addresses: Vec<String> = fact_hits(&asked)
            .iter()
            .map(|f| f.address().to_string())
            .collect();
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
            NewFact::about(
                talker.clone(),
                "keeps talking about contract-faraway",
                date(2026, 7, 3),
            ),
        )
        .await;
        // …and a place that is edged there but is not a person.
        let project = EntityId::new(EntityKind::Project, "contract-away-project");
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, far.clone())),
                event: None,
                ..NewFact::about(
                    project.clone(),
                    "runs out of contract-faraway",
                    date(2026, 7, 4),
                )
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
                event: None,
                ..NewFact::about(
                    EntityId::person("contract-conn-one"),
                    "went both nights",
                    date(2026, 7, 1),
                )
            },
        )
        .await;
        let about = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::About, fest.clone())),
                event: None,
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
                ..NewFact::about(
                    EntityId::person("contract-conn-two"),
                    "went to the other one",
                    date(2026, 7, 3),
                )
            },
        )
        .await;

        let hits = found(
            store,
            SearchQuery {
                edge: Some(EdgeFilter {
                    shape: None,
                    object: fest,
                }),
                ..Default::default()
            },
        );
        let addresses: Vec<String> = fact_hits(&hits)
            .iter()
            .map(|f| f.address().to_string())
            .collect();
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
        add(
            store,
            NewEntity::new(handle.clone(), "Pinnable Guild", "user-named"),
        )
        .await;
        // Facts that also match the query text, so the pin has something to beat.
        capture(
            store,
            NewFact::about(
                handle.clone(),
                "meets at the contract-pinnable-guild hall",
                date(2026, 7, 1),
            ),
        )
        .await;

        let hits = found(store, SearchQuery::text(handle.as_str()));
        assert!(
            matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == handle),
            "an exact handle query must return that entity first: {hits:?}"
        );
    }

    /// **No bare hits.** A fact hit names the entity it is about and the entity
    /// whose page it sits on — handle, kind AND display name — so a reader knows
    /// what came back without spending a call per handle to find out.
    ///
    /// The name is the part that cannot be derived: a handle carries its kind in
    /// its grammar, but `person:contract-orient` says nothing about who that is.
    pub async fn search_fact_hits_name_their_subject_and_home<S: Memory + Search>(store: &S) {
        let subject = EntityId::person("contract-orient-subject");
        add(
            store,
            NewEntity {
                aliases: vec!["Contract Compass".into()],
                ..NewEntity::new(subject.clone(), "Orienteering Otto", "user-named")
            },
        )
        .await;
        capture(
            store,
            NewFact::about(subject.clone(), "reads a map for fun", date(2026, 7, 1)),
        )
        .await;

        let hits = found(store, SearchQuery::text("map for fun"));
        let (fact_subject, fact_home) = hits
            .iter()
            .find_map(|h| match h {
                Hit::Fact {
                    fact,
                    subject: s,
                    home,
                } if fact.subject == subject => Some((s.clone(), home.clone())),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the captured fact must come back: {hits:?}"));

        assert_eq!(fact_subject.id, subject);
        assert_eq!(fact_subject.kind, Some(EntityKind::Person));
        assert_eq!(
            fact_subject.name.as_deref(),
            Some("Orienteering Otto"),
            "a hit that names only the handle is the bare hit this exists to kill"
        );
        // Every name it answers to, not only the preferred one: a search on the
        // nickname otherwise returns a row labelled with a name the asker did
        // not use and has no way to connect to the one they did.
        assert_eq!(
            fact_subject.aliases,
            vec!["Contract Compass".to_string()],
            "the nickname rides along with the hit that names them"
        );
        // A single-subject capture homes the row on its own subject, so the two
        // agree here. What matters is that home is *resolved*, not that it
        // differs — a reader has to be able to tell when it does.
        assert_eq!(fact_home.id, subject);
        assert_eq!(fact_home.name.as_deref(), Some("Orienteering Otto"));
        assert_eq!(fact_home.aliases, vec!["Contract Compass".to_string()]);
    }

    /// An entity hit arrives with **where it sits in the graph** — the edges its
    /// facts draw. Asking about someone and getting back only their name is the
    /// same bare answer as a fact with no subject: the surroundings are the part
    /// that makes the next question askable.
    pub async fn search_entity_hits_carry_their_edges<S: Memory + Search>(store: &S) {
        let handle = EntityId::new(EntityKind::Org, "contract-orient-guild");
        let hall = EntityId::new(EntityKind::Place, "contract-orient-hall");
        add(
            store,
            NewEntity::new(handle.clone(), "Orienting Guild", "user-named"),
        )
        .await;
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, hall.clone())),
                event: None,
                ..NewFact::about(
                    handle.clone(),
                    "meets on the first Sunday",
                    date(2026, 7, 1),
                )
            },
        )
        .await;

        let hits = found(store, SearchQuery::text(handle.as_str()));
        let edges = hits
            .iter()
            .find_map(|h| match h {
                Hit::Entity { entity, edges, .. } if entity.id == handle => Some(edges.clone()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the entity must come back: {hits:?}"));

        assert!(
            edges.contains(&Edge::new(EdgeShape::Location, hall)),
            "the entity's own edges ride along with it: {edges:?}"
        );
    }

    /// Capture a fact placing `who` at `place`, and return it.
    async fn capture_at<M: Memory>(store: &M, who: &str, place: &EntityId, on: Date) -> Fact {
        capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Location, place.clone())),
                event: None,
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
        search_excludes_a_retracted_record_by_default(store).await;
        search_answers_ask_across_by_kind_and_edge(store).await;
        search_by_edge_object_alone_finds_any_shape(store).await;
        search_pins_a_named_entity_first(store).await;
        search_fact_hits_name_their_subject_and_home(store).await;
        search_entity_hits_carry_their_edges(store).await;
    }

    /// Run the whole contract against one store.
    pub async fn run_all<M: Memory>(store: &M) {
        capture_reads_back(store).await;
        preserves_all_fields(store).await;
        pipe_in_content_round_trips(store).await;
        a_backslash_in_content_round_trips(store).await;
        both_provenances_survive(store).await;
        edge_whitespace_is_normalized(store).await;
        multiple_facts_all_recallable(store).await;
        subjects_are_isolated(store).await;
        malicious_subjects_are_rejected(store).await;
        recall_unknown_is_a_miss_not_an_empty_page(store).await;

        every_kind_holds_facts(store).await;

        a_child_names_its_parent_and_reads_back(store).await;
        children_are_handles_and_one_level_deep(store).await;
        a_write_that_rewrites_a_child_leaves_it_where_it_was(store).await;
        a_parent_that_is_not_a_handle_is_refused_before_the_guard(store).await;
        children_of_an_unknown_entity_is_a_miss(store).await;
        an_unnamed_parent_is_refused_and_provisions_nothing(store).await;
        nothing_may_be_its_own_parent(store).await;

        prose_is_replaced_whole_and_reads_back(store).await;
        add_entity_reads_back(store).await;
        list_entities_filters_by_kind(store).await;
        update_entity_edits_metadata_in_place(store).await;
        update_entity_screens_a_colliding_rename(store).await;
        update_entity_screens_a_colliding_alias(store).await;
        update_entity_is_not_blocked_by_its_own_labels(store).await;
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

        an_event_survives_capture(store).await;
        an_events_ref_is_screened_by_the_guard(store).await;
        a_reserved_metadata_key_is_refused(store).await;
        retracting_an_event_marks_it_and_records_why(store).await;
        a_retraction_is_one_way(store).await;
        a_retraction_needs_no_reason(store).await;
        an_ordinary_fact_is_not_retractable(store).await;
        retracting_an_unknown_address_never_writes(store).await;

        facts_carry_a_usable_address(store).await;
        update_fact_edits_in_place(store).await;
        a_refutation_is_an_ordinary_content_edit(store).await;
        promotion_to_testimony_needs_confirmation(store).await;
        demotion_to_inference_is_free(store).await;

        a_hedged_claim_round_trips(store).await;
        standing_defaults_to_what_the_provenance_implies(store).await;
        a_capture_declares_its_own_standing(store).await;
        settling_a_hedge_needs_confirmation_and_keeps_its_provenance(store).await;
        reopening_a_settled_claim_needs_no_ceremony(store).await;
        a_patch_moves_only_the_axis_it_names(store).await;
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
