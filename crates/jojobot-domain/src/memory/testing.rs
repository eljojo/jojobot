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
    FieldWrite, Guarded, MAX_KEY_CHARS, Memory, MemoryError, NewEntity, NewFact, Retraction,
    Standing, apply_entity_patch, apply_fact_patch,
    guard::{self, Decision},
    normalize_content, normalize_details, normalize_prose, retraction_of, screen_entity_patch,
    search, standing_of, validate_content, validate_details, validate_edge, validate_entity,
    validate_fields, validate_prose, validate_provenance_source, validate_subject,
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
    types: Mutex<Vec<crate::memory::types::DeclaredType>>,
    /// The kinds, one per token, with where each came from. Rows in the real
    /// store, so a `Vec` here for the same reason the types are one.
    kinds: Mutex<Vec<(String, crate::memory::types::Origin)>>,
    /// Which declarations are a kind's. The real store carries this on the row;
    /// here it is the same fact kept beside the rows.
    kind_keys: Mutex<std::collections::BTreeSet<String>>,
}

impl InMemoryMemory {
    /// A new fake, holding the shipped kinds as rows and **touching nothing
    /// outside itself**.
    ///
    /// **A store holds the kinds, so this holds them** — the shipped ones
    /// arrive as rows exactly as a boot writes them, in this object's own
    /// state.
    ///
    /// **It does NOT fill the set this process parses against.** That is a
    /// boot's step, not a store's, and the two were done here together: the
    /// constructor wrote a process-wide set, so building a fixture reached
    /// every case in the binary and a case could pass because of the order the
    /// cases ran. A double stands in for a store; it does not stand in for a
    /// startup. Ask for [`InMemoryMemory::booted`] when the case needs the set.
    pub fn new() -> Self {
        let fake = Self::default();
        *fake.kinds.lock().unwrap() = crate::memory::kinds::SHIPPED
            .iter()
            .map(|token| (token.to_string(), crate::memory::types::Origin::Shipped))
            .collect();
        fake
    }

    /// **A store that has been booted** — the fake, plus the step a startup
    /// takes after standing one up: the set this process parses against is
    /// filled from what this store holds.
    ///
    /// **This is the boundary, and it is explicit on purpose.** A case that
    /// parses a handle needs the set the way it needs a runtime, and asking for
    /// a booted store is how it says so. The constructor cannot do it: filling
    /// a process-wide set as a side effect of building an object makes every
    /// case in the binary depend on which case built one first.
    ///
    /// **Filled from what this store HOLDS, never from [`kinds::SHIPPED`]**, so
    /// a case cannot be green because a constant was in scope, and a kind a
    /// caller declared on this store is parsed here as it would be after a real
    /// boot.
    ///
    /// It is synchronous where [`kinds::seed`] and [`kinds::reload`] are not,
    /// because this store answers from memory and needs no runtime to say what
    /// it holds. The rails with a real store keep those two, which are the same
    /// two steps: write the rows, then load what came back.
    ///
    /// [`kinds::seed`]: crate::memory::kinds::seed
    /// [`kinds::reload`]: crate::memory::kinds::reload
    /// [`kinds::SHIPPED`]: crate::memory::kinds::SHIPPED
    pub fn booted() -> Self {
        let fake = Self::new();
        fake.boot();
        fake
    }

    /// **Boot a store that is already standing** — the second of the two steps,
    /// on its own, for a case that has to write rows before the set is filled.
    ///
    /// [`booted`](Self::booted) is this call on a fresh store, and takes the
    /// same shape a real startup does: stand the store up, then load the set
    /// from what it answers.
    pub fn boot(&self) {
        let held: Vec<String> = self
            .kinds
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .map(|(token, _)| token.clone())
            .collect();
        crate::memory::kinds::load(held);
    }

    /// **A kind's keys, in the same place a schema's keys live.** Declaring
    /// with no keys leaves none, so a kind that names nothing is representable
    /// while a schema that names nothing still is not — the kind's identity is
    /// its row, not its keys.
    fn keys_of_kind(
        &self,
        token: &str,
        origin: crate::memory::types::Origin,
        fields: Vec<crate::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        use crate::memory::types::DeclaredType;
        // **Naming no keys is not taking every key away**, and neither half of
        // the sentence may write over the other's — the real store holds both
        // lines, so the double does too.
        if fields.is_empty() {
            return Ok(());
        }
        let mut types = self.types.lock().unwrap();
        if types
            .iter()
            .any(|held| held.name == token && !self.kind_keys.lock().unwrap().contains(&held.name))
        {
            return Err(MemoryError::InvalidEntity(format!(
                "'{token}' already names a declared type, and its keys are not this \
                 declaration's to replace"
            )));
        }
        types.retain(|held| held.name != token);
        types.push(DeclaredType {
            name: token.to_string(),
            fields,
            origin,
        });
        self.kind_keys.lock().unwrap().insert(token.to_string());
        Ok(())
    }

