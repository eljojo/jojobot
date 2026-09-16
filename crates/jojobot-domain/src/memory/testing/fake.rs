use std::sync::Mutex;

use jiff::civil::Date;

use super::*;

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
    /// **The claim substrate: every write of every claim, oldest first.**
    /// Nothing is ever removed from it — an edit and a retraction are writes
    /// like a capture, each carrying the whole of what the claim said then.
    ///
    /// The real store keeps this in a table beside the claim's own row, and the
    /// fake has to keep it too: a fake that answered a claim's history from the
    /// row it holds could only ever answer with one write, and every case about
    /// a correction leaving a trace would pass against a store that keeps none.
    claim_writes: Mutex<Vec<(EntityId, FactId, ClaimWrite)>>,
    /// The human half of each entity's doc, keyed by handle — replaced whole by
    /// `set_prose`, exactly as the real store replaces the region.
    prose: Mutex<std::collections::HashMap<EntityId, String>>,
    /// **The badge each entity wears**, minted when it is created and never
    /// changed after.
    ///
    /// The real store keeps this in a column and the fake has to keep it too:
    /// a fake whose document id is the handle would pass every case about the
    /// index evicting a document while hiding the seam the badge exists to
    /// open. **A polite fake is how a green suite ships a collapsed seam.**
    badges: Mutex<std::collections::HashMap<EntityId, String>>,
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
    /// **What the build supplies over this store**, for the one question a
    /// guard asks: does this handle name something that exists?
    ///
    /// A read resolves supplied records above the store and the guard reads
    /// only the rows, so the two halves disagree about what exists — and the
    /// guard is the half that fails silently, refusing a claim that points at
    /// something a read answers for. The real store holds this for the same
    /// reason and the fake has to, or the case proving it passes here and
    /// fails there.
    supplied: Mutex<crate::memory::owned::Provisions>,
    /// **The clock this store stamps with.** The real store holds one and the
    /// fake has to: a story about an instance acting out a day would otherwise
    /// read *when jojobot took this in* off the wall clock, and pass on a build
    /// where the stated day never reaches the store at all.
    clock: crate::clock::Clock,
    /// **Rename history.** `rename_entity` appends here on every real rename;
    /// [`InMemoryMemory::former_handle_past_the_guard`] is the separate seam
    /// for staging a row directly, the same way `past_the_guard` stages an
    /// entity without going through the guard.
    former_handles: Mutex<Vec<FormerHandle>>,
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

    /// **The store, told what the build supplies over it.** Only the existence
    /// guard reads it: nothing is stored, nothing is listed, and a read still
    /// resolves supplied records in the layer above.
    /// **The store, told which clock it stamps with** — the real store's own
    /// builder, so a fixture wires a day the way the binary does.
    #[must_use]
    pub fn on_clock(mut self, clock: crate::clock::Clock) -> Self {
        self.clock = clock;
        self
    }

    pub fn knowing(self, supplied: crate::memory::owned::Provisions) -> Self {
        *self.supplied.lock().expect("fake mutex poisoned") = supplied;
        self
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

    /// **Move a row to another handle, keeping the badge it wears** — staged
    /// past every verb, because no verb does this.
    ///
    /// There is no rename on the surface and the mention layer does not add
    /// one. What it claims is that text survives a handle moving, and the only
    /// way that state arises today is an edit made outside jojobot — so that is
    /// how the case produces it.
    pub fn rehandle_past_the_guard(&self, from: &EntityId, to: &EntityId) {
        let mut rows = self.entities.lock().expect("fake mutex poisoned");
        let row = rows
            .iter_mut()
            .find(|e| &e.id == from)
            .expect("the row to move is there");
        row.id = to.clone();
        row.kind = to.kind().expect("a staged handle names a kind");
    }

    /// **Stage a rename event** — the record a real rename verb would leave
    /// behind, so a stale handle keeps resolving. No verb writes this yet.
    pub fn former_handle_past_the_guard(&self, event: FormerHandle) {
        self.former_handles
            .lock()
            .expect("fake mutex poisoned")
            .push(event);
    }

    /// The rows this store holds — used where a supplied record has no place
    /// (`list_entities`, and as the base [`InMemoryMemory::known`] extends).
    ///
    /// **A parent pointer is badge-keyed exactly as `home` is** (rule 268),
    /// resolved here on the way out. It names a DIFFERENT row, not this
    /// one's own key, and nothing here queries by it — `children` reads
    /// every entity and filters in memory — so nothing but this resolution
    /// stands between the stored badge and a reader who wants the handle.
    fn index(&self) -> Vec<Entity> {
        let mut entities = self.entities.lock().expect("fake mutex poisoned").clone();
        let snapshot = entities.clone();
        for entity in &mut entities {
            if let Some(parent) = &entity.parent {
                entity.parent = Some(
                    super::super::entity_wearing(parent.as_str(), &snapshot)
                        .map(|found| found.id.clone())
                        .unwrap_or_else(|| parent.clone()),
                );
            }
        }
        entities
    }

    /// **What EXISTS, as any guard that consults the store has to see it**: the
    /// rows, plus what the build supplies over this store (rule 234) — read by
    /// the existence gate and by the creation screen alike.
    ///
    /// A read resolves a supplied record, and a guard reading only the rows
    /// disagrees with it about what exists: it either refuses a claim pointing
    /// at a supplied record, or lets a caller declare a name that shadows one
    /// the near-miss screen exists to catch. **A stored row wins**, so nothing
    /// the operator wrote is shadowed by what the build ships.
    /// ⭐ **The READS ask it too, not only the guards.** A claim may be
    /// written on a supplied record — the write path's gate already reads this
    /// set — so a read gated on the rows alone answered as if that claim did
    /// not exist, over a store holding its rows.
    fn known(&self) -> Vec<Entity> {
        let mut known = self.index();
        for (entity, _) in self.supplied.lock().expect("fake mutex poisoned").records() {
            if !known.iter().any(|held| held.id == entity.id) {
                known.push(entity.clone());
            }
        }
        known
    }

    /// Every rename event, synchronously — the sibling `known()` already is,
    /// for the same reason: the callers that need this are themselves
    /// synchronous helpers taking and releasing the same locks.
    ///
    /// **Newest event first.** A former handle may carry more than one event
    /// — reused after a rename vacated it, or renamed away and back — and
    /// [`super::resolve_handle`] takes the first match in this list.
    /// `former_handles` is pushed to in chronological order, so reversing it
    /// here is what makes the first match the newest one, exactly as the
    /// real store's `ORDER BY ordinal DESC` does.
    fn former(&self) -> Vec<FormerHandle> {
        let mut events = self
            .former_handles
            .lock()
            .expect("fake mutex poisoned")
            .clone();
        events.reverse();
        events
    }

    /// **What a handle is stored as, and what it is called today, together**
    /// — the pair every write and every miss-report needs. A thing wearing a
    /// badge is stored under it — permanent, so a claim addressed or homed
    /// there survives whatever the thing is renamed to next. A
    /// build-supplied record wears no badge and needs none: there is no row
    /// to rename, so its handle is already as permanent as anything here
    /// (mirrors `mention::resolved`'s same fallback for the same reason).
    ///
    /// `None` only when the handle resolves to nothing at all — not now, not
    /// ever — which is the caller's cue to refuse rather than store.
    fn resolve(&self, id: &EntityId) -> Option<(EntityId, EntityId)> {
        let known = self.known();
        let former = self.former();
        let entity = super::resolve_handle(id, &known, &former)?;
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        Some((key, entity.id.clone()))
    }

    /// The storage key alone, for a caller that has no use for the handle.
    fn storage_key(&self, id: &EntityId) -> Option<EntityId> {
        self.resolve(id).map(|(key, _)| key)
    }

    /// **A fact's address, under the handle its home answers to today** —
    /// never the raw `f.home`, which is the storage key and would print as a
    /// badge.
    fn address_under(&self, f: &Fact, handle: &EntityId) -> String {
        FactAddress::new(handle.clone(), f.id.clone()).to_string()
    }

    /// **The other direction**: a stored key — a badge, or an unrenamed
    /// handle stored as itself — read back as whatever handle it answers to
    /// today. A stored value wearing nobody's badge is kept as written: it
    /// predates this mechanism, or it was never a badge to begin with, and
    /// guessing at it would be inventing a resolution nobody asked for.
    fn current_handle(&self, stored: &EntityId) -> EntityId {
        let known = self.known();
        match super::super::entity_wearing(stored.as_str(), &known) {
            Some(entity) => entity.id.clone(),
            None => stored.clone(),
        }
    }

    /// **Every reference-typed field value, lowered from whatever handle a
    /// caller wrote to the permanent id the store keeps** (rule 268, reaching
    /// a field the way it already reaches an edge's object and a ref). Each
    /// item was already checked to exist, so `storage_key` cannot miss on
    /// one; the fallback only covers a caller-supplied value that never
    /// looked like a handle in the first place.
    fn lower_reference_fields(&self, fields: &mut std::collections::BTreeMap<String, String>) {
        let declared = self.declarations();
        for (key, items) in super::super::reference_field_values(fields, &declared) {
            let lowered: Vec<String> = items
                .iter()
                .map(|item| {
                    self.storage_key(&EntityId(item.clone()))
                        .map(|id| id.to_string())
                        .unwrap_or_else(|| item.clone())
                })
                .collect();
            fields.insert(key, lowered.join(", "));
        }
    }

    /// **The other direction**: every reference-typed field value, composed
    /// from the permanent id it is stored as to the handle it answers to
    /// today — the mirror of [`Self::lower_reference_fields`], run once,
    /// right before a fields map reaches whoever asked for it.
    fn compose_reference_fields(&self, fields: &mut std::collections::BTreeMap<String, String>) {
        let declared = self.declarations();
        for (key, items) in super::super::reference_field_values(fields, &declared) {
            let composed: Vec<String> = items
                .iter()
                .map(|item| self.current_handle(&EntityId(item.clone())).to_string())
                .collect();
            fields.insert(key, composed.join(", "));
        }
    }

    /// **The writes [`Self::append_writes`] is about to persist, with every
    /// reference-typed value lowered to the permanent id it names** — the
    /// same treatment [`Self::lower_reference_fields`] gives a whole fields
    /// map, applied to the delta a patch produces instead of the thing's
    /// whole state. A clear carries no value to lower.
    fn lower_writes(&self, writes: Vec<(String, Option<String>)>) -> Vec<(String, Option<String>)> {
        writes
            .into_iter()
            .map(|(key, value)| match value {
                None => (key, None),
                Some(value) => {
                    let mut one = std::collections::BTreeMap::new();
                    one.insert(key.clone(), value);
                    self.lower_reference_fields(&mut one);
                    let lowered = one.remove(&key);
                    (key, lowered)
                }
            })
            .collect()
    }

    /// **Every stored key on a fact, served under the handle it answers to
    /// today** — `home`, `subject`, and a lineage pointer's own `home`, which
    /// is a `FactAddress` like any other and goes stale the same way. Applied
    /// once, right before a fact reaches whoever asked for it.
    fn served(&self, mut f: Fact, handle: &EntityId) -> Fact {
        f.home = handle.clone();
        f.subject = handle.clone();
        if let Some(source) = &f.derived_from {
            f.derived_from = Some(FactAddress::new(
                self.current_handle(&source.home),
                source.local.clone(),
            ));
        }
        // **An edge and a ref are badge-keyed exactly as `home` is** (rule
        // 268), so they are resolved the same way, here, once, rather than a
        // reader chasing a handle's rename history on every use.
        if let Some(edge) = &f.edge {
            f.edge = Some(Edge::new(edge.shape, self.current_handle(&edge.object)));
        }
        f.refs = f
            .refs
            .iter()
            .map(|object| self.current_handle(object))
            .collect();
        f.stands_for = f
            .stands_for
            .iter()
            .map(|named| FactAddress::new(self.current_handle(&named.home), named.local.clone()))
            .collect();
        // **A reference-typed field value is a pointer like any other**, so
        // it is composed here too. Safe to run over the fields this
        // function is handed whether they are already handle form (a
        // capture's own immediate return) or still badge form (a fields
        // map read back off the substrate): resolving an already-current
        // handle finds no badge wearing it and leaves the value as it was.
        self.compose_reference_fields(&mut f.fields);
        f
    }

    /// **Keep this write of the claim**, beside the row it just rewrote — the
    /// same act the real store takes in `write_fact`, so every verb that
    /// produces a claim leaves a write behind here too.
    fn append_claim_write(&self, fact: &Fact) {
        let mut writes = self.claim_writes.lock().expect("fake mutex poisoned");
        let ordinal = writes
            .iter()
            .filter(|(home, id, _)| home == &fact.home && id == &fact.id)
            .count()
            + 1;
        writes.push((
            fact.home.clone(),
            fact.id.clone(),
            // **The moment this write happened**, which the real store stamps
            // too. A fake that left it empty would let every case about a
            // chain of corrections pass on a build that records none.
            ClaimWrite::of(fact, ordinal, Some(self.clock.now())),
        ));
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
        super::super::folded_fields(&self.writes_on(entity, &facts), &self.declarations())
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
    fn writes_on(&self, entity: &EntityId, facts: &[Fact]) -> Vec<super::super::KeyWrite> {
        let writes = self.writes.lock().expect("fake mutex poisoned");
        writes
            .iter()
            .filter(|w| &w.entity == entity)
            .filter_map(|w| {
                let carried = facts
                    .iter()
                    .find(|f| f.home == w.entity && f.id == w.fact)?;
                Some(super::super::KeyWrite {
                    key: w.key.clone(),
                    ordinal: w.ordinal,
                    value: w.value.clone(),
                    fact: w.fact.clone(),
                    status: carried.status,
                    provenance: carried.provenance,
                    standing: carried.standing,
                    // **Off the record the write came from**, exactly as the
                    // real store reads it off the joined row. A fake that left
                    // it empty would pass every case about a caveat riding a
                    // folded value on a build where none does.
                    note: carried.details.clone(),
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
        let index = self.known();
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
            merged_into: None,
            // **Minted with the row and never changed after**, which is what
            // the real store's column does. Drawn from the same alphabet, so a
            // case reasoning about the shape reads the same here.
            badge: Some(crate::handle::draw(6)),
            archived: None,
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
        // **Stored as the badge the parent wears, never the handle it was
        // named with** (rule 268). The check above already found it, so
        // `storage_key` cannot miss here. Kept apart from `entity`, which
        // still carries the handle the caller sent and is served back
        // exactly as written — the same way `capture` never shows a caller
        // the badge it stored an edge's object as.
        let mut stored = entity.clone();
        if let Some(parent) = &entity.parent {
            stored.parent = Some(self.storage_key(parent).unwrap_or_else(|| parent.clone()));
        }
        self.entities
            .lock()
            .expect("fake mutex poisoned")
            .push(stored);
        self.badges.lock().expect("fake mutex poisoned").insert(
            entity.id.clone(),
            entity.badge.clone().expect("a row minted here wears one"),
        );
        Ok(Guarded::Written(entity))
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        Ok(self
            .index()
            .into_iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .collect())
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        Ok(self.former())
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(handle)?;
        // Taken before the lock: known() locks too.
        let index = self.known();
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
        // **Served under the handle, stored under the badge** — `parent`
        // names a different row and never moved under this edit, so what it
        // carries here is whatever was stored, resolved for the reader.
        // `index`, read before this row's lock was taken, still answers: the
        // patch this verb applies never touches another entity's badge.
        let mut served = entity.clone();
        if let Some(parent) = &served.parent {
            served.parent = Some(
                super::super::entity_wearing(parent.as_str(), &index)
                    .map(|found| found.id.clone())
                    .unwrap_or_else(|| parent.clone()),
            );
        }
        Ok(Guarded::Written(served))
    }

    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        validate_write_subject(id)?;
        validate_field("reason", reason)?;
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == id) else {
            if self
                .supplied
                .lock()
                .expect("fake mutex poisoned")
                .record_for(id)
                .is_some()
            {
                return Err(MemoryError::SuppliedHandle {
                    attempted: id.to_string(),
                });
            }
            return Err(MemoryError::UnknownEntity {
                attempted: id.to_string(),
                nearest: guard::screen(id, &[], &entities),
            });
        };
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: id.to_string(),
                into: into.to_string(),
            });
        }
        entity.archived = Some(Archived {
            reason: reason.trim().to_string(),
            at: self.clock.now(),
        });
        Ok(entity.clone())
    }

    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(from)?;
        if from == to {
            return Err(MemoryError::NothingToRename {
                attempted: from.to_string(),
            });
        }
        // **A row, never a supplied record** — the same exception
        // `update_entity` reads off: a rename mutates a stored row, and a
        // build-shipped record has none to mutate. Looking this up through
        // `known()` (which extends with what the build supplies) would let
        // a supplied handle pass this check, fall through every guard, and
        // then silently rename nothing — the mutation loop below only ever
        // touches `self.entities`.
        let stored = self.entities.lock().expect("fake mutex poisoned").clone();
        let known = self.known();
        let Some(entity) = stored.iter().find(|e| &e.id == from).cloned() else {
            // **A supplied record is real and still not a row** — checked
            // before `resolve_handle`, whose own direct-match branch reads
            // the wider `known` set and would otherwise report this exact
            // case as a rename that moved the handle to itself.
            if self
                .supplied
                .lock()
                .expect("fake mutex poisoned")
                .record_for(from)
                .is_some()
            {
                return Err(MemoryError::SuppliedHandle {
                    attempted: from.to_string(),
                });
            }
            let former = self.former();
            if let Some(moved) = super::resolve_handle(from, &known, &former) {
                return Err(MemoryError::HandleMoved {
                    attempted: from.to_string(),
                    now: moved.id.to_string(),
                });
            }
            return Err(MemoryError::UnknownEntity {
                attempted: from.to_string(),
                nearest: guard::screen(from, &[], &stored),
            });
        };
        // A forwarding row is not a thing to rename, for the same reason it
        // is not a side to fold or a survivor to fold into.
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: from.to_string(),
                into: into.to_string(),
            });
        }
        // **`entity.parent` is stored as a badge** (rule 268), so it is
        // resolved back to the handle it answers to today before it reaches
        // validation or a caller — `validate_entity` checks handle grammar,
        // which a bare badge does not have, and a caller was never shown a
        // badge for this field to begin with.
        let effective_parent = match &parent {
            Some(new_parent) => Some(new_parent.clone()),
            None => entity
                .parent
                .as_ref()
                .map(|badge| self.current_handle(badge)),
        };
        validate_entity(
            to,
            &entity.name,
            &entity.aliases,
            &entity.source,
            entity.crm.as_deref(),
            effective_parent.as_ref(),
        )?;
        let renamed = Entity {
            id: to.clone(),
            kind: to.kind().expect("validated above"),
            parent: effective_parent,
            ..entity.clone()
        };
        // Screened like a creation, with the thing's own row excluded from
        // the index it is screened against — or it would collide with its
        // own former name the moment the new one is close to it.
        let others: Vec<Entity> = known.iter().filter(|e| &e.id != from).cloned().collect();
        if let Decision::Block(candidates) =
            guard::decide(to, &renamed.labels(), &others, override_token)
        {
            return Ok(Guarded::Blocked {
                attempted: to.clone(),
                candidates,
            });
        }
        if let Some(new_parent) = &parent
            && let Decision::Block(candidates) = guard::decide_parent(&renamed, new_parent, &others)
        {
            return Ok(Guarded::Blocked {
                attempted: new_parent.clone(),
                candidates,
            });
        }
        // **What actually gets written is the badge**: unchanged when this
        // rename named no new parent (`entity.parent` already is one), or
        // resolved fresh when it did — the guard just above already found
        // it, so `storage_key` cannot miss.
        let stored_parent = match &parent {
            Some(new_parent) => Some(
                self.storage_key(new_parent)
                    .unwrap_or_else(|| new_parent.clone()),
            ),
            None => entity.parent.clone(),
        };

        // **Nothing else needs to move.** A parent pointer is a badge now
        // (rule 268); the badge this row wears never changes across a
        // rename, so a child naming it as `parent` needs no sweep — the
        // former, handle-chasing version of this rename touched every other
        // entity's `parent` field for exactly the reason this no longer
        // does.
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        for held in entities.iter_mut() {
            if &held.id == from {
                held.id = to.clone();
                held.kind = renamed.kind;
                held.parent = stored_parent.clone();
            }
        }
        drop(entities);
        self.former_handles
            .lock()
            .expect("fake mutex poisoned")
            .push(FormerHandle {
                former: from.clone(),
                badge: entity.badge.clone().expect("a written row wears a badge"),
                changed_at: date,
            });
        Ok(Guarded::Written(renamed))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        // Same guards the real adapter applies, so the fake can't drift.
        validate_write_subject(&fact.subject)?;
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
        //
        // **What EXISTS**, which is the rows plus what the build supplies.
        let index = self.known();
        let former = self.former();
        // **A stale-but-renamed subject exists too.** The direct check is
        // what the guard already asks; a miss on it is checked again through
        // the thing's own rename history before it is called unknown. Kept,
        // rather than re-resolved, for its kind and its storage key below.
        let subject_entity = super::resolve_handle(&fact.subject, &index, &former).cloned();
        if subject_entity.is_none()
            && let Decision::Block(candidates) = guard::decide_existing(&fact.subject, &index)
        {
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
        for object in super::super::referenced_by(
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
            validate_write_subject(object)?;
            if let Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
                    candidates,
                });
            }
        }
        // **Stored as the badge the object wears, never the handle it was
        // named with** (rule 268). Both checks above already found it, so
        // `storage_key` cannot miss here. Storing the badge is what makes a
        // later handle collision harmless: there is no plain handle left in
        // the row for a newcomer to inherit.
        let edge = fact.edge.as_ref().map(|edge| {
            Edge::new(
                edge.shape,
                self.storage_key(&edge.object)
                    .unwrap_or_else(|| edge.object.clone()),
            )
        });
        let refs: Vec<EntityId> = fact
            .refs
            .iter()
            .map(|object| self.storage_key(object).unwrap_or_else(|| object.clone()))
            .collect();
        // **A reference-typed field value is stored the same way** — every
        // item checked to exist just above, so nothing here can be an
        // unresolvable handle.
        let mut fields = fact.fields.clone();
        self.lower_reference_fields(&mut fields);

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
            let Some((source_key, source_handle)) = self.resolve(&source.home) else {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &index),
                });
            };
            if !facts
                .iter()
                .any(|f| f.home == source_key && f.id == source.local)
            {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    nearest: facts
                        .iter()
                        .filter(|f| f.home == source_key)
                        .map(|f| self.address_under(f, &source_handle))
                        .collect(),
                });
            }
        }
        // **Stored under its source's storage key, exactly as `home` is** — a
        // lineage pointer is a `FactAddress` like any other, and it goes
        // stale the same way if the entity it names is ever renamed.
        let derived_from = fact.derived_from.as_ref().map(|source| {
            let key = self
                .storage_key(&source.home)
                .expect("checked to exist just above");
            FactAddress::new(key, source.local.clone())
        });
        // **What the subject is stored as** — the badge it wears, or its own
        // handle when it wears none. Already resolved above, direct or
        // through its rename history.
        let subject_entity = subject_entity.expect("checked to exist just above");
        let home = match &subject_entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => subject_entity.id.clone(),
        };
        let existing: Vec<&Fact> = facts.iter().filter(|f| f.home == home).collect();
        let id = FactId(format!("f{}", existing.len() + 1));
        let wrote: Vec<(String, Option<String>)> = fields
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        let stored = Fact {
            id,
            home: home.clone(),
            // One column, stored into both fields — the real store reads
            // the same value into each and this fake must not drift from it.
            subject: home,
            // Edge whitespace doesn't survive a table cell, so it isn't significant.
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            standing,
            status: fact.status,
            recorded_at: fact.recorded_at,
            happened_at: fact.happened_at,
            edge,
            fields: fact.fields,
            refs,
            derived_from,
            // A capture never carries a mark — see [`NewFact`]; the mark is
            // an edit's to make, once the record it names already exists.
            stands_for: Vec::new(),
            // **A store stamps this, so the double does too.** A fake that left
            // it empty would let every case above it pass on a build where the
            // real store's stamp never happens.
            inserted_at: Some(self.clock.now()),
            stale_after: fact.stale_after,
        };
        // **A new record's keys land on the thing too** — the same guard the
        // edit path runs, because a thing's fields are every write on it
        // folded. Run here beside the real store's copy, so a rule that held in
        // one adapter and not the other cannot ship.
        let writes = self.writes_on(&stored.home, &facts);
        let declared = self.types.lock().expect("fake mutex poisoned").clone();
        // **The fold reads both halves and the guard reads one.** How a key
        // folds is declared by whoever declared it; what governs a thing is
        // its own kind, and nothing else. **Read off the subject entity
        // itself, never off `stored.home`** — that is a badge now, and a
        // badge carries no kind token to parse.
        let governs = self.kind_keys_of(subject_entity.kind.as_token());
        super::super::guard_fit(
            subject_entity.kind.as_token(),
            &super::super::folded_fields(&writes, &declared),
            &super::super::stood_after_capture(&writes, &stored, &declared),
            &governs,
        )?;
        // **The claim is kept without its fields and the fields are kept as
        // writes.** One body of data, projected on the way out.
        facts.push(Fact {
            fields: Default::default(),
            ..stored.clone()
        });
        // **A capture is the claim's first write.** Kept here as the real store
        // keeps it, so a claim nobody has corrected answers with one write.
        self.append_claim_write(&stored);
        self.append_writes(&stored.home, &stored.id, wrote);
        // **Stored under the storage key; served under the handle.** The row
        // just pushed keeps the badge, exactly as every other row does; the
        // caller that just wrote it reads back the handle it wrote with,
        // never the internal key — the same rule an edge and a mention
        // already answer to.
        Ok(Guarded::Written(self.served(stored, &subject_entity.id)))
    }

    /// Home-doc membership counts alongside the subject, as it does in the real
    /// store: a row homed here is reachable here, whatever its subject cell says.
    /// The fake cannot produce that disagreement — every capture homes a fact at
    /// its subject — but the two adapters must not differ on the rule.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        // An unknown entity is a miss with its near candidates — never an
        // empty page. Empty-but-real and nonexistent are different answers.
        // A stale-but-renamed handle is not this miss: it resolves through
        // its own history to the one storage key its claims were ever filed
        // under, so this is one lookup rather than a walk of every handle it
        // has worn.
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(subject, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: subject.to_string(),
                nearest: guard::screen(subject, &[], &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let facts = self.facts.lock().expect("fake mutex poisoned");
        let mine: Vec<Fact> = facts
            .iter()
            .filter(|f| f.subject == key || f.home == key)
            .cloned()
            .collect();
        drop(facts);
        // **Resolved to the current handle last**, after the fold above has
        // used the storage key it actually needs. A reader gets the handle
        // this thing wears today, whatever it wore when the claim was filed.
        Ok(mine
            .iter()
            .map(|f| self.served(self.projected(f), &entity.id))
            .collect())
    }

    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let index = self.known();
        let Some(key) = self.resolve(entity).map(|(key, _)| key) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let mut held = self.held(&key);
        self.compose_reference_fields(&mut held);
        Ok(held)
    }

    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(&address.home, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let facts = self.facts.lock().expect("fake mutex poisoned");
        if !facts.iter().any(|f| f.home == key && f.id == address.local) {
            if let Some(err) = super::super::already_merged(&address.home, entity) {
                return Err(err);
            }
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: facts
                    .iter()
                    .filter(|f| f.home == key)
                    .map(|f| self.address_under(f, &entity.id))
                    .collect(),
            });
        }
        drop(facts);
        let writes = self.claim_writes.lock().expect("fake mutex poisoned");
        let mut mine: Vec<ClaimWrite> = writes
            .iter()
            .filter(|(home, id, _)| home == &key && id == &address.local)
            .map(|(_, _, write)| write.clone())
            .collect();
        mine.sort_by_key(|write| write.ordinal);
        // **A lineage pointer in the chain is a `FactAddress` like any
        // other** — stored under its source's storage key, served under the
        // handle that key answers to today.
        for write in &mut mine {
            if let Some(source) = &write.derived_from {
                write.derived_from = Some(FactAddress::new(
                    self.current_handle(&source.home),
                    source.local.clone(),
                ));
            }
        }
        Ok(mine)
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        let index = self.known();
        let former = self.former();
        let Some(resolved) = super::resolve_handle(entity, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };
        let storage_key = match &resolved.badge {
            Some(badge) => EntityId(badge.clone()),
            None => resolved.id.clone(),
        };
        // The record each write arrived in says when it happened and what
        // became of it, so the two are read together.
        let facts = self.facts.lock().expect("fake mutex poisoned").clone();
        let writes = self.writes.lock().expect("fake mutex poisoned");
        let mut mine: Vec<&StoredWrite> = writes
            .iter()
            .filter(|w| w.entity == storage_key && w.key == key)
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
                    fact: FactAddress::new(resolved.id.clone(), carried.id.clone()),
                    recorded_at: carried.recorded_at,
                    status: carried.status,
                    provenance: carried.provenance,
                    standing: carried.standing,
                    note: carried.details.clone(),
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
            if let Decision::Block(candidates) = guard::decide_existing(&edge.object, &self.known())
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
        for object in super::super::referenced_by(
            &patch.fields,
            &self.types.lock().expect("fake mutex poisoned"),
        ) {
            if let Decision::Block(candidates) = guard::decide_existing(&object, &self.known()) {
                return Ok(Guarded::Blocked {
                    attempted: object,
                    candidates,
                });
            }
        }
        // A miss on the HANDLE is an entity miss, with the near candidates that
        // explain it — not a fact miss trailing an empty address list. A
        // stale-but-renamed handle is not this miss: it resolves through its
        // own history to the one storage key its claims were ever filed
        // under, so an address minted before a rename still finds its claim.
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(&address.home, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let handle = entity.id.clone();
        let kind = entity.kind;

        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let nearest: Vec<String> = facts
            .iter()
            .filter(|f| f.home == key)
            .map(|f| self.address_under(f, &handle))
            .collect();
        let Some(found) = facts
            .iter()
            .find(|f| f.home == key && f.id == address.local)
            .cloned()
        else {
            if let Some(err) = super::super::already_merged(&address.home, entity) {
                return Err(err);
            }
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        };
        let fact = &found;
        // An archived row is out of reach of an ordinary edit — checked here,
        // beside the real store's copy, because one-way that holds in only one
        // adapter holds until somebody switches adapters.
        if fact.status == FactStatus::Archived {
            return Err(MemoryError::NotRetractable {
                attempted: address.to_string(),
                why: "it is archived, and an archived record is not editable — archiving is \
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
        if let Some(source) = &patch.derived_from {
            // **A home nobody has heard of is an ENTITY miss**, the same
            // shape `stands_for`'s own check just below already answers
            // with — two shapes, no third: what is absent differs, so what
            // the caller does about it differs.
            let Some((source_key, handle)) = self.resolve(&source.home) else {
                return Err(MemoryError::UnknownEntity {
                    attempted: source.home.to_string(),
                    nearest: guard::screen(&source.home, &[], &self.known()),
                });
            };
            if !facts
                .iter()
                .any(|f| f.home == source_key && f.id == source.local)
            {
                return Err(MemoryError::UnknownFact {
                    attempted: source.to_string(),
                    nearest: facts
                        .iter()
                        .filter(|f| f.home == source_key)
                        .map(|f| self.address_under(f, &handle))
                        .collect(),
                });
            }
        }
        // **Every claim a mark names faces the same existence rule a source
        // does** — a mark is a set of citations, and a citation to nothing
        // is exactly the failure [`derived_from`] is screened against above,
        // just plural.
        if let Some(stands_for) = &patch.stands_for {
            for named in stands_for {
                // **A home nobody has heard of is an ENTITY miss, the same
                // shape `capture`'s own existence check already answers
                // with** — two shapes, no third: what is absent differs, so
                // what the caller does about it differs.
                let Some((named_key, handle)) = self.resolve(&named.home) else {
                    return Err(MemoryError::UnknownEntity {
                        attempted: named.home.to_string(),
                        nearest: guard::screen(&named.home, &[], &self.known()),
                    });
                };
                let Some(named_fact) = facts
                    .iter()
                    .find(|f| f.home == named_key && f.id == named.local)
                else {
                    return Err(MemoryError::UnknownFact {
                        attempted: named.to_string(),
                        nearest: facts
                            .iter()
                            .filter(|f| f.home == named_key)
                            .map(|f| self.address_under(f, &handle))
                            .collect(),
                    });
                };
                // **Self-reference is checked here, on the same storage key
                // the record being edited already resolved to** — never on
                // the name the patch sent, which is not yet in that key
                // space and would let a self-reference through unnoticed
                // whenever a badge is in play (see [`validate_stands_for`]).
                if named_key == key && named.local == address.local {
                    return Err(MemoryError::InvalidFact(format!(
                        "a record cannot be marked as standing for itself: {named} is its own \
                         address"
                    )));
                }
                // **One layer, so the pile is always one step away.** A
                // record already marked as standing for others is itself a
                // shape, and a shape cannot be folded into another mark: that
                // would let a walk that unfolded a shape find a second shape
                // underneath, and a read one layer deep would leave the
                // pile's bottom permanently out of reach.
                if !named_fact.stands_for.is_empty() {
                    return Err(MemoryError::InvalidFact(format!(
                        "{named} already stands for its own sources, so it cannot be folded into \
                         another mark: a shape may only name sources that are not themselves \
                         shapes"
                    )));
                }
            }
        }
        apply_fact_patch(&mut edited, &patch)?;
        // **Stored under its source's storage key, exactly as a capture's
        // does.** `apply_fact_patch` only carries the patch's address
        // through; it does not know a fake's homes are badges, so this
        // corrects it here rather than teaching a shared function about one
        // adapter's storage.
        // **An edge attached by this patch is stored as a badge too**, for
        // the same reason a captured one is (rule 268). `apply_fact_patch`
        // carried the handle the patch named; this is where a fake's own
        // storage rule corrects it, exactly as it does for `derived_from`
        // just below.
        if patch.edge.is_some()
            && let Some(edge) = &edited.edge
        {
            edited.edge = Some(Edge::new(
                edge.shape,
                self.storage_key(&edge.object)
                    .unwrap_or_else(|| edge.object.clone()),
            ));
        }
        if let Some(source) = &patch.derived_from {
            edited.derived_from = Some(FactAddress::new(
                self.storage_key(&source.home)
                    .expect("checked to exist just above"),
                source.local.clone(),
            ));
        }
        // **Stored under each source's storage key, exactly as `derived_from`
        // just above.** A mark's addresses go stale the same way if the
        // entity they name is ever renamed.
        if patch.stands_for.is_some() {
            edited.stands_for = edited
                .stands_for
                .iter()
                .map(|named| {
                    FactAddress::new(
                        self.storage_key(&named.home)
                            .expect("checked to exist just above"),
                        named.local.clone(),
                    )
                })
                .collect();
        }
        // **The thing's fields as they will stand, against the thing's fields
        // as they stand now** — the same guard the real store runs, so the two
        // cannot come to disagree about what a write may cost.
        let home = fact.home.clone();
        let writes = self.writes_on(&home, &facts);
        let declared = self.declarations();
        let before = super::super::folded_fields(&writes, &declared);
        let after = super::super::stood_after(&writes, &edited, &patch, &carried, &declared);
        // **Off the entity resolved above, never off `home`** — that is a
        // badge now, and a badge carries no kind token to parse.
        let governs = self.kind_keys_of(kind.as_token());
        super::super::guard_fit(kind.as_token(), &before, &after, &governs)?;
        let id = fact.id.clone();
        for held in facts.iter_mut() {
            if held.home == home && held.id == id {
                *held = Fact {
                    fields: Default::default(),
                    ..edited.clone()
                };
            }
        }
        // **An edit rewrites the row and appends a write**, which is what makes
        // what the claim used to say readable after it stops being true.
        self.append_claim_write(&edited);
        drop(facts);
        // **Guarded and stored are different questions.** The guard just
        // above validated the patch's own handle-form values, exactly as a
        // caller wrote them (rule 268 needs the shape it named to check the
        // shape); what actually lands is lowered to a permanent id per
        // reference-typed item, same as a capture's does.
        let lowered_writes = self.lower_writes(super::super::writes_of(&patch, &carried));
        self.append_writes(&home, &id, lowered_writes);
        let facts = self.facts.lock().expect("fake mutex poisoned");
        let stored = facts
            .iter()
            .find(|f| f.home == home && f.id == id)
            .expect("the record was just edited in place");
        Ok(Guarded::Written(
            self.served(self.projected(stored), &handle),
        ))
    }

    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Merge, MemoryError> {
        if folded == survivor {
            return Err(MemoryError::NothingToMerge {
                attempted: folded.to_string(),
            });
        }
        let index = self.index();
        // **Rows only, deliberately** — a fold mutates stored rows, and a
        // build-supplied record has none to mutate, exactly as
        // `rename_entity`'s own existence check reads rows only. Reading the
        // wider `known()` set here would let a supplied handle pass this
        // check and fall through to the guard, which the mutation below
        // cannot actually carry out.
        //
        // Both sides are checked before anything moves, so a refusal leaves
        // the store exactly as it was.
        for side in [folded, survivor] {
            if !index.iter().any(|e| &e.id == side) {
                // **A supplied record is real and still not a row.** Reported
                // apart from an ordinary miss (rule 234): the handle reads,
                // lists and searches as existing, so `UnknownEntity` here
                // would tell the caller in the next breath that it does not.
                if self
                    .supplied
                    .lock()
                    .expect("fake mutex poisoned")
                    .record_for(side)
                    .is_some()
                {
                    return Err(MemoryError::SuppliedHandle {
                        attempted: side.to_string(),
                    });
                }
                return Err(MemoryError::UnknownEntity {
                    attempted: side.to_string(),
                    nearest: guard::screen(side, &[], &self.known()),
                });
            }
        }
        // **Neither side may already be a forwarding row.** A chain stops
        // answering in one hop, so it is refused rather than followed.
        for side in [folded, survivor] {
            let held = index
                .iter()
                .find(|e| &e.id == side)
                .expect("checked present just above");
            if let Some(into) = &held.merged_into {
                return Err(MemoryError::AlreadyMerged {
                    attempted: side.to_string(),
                    into: into.to_string(),
                });
            }
        }

        let account = merge_account(folded, survivor, reason, date)?;
        let standing = standing_of(&account);

        // **Both sides resolved to their storage keys once, up front.** Every
        // comparison below is against these — never against the raw handles a
        // caller sent — because a claim's home is the badge now, not the
        // handle, and comparing against the handle would silently move
        // nothing.
        let (folded_key, _) = self.resolve(folded).expect("checked present above");
        let (survivor_key, survivor_handle) =
            self.resolve(survivor).expect("checked present above");

        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let mut facts = self.facts.lock().expect("fake mutex poisoned");

        // **`parent` re-points too, for the same reason `edge.object` and
        // `refs` do below**: it names a different row, and it is stored as
        // the badge that row wears (rule 268) — a fold is the one place that
        // still has to sweep it, because the folded row keeps forwarding
        // rather than disappearing, and its badge is not the survivor's.
        for entity in entities.iter_mut() {
            if let Some(parent) = &entity.parent
                && *parent == folded_key
            {
                entity.parent = Some(survivor_key.clone());
            }
        }

        // **The claims move home, and each is RENUMBERED as it goes.** A fact
        // id is local to the doc that holds it, so both sides own an `f1` and
        // moving a row without a fresh number collides. **The real store finds
        // this and rows in a `Vec` cannot**, so the renumbering is mirrored
        // here rather than left as a difference between the two.
        //
        // ⚠️ Moving a row therefore CHANGES ITS ADDRESS, and what points at
        // that address moves with it.
        let mut next = facts.iter().filter(|f| f.home == survivor_key).count();
        let mut moved: Vec<(FactId, FactId)> = Vec::new();
        for fact in facts.iter_mut() {
            if fact.home == folded_key {
                next += 1;
                let now = FactId(format!("f{next}"));
                moved.push((fact.id.clone(), now.clone()));
                fact.home = survivor_key.clone();
                fact.id = now;
            }
            if fact.subject == folded_key {
                fact.subject = survivor_key.clone();
            }
            // An edge drawn AT the folded thing is re-pointed too, or the
            // graph goes on naming a badge nobody answers to. An edge's
            // object is stored as the badge the entity wears (rule 268), so
            // this compares against the folded side's own badge alone —
            // never a handle, and never a former handle, because nothing but
            // the entity's own current badge was ever written into this
            // column. Rewritten to the SURVIVOR'S BADGE, for the same reason.
            if let Some(edge) = &mut fact.edge
                && edge.object == folded_key
            {
                edge.object = survivor_key.clone();
            }
            for target in fact.refs.iter_mut() {
                if *target == folded_key {
                    *target = survivor_key.clone();
                }
            }
        }
        // A lineage pointer at a moved row follows it to its new address.
        for fact in facts.iter_mut() {
            if let Some(source) = &mut fact.derived_from {
                if source.home == folded_key {
                    if let Some((_, now)) = moved.iter().find(|(was, _)| was == &source.local) {
                        source.local = now.clone();
                    }
                    source.home = survivor_key.clone();
                }
            }
            // A mark at a moved row follows it to its new address, for the
            // same reason [`Fact::derived_from`] just above does.
            for named in fact.stands_for.iter_mut() {
                if named.home == folded_key {
                    if let Some((_, now)) = moved.iter().find(|(was, _)| was == &named.local) {
                        named.local = now.clone();
                    }
                    named.home = survivor_key.clone();
                }
            }
        }
        // The field substrate moves with the claims that wrote it: a folded
        // thing's history is the survivor's history now.
        {
            let mut writes = self.writes.lock().expect("fake mutex poisoned");
            for write in writes.iter_mut() {
                if write.entity == folded_key {
                    write.entity = survivor_key.clone();
                    if let Some((_, now)) = moved.iter().find(|(was, _)| was == &write.fact) {
                        write.fact = now.clone();
                    }
                }
            }
        }
        // **And the claim substrate moves with the claim**, for the same
        // reason: a claim whose row moved and whose writes did not is a claim
        // the projection cannot find, which is the claim gone.
        {
            let mut writes = self.claim_writes.lock().expect("fake mutex poisoned");
            for (home, id, write) in writes.iter_mut() {
                if *home == folded_key {
                    *home = survivor_key.clone();
                    if let Some((_, now)) = moved.iter().find(|(was, _)| was == id) {
                        *id = now.clone();
                    }
                }
                if let Some(edge) = &mut write.edge
                    && edge.object == folded_key
                {
                    edge.object = survivor_key.clone();
                }
                // 🚨 **A lineage pointer at a moved claim follows it here too,
                // wherever the write itself lives.** The real store serves a
                // claim out of its newest write, so a fake that repointed the
                // row and not the write would answer a question the real one
                // gets wrong — and the chain is a reader of lineage either way.
                if let Some(source) = &mut write.derived_from
                    && source.home == folded_key
                {
                    if let Some((_, now)) = moved.iter().find(|(was, _)| was == &source.local) {
                        source.local = now.clone();
                    }
                    source.home = survivor_key.clone();
                }
            }
        }
        let rehomed = moved.len();

        let existing = facts.iter().filter(|f| f.home == survivor_key).count();
        let record = Fact {
            id: FactId(format!("f{}", existing + 1)),
            home: survivor_key.clone(),
            subject: survivor_key.clone(),
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        facts.push(Fact {
            fields: Default::default(),
            ..record.clone()
        });
        self.append_claim_write(&record);

        // **The folded row stays and starts forwarding.** It is not deleted and
        // it is not left looking like a thing.
        for entity in entities.iter_mut() {
            if entity.id == *folded {
                entity.merged_into = Some(survivor.clone());
            }
        }
        let survived = entities
            .iter()
            .find(|e| &e.id == survivor)
            .cloned()
            .expect("the survivor was checked present");
        drop(facts);
        drop(entities);
        self.append_writes(
            &record.home,
            &record.id,
            record
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), Some(value.clone()))),
        );

        Ok(Merge {
            survivor: survived,
            folded: folded.clone(),
            record: self.served(record, &survivor_handle),
            rehomed,
        })
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(&address.home, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let handle = entity.id.clone();

        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was — the same shape `apply_fact_patch` has.
        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        let nearest: Vec<String> = facts
            .iter()
            .filter(|f| f.home == key)
            .map(|f| self.address_under(f, &handle))
            .collect();
        let Some(target) = facts
            .iter()
            .find(|f| f.home == key && f.id == address.local)
            .cloned()
        else {
            if let Some(err) = super::super::already_merged(&address.home, entity) {
                return Err(err);
            }
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest,
            });
        };
        // **Projected before it is judged.** What a record takes back is a key
        // it carries, so a target read without its fields would read as an
        // ordinary claim — and a retraction would become retractable.
        let target = self.projected(&target);
        // **`retraction_of` addresses the claim it takes back in a field
        // value, as free text** — served under the handle, exactly as any
        // other address a reader is given, never under the storage key.
        let account = retraction_of(&self.served(target.clone(), &handle), reason, date)?;
        let standing = standing_of(&account);

        let home = target.home.clone();
        let existing = facts.iter().filter(|f| f.home == home).count();
        let record = Fact {
            id: FactId(format!("f{}", existing + 1)),
            home: home.clone(),
            subject: home,
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock.now()),
            stale_after: None,
        };
        let retracted = Fact {
            status: FactStatus::Archived,
            ..target
        };
        for fact in facts.iter_mut() {
            if fact.home == key && fact.id == address.local {
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
        // **A retraction is two writes**: the one that marked the claim, and
        // the account's first. Taking a claim back is a write of it, so the
        // chain says when it stopped standing rather than only that it did.
        self.append_claim_write(&retracted);
        self.append_claim_write(&record);
        drop(facts);
        self.append_writes(
            &record.home,
            &record.id,
            record
                .fields
                .iter()
                .map(|(field, value)| (field.clone(), Some(value.clone()))),
        );
        // **Served under the handle, stored under the key** — the same rule
        // capture and update_fact answer to.
        Ok(Retraction {
            retracted: self.served(self.projected(&retracted), &handle),
            record: self.served(record, &handle),
        })
    }

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        validate_write_subject(entity)?;
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
        let badges = self.badges.lock().expect("fake mutex poisoned").clone();
        let declared = self.declarations();
        // No Journal document: a wrap publishes nowhere, so the journal stays
        // dark until events land — there is no shared page for `search` to
        // scan here.
        Ok(std::iter::empty()
            .chain(self.index().into_iter().map(|entity| {
                // **The storage key this entity's own rows are filed under**
                // — its badge, or its own handle when it wears none. `home`
                // and `writes_on` both read the key; `facts` served back
                // carry the handle, exactly as any other read does.
                let key = match &entity.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => entity.id.clone(),
                };
                // The scan carries what the thing IS, because the records
                // it also carries cannot be folded back into it.
                let mut fields =
                    super::super::folded_fields(&self.writes_on(&key, &facts), &declared);
                self.compose_reference_fields(&mut fields);
                search::DocScan {
                    doc_id: badges
                        .get(&entity.id)
                        .cloned()
                        .unwrap_or_else(|| entity.id.to_string()),
                    title: entity.name.clone(),
                    prose: prose.get(&entity.id).cloned().unwrap_or_default(),
                    facts: facts
                        .iter()
                        .filter(|f| f.home == key)
                        .map(|f| self.served(self.projected(f), &entity.id))
                        .collect(),
                    fields,
                    entity: Some(entity),
                    // A stored row is the whole instance's, exactly as it was.
                    owner: None,
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
        let mut kinds = self.kinds.lock().unwrap();
        if let Some((held, held_origin)) = kinds.iter_mut().find(|(held, _)| held == token) {
            crate::memory::types::guard_kind_replacement(held, origin, Some(*held_origin))?;
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

    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        use crate::memory::types::Origin;
        let mut kinds = self.kinds.lock().unwrap();
        // **The origin decides**, exactly as the row's column decides in the
        // real store: a kind the operator declared is not this path's to take,
        // whatever name it is handed.
        let owned = kinds
            .iter()
            .any(|(held, origin)| held == token && *origin == Origin::Shipped);
        if !owned {
            return Ok(());
        }
        kinds.retain(|(held, _)| held != token);
        drop(kinds);
        // The keys go with the row, so a name shipped again later starts from
        // nothing rather than inheriting keys it never declared.
        self.kind_keys
            .lock()
            .expect("fake mutex poisoned")
            .remove(token);
        self.types
            .lock()
            .expect("fake mutex poisoned")
            .retain(|t| t.name != token);
        Ok(())
    }
}
