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
    FieldWrite, Guarded, Memory, MemoryError, NewEntity, NewFact, Retraction, Standing,
    apply_entity_patch, apply_fact_patch,
    guard::{self, Decision},
    normalize_content, normalize_details, normalize_prose, retraction_of, screen_entity_patch,
    search, standing_of, validate_content, validate_details, validate_edge, validate_entity,
    validate_fields, validate_prose, validate_subject,
};

/// An in-memory [`Memory`] adapter for tests. Holds entities and facts in `Vec`s
/// behind `Mutex`es; mints fact ids `f1`, `f2`, … per home doc, mirroring the
/// real store's per-doc numbering. A fresh instance starts empty.
#[derive(Default)]
pub struct InMemoryMemory {
    entities: Mutex<Vec<Entity>>,
    /// The claims. **Their fields are not here**: a claim is stored with an
    /// empty bag and its fields are projected from [`InMemoryMemory::writes`]
    /// on every read, exactly as the real store projects them from its own
    /// rows. A fake that kept a second copy of the current value could pass a
    /// case the real store fails.
    facts: Mutex<Vec<Fact>>,
    /// **The field substrate: every write of every key, oldest first.** Nothing
    /// is ever removed from it — a clear is a write that carries no value.
    writes: Mutex<Vec<StoredWrite>>,
    /// The human half of each entity's doc, keyed by handle — replaced whole by
    /// `set_prose`, exactly as the real store replaces the region.
    prose: Mutex<std::collections::HashMap<EntityId, String>>,
    /// The type declarations, one per name. A `Vec` rather than a map because
    /// the real store keeps them as rows and the order they were declared in
    /// is part of what each one says.
    types: Mutex<Vec<super::types::DeclaredType>>,
}

impl InMemoryMemory {
    /// A new, empty fake.
    pub fn new() -> Self {
        Self::default()
    }

    /// Put an entity in the store without the write guard seeing it — **the
    /// only way to stage a record the port refuses to write.**
    ///
    /// A parent that names nothing is the case this exists for: `add_entity`
    /// blocks it, here and in the real store alike, so a record that holds one
    /// came from a hand edit outside jojobot rather than from a verb. A fixture
    /// for that state has to enter the same way the state does, and a flag on
    /// the guard would make the state writable, which is the opposite of true.
    ///
    /// **It is for standing up damage, never for convenience.** A fixture
    /// `add_entity` could have built is built with `add_entity`, however much
    /// tidier this looks: reaching for this because seeding through the port is
    /// tedious grows a second way to create entities that no rule governs.
    pub fn past_the_guard(&self, entity: Entity) {
        self.entities
            .lock()
            .expect("fake mutex poisoned")
            .push(entity);
    }

    /// The entity index the write guard screens against.
    fn index(&self) -> Vec<Entity> {
        self.entities.lock().expect("fake mutex poisoned").clone()
    }

    /// **Append what a write said about a record's keys**, each taking the next
    /// ordinal for its own (thing, key) — never for the record.
    ///
    /// A value of `None` is a clear: the key stops being current and the writes
    /// that put it there stay where they are.
    fn append_writes<I>(&self, home: &EntityId, fact: &FactId, wrote: I)
    where
        I: IntoIterator<Item = (String, Option<String>)>,
    {
        let mut writes = self.writes.lock().expect("fake mutex poisoned");
        for (key, value) in wrote {
            let ordinal = writes
                .iter()
                .filter(|w| &w.entity == home && w.key == key)
                .count() as u64
                + 1;
            writes.push(StoredWrite {
                entity: home.clone(),
                key,
                ordinal,
                value,
                fact: fact.clone(),
            });
        }
    }

    /// **The record as a reader sees it: its claim, with its fields projected.**
    ///
    /// One value per key — the newest write of that key made by THIS record —
    /// and a key whose newest write inside the record took it off is not there.
    fn projected(&self, fact: &Fact) -> Fact {
        let writes = self.writes.lock().expect("fake mutex poisoned");
        let mut mine: Vec<&StoredWrite> = writes
            .iter()
            .filter(|w| w.entity == fact.home && w.fact == fact.id)
            .collect();
        mine.sort_by_key(|w| w.ordinal);
        let mut fields = std::collections::BTreeMap::new();
        for write in mine {
            match &write.value {
                Some(value) => fields.insert(write.key.clone(), value.clone()),
                None => fields.remove(&write.key),
            };
        }
        Fact {
            fields,
            ..fact.clone()
        }
    }
}