    /// **The keys a KIND names, read as the kind's own.**
    ///
    /// The two halves share one place and are told apart by who declared them.
    /// [`Self::declarations`] reads both, which is right for the fold — how a
    /// key folds is declared by whoever declared it — and wrong for the fit
    /// guard, which selects a declaration whose NAME matches the thing's kind.
    /// Handed the mixed list, that guard reads a caller's type named `place` as
    /// the kind `place`'s schema and gates every write to every place with it.
    ///
    /// **The real store filters on its owner column here, so this filters on
    /// the same fact.** A double that hands the guard more than the store does
    /// accepts writes the store refuses, and every case written against it is
    /// blind to that class until it lands.
    fn kind_keys_of(&self, kind: &str) -> Vec<crate::memory::types::DeclaredType> {
        // types before kind_keys, the order `keys_of_kind` takes them in.
        let types = self.types.lock().expect("fake mutex poisoned");
        let owned = self.kind_keys.lock().expect("fake mutex poisoned");
        types
            .iter()
            .filter(|held| held.name == kind && owned.contains(&held.name))
            .cloned()
            .collect()
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

    /// **What a thing holds: every write on it, handed to the one fold.**
    ///
    /// The status comes off the record that carried each write, so a write
    /// inside a record somebody took back is still there and no longer counts —
    /// the same join the real store does, done here so the two cannot come to
    /// disagree about what a thing is.
    fn held(&self, entity: &EntityId) -> std::collections::BTreeMap<String, String> {
        let facts = self.facts.lock().expect("fake mutex poisoned");
        super::folded_fields(&self.writes_on(entity, &facts), &self.declarations())
    }

    /// What has been declared — which is what says how each key folds.
    fn declarations(&self) -> Vec<crate::memory::types::DeclaredType> {
        self.types.lock().expect("fake mutex poisoned").clone()
    }

    /// **Every write on this thing, each with the standing of the record that
    /// carried it** — the substrate the fold and the write guard both read.
    ///
    /// The records are passed in rather than locked here, because the write
    /// path reads this while it is already holding them.
    fn writes_on(&self, entity: &EntityId, facts: &[Fact]) -> Vec<super::KeyWrite> {
        let writes = self.writes.lock().expect("fake mutex poisoned");
        writes
            .iter()
            .filter(|w| &w.entity == entity)
            .filter_map(|w| {
                let carried = facts
                    .iter()
                    .find(|f| f.home == w.entity && f.id == w.fact)?;
                Some(super::KeyWrite {
                    key: w.key.clone(),
                    ordinal: w.ordinal,
                    value: w.value.clone(),
                    fact: w.fact.clone(),
                    status: carried.status,
                    provenance: carried.provenance,
                    standing: carried.standing,
                })
            })
            .collect()
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
        validate_provenance_source(fact.provenance, &fact.fields)?;
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
        // **A reference key names an entity, so it faces the same rule the
        // edge's object faces** — beside the real store's copy, because a rule
        // that holds in one adapter holds until somebody switches adapters.
        for object in super::referenced_by(
            &fact.fields,
            &self.types.lock().expect("fake mutex poisoned"),
        ) {
            if let Decision::Block(candidates) = guard::decide_existing(&object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object,
                    candidates,
                });
            }
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
        // A withdrawn claim is still there, and it is no longer evidence.
        if let Some(source) = &fact.derived_from
            && facts.iter().any(|f| {
                f.home == source.home && f.id == source.local && f.status == FactStatus::Retracted
            })
        {
            return Err(MemoryError::SourceRetracted {
                attempted: source.to_string(),
            });
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
            // **A store stamps this, so the double does too.** A fake that left
            // it empty would let every case above it pass on a build where the
            // real store's stamp never happens.
            inserted_at: Some(jiff::Timestamp::now()),
            stale_after: fact.stale_after,
        };
        // **A new record's keys land on the thing too** — the same guard the
        // edit path runs, because a thing's fields are every write on it
        // folded. Run here beside the real store's copy, so a rule that held in
        // one adapter and not the other cannot ship.
        let writes = self.writes_on(&stored.home, &facts);
        let declared = self.types.lock().expect("fake mutex poisoned").clone();
        // **The fold reads both halves and the guard reads one.** How a key
        // folds is declared by whoever declared it; what governs a thing is its
        // own kind, and nothing else.
        let governs = self.kind_keys_of(stored.home.kind_token());
        super::guard_fit(
            stored.home.kind_token(),
            &super::folded_fields(&writes, &declared),
            &super::stood_after_capture(&writes, &stored, &declared),
            &governs,
        )?;
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

    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let index = self.index();
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        }
        Ok(self.held(entity))
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
                    provenance: carried.provenance,
                    standing: carried.standing,
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
        // **The same rule on the edit path**, over the keys this patch sets:
        // a reference names an entity, and nothing a write names is created as
        // a side effect of being named.
        for object in super::referenced_by(
            &patch.fields,
            &self.types.lock().expect("fake mutex poisoned"),
        ) {
            if let Decision::Block(candidates) = guard::decide_existing(&object, &self.index()) {
                return Ok(Guarded::Blocked {
                    attempted: object,
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
        let Some(found) = facts
            .iter()
            .find(|f| f.home == address.home && f.id == address.local)
            .cloned()
        else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        };
        let fact = &found;
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
        // What the ADDRESSED RECORD carries right now — kept before the patch
        // rewrites it, because that is what decides which of the patch's clears
        // is a write and which names a key this record never had.
        let carried = edited.fields.clone();
        // A source named by an edit faces the capture rule: a link at a claim
        // nobody wrote reads as evidence and leads nowhere.
        if let Some(source) = &patch.derived_from
            && !facts
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
        apply_fact_patch(&mut edited, &patch)?;
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now** — the same guard the real store runs, so the two
        // cannot come to disagree about what a write may cost.
        let home = fact.home.clone();
        let writes = self.writes_on(&home, &facts);
        let declared = self.declarations();
        let before = super::folded_fields(&writes, &declared);
        let after = super::stood_after(&writes, &edited, &patch, &carried, &declared);
        let governs = self.kind_keys_of(home.kind_token());
        super::guard_fit(home.kind_token(), &before, &after, &governs)?;
        let id = fact.id.clone();
        for held in facts.iter_mut() {
            if held.home == home && held.id == id {
                *held = Fact {
                    fields: Default::default(),
                    ..edited.clone()
                };
            }
        }
        drop(facts);
        self.append_writes(&home, &id, super::writes_of(&patch, &carried));
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
            inserted_at: Some(jiff::Timestamp::now()),
            stale_after: None,
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
        let declared = self.declarations();
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
                    // The scan carries what the thing IS, because the records
                    // it also carries cannot be folded back into it.
                    fields: super::folded_fields(&self.writes_on(&entity.id, &facts), &declared),
                    entity: Some(entity),
                }
            }))
            .collect())
    }

    async fn declare_type(
        &self,
        declared: crate::memory::types::DeclaredType,
    ) -> Result<crate::memory::types::DeclaredType, MemoryError> {
        crate::memory::types::validate_type(&declared)?;
        let declared = declared.normalized();
        let mut held = self.types.lock().unwrap();
        crate::memory::types::guard_replacement(
            &declared,
            held.iter()
                .find(|t| t.name == declared.name)
                .map(|t| t.origin),
        )?;
        // **Neither half writes over the other's keys**, and this is the same
        // line `keys_of_kind` holds from the other side: the two share this
        // place and the name alone cannot say which of them wrote a row.
        if self
            .kind_keys
            .lock()
            .expect("fake mutex poisoned")
            .contains(&declared.name)
        {
            return Err(MemoryError::InvalidEntity(format!(
                "'{}' already names a kind, and its keys are not this declaration's to replace",
                declared.name
            )));
        }
        // Replaced whole, the way the real store replaces the rows sharing the
        // name: a type is the keys it names now.
        held.retain(|t| t.name != declared.name);
        held.push(declared.clone());
        Ok(declared)
    }

    async fn declared_types(&self) -> Result<Vec<crate::memory::types::DeclaredType>, MemoryError> {
        Ok(self.types.lock().unwrap().clone())
    }

    async fn declare_kind(
        &self,
        token: &str,
        origin: crate::memory::types::Origin,
        fields: Vec<crate::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        use crate::memory::types::Origin;
        let mut kinds = self.kinds.lock().unwrap();
        if let Some((held, held_origin)) = kinds.iter_mut().find(|(held, _)| held == token) {
            if *held_origin == Origin::Shipped && origin == Origin::Declared {
                return Err(MemoryError::InvalidEntity(format!(
                    "'{held}' is a kind the software ships, and a caller cannot redeclare one"
                )));
            }
            *held_origin = origin;
            drop(kinds);
            return self.keys_of_kind(token, origin, fields);
        }
        kinds.push((token.to_string(), origin));
        drop(kinds);
        self.keys_of_kind(token, origin, fields)
    }

    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, crate::memory::types::Origin)>, MemoryError> {
        Ok(self.kinds.lock().unwrap().clone())
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
    use crate::memory::{Boot, Edge, EdgeShape, FACTS_HEADER, FactStatus, Provenance, RETRACTS};
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
        // A rhythm is refused without a parent, so provisioning one provisions
        // the thing it is a loop on. The owner is a plain entity of the
        // fixture's own, which keeps the rule the store enforces out of the way
        // of cases that are about something else.
        let parent = if id.kind() == Some(EntityKind::RHYTHM) {
            let owner = EntityId::new(EntityKind::THING, format!("{}-owner", id.slug()));
            if !known.iter().any(|e| e.id == owner) {
                add(
                    store,
                    NewEntity::new(owner.clone(), owner.slug(), "contract-fixture"),
                )
                .await;
            }
            Some(owner)
        } else {
            None
        };
        add(
            store,
            NewEntity {
                parent,
                ..NewEntity::new(id.clone(), id.slug(), "contract-fixture")
            },
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

    /// **What the thing IS**, read the way the served answer reads it: one
    /// dense row, folded, off the store rather than off records the caller
    /// folded for itself.
    async fn thing_fields<M: Memory>(
        store: &M,
        id: &EntityId,
    ) -> std::collections::BTreeMap<String, String> {
        graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(id.clone()),
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
        .expect("a handle is a selection")
        .first()
        .unwrap_or_else(|| panic!("a walk from {id} must answer with the thing itself"))
        .fields
        .clone()
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
    /// The surface teaches that everything a write names must already exist,
    /// and a claim is named as surely as an entity is. A field that takes any
    /// well-formed address without looking makes a link to a claim that never
    /// existed — a citation to nothing, and the reader it fails is a later
    /// session following the provenance chain, which is the whole reason the
    /// field is there.
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
            stale_after: None,
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
        let id = EntityId::new(EntityKind::PROJECT, "contract-atlas");
        let added = add(
            store,
            NewEntity {
                crm: Some("card:874".into()),
                boot: Boot::Always,
                ..NewEntity::new(id.clone(), "Atlas", "user-named")
            },
        )
        .await;
        assert_eq!(added.kind, EntityKind::PROJECT);

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
    /// **Who points here, read from the far end.**
    ///
    /// A reference key makes a value a link, and a store has to answer that
    /// link from the thing it points AT — otherwise the only way to find the
    /// records naming something is to read every record there is.
    ///
    /// Every store answers the same three ways: a record naming the handle
    /// comes back whatever key it used, a record naming something else does
    /// not, and a key that was overwritten stops pointing. **The third is what
    /// separates reading the writes from reading the records**: the write row
    /// that named the old target is still in the store, and the record no
    /// longer carries it.
    /// **The two clocks, and they are allowed to disagree.**
    ///
    /// A claim's date says when it is TRUE OF. The stamp says when this store
    /// took it in. Neither is checked against the other, because both
    /// disagreements are ordinary: a booking is true of a day that has not
    /// arrived, and a backfill carries years of records that are true of days
    /// long past and land this morning.
    ///
    /// **The store writes the stamp.** Nothing above it can, which is what
    /// makes *jojobot knew this then* something nobody can claim after the
    /// fact.
    pub async fn a_claim_carries_when_it_was_taken_in<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::PERSON, "contract-clocks");
        add(
            store,
            NewEntity::new(subject.clone(), "Contract Clocks", "contract-fixture"),
        )
        .await;

        let before = jiff::Timestamp::now();
        // True of a day long past, taken in now: the backfill.
        let backfilled = capture(
            store,
            NewFact::about(subject.clone(), "moved here", date(2022, 3, 1)),
        )
        .await;
        // True of a day that has not arrived, taken in now: the booking.
        let booked = capture(
            store,
            NewFact::about(subject.clone(), "flies out", date(2027, 6, 12)),
        )
        .await;
        let after = jiff::Timestamp::now();

        for (what, fact, held) in [
            ("the backfilled claim", &backfilled, date(2022, 3, 1)),
            ("the booking", &booked, date(2027, 6, 12)),
        ] {
            assert_eq!(fact.date, held, "{what} lost the day it is true of");
            let stamp = fact
                .inserted_at
                .unwrap_or_else(|| panic!("{what} came back with no stamp: {fact:?}"));
            assert!(
                before <= stamp && stamp <= after,
                "{what} was stamped outside the call that wrote it: {stamp}",
            );
        }

        // **Read back, because a stamp the write invented and the store did not
        // keep is not a stamp.**
        let held = store
            .recall(&subject)
            .await
            .expect("the claims read back")
            .into_iter()
            .map(|fact| (fact.content, fact.date, fact.inserted_at))
            .collect::<Vec<_>>();
        assert_eq!(
            held,
            vec![
                (
                    "moved here".to_string(),
                    date(2022, 3, 1),
                    backfilled.inserted_at
                ),
                (
                    "flies out".to_string(),
                    date(2027, 6, 12),
                    booked.inserted_at
                ),
            ],
            "a read gives back both clocks, unchanged",
        );

        // **An edit does not re-stamp.** Correcting a claim's wording is not
        // the moment this store learned it.
        let edited = edit(
            store,
            &backfilled.address(),
            FactPatch {
                content: Some("moved here in the spring".to_string()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            edited.inserted_at, backfilled.inserted_at,
            "an edit re-stamped the record with the moment somebody corrected it",
        );

        // **The day somebody set to look again survives storage**, and comes
        // off when an edit says so. It is the caller's judgement, unlike the
        // stamp above, so a caller may move it and remove it.
        let watched = capture(
            store,
            NewFact {
                stale_after: Some(date(2026, 12, 1)),
                ..NewFact::about(
                    subject.clone(),
                    "the rate is fixed for now",
                    date(2026, 8, 1),
                )
            },
        )
        .await;
        assert_eq!(watched.stale_after, Some(date(2026, 12, 1)));
        assert_eq!(
            read_back(store, &subject, &watched.id).await.stale_after,
            Some(date(2026, 12, 1)),
            "the day did not survive the store",
        );
        let cleared = edit(
            store,
            &watched.address(),
            FactPatch {
                clear_stale_after: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            cleared.stale_after, None,
            "a claim nobody has to look at again still carries a day",
        );

        // **A retraction is a record in its own right**, so it is taken in at
        // the moment it is written — and the record it takes back keeps the
        // moment IT was taken in, because a retraction says a claim should not
        // have been recorded, never that it arrived later than it did.
        let taken_back = store
            .retract(
                &watched.address(),
                Some("written in error"),
                date(2026, 8, 2),
            )
            .await
            .expect("the retraction lands");
        assert!(
            taken_back.record.inserted_at.is_some(),
            "the account of a retraction carries no stamp: {:?}",
            taken_back.record,
        );
        assert_eq!(
            taken_back.retracted.inserted_at, watched.inserted_at,
            "taking a claim back re-stamped it with the moment somebody took it back",
        );
    }

    /// **Lineage, walked from the source's end.**
    ///
    /// A claim names what it was worked out from; this is the question nobody
    /// could ask — **what was built on this** — and it is asked the moment a
    /// claim is taken back.
    ///
    /// Three legs, and the second two are what make the first mean anything: a
    /// claim derived from this one is found · a claim about the same subject
    /// that was derived from nothing is NOT found there · and **a claim that
    /// stops resting on it stops being found**, which no test that only adds
    /// records can catch.
    /// **A folded value says which claim it came from and who backs it.**
    ///
    /// The same string arrives whether the user said it this morning or an
    /// assistant guessed it two years ago, and a reader that cannot tell them
    /// apart has to treat both alike. **Under the default fold exactly one
    /// write wins**, so the honest answer is that claim's own certainty.
    ///
    /// Three legs. The winning write's certainty is reported · the loser's is
    /// NOT, which is what stops this passing on a build that reports whatever
    /// it finds first · and **the answer changes when the winning write
    /// changes**, which no test that only adds one claim can reach.
    /// **A claim an agent read out of a system of record, and what it costs to
    /// say so.**
    ///
    /// It is neither the user's word nor a guess: filing a statement as a
    /// derivation makes a system of record read as a hypothesis, and filing it
    /// as testimony puts words in somebody's mouth. **What makes it usable a
    /// year later is the attribution**, so the claim is refused when it names
    /// no system — the check is on the source and never on the standing.
    ///
    /// Four legs, and each of the last three is why the first means anything:
    /// unattributed is refused · attributed lands and **reads back settled**,
    /// which is the whole point of the value · an ordinary derivation with no
    /// source is untouched, so the rule reaches only the claims it is about ·
    /// and **a claim that stops being machine-read stops being held to it**.
    pub async fn a_machine_read_claim_names_what_it_was_read_from<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::PERSON, "contract-observed");
        add(
            store,
            NewEntity::new(subject.clone(), "Contract Observed", "contract-fixture"),
        )
        .await;

        let unattributed = store
            .capture(NewFact {
                provenance: Provenance::Observation,
                ..NewFact::about(subject.clone(), "the invoice is paid", date(2026, 8, 1))
            })
            .await
            .expect_err("a machine-read claim with no source is refused");
        assert!(
            matches!(unattributed, MemoryError::UnsourcedObservation),
            "the refusal is about something else: {unattributed:?}",
        );

        let read = capture(
            store,
            NewFact {
                provenance: Provenance::Observation,
                fields: [
                    ("read_from".to_string(), "the ledger app".to_string()),
                    ("read_ref".to_string(), "invoice-4471".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(subject.clone(), "the invoice is paid", date(2026, 8, 1))
            },
        )
        .await;
        assert_eq!(read.provenance, Provenance::Observation);
        assert_eq!(
            read.standing,
            Standing::Settled,
            "a confident read of a system of record reads back as a hypothesis",
        );
        assert_eq!(
            read_back(store, &subject, &read.id).await.provenance,
            Provenance::Observation,
            "the provenance did not survive the store",
        );

        // **The rule reaches only the claims it is about.** An ordinary
        // derivation names no source and is written as it always was — without
        // this, the refusal above passes against a store demanding a source
        // from everything.
        capture(
            store,
            NewFact::about(
                subject.clone(),
                "so the account is square",
                date(2026, 8, 2),
            ),
        )
        .await;

        // **The leg a change reaches.** Edit the machine-read claim down to a
        // derivation and the source stops being required: what is held to the
        // rule is the claim's provenance now, not the one it was written with.
        let softened = edit(
            store,
            &read.address(),
            FactPatch {
                provenance: Some(Provenance::Inference),
                clear_fields: vec!["read_from".to_string(), "read_ref".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert_eq!(softened.provenance, Provenance::Inference);
        assert!(
            !softened.fields.contains_key("read_from"),
            "the source stayed on a claim that is no longer machine-read: {softened:?}",
        );

        // **And the leg that reaches the same change the other way.** The rule
        // is about what a claim IS, so it binds every path that can make one a
        // machine read — not only the one that writes it first. A guard on
        // capture alone lets a guess become a system read of a system nobody
        // named, and the value outranks the honest guesses beside it.
        let guessed = capture(
            store,
            NewFact::about(
                subject.clone(),
                "the balance looks settled",
                date(2026, 8, 3),
            ),
        )
        .await;
        let hardened = store
            .update_fact(
                &guessed.address(),
                FactPatch {
                    provenance: Some(Provenance::Observation),
                    ..Default::default()
                },
            )
            .await
            .expect_err("a claim moved to a machine read with no source is refused");
        assert!(
            matches!(hardened, MemoryError::UnsourcedObservation),
            "the refusal is about something else: {hardened:?}",
        );
        assert_eq!(
            read_back(store, &subject, &guessed.id).await.provenance,
            Provenance::Inference,
            "the refused edit moved the claim anyway",
        );

        // The positive it rests on: the same move, naming the system. Without
        // this the refusal passes on a store that turns every edit away.
        let attributed = edit(
            store,
            &guessed.address(),
            FactPatch {
                provenance: Some(Provenance::Observation),
                fields: [("read_from".to_string(), "the ledger app".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(attributed.provenance, Provenance::Observation);

        // **A claim that already names its system may be moved without naming
        // it again.** What the rule protects is an attribution ON THE RECORD,
        // and this record has one — so the question is asked of the claim as it
        // will stand, never of the patch alone.
        let softened_again = edit(
            store,
            &attributed.address(),
            FactPatch {
                provenance: Some(Provenance::Inference),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(softened_again.provenance, Provenance::Inference);
        let re_hardened = edit(
            store,
            &attributed.address(),
            FactPatch {
                provenance: Some(Provenance::Observation),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            re_hardened.provenance,
            Provenance::Observation,
            "a claim carrying its source already does not have to name it twice",
        );

        // **And the same rule reached from the fields.** Taking the source off
        // a claim that stays a machine read leaves exactly the state the rule
        // forbids, so it is refused for the same reason.
        let stripped = store
            .update_fact(
                &attributed.address(),
                FactPatch {
                    clear_fields: vec!["read_from".to_string()],
                    ..Default::default()
                },
            )
            .await
            .expect_err("taking the source off a machine read is refused");
        assert!(
            matches!(stripped, MemoryError::UnsourcedObservation),
            "the refusal is about something else: {stripped:?}",
        );
    }

    pub async fn a_folded_value_says_who_backs_it<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::PERSON, "contract-backing");
        add(
            store,
            NewEntity::new(subject.clone(), "Contract Backing", "contract-fixture"),
        )
        .await;

        // A guess first, then the user's own word: the newest write wins, and
        // it is the one whose certainty the answer must carry.
        capture(
            store,
            NewFact {
                provenance: Provenance::Inference,
                fields: [("rent".to_string(), "900".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    subject.clone(),
                    "worked it out from the listing",
                    date(2026, 5, 1),
                )
            },
        )
        .await;
        let stated = capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                standing: Some(Standing::Settled),
                fields: [("rent".to_string(), "950".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    subject.clone(),
                    "he said what the rent is",
                    date(2026, 6, 1),
                )
            },
        )
        .await;

        let backing = |entity: EntityId| async move {
            store
                .backing(&entity)
                .await
                .expect("a store says what backs a folded value")
        };
        let held = backing(subject.clone()).await;
        let rent = held
            .get("rent")
            .unwrap_or_else(|| panic!("the folded value says nothing about its claim: {held:?}"));
        assert_eq!(rent.fact, stated.id, "the backing names the losing claim");
        assert_eq!(
            (rent.provenance, rent.standing),
            (Provenance::Testimony, Standing::Settled),
            "the value the user stated reads as a guess, or the other way about",
        );

        // **The leg only a change reaches.** A later guess wins the key, and the
        // certainty reported moves with it — a build that read the claim once
        // would go on reporting testimony for a value nobody stated.
        capture(
            store,
            NewFact {
                provenance: Provenance::Inference,
                fields: [("rent".to_string(), "975".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    subject.clone(),
                    "the listing went up again",
                    date(2026, 7, 1),
                )
            },
        )
        .await;
        let held = backing(subject.clone()).await;
        let rent = held.get("rent").expect("the key is still held");
        assert_eq!(
            (rent.provenance, rent.standing),
            (Provenance::Inference, Standing::Open),
            "the certainty did not move with the write that won: {held:?}",
        );
    }

    pub async fn a_claims_lineage_is_walkable_from_its_source<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::PERSON, "contract-lineage");
        add(
            store,
            NewEntity::new(subject.clone(), "Contract Lineage", "contract-fixture"),
        )
        .await;
        let source = capture(
            store,
            NewFact::about(
                subject.clone(),
                "the ferry moved to the north pier",
                date(2026, 4, 1),
            ),
        )
        .await;
        let other = capture(
            store,
            NewFact::about(
                subject.clone(),
                "the bridge is closed on Sundays",
                date(2026, 4, 1),
            ),
        )
        .await;
        let built = capture(
            store,
            NewFact {
                derived_from: Some(source.address()),
                ..NewFact::about(
                    subject.clone(),
                    "so the crossing is longer",
                    date(2026, 4, 2),
                )
            },
        )
        .await;

        let standing_on = |address: FactAddress| async move {
            store
                .built_on(&address)
                .await
                .expect("a store answers what was built on a claim")
                .into_iter()
                .map(|fact| fact.address().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            standing_on(source.address()).await,
            vec![built.address().to_string()],
            "the claim built on this one is not found from its source's end",
        );
        assert!(
            standing_on(other.address()).await.is_empty(),
            "a claim nobody built on answers with the claims built on something else",
        );

        // **The leg that only a change can prove.** Point the derived claim at
        // the other source and it must leave the first one's answer — a walk
        // that read the pointer once and never again would keep it there.
        edit(
            store,
            &built.address(),
            FactPatch {
                derived_from: Some(other.address()),
                ..Default::default()
            },
        )
        .await;
        assert!(
            standing_on(source.address()).await.is_empty(),
            "a claim that stopped resting on this one is still found under it",
        );
        assert_eq!(
            standing_on(other.address()).await,
            vec![built.address().to_string()],
            "…and it is not found under the source it now names",
        );

        // **A claim that was taken back cannot be what another claim rests
        // on.** It is still there — retraction is a state, not a deletion — so
        // this is not the missing-source refusal: it is the same claim, no
        // longer able to serve as evidence.
        let withdrawn = capture(
            store,
            NewFact::about(subject.clone(), "the pier is open again", date(2026, 4, 3)),
        )
        .await;
        store
            .retract(
                &withdrawn.address(),
                Some("misread the notice"),
                date(2026, 4, 4),
            )
            .await
            .expect("the retraction lands");
        let refused = store
            .capture(NewFact {
                derived_from: Some(withdrawn.address()),
                ..NewFact::about(
                    subject.clone(),
                    "so the crossing is short again",
                    date(2026, 4, 5),
                )
            })
            .await
            .expect_err("a claim resting on a withdrawn one is refused");
        assert!(
            matches!(refused, MemoryError::SourceRetracted { .. }),
            "a claim was allowed to rest on one that had been taken back: {refused:?}",
        );

        // **The positive it depends on**: the same claim goes through when it
        // names a source that still stands. Without this, the refusal above
        // passes against a store that refuses every lineage pointer.
        capture(
            store,
            NewFact {
                derived_from: Some(other.address()),
                ..NewFact::about(
                    subject.clone(),
                    "so the timetable changed",
                    date(2026, 4, 5),
                )
            },
        )
        .await;
    }

    pub async fn referring_to_answers_from_the_far_end<M: Memory>(store: &M) {
        let gate = EntityId::new(EntityKind::EVENT, "contract-winter-fest");
        let other = EntityId::new(EntityKind::EVENT, "contract-leaving-party");
        let holder = EntityId::new(EntityKind::PERSON, "contract-milhouse");
        let bystander = EntityId::new(EntityKind::PERSON, "contract-otto");
        for (id, name) in [
            (&gate, "Contract Winter Fest"),
            (&other, "Contract Leaving Party"),
            (&holder, "Contract Milhouse"),
            (&bystander, "Contract Otto"),
        ] {
            add(store, NewEntity::new(id.clone(), name, "contract-fixture")).await;
        }

        let held = capture(
            store,
            NewFact {
                fields: [("admits".to_string(), gate.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(holder.clone(), "holds a full pass", date(2026, 8, 1))
            },
        )
        .await;
        capture(
            store,
            NewFact {
                fields: [("admits".to_string(), other.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    bystander.clone(),
                    "holds a pass to the other one",
                    date(2026, 8, 1),
                )
            },
        )
        .await;

        let pointing = store
            .referring_to(&gate)
            .await
            .expect("a store answers who points here");
        assert_eq!(
            pointing
                .iter()
                .map(|fact| fact.address().to_string())
                .collect::<Vec<_>>(),
            vec![held.address().to_string()],
            "the record naming this handle comes back, and the one naming another does not",
        );

        // **A key pointed elsewhere stops pointing here.** The write that named
        // the gate is still in the store; the record is not carrying it any
        // more, and that is what the answer follows.
        edit(
            store,
            &held.address(),
            FactPatch {
                fields: [("admits".to_string(), other.to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
        assert!(
            store
                .referring_to(&gate)
                .await
                .expect("a store answers who points here")
                .is_empty(),
            "a key that was pointed somewhere else still points here",
        );

        // **Every status comes back, retracted included**, because this read
        // serves history as often as current truth. **A reader of current truth
        // has to filter for itself**, and it can only do that if the store says
        // what state each record is in rather than deciding for it.
        let withdrawn = capture(
            store,
            NewFact {
                fields: [("admits".to_string(), gate.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(bystander.clone(), "was given a pass", date(2026, 8, 1))
            },
        )
        .await;
        store
            .retract(&withdrawn.address(), Some("never issued"), date(2026, 8, 2))
            .await
            .expect("the retraction lands");
        let after = store
            .referring_to(&gate)
            .await
            .expect("a store answers who points here");
        let taken_back = after
            .iter()
            .find(|fact| fact.id == withdrawn.id)
            .unwrap_or_else(|| panic!("the withdrawn record is not in the answer: {after:?}"));
        assert_eq!(
            taken_back.status,
            FactStatus::Retracted,
            "the answer hides what state a record is in, so no reader above can filter on it",
        );
    }

    pub async fn a_child_names_its_parent_and_reads_back<M: Memory>(store: &M) {
        let parent = EntityId::new(EntityKind::PROJECT, "contract-monorail");
        let child = EntityId::new(EntityKind::PROJECT, "contract-monorail-funding");

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
        let root = EntityId::new(EntityKind::PROJECT, "contract-springfield");
        let track = EntityId::new(EntityKind::PROJECT, "contract-springfield-track");
        let cars = EntityId::new(EntityKind::PROJECT, "contract-springfield-cars");
        let brakes = EntityId::new(EntityKind::PROJECT, "contract-springfield-brakes");

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
        let parent = EntityId::new(EntityKind::PROJECT, "contract-kwik-e");
        let child = EntityId::new(EntityKind::PROJECT, "contract-kwik-e-squishee");
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
        let child = EntityId::new(EntityKind::PROJECT, "contract-bad-parent");
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
        let known = EntityId::new(EntityKind::PROJECT, "contract-ghost-parent");
        add(
            store,
            NewEntity::new(known.clone(), "Contract Ghost Parent", "contract-fixture"),
        )
        .await;

        let typo = EntityId::new(EntityKind::PROJECT, "contract-ghost-parnt");
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
        let real = EntityId::new(EntityKind::PROJECT, "contract-plant");
        let typo = EntityId::new(EntityKind::PROJECT, "contract-plnt");
        // Named so it resembles neither the real parent nor the typo: this
        // case is about the PARENT gate, and a child that tripped the screen on
        // its own handle first would report a block about itself.
        let child = EntityId::new(EntityKind::PROJECT, "contract-shift-rota");
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
        let ouroboros = EntityId::new(EntityKind::PROJECT, "contract-ouroboros");
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
        let bot = EntityId::new(EntityKind::BOT, "contract-epsilon");
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
        let ghost = EntityId::new(EntityKind::BOT, "contract-ghost-bot");
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
        let place = EntityId::new(EntityKind::PLACE, "contract-north-trail");
        let topic = EntityId::new(EntityKind::TOPIC, "contract-widgets");
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
            .list_entities(Some(EntityKind::PLACE))
            .await
            .expect("list places");
        assert!(places.iter().all(|e| e.kind == EntityKind::PLACE));
        assert!(places.iter().any(|e| e.id == place));
        assert!(
            !places.iter().any(|e| e.id == topic),
            "a topic must not appear in the place listing"
        );
    }

    /// An entity's metadata edits in place; the handle is untouched.
    pub async fn update_entity_edits_metadata_in_place<M: Memory>(store: &M) {
        let id = EntityId::new(EntityKind::THING, "contract-red-bike");
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
        let id = EntityId::new(EntityKind::ORG, "contract-self-labelled");
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
        let first = EntityId::new(EntityKind::ORG, "contract-unscreened");
        add(
            store,
            NewEntity::new(first.clone(), "Unscreened Org", "user-named"),
        )
        .await;
        // A second entity that legitimately shares the name — settled once, at
        // creation, over that refusal's own token. That settlement must not be
        // re-litigated by a patch that touches no label at all.
        let twin = EntityId::new(EntityKind::ORG, "contract-unscreened-twin");
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
        let ghost = EntityId::new(EntityKind::THING, "contract-red-bikee");
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
    /// The alternative — a `negated` flag beside the disproved claim — is the
    /// "was wrong, see flag" shape: it leaves two versions on the page for the
    /// reader to adjudicate, and hides the correction from every default
    /// search, which is precisely where it is needed.
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
    /// and both stored. One field for the two questions makes a session pick
    /// which of them to be wrong about.
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
    /// A hedge the operator wrote is testimony from the start, so there is no
    /// provenance for a confirmation to promote. What confirmation closes is
    /// the standing, and there must still be something for it to close. A
    /// promotion that "works" here is one reading a hedge stored as inference,
    /// which is a claim mis-stored rather than a claim settled.
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
            EntityId::new(EntityKind::PLACE, "contract-far-country"),
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
            (EdgeShape::Location, EntityKind::PLACE),
            (EdgeShape::Membership, EntityKind::ORG),
            (EdgeShape::Attendance, EntityKind::EVENT),
            (EdgeShape::About, EntityKind::TOPIC),
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
        let object = EntityId::new(EntityKind::PLACE, "contract-riverbend");
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

        let typo = EntityId::new(EntityKind::PLACE, "contract-riverbnd");
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

        assert_eq!(
            store
                .fields(&subject)
                .await
                .expect("the fields of a thing should succeed")
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

    /// **A key declared a counter reads back as the total of its writes, and
    /// the writes are all still there.**
    ///
    /// The declaration is what buys it, so this asserts the fold both ways in
    /// one case: the counter adds, and an undeclared key written the same
    /// number of times on the same thing still takes its newest write. A case
    /// that only looked at the counter would pass on a store that sums every
    /// number it holds.
    ///
    /// It also reads the declaration back, because the fold travels through the
    /// store's own type rows: a store that dropped the column would keep every
    /// key on newest-wins and give no other sign of it.
    pub async fn a_counter_totals_its_writes_and_keeps_them<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-totalled");
        store
            .declare_type(crate::memory::types::DeclaredType::new(
                "contract-snacking",
                vec![
                    crate::memory::types::Field::summing("donuts").needed(),
                    crate::memory::types::Field::required(
                        "mood",
                        crate::memory::types::ValueType::Text,
                    ),
                ],
            ))
            .await
            .expect("declaring a type should succeed");
        assert_eq!(
            store
                .declared_types()
                .await
                .expect("the roster reads")
                .iter()
                .find(|t| t.name == "contract-snacking")
                .and_then(|t| t.field("donuts"))
                .map(|f| f.folds),
            Some(crate::memory::types::Fold::Sum),
            "the fold survives the store, or nothing below is about a counter"
        );

        for (nth, mood) in [(1, "hopeful"), (2, "content"), (3, "queasy")] {
            capture(
                store,
                NewFact {
                    fields: [
                        ("donuts".to_string(), "1".to_string()),
                        ("mood".to_string(), mood.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                    ..NewFact::about(
                        subject.clone(),
                        format!("a donut, number {nth}"),
                        date(2026, 7, 1),
                    )
                },
            )
            .await;
        }

        let held = store
            .fields(&subject)
            .await
            .expect("the fields of a thing should succeed");
        assert_eq!(
            held.get("donuts").map(String::as_str),
            Some("3"),
            "three writes of one add up: {held:?}"
        );
        assert_eq!(
            held.get("mood").map(String::as_str),
            Some("queasy"),
            "and the key nobody declared a counter takes its newest write: {held:?}"
        );

        let history = store
            .history(&subject, "donuts")
            .await
            .expect("history should succeed");
        assert_eq!(
            history
                .iter()
                .map(|w| w.value.as_deref())
                .collect::<Vec<_>>(),
            vec![Some("1"), Some("1"), Some("1")],
            "the projection adds them up and the substrate keeps each one: {history:?}"
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
            EntityId::new(EntityKind::EVENT, "contract-winter-fest"),
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
    /// adapter *believed* it stored, so a write that drops the fields on both
    /// halves passes its own invariant. The
    /// comparison a dropped field cannot survive is against the CALLER's
    /// record, and this is the only place that comparison is made.
    pub async fn a_records_fields_survive_capture<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-evented");
        let touched = EntityId::new(EntityKind::PLACE, "contract-kiln-yard");
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
            // **The longest key a caller may write, and that is not
            // decoration.** A key crosses to a store as a column value, and
            // the column carrying it is part of a primary key — so keys do not
            // all cost the same, and a store that holds a short one can refuse
            // a long one on the write. A case that picks a short key answers
            // for short keys only. The fake keeps a map in memory and has no
            // cell to overflow, so it answers the same either way: only a
            // store can answer for this, which is why the key rides in the
            // contract both stores run.
            ("k".repeat(MAX_KEY_CHARS), "at the limit".to_string()),
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
    /// this surface where naming a stranger is free, and the hatch is ungated
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

        // **The refusal is served to the caller verbatim, so it teaches
        // whatever it says.** A record is a claim; whether this store keeps one
        // as a line in a table is the store's business and a word that stops
        // being true the day the product underneath is swapped. An agent taught
        // it has to unlearn a model rather than read a new sentence.
        let said = outcome.expect_err("refused above").to_string();
        assert!(
            said.contains("retracts"),
            "the refusal names the key it stopped, or the caller cannot act on it: {said}"
        );
        let word = |needle: &str| {
            said.split(|c: char| !c.is_alphanumeric())
                .any(|w| w.eq_ignore_ascii_case(needle))
        };
        assert!(
            !(word("row") || word("rows")),
            "the refusal calls the thing taken back a row, which is this store's furniture \
             rather than what a caller holds: {said}"
        );
        // **The noun on the thing taken back, not merely somewhere in the
        // sentence.** The refusal opens by saying what a record's fields are,
        // so a needle anywhere in the text is satisfied by that opening and
        // stays green while the clause at issue says anything at all — the fix
        // here is never deletion.
        assert!(
            said.contains("names the record"),
            "…and what the key names is a record, said where it is named: dropping the noun \
             costs the caller the only sentence that says what the key holds: {said}"
        );

        // …and the ordinary keys beside it are untouched, `type` and `ref`
        // included: those words belong to the edge grammar, not to a record's
        // own fields.
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
    /// The reason is optional and a row still has to carry content, so the
    /// absent case writes a sentence either way. The risk it leaves behind is that the sentence is jojobot's
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

    /// **The marker is machinery, and machinery is not one of the thing's
    /// properties.**
    ///
    /// A retraction account is an active record, so its writes fold like any
    /// other's — and the key it carries is the one no caller may write. Folded
    /// into the dense row it reads as the thing's own property: an agent is
    /// told that what this thing IS is a fact address, the operator's page
    /// renders it under what the thing is, and the index takes a posting for
    /// it.
    ///
    /// The record keeps it. That is where the marker means something, and
    /// [`Fact::is_retraction`] is read off it.
    pub async fn the_retraction_marker_is_not_one_of_the_things_fields<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::THING, "contract-marker-not-a-field");
        let claim = capture(
            store,
            NewFact {
                fields: [("cost".to_string(), "40".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "bought at the counter", date(2026, 4, 18))
            },
        )
        .await;
        assert_eq!(
            thing_fields(store, &subject).await.get("cost"),
            Some(&"40".to_string()),
            "the setup has to have landed, or what is asserted below passes on nothing"
        );

        let taken_back = store
            .retract(
                &claim.address(),
                Some("the receipt said otherwise"),
                date(2026, 4, 19),
            )
            .await
            .expect("retracting the claim should succeed");

        let held = thing_fields(store, &subject).await;
        assert!(
            !held.contains_key(RETRACTS),
            "the key jojobot writes to mark a record taken back is not a property of the \
             thing: {held:?}"
        );
        assert!(
            !held.contains_key("cost"),
            "…and the retracted claim's own key went with it: {held:?}"
        );

        // The other half: it did not leave the record, which is the only place
        // it ever meant anything.
        let account = read_back(store, &subject, &taken_back.record.id).await;
        assert_eq!(
            account.retracts(),
            Some(claim.address().to_string().as_str()),
            "the account still names what it took back"
        );
        assert!(
            account.is_retraction(),
            "…and still reads as a retraction: {account:?}"
        );
    }

    /// **Taking the marker off is refused the way writing it is.**
    ///
    /// The set path screens the reserved key so the marker cannot be forged.
    /// The clear path is the same key on the same rail: an edit that removes it
    /// makes [`Fact::is_retraction`] answer false, which lets the account be
    /// retracted — the reversal the one-way rule exists to forbid — and takes
    /// away the only machine-readable link from the marked row to its account.
    ///
    /// **The refusal is one move, not a lockout.** The account is an ordinary
    /// record otherwise, and the repairs any record gets still reach it.
    pub async fn clearing_the_retraction_marker_is_refused<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-clear-marker");
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
            .expect("the retraction lands");
        let account = taken_back.record.address();

        let refused = store
            .update_fact(
                &account,
                FactPatch {
                    clear_fields: vec![RETRACTS.to_string()],
                    ..Default::default()
                },
            )
            .await;
        assert!(
            matches!(refused, Err(MemoryError::InvalidFact(_))),
            "clearing the reserved key must be refused as hard as writing it: {refused:?}"
        );

        // **The refusal is served verbatim, so it teaches whatever it says** —
        // the same pin the set path carries, because both are text an agent
        // reads and acts on. Read once and asserted on both halves at once: the
        // key it stopped, which is what says this refusal and not some other
        // error came back, and the way out, without which a caller holding a
        // retraction it regrets has nowhere to go but a second attempt at the
        // move that was just refused.
        let said = refused.expect_err("refused above").to_string();
        assert!(
            said.contains(RETRACTS),
            "the refusal names the key it stopped, or the caller cannot tell which \
             refusal this is: {said}"
        );
        assert!(
            said.contains("capture what is so now as a new record"),
            "…and it hands back the move that IS allowed — a new record saying what is \
             so — because the one it refused is the only one a caller would try next: {said}"
        );

        // And nothing moved: the account is still a retraction, so the row it
        // took back is still linked to it and it is still not retractable.
        let still = read_back(store, &subject, &taken_back.record.id).await;
        assert_eq!(
            still.retracts(),
            Some(event.address().to_string().as_str()),
            "the refused edit wrote nothing"
        );
        let reversal = store
            .retract(&account, Some("undo"), date(2026, 7, 5))
            .await;
        assert!(
            matches!(reversal, Err(MemoryError::NotRetractable { .. })),
            "a retraction of a retraction stays refused: {reversal:?}"
        );

        // The floor rule's other half: the screen refuses the one move, not the
        // record. An ordinary key on the account is set and cleared as usual.
        edit(
            store,
            &account,
            FactPatch {
                fields: [("filed_by".to_string(), "the desk".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
        let repaired = edit(
            store,
            &account,
            FactPatch {
                clear_fields: vec!["filed_by".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert!(
            !repaired.fields.contains_key("filed_by"),
            "an ordinary key on a retraction account is still the caller's to clear: {repaired:?}"
        );
        assert!(
            repaired.is_retraction(),
            "…and the repair left the marker where it was: {repaired:?}"
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
        let first = EntityId::new(EntityKind::ORG, "contract-riverside");
        add(
            store,
            NewEntity::new(first.clone(), "Riverside", "user-named"),
        )
        .await;

        let typo = EntityId::new(EntityKind::ORG, "contract-riversid");
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
                .list_entities(Some(EntityKind::ORG))
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
        let stranger = EntityId::new(EntityKind::WORK, "contract-first-mix");
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

        let stranger = EntityId::new(EntityKind::EVENT, "contract-unheard-of-fest");
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

    /// The same gate on an **edge attached later**. A path carrying the code
    /// and no spec is one where deleting the check leaves the whole suite
    /// green, and the hole is exactly the interesting one: an edge realized
    /// after the fact is the day-to-day way edges get drawn.
    pub async fn update_fact_requires_an_existing_edge_object<M: Memory>(store: &M) {
        let subject = EntityId::person("contract-late-edge");
        let captured = capture(
            store,
            NewFact::about(subject.clone(), "was somewhere that week", date(2026, 7, 1)),
        )
        .await;

        let stranger = EntityId::new(EntityKind::PLACE, "contract-nowhere-in-particular");
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

    /// **A frontmatter field at the validator's limit survives the store.**
    ///
    /// `source` and `crm` are screened by one rule — one plain line, at most
    /// two hundred characters — and a column narrower than that rule refuses a
    /// value the domain admits. The write is already past every check jojobot
    /// makes when the store answers, so nothing above the port sees it coming,
    /// and a double cannot answer for it: a double keeps whatever bytes it is
    /// handed, whatever a column would have said.
    ///
    /// The two fields are written together and asserted apart, because they
    /// are two columns and a widening that reaches one leaves the other
    /// exactly as it was.
    pub async fn a_field_at_the_validators_limit_survives_storage<M: Memory>(store: &M) {
        let id = EntityId::person("contract-brimful");
        let source = format!("{:-<200}", "contract-source-");
        let crm = format!("{:0<200}", "card:");
        assert_eq!(
            (source.chars().count(), crm.chars().count()),
            (200, 200),
            "this case is about the limit only if it writes the limit",
        );

        add(
            store,
            NewEntity {
                crm: Some(crm.clone()),
                ..NewEntity::new(id.clone(), "Omicron", source.clone())
            },
        )
        .await;

        let seen = read_entity(store, &id).await;
        assert_eq!(
            seen.source, source,
            "the source the validator admits comes back whole",
        );
        assert_eq!(
            seen.crm.as_deref(),
            Some(crm.as_str()),
            "…and so does the cross-link, which is a column of its own",
        );
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
    /// **A record found through the index carries both clocks and the day it
    /// stays good.**
    ///
    /// The index keeps a record as JSON and builds the hit back from it, so it
    /// is a second storage path with a second chance to drop a value — and a
    /// read-back inside either store cannot see it, because both halves of that
    /// comparison come from the same construction.
    ///
    /// **The negative is paired here on purpose.** A record that made no
    /// promise about how long it stays good comes back with none, so this
    /// cannot pass on a build that fills the field in on the way through.
    pub async fn a_hit_carries_the_clocks_the_store_kept<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let subject = EntityId::person("contract-penny");
        let watched = capture(
            store,
            NewFact {
                stale_after: Some(date(2026, 11, 30)),
                ..NewFact::about(subject.clone(), "rides a penny farthing", date(2026, 7, 1))
            },
        )
        .await;
        capture(
            store,
            NewFact::about(subject.clone(), "owns a penny whistle", date(2026, 7, 1)),
        )
        .await;

        let hits = found(search, SearchQuery::text("penny")).await;
        let facts = fact_hits(&hits);
        let hit = |needle: &str| {
            (*facts
                .iter()
                .find(|f| f.content.contains(needle))
                .unwrap_or_else(|| panic!("the record saying {needle} must come back: {hits:?}")))
            .clone()
        };

        let carried = hit("farthing");
        assert_eq!(
            carried.stale_after,
            Some(date(2026, 11, 30)),
            "the day this reading stays good did not survive the index",
        );
        assert_eq!(
            carried.inserted_at, watched.inserted_at,
            "the moment the store took the record in did not survive the index",
        );
        assert!(
            carried.inserted_at.is_some(),
            "the store kept no stamp at all, so the check above compares two absences",
        );

        let ordinary = hit("whistle");
        assert_eq!(
            ordinary.stale_after, None,
            "a record that made no promise came back carrying one",
        );
    }

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
        let far = EntityId::new(EntityKind::PLACE, "contract-faraway");
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
        let project = EntityId::new(EntityKind::PROJECT, "contract-away-project");
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
                kind: Some(EntityKind::PERSON),
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
        let fest = EntityId::new(EntityKind::EVENT, "contract-connected-fest");
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
                    EntityId::new(EntityKind::WORK, "contract-conn-mix"),
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
                    EntityId::new(EntityKind::EVENT, "contract-connected-other"),
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
        let handle = EntityId::new(EntityKind::ORG, "contract-pinnable-guild");
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
        assert_eq!(fact_subject.kind, Some(EntityKind::PERSON));
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
        let handle = EntityId::new(EntityKind::ORG, "contract-orient-guild");
        let hall = EntityId::new(EntityKind::PLACE, "contract-orient-hall");
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
                Field::required("starts", ValueType::Date),
                Field::required("rooms", ValueType::Number),
                Field::required("building", ValueType::Reference),
                Field::required("active", ValueType::Boolean),
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
                    Field::required("weight", ValueType::Number),
                    Field::required("sender", ValueType::Reference),
                ],
            ),
        )
        .await;
        let second = DeclaredType::new(
            "contract-parcel",
            vec![
                Field::required("weight", ValueType::Number),
                Field::required("arrives", ValueType::Date),
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
            DeclaredType::new(
                "contract-not-empty",
                vec![Field::required("x", ValueType::Text)],
            ),
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
                Field::required("starts", ValueType::Date),
                Field::required("cover", ValueType::Reference),
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
                vec![Field::required("starts", ValueType::Date)],
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
                vec![Field::required("starts", ValueType::Date)],
            ),
        )
        .await;
        let replaced = DeclaredType::new(
            "contract-shift",
            vec![Field::required("cover", ValueType::Text)],
        );
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
                vec![Field::required("seats", ValueType::Text)],
            ),
        )
        .await;
        declare(
            store,
            DeclaredType::new(
                "contract-booking",
                vec![Field::required("seats", ValueType::Number)],
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
                    Field::required("weight", ValueType::Number),
                    Field::required("arrives", ValueType::Date),
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
                    Field::required("weight", ValueType::Number),
                    Field::required("arrives", ValueType::Date),
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

    /// **The strict question keeps only what fits; the tolerant one still
    /// reports the gaps.**
    ///
    /// Which question a reader is asking is the reader's choice, and `recall`
    /// is what lets them make it. Both halves in one case: strict alone passes
    /// on a build that returns nothing, and tolerant alone passes on a build
    /// that never asked the strict question.
    ///
    /// **The tolerant one is what a caller naming neither gets.** A thing
    /// arriving with its gaps named can neither hide nor overclaim; a thing
    /// missing from an answer looks exactly like a thing that is not there.
    pub async fn search_keeps_only_what_fits_when_the_caller_asks<M: Memory, S: Search>(
        store: &M,
        search: &S,
    ) {
        let declared = store
            .declare_type(DeclaredType::new(
                "contract-pallet",
                vec![
                    Field::required("stacked", ValueType::Number),
                    Field::required("shipped_on", ValueType::Date),
                ],
            ))
            .await
            .expect("declaring should succeed");

        let whole = EntityId::new(EntityKind::THING, "contract-pallet-whole");
        for (key, value, said) in [
            ("stacked", "12", "somebody counted the boxes"),
            (
                "shipped_on",
                "2026-08-10",
                "and somebody else stamped the day",
            ),
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
        let partial = EntityId::new(EntityKind::THING, "contract-pallet-half");
        capture(
            store,
            NewFact {
                fields: [("stacked".to_string(), "3".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    partial.clone(),
                    "a shorter stack, and no day",
                    date(2026, 8, 2),
                )
            },
        )
        .await;

        let strict = found(
            search,
            SearchQuery {
                fits_type: Some(declared.clone()),
                limit: 50,
                ..Default::default()
            },
        )
        .await;
        let kept: Vec<&EntityId> = strict
            .iter()
            .filter_map(|h| match h {
                Hit::Entity { entity, .. } => Some(&entity.id),
                _ => None,
            })
            .collect();
        assert!(
            kept.contains(&&whole),
            "a thing holding every key is what the strict question is for: {kept:?}"
        );
        assert!(
            !kept.contains(&&partial),
            "…and a thing with a gap is not in that answer: {kept:?}"
        );

        // The other question, over the same two things: both come back, and
        // the incomplete one says what it lacks rather than disappearing.
        let tolerant = found(
            search,
            SearchQuery {
                answers_type: Some(declared),
                limit: 50,
                ..Default::default()
            },
        )
        .await;
        let answered: Vec<(&EntityId, &crate::memory::types::Match)> = tolerant
            .iter()
            .filter_map(|h| match h {
                Hit::Entity {
                    entity, answers, ..
                } => Some((&entity.id, answers.as_deref()?)),
                _ => None,
            })
            .collect();
        let (_, gapped) = answered
            .iter()
            .find(|(id, _)| **id == partial)
            .unwrap_or_else(|| {
                panic!("the tolerant question keeps the gapped thing: {answered:?}")
            });
        assert_eq!(
            gapped.lacking,
            vec!["shipped_on"],
            "…and names the gap: {gapped:?}"
        );
        assert!(
            answered.iter().any(|(id, _)| **id == whole),
            "…without losing the whole one: {answered:?}"
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
                vec![Field::required("arrives", ValueType::Date)],
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

        // The tolerant question keeps it and says what is wrong with it, which
        // is the point of this case: no key is absent…
        assert!(flagged.lacking.is_empty(), "{flagged:?}");
        // …and it still does not fit, because holding a key badly is not
        // holding it.
        assert!(
            !flagged.complete(),
            "a date slot holding a phrase leaves the type unanswered: {flagged:?}",
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
        a_hit_carries_the_clocks_the_store_kept(store, search).await;
        search_excludes_superseded_by_default_and_lists_it_on_request(store, search).await;
        search_excludes_a_retracted_record_by_default(store, search).await;
        search_answers_ask_across_by_kind_and_edge(store, search).await;
        search_by_edge_object_alone_finds_any_shape(store, search).await;
        search_pins_a_named_entity_first(store, search).await;
        search_fact_hits_name_their_subject_and_home(store, search).await;
        search_entity_hits_carry_their_edges(store, search).await;

        search_finds_things_that_answer_a_type_structurally(store, search).await;
        search_keeps_only_what_fits_when_the_caller_asks(store, search).await;
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
        let one = EntityId::new(EntityKind::BOT, "contract-graph-one");
        let two = EntityId::new(EntityKind::BOT, "contract-graph-two");
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
                    kind: Some(EntityKind::BOT),
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
        let gathering = EntityId::new(EntityKind::EVENT, "contract-graph-gathering");
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
    /// **A trip records who came, and the record answers from either end.**
    ///
    /// The trip keys say where and when, and nothing on them says who — so a
    /// companion is a link rather than a key. It is drawn as `attendance` from
    /// the person at the trip, which is the shape that already means "was at":
    /// a link drawn the other way could only be `about`, which asserts a claim
    /// nobody made, or `connection`, which says the nature of the link was not
    /// recorded.
    ///
    /// **Both questions come out of that one edge**, and both are asserted
    /// here, because the walk carries its own direction: *who came on this
    /// trip* is the edge walked inbound, and *which trips was this person on*
    /// is the same edge walked outbound from the person. The second is the one
    /// a change would break silently, since nothing else in the suite asks it
    /// of a trip.
    ///
    /// **More than two companions on one trip**, which is what a key could not
    /// hold: a thing's fields fold to the newest write of each key, so a key
    /// would keep the last companion and drop the rest. Three would pass an
    /// implementation that keeps only a pair.
    /// **The kinds are rows, and a shipped one is closed to a caller.**
    ///
    /// Only a store can answer this: the set a process parses against is
    /// loaded from these rows, so a store that cannot keep them is a store
    /// where no handle resolves. The double keeps whatever it is handed, which
    /// is why this belongs to the contract rather than to a unit test.
    pub async fn the_kinds_are_rows_and_a_shipped_one_is_closed<M: Memory>(store: &M) {
        // The two steps a boot takes, in the order it takes them: write what
        // this build ships, then parse against what the store answers with.
        crate::memory::kinds::seed(store)
            .await
            .expect("the kinds are seeded");

        let held = store.declared_kinds().await.expect("the kinds read back");
        for shipped in crate::memory::kinds::SHIPPED {
            assert!(
                held.iter()
                    .any(|(token, origin)| token == shipped && *origin == Origin::Shipped),
                "the store holds '{shipped}' as a kind the software ships: {held:?}",
            );
        }

        // **Re-seeding changes nothing**, which is what lets the boot write
        // them unconditionally.
        store
            .declare_kind("person", Origin::Shipped, Vec::new())
            .await
            .expect("the seed runs again");
        let after = store.declared_kinds().await.expect("the kinds read back");
        assert_eq!(
            held.len(),
            after.len(),
            "a second seed writes no second row: {after:?}",
        );

        // **A caller cannot take one over.** The refusal reads the row's own
        // origin, so nothing anywhere keeps a list of protected names.
        let refused = store
            .declare_kind("person", Origin::Declared, Vec::new())
            .await
            .expect_err("a caller cannot redeclare a kind the software ships");
        assert!(
            matches!(refused, MemoryError::InvalidEntity(_)),
            "the refusal says what is wrong rather than failing the store: {refused:?}",
        );
        assert_eq!(
            store
                .declared_kinds()
                .await
                .expect("the kinds read back")
                .into_iter()
                .find(|(token, _)| token == "person")
                .map(|(_, origin)| origin),
            Some(Origin::Shipped),
            "…and the row it refused to take over is untouched",
        );
    }

    pub async fn a_trip_records_who_came_and_answers_from_either_end<M: Memory>(store: &M) {
        let away = EntityId::new(EntityKind::EVENT, "contract-long-weekend");
        let home = EntityId::new(EntityKind::PLACE, "contract-harbour-end");
        let there = EntityId::new(EntityKind::PLACE, "contract-fjord-town");
        for id in [&away, &home, &there] {
            ensure(store, id).await;
        }
        let companions = [
            EntityId::person("contract-omicron"),
            EntityId::person("contract-sigma"),
            EntityId::person("contract-tau"),
            EntityId::person("contract-upsilon"),
        ];

        // The trip itself: the keys the shipped type names, so this is a trip
        // rather than any event that happens to have guests.
        capture(
            store,
            NewFact {
                fields: [
                    ("departs_from".to_string(), home.to_string()),
                    ("arrives_at".to_string(), there.to_string()),
                    ("leaves_on".to_string(), "2026-05-01".to_string()),
                    ("returns_on".to_string(), "2026-05-04".to_string()),
                ]
                .into_iter()
                .collect(),
                refs: Vec::new(),
                ..NewFact::about(away.clone(), "four days away", date(2026, 4, 1))
            },
        )
        .await;

        for who in &companions {
            ensure(store, who).await;
            capture(
                store,
                NewFact {
                    edge: Some(Edge::new(EdgeShape::Attendance, away.clone())),
                    ..NewFact::about(who.clone(), "came along", date(2026, 5, 1))
                },
            )
            .await;
        }

        // **Who came on this trip** — inbound, and the companions are reached
        // rather than named.
        let came = walked_from(store, &away, graph::Direction::In).await;
        for who in &companions {
            assert!(
                came.contains(who),
                "every companion hangs off the trip, and {who} does not: {came:?}",
            );
        }

        // **Which trips was this person on** — the same edge, outbound from the
        // person's end, which is the half nothing else asks.
        let went = walked_from(store, &companions[0], graph::Direction::Out).await;
        assert!(
            went.contains(&away),
            "the trip is reached from the companion it carried: {went:?}",
        );

        // **A trip nobody came on still reads back**, so recording companions
        // is something a trip may do rather than something it must.
        let alone = EntityId::new(EntityKind::EVENT, "contract-lone-crossing");
        ensure(store, &alone).await;
        capture(
            store,
            NewFact {
                fields: [
                    ("departs_from".to_string(), home.to_string()),
                    ("arrives_at".to_string(), there.to_string()),
                ]
                .into_iter()
                .collect(),
                refs: Vec::new(),
                ..NewFact::about(alone.clone(), "went by myself", date(2026, 6, 1))
            },
        )
        .await;
        let read = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(alone.clone()),
                    ..graph::Selection::default()
                },
                ..graph::GraphQuery::default()
            },
        )
        .await
        .expect("a handle is a selection");
        assert_eq!(
            read.first().map(|o| &o.entity.id),
            Some(&alone),
            "a trip with no companions comes back as itself: {read:?}",
        );
        assert!(
            walked_from(store, &alone, graph::Direction::In)
                .await
                .is_empty(),
            "…and nobody is reached from it",
        );
    }

    /// The entities one attendance hop reaches from this one, in the direction
    /// asked. The two directions are two questions, which is why the direction
    /// is the argument.
    async fn walked_from<M: Memory>(
        store: &M,
        from: &EntityId,
        direction: graph::Direction,
    ) -> Vec<EntityId> {
        let walked = graph::walk(
            store,
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(from.clone()),
                    ..graph::Selection::default()
                },
                include: graph::Include {
                    facts: false,
                    prose: false,
                },
                follow: Some(graph::Follow {
                    along: graph::Along::Edge(EdgeShape::Attendance),
                    direction: Some(direction),
                    depth: 1,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
            },
        )
        .await
        .expect("a subject with a walk");
        walked
            .first()
            .map(|o| o.connected.iter().map(|c| c.entity.id.clone()).collect())
            .unwrap_or_default()
    }

    pub async fn a_declared_reference_key_is_walkable_against_the_store<M: Memory>(store: &M) {
        let owner = EntityId::person("contract-relation-owner");
        let held = EntityId::new(EntityKind::THING, "contract-relation-held");
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
                        Field::required("keeper", holds),
                        Field::required("since", ValueType::Date),
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

    /// **The kind a reference points at survives the store.**
    ///
    /// A store keeps a declaration as one token, so a narrowing it can write
    /// and not read back is a declaration that silently widens on the next
    /// read — and every check that rests on it then passes a handle of any
    /// kind. Only a store can answer for that, which is why this is a contract
    /// case.
    ///
    /// The unnarrowed reference is in the same type, because a store that read
    /// every reference back as one kind would satisfy the first assertion
    /// alone.
    ///
    /// **One of the narrowings names the LONGEST kind, and that is not
    /// decoration.** A declaration crosses to a store as one token —
    /// `reference:` and the kind — so the kinds do not all cost the same
    /// number of characters, and a cell wide enough for most of them holds a
    /// declaration that reads back as any handle at all, or fails on the
    /// write. A case that picks a short kind answers for the short kinds only.
    pub async fn a_reference_keeps_the_kind_it_points_at<M: Memory>(store: &M) {
        store
            .declare_type(DeclaredType::new(
                "contract-stay",
                vec![
                    Field::pointing_at("venue", EntityKind::PLACE).needed(),
                    Field::pointing_at("part_of", EntityKind::PROJECT).needed(),
                    Field::required("booked_by", ValueType::Reference),
                ],
            ))
            .await
            .expect("declaring a type of my own is accepted");

        let held = store
            .declared_types()
            .await
            .expect("the roster reads")
            .into_iter()
            .find(|t| t.name == "contract-stay")
            .expect("the store holds the type it just took");
        assert_eq!(
            held.field("venue").and_then(|f| f.points_at),
            Some(EntityKind::PLACE),
            "the narrowing came back off the store: {held:?}",
        );
        assert_eq!(
            held.field("part_of").and_then(|f| f.points_at),
            Some(EntityKind::PROJECT),
            "…and so did the one naming the longest kind, which is the token a cell sized for \
             the others cannot hold: {held:?}",
        );
        assert_eq!(
            held.field("booked_by").and_then(|f| f.points_at),
            None,
            "…and a reference that named no kind is still any handle: {held:?}",
        );
    }

    /// **Neither half may write over the other's keys.**
    ///
    /// A kind's keys and a type's keys live in one place, so the name alone
    /// cannot say which of the two wrote a row. Sharing the place is the model
    /// working — a kind IS a schema — and taking the other side's keys is not:
    /// a declaration that silently replaced them would move a kind's floor
    /// from a verb nobody called.
    ///
    /// **Both directions, because the two are separate checks and either can
    /// go missing on its own.**
    ///
    /// **The narrow reach is the point rather than a limitation.** It needs a
    /// kind whose keys a CALLER declared: on a kind the software ships, the
    /// origin check refuses the caller first, in both stores. So this is the
    /// one door where the two halves can reach each other at all.
    ///
    /// A contract case because a store answering this differently from the
    /// double is what a case belonging to either alone cannot see.
    pub async fn neither_half_writes_over_the_others_keys<M: Memory>(store: &M) {
        // ① A type may not take a kind's keys over.
        store
            .declare_kind(
                "handcart",
                Origin::Declared,
                vec![Field::required("load", ValueType::Number)],
            )
            .await
            .expect("a caller declares a kind that names a key");
        let refused = store
            .declare_type(DeclaredType::new(
                "handcart",
                vec![Field::required("colour", ValueType::Text)],
            ))
            .await
            .expect_err("a type may not write over the keys of a kind of that name");
        assert!(
            matches!(refused, MemoryError::InvalidEntity(_)),
            "the declaration is what is wrong, not the store: {refused:?}",
        );

        // **…and the kind's keys are where they were.** Without this the case
        // passes against a store that refuses the declaration and replaces the
        // keys anyway.
        let held = store
            .declared_types()
            .await
            .expect("the roster reads")
            .into_iter()
            .find(|t| t.name == "handcart")
            .expect("the kind's keys are still held under its name");
        assert!(
            held.field("load").is_some() && held.field("colour").is_none(),
            "the kind kept its own keys and took none of the type's: {held:?}",
        );

        // ② And a kind may not take a type's keys over, which is the same rule
        // read from the other side.
        store
            .declare_type(DeclaredType::new(
                "contract-cartload",
                vec![Field::required("weighed_on", ValueType::Date)],
            ))
            .await
            .expect("a caller declares a type of their own");
        let refused = store
            .declare_kind(
                "contract-cartload",
                Origin::Declared,
                vec![Field::required("load", ValueType::Number)],
            )
            .await
            .expect_err("a kind may not write over the keys of a type of that name");
        assert!(
            matches!(refused, MemoryError::InvalidEntity(_)),
            "the declaration is what is wrong, not the store: {refused:?}",
        );
        let held = store
            .declared_types()
            .await
            .expect("the roster reads")
            .into_iter()
            .find(|t| t.name == "contract-cartload")
            .expect("the type's keys are still held under its name");
        assert!(
            held.field("weighed_on").is_some() && held.field("load").is_none(),
            "the type kept its own keys and took none of the kind's: {held:?}",
        );
    }

    /// **A caller's type named after a kind governs nothing, and the kind's
    /// own keys still govern everything.**
    ///
    /// A kind's keys and a type's keys live in one place and are told apart by
    /// who declared them. The fit guard selects a declaration whose NAME
    /// matches the thing's kind, so a guard handed both halves as one list
    /// reads a caller's type named `place` as the kind `place`'s schema — a
    /// gate over a shipped kind, made with no new verb and with no verb to
    /// undo it.
    ///
    /// **The declaration itself stays legal.** A caller's schema may be named
    /// after a kind, and a restart must not destroy it. What is wrong is the
    /// lookup, so the halves are kept apart and the kind's keys reach the
    /// guard as the kind's.
    ///
    /// **A contract case because the two stores answered this differently.**
    /// One read the halves apart and the other did not, and a case belonging
    /// to either alone is a case that cannot see the difference. The tooth is
    /// the value-type one on an ordinary write with no floor involved, which
    /// is the widest blast radius and the one a fix aimed at fitting walks
    /// past.
    pub async fn a_type_named_after_a_kind_does_not_gate_that_kinds_writes<M: Memory>(store: &M) {
        // A shipped kind that names no key, so what governs a place here can
        // only be the caller's type below.
        let moes = EntityId::new(EntityKind::PLACE, "contract-tavern");
        ensure(store, &moes).await;
        store
            .declare_type(DeclaredType::new(
                "place",
                vec![Field::required("shadow_postcode", ValueType::Number)],
            ))
            .await
            .expect("a caller's schema may be named after a kind");

        // **The write the shadow refused.** A place carrying a postcode that is
        // no number is an ordinary claim: nothing a caller declared governs it.
        capture(
            store,
            NewFact {
                fields: [("shadow_postcode".to_string(), "SW1A".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(moes.clone(), "the postcode is SW1A", date(2026, 8, 19))
            },
        )
        .await;

        // **The positive the negative rests on: a kind's OWN key still holds a
        // write to what it says.** Without this half the case passes against a
        // build whose fit guard reads nothing at all and lets everything
        // through.
        //
        // A shipped kind is used rather than a new one, for the reason the
        // cases above give: declaring a new kind fills the set this process
        // parses against.
        store
            .declare_kind(
                "pet",
                Origin::Shipped,
                vec![Field::required("pet_weight", ValueType::Number)],
            )
            .await
            .expect("a kind may name the keys its things keep");
        let helper = EntityId::new(EntityKind::PET, "contract-the-heavy-one");
        ensure(store, &helper).await;
        let refused = store
            .capture(NewFact {
                fields: [("pet_weight".to_string(), "quite a lot".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(helper, "it weighs a fair bit", date(2026, 8, 19))
            })
            .await;
        let Err(MemoryError::BreaksType { name, key, .. }) = &refused else {
            panic!("a kind's own key must hold a write to what it says, got {refused:?}");
        };
        assert_eq!(name, "pet", "the refusal names the kind");
        assert_eq!(key, "pet_weight", "…and the key it wrote badly");
    }

    /// **A write cannot put a value on a thing that its type refuses — and
    /// the same write against a thing that fits nothing is allowed.**
    ///
    /// Holding a key badly is not holding it, so a value that breaks its key's
    /// declaration drops the thing below the type exactly as taking the key
    /// away would. The refusal is the floor rule reaching a second way of
    /// losing a key, never a second rule.
    ///
    /// Four beats, and the last three are what stop this becoming a gate: the
    /// wrong-kinded handle is refused on a thing that IS a stay; the identical
    /// write lands on a thing that fits nothing, which is what keeps a
    /// half-described thing repairable; a key no type mentions is welcome on
    /// the fitting thing, because strict is a floor; and the refused record is
    /// unchanged afterwards.
    ///
    /// A contract case because only a store can say whether the write was
    /// refused AND the record kept.
    pub async fn a_write_cannot_put_a_value_the_type_refuses<M: Memory>(store: &M) {
        // **A KIND, not a declared type.** What a write may take off a thing is
        // its kind's question: a declared type describes and holds nothing.
        store
            .declare_kind(
                "work",
                Origin::Shipped,
                vec![
                    Field::pointing_at("stay_venue", EntityKind::PLACE).needed(),
                    Field::required("stay_nights", ValueType::Number),
                ],
            )
            .await
            .expect("a kind may name the keys its things keep");
        // a shipped kind is used rather than a new one:
        // declaring a new kind fills the set this process parses against, and any
        // case beside this one that stands a store up empties it again.

        let moes = EntityId::new(EntityKind::PLACE, "contract-moes");
        let helper = EntityId::new(EntityKind::PET, "contract-santas-little-helper");
        ensure(store, &moes).await;
        ensure(store, &helper).await;

        // A thing that fits: the venue is a place and the nights are a number.
        let stay = EntityId::new(EntityKind::WORK, "contract-the-stay");
        let booked = capture(
            store,
            NewFact {
                fields: [
                    ("stay_venue".to_string(), moes.to_string()),
                    ("stay_nights".to_string(), "2".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(stay.clone(), "two nights booked", date(2026, 4, 18))
            },
        )
        .await;

        // ① The wrong kind on a thing that fits is refused, and the refusal
        // says which key and what it wanted.
        let refused = store
            .update_fact(
                &booked.address(),
                FactPatch {
                    fields: [("stay_venue".to_string(), helper.to_string())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            )
            .await;
        let Err(MemoryError::BreaksType {
            name,
            key,
            wanted,
            value,
        }) = &refused
        else {
            panic!("a handle of the wrong kind must be refused, got {refused:?}");
        };
        assert_eq!(name, "work", "the refusal names the kind");
        assert_eq!(key, "stay_venue", "…and the key");
        assert_eq!(
            wanted, "reference:place",
            "…and what the key wanted, which `reference` alone could not say",
        );
        assert_eq!(value, &helper.to_string(), "…and what was actually sent");

        // ② The record is exactly as it was: a refusal writes nothing.
        assert_eq!(
            read_back(store, &stay, &booked.id)
                .await
                .fields
                .get("stay_venue")
                .map(String::as_str),
            Some(moes.to_string().as_str()),
            "the refused write left the record alone",
        );

        // ③ **The same write, on a thing that fits nothing.** It carries one of
        // the type's keys and not the other, so there is no fit to protect and
        // the store takes the value as written. Without this beat the case
        // above passes on a build that refuses every reference everywhere.
        let sketch = EntityId::new(EntityKind::EVENT, "contract-the-sketch");
        let jotted = capture(
            store,
            NewFact {
                fields: [("stay_venue".to_string(), moes.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(sketch.clone(), "somewhere, some time", date(2026, 4, 18))
            },
        )
        .await;
        store
            .update_fact(
                &jotted.address(),
                FactPatch {
                    fields: [("stay_venue".to_string(), helper.to_string())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            )
            .await
            .expect("a thing that fits nothing has nothing to protect")
            .written()
            .expect("…and the write lands");

        // ④ **Adding a key no type mentions is never refused**, on the thing
        // that does fit. Strict is a floor: a type says what must survive, not
        // what may be there.
        store
            .update_fact(
                &booked.address(),
                FactPatch {
                    fields: [("stay_mood".to_string(), "quiet".to_string())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            )
            .await
            .expect("a key beyond the type is welcome")
            .written()
            .expect("…and it lands");

        // ⑤ **The other door.** A thing's fields are every write on it folded,
        // so a NEW record carrying the same bad value takes the key on the
        // thing just as an edit does. Guarding only the edit would leave the
        // rule true of one verb and the thing broken by the other.
        let captured = store
            .capture(NewFact {
                fields: [("stay_venue".to_string(), helper.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(stay.clone(), "moved it, so they say", date(2026, 4, 19))
            })
            .await;
        assert!(
            matches!(&captured, Err(MemoryError::BreaksType { key, .. }) if key == "stay_venue"),
            "a capture carrying the wrong kind is refused too, got {captured:?}",
        );
        // …and the same capture against the thing that fits nothing lands, so
        // this cannot pass on a build where capture refuses every reference.
        store
            .capture(NewFact {
                fields: [("stay_venue".to_string(), helper.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(sketch.clone(), "or maybe there", date(2026, 4, 19))
            })
            .await
            .expect("a thing that fits nothing has nothing to protect")
            .written()
            .expect("…and the capture lands");
    }

    /// **A reference naming an entity nobody recorded is refused, the way a
    /// missing edge object already is.**
    ///
    /// A reference is a walkable link, so a value naming nothing is a link into
    /// a node no question can reach — the same hole an edge into a missing
    /// entity leaves, arrived at through a key instead of an edge. It comes
    /// back blocked with candidates rather than as an error, because the repair
    /// is the caller's and the near handles are what it needs.
    ///
    /// **Only a value that is a HANDLE is asked about.** A reference key
    /// holding a phrase is a value that fails its declaration, which is the
    /// floor's business and not this one's — asking whether "the airport"
    /// exists would refuse loose prose in the name of a link nobody drew.
    ///
    /// The real target is written in the same shape, so this cannot pass on a
    /// build that refuses every reference.
    /// **A key declared to hold one of a named set refuses a value outside it,
    /// and the set survives storage to say so.**
    ///
    /// A contract case rather than a unit one, and the storage is the reason:
    /// the values are part of the declaration, so a store that keeps the key
    /// and drops its set narrows nothing. Every beat below passes on such a
    /// store except the refusal itself.
    pub async fn a_closed_set_refuses_a_write_outside_it<M: Memory>(store: &M) {
        // A shipped KIND, because what a write may put on a thing is its kind's
        // question. A declared type describes and governs nothing.
        store
            .declare_kind(
                "project",
                Origin::Shipped,
                vec![Field::one_of("project_stage", ["draft", "building", "done"]).needed()],
            )
            .await
            .expect("a kind may narrow a key to a named set");

        // ⓪ **The set came back off the store.** Read before anything is
        // written, so a failure here is storage rather than the guard.
        let held = store
            .declared_types()
            .await
            .expect("the declarations read back");
        let stage = held
            .iter()
            .find(|d| d.name == "project")
            .and_then(|d| d.field("project_stage"))
            .expect("the kind holds the key it declared");
        assert_eq!(
            stage.one_of.as_deref(),
            Some(["draft", "building", "done"].map(String::from).as_slice()),
            "the values are part of the declaration, so they survive storage",
        );

        // ① A member of the set lands.
        let sketch = EntityId::new(EntityKind::PROJECT, "contract-the-sketchbook");
        ensure(store, &sketch).await;
        let opened = capture(
            store,
            NewFact {
                fields: [("project_stage".to_string(), "draft".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(sketch.clone(), "started it", date(2026, 4, 18))
            },
        )
        .await;

        // ② A value outside the set is refused, and the refusal NAMES the
        // values — `text` is what the key holds underneath and would tell a
        // caller nothing about a value that is text.
        let refused = store
            .update_fact(
                &opened.address(),
                FactPatch {
                    fields: [("project_stage".to_string(), "shipped".to_string())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            )
            .await;
        let Err(MemoryError::BreaksType {
            name,
            key,
            wanted,
            value,
        }) = &refused
        else {
            panic!("a value outside the set must be refused, got {refused:?}");
        };
        assert_eq!(name, "project", "the refusal names the kind");
        assert_eq!(key, "project_stage", "…and the key");
        for allowed in ["draft", "building", "done"] {
            assert!(
                wanted.contains(allowed),
                "…and every value the caller may write: {wanted:?}",
            );
        }
        assert_eq!(value, "shipped", "…and what was actually sent");

        // ③ The record is exactly as it was: a refusal writes nothing.
        assert_eq!(
            read_back(store, &sketch, &opened.id)
                .await
                .fields
                .get("project_stage")
                .map(String::as_str),
            Some("draft"),
            "the refused write left the record alone",
        );

        // ④ **Another member lands on the same thing.** Without this the
        // refusal above passes on a build that refuses every write to the key.
        store
            .update_fact(
                &opened.address(),
                FactPatch {
                    fields: [("project_stage".to_string(), "building".to_string())]
                        .into_iter()
                        .collect(),
                    ..Default::default()
                },
            )
            .await
            .expect("a value the set names is written")
            .written()
            .expect("…and the write lands");

        // ⑤ **A thing below the floor is refused NOTHING.** It carries no key
        // of this kind, so there is no fit to protect, and the same value the
        // fitting thing was refused is taken as written. That is what keeps a
        // messy record repairable.
        let jotted = EntityId::new(EntityKind::EVENT, "contract-the-jotting");
        ensure(store, &jotted).await;
        store
            .capture(NewFact {
                fields: [("project_stage".to_string(), "shipped".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(jotted.clone(), "somewhere in it", date(2026, 4, 18))
            })
            .await
            .expect("a thing with no fit to protect takes the value as written")
            .written()
            .expect("…and the write lands");
    }

    pub async fn a_reference_must_name_an_entity_that_exists<M: Memory>(store: &M) {
        store
            .declare_type(DeclaredType::new(
                "contract-loan",
                vec![Field::required("loaned_to", ValueType::Reference)],
            ))
            .await
            .expect("declaring a type of my own is accepted");

        let borrower = EntityId::new(EntityKind::PERSON, "contract-milhouse");
        ensure(store, &borrower).await;
        let ledger = EntityId::new(EntityKind::THING, "contract-the-ledger");
        ensure(store, &ledger).await;

        let missing = store
            .capture(NewFact {
                fields: [(
                    "loaned_to".to_string(),
                    "person:contract-nobody".to_string(),
                )]
                .into_iter()
                .collect(),
                ..NewFact::about(ledger.clone(), "lent it to somebody", date(2026, 5, 1))
            })
            .await
            .expect("a miss is an answer, not a failure");
        let Guarded::Blocked { attempted, .. } = &missing else {
            panic!("a reference to nobody must be blocked, got {missing:?}");
        };
        assert_eq!(
            attempted.as_str(),
            "person:contract-nobody",
            "the answer names the handle that missed",
        );

        // …and the same key naming somebody who exists goes straight through.
        store
            .capture(NewFact {
                fields: [("loaned_to".to_string(), borrower.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(ledger.clone(), "lent it to Milhouse", date(2026, 5, 2))
            })
            .await
            .expect("capture succeeds")
            .written()
            .expect("a reference to somebody real lands");

        // **The edit path names entities too.** A patch setting a reference key
        // is a write that names one, so it faces the same rule — otherwise the
        // link nobody could capture could still be edited into place.
        let lent = capture(
            store,
            NewFact {
                fields: [("loaned_to".to_string(), borrower.to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(ledger.clone(), "out on loan", date(2026, 5, 4))
            },
        )
        .await;
        let edited = store
            .update_fact(
                &lent.address(),
                FactPatch {
                    fields: [(
                        "loaned_to".to_string(),
                        "person:contract-nobody".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                    ..Default::default()
                },
            )
            .await
            .expect("a miss is an answer, not a failure");
        assert!(
            matches!(&edited, Guarded::Blocked { attempted, .. }
                     if attempted.as_str() == "person:contract-nobody"),
            "an edit to a reference nobody recorded is blocked too, got {edited:?}",
        );

        // **A value that is no handle is not asked to exist.** It fails its
        // declaration and that is the floor's question; this thing fits no
        // type, so nothing refuses it and the note stays writable.
        let scrap = EntityId::new(EntityKind::THING, "contract-the-scrap");
        ensure(store, &scrap).await;
        store
            .capture(NewFact {
                fields: [("loaned_to".to_string(), "somebody at the shop".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(scrap, "lent it to somebody, no idea who", date(2026, 5, 3))
            })
            .await
            .expect("capture succeeds")
            .written()
            .expect("a phrase under a reference key is messy, not a broken link");
    }

    /// **A kind the code learned today survives the store.**
    ///
    /// The store keeps a kind as a string and reads it back off the handle, so
    /// a kind arriving in the enum needs nothing migrated — but that is a claim
    /// **A write cannot drop a thing below a type it already fits — and the
    /// same write against a thing that fits nothing is allowed.**
    ///
    /// Three claims that only hold together. The refusal protects a thing that
    /// IS something; the acceptance keeps a thing that is not something
    /// repairable, which is the whole reason the check reads the result rather
    /// than the change; and adding keys is never refused, because strict here
    /// is a floor and not a ceiling.
    ///
    /// A contract case rather than a domain one: what is under test is that a
    /// STORE refuses the write and keeps the record, which only a store can
    /// answer for.
    pub async fn a_write_cannot_break_a_fit_that_already_exists<M: Memory>(store: &M) {
        // **A KIND, not a declared type.** What a write may take off a thing is
        // its kind's question: a thing is held to what its own kind names, and to nothing else.
        store
            .declare_kind(
                "org",
                Origin::Shipped,
                vec![
                    Field::required("cost", ValueType::Number),
                    Field::required("done_on", ValueType::Date),
                ],
            )
            .await
            .expect("a kind may name the keys its things keep");
        // a shipped kind is used rather than a new one, and `thing`
        // is left keyless for the case below that asks the other half.

        // A thing that fits: both keys, over two sittings, because that is how
        // things get written down.
        let whole = EntityId::new(EntityKind::ORG, "contract-fitting-thing");
        let costed = capture(
            store,
            NewFact {
                fields: [("cost".to_string(), "40".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(whole.clone(), "the annual service", date(2026, 4, 18))
            },
        )
        .await;
        capture(
            store,
            NewFact {
                fields: [("done_on".to_string(), "2026-04-18".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(whole.clone(), "and it was done that day", date(2026, 4, 18))
            },
        )
        .await;

        let refused = store
            .update_fact(
                &costed.address(),
                FactPatch {
                    clear_fields: vec!["cost".to_string()],
                    ..Default::default()
                },
            )
            .await;
        let Err(MemoryError::BreaksFit { name, keys }) = &refused else {
            panic!("taking a key off a thing that fits must be refused, got {refused:?}");
        };
        assert_eq!(name, "org", "the refusal names the kind");
        assert!(
            keys.contains(&"cost".to_string()),
            "…and the key that would go: {keys:?}"
        );
        assert_eq!(
            read_back(store, &whole, &costed.id)
                .await
                .fields
                .get("cost")
                .map(String::as_str),
            Some("40"),
            "a refused write leaves the record exactly as it was"
        );

        // **The same write, against a thing that fits nothing: allowed.** A
        // thing with one of the two keys was never a service, so there is
        // nothing here to protect — and a rule that read the CHANGE rather
        // than the result would refuse this one too and leave the record
        // unrepairable.
        let partial = EntityId::new(EntityKind::ORG, "contract-loose-record");
        let half = capture(
            store,
            NewFact {
                fields: [("cost".to_string(), "40".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(
                    partial.clone(),
                    "somebody wrote down a price",
                    date(2026, 4, 18),
                )
            },
        )
        .await;
        edit(
            store,
            &half.address(),
            FactPatch {
                clear_fields: vec!["cost".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert!(
            !read_back(store, &partial, &half.id)
                .await
                .fields
                .contains_key("cost"),
            "a thing that fits nothing has nothing to protect, so the key goes"
        );

        // **Adding is never refused**, including a key no type names: the
        // floor is what a type asks for, and everything above it is welcome.
        let added = edit(
            store,
            &costed.address(),
            FactPatch {
                fields: [
                    ("cost".to_string(), "45".to_string()),
                    ("mechanic".to_string(), "somebody at the yard".to_string()),
                ]
                .into_iter()
                .collect(),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(
            added.fields.get("mechanic").map(String::as_str),
            Some("somebody at the yard"),
            "a key no type mentions is welcome on a thing that fits one"
        );
        assert_eq!(added.fields.get("cost").map(String::as_str), Some("45"));
    }

    /// **Moving a record past is refused when it would break a fit; taking it
    /// back is not.**
    ///
    /// A supersede and a clear are the same act with two spellings — both take
    /// a key out of what the thing carries, neither is ceremonious, and a rule
    /// that refused one and served the other would read as arbitrary.
    ///
    /// **Retraction is the other kind of write and is left alone.** It is
    /// one-way, it is deliberate, and it says a record should never have been
    /// written; refusing it because of what it costs a type would make a claim
    /// somebody wants taken back impossible to take back. Guard the ordinary
    /// writes, never the deliberate ones.
    ///
    /// Both in one case, on one record, because each half passes on its own
    /// against a build that refuses everything or nothing.
    pub async fn a_supersede_that_breaks_a_fit_is_refused_and_a_retraction_is_not<M: Memory>(
        store: &M,
    ) {
        // **A KIND, not a declared type.** A supersede costs the thing a key,
        // and what a thing may not lose is what its own kind names.
        store
            .declare_kind(
                "topic",
                Origin::Shipped,
                vec![
                    Field::required("season", ValueType::Text),
                    Field::required("pitch_fee", ValueType::Number),
                ],
            )
            .await
            .expect("a kind may name the keys its things keep");
        // a shipped kind is used rather than a new one, for the reason
        // the case above gives.

        let held = EntityId::new(EntityKind::TOPIC, "contract-run-of-stalls");
        let seasonal = capture(
            store,
            NewFact {
                fields: [("season".to_string(), "summer".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(held.clone(), "the season it runs in", date(2026, 4, 18))
            },
        )
        .await;
        capture(
            store,
            NewFact {
                fields: [("pitch_fee".to_string(), "14".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(held.clone(), "and what a pitch costs", date(2026, 4, 18))
            },
        )
        .await;

        // Only active records fold, so moving this one past takes its key out
        // of what the thing carries — the same cost a clear has.
        let refused = store
            .update_fact(
                &seasonal.address(),
                FactPatch {
                    status: Some(FactStatus::Superseded),
                    ..Default::default()
                },
            )
            .await;
        let Err(MemoryError::BreaksFit { name, keys }) = &refused else {
            panic!("moving a record past must cost what a clear costs, got {refused:?}");
        };
        assert_eq!(name, "topic");
        assert!(keys.contains(&"season".to_string()), "{keys:?}");
        assert_eq!(
            read_back(store, &held, &seasonal.id).await.status,
            FactStatus::Active,
            "a refused write leaves the record where it was"
        );

        // **The same record, taken back: served.** Retraction is deliberate and
        // one-way, and a claim somebody wants taken back has to be takeable
        // back whatever it costs a type.
        let taken = store
            .retract(
                &seasonal.address(),
                Some("it was never so"),
                date(2026, 4, 19),
            )
            .await
            .expect("a retraction is not refused by the fit guard");
        assert_eq!(taken.retracted.status, FactStatus::Retracted);
        assert_eq!(
            read_back(store, &held, &seasonal.id).await.status,
            FactStatus::Retracted,
            "…and the store kept it"
        );
    }

    /// **A declared type governs no write at all.**
    ///
    /// A type is the QUERY vocabulary — how a caller asks which things answer a
    /// shape. What may be taken off a thing is its KIND's question, and the two
    /// were one function until this case existed: a thing that structurally
    /// completed any declaration anybody had made became governed by it, with
    /// nothing offered and nothing switched on.
    ///
    /// **On its own this case proves nothing**, and it must be read beside
    /// [`a_write_cannot_break_a_fit_that_already_exists`], which holds the
    /// other half. Delete the fit guard outright and this one still passes —
    /// "no write is refused" is exactly what a build with no guard does. Only
    /// the kind half can tell the two apart.
    ///
    /// A contract case rather than a domain one: what is under test is that a
    /// STORE takes the write and the key is gone afterwards, which only a store
    /// can answer for.
    pub async fn a_declared_type_governs_no_write<M: Memory>(store: &M) {
        store
            .declare_type(DeclaredType::new(
                "contract-vocabulary",
                vec![
                    Field::required("cost", ValueType::Number),
                    Field::required("done_on", ValueType::Date),
                ],
            ))
            .await
            .expect("declaring a type of my own is accepted");

        // A THING carrying every key that type names. Its kind is `thing`, and
        // `thing` names no keys, so nothing here is governed however completely
        // it answers the vocabulary.
        let answers = EntityId::new(EntityKind::THING, "contract-vocabulary-answerer");
        let costed = capture(
            store,
            NewFact {
                fields: [
                    ("cost".to_string(), "40".to_string()),
                    ("done_on".to_string(), "2026-04-18".to_string()),
                ]
                .into_iter()
                .collect(),
                ..NewFact::about(answers.clone(), "the annual service", date(2026, 4, 18))
            },
        )
        .await;

        edit(
            store,
            &costed.address(),
            FactPatch {
                clear_fields: vec!["cost".to_string()],
                ..Default::default()
            },
        )
        .await;
        assert!(
            !read_back(store, &answers, &costed.id)
                .await
                .fields
                .contains_key("cost"),
            "a declared type is a vocabulary to ask with, so it takes nothing away from a \
             caller: the key goes",
        );
    }

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
        let subject = EntityId::new(EntityKind::THING, "contract-folded-thing");
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

    /// **What a thing holds is the newest WRITE of a key, never the newest
    /// RECORD carrying it.**
    ///
    /// The two orders agree until a key lives on more than one record and the
    /// OLDER record is edited afterwards. Then the edit is the newest write of
    /// that key on that thing while the record it landed in is still the older
    /// one — and a fold that ranked records by their id answers with the value
    /// the edit replaced. The agent edits a claim, is told it worked, reads it
    /// back on the record and in the history, and every reader of "what the
    /// thing IS" keeps serving the stale value.
    pub async fn the_newest_write_wins_however_old_the_record_it_landed_in<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::THING, "contract-edited-older-record");
        let older = capture(
            store,
            NewFact {
                fields: [("ridden_km".to_string(), "10".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the first sitting", date(2026, 4, 18))
            },
        )
        .await;
        capture(
            store,
            NewFact {
                fields: [("ridden_km".to_string(), "20".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the second sitting", date(2026, 4, 19))
            },
        )
        .await;
        let edited = edit(
            store,
            &older.address(),
            FactPatch {
                fields: [("ridden_km".to_string(), "30".to_string())]
                    .into_iter()
                    .collect(),
                ..Default::default()
            },
        )
        .await;
        // The three surfaces that must agree, and the reason the third one
        // disagreeing is not a footnote: the record says the edit landed, the
        // history says it is the newest write, so a thing still holding the
        // value it replaced is the same data read two ways and answering twice.
        assert_eq!(
            edited.fields.get("ridden_km").map(String::as_str),
            Some("30"),
            "the edited record reads back changed, which is the surface"
        );
        assert_eq!(
            store
                .history(&subject, "ridden_km")
                .await
                .expect("history should succeed")
                .last()
                .map(|w| w.value.as_deref()),
            Some(Some("30")),
            "…and the edit is the newest write in the substrate"
        );
        assert_eq!(
            thing_fields(store, &subject)
                .await
                .get("ridden_km")
                .map(String::as_str),
            Some("30"),
            "…so it is what the thing holds, though its record was written first"
        );
    }

    /// **A cleared key stays cleared, and an older record does not put it
    /// back.**
    ///
    /// A clear is a write like any other — the newest one — so it takes the key
    /// off the THING and not merely off the record that carried it. A fold
    /// ranking records would find the key gone from the newest record, fall
    /// back to an older one, and resurrect a value nobody wrote back.
    pub async fn a_cleared_key_is_not_resurrected_by_an_older_record<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::THING, "contract-cleared-key");
        capture(
            store,
            NewFact {
                fields: [("chain_wear".to_string(), "9".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the first sitting", date(2026, 4, 18))
            },
        )
        .await;
        let newer = capture(
            store,
            NewFact {
                fields: [("chain_wear".to_string(), "11".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "the second sitting", date(2026, 4, 19))
            },
        )
        .await;
        edit(
            store,
            &newer.address(),
            FactPatch {
                clear_fields: vec!["chain_wear".to_string()],
                ..Default::default()
            },
        )
        .await;

        let held = thing_fields(store, &subject).await;
        assert!(
            !held.contains_key("chain_wear"),
            "the key was taken off the thing and stayed off: {held:?}"
        );
    }

    /// **A clear names what the ADDRESSED RECORD must not carry.** Naming a key
    /// that record never carried changes nothing — not the record, and not the
    /// thing.
    ///
    /// The contract a caller reads says the patch describes the record, so an
    /// agent tidying one record's keys has no reason to expect another
    /// record's to move. A clear that reached past its address would take the
    /// key off the THING while every surface the caller can see stayed
    /// identical: the receipt is the edited record's own projection, which
    /// never carried the key, and the record that did still reads it back on a
    /// plain recall. Only the fold would know, and nothing told the caller to
    /// look there.
    pub async fn clearing_a_key_the_record_never_carried_changes_nothing<M: Memory>(store: &M) {
        let subject = EntityId::new(EntityKind::THING, "contract-clear-off-address");
        capture(
            store,
            NewFact {
                fields: [("shipping_weight".to_string(), "10".to_string())]
                    .into_iter()
                    .collect(),
                ..NewFact::about(subject.clone(), "weighed at the bench", date(2026, 4, 18))
            },
        )
        .await;
        let bare = capture(
            store,
            NewFact::about(subject.clone(), "rode it home", date(2026, 4, 19)),
        )
        .await;

        let receipt = edit(
            store,
            &bare.address(),
            FactPatch {
                clear_fields: vec!["shipping_weight".to_string()],
                ..Default::default()
            },
        )
        .await;

        assert!(
            !receipt.fields.contains_key("shipping_weight"),
            "the addressed record did not carry the key and still does not: {:?}",
            receipt.fields
        );
        assert_eq!(
            thing_fields(store, &subject)
                .await
                .get("shipping_weight")
                .map(String::as_str),
            Some("10"),
            "…and the record that DOES carry it keeps it on the thing"
        );
        assert_eq!(
            store
                .history(&subject, "shipping_weight")
                .await
                .expect("history should succeed")
                .last()
                .map(|w| w.value.as_deref()),
            Some(Some("10")),
            "…because a clear the record never needed wrote nothing at all"
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
        let subject = EntityId::new(EntityKind::THING, "contract-long-history");
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
        let short = EntityId::new(EntityKind::THING, "contract-short-history");
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
        let cat = EntityId::new(EntityKind::PET, "contract-pet-cat");
        ensure(store, &cat).await;

        let read = store
            .list_entities(Some(EntityKind::PET))
            .await
            .expect("a listing of pets");
        let found = read
            .iter()
            .find(|e| e.id == cat)
            .expect("the pet comes back from a listing of its own kind");
        assert_eq!(
            found.kind,
            EntityKind::PET,
            "and it reads back as a pet rather than as whatever a store defaults to: {found:?}",
        );

        let things = store
            .list_entities(Some(EntityKind::THING))
            .await
            .expect("a listing of things");
        assert!(
            !things.iter().any(|e| e.id == cat),
            "a pet is not a thing, and the store's own filter agrees: {things:?}",
        );
    }

    /// **A rhythm is a noun of its own, and it is refused without a parent.**
    ///
    /// The parent answers whose job the loop is: a maintenance loop sits under
    /// the thing maintained, a review loop under the bot that carries it. A
    /// rhythm nobody owns is a modelling failure rather than a valid shape, so
    /// the store never holds one.
    ///
    /// Both halves, because either alone passes on the wrong build. Without the
    /// refusal, nothing enforces the rule; without the write that succeeds, the
    /// case passes identically on a build that refuses every rhythm there is.
    pub async fn a_rhythm_is_refused_without_a_parent<M: Memory>(store: &M) {
        let owner = EntityId::new(EntityKind::THING, "contract-kettle");
        let orphan = EntityId::new(EntityKind::RHYTHM, "contract-descale");
        add(
            store,
            NewEntity::new(owner.clone(), "Contract Kettle", "contract-fixture"),
        )
        .await;

        let refused = store
            .add_entity(NewEntity::new(
                orphan.clone(),
                "Descale The Kettle",
                "contract-fixture",
            ))
            .await
            .expect_err("a rhythm under nothing is refused");
        assert!(
            matches!(refused, MemoryError::InvalidEntity(_)),
            "the shape is wrong, so it is the entity that is invalid: {refused:?}",
        );
        assert!(
            !store
                .list_entities(Some(EntityKind::RHYTHM))
                .await
                .expect("a listing of rhythms")
                .iter()
                .any(|e| e.id == orphan),
            "a refused rhythm is not in the store",
        );

        // The positive the verdict rests on: the same write with a parent
        // lands, so the refusal above is about the parent and not about the
        // kind being unwritable.
        let held = add(
            store,
            NewEntity {
                parent: Some(owner.clone()),
                ..NewEntity::new(orphan.clone(), "Descale The Kettle", "contract-fixture")
            },
        )
        .await;
        assert_eq!(held.parent.as_ref(), Some(&owner));
        assert_eq!(held.kind, EntityKind::RHYTHM);
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

        a_claim_carries_when_it_was_taken_in(store).await;
        a_claims_lineage_is_walkable_from_its_source(store).await;
        a_folded_value_says_who_backs_it(store).await;
        a_machine_read_claim_names_what_it_was_read_from(store).await;
        referring_to_answers_from_the_far_end(store).await;
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
        a_counter_totals_its_writes_and_keeps_them(store).await;
        an_edit_appends_and_the_value_it_replaced_stays_in_the_history(store).await;
        clearing_a_key_leaves_its_writes_behind(store).await;
        history_of_an_unwritten_key_is_empty_and_of_no_entity_is_a_miss(store).await;

        a_records_fields_survive_capture(store).await;
        a_records_ref_is_screened_by_the_guard(store).await;
        a_reserved_field_key_is_refused(store).await;
        retracting_a_record_marks_it_and_records_why(store).await;
        a_retraction_is_one_way(store).await;
        the_retraction_marker_is_not_one_of_the_things_fields(store).await;
        clearing_the_retraction_marker_is_refused(store).await;
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
        a_field_at_the_validators_limit_survives_storage(store).await;

        a_declared_type_reads_back(store).await;
        declaring_a_type_again_replaces_its_keys(store).await;
        a_type_with_no_keys_is_refused_and_writes_nothing(store).await;
        a_shipped_type_refuses_a_callers_redeclaration(store).await;
        two_stored_types_may_name_one_key(store).await;
        a_stored_type_matches_a_record_that_never_declared_it(store).await;

        a_graph_query_selects_a_kind_and_returns_its_prose(store).await;
        a_graph_query_filters_on_a_stored_value_and_walks_an_edge(store).await;
        a_declared_reference_key_is_walkable_against_the_store(store).await;
        a_trip_records_who_came_and_answers_from_either_end(store).await;
        the_kinds_are_rows_and_a_shipped_one_is_closed(store).await;
        a_pet_is_its_own_kind_in_the_store(store).await;
        a_rhythm_is_refused_without_a_parent(store).await;
        a_thing_reads_back_as_its_fields_folded(store).await;
        the_newest_write_wins_however_old_the_record_it_landed_in(store).await;
        a_cleared_key_is_not_resurrected_by_an_older_record(store).await;
        clearing_a_key_the_record_never_carried_changes_nothing(store).await;
        a_reference_keeps_the_kind_it_points_at(store).await;
        a_write_cannot_put_a_value_the_type_refuses(store).await;
        a_type_named_after_a_kind_does_not_gate_that_kinds_writes(store).await;
        neither_half_writes_over_the_others_keys(store).await;
        a_closed_set_refuses_a_write_outside_it(store).await;
        a_reference_must_name_an_entity_that_exists(store).await;
        a_write_cannot_break_a_fit_that_already_exists(store).await;
        a_supersede_that_breaks_a_fit_is_refused_and_a_retraction_is_not(store).await;
        a_declared_type_governs_no_write(store).await;
        a_long_history_is_cut_to_its_newest_and_says_how_many(store).await;
    }
}