/// One row of the fake's field substrate — the shape the real store keeps in a
/// table, kept here so the two cannot come to disagree about what a read
/// projects.
struct StoredWrite {
    /// The thing the key was written on. With [`StoredWrite::key`] it is the
    /// address a history is asked for.
    entity: EntityId,
    key: String,
    /// Which write of this key on this thing it is, counting from one.
    ordinal: u64,
    /// What it put there; `None` took the key off.
    value: Option<String>,
    /// The record that carried it.
    fact: FactId,
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
        if let Decision::Block(candidates) = guard::decide(
            &new.id,
            &new.labels(),
            &index,
            new.override_token.as_deref(),
        ) {
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
        validate_fields(&fact.fields)?;
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
        // **A record's refs are named entities like any other.** The rule is
        // not about edges, it is about naming: nothing a write mentions is
        // brought into being as a side effect of mentioning it. A ref that
        // provisioned its own entity would make the open hatch the one place on
        // the surface where that stopped being true.
        for object in &fact.refs {
            validate_subject(object)?;
            if let Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
                    candidates,
                });
            }
        }

        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        // **A claim this one is derived from is named, so it must already
        // exist** — the same rule the subject and the edge's object face, on
        // the one field that names a claim rather than an entity. A citation
        // to nothing fails the reader the field exists for: a later session
        // walking the provenance chain back.
        //
        // Answered in `retract`'s two shapes rather than a third of its own
        // (rule 51): an unknown home is an entity miss, and a home holding no
        // such row is a fact miss with the addresses that do exist.
        if let Some(source) = &fact.derived_from {
            if !index.iter().any(|e| e.id == source.home) {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            }
            if !facts
                .iter()
                .any(|f| f.home == source.home && f.id == source.local)
            {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    nearest: facts
                        .iter()
                        .filter(|f| f.home == source.home)
                        .map(|f| f.address().to_string())
                        .collect(),
                });
            }
        }
        let home = fact.subject.clone();
        let existing: Vec<&Fact> = facts.iter().filter(|f| f.home == home).collect();
        let id = FactId(format!("f{}", existing.len() + 1));
        let wrote: Vec<(String, Option<String>)> = fact
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
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
            fields: fact.fields,
            refs: fact.refs,
            derived_from: fact.derived_from,
        };
        // **The claim is kept without its fields and the fields are kept as
        // writes.** One body of data, projected on the way out.
        facts.push(Fact {
            fields: Default::default(),
            ..stored.clone()
        });
        self.append_writes(&stored.home, &stored.id, wrote);
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
        let mine: Vec<Fact> = facts
            .iter()
            .filter(|f| &f.subject == subject || &f.home == subject)
            .cloned()
            .collect();
        drop(facts);
        Ok(mine.iter().map(|f| self.projected(f)).collect())
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        let index = self.index();
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        // The record each write arrived in says when it happened and what
        // became of it, so the two are read together.
        let facts = self.facts.lock().expect("fake mutex poisoned").clone();
        let writes = self.writes.lock().expect("fake mutex poisoned");
        let mut mine: Vec<&StoredWrite> = writes
            .iter()
            .filter(|w| &w.entity == entity && w.key == key)
            .collect();
        mine.sort_by_key(|w| w.ordinal);
        Ok(mine
            .into_iter()
            .filter_map(|write| {
                let carried = facts
                    .iter()
                    .find(|f| f.home == write.entity && f.id == write.fact)?;
                Some(FieldWrite {
                    value: write.value.clone(),
                    fact: carried.address(),
                    date: carried.date,
                    status: carried.status,
                })
            })
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
        // **The patch is applied to the record as it reads now**, so a set that
        // replaces a value is validated against the value it replaces — and the
        // claim goes back without fields, because the fields are the writes.
        let mut edited = self.projected(fact);
        apply_fact_patch(&mut edited, &patch)?;
        *fact = Fact {
            fields: Default::default(),
            ..edited
        };
        let (home, id) = (fact.home.clone(), fact.id.clone());
        drop(facts);
        self.append_writes(&home, &id, super::writes_of(&patch));
        let facts = self.facts.lock().expect("fake mutex poisoned");
        let stored = facts
            .iter()
            .find(|f| f.home == home && f.id == id)
            .expect("the record was just edited in place");
        Ok(Guarded::Written(self.projected(stored)))
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
        // **Projected before it is judged.** What a record takes back is a key
        // it carries, so a target read without its fields would read as an
        // ordinary claim — and a retraction would become retractable.
        let target = self.projected(&target);
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
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
        };
        let retracted = Fact {
            status: FactStatus::Retracted,
            ..target
        };
        for fact in facts.iter_mut() {
            if fact.home == address.home && fact.id == address.local {
                *fact = Fact {
                    fields: Default::default(),
                    ..retracted.clone()
                };
            }
        }
        // The account is a new record like any other: its own keys — the one
        // naming what it takes back — are writes of its own.
        facts.push(Fact {
            fields: Default::default(),
            ..record.clone()
        });
        drop(facts);
        self.append_writes(
            &record.home,
            &record.id,
            record
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), Some(value.clone()))),
        );
        Ok(Retraction {
            retracted: self.projected(&retracted),
            record,
        })
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
                        .map(|f| self.projected(f))
                        .collect(),
                    entity: Some(entity),
                }
            }))
            .collect())
    }

    async fn declare_type(
        &self,
        declared: super::types::DeclaredType,
    ) -> Result<super::types::DeclaredType, MemoryError> {
        super::types::validate_type(&declared)?;
        let declared = declared.normalized();
        let mut held = self.types.lock().unwrap();
        super::types::guard_replacement(
            &declared,
            held.iter()
                .find(|t| t.name == declared.name)
                .map(|t| t.origin),
        )?;
        // Replaced whole, the way the real store replaces the rows sharing the
        // name: a type is the keys it names now.
        held.retain(|t| t.name != declared.name);
        held.push(declared.clone());
        Ok(declared)
    }

    async fn declared_types(&self) -> Result<Vec<super::types::DeclaredType>, MemoryError> {
        Ok(self.types.lock().unwrap().clone())
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
    use crate::memory::graph;
    use crate::memory::search::{EdgeFilter, Hit, Search, SearchQuery};
    use crate::memory::types::{DeclaredType, Field, Origin, ValueType};
    use crate::memory::{Boot, Edge, EdgeShape, FACTS_HEADER, FactStatus, Provenance};
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

    /// Add an entity the guard is expected to **refuse first** — the way a
    /// caller really gets one made: read the refusal, take the token it minted,
    /// come again with it.
    ///
    /// The token is read out of the answer rather than made up beside it. A
    /// setup line that mints its own token proves nothing about the store it is
    /// setting up, and would pass on one that accepts any string.
    async fn add_over_the_screen<M: Memory>(store: &M, new: NewEntity) -> Entity {
        let id = new.id.clone();
        let Guarded::Blocked {
            attempted,
            candidates,
        } = store
            .add_entity(new.clone())
            .await
            .expect("add_entity should succeed")
        else {
            panic!("{id} was expected to hit the near-miss screen and did not");
        };
        store
            .add_entity(NewEntity {
                override_token: Some(guard::override_token(&attempted, &candidates)),
                ..new
            })
            .await
            .expect("add_entity should succeed")
            .written()
            .unwrap_or_else(|| panic!("the refusal's own token must let {id} through"))
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

    /// **A `derived_from` names a claim, and the claim has to be there.**
    ///
    /// The surface teaches that everything a write names must already exist.
    /// That was true of entities — a subject, an edge's object, an event's
    /// refs — and not of claims: this field took any well-formed address and
    /// nothing looked. A link to a claim that never existed is a citation to
    /// nothing, and the reader it fails is a later session following the
    /// provenance chain, which is the whole reason the field is there.
    ///
    /// Both halves, in one read. The refusal alone passes on a store whose
    /// capture is simply broken; the acceptance alone passes on the store that
    /// checked nothing.
    pub async fn derived_from_must_name_a_fact_that_exists<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-derived");
        let source = capture(
            store,
            NewFact::about(subject.clone(), "said the ferry moved", date(2026, 4, 1)),
        )
        .await;

        // Named and there: accepted, and the link reads back off the record.
        let derived = capture(
            store,
            NewFact {
                derived_from: Some(source.address()),
                ..NewFact::about(
                    subject.clone(),
                    "so the crossing is longer now",
                    date(2026, 4, 2),
                )
            },
        )
        .await;
        assert_eq!(
            read_back(store, &subject, &derived.id).await.derived_from,
            Some(source.address()),
            "a link to a claim that exists survives the write"
        );

        // Named and absent: refused, with the addresses that do exist.
        let missing = FactAddress::parse("person:contract-derived#f99").expect("well-formed");
        let refused = store
            .capture(NewFact {
                derived_from: Some(missing.clone()),
                ..NewFact::about(subject.clone(), "and the fare went up", date(2026, 4, 3))
            })
            .await;
        let Err(MemoryError::UnknownFact { attempted, nearest }) = &refused else {
            panic!("a derived_from naming no claim must be refused as a miss, got {refused:?}");
        };
        assert_eq!(attempted, &missing.to_string());
        assert!(
            nearest.contains(&source.address().to_string()),
            "the addresses that DO exist are what makes it repairable: {nearest:?}"
        );

        // A home nobody has heard of is an ENTITY miss, which is the shape
        // `retract` already answers with. Two shapes, no third: what is absent
        // differs, so what the caller does about it differs.
        let nowhere = FactAddress::parse("person:contract-nobody#f1").expect("well-formed");
        let refused = store
            .capture(NewFact {
                derived_from: Some(nowhere),
                ..NewFact::about(subject.clone(), "and the pier closed", date(2026, 4, 4))
            })
            .await;
        assert!(
            matches!(refused, Err(MemoryError::UnknownEntity { .. })),
            "a derived_from whose home is unknown is an entity miss, got {refused:?}"
        );

        // Nothing was written by either refusal. A guard that refuses and
        // writes anyway is the failure this whole rule exists to prevent
        // (rule 18).
        let facts = store.recall(&subject).await.expect("recall should succeed");
        assert_eq!(
            facts.len(),
            2,
            "a refused capture leaves the record as it was: {facts:?}"
        );
    }

    /// Every field survives capture→recall unchanged and byte-identical —
    /// `derived_from` included, since it is a fact field like any other and
    /// this is the one test that pins ALL of them at once.
    pub async fn preserves_all_fields<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-fields");
        // A claim that is really there: `derived_from` names one, and naming
        // one that does not exist is refused rather than stored.
        let source = capture(
            store,
            NewFact::about(subject.clone(), "mentioned the café", date(2026, 3, 8)),
        )
        .await
        .address();
        assert_eq!(
            source.to_string(),
            "person:contract-fields#f1",
            "the first claim on an entity is addressed f1, and that address is what a link carries"
        );
        let new = NewFact {
            subject: subject.clone(),
            content: "prefers a café table".into(),
            details: Some("mentioned it twice".into()),
            provenance: Provenance::Testimony,
            standing: Some(Standing::Open),
            status: FactStatus::Active,
            date: date(2026, 3, 9),
            edge: None,
            fields: [("seats".to_string(), "2".to_string())]
                .into_iter()
                .collect(),
            refs: vec![subject.clone()],
            derived_from: Some(source.clone()),
        };
        let captured = capture(store, new).await;
        assert_eq!(captured.subject, subject);
        assert_eq!(captured.content, "prefers a café table");
        assert_eq!(captured.details.as_deref(), Some("mentioned it twice"));
        assert_eq!(captured.provenance, Provenance::Testimony);
        assert_eq!(captured.standing, Standing::Open);
        assert_eq!(captured.date, date(2026, 3, 9));
        assert_eq!(captured.fields.get("seats").map(String::as_str), Some("2"));
        assert_eq!(captured.refs, vec![subject.clone()]);
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

    /// A fact can be about any of the ten kinds, not just people — and each
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

        // **A child named for its parent is a containment near miss**, and
        // rightly: `monorail-funding` beside `monorail` is exactly the shape
        // that is usually one thing filed twice. Here the two really are
        // different, so it goes over the screen the way a caller would say so —
        // read the refusal, hand its token back.
        let added = add_over_the_screen(
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
        // Every child's handle contains its parent's, which is the containment
        // near miss doing its job — a branch named for its trunk is the shape
        // that is usually one thing filed twice. These are deliberate, so each
        // goes over the screen with the token its own refusal minted.
        for (id, name, under) in [
            (&track, "Springfield Track", &root),
            (&cars, "Springfield Cars", &root),
            (&brakes, "Springfield Brakes", &cars),
        ] {
            add_over_the_screen(
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
        // A child named for its parent trips the containment screen; the two
        // are deliberately different, so it goes over with its own token.
        add_over_the_screen(
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
        // Named so it resembles neither the real parent nor the typo: this
        // case is about the PARENT gate, and a child that tripped the screen on
        // its own handle first would report a block about itself.
        let child = EntityId::new(EntityKind::Project, "contract-shift-rota");
        add(
            store,
            NewEntity::new(real.clone(), "Contract Plant", "contract-fixture"),
        )
        .await;

        let blocked = store
            .add_entity(NewEntity {
                parent: Some(typo.clone()),
                ..NewEntity::new(child.clone(), "Shift Rota", "contract-fixture")
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

        // **The token that clears a name collision must not conjure a parent.**
        // This is the token for exactly the refusal above — correctly derived,
        // not invented — and this gate still refuses it, because a write that
        // NAMES an entity has no override to offer.
        let held = guard::override_token(&attempted, &candidates);
        assert!(
            matches!(
                store
                    .add_entity(NewEntity {
                        parent: Some(typo.clone()),
                        override_token: Some(held),
                        ..NewEntity::new(child.clone(), "Shift Rota", "contract-fixture")
                    })
                    .await
                    .expect("an unresolvable parent is an answer, not a failure"),
                Guarded::Blocked { .. }
            ),
            "no token creates a parent: naming a thing is not creating it"
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
                ..NewEntity::new(ouroboros.clone(), "Contract Ouroboros", "contract-fixture")
            })
            .await
            .expect("a self-parenting write is an answer, not a failure");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = blocked
        else {
            panic!("an entity naming itself as its parent must block");
        };
        assert!(
            candidates
                .iter()
                .any(|c| c.handle == ouroboros && c.reason == guard::MatchReason::SelfParent),
            "the answer says WHICH refusal this is, or it reads as an unknown handle: {candidates:?}"
        );

        // The token derived from this very refusal, handed back. It changes
        // nothing, because there is no honest "I checked, they're different"
        // answer when both handles are the same one.
        let held = guard::override_token(&attempted, &candidates);
        assert!(
            matches!(
                store
                    .add_entity(NewEntity {
                        parent: Some(ouroboros.clone()),
                        override_token: Some(held),
                        ..NewEntity::new(
                            ouroboros.clone(),
                            "Contract Ouroboros",
                            "contract-fixture"
                        )
                    })
                    .await
                    .expect("a self-parenting write is an answer, not a failure"),
                Guarded::Blocked { .. }
            ),
            "self-parenting is never overridable"
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
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("a rename onto an existing name must be blocked");
        };
        assert!(
            candidates.iter().any(|m| m.handle == first),
            "the guard must name the entity already wearing it: {candidates:?}"
        );
        let token = guard::override_token(&attempted, &candidates);

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

        // A token nobody minted must not resolve it, or the mechanism is a
        // boolean with more ceremony.
        assert!(
            matches!(
                store
                    .update_entity(
                        &second,
                        EntityPatch {
                            name: Some("Renamed Onto".into()),
                            override_token: Some("0000000000000000".into()),
                            ..Default::default()
                        },
                    )
                    .await
                    .expect("the call itself succeeds; the guard answers in the result"),
                Guarded::Blocked { .. }
            ),
            "a token this refusal did not mint resolves nothing"
        );

        // The token this refusal minted clears a rename, exactly as it clears a
        // creation — one mechanism over both gates.
        let forced = store
            .update_entity(
                &second,
                EntityPatch {
                    name: Some("Renamed Onto".into()),
                    override_token: Some(token),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("the refusal's own token resolves the rename");
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
        // The near-slug the guard reported, judged different at creation — made
        // the way a caller really makes one, over the refusal's own token.
        add_over_the_screen(
            store,
            NewEntity::new(neighbour.clone(), "Quite Another Two", "user-named"),
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
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("an alias onto a name another entity wears must be blocked");
        };
        assert!(
            candidates.iter().any(|m| m.handle == owner),
            "the guard must name the entity already wearing it: {candidates:?}"
        );
        let token = guard::override_token(&attempted, &candidates);
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

        // The same mechanism that clears a rename clears this: names are not
        // unique, and two entities may legitimately answer to one word.
        let forced = store
            .update_entity(
                &borrower,
                EntityPatch {
                    aliases: Some(vec!["Contract Alias Owner".into()]),
                    override_token: Some(token),
                    ..Default::default()
                },
            )
            .await
            .expect("update should succeed")
            .written()
            .expect("the refusal's own token resolves the collision");
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
        // creation, over that refusal's own token. That settlement must not be
        // re-litigated by a patch that touches no label at all.
        let twin = EntityId::new(EntityKind::Org, "contract-unscreened-twin");
        add_over_the_screen(
            store,
            NewEntity::new(twin.clone(), "Unscreened Org", "user-named"),
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
                ..NewFact::about(subject.clone(), "now it lands", date(2026, 7, 1))
            },
        )
        .await;
        assert_eq!(landed.edge.map(|e| e.object), Some(object));
    }

    /// **`update_fact` sets and clears a field, and the store keeps both
    /// moves.**
    ///
    /// A contract case rather than a domain one, because the claim is about
    /// STORAGE: the patch is applied to a record in memory either way, and only
    /// a real store can say that a cleared key left the rows under the fact
    /// rather than lingering there to be read back on the next call.
    pub async fn update_fact_sets_and_clears_a_field<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-field-edit");
        let captured = capture(
            store,
            NewFact {
                fields: [
                    ("cost".to_string(), "40".to_string()),
                    ("done_on".to_string(), "2026-04-18".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(subject.clone(), "the annual service", date(2026, 7, 1))
            },
        )
        .await;

        let edited = edit(
            store,
            &captured.address(),
            FactPatch {
                fields: [("cost".to_string(), "45".to_string())]
                    .into_iter()
                    .collect(),
                clear_fields: vec!["done_on".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            edited.fields,
            [("cost".to_string(), "45".to_string())]
                .into_iter()
                .collect(),
            "the key named is rewritten and the key cleared is gone"
        );

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen.fields.get("cost").map(String::as_str),
            Some("45"),
            "the new value is on the read path"
        );
        assert!(
            !seen.fields.contains_key("done_on"),
            "…and the cleared key did not survive the store: {seen:?}"
        );
    }

    /// **A key written a hundred times holds one value and counts a hundred.**
    ///
    /// The two questions the substrate exists to answer with one body of data.
    /// Nobody wants the hundred sittings when they ask what the count is now;
    /// one time in a hundred they want every one of them, with its date and the
    /// record it arrived in.
    ///
    /// **The address is the thing and the key.** Every sitting here is a record
    /// of its own with an id of its own, so a history hanging off a record's id
    /// would return a hundred histories of length one — which is the same
    /// firehose the caller already had, and counts nothing.
    pub async fn a_key_written_many_times_holds_one_value_and_counts<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-counted");
        let key = "donuts_eaten";
        let written = 100;
        for nth in 1..=written {
            capture(
                store,
                NewFact {
                    fields: [(key.to_string(), nth.to_string())].into_iter().collect(),
                    ..NewFact::about(subject.clone(), format!("ate one, number {nth}"), {
                        date(2026, 7, 1)
                    })
                },
            )
            .await;
        }

        let facts = store.recall(&subject).await.expect("recall should succeed");
        assert_eq!(
            crate::memory::folded_fields(&facts)
                .get(key)
                .map(String::as_str),
            Some(written.to_string().as_str()),
            "the ordinary read is current truth: the newest write, and one value"
        );

        let history = store
            .history(&subject, key)
            .await
            .expect("history should succeed");
        assert_eq!(
            history.len(),
            written,
            "the count of the writes IS the answer to how many times"
        );
        assert_eq!(
            history.first().map(|w| w.value.as_deref()),
            Some(Some("1")),
            "oldest first: {:?}",
            history.first()
        );
        assert_eq!(
            history.last().map(|w| w.value.as_deref()),
            Some(Some(written.to_string().as_str())),
            "…and the newest last"
        );
        assert!(
            history
                .iter()
                .all(|w| w.date == date(2026, 7, 1) && w.status == FactStatus::Active),
            "every write says when it happened and what became of the record that carried it"
        );
        // Each write names the record it arrived in, and the addresses differ:
        // that is what makes the history worth having rather than a list of
        // values, and what proves the writes were not gathered off one row.
        let records: std::collections::BTreeSet<String> =
            history.iter().map(|w| w.fact.to_string()).collect();
        assert_eq!(
            records.len(),
            written,
            "each sitting is its own record and the history says which"
        );
    }

    /// **An edit appends; the value it replaced stays readable.**
    ///
    /// Fix-the-source is the surface — the record reads back changed, with no
    /// second copy beside it — and underneath, the write that put the old value
    /// there is still a write that happened. Both halves in one case: the
    /// projection alone passes on a store that overwrites, and the history
    /// alone passes on one that never projects.
    pub async fn an_edit_appends_and_the_value_it_replaced_stays_in_the_history<M: Memory>(
        store: &M,
    ) {
        let subject = EntityId::person("contract-appended");
        let captured = capture(
            store,
            NewFact {
                fields: [("cost".to_string(), "40".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the annual service", date(2026, 4, 18))
            },
        )
        .await;
        edit(
            store,
            &captured.address(),
            FactPatch {
                fields: [("cost".to_string(), "45".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen.fields.get("cost").map(String::as_str),
            Some("45"),
            "the record reads back edited, which is the surface"
        );

        let history = store
            .history(&subject, "cost")
            .await
            .expect("history should succeed");
        assert_eq!(
            history
                .iter()
                .map(|w| w.value.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("40"), Some("45")],
            "both writes are there, oldest first: {history:?}"
        );
        assert!(
            history.iter().all(|w| w.fact == captured.address()),
            "an edit is a write on the record it edited"
        );
    }

    /// **Clearing a key is a write, not a removal.**
    ///
    /// The key stops being current — the record reads back without it — and the
    /// writes that put it there are still in its history, with the clear itself
    /// recorded as the write that took it off. Nothing is deleted from the
    /// substrate, which is the same one-way rule retraction runs on.
    pub async fn clearing_a_key_leaves_its_writes_behind<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-cleared");
        let captured = capture(
            store,
            NewFact {
                fields: [("done_on".to_string(), "2026-04-18".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the annual service", date(2026, 4, 18))
            },
        )
        .await;
        edit(
            store,
            &captured.address(),
            FactPatch {
                clear_fields: vec!["done_on".to_string()],
                ..Default::default()
            },
        )
        .await;

        let seen = read_back(store, &subject, &captured.id).await;
        assert!(
            !seen.fields.contains_key("done_on"),
            "the cleared key is not current truth any more: {seen:?}"
        );
        let history = store
            .history(&subject, "done_on")
            .await
            .expect("history should succeed");
        assert_eq!(
            history
                .iter()
                .map(|w| w.value.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("2026-04-18"), None],
            "the write that set it and the write that took it off, in that order: {history:?}"
        );
    }

    /// **A key nobody wrote is an empty history; an entity nobody created is a
    /// miss.**
    ///
    /// The two nothings a caller has to tell apart, and the positive they rest
    /// on: an empty list means the thing is there and nothing was recorded
    /// under that key, which is a different instruction from "that handle is
    /// wrong".
    pub async fn history_of_an_unwritten_key_is_empty_and_of_no_entity_is_a_miss<M: Memory>(
        store: &M,
    ) {
        let subject = EntityId::person("contract-unwritten");
        capture(
            store,
            NewFact {
                fields: [("weight".to_string(), "11".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "weighed at the yard", date(2026, 4, 18))
            },
        )
        .await;

        assert_eq!(
            store
                .history(&subject, "weight")
                .await
                .expect("history should succeed")
                .len(),
            1,
            "the key that was written comes back — the positive the absences rest on"
        );
        assert!(
            store
                .history(&subject, "height")
                .await
                .expect("a key nobody wrote is an answer, not a failure")
                .is_empty(),
            "nothing was recorded under that key, and the thing is still there"
        );
        let missed = store
            .history(&EntityId::person("contract-no-such"), "weight")
            .await;
        assert!(
            matches!(missed, Err(MemoryError::UnknownEntity { .. })),
            "a handle that names nothing is a miss, exactly as recall answers one: {missed:?}"
        );
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

    /// **A record's fields survive capture through any store, and no label is
    /// asked for.**
    ///
    /// Every other spec for these fields in this workspace runs against
    /// something that holds the record in memory, so all of them can pass while
    /// the one store production actually writes to drops the bag on the floor.
    /// That is not hypothetical: it is what the Outline adapter did — it built
    /// its stored fact field by field and left the bag out, so a capture
    /// answered with a record it had not written and a restart read back a
    /// record with no fields.
    ///
    /// **The read-back guard cannot catch this one**, which is why it needs a
    /// spec of its own. Read-back compares what came back against what the
    /// adapter *believed* it stored, and both halves were missing the fields
    /// in the same way — so a lossy write passed its own invariant. The
    /// comparison a dropped field cannot survive is against the CALLER's
    /// record, and this is the only place that comparison is made.
    pub async fn a_records_fields_survive_capture<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-evented");
        let touched = EntityId::new(EntityKind::Place, "contract-kiln-yard");
        ensure(store, &touched).await;

        let recorded: std::collections::BTreeMap<String, String> = [
            ("mood".to_string(), "delighted".to_string()),
            // A key no build has ever heard of, because the promise is that an
            // unknown field is kept as written rather than that known fields
            // survive.
            (
                "a-field-from-a-later-build".to_string(),
                "and its value".to_string(),
            ),
            // **The punctuation battery, and it is not decoration.** Every
            // character below was mangled by a real store at some point in this
            // record's short life: a space and an `=`, which a store rewrote
            // when it re-serialized the escapes around them, and a `~`, in
            // front of which a store INSERTED an escape of its own. A fake that
            // stores bytes verbatim finds none of this, which is why the battery
            // rides in the shared contract rather than in an adapter's own
            // tests.
            (
                "punctuation".to_string(),
                "a = b, c~d, <e> & \"f\" — 100% ünïcode".to_string(),
            ),
        ]
        .into_iter()
        .collect();
        let captured = capture(
            store,
            NewFact {
                fields: recorded.clone(),
                refs: vec![touched.clone()],
                ..NewFact::about(
                    subject.clone(),
                    "the kiln was finally lit",
                    date(2026, 7, 2),
                )
            },
        )
        .await;
        assert_eq!(
            captured.fields, recorded,
            "capture answered with fields it did not store"
        );

        let seen = read_back(store, &subject, &captured.id).await;
        assert_eq!(
            seen, captured,
            "the fields must survive read-back byte-identical"
        );
        assert_eq!(
            seen.fields, recorded,
            "…and they are what a later reader takes off the record"
        );
        assert_eq!(
            seen.refs,
            vec![touched],
            "the unnamed references survive with them"
        );
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
    /// And it takes the whole write with it: a record is one write, so a ref
    /// that cannot be resolved leaves no half-recorded fact behind.
    pub async fn a_records_ref_is_screened_by_the_guard<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-ref-guarded");
        ensure(store, &subject).await;
        let stranger = EntityId::person("contract-nobody-created-this");

        let outcome = store
            .capture(NewFact {
                refs: vec![stranger.clone()],
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

    /// **The key jojobot writes itself is refused to a caller, not silently
    /// eaten.**
    ///
    /// `retracts` names the row a retraction takes back, and jojobot is the
    /// only writer of it: a caller able to write that key could mark somebody
    /// else's record taken back without going through the verb that decides
    /// whether it may be.
    ///
    /// **The spec belongs in the shared contract because the refusal is the
    /// domain's**, so both stores answer for it: a rule one store enforces and
    /// the other does not is a rule that holds until somebody switches stores.
    pub async fn a_reserved_field_key_is_refused<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-reserved-key");
        ensure(store, &subject).await;

        let outcome = store
            .capture(NewFact {
                fields: [("retracts".to_string(), "person:someone-else#f1".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "it happened", date(2026, 7, 3))
            })
            .await;
        assert!(
            matches!(outcome, Err(MemoryError::InvalidFact(_))),
            "a field named `retracts` must be refused rather than forging the marker \
             that says a record was taken back: {outcome:?}"
        );

        // …and the ordinary keys beside it are untouched, `type` and `ref`
        // included: those were the previous grammar's words, not the record's.
        let landed = capture(
            store,
            NewFact {
                fields: [
                    ("kind".to_string(), "a value".to_string()),
                    ("type".to_string(), "another".to_string()),
                    ("ref".to_string(), "a third".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(subject.clone(), "it happened later", date(2026, 7, 4))
            },
        )
        .await;
        assert_eq!(
            landed.fields.get("kind").map(String::as_str),
            Some("a value")
        );
        assert_eq!(
            landed.fields.get("type").map(String::as_str),
            Some("another")
        );
        assert_eq!(
            landed.fields.get("ref").map(String::as_str),
            Some("a third")
        );
    }

    /// **Retraction: the record stays, marked, and the reason lands beside
    /// it.** Nothing is removed — that is the no-delete rule, and it is what
    /// makes this different from every store where taking something back means
    /// losing the evidence that it was ever said.
    pub async fn retracting_a_record_marks_it_and_records_why<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-retracted");
        let event = capture(
            store,
            NewFact::about(subject.clone(), "moved to the 14th", date(2026, 7, 3)),
        )
        .await;

        let taken_back = store
            .retract(
                &event.address(),
                Some("it was rebooked twice"),
                date(2026, 7, 4),
            )
            .await
            .expect("retracting a record should succeed");

        // The row it names: same id, same words, same place.
        assert_eq!(taken_back.retracted.id, event.id);
        assert_eq!(taken_back.retracted.content, event.content);
        assert_eq!(taken_back.retracted.fields, event.fields);
        assert_eq!(
            taken_back.retracted.status,
            FactStatus::Retracted,
            "the row is marked rather than removed"
        );

        // And the account of why, as a record of its own.
        assert_eq!(taken_back.record.content, "it was rebooked twice");
        assert_eq!(taken_back.record.date, date(2026, 7, 4));
        assert_eq!(
            taken_back.record.retracts(),
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
            NewFact::about(subject.clone(), "it happened", date(2026, 7, 3)),
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
            taken_back.record.retracts(),
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
            NewFact::about(subject.clone(), "it happened", date(2026, 7, 3)),
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
        // **Refused as already-done, never as impossible.** The caller asked
        // for a state jojobot is holding: an answer that says the retraction
        // did not happen and cannot happen is false about the record and about
        // what to do next.
        assert!(
            matches!(again, Err(MemoryError::AlreadyRetracted { .. })),
            "a second retraction is refused because it is already so: {again:?}"
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
    /// **no token forces it — not even the one this refusal itself mints.** Two
    /// same-named people can never merge into one portrait silently (rule 61).
    pub async fn add_entity_blocks_an_existing_handle<M: Memory>(store: &M) {
        let id = EntityId::person("contract-alpha");
        add(store, NewEntity::new(id.clone(), "Alpha", "crm-card")).await;

        let outcome = store
            .add_entity(NewEntity::new(id.clone(), "Alpha Two", "user-named"))
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
            panic!("a colliding handle must be blocked");
        };
        assert_eq!(candidates[0].reason, guard::MatchReason::ExactHandle);
        assert_eq!(
            candidates[0].source, "crm-card",
            "the caller decides on the source"
        );

        let held = guard::override_token(&attempted, &candidates);
        let again = store
            .add_entity(NewEntity {
                override_token: Some(held),
                ..NewEntity::new(id.clone(), "Alpha Two", "user-named")
            })
            .await
            .expect("the call itself succeeds; the guard answers in the result");
        let Guarded::Blocked { candidates, .. } = again else {
            panic!("a colliding handle stays blocked, token or not");
        };
        assert_eq!(candidates[0].reason, guard::MatchReason::ExactHandle);

        let seen = read_entity(store, &id).await;
        assert_eq!(
            seen.name, "Alpha",
            "the blocked write must not have overwritten anything"
        );
    }

    /// A near-miss handle is reported, and **the token that refusal minted** is
    /// what lets a genuinely different entity through — while a token nobody
    /// minted lets nothing through at all. Both halves, because a store that
    /// accepts any string passes the first one.
    pub async fn add_entity_reports_a_near_miss_then_accepts_its_own_token<M: Memory>(store: &M) {
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
        let Guarded::Blocked {
            attempted,
            candidates,
        } = outcome
        else {
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
        let token = guard::override_token(&attempted, &candidates);

        assert!(
            matches!(
                store
                    .add_entity(NewEntity {
                        override_token: Some("0000000000000000".into()),
                        ..NewEntity::new(typo.clone(), "Riversid", "user-named")
                    })
                    .await
                    .expect("call succeeds"),
                Guarded::Blocked { .. }
            ),
            "a token nobody minted lifts nothing"
        );

        let forced = add(
            store,
            NewEntity {
                override_token: Some(token),
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
    /// There is no override on this path either: a genuinely new
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
            // A cross-link must stay one token on its frontmatter line, so
            // whitespace, a comma and a backtick are all refused whatever the
            // task layer's grammar is.
            ("ok", "user-named", Some("card 874")),
            ("ok", "user-named", Some("ENG-421,ENG-422")),
            ("ok", "user-named", Some("ENG-`421`")),
            ("ok", "user-named", Some("")),
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

    /// **The cross-link takes the task layer's own grammar.** `crm` points at
    /// this entity in whatever system holds the operator's tasks, and that
    /// system decides how it addresses things: `card:874` in one, `ENG-421` in
    /// another. A validator that accepted only one of those refused the link
    /// outright to every other layer, which left the entity with no way to
    /// record it at all.
    pub async fn a_cross_link_takes_the_task_layers_own_grammar<M: Memory>(store: &M) {
        let id = EntityId::person("contract-crosslink");
        add(
            store,
            NewEntity {
                crm: Some("ENG-421".into()),
                ..NewEntity::new(id.clone(), "Beta", "user-named")
            },
        )
        .await;
        let seen = read_entity(store, &id).await;
        assert_eq!(seen.crm.as_deref(), Some("ENG-421"));
    }

    // --- retrieval: the search verb ------------------------------------------
    //
    // These run against a store that also carries the search projection. They
    // are scoped to handles this suite owns, so they hold against a shared,
    // pre-populated collection as much as against an empty fake.

    /// Search the store, expecting the query to be well-formed.
    async fn found<S: Search>(search: &S, query: SearchQuery) -> Vec<Hit> {
        search
            .search(&query)
            .await
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
    pub async fn search_finds_a_fact_captured_moments_ago<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
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

        let hits = found(search, SearchQuery::text("zamboni")).await;
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
    pub async fn search_fact_hits_carry_an_address_and_provenance<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let subject = EntityId::person("contract-searchable");
        capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                details: Some("said so twice".into()),
                ..NewFact::about(subject.clone(), "cycles to the velodrome", date(2026, 7, 1))
            },
        )
        .await;

        let hits = found(search, SearchQuery::text("velodrome")).await;
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
    pub async fn search_excludes_a_retracted_record_by_default<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let subject = EntityId::person("contract-retraction-hit");
        let live = capture(
            store,
            NewFact::about(subject.clone(), "the quartet rehearsed", date(2026, 7, 1)),
        )
        .await;
        let taken_back = capture(
            store,
            NewFact::about(
                subject.clone(),
                "the quartet rehearsed twice",
                date(2026, 7, 2),
            ),
        )
        .await;
        store
            .retract(
                &taken_back.address(),
                Some("it never happened"),
                date(2026, 7, 3),
            )
            .await
            .expect("retracting a record should succeed");

        let addresses = |hits: &[Hit]| -> Vec<String> {
            fact_hits(hits)
                .iter()
                .map(|f| f.address().to_string())
                .collect()
        };

        let default = found(search, SearchQuery::text("rehearsed")).await;
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
            search,
            SearchQuery {
                status: Some(FactStatus::Retracted),
                ..SearchQuery::text("rehearsed")
            },
        )
        .await;
        assert!(
            addresses(&asked).contains(&taken_back.address().to_string()),
            "nothing was deleted, so asking for it by name finds it: {asked:?}"
        );
    }

    pub async fn search_excludes_superseded_by_default_and_lists_it_on_request<
        M: Memory,
        S: Search,
    >(
        store: &M,
        search: &S,
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

        let default = found(search, SearchQuery::text("theremin")).await;
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
            search,
            SearchQuery {
                status: Some(FactStatus::Superseded),
                ..SearchQuery::text("theremin")
            },
        )
        .await;
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
    pub async fn search_answers_ask_across_by_kind_and_edge<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
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
                ..NewFact::about(
                    project.clone(),
                    "runs out of contract-faraway",
                    date(2026, 7, 4),
                )
            },
        )
        .await;

        let hits = found(
            search,
            SearchQuery {
                kind: Some(EntityKind::Person),
                edge: Some(EdgeFilter {
                    shape: Some(EdgeShape::Location),
                    object: far.clone(),
                }),
                ..Default::default()
            },
        )
        .await;
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
    pub async fn search_by_edge_object_alone_finds_any_shape<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let fest = EntityId::new(EntityKind::Event, "contract-connected-fest");
        let attendee = capture(
            store,
            NewFact {
                edge: Some(Edge::new(EdgeShape::Attendance, fest.clone())),
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
            search,
            SearchQuery {
                edge: Some(EdgeFilter {
                    shape: None,
                    object: fest,
                }),
                ..Default::default()
            },
        )
        .await;
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
    pub async fn search_pins_a_named_entity_first<M: Memory, S: Search>(store: &M, search: &S) {
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

        let hits = found(search, SearchQuery::text(handle.as_str())).await;
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
    pub async fn search_fact_hits_name_their_subject_and_home<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let subject = EntityId::person("contract-orienteer");
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

        let hits = found(search, SearchQuery::text("map for fun")).await;
        let (fact_subject, fact_home) = hits
            .iter()
            .find_map(|h| match h {
                Hit::Fact {
                    fact,
                    subject: s,
                    home,
                    ..
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
    pub async fn search_entity_hits_carry_their_edges<M: Memory, S: Search>(store: &M, search: &S) {
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
                ..NewFact::about(
                    handle.clone(),
                    "meets on the first Sunday",
                    date(2026, 7, 1),
                )
            },
        )
        .await;

        let hits = found(search, SearchQuery::text(handle.as_str())).await;
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
                ..NewFact::about(EntityId::person(who), "spending the season there", on)
            },
        )
        .await
    }

    // --- declared types ------------------------------------------------------

    /// Declare a type the store is expected to keep.
    async fn declare<M: Memory>(store: &M, declared: DeclaredType) -> DeclaredType {
        let name = declared.name.clone();
        store
            .declare_type(declared)
            .await
            .unwrap_or_else(|e| panic!("declaring '{name}' should succeed: {e}"))
    }

    /// One type out of the store's own listing.
    async fn read_type<M: Memory>(store: &M, name: &str) -> DeclaredType {
        store
            .declared_types()
            .await
            .expect("declared_types should succeed")
            .into_iter()
            .find(|t| t.name == name)
            .unwrap_or_else(|| panic!("declared_types must return '{name}'"))
    }

    /// **A declared type reads back, complete and in the order it was
    /// declared.**
    ///
    /// The order is a claim and not a convenience: an answer names the keys a
    /// record holds and lacks in the type's own order, so a store that sorted
    /// them by name would replace an order somebody chose with one nobody did.
    /// It is asserted with keys whose declared order is NOT their alphabetical
    /// order, or a sorting store passes this unchanged.
    pub async fn a_declared_type_reads_back<M: Memory>(store: &M) {
        let declared = DeclaredType::new(
            "contract-tenancy",
            vec![
                Field::new("starts", ValueType::Date),
                Field::new("rooms", ValueType::Number),
                Field::new("building", ValueType::Reference),
                Field::new("active", ValueType::Boolean),
            ],
        );
        let written = declare(store, declared.clone()).await;
        assert_eq!(written, declared, "the store returns what it stored");
        assert_eq!(
            read_type(store, "contract-tenancy").await,
            declared,
            "…and a later read returns it unchanged, keys and order and all",
        );

        // The negative the read rests on: nothing invents a type nobody
        // declared. Without it the assertions above hold on a store that
        // answers every question with every type it can think of.
        let known = store
            .declared_types()
            .await
            .expect("declared_types should succeed");
        assert!(
            !known.iter().any(|t| t.name == "contract-never-declared"),
            "a type nobody declared is not in the store: {known:?}",
        );
    }

    /// **Declaring again replaces the keys whole.**
    ///
    /// A type is the set of keys it names now. One that accumulated every key
    /// it had ever named would describe no record at all, and every match
    /// against it would report keys the writer had already dropped as lacking.
    pub async fn declaring_a_type_again_replaces_its_keys<M: Memory>(store: &M) {
        declare(
            store,
            DeclaredType::new(
                "contract-parcel",
                vec![
                    Field::new("weight", ValueType::Number),
                    Field::new("sender", ValueType::Reference),
                ],
            ),
        )
        .await;
        let second = DeclaredType::new(
            "contract-parcel",
            vec![
                Field::new("weight", ValueType::Number),
                Field::new("arrives", ValueType::Date),
            ],
        );
        declare(store, second.clone()).await;

        assert_eq!(
            read_type(store, "contract-parcel").await,
            second,
            "the second declaration is what the type is now",
        );
        assert_eq!(
            store
                .declared_types()
                .await
                .expect("declared_types should succeed")
                .iter()
                .filter(|t| t.name == "contract-parcel")
                .count(),
            1,
            "and it replaced the first rather than standing beside it",
        );
    }

    /// **A type with no keys is refused, and the store keeps nothing.**
    ///
    /// A type arrives complete with its fields. A bare name is a type nobody
    /// can query by, and it would have to be configured into usefulness before
    /// it did anything.
    ///
    /// Both halves: the refusal, and that the refusal wrote nothing. A store
    /// that errored after inserting the name would pass the first alone.
    pub async fn a_type_with_no_keys_is_refused_and_writes_nothing<M: Memory>(store: &M) {
        let refused = store
            .declare_type(DeclaredType::new("contract-empty", vec![]))
            .await;
        assert!(
            matches!(refused, Err(MemoryError::InvalidType(_))),
            "a type is the keys it names: {refused:?}",
        );
        let known = store
            .declared_types()
            .await
            .expect("declared_types should succeed");
        assert!(
            !known.iter().any(|t| t.name == "contract-empty"),
            "a refused declaration leaves nothing behind: {known:?}",
        );

        // The positive it rests on: the same store takes a declaration that
        // does name a key, so the absence above is the refusal and not a store
        // that declares nothing at all.
        declare(
            store,
            DeclaredType::new("contract-not-empty", vec![Field::new("x", ValueType::Text)]),
        )
        .await;
        assert_eq!(read_type(store, "contract-not-empty").await.fields.len(), 1);
    }

    /// **A type that ships with the software is closed to callers, and the
    /// store is what closes it.**
    ///
    /// Three claims in one case, because each is worthless without the others.
    /// The origin survives being written and read back — a store that dropped
    /// it would leave every shipped type looking like a caller's. A caller's
    /// declaration over that name is refused and changes nothing. And the same
    /// caller's own type still replaces on redeclare, so what was refused is
    /// the origin and not the act of redeclaring.
    pub async fn a_shipped_type_refuses_a_callers_redeclaration<M: Memory>(store: &M) {
        let shipped = DeclaredType::shipped(
            "contract-rota",
            vec![
                Field::new("starts", ValueType::Date),
                Field::new("cover", ValueType::Reference),
            ],
        );
        declare(store, shipped.clone()).await;
        assert_eq!(
            read_type(store, "contract-rota").await.origin,
            Origin::Shipped,
            "where the type came from survives the store",
        );

        let refused = store
            .declare_type(DeclaredType::new(
                "contract-rota",
                vec![Field::new("starts", ValueType::Date)],
            ))
            .await;
        assert!(
            matches!(refused, Err(MemoryError::ShippedType { .. })),
            "a caller cannot declare over a type the software ships: {refused:?}",
        );
        assert_eq!(
            read_type(store, "contract-rota").await,
            shipped,
            "…and the refused declaration left the shipped type exactly as it was",
        );

        // The positive the refusal rests on: redeclaring is not what was
        // refused. Without this the case above passes on a store that refuses
        // every second declaration of any name at all.
        declare(
            store,
            DeclaredType::new(
                "contract-shift",
                vec![Field::new("starts", ValueType::Date)],
            ),
        )
        .await;
        let replaced =
            DeclaredType::new("contract-shift", vec![Field::new("cover", ValueType::Text)]);
        declare(store, replaced.clone()).await;
        assert_eq!(
            read_type(store, "contract-shift").await,
            replaced,
            "a caller's own type replaces on redeclare, origin and all",
        );
        assert_eq!(
            read_type(store, "contract-shift").await.origin,
            Origin::Declared,
            "and a caller's type reads back as a caller's",
        );
    }

    /// **Two types may name one key and mean their own thing by it — through
    /// the store.**
    ///
    /// Keys are scoped by the type that names them and registered nowhere, so
    /// there is no list of legal keys anywhere and nothing to keep in step. A
    /// store keying its rows on the key name alone would lose one of these, and
    /// nothing above it could tell.
    pub async fn two_stored_types_may_name_one_key<M: Memory>(store: &M) {
        declare(
            store,
            DeclaredType::new(
                "contract-seating",
                vec![Field::new("seats", ValueType::Text)],
            ),
        )
        .await;
        declare(
            store,
            DeclaredType::new(
                "contract-booking",
                vec![Field::new("seats", ValueType::Number)],
            ),
        )
        .await;

        assert_eq!(
            read_type(store, "contract-seating").await.fields[0].holds,
            ValueType::Text,
        );
        assert_eq!(
            read_type(store, "contract-booking").await.fields[0].holds,
            ValueType::Number,
            "one key name, two types, and each keeps what it declared",
        );
    }

    /// **A type read back out of the store still matches a record nobody
    /// declared.**
    ///
    /// The journey, rather than the function: the declaration goes into the
    /// store, comes back out, and matches a loose bag of keys that was never
    /// told which type it was. This is what makes matching structural in the
    /// built system rather than in one pure function — a store that mangled a
    /// key name or a value type would leave the domain's own cases green and
    /// every real query wrong.
    pub async fn a_stored_type_matches_a_record_that_never_declared_it<M: Memory>(store: &M) {
        declare(
            store,
            DeclaredType::new(
                "contract-shipment",
                vec![
                    Field::new("weight", ValueType::Number),
                    Field::new("arrives", ValueType::Date),
                ],
            ),
        )
        .await;
        let stored = read_type(store, "contract-shipment").await;

        let loose: std::collections::BTreeMap<String, String> = [("weight", "12")]
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect();
        let found = stored
            .matched_by(&loose)
            .expect("a record carrying one of the keys answers the type");
        assert_eq!(found.held, vec!["weight"]);
        assert_eq!(
            found.lacking,
            vec!["arrives"],
            "a partial match comes back and names what it lacks: {found:?}",
        );

        // And the negative it rests on, with the same stored type: a record
        // sharing no key is not a match, or "it matched" says nothing.
        let unrelated: std::collections::BTreeMap<String, String> =
            [("mood".to_string(), "curious".to_string())]
                .into_iter()
                .collect();
        assert_eq!(stored.matched_by(&unrelated), None);
    }

    /// **Search finds a record by the keys it carries, and the record never
    /// said which type it was.**
    ///
    /// This is the card's load-bearing claim, exercised through the verb a
    /// caller actually uses rather than through the matcher. The records here
    /// name no type at all, so nothing on a record admits it to the answer:
    /// only its keys do. A build that matched on a name the record carried
    /// would pass every case in the domain and fail this one.
    ///
    /// Both matches come back in the one answer — complete and partial — and
    /// the partial names what it lacks. Complete-versus-partial is reported,
    /// never filtered.
    pub async fn search_finds_things_that_answer_a_type_structurally<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let declared = store
            .declare_type(DeclaredType::new(
                "contract-crate",
                vec![
                    Field::new("weight", ValueType::Number),
                    Field::new("arrives", ValueType::Date),
                ],
            ))
            .await
            .expect("declaring should succeed");

        // **Carries both keys across TWO records, and neither answers alone.**
        // This is the whole point of the unit: what a thing is gets written
        // down a piece at a time, and a store full of half-descriptions is what
        // a real one looks like.
        let whole = EntityId::person("contract-crate-whole");
        for (key, value, said) in [
            ("weight", "12", "somebody weighed it"),
            ("arrives", "2026-08-10", "and somebody else was told when"),
        ] {
            capture(
                store,
                NewFact {
                    fields: [(key.to_string(), value.to_string())].into_iter().collect(),
                    ..NewFact::about(whole.clone(), said, date(2026, 8, 1))
                },
            )
            .await;
        }
        // Carries one of them, and nothing else ever says the rest.
        let partial = EntityId::person("contract-crate-partial");
        capture(
            store,
            NewFact {
                fields: [("weight".to_string(), "3".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    partial.clone(),
                    "a lighter one, and nobody wrote down when it lands",
                    date(2026, 8, 2),
                )
            },
        )
        .await;
        // Carries neither, and is a thing all the same.
        let unrelated = EntityId::person("contract-crate-unrelated");
        capture(
            store,
            NewFact {
                fields: [("mood".to_string(), "curious".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    unrelated.clone(),
                    "nothing to do with crates",
                    date(2026, 8, 3),
                )
            },
        )
        .await;

        let hits = found(
            search,
            SearchQuery {
                answers_type: Some(declared),
                limit: 50,
                ..Default::default()
            },
        )
        .await;
        // **The answer is about THINGS**, so it is the entity hits that carry
        // the match. A fact hit could not: the question was never asked of one
        // row.
        let answered: Vec<(&EntityId, &crate::memory::types::Match)> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Entity {
                    entity, answers, ..
                } => Some((&entity.id, answers.as_deref()?)),
                _ => None,
            })
            .collect();

        let (_, whole_match) = answered
            .iter()
            .find(|(id, _)| **id == whole)
            .unwrap_or_else(|| panic!("a thing carrying every key must come back: {answered:?}"));
        assert!(
            whole_match.complete(),
            "two records between them hold every key, and the thing says so \
             rather than the caller counting: {whole_match:?}",
        );

        let (_, partial_match) = answered
            .iter()
            .find(|(id, _)| **id == partial)
            .unwrap_or_else(|| {
                panic!("a partial match is returned, not filtered out: {answered:?}")
            });
        assert!(!partial_match.complete(), "{partial_match:?}");
        assert_eq!(
            partial_match.lacking,
            vec!["arrives"],
            "and it names what it lacks, by name: {partial_match:?}",
        );

        // **The negative, and it rests on the two positives above.** A thing
        // sharing no key with the type is not a weak match, it is not a match:
        // without this, "it matched" says nothing, because everything would.
        assert!(
            !answered.iter().any(|(id, _)| **id == unrelated),
            "a thing carrying none of the keys is not in the answer: {answered:?}",
        );
    }

    /// **A type query says which keys are wrong, and returns the thing
    /// anyway.**
    ///
    /// The typed path is not a gate at read time either. A value that does not
    /// hold what the type declared comes back flagged, with what was declared
    /// and what is actually there, on a thing that is still found.
    pub async fn a_type_query_flags_a_bad_value_and_returns_the_record<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let declared = store
            .declare_type(DeclaredType::new(
                "contract-pallet",
                vec![Field::new("arrives", ValueType::Date)],
            ))
            .await
            .expect("declaring should succeed");

        let messy = EntityId::person("contract-pallet-messy");
        capture(
            store,
            NewFact {
                fields: [("arrives".to_string(), "next tuesday".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    messy.clone(),
                    "somebody wrote the date in words",
                    date(2026, 8, 4),
                )
            },
        )
        .await;

        let hits = found(
            search,
            SearchQuery {
                answers_type: Some(declared),
                limit: 50,
                ..Default::default()
            },
        )
        .await;
        let flagged = hits
            .iter()
            .find_map(|h| match h {
                Hit::Entity {
                    entity, answers, ..
                } if entity.id == messy => answers.as_deref(),
                _ => None,
            })
            .unwrap_or_else(|| panic!("the thing is found, not dropped: {hits:?}"));

        assert!(
            flagged.complete(),
            "a bad value is not a missing key: {flagged:?}",
        );
        assert_eq!(flagged.mistyped.len(), 1, "{flagged:?}");
        assert_eq!(flagged.mistyped[0].key, "arrives");
        assert_eq!(flagged.mistyped[0].declared, ValueType::Date);
        assert_eq!(
            flagged.mistyped[0].value, "next tuesday",
            "the reader sees the mistake rather than being told one happened",
        );
    }

    /// Run the whole contract, **including retrieval**, against a store that
    /// carries the search projection. The search half can't live in `run_all`:
    /// the bare Memory port has no read side for it.
    pub async fn run_all_searchable<M: Memory, S: Search>(store: &M, search: &S) {
        run_all(store).await;

        search_finds_a_fact_captured_moments_ago(store, search).await;
        search_fact_hits_carry_an_address_and_provenance(store, search).await;
        search_excludes_superseded_by_default_and_lists_it_on_request(store, search).await;
        search_excludes_a_retracted_record_by_default(store, search).await;
        search_answers_ask_across_by_kind_and_edge(store, search).await;
        search_by_edge_object_alone_finds_any_shape(store, search).await;
        search_pins_a_named_entity_first(store, search).await;
        search_fact_hits_name_their_subject_and_home(store, search).await;
        search_entity_hits_carry_their_edges(store, search).await;

        search_finds_things_that_answer_a_type_structurally(store, search).await;
        a_type_query_flags_a_bad_value_and_returns_the_record(store, search).await;
    }

    /// Run the whole contract against one store.
    /// **A kind selects its objects, and each one's page comes back whole.**
    ///
    /// The journey rather than the function: the prose goes into the store,
    /// and a query that named a KIND — never a handle — brings it back. A
    /// charter is prose, so this is the shape behind "the objects of a kind,
    /// with their prose", and nothing on this path knows what a charter is.
    ///
    /// It asserts containment rather than the whole list: the contract runs
    /// every case against one store, so other cases' entities are legitimately
    /// there. The negative it rests on is a different kind, which must NOT be
    /// in a kind-scoped answer however many objects share the store.
    pub async fn a_graph_query_selects_a_kind_and_returns_its_prose<M: Memory>(store: &M) {
        let one = EntityId::new(EntityKind::Bot, "contract-graph-one");
        let two = EntityId::new(EntityKind::Bot, "contract-graph-two");
        let outsider = EntityId::person("contract-graph-outsider");
        for id in [&one, &two, &outsider] {
            ensure(store, id).await;
        }
        // Several paragraphs, because prose comes back WHOLE and a store that
        // kept the first line would satisfy a one-line fixture.
        let charter = "Keeps the roster.\n\nHard line: never writes to the ledger.\n\nWorks in \
                       the yard.";
        store
            .set_prose(&one, charter)
            .await
            .expect("set_prose should succeed");
        store
            .set_prose(&outsider, "Not a bot, and holds a page anyway.")
            .await
            .expect("set_prose should succeed");

        let found = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    kind: Some(EntityKind::Bot),
                    ..graph::Selection::default()
                },
                include: graph::Include {
                    facts: false,
                    prose: true,
                },
                follow: None,
                history: None,
            },
        )
        .await
        .expect("a kind is a selection");

        let page = |id: &EntityId| {
            found
                .iter()
                .find(|o| &o.entity.id == id)
                .map(|o| o.prose.clone().unwrap_or_default())
        };
        assert_eq!(
            page(&one).as_deref(),
            Some(charter),
            "the page comes back whole, through a query that named only a kind",
        );
        assert_eq!(
            page(&two).as_deref(),
            Some(""),
            "a bot with no page comes back with an empty one, not missing from the answer",
        );
        assert_eq!(
            page(&outsider),
            None,
            "and the kind filter holds: a person is not in an answer about bots",
        );
    }

    /// **A key's VALUE selects, over a value that survived storage — and the
    /// edge beside it is walkable from either end.**
    ///
    /// Two claims about the store in one case, because they are one write.
    /// The value carries the punctuation battery for the reason
    /// [`an_event_survives_capture`] does: a markdown store rewrites markdown,
    /// so a filter comparing against what the caller SENT will miss what the
    /// store KEPT, and a fake that stores bytes verbatim never shows it.
    ///
    /// The negative is the same key with the other value, which must select
    /// the other record — a build ignoring the value passes the positive and
    /// fails here.
    pub async fn a_graph_query_filters_on_a_stored_value_and_walks_an_edge<M: Memory>(store: &M) {
        let gathering = EntityId::new(EntityKind::Event, "contract-graph-gathering");
        let coming = EntityId::person("contract-graph-coming");
        let staying = EntityId::person("contract-graph-staying");
        ensure(store, &gathering).await;

        let yes = "yes = certain, ~confirmed~ 100% ünïcode";
        let reply = |who: &EntityId, answer: &str, said: &str| NewFact {
            edge: Some(Edge::new(EdgeShape::Attendance, gathering.clone())),
            fields: [("answer".to_string(), answer.to_string())]
                .into_iter()
                .collect(),
            refs: Vec::new(),
            ..NewFact::about(who.clone(), said, date(2026, 8, 10))
        };
        capture(store, reply(&coming, yes, "will be there")).await;
        capture(store, reply(&staying, "no", "cannot make it")).await;

        let selected = |answer: &str| {
            let answer = answer.to_string();
            async move {
                let found = graph::walk(
                    store,
                    &graph::GraphQuery {
                        select: graph::Selection {
                            fields: vec![graph::FieldFilter::holding("answer", &answer)],
                            ..graph::Selection::default()
                        },
                        ..graph::GraphQuery::default()
                    },
                )
                .await
                .expect("a key filter is a selection");
                found
                    .iter()
                    .map(|o| o.entity.id.clone())
                    .collect::<Vec<_>>()
            }
        };
        let confirmed = selected(yes).await;
        assert!(
            confirmed.contains(&coming),
            "the value the store KEPT is what the filter must match: {confirmed:?}",
        );
        assert!(
            !confirmed.contains(&staying),
            "and the other answer is not in it: {confirmed:?}",
        );
        let declined = selected("no").await;
        assert!(
            declined.contains(&staying) && !declined.contains(&coming),
            "the same key with the other value selects the other record: {declined:?}",
        );

        // The edge, walked back from the thing both records point at. The
        // guests were never named in this query: they are reached.
        let walked = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(gathering.clone()),
                    ..graph::Selection::default()
                },
                include: graph::Include::default(),
                follow: Some(graph::Follow {
                    along: graph::Along::Edge(EdgeShape::Attendance),
                    direction: Some(graph::Direction::In),
                    depth: 1,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
            },
        )
        .await
        .expect("a subject with a walk");
        let reached: Vec<&EntityId> = walked[0].connected.iter().map(|o| &o.entity.id).collect();
        assert!(
            reached.contains(&&coming) && reached.contains(&&staying),
            "both guests hang off the gathering: {reached:?}",
        );
        let brought = walked[0]
            .connected
            .iter()
            .find(|o| o.entity.id == coming)
            .expect("the guest who is coming");
        assert!(
            brought.facts.iter().any(|f| f.content == "will be there"),
            "and a reached object arrives carrying its own records: {brought:?}",
        );
        assert_eq!(
            brought.via,
            Some(graph::Via {
                link: graph::Link::Edge(EdgeShape::Attendance),
                direction: graph::Direction::In,
            }),
            "which says how the walk got to it",
        );
    }

    /// **A declared reference key is walkable against the store**, and the
    /// declaration it rests on comes out of that same store.
    ///
    /// The claim about storage is the one the pure resolver cannot make: the
    /// walk asks the store what has been declared, so a store that keeps a
    /// declaration but does not hand it back leaves every relation unfollowable
    /// while every unit test over the resolver stays green.
    ///
    /// The negative is the same walk with the type declared as TEXT rather than
    /// a reference — same records, same values, and no link — so a build that
    /// walked any key holding a handle fails here.
    pub async fn a_declared_reference_key_is_walkable_against_the_store<M: Memory>(store: &M) {
        let owner = EntityId::person("contract-relation-owner");
        let held = EntityId::new(EntityKind::Thing, "contract-relation-held");
        ensure(store, &owner).await;
        ensure(store, &held).await;

        capture(
            store,
            NewFact {
                fields: [
                    ("keeper".to_string(), owner.to_string()),
                    ("since".to_string(), "2019-04-15".to_string()),
                ]
                .into_iter()
                .collect(),
                refs: Vec::new(),
                ..NewFact::about(held.clone(), "the one that is held", date(2026, 8, 10))
            },
        )
        .await;

        let declare = |holds: ValueType| async move {
            store
                .declare_type(DeclaredType::new(
                    "contract-holding",
                    vec![
                        Field::new("keeper", holds),
                        Field::new("since", ValueType::Date),
                    ],
                ))
                .await
                .expect("a declaration the store keeps");
        };
        let reached = |relation: &str| {
            let relation = relation.to_string();
            let subject = owner.clone();
            async move {
                graph::walk(
                    store,
                    &graph::GraphQuery {
                        select: graph::Selection {
                            subject: Some(subject),
                            ..graph::Selection::default()
                        },
                        include: graph::Include {
                            facts: false,
                            prose: false,
                        },
                        follow: Some(graph::Follow {
                            along: graph::Along::Relation(relation),
                            direction: Some(graph::Direction::In),
                            ..graph::Follow::hop()
                        }),
                        history: None,
                    },
                )
                .await
            }
        };

        declare(ValueType::Text).await;
        reached("keeper").await.expect_err(
            "a key declared to hold text is no relation, whatever its value looks like",
        );

        declare(ValueType::Reference).await;
        let found = reached("keeper")
            .await
            .expect("declared a reference, the key is a relation");
        let connected: Vec<&EntityId> = found[0].connected.iter().map(|o| &o.entity.id).collect();
        assert!(
            connected.contains(&&held),
            "the key walked inbound reaches what points at this object: {connected:?}",
        );

        // And the ordering the same declaration licenses, over a value that
        // survived storage rather than one this test still holds.
        let older = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    fields: vec![graph::FieldFilter::comparing(
                        "since",
                        crate::memory::types::Compare::Before,
                        "2020-01-01",
                    )],
                    ..graph::Selection::default()
                },
                ..graph::GraphQuery::default()
            },
        )
        .await
        .expect("a declared date licenses an ordering");
        assert!(
            older.iter().any(|o| o.entity.id == held),
            "the record stored before that date is selected by the ordering: {older:?}",
        );
    }

    /// **A kind the code learned today survives the store.**
    ///
    /// The store keeps a kind as a string and reads it back off the handle, so
    /// a kind arriving in the enum needs nothing migrated — but that is a claim
    /// **A thing reads back as one dense row: its records' fields, folded.**
    ///
    /// What a thing IS gets written down over several sittings, so the answer
    /// to "what is this" is the fold and not the list of sittings. A caller
    /// handed the records and left to fold them itself is a caller doing
    /// jojobot's job.
    ///
    /// **The row is there whether or not the records are**, which is the half
    /// that cannot be tested against the records alone: a fold taken from the
    /// shipped list would come back empty on exactly the call that asked for
    /// the dense answer and nothing else.
    ///
    /// **And it folds over ALL the thing's records rather than the kept ones.**
    /// A filter chooses which objects come back; it does not change what each
    /// one is.
    pub async fn a_thing_reads_back_as_its_fields_folded<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::Thing, "contract-folded-thing");
        let sittings = [
            [("weight", "11"), ("wheel", "700c")],
            // The second sitting adds a key and writes one of the first
            // sitting's again — the newest write is what the thing holds now.
            [("weight", "12"), ("gears", "22")],
        ];
        for fields in sittings {
            capture(
                store,
                NewFact {
                    fields: fields
                        .iter()
                        .map(|(key, value)| (key.to_string(), value.to_string()))
                        .collect(),
                    ..NewFact::about(subject.clone(), "a sitting at the bench", date(2026, 4, 18))
                },
            )
            .await;
        }

        let dense = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(subject.clone()),
                    ..graph::Selection::default()
                },
                include: graph::Include {
                    facts: false,
                    prose: false,
                },
                follow: None,
                history: None,
            },
        )
        .await
        .expect("a handle is a selection");
        let object = &dense[0];
        assert_eq!(
            object.fields,
            [
                ("gears".to_string(), "22".to_string()),
                ("weight".to_string(), "12".to_string()),
                ("wheel".to_string(), "700c".to_string()),
            ]
            .into_iter()
            .collect(),
            "one row, one value per key, the newest write winning"
        );
        assert!(
            object.facts.is_empty(),
            "…and the records themselves were not asked for: {:?}",
            object.facts
        );

        // The other half, and the positive the negative above rests on: asking
        // for the records still gets every one of them, each addressed.
        let whole = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(subject.clone()),
                    ..graph::Selection::default()
                },
                include: graph::Include {
                    facts: true,
                    prose: false,
                },
                follow: None,
                history: None,
            },
        )
        .await
        .expect("a handle is a selection");
        assert_eq!(
            whole[0].facts.len(),
            2,
            "the sittings are still reachable: {:?}",
            whole[0].facts
        );
        assert_eq!(
            whole[0].fields, object.fields,
            "and the row does not change with them"
        );
        assert!(
            whole[0]
                .facts
                .iter()
                .all(|f| f.address().home == subject && f.address().local.ordinal().is_some()),
            "every record keeps the address that makes it editable: {:?}",
            whole[0].facts
        );
    }

    /// **A history longer than the window comes back cut, and says how much
    /// exists.**
    ///
    /// The read whose job is the small answer must not be able to flood the
    /// caller it serves. The total is what makes the cut honest: it says how
    /// many writes are there without handing them over.
    ///
    /// Both ends of the rule in one case, because a cap that always fires is
    /// indistinguishable from one that never does: a history inside the window
    /// comes back whole, with nothing left out.
    pub async fn a_long_history_is_cut_to_its_newest_and_says_how_many<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::Thing, "contract-long-history");
        let written = graph::WRITES_SHOWN + 5;
        for nth in 1..=written {
            capture(
                store,
                NewFact {
                    fields: [("weight".to_string(), nth.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(subject.clone(), "weighed at the bench", date(2026, 4, 18))
                },
            )
            .await;
        }
        let asking = |key: &str| {
            let key = key.to_string();
            let subject = subject.clone();
            async move {
                graph::walk(
                    store,
                    &graph::GraphQuery {
                        select: graph::Selection {
                            subject: Some(subject),
                            ..graph::Selection::default()
                        },
                        include: graph::Include {
                            facts: false,
                            prose: false,
                        },
                        follow: None,
                        history: Some(graph::History::of(&key)),
                    },
                )
                .await
                .expect("a handle is a selection")
            }
        };

        let found = asking("weight").await;
        let history = found[0]
            .history
            .as_ref()
            .expect("the query named a key, so the object carries its writes");
        assert_eq!(
            history.total, written,
            "the answer says how many writes exist"
        );
        assert_eq!(
            history.writes.len(),
            graph::WRITES_SHOWN,
            "…and hands back a window rather than all of them"
        );
        assert_eq!(history.elided(), 5, "…and how many it left out");
        assert_eq!(
            history
                .writes
                .iter()
                .map(|w| w.value.as_deref())
                .collect::<Vec<_>>(),
            (6..=written)
                .map(|nth| nth.to_string())
                .collect::<Vec<_>>()
                .iter()
                .map(|v| Some(v.as_str()))
                .collect::<Vec<_>>(),
            "the window is the NEWEST writes, still oldest first inside it"
        );

        // A key written a handful of times is untouched by any of this, and
        // says nothing was left out.
        let short = EntityId::new(EntityKind::Thing, "contract-short-history");
        for nth in 1..=3 {
            capture(
                store,
                NewFact {
                    fields: [("cost".to_string(), nth.to_string())]
                        .into_iter()
                        .collect(),
                    ..NewFact::about(short.clone(), "paid at the counter", date(2026, 4, 18))
                },
            )
            .await;
        }
        let found = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(short.clone()),
                    ..graph::Selection::default()
                },
                include: graph::Include {
                    facts: false,
                    prose: false,
                },
                follow: None,
                history: Some(graph::History::of("cost")),
            },
        )
        .await
        .expect("a handle is a selection");
        let history = found[0].history.as_ref().expect("the query named a key");
        assert_eq!((history.total, history.writes.len()), (3, 3));
        assert_eq!(
            history.elided(),
            0,
            "a history that fits leaves nothing out, and says so"
        );
    }

    /// about the store rather than about the enum, and only a store can answer
    /// it. The negative is the filter: a pet is not returned by a listing of
    /// things, so the kind is carried rather than defaulted to something.
    pub async fn a_pet_is_its_own_kind_in_the_store<M: Memory>(store: &M) {
        let cat = EntityId::new(EntityKind::Pet, "contract-pet-cat");
        ensure(store, &cat).await;

        let read = store
            .list_entities(Some(EntityKind::Pet))
            .await
            .expect("a listing of pets");
        let found = read
            .iter()
            .find(|e| e.id == cat)
            .expect("the pet comes back from a listing of its own kind");
        assert_eq!(
            found.kind,
            EntityKind::Pet,
            "and it reads back as a pet rather than as whatever a store defaults to: {found:?}",
        );

        let things = store
            .list_entities(Some(EntityKind::Thing))
            .await
            .expect("a listing of things");
        assert!(
            !things.iter().any(|e| e.id == cat),
            "a pet is not a thing, and the store's own filter agrees: {things:?}",
        );
    }

    pub async fn run_all<M: Memory>(store: &M) {
        capture_reads_back(store).await;
        preserves_all_fields(store).await;
        derived_from_must_name_a_fact_that_exists(store).await;
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
        update_fact_sets_and_clears_a_field(store).await;

        a_key_written_many_times_holds_one_value_and_counts(store).await;
        an_edit_appends_and_the_value_it_replaced_stays_in_the_history(store).await;
        clearing_a_key_leaves_its_writes_behind(store).await;
        history_of_an_unwritten_key_is_empty_and_of_no_entity_is_a_miss(store).await;

        a_records_fields_survive_capture(store).await;
        a_records_ref_is_screened_by_the_guard(store).await;
        a_reserved_field_key_is_refused(store).await;
        retracting_a_record_marks_it_and_records_why(store).await;
        a_retraction_is_one_way(store).await;
        a_retraction_needs_no_reason(store).await;
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
        add_entity_reports_a_near_miss_then_accepts_its_own_token(store).await;
        capture_requires_an_existing_subject(store).await;
        capture_requires_an_existing_edge_object(store).await;
        update_fact_requires_an_existing_edge_object(store).await;
        malformed_entity_fields_are_rejected(store).await;
        a_cross_link_takes_the_task_layers_own_grammar(store).await;

        a_declared_type_reads_back(store).await;
        declaring_a_type_again_replaces_its_keys(store).await;
        a_type_with_no_keys_is_refused_and_writes_nothing(store).await;
        a_shipped_type_refuses_a_callers_redeclaration(store).await;
        two_stored_types_may_name_one_key(store).await;
        a_stored_type_matches_a_record_that_never_declared_it(store).await;

        a_graph_query_selects_a_kind_and_returns_its_prose(store).await;
        a_graph_query_filters_on_a_stored_value_and_walks_an_edge(store).await;
        a_declared_reference_key_is_walkable_against_the_store(store).await;
        a_pet_is_its_own_kind_in_the_store(store).await;
        a_thing_reads_back_as_its_fields_folded(store).await;
        a_long_history_is_cut_to_its_newest_and_says_how_many(store).await;
    }
}
