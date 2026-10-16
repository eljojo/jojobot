use std::sync::Mutex;

use jiff::civil::Date;

use super::*;
use crate::memory::claim_walk::{
    ClaimMove, ClaimWalk, MergeLoop, folded_into_walk, follow_claim, follow_thing,
};
use crate::memory::forward_field_value;

/// An in-memory [`Memory`] adapter for tests. Holds entities and facts in `Vec`s
/// behind `Mutex`es; mints fact ids `f1`, `f2`, … per home doc, mirroring the
/// real store's per-doc numbering. A fresh instance starts empty.
/// One merge as the fake keeps it, under the storage keys.
#[derive(Clone)]
struct StoredMerge {
    folded: EntityId,
    survivor: EntityId,
    at: jiff::Timestamp,
    session: Option<String>,
    claims: Vec<(FactId, FactId)>,
}

/// The rows of the fake as a reader sees them once the merges are applied.
/// What a [`World`] was made from.
type WorldKey = (usize, usize, usize, usize, u64);

struct World {
    facts: Vec<Fact>,
    writes: Vec<StoredWrite>,
    claim_writes: Vec<(EntityId, FactId, ClaimWrite)>,
}

impl World {
    /// **Every claim address moved to where the one walk says it went.** A claim
    /// the events map is served under the survivor and the id the newest event
    /// gave it, through a survivor folded later; a claim no event maps stays where
    /// it was made. This is [`follow_claim`], the walk every store applies, and
    /// not a second resolver: the fake asks it for each address it holds.
    fn relocate(&mut self, merges: &[StoredMerge]) {
        let end_of = |home: &EntityId, id: &FactId| {
            follow_claim::<_, MergeLoop>(home.clone(), id.clone(), claim_moves(merges))
                .expect("the fake merges nothing into itself, so its merges never loop")
        };
        let follow = |address: &mut FactAddress| {
            if let Some((home, local)) = end_of(&address.home, &address.local) {
                *address = FactAddress::new(home, local);
            }
        };
        for fact in self.facts.iter_mut() {
            if let Some((home, id)) = end_of(&fact.home, &fact.id) {
                if fact.subject == fact.home {
                    fact.subject = home.clone();
                }
                fact.home = home;
                fact.id = id;
            }
            if let Some(source) = &mut fact.derived_from {
                follow(source);
            }
            for named in fact.stands_for.iter_mut() {
                follow(named);
            }
        }
        for write in self.writes.iter_mut() {
            if let Some((entity, fact)) = end_of(&write.entity, &write.fact) {
                write.entity = entity;
                write.fact = fact;
            }
        }
        for (home, id, write) in self.claim_writes.iter_mut() {
            if let Some((entity, fact)) = end_of(home, id) {
                *home = entity;
                *id = fact;
            }
            if let Some(source) = &mut write.derived_from {
                follow(source);
            }
        }
    }

    /// **Every field value served as the thing each link in it became**, through
    /// the one rule for a value ([`forward_field_value`]) over the one thing walk,
    /// so a value naming a thing folded in a chain names the end of it.
    fn forward_values(
        &mut self,
        merges: &[StoredMerge],
        declared: &[crate::memory::types::DeclaredType],
    ) {
        let survivor_of = survivor_in(merges);
        let expected = "the fake merges nothing into itself, so its merges never loop";
        for fact in self.facts.iter_mut() {
            for (key, value) in fact.fields.iter_mut() {
                if let Some(now) =
                    forward_field_value(key, value, declared, &survivor_of).expect(expected)
                {
                    *value = now;
                }
            }
        }
        for write in self.writes.iter_mut() {
            if let Some(value) = &write.value
                && let Some(now) =
                    forward_field_value(&write.key, value, declared, &survivor_of).expect(expected)
            {
                write.value = Some(now);
            }
        }
    }

    /// **One merge applied to what points at the folded thing**: an edge drawn at
    /// it, a ref and a field value naming it all read as the survivor's.
    fn repoint(&mut self, merge: &StoredMerge) {
        let (folded_key, survivor_key) = (&merge.folded, &merge.survivor);
        for fact in self.facts.iter_mut() {
            if let Some(edge) = &mut fact.edge
                && edge.object == *folded_key
            {
                edge.object = survivor_key.clone();
            }
            for target in fact.refs.iter_mut() {
                if *target == *folded_key {
                    *target = survivor_key.clone();
                }
            }
        }
        for (_, _, write) in self.claim_writes.iter_mut() {
            if let Some(edge) = &mut write.edge
                && edge.object == *folded_key
            {
                edge.object = survivor_key.clone();
            }
        }
    }
}

/// **The events that moved one claim address**, as the walk asks for them: the
/// position of a merge among all merges is its order, so the newest wins.
fn claim_moves(
    merges: &[StoredMerge],
) -> impl Fn(&EntityId, &FactId) -> Result<Vec<ClaimMove<EntityId>>, MergeLoop> + '_ {
    |home, local| {
        Ok(merges
            .iter()
            .enumerate()
            .filter(|(_, merge)| &merge.folded == home)
            .filter_map(|(order, merge)| {
                merge
                    .claims
                    .iter()
                    .find(|(was, _)| was == local)
                    .map(|(_, now)| ClaimMove {
                        order: order as u64,
                        survivor: merge.survivor.clone(),
                        survivor_id: now.clone(),
                    })
            })
            .collect())
    }
}

/// **An entity as a receipt serves it**: with the names of what was folded into
/// it, as a listing of the roster serves them. The stored row carries none, so a
/// receipt built from the row alone would disagree with the listing.
fn naming_folded_in(rows: &[Entity], entity: &mut Entity) {
    let mut listed = rows.to_vec();
    super::super::with_folded_names(&mut listed)
        .expect("the fake merges nothing into itself, so its merges never loop");
    if let Some(row) = listed.into_iter().find(|row| row.id == entity.id) {
        entity.folded_names = row.folded_names;
    }
}

/// **Where the merges sent a thing**, as the thing walk asks: the survivor of the
/// newest merge that folded it, `None` when nothing did.
fn survivor_in(
    merges: &[StoredMerge],
) -> impl Fn(&EntityId) -> Result<Option<EntityId>, MergeLoop> + '_ {
    |folded| {
        Ok(merges
            .iter()
            .rev()
            .find(|merge| merge.folded == *folded)
            .map(|merge| merge.survivor.clone()))
    }
}

/// **Where a thing ended up after every merge**, through a survivor folded in its
/// turn.
fn ended_at(merges: &[StoredMerge], thing: EntityId) -> EntityId {
    follow_thing(thing, survivor_in(merges))
        .expect("the fake merges nothing into itself, so its merges never loop")
}

#[derive(Default)]
pub struct InMemoryMemory {
    /// **Whether a key sent as an own field reaches its thing**, as it did
    /// before own fields stayed on their record. Off by default, as the deployed
    /// store is: an own field stays ([`WriteClass::Stays`]). A case about a store
    /// from before the change builds it with
    /// [`InMemoryMemory::with_own_fields_reaching`].
    own_fields_reach: bool,
    /// **The ledger of spent request ids this store spends into**, in the same
    /// step as the write, as the real store's table is written in the write's
    /// transaction. `None` is a store that keeps no ledger: a write that carries
    /// an id spends nothing.
    requests: Option<std::sync::Arc<crate::request::testing::InMemoryRequests>>,
    entities: Mutex<Vec<Entity>>,
    /// **How many times the whole listing has been asked for**, through
    /// [`Memory::list_entities`]. The real store builds every entity's row on
    /// that call, so a caller that lists to resolve one handle pays for all of
    /// them; this is how a case sees whether it did.
    listings: std::sync::atomic::AtomicUsize,
    /// **Which kind each listing asked for**, in the order they were asked, with
    /// `None` for the whole listing. [`listings`](Self::listings) counts calls; a
    /// case about how much of the store a read pays for needs to see what each
    /// call asked.
    listed_kinds: Mutex<Vec<Option<EntityKind>>>,
    /// **How many times one thing's records have been read**, through
    /// [`Memory::recall`]. A walk reads the records of every thing it has to
    /// choose among, so a lookup that chooses among everything pays for all of
    /// them; this is how a case sees how many it paid for.
    record_reads: std::sync::atomic::AtomicUsize,
    /// **How many times the entity rows have been rebuilt.** Every read that
    /// resolves a handle builds the whole list, so a read that does it once per
    /// entity costs the square of the store; this is how a case sees it.
    index_reads: std::sync::atomic::AtomicUsize,
    /// **How many times the declared types have been cloned out**, and how many
    /// times the whole list of writes has been passed over to project a claim.
    /// Serving a claim does both, so a read that serves every claim pays them
    /// once per claim unless it shares them; these are how a case sees it.
    declaration_reads: std::sync::atomic::AtomicUsize,
    write_scans: std::sync::atomic::AtomicUsize,
    /// **How many times the whole store has been scanned**, through
    /// [`Memory::scan`]. A refresh that re-reads one document goes through
    /// [`Memory::scan_entity`] and does not count; this is how a case sees whether
    /// a read paid for the whole corpus.
    full_scans: std::sync::atomic::AtomicUsize,
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
    /// **When each thing's own row was written**, keyed by its storage key as
    /// the real store keeps `entity_write`: creation, an edit, a rename, an
    /// archive or restore, a merge and a prose write each leave a moment.
    entity_writes: Mutex<Vec<(EntityId, jiff::Timestamp)>>,
    /// The rows as the merges served them, kept until a write changes what they
    /// are made of: the number of rows of each kind, the merges and the edits in
    /// place. A read that serves every claim asks for it once per claim.
    world_cache: Mutex<Option<(WorldKey, std::sync::Arc<World>)>>,
    /// **How many rows have been rewritten in place**, which no length shows.
    edits: std::sync::atomic::AtomicU64,
    /// **Every merge made, as the real store appends it**: the two storage
    /// keys, the moment, the run, and each folded claim's old and new local id.
    merges: Mutex<Vec<StoredMerge>>,
    /// **What each of those writes left the row holding**, and the session that
    /// made it, keyed the same way: the fake's `entity_write` snapshot.
    entity_versions: Mutex<Vec<(EntityId, super::super::EntityVersion)>>,
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
    /// **A caller's own declaration, kept after a shipped write took the name
    /// over.** At most one per name, ever: once a name is shipped, a caller's
    /// write over it is refused, so the name never returns to `Declared` for
    /// this to capture a second time.
    displaced_types: Mutex<Vec<crate::memory::types::Displaced>>,
    /// **The run that sent each declaration a caller made**, by type name, in
    /// the order they were sent. The real store keeps the same account in
    /// `type_write`; what the software ships is not kept.
    type_writes: Mutex<Vec<(String, Option<String>)>>,
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
    clock: std::sync::Arc<Mutex<crate::clock::Clock>>,
    /// **Rename history.** `rename_entity` appends here on every real rename;
    /// [`InMemoryMemory::former_handle_past_the_guard`] is the separate seam
    /// for staging a row directly, the same way `past_the_guard` stages an
    /// entity without going through the guard.
    former_handles: Mutex<Vec<FormerHandle>>,
}

impl InMemoryMemory {
    /// The run that sent each declaration of the type `name`, oldest first.
    /// `None` is a declaration no run was known to send.
    pub fn type_write_sessions(&self, name: &str) -> Vec<Option<String>> {
        self.type_writes
            .lock()
            .unwrap()
            .iter()
            .filter(|(written, _)| written == name)
            .map(|(_, session)| session.clone())
            .collect()
    }

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

    /// How many times the whole listing has been asked for. A caller measures a
    /// delta across the call it is checking.
    pub fn listings(&self) -> usize {
        self.listings.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// The kind each listing has asked for so far, in order. A caller measures a
    /// delta across the call it is checking, as it does with
    /// [`listings`](Self::listings).
    pub fn listed_kinds(&self) -> Vec<Option<EntityKind>> {
        self.listed_kinds
            .lock()
            .expect("fake mutex poisoned")
            .clone()
    }

    /// How many times one thing's records have been read. A caller measures a
    /// delta across the call it is checking.
    pub fn record_reads(&self) -> usize {
        self.record_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the entity rows have been rebuilt, by any read. A caller
    /// measures a delta across the call it is checking.
    pub fn index_reads(&self) -> usize {
        self.index_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the declared types have been cloned out, by any read.
    pub fn declaration_reads(&self) -> usize {
        self.declaration_reads
            .load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the whole store has been scanned. A caller measures a
    /// delta across the call it is checking.
    pub fn full_scans(&self) -> usize {
        self.full_scans.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times the whole list of writes has been passed over, by any
    /// read.
    pub fn write_scans(&self) -> usize {
        self.write_scans.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// **The store, told what the build supplies over it.** Only the existence
    /// guard reads it: nothing is stored, nothing is listed, and a read still
    /// resolves supplied records in the layer above.
    /// **The store, told which clock it stamps with** — the real store's own
    /// builder, so a fixture wires a day the way the binary does.
    #[must_use]
    pub fn on_clock(self, clock: crate::clock::Clock) -> Self {
        *self.clock.lock().expect("fake mutex poisoned") = clock;
        self
    }

    /// **A hand on the clock this store stamps with**, taken before the store
    /// is wrapped and handed to a server, so a fixture can let days pass between
    /// two writes the way a deployment does.
    pub fn clock_handle(&self) -> ClockHandle {
        ClockHandle(self.clock.clone())
    }

    /// The clock as it stands now.
    fn clock(&self) -> crate::clock::Clock {
        *self.clock.lock().expect("fake mutex poisoned")
    }

    /// **Leave the moment a thing's own row was written**, under the key the
    /// real store keeps it: the badge the row wears, else its handle.
    fn touched(&self, entity: &Entity, session: Option<&str>) {
        self.touched_naming(entity, None, session);
    }

    /// **The same moment, naming `parent` where the row still holds a folded
    /// thing's key.** A merge moves no row; the version it appends for a child
    /// says the child sits under the survivor.
    fn touched_naming(&self, entity: &Entity, parent: Option<&EntityId>, session: Option<&str>) {
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let now = self.clock().now();
        self.entity_writes
            .lock()
            .expect("fake mutex poisoned")
            .push((key.clone(), now));
        let prose = self
            .prose
            .lock()
            .expect("fake mutex poisoned")
            .get(&entity.id)
            .cloned()
            .unwrap_or_default();
        let mut versions = self.entity_versions.lock().expect("fake mutex poisoned");
        let ordinal = versions.iter().filter(|(home, _)| home == &key).count() + 1;
        versions.push((
            key,
            super::super::EntityVersion {
                ordinal,
                // **Filled when the history is read**, from the key: a rename
                // since moves the handle this version is served under.
                thing: entity.id.clone(),
                written_at: Some(now),
                session: session.map(str::to_string),
                snapshot: Some(super::super::EntitySnapshot {
                    kind: entity.kind.as_token().to_string(),
                    name: entity.name.clone(),
                    source: entity.source.clone(),
                    crm: entity.crm.clone(),
                    parent: parent.cloned().or_else(|| entity.parent.clone()),
                    boot: entity.boot.as_token().to_string(),
                    archived_reason: entity.archived.as_ref().map(|a| a.reason.clone()),
                    archived_at: entity.archived.as_ref().map(|a| a.at.to_string()),
                    merged_into: entity.merged_into.clone(),
                    aliases: entity.aliases.clone(),
                    prose,
                }),
            },
        ));
    }

    /// **This store spending request ids into `ledger`.** See the field.
    pub fn with_requests(
        mut self,
        ledger: std::sync::Arc<crate::request::testing::InMemoryRequests>,
    ) -> Self {
        self.requests = Some(ledger);
        self
    }

    /// **Spend the id of the call in flight**, when it carries one and this write
    /// is the verb's own: the write that finds the id already spent is refused and
    /// stores nothing. Called under the lock the write is made under, just before
    /// it changes anything.
    fn spend_request(&self, landed: impl FnOnce() -> String) -> Result<(), MemoryError> {
        self.spend_claim(crate::request::take(), landed)
    }

    /// [`Self::spend_request`] for a claim taken earlier, by a write made of
    /// several steps that spends once, after the last one.
    fn spend_claim(
        &self,
        claim: Option<crate::request::Reservation>,
        landed: impl FnOnce() -> String,
    ) -> Result<(), MemoryError> {
        let Some(claim) = claim else {
            return Ok(());
        };
        match &self.requests {
            Some(ledger) => {
                if ledger.spend(claim, landed()) {
                    Ok(())
                } else {
                    Err(MemoryError::Conflict)
                }
            }
            None => {
                claim.failed();
                Ok(())
            }
        }
    }

    /// **This store with own fields reaching their thing.** See the field.
    pub fn with_own_fields_reaching(mut self) -> Self {
        self.own_fields_reach = true;
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

    /// **A booted store whose shipped kinds carry the keys a startup declares
    /// for them** — [`kinds::seed`]'s own two writes, done in this object's
    /// state, then the boot.
    ///
    /// [`booted`](Self::booted) holds the shipped kinds as rows with no keys,
    /// which is right for a case about something else and wrong for one that
    /// walks a relation a shipped kind declares: the walk is refused for naming
    /// a key no type declares, on a store production never runs.
    ///
    /// [`kinds::seed`]: crate::memory::kinds::seed
    pub fn seeded() -> Self {
        let fake = Self::new();
        for token in crate::memory::kinds::SHIPPED {
            fake.keys_of_kind(
                token,
                crate::memory::types::Origin::Shipped,
                crate::memory::kinds::keys_of(token),
            )
            .expect("a shipped kind's keys declare on a fresh store");
        }
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
        if let Some(prior) = types
            .iter()
            .find(|held| held.name == token && !self.kind_keys.lock().unwrap().contains(&held.name))
            .cloned()
        {
            // **A shipped kind landing on a caller's own type of that name is
            // not a refusal** — see the real store's copy of this line: a build
            // that adds a kind has to boot where a type already holds the name,
            // and what the caller declared is remembered, then replaced.
            if origin == crate::memory::types::Origin::Shipped
                && prior.origin == crate::memory::types::Origin::Declared
            {
                self.displaced_types
                    .lock()
                    .unwrap()
                    .push(crate::memory::types::Displaced {
                        name: token.to_string(),
                        fields: prior.fields.clone(),
                        replaced_on: self.clock().today_in(&jiff::tz::TimeZone::UTC),
                    });
            } else {
                return Err(MemoryError::InvalidEntity(format!(
                    "'{token}' already names a declared type, and its keys are not this \
                     declaration's to replace"
                )));
            }
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

    /// **Stage a record's keys without the write guard seeing them** — the
    /// state a thing holds when it was written before a rule existed.
    ///
    /// A loop made before a rhythm's schedule was checked at the making can
    /// hold a cadence and no `advances_from`, and the guard now refuses to
    /// write that. The backstops behind the guard — the check-in refusal, the
    /// overdue read — still serve such loops, and a case about them has to
    /// enter the state the way the state arose, which no verb can.
    ///
    /// **It is for standing up damage, never for convenience.** Anything a
    /// verb can write is written with that verb.
    pub fn fields_past_the_guard(&self, subject: &EntityId, fields: &[(&str, &str)]) {
        let index = self.known();
        let entity = index
            .iter()
            .find(|e| &e.id == subject)
            .expect("the thing to stage keys on is there");
        let home = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let id = FactId(format!(
            "f{}",
            self.facts_view().iter().filter(|f| f.home == home).count() + 1
        ));
        let stored = Fact {
            sets: Default::default(),
            id,
            home: home.clone(),
            subject: home,
            content: "staged past the guard".to_string(),
            details: None,
            // **An inference, so a case about the keys staged here is not also a
            // case about who may edit the operator's words**: testimony with no
            // session is nobody's to edit in place.
            provenance: crate::memory::Provenance::Inference,
            standing: crate::memory::Standing::Open,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 8, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock().now()),
            stale_after: None,
            receipt: None,
        };
        self.append_claim_write(&stored, None);
        self.append_writes(
            &stored.home,
            &stored.id,
            fields
                .iter()
                .map(|(key, value)| (key.to_string(), Some(value.to_string()))),
            &Default::default(),
        );
        self.push_stored(stored);
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
    /// The documents of `rows`, each built the way a scan builds it: its
    /// frontmatter, its prose, the facts homed in it and what the thing holds.
    fn docs_of(&self, rows: Vec<Entity>) -> Vec<search::DocScan> {
        let facts = self.facts_view();
        let prose = self.prose.lock().expect("fake mutex poisoned").clone();
        let badges = self.badges.lock().expect("fake mutex poisoned").clone();
        let declared = self.declarations();
        rows.into_iter()
            .map(|entity| {
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
                        .filter(|f| f.home == key && !self.is_folded_key(&key))
                        .map(|f| self.served(self.projected(f), &entity.id))
                        .collect(),
                    fields,
                    entity: Some(entity),
                    // A stored row is the whole instance's, exactly as it was.
                    owner: None,
                    unmapped: self.unmapped_among(&key, &facts),
                }
            })
            .collect()
    }

    fn index(&self) -> Vec<Entity> {
        self.index_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut entities = self.entities.lock().expect("fake mutex poisoned").clone();
        super::super::with_folded_names(&mut entities)
            .expect("the fake merges nothing into itself, so its merges never loop");
        let snapshot = entities.clone();
        let merges = self.merges.lock().expect("fake mutex poisoned");
        for entity in &mut entities {
            if let Some(parent) = &entity.parent {
                // **A merge moves no row, so a parent left naming a folded thing
                // reads as the survivor it was folded into** — through a survivor
                // folded later.
                let parent = ended_at(&merges, parent.clone());
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

    /// **[`known`](Self::known) with the operator's name and aliases laid over
    /// each supplied record** (rule 508), for the screens that compare NAMES.
    ///
    /// A separate set because the overlay is read off the claims, which takes the
    /// store's locks, and `known` is asked by write paths that already hold them.
    /// **Ask it only where no lock is held.**
    fn known_by_name(&self) -> Vec<Entity> {
        let supplied: Vec<Entity> = self
            .supplied
            .lock()
            .expect("fake mutex poisoned")
            .records()
            .map(|(entity, _)| entity.clone())
            .collect();
        let mut known = self.known();
        for entity in supplied {
            if let Some(at) = known
                .iter()
                .position(|held| held.id == entity.id && held == &entity)
            {
                known[at] = super::super::owned::overlaid(&entity, &self.held(&entity.id));
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
    /// **Whether any thing here numbers its members.** A scan of the writes for
    /// the key, so a store that never wrote it pays nothing for the feature.
    fn carries_numbering(&self) -> bool {
        self.with_writes(|writes| {
            writes
                .iter()
                .any(|w| w.key == super::super::numbered::NUMBERS_BY && w.value.is_some())
        })
    }

    /// **Each thing's folded fields, by handle**, over the claims given.
    fn fields_by_handle(
        &self,
        entities: &[Entity],
        facts: &[Fact],
    ) -> std::collections::BTreeMap<EntityId, std::collections::BTreeMap<String, String>> {
        let declared = self.declarations();
        entities
            .iter()
            .map(|e| {
                let key = match &e.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => e.id.clone(),
                };
                (
                    e.id.clone(),
                    super::super::folded_fields(&self.writes_on(&key, facts), &declared),
                )
            })
            .collect()
    }

    /// The handles a number makes, served as former handles — see
    /// [`numbered::derived_formers`](super::super::numbered::derived_formers).
    fn numbered_formers(&self) -> Vec<FormerHandle> {
        if !self.carries_numbering() {
            return Vec::new();
        }
        let known = self.known();
        let facts = self.facts_view();
        super::super::numbered::derived_formers(&known, &self.fields_by_handle(&known, &facts))
    }

    /// **What a miss offers**: the numbers a collection holds when the handle
    /// asked for is `<collection>/<N>`, else the ordinary near-miss screen.
    fn nearest_for(&self, asked: &EntityId, index: &[Entity]) -> Vec<guard::EntityMatch> {
        if super::super::numbered::numbered_handle(asked).is_some() && self.carries_numbering() {
            let facts = self.facts_view();
            let fields = self.fields_by_handle(index, &facts);
            if let Some(held) = super::super::numbered::nearest_numbers(asked, index, &fields)
                && !held.is_empty()
            {
                return held;
            }
        }
        guard::screen(asked, &[], index)
    }

    /// **Refuse a write that would give a numbered member a bad number, one
    /// another member holds, or would leave a number nobody holds**, over the
    /// claims and the roster the write sees. `under` is the parent a moved thing
    /// takes. The one function every write path asks.
    fn guards_numbering(
        &self,
        change: &super::super::numbered::Change,
        under: Option<&EntityId>,
        rows: &[Entity],
        facts: &[Fact],
    ) -> Result<(), MemoryError> {
        if !self.carries_numbering() && !change.sets_numbering() {
            return Ok(());
        }
        let fields = self.fields_by_handle(rows, facts);
        let reach = super::super::numbered::reach_of(change, under, rows, &fields);
        match reach.refuses(change) {
            Some(refusal) => Err(refusal),
            None => Ok(()),
        }
    }

    fn former(&self) -> Vec<FormerHandle> {
        let mut events = self
            .former_handles
            .lock()
            .expect("fake mutex poisoned")
            .clone();
        events.reverse();
        events.extend(self.numbered_formers());
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
    /// **Where an address went when its thing was merged away**, followed through
    /// a survivor that was merged later. `None` when no merge moved it.
    fn moved(&self, address: &FactAddress) -> Option<FactAddress> {
        let (key, _) = self.resolve(&address.home)?;
        let merges = self.merges.lock().expect("fake mutex poisoned");
        let (key, local) =
            follow_claim::<_, MergeLoop>(key, address.local.clone(), claim_moves(&merges))
                .expect("the fake merges nothing into itself, so its merges never loop")?;
        drop(merges);
        Some(FactAddress::new(self.current_handle(&key), local))
    }

    /// **The stored rows read as the world the merges made.** A merge writes only
    /// its event, as the real store does, and a folded thing's claims stay under
    /// the key and the ids they were made under. This applies every merge, oldest
    /// first, to a copy of the rows: each claim the event maps is served under the
    /// survivor and the id the event gave it, and every pointer at the folded
    /// thing or at a mapped claim reads as the survivor's. A claim the event does
    /// not map stays under the folded key, where it is named as one the survivor
    /// cannot serve.
    fn world(&self) -> std::sync::Arc<World> {
        let key: WorldKey = (
            self.facts.lock().expect("fake mutex poisoned").len(),
            self.writes.lock().expect("fake mutex poisoned").len(),
            self.claim_writes.lock().expect("fake mutex poisoned").len(),
            self.merges.lock().expect("fake mutex poisoned").len(),
            self.edits.load(std::sync::atomic::Ordering::SeqCst),
        );
        {
            let cached = self.world_cache.lock().expect("fake mutex poisoned");
            if let Some((held, world)) = cached.as_ref()
                && *held == key
            {
                return world.clone();
            }
        }
        let mut world = World {
            facts: self.facts.lock().expect("fake mutex poisoned").clone(),
            writes: self.writes.lock().expect("fake mutex poisoned").clone(),
            claim_writes: self
                .claim_writes
                .lock()
                .expect("fake mutex poisoned")
                .clone(),
        };
        let declared = self.types.lock().expect("fake mutex poisoned").clone();
        let merges = self.merges.lock().expect("fake mutex poisoned");
        world.relocate(&merges);
        for merge in merges.iter() {
            world.repoint(merge);
        }
        world.forward_values(&merges, &declared);
        drop(merges);
        let world = std::sync::Arc::new(world);
        *self.world_cache.lock().expect("fake mutex poisoned") = Some((key, world.clone()));
        world
    }

    /// **The world as it would read once `extra` had been merged**, built the way
    /// [`Self::world`] builds the real one and kept nowhere. A write that must be
    /// judged on the state it leaves behind asks this before it writes.
    fn world_if_merged(&self, extra: StoredMerge) -> World {
        let mut world = World {
            facts: self.facts.lock().expect("fake mutex poisoned").clone(),
            writes: self.writes.lock().expect("fake mutex poisoned").clone(),
            claim_writes: self
                .claim_writes
                .lock()
                .expect("fake mutex poisoned")
                .clone(),
        };
        let declared = self.types.lock().expect("fake mutex poisoned").clone();
        let mut merges = self.merges.lock().expect("fake mutex poisoned").clone();
        merges.push(extra);
        world.relocate(&merges);
        for merge in merges.iter() {
            world.repoint(merge);
        }
        world.forward_values(&merges, &declared);
        world
    }

    /// The claims as served: a copy of the rows when no merge has been made.
    fn facts_view(&self) -> Vec<Fact> {
        if self.merges.lock().expect("fake mutex poisoned").is_empty() {
            return self.facts.lock().expect("fake mutex poisoned").clone();
        }
        self.world().facts.clone()
    }

    /// The claim writes as served.
    fn claim_writes_view(&self) -> Vec<(EntityId, FactId, ClaimWrite)> {
        if self.merges.lock().expect("fake mutex poisoned").is_empty() {
            return self
                .claim_writes
                .lock()
                .expect("fake mutex poisoned")
                .clone();
        }
        self.world().claim_writes.clone()
    }

    /// The field writes as served, copied.
    fn writes_view(&self) -> Vec<StoredWrite> {
        self.with_writes(|writes| writes.to_vec())
    }

    /// **The field writes as served**, without a copy while no merge has been made.
    fn with_writes<R>(&self, read: impl FnOnce(&[StoredWrite]) -> R) -> R {
        if self.merges.lock().expect("fake mutex poisoned").is_empty() {
            return read(&self.writes.lock().expect("fake mutex poisoned"));
        }
        read(&self.world().writes)
    }

    /// **The claims of a thing that cannot be served**: stored under a key a merge
    /// folded into it, through a survivor folded later, that the merge's event
    /// does not map. [`World::relocate`] leaves such a claim under the folded key.
    fn unmapped_among(
        &self,
        key: &EntityId,
        facts: &[Fact],
    ) -> Vec<crate::memory::unmapped::UnmappedClaim> {
        let merges = self.merges.lock().expect("fake mutex poisoned");
        if merges.is_empty() {
            return Vec::new();
        }
        let folded_into_key =
            |folded: &EntityId| ended_at(&merges, folded.clone()) == *key && *folded != *key;
        let named: Vec<(EntityId, FactId, u64)> = facts
            .iter()
            .filter_map(|fact| {
                let at = merges.iter().position(|m| m.folded == fact.home)?;
                folded_into_key(&fact.home)
                    .then(|| (fact.home.clone(), fact.id.clone(), at as u64 + 1))
            })
            .collect();
        drop(merges);
        named
            .into_iter()
            .map(
                |(home, id, merge_no)| crate::memory::unmapped::UnmappedClaim {
                    address: FactAddress::new(self.current_handle(&home), id),
                    merge_no,
                },
            )
            .collect()
    }

    /// **Stage a claim under a folded thing's key that the merge's event does not
    /// map** — the state a store holds when a row lands there after the fold. It
    /// is for standing up damage, never for convenience: no verb writes here.
    pub fn stray_claim_under(&self, folded: &EntityId, content: &str) -> FactAddress {
        let (key, handle) = self.resolve(folded).expect("the folded thing is there");
        let id = FactId(format!(
            "f{}",
            self.facts
                .lock()
                .expect("fake mutex poisoned")
                .iter()
                .filter(|f| f.home == key)
                .count()
                + 100
        ));
        let stray = Fact {
            sets: Default::default(),
            id: id.clone(),
            home: key.clone(),
            subject: key.clone(),
            content: content.to_string(),
            details: None,
            provenance: crate::memory::Provenance::Inference,
            standing: crate::memory::Standing::Open,
            status: FactStatus::Active,
            recorded_at: jiff::civil::date(2026, 8, 1),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock().now()),
            stale_after: None,
            receipt: None,
        };
        self.append_claim_write(&stray, None);
        self.push_stored(stray);
        FactAddress::new(handle, id)
    }

    /// **A claim rewritten at the address it is stored at.** The claim is served
    /// under the survivor and an id of the merge's, and its row is under the key
    /// it was made under; writing the served form back as a row of the survivor
    /// would start a second claim at one served address.
    fn rewrite_stored(&self, served: &FactAddress, row: Fact) {
        let at = self.stored_address(served);
        self.edits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut facts = self.facts.lock().expect("fake mutex poisoned");
        for held in facts.iter_mut() {
            if held.home == at.home && held.id == at.local {
                *held = Fact {
                    subject: if row.subject == served.home {
                        at.home.clone()
                    } else {
                        row.subject.clone()
                    },
                    home: at.home.clone(),
                    id: at.local.clone(),
                    ..row.clone()
                };
            }
        }
    }

    /// A new claim, kept under the key it is made under.
    fn push_stored(&self, row: Fact) {
        self.facts.lock().expect("fake mutex poisoned").push(row);
    }

    /// **Where a claim served at an address is stored**: the key and id it was
    /// made under, found by walking the merges back from the newest. An address
    /// no merge gave to a claim is where the claim is stored.
    fn stored_address(&self, served: &FactAddress) -> FactAddress {
        let merges = self.merges.lock().expect("fake mutex poisoned");
        let mut walk = ClaimWalk::new(served.home.clone(), served.local.clone());
        loop {
            let (home, local) = walk.at();
            let origins = merges
                .iter()
                .enumerate()
                .filter(|(_, merge)| merge.survivor == *home)
                .filter_map(|(order, merge)| {
                    merge
                        .claims
                        .iter()
                        .find(|(_, now)| now == local)
                        .map(|(was, _)| ClaimMove {
                            order: order as u64,
                            survivor: merge.folded.clone(),
                            survivor_id: was.clone(),
                        })
                })
                .collect();
            if !walk.step(origins) {
                break;
            }
        }
        match walk
            .finish()
            .expect("the fake merges nothing into itself, so its merges never loop")
        {
            Some((home, local)) => FactAddress::new(home, local),
            None => served.clone(),
        }
    }

    /// **Whether a merge folded this key into another thing.** What such a key
    /// still stores is its survivor's now, so a listing of the thing's claims
    /// serves none of it: a claim no event maps is named by the survivor as left
    /// out, and read at its own address.
    fn is_folded_key(&self, key: &EntityId) -> bool {
        self.merges
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .any(|merge| merge.folded == *key)
    }

    /// **Every key a merge folded into this one**, and every key folded into
    /// those earlier. A key never counts twice, so a loop in the merges ends.
    fn keys_folded_into(
        &self,
        key: &EntityId,
    ) -> Result<std::collections::HashSet<EntityId>, MergeLoop> {
        let merges = self.merges.lock().expect("fake mutex poisoned");
        folded_into_walk(key.clone(), |at| {
            merges
                .iter()
                .filter(|merge| merge.survivor == *at)
                .map(|merge| merge.folded.clone())
                .collect()
        })
        .map(|folded| folded.into_iter().collect())
    }

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

    /// **Each thought's last write moment**, keyed by its local id — what
    /// ageing reads, never the claim's own `recorded_at`. A write kept before
    /// the substrate recorded a moment has no entry, so it is never aged.
    fn touched_moments(
        &self,
        home: &EntityId,
        room: &[Fact],
    ) -> std::collections::HashMap<FactId, jiff::Timestamp> {
        let writes = self.claim_writes_view();
        room.iter()
            .filter_map(|f| {
                writes
                    .iter()
                    .filter(|(h, id, _)| h == home && id == &f.id)
                    .filter_map(|(_, _, w)| w.written_at)
                    .max()
                    .map(|at| (f.id.clone(), at))
            })
            .collect()
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
        // **A handle under a key nobody declared is lowered too**, behind the
        // mark that tells a stored id from a word. Each item was already
        // checked to exist.
        for (key, items) in super::super::handle_field_values(fields, &declared) {
            let lowered: Vec<String> = items
                .iter()
                .map(|item| {
                    let stored = self
                        .storage_key(&EntityId(item.clone()))
                        .map_or_else(|| item.clone(), |id| id.to_string());
                    super::super::marked_stored_handle(&stored)
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
        self.compose_reference_fields_in(&self.resolution(), fields);
    }

    /// [`Self::compose_reference_fields`] over a resolution the caller shares
    /// with every other claim it serves.
    fn compose_reference_fields_in(
        &self,
        resolution: &Resolution<'_>,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) {
        let referenced = super::super::reference_field_values(fields, &resolution.declared);
        for (key, items) in &referenced {
            let composed: Vec<String> = items
                .iter()
                .map(|item| {
                    // A key declared a reference AFTER a handle was stored under
                    // it as an undeclared one holds that value behind the mark.
                    let stored = super::super::stored_handle_id(item).unwrap_or(item);
                    resolution
                        .current_handle(&EntityId(stored.to_string()))
                        .to_string()
                })
                .collect();
            fields.insert(key.clone(), composed.join(", "));
        }
        // **Every other value, served as the handle it answers to now.** A
        // value stored behind the mark is the thing's id. One stored as plain
        // handle text, from before ids were kept, is resolved through the
        // thing's rename history, so it keeps pointing at the thing it named.
        for (key, value) in fields.clone() {
            if referenced.iter().any(|(held, _)| held == &key) {
                continue;
            }
            let items: Vec<&str> = value.split(',').collect();
            if items
                .iter()
                .all(|item| super::super::stored_handle_id(item).is_some())
            {
                let composed: Vec<String> = items
                    .iter()
                    .filter_map(|item| super::super::stored_handle_id(item))
                    .map(|id| {
                        resolution
                            .current_handle(&EntityId(id.to_string()))
                            .to_string()
                    })
                    .collect();
                fields.insert(key, composed.join(", "));
            } else {
                let handles = super::super::handles_in_value(&value);
                if handles.is_empty() {
                    continue;
                }
                let composed: Vec<String> = handles
                    .iter()
                    .map(|id| {
                        super::super::resolve_handle(id, resolution.known(), resolution.former())
                            .map_or_else(|| id.to_string(), |entity| entity.id.to_string())
                    })
                    .collect();
                fields.insert(key, composed.join(", "));
            }
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

    /// **What serving claims needs to resolve a handle, read once.** The rows,
    /// the rename history and the declared types are each a clone of the whole
    /// of something, so a call that serves many claims builds one of these and
    /// hands it to every one, rather than rebuilding them for each claim. The
    /// rows and the rename history are read the first time a value needs them
    /// and not before, as most claims name no handle at all.
    fn resolution(&self) -> Resolution<'_> {
        Resolution {
            store: self,
            declared: self.declarations(),
            known: std::sync::OnceLock::new(),
            former: std::sync::OnceLock::new(),
        }
    }

    /// **Every stored key on a fact, served under the handle it answers to
    /// today.** One claim, so one resolution; a call serving many builds its
    /// own and uses [`Self::served_in`].
    fn served(&self, f: Fact, handle: &EntityId) -> Fact {
        self.served_in(&self.resolution(), f, handle)
    }

    /// **Every stored key on a fact, served under the handle it answers to
    /// today** — `home`, `subject`, and a lineage pointer's own `home`, which
    /// is a `FactAddress` like any other and goes stale the same way. Applied
    /// once, right before a fact reaches whoever asked for it.
    fn served_in(&self, resolution: &Resolution<'_>, mut f: Fact, handle: &EntityId) -> Fact {
        f.home = handle.clone();
        f.subject = handle.clone();
        if let Some(source) = &f.derived_from {
            f.derived_from = Some(FactAddress::new(
                resolution.current_handle(&source.home),
                source.local.clone(),
            ));
        }
        // **An edge and a ref are badge-keyed exactly as `home` is** (rule
        // 268), so they are resolved the same way, here, once, rather than a
        // reader chasing a handle's rename history on every use.
        if let Some(edge) = &f.edge {
            f.edge = Some(Edge::new(
                edge.shape,
                resolution.current_handle(&edge.object),
            ));
        }
        f.refs = f
            .refs
            .iter()
            .map(|object| resolution.current_handle(object))
            .collect();
        f.stands_for = f
            .stands_for
            .iter()
            .map(|named| {
                FactAddress::new(resolution.current_handle(&named.home), named.local.clone())
            })
            .collect();
        // **A reference-typed field value is a pointer like any other**, so
        // it is composed here too. Safe to run over the fields this
        // function is handed whether they are already handle form (a
        // capture's own immediate return) or still badge form (a fields
        // map read back off the substrate): resolving an already-current
        // handle finds no badge wearing it and leaves the value as it was.
        self.compose_reference_fields_in(resolution, &mut f.fields);
        f
    }

    /// **Keep this write of the claim**, beside the row it just rewrote — the
    /// same act the real store takes in `write_fact`, so every verb that
    /// produces a claim leaves a write behind here too.
    fn append_claim_write(&self, fact: &Fact, session: Option<String>) {
        let at = self.stored_address(&FactAddress::new(fact.home.clone(), fact.id.clone()));
        let mut writes = self.claim_writes.lock().expect("fake mutex poisoned");
        let ordinal = writes
            .iter()
            .filter(|(home, id, _)| home == &at.home && id == &at.local)
            .count()
            + 1;
        writes.push((
            at.home.clone(),
            at.local.clone(),
            // **The moment this write happened**, which the real store stamps
            // too. A fake that left it empty would let every case about a
            // chain of corrections pass on a build that records none.
            ClaimWrite {
                session,
                ..ClaimWrite::of(fact, ordinal, Some(self.clock().now()))
            },
        ));
    }

    /// **The session of the claim's FIRST write**, which is the session that
    /// brought the claim in — `None` when that write had none.
    ///
    /// **Read where the claim is stored**, which is not where it is served once a
    /// merge moved it: the writes keep the key and id the claim was made under.
    fn first_write_session(&self, home: &EntityId, id: &FactId) -> Option<String> {
        let stored = self.stored_address(&FactAddress::new(home.clone(), id.clone()));
        self.claim_writes
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .find(|(held, held_id, write)| {
                *held == stored.home && *held_id == stored.local && write.ordinal == 1
            })
            .and_then(|(_, _, write)| write.session.clone())
    }

    /// **Append what a write said about a record's keys**, each taking the next
    /// ordinal for its own (thing, key) — never for the record.
    ///
    /// A value of `None` is a clear: the key stops being current and the writes
    /// that put it there stay where they are.
    fn append_writes<I>(
        &self,
        home: &EntityId,
        fact: &FactId,
        wrote: I,
        classes: &std::collections::BTreeMap<String, super::super::WriteClass>,
    ) where
        I: IntoIterator<Item = (String, Option<String>)>,
    {
        // **Counted over the writes as served, stored where the claim is.** The
        // key's next place on the thing counts the writes of its folded things
        // too, and the write is kept under the key and id the claim was made
        // under.
        let at = self.stored_address(&FactAddress::new(home.clone(), fact.clone()));
        // **One moment for the whole write**, as the real store takes it: the keys
        // of one record's write are one act, so none is a later change than
        // another. The review stamp rides such a write and reads as reviewed only
        // if it is not earlier than the keys beside it.
        let moment = self.clock().now();
        for (key, value) in wrote {
            let ordinal = self.with_writes(|served| {
                served
                    .iter()
                    .filter(|w| &w.entity == home && w.key == key)
                    .count()
            }) as u64
                + 1;
            let class = classes
                .get(key.trim())
                .copied()
                .unwrap_or(super::super::WriteClass::Own);
            self.writes
                .lock()
                .expect("fake mutex poisoned")
                .push(StoredWrite {
                    entity: at.home.clone(),
                    key,
                    ordinal,
                    value,
                    fact: at.local.clone(),
                    class,
                    written_at: Some(moment),
                });
        }
    }

    /// **The bag each key of a record is held under now**, as the real store reads
    /// it: the class of the key's newest write on the record, for a key whose
    /// newest write did not take it off.
    fn held_classes(
        &self,
        home: &EntityId,
        fact: &FactId,
    ) -> std::collections::BTreeMap<String, super::super::WriteClass> {
        self.with_writes(|writes| {
            let mut mine: Vec<&StoredWrite> = writes
                .iter()
                .filter(|w| &w.entity == home && &w.fact == fact)
                .collect();
            mine.sort_by(|a, b| a.key.cmp(&b.key).then(a.ordinal.cmp(&b.ordinal)));
            let mut held = std::collections::BTreeMap::new();
            for write in mine {
                match &write.value {
                    Some(_) => held.insert(write.key.clone(), write.class),
                    None => held.remove(&write.key),
                };
            }
            held
        })
    }

    /// **The record as a reader sees it: its claim, with its fields projected.**
    ///
    /// One value per key — the newest write of that key made by THIS record —
    /// and a key whose newest write inside the record took it off is not there.
    fn projected(&self, fact: &Fact) -> Fact {
        self.write_scans
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.with_writes(|writes| {
            let mine: Vec<&StoredWrite> = writes
                .iter()
                .filter(|w| w.entity == fact.home && w.fact == fact.id)
                .collect();
            Self::project(fact, mine)
        })
    }

    /// **Every write, grouped by the record that carried it, in one pass.** A
    /// read that projects every claim asks this once and then looks each claim
    /// up, instead of passing over all the writes for each claim.
    fn writes_by_record(&self) -> std::collections::HashMap<(EntityId, FactId), Vec<StoredWrite>> {
        self.write_scans
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut grouped: std::collections::HashMap<(EntityId, FactId), Vec<StoredWrite>> =
            std::collections::HashMap::new();
        self.with_writes(|writes| {
            for write in writes {
                grouped
                    .entry((write.entity.clone(), write.fact.clone()))
                    .or_default()
                    .push(write.clone());
            }
        });
        grouped
    }

    /// [`Self::projected`] for a claim whose writes were grouped by
    /// [`Self::writes_by_record`].
    fn projected_from(
        grouped: &std::collections::HashMap<(EntityId, FactId), Vec<StoredWrite>>,
        fact: &Fact,
    ) -> Fact {
        let mine: Vec<&StoredWrite> = grouped
            .get(&(fact.home.clone(), fact.id.clone()))
            .map(|writes| writes.iter().collect())
            .unwrap_or_default();
        Self::project(fact, mine)
    }

    fn project(fact: &Fact, mut mine: Vec<&StoredWrite>) -> Fact {
        mine.sort_by_key(|w| w.ordinal);
        let mut fields = std::collections::BTreeMap::new();
        let mut sets = std::collections::BTreeSet::new();
        for write in mine {
            match &write.value {
                Some(value) => {
                    fields.insert(write.key.clone(), value.clone());
                    // **The class of the newest write decides the bag**, as the
                    // real store reads it: a key set on the thing is here, and
                    // one rewritten as an own field is not.
                    if write.class == super::super::WriteClass::Sets {
                        sets.insert(write.key.clone());
                    } else {
                        sets.remove(&write.key);
                    }
                }
                None => {
                    fields.remove(&write.key);
                    sets.remove(&write.key);
                }
            };
        }
        Fact {
            fields,
            sets,
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
        let facts = self.facts_view();
        super::super::folded_fields(&self.writes_on(entity, &facts), &self.declarations())
    }

    /// **The chart around a write**, read from the records the write holds: the
    /// bots above `subject`, the manager the write names and the bots above
    /// that one. Passed the records rather than locking them, because the write
    /// path reads this while it already holds them.
    fn lineage_among(
        &self,
        facts: &[Fact],
        subject: &EntityId,
        named: Option<EntityId>,
    ) -> super::super::Lineage {
        let above = self.chain_above_among(facts, subject);
        let above_named = match &named {
            Some(manager) => self.chain_above_among(facts, manager),
            None => Vec::new(),
        };
        // Asked only by the write that names a manager for a thing with none.
        let has_reports =
            above.is_empty() && named.is_some() && self.has_reports_among(facts, subject);
        // The guarded keys the subject holds now, off its fold before this write.
        let holds = match self.storage_key(subject) {
            Some(key) => super::super::held_guarded_keys(&super::super::folded_fields(
                &self.writes_on(&key, facts),
                &self.declarations(),
            )),
            None => Default::default(),
        };
        // The bots that head the chart, read only when the subject holds a key
        // only the head may change.
        let heads = if super::super::heads_wanted(&holds) {
            self.chart_heads_among(facts)
        } else {
            Vec::new()
        };
        // The bots that wrote a thought into the subject, when it is a thread.
        let (writers, above_writers) = if subject.kind() == Some(EntityKind::THREAD) {
            let writers = self.writers_among(facts, subject);
            let mut above_writers: Vec<EntityId> = Vec::new();
            for writer in &writers {
                for bot in self.chain_above_among(facts, writer) {
                    if !above_writers.contains(&bot) {
                        above_writers.push(bot);
                    }
                }
            }
            (writers, above_writers)
        } else {
            (Vec::new(), Vec::new())
        };
        super::super::Lineage {
            above,
            named,
            above_named,
            has_reports,
            holds,
            heads,
            writers,
            above_writers,
        }
    }

    /// **The bots that have written a thought into `thread`**, read off the
    /// records the write holds, as the real store reads them in its transaction.
    fn writers_among(&self, facts: &[Fact], thread: &EntityId) -> Vec<EntityId> {
        let claims: Vec<Fact> = facts
            .iter()
            .filter(|claim| claim.edge.is_some())
            .map(|claim| self.served(claim.clone(), &self.current_handle(&claim.home)))
            .collect();
        // **An archived bot writes nothing**, so it is not one of the writers:
        // its thoughts stay where they are and it does not count.
        let known = self.known();
        let mut writers = super::super::writers_in(thread, &claims);
        writers.retain(|bot| {
            known
                .iter()
                .find(|entity| &entity.id == bot)
                .is_none_or(|entity| entity.archived.is_none())
        });
        writers
    }

    /// **Whether any other thing reports to `subject`**, read off each thing's
    /// fold served as handles, the way the chain is.
    fn has_reports_among(&self, facts: &[Fact], subject: &EntityId) -> bool {
        let declared = self.declarations();
        self.known()
            .iter()
            .filter(|other| &other.id != subject)
            .any(|other| {
                let held = match self.storage_key(&other.id) {
                    Some(key) => {
                        super::super::folded_fields(&self.writes_on(&key, facts), &declared)
                    }
                    None => Default::default(),
                };
                super::super::reports_to(&self.with_manager_served(&held), subject)
            })
    }

    /// **The bots that head the chart**: each manager something reports to that
    /// has no manager of its own, off each thing's fold served as handles.
    fn chart_heads_among(&self, facts: &[Fact]) -> Vec<EntityId> {
        let declared = self.declarations();
        let managers: std::collections::BTreeSet<EntityId> = self
            .known()
            .iter()
            .filter_map(|other| {
                let key = self.storage_key(&other.id)?;
                let held = self.with_manager_served(&super::super::folded_fields(
                    &self.writes_on(&key, facts),
                    &declared,
                ));
                held.get(super::super::REPORTS_TO)
                    .and_then(|value| super::super::manager_in(value))
            })
            .collect();
        managers
            .into_iter()
            .filter(|manager| self.chain_above_among(facts, manager).is_empty())
            .collect()
    }

    /// The bots above `start` on its `reports_to` chain, nearest first.
    fn chain_above_among(&self, facts: &[Fact], start: &EntityId) -> Vec<EntityId> {
        let declared = self.declarations();
        let mut walk = super::super::ChainWalk::from(start);
        while let Some(at) = walk.at().cloned() {
            let held = match self.storage_key(&at) {
                Some(key) => super::super::folded_fields(&self.writes_on(&key, facts), &declared),
                None => Default::default(),
            };
            let held = self.with_manager_served(&held);
            walk.step(held.get(super::super::REPORTS_TO).map(String::as_str));
        }
        walk.above()
    }

    /// **A fold served with its `reports_to` as the handle it answers to
    /// today**, through the one domain step every store's chart question goes
    /// through. This store keeps the permanent id under any key that names an
    /// entity, so the walk and the manager a write names are judged in handles
    /// only after this.
    fn with_manager_served(
        &self,
        fold: &std::collections::BTreeMap<String, String>,
    ) -> std::collections::BTreeMap<String, String> {
        let known = self.known();
        super::super::with_manager_served(fold, |stored| {
            // A manager folded into another is the one it became.
            follow_thing::<_, MergeLoop>(self.current_handle(stored), |at| {
                Ok(known
                    .iter()
                    .find(|e| &e.id == at)
                    .and_then(|e| e.merged_into.clone()))
            })
            .expect("the fake merges nothing into itself, so its merges never loop")
        })
    }

    /// What has been declared — which is what says how each key folds.
    fn declarations(&self) -> Vec<crate::memory::types::DeclaredType> {
        self.declaration_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.types.lock().expect("fake mutex poisoned").clone()
    }

    /// **Every write on this thing, each with the standing of the record that
    /// carried it** — the substrate the fold and the write guard both read.
    ///
    /// The records are passed in rather than locked here, because the write
    /// path reads this while it is already holding them.
    /// **A role object's lease state**, or `None` when the entity is no role. The
    /// role's own folded fields answer.
    fn role_state_for(
        &self,
        entity: &Entity,
        facts: &[Fact],
    ) -> Option<(String, crate::session::RoleState)> {
        let role = super::super::role_named_by(&entity.id)?;
        let stored_as = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let child =
            super::super::folded_fields(&self.writes_on(&stored_as, facts), &self.declarations());
        let state = crate::session::role_state(Some(&child));
        Some((role, state))
    }

    /// **The one place a write of the fake's field substrate is changed after
    /// it was made, and the only key it may change.** A role's lease moment is
    /// machine state: the holder is alive, and what changes is when. Appending a
    /// write per renewal grew the claim for as long as the role was held, and
    /// nothing reads those writes back. So a renewal replaces the value of the
    /// newest `claimed_at` write on the role and adds nothing. A claim and a
    /// release still append, and so does every other key, which is what the
    /// contract's second role case holds.
    ///
    /// `false` when the role has no live moment to overwrite (a role whose object
    /// holds no moment yet, whose first renewal is an append): the caller
    /// then writes as it always did.
    fn overwrite_lease_moment(&self, home: &EntityId, moment: &str) -> bool {
        let now = self.clock().now();
        self.edits.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut writes = self.writes.lock().expect("fake mutex poisoned");
        let newest = writes
            .iter_mut()
            .filter(|w| &w.entity == home && w.key == crate::session::ROLE_CLAIMED_AT)
            .max_by_key(|w| w.ordinal);
        match newest {
            Some(write) if write.value.is_some() => {
                write.value = Some(moment.to_string());
                // **Restamped with the value**, as the real store does: the age
                // of a key is the age of the write holding it, and this write
                // is the renewal now.
                write.written_at = Some(now);
                true
            }
            _ => false,
        }
    }

    fn writes_on(&self, entity: &EntityId, facts: &[Fact]) -> Vec<super::super::KeyWrite> {
        self.with_writes(|writes| Self::key_writes_on(writes, entity, facts))
    }

    fn key_writes_on(
        writes: &[StoredWrite],
        entity: &EntityId,
        facts: &[Fact],
    ) -> Vec<super::super::KeyWrite> {
        writes
            .iter()
            .filter(|w| &w.entity == entity)
            // **The one reading of whether a write reaches its thing**, shared by
            // every store and the fake.
            .filter(|w| super::super::reaches_the_thing(w.class))
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
                    written_at: w.written_at,
                })
            })
            .collect()
    }
}

/// A hand on the clock a fake store stamps with. See
/// [`InMemoryMemory::clock_handle`].
#[derive(Clone)]
pub struct ClockHandle(std::sync::Arc<Mutex<crate::clock::Clock>>);

impl ClockHandle {
    /// From now on the store stamps on this day.
    pub fn stating(&self, day: jiff::civil::Date) {
        *self.0.lock().expect("fake mutex poisoned") = crate::clock::Clock::stating(day);
    }
}

/// One row of the fake's field substrate — the shape the real store keeps in a
/// table, kept here so the two cannot come to disagree about what a read
/// projects.
#[derive(Clone)]
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
    /// **The bag this write was made under**, kept as the real store keeps it
    /// in a column.
    class: super::super::WriteClass,
    /// When the store took the write in, stamped at the append as the real
    /// store does.
    written_at: Option<jiff::Timestamp>,
}

/// **What serving claims resolves handles through**, built once by a call that
/// serves many. The declared types are cloned when it is built; the rows and
/// the rename history are read the first time a value holds a handle, and then
/// kept for the rest of the call.
struct Resolution<'a> {
    store: &'a InMemoryMemory,
    declared: Vec<crate::memory::types::DeclaredType>,
    known: std::sync::OnceLock<Vec<Entity>>,
    former: std::sync::OnceLock<Vec<FormerHandle>>,
}

impl Resolution<'_> {
    fn known(&self) -> &[Entity] {
        self.known.get_or_init(|| self.store.known())
    }

    fn former(&self) -> &[FormerHandle] {
        self.former.get_or_init(|| self.store.former())
    }

    /// The handle a stored value answers to today, as
    /// [`InMemoryMemory::current_handle`] answers it, over the rows read once.
    fn current_handle(&self, stored: &EntityId) -> EntityId {
        match super::super::entity_wearing(stored.as_str(), self.known()) {
            Some(entity) => entity.id.clone(),
            None => stored.clone(),
        }
    }
}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let session = new.session.clone();
        validate_entity(
            &new.id,
            &new.name,
            &new.aliases,
            &new.source,
            new.crm.as_deref(),
            new.parent.as_ref(),
        )?;
        let index = self.known_by_name();
        if let Decision::Block(candidates) = guard::decide_creating(
            &new.id,
            &new.labels(),
            new.parent.as_ref(),
            &index,
            new.override_token.as_deref(),
            new.override_reason.as_deref(),
        ) {
            return Ok(Guarded::Blocked {
                attempted: new.id,
                candidates,
            });
        }
        let entity = Entity {
            folded_names: Vec::new(),
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
        self.spend_request(|| format!("{{\"handle\":\"{}\"}}", entity.id))?;
        self.entities
            .lock()
            .expect("fake mutex poisoned")
            .push(stored);
        self.badges.lock().expect("fake mutex poisoned").insert(
            entity.id.clone(),
            entity.badge.clone().expect("a row minted here wears one"),
        );
        self.touched(&entity, session.as_deref());
        Ok(Guarded::Written(entity))
    }

    /// **The entity and its first claim, as one act.** A creation writes two
    /// things here, the entity and the badge it wears, so a refusal of the claim
    /// takes back exactly those two and the refusal is returned as it came. The
    /// real store does this in one transaction; a fake that left the entity behind
    /// would pass a case the real store fails.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if first.subject != new.id {
            return Err(MemoryError::InvalidFact(format!(
                "the first claim is about {}, and the thing being made is {}",
                first.subject, new.id
            )));
        }
        // **One spend for the act**, after its last step: the inner writes take
        // nothing because this has taken the claim.
        let claim = crate::request::take();
        let id = new.id.clone();
        let entity = match self.add_entity(new).await? {
            Guarded::Written(entity) => entity,
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                return Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                });
            }
        };
        let take_back = || {
            self.entities
                .lock()
                .expect("fake mutex poisoned")
                .retain(|held| held.id != id);
            self.badges.lock().expect("fake mutex poisoned").remove(&id);
        };
        match self.capture(first).await {
            Ok(Guarded::Written(fact)) => {
                if let Err(e) = self.spend_claim(claim, || {
                    format!(
                        "{{\"handle\":\"{}\",\"address\":\"{}\"}}",
                        entity.id,
                        fact.address()
                    )
                }) {
                    take_back();
                    // The claim was written above and is taken back with the entity.
                    return Err(e);
                }
                Ok(Guarded::Written((entity, fact)))
            }
            Ok(Guarded::Blocked {
                attempted,
                candidates,
            }) => {
                take_back();
                Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                })
            }
            Err(e) => {
                take_back();
                Err(e)
            }
        }
    }

    /// **The edit and its claim, as one act.** The edit rewrites the entity's own
    /// row, so a refusal of the claim puts that row back exactly as it was and the
    /// refusal is returned as it came. The real store does this in one
    /// transaction; a fake that left the edit behind would pass a case the real
    /// store fails.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if claim.subject != *handle {
            return Err(MemoryError::InvalidFact(format!(
                "the claim is about {}, and the thing being edited is {handle}",
                claim.subject
            )));
        }
        // **One spend for the act**, after its last step.
        let request = crate::request::take();
        let before = self
            .entities
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .find(|held| held.id == *handle)
            .cloned();
        let entity = match self.update_entity(handle, patch).await? {
            Guarded::Written(entity) => entity,
            Guarded::Blocked {
                attempted,
                candidates,
            } => {
                return Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                });
            }
        };
        let take_back = || {
            if let Some(before) = &before {
                let mut held = self.entities.lock().expect("fake mutex poisoned");
                if let Some(row) = held.iter_mut().find(|row| row.id == before.id) {
                    *row = before.clone();
                }
            }
        };
        match self.capture(claim).await {
            Ok(Guarded::Written(fact)) => {
                if let Err(e) = self.spend_claim(request, || {
                    format!(
                        "{{\"handle\":\"{}\",\"address\":\"{}\"}}",
                        entity.id,
                        fact.address()
                    )
                }) {
                    take_back();
                    return Err(e);
                }
                Ok(Guarded::Written((entity, fact)))
            }
            Ok(Guarded::Blocked {
                attempted,
                candidates,
            }) => {
                take_back();
                Ok(Guarded::Blocked {
                    attempted,
                    candidates,
                })
            }
            Err(e) => {
                take_back();
                Err(e)
            }
        }
    }

    /// **The port's default walk, over the fake's own index.** The real store
    /// answers this with one targeted read and never lists, so the default
    /// body, which calls [`Memory::list_entities`], would count a listing no
    /// real caller pays for.
    /// **Answered from the claims directly, without listing the entities**, as
    /// the real store answers it. A decorator that does not forward the call gets
    /// the default instead, which lists every bot, and the listing counter is how
    /// a case sees it.
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        super::super::validate_subject(thread)?;
        // **The thread is found as every read finds a thing**, by the handle it
        // wears or one it wore, and a thread nothing answers to has no writers.
        // The real store resolves it the same way before it reads a claim.
        let known = self.known();
        let Some(thread) = super::resolve_handle(thread, &known, &self.former()) else {
            return Ok(Vec::new());
        };
        let facts = self.facts_view();
        // **In the order the real store reads its rows**, by the key each bot is
        // filed under, whatever order the claims were written in.
        let mut writers = self.writers_among(&facts, &thread.id);
        writers.sort_by_key(|bot| self.storage_key(bot));
        Ok(writers)
    }

    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        let moved = self.moved(source);
        let source = moved.as_ref().unwrap_or(source);
        super::super::validate_subject(&source.home)?;
        // A pointer names its source by the handle the thing wears now, so an
        // address written under a former handle is compared as the current one.
        let current = super::resolve_handle(&source.home, &self.known(), &self.former())
            .map(|entity| FactAddress::new(entity.id.clone(), source.local.clone()));
        let source = current.as_ref().unwrap_or(source);
        let mut standing_on = Vec::new();
        for entity in self.index() {
            for fact in self.recall(&entity.id).await? {
                if fact.derived_from.as_ref() == Some(source) {
                    standing_on.push(fact);
                }
            }
        }
        Ok(standing_on)
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.listings
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        self.listed_kinds
            .lock()
            .expect("fake mutex poisoned")
            .push(kind);
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
        let session = patch.session.clone();
        validate_write_subject(handle)?;
        // Taken before the lock: known() locks too.
        let index = self.known_by_name();
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == handle) else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: self.nearest_for(handle, &entities),
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
        let before = entity.clone();
        apply_entity_patch(entity, &patch)?;
        if let Err(e) = self.spend_request(|| format!("{{\"handle\":\"{handle}\"}}")) {
            *entity = before;
            return Err(e);
        }
        self.touched(entity, session.as_deref());
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
        naming_folded_in(&entities, &mut served);
        Ok(Guarded::Written(served))
    }

    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
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
                nearest: self.nearest_for(id, &entities),
            });
        };
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: id.to_string(),
                into: into.to_string(),
            });
        }
        if entity.archived.is_some() {
            return Err(MemoryError::AlreadyArchived {
                attempted: id.to_string(),
            });
        }
        self.spend_request(|| format!("{{\"handle\":\"{id}\"}}"))?;
        entity.archived = Some(Archived {
            reason: reason.trim().to_string(),
            at: self.clock().now(),
        });
        self.touched(entity, session);
        let mut served = entity.clone();
        naming_folded_in(&entities, &mut served);
        Ok(served)
    }

    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        validate_write_subject(id)?;
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
                nearest: self.nearest_for(id, &entities),
            });
        };
        if let Some(into) = &entity.merged_into {
            return Err(MemoryError::AlreadyMerged {
                attempted: id.to_string(),
                into: into.to_string(),
            });
        }
        if entity.archived.is_none() {
            return Err(MemoryError::NotArchived {
                attempted: id.to_string(),
            });
        }
        let archived = entity.archived.clone().expect("checked just above");
        self.spend_request(|| {
            format!(
                "{{\"handle\":\"{id}\",\"archived_at\":\"{}\",\"archived_reason\":{}}}",
                archived.at,
                crate::request::json_text(&archived.reason)
            )
        })?;
        let was = entity.archived.take().expect("checked just above");
        self.touched(entity, session);
        let mut served = entity.clone();
        naming_folded_in(&entities, &mut served);
        Ok((served, was))
    }

    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: crate::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        validate_write_subject(from)?;
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
                nearest: self.nearest_for(from, &stored),
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
        // **A same handle only reparents when the parent actually
        // differs.** Compared as badges rather than as the handle a caller
        // spelled it with, so naming a former spelling of the parent it
        // already has is recognised as no change too — never as a
        // reparent that happens to land on the same place. An unresolvable
        // new parent is never "unchanged": that is the near-miss guard's
        // question, asked below, and answered with candidates rather than
        // with this refusal.
        if from == to {
            let target_badge = parent.as_ref().and_then(|p| self.storage_key(p));
            let unchanged = match &parent {
                None => true,
                Some(_) => target_badge.is_some() && target_badge == entity.parent,
            };
            if unchanged {
                return Err(MemoryError::NothingToRename {
                    attempted: from.to_string(),
                });
            }
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
        let earlier_selves = super::super::folded_into(from, &known)?;
        let others: Vec<Entity> = known
            .iter()
            .filter(|e| &e.id != from && !earlier_selves.contains(&e.id))
            .cloned()
            .collect();
        // **A move that keeps its name is never screened for resemblance**: it
        // gives the thing no new name, so there is nothing new to resemble. A
        // call that changes the handle is screened as before.
        if from != to
            && let Decision::Block(candidates) = guard::decide_under(
                to,
                &renamed.labels(),
                renamed.parent.as_ref(),
                &others,
                resemblance.token,
                resemblance.reason,
            )
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
        // **A move changes which collections the thing and everything under it
        // sit in**, so it faces the numbering every other write faces.
        if let Some(new_parent) = &parent {
            let facts = self.facts_view();
            let fields = self.fields_by_handle(&known, &facts);
            let mut moved = vec![entity.id.clone()];
            moved.extend(
                super::super::numbered::descendants(&entity.id, &known)
                    .into_iter()
                    .map(|e| e.id.clone()),
            );
            let under = self
                .resolve(new_parent)
                .map(|(_, handle)| handle)
                .unwrap_or_else(|| new_parent.clone());
            self.guards_numbering(
                &super::super::numbered::Change {
                    moved: moved
                        .into_iter()
                        .map(|handle| {
                            let held = fields.get(&handle).cloned().unwrap_or_default();
                            (handle, held)
                        })
                        .collect(),
                    ..Default::default()
                },
                Some(&under),
                &known,
                &facts,
            )?;
        }
        // **A retype faces the ceiling of the kind it becomes.** A thing that
        // holds more records than a thread allows does not become one.
        if renamed.kind != entity.kind {
            let facts = self.facts_view();
            let home = match &entity.badge {
                Some(badge) => EntityId(badge.clone()),
                None => entity.id.clone(),
            };
            let held =
                super::super::folded_fields(&self.writes_on(&home, &facts), &self.declarations());
            // **Projected, as the real store's count reads them**: a stored claim
            // carries no fields, and the ceiling's count asks for the mark's.
            let filed: Vec<Fact> = facts
                .iter()
                .filter(|f| f.home == home)
                .map(|f| self.projected(f))
                .collect();
            let live: Vec<Fact> = crate::memory::capacity::live_records(&filed)
                .into_iter()
                .map(|f| Fact {
                    home: to.clone(),
                    subject: to.clone(),
                    ..f
                })
                .collect();
            if let crate::memory::capacity::Admission::Refused(refused) =
                crate::memory::capacity::admit(
                    crate::memory::capacity::Container {
                        handle: to,
                        kind: Some(renamed.kind),
                        held: &held,
                    },
                    &live,
                    0,
                    false,
                )
            {
                return Err(refused);
            }
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
            // **A forwarding row names the survivor by the handle it wears
            // now.** The fold stored the handle as it stood, and a rename moves
            // that handle.
            if held.merged_into.as_ref() == Some(from) {
                held.merged_into = Some(to.clone());
            }
        }
        let mut renamed = renamed;
        naming_folded_in(&entities, &mut renamed);
        drop(entities);
        self.touched(&entity, session);
        // **A same-handle reparent is not a former handle of itself.** The
        // handle never moved, so there is nothing here for a later resolve
        // to walk back through.
        if from != to {
            self.former_handles
                .lock()
                .expect("fake mutex poisoned")
                .push(FormerHandle {
                    former: from.clone(),
                    badge: entity.badge.clone().expect("a written row wears a badge"),
                    changed_at: date,
                });
        }
        Ok(Guarded::Written(renamed))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        // Same guards the real adapter applies, so the fake can't drift.
        validate_write_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        if let Some(edge) = &fact.edge {
            validate_edge(edge)?;
        }
        validate_happened_span(fact.happened_at, fact.happened_through)?;
        let receipt = fact
            .receipt
            .clone()
            .map(crate::memory::Receipt::validated)
            .transpose()?;
        validate_fields(&fact.fields)?;
        validate_provenance_source(fact.provenance, &fact.fields)?;
        let standing = standing_of(&fact);
        // **Which bag each key is written under**, decided before anything lands.
        let classes = super::super::classes_for_write(
            &Default::default(),
            &fact.fields,
            &fact.sets,
            &fact.derived,
            &[],
            &self.declarations(),
            !self.own_fields_reach,
        )?;

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

        let mut facts = self.facts_view();
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
                    nearest: self.nearest_for(&source.home, &index),
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
        // **A role's own claim is decided atomically with the write it
        // gates, against the state this same lock already holds**, in both
        // shapes. Only a write whose subject is a role object reads anything:
        // every other write costs nothing here.
        if let Some((role, state)) = self.role_state_for(&subject_entity, &facts)
            && let Some(refused) =
                super::super::refuses_role_write(&role, None, &fact.fields, &state)
        {
            return Err(refused);
        }
        // **A ceiling's cardinality is enforced here, atomically with the
        // write it gates** — the same rule and the same reasons the real
        // store's copy carries. **Structural, never a kind question**: a
        // room's member is an ordinary claim on the bound thing's own
        // handle drawing a `connection` edge, and capacity is an ordinary
        // field on that thing. A thing carrying none is uncapped.
        if edge
            .as_ref()
            .is_some_and(|e| e.shape == crate::memory::EdgeShape::Connection)
        {
            let folded =
                super::super::folded_fields(&self.writes_on(&home, &facts), &self.declarations());
            // **The body cap, checked before the room has anything to say.**
            // A thought over its container's cap is refused whether or not
            // the room has space — see `refuses_thought_over_cap`.
            let cap = folded
                .get(super::super::THOUGHT_BODY_CAP)
                .and_then(|v| v.trim().parse::<usize>().ok());
            let capacity = folded
                .get(super::super::THOUGHT_CAPACITY)
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(err) = super::super::refuses_thought_over_cap(
                &subject_entity.id,
                &normalize_content(&fact.content),
                cap,
                capacity,
            ) {
                return Err(err);
            }
            if let Some(capacity) = capacity {
                let existing_owned: Vec<Fact> =
                    facts.iter().filter(|f| f.home == home).cloned().collect();
                let nominal_room = super::super::thought_room(&existing_owned);
                // **Ageing's own cost is paid only when the room might
                // actually be full** — a bot nowhere near capacity never
                // pays for a touch-moment lookup on every write.
                if nominal_room.len() >= capacity {
                    let touched = self.touched_moments(&home, &nominal_room);
                    let counted = super::super::count_room(
                        nominal_room,
                        &touched,
                        fact.aged_before,
                        capacity,
                    );
                    if counted.full() {
                        let aged_out = counted.aged_out.len();
                        let refuse = |room: Vec<Fact>| MemoryError::RoomFull {
                            // **The handle the caller reached this thing
                            // by, never `home`** — `home` is the badge
                            // every entity wears from creation, and a
                            // refusal naming it hands back a token the
                            // caller cannot address anything by.
                            subject: subject_entity.id.to_string(),
                            live: room.len(),
                            capacity,
                            // **The room's thoughts are served under the
                            // handle too**, as the real store serves them: a
                            // caller names one of these addresses as its drop.
                            room: room
                                .into_iter()
                                .map(|f| Fact {
                                    home: subject_entity.id.clone(),
                                    subject: subject_entity.id.clone(),
                                    ..f
                                })
                                .collect(),
                            aged_out,
                        };
                        match (&fact.drop, &fact.drop_because) {
                            (Some(victim), Some(reason)) => {
                                // **The victim must resolve to a live thought
                                // IN THIS ROOM** — not merely to a fact that
                                // exists, and not to one aged out of the
                                // count. Anything else answers exactly as an
                                // empty drop does: refused, with the room
                                // shown.
                                let victim_home = self.storage_key(&victim.home);
                                let in_room = victim_home.filter(|key| *key == home).is_some()
                                    && counted.live.iter().any(|f| f.id == victim.local);
                                let Some(victim_index) = in_room
                                    .then(|| {
                                        facts
                                            .iter()
                                            .position(|f| f.home == home && f.id == victim.local)
                                    })
                                    .flatten()
                                else {
                                    return Err(refuse(counted.live));
                                };
                                // **A thought that carries a key only its own
                                // relation may change is not droppable.** A
                                // capture carries no caller to ask who may change
                                // that key, and archiving the thought takes the
                                // key off the fold. Retracting it asks.
                                if !super::super::guarded_keys_in(
                                    &self.projected(&facts[victim_index]).fields,
                                )
                                .is_empty()
                                {
                                    return Err(refuse(counted.live));
                                }
                                // **Archived, through the one writer every
                                // archive goes through** — `append_claim_write`
                                // is what leaves a write behind; a status flip
                                // that skipped it would read back changed with
                                // nothing behind it saying when.
                                facts[victim_index].status = FactStatus::Archived;
                                facts[victim_index].details = Some(reason.clone());
                                let archived = facts[victim_index].clone();
                                self.rewrite_stored(
                                    &FactAddress::new(home.clone(), victim.local.clone()),
                                    Fact {
                                        fields: Default::default(),
                                        ..archived.clone()
                                    },
                                );
                                self.append_claim_write(&archived, None);
                            }
                            // **The emergency reserve, spent — never twice
                            // in a row**, the same rule the real store's
                            // copy carries. Only honoured at the exact
                            // threshold: a room already over capacity means
                            // a borrow already landed and was not repaid.
                            _ if fact.borrow && !counted.over() => {}
                            _ => return Err(refuse(counted.live)),
                        }
                    }
                }
            }
        }
        // **The thing's record ceiling is decided here, atomically with the
        // write it gates**, by the one entry point every store calls.
        {
            let held =
                super::super::folded_fields(&self.writes_on(&home, &facts), &self.declarations());
            // **Projected, as the real store's count reads them**: a stored claim
            // carries no fields, and the ceiling's count asks for the mark's.
            let filed: Vec<Fact> = facts
                .iter()
                .filter(|f| f.home == home)
                .map(|f| self.projected(f))
                .collect();
            let live: Vec<Fact> = crate::memory::capacity::live_records(&filed)
                .into_iter()
                .map(|f| Fact {
                    home: subject_entity.id.clone(),
                    subject: subject_entity.id.clone(),
                    ..f
                })
                .collect();
            if let crate::memory::capacity::Admission::Refused(refused) =
                crate::memory::capacity::admit(
                    crate::memory::capacity::Container {
                        handle: &subject_entity.id,
                        kind: Some(subject_entity.kind),
                        held: &held,
                    },
                    &live,
                    crate::memory::capacity::slots_taken(&fact.fields),
                    fact.borrow,
                )
            {
                return Err(refused);
            }
        }
        let wrote: Vec<(String, Option<String>)> = fields
            .iter()
            .map(|(key, value)| (key.clone(), Some(value.clone())))
            .collect();
        let stored = Fact {
            sets: Default::default(),
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
            happened_through: fact.happened_through,
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
            inserted_at: Some(self.clock().now()),
            stale_after: fact.stale_after,
            receipt,
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
        let after = super::super::stood_after_capture(&writes, &stored, &declared);
        super::super::guard_fit(
            subject_entity.kind.as_token(),
            &super::super::folded_fields(&writes, &declared),
            &after,
            &governs,
        )?;
        self.guards_numbering(
            &super::super::numbered::Change {
                holds: vec![(subject_entity.id.clone(), after.clone())],
                ..Default::default()
            },
            None,
            &self.known(),
            &facts,
        )?;
        // **The write spends the id of the call that carries one**, under the
        // lock the write is made under, so a second call with the same id finds
        // it spent and stores nothing.
        self.spend_request(|| {
            format!(
                "{{\"address\":\"{}\"}}",
                FactAddress::new(subject_entity.id.clone(), stored.id.clone())
            )
        })?;
        // **The claim is kept without its fields and the fields are kept as
        // writes.** One body of data, projected on the way out.
        self.push_stored(Fact {
            fields: Default::default(),
            ..stored.clone()
        });
        // **A capture is the claim's first write.** Kept here as the real store
        // keeps it, so a claim nobody has corrected answers with one write.
        self.append_claim_write(&stored, fact.session.clone());
        self.append_writes(&stored.home, &stored.id, wrote, &classes);
        // **The keys this record set on its thing**, as the bags were decided above
        // and as a read of it will answer.
        let stored = Fact {
            sets: classes
                .iter()
                .filter(|(_, class)| **class == super::super::WriteClass::Sets)
                .map(|(key, _)| key.clone())
                .collect(),
            ..stored
        };
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
        self.record_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
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
                nearest: self.nearest_for(subject, &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let facts = self.facts_view();
        let folded = self.is_folded_key(&key);
        let mine: Vec<Fact> = facts
            .iter()
            .filter(|f| !folded && (f.subject == key || f.home == key))
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

    /// **Who points at a thing, from one read of the rows.** The trait's default
    /// asks `recall` of every entity, and this store's `recall` rebuilds the
    /// whole list of rows for each ask, so a recall of a whole kind, which asks
    /// this once per object, cost the cube of the store. The answer is the same
    /// one: every entity's records, served and projected as `recall` serves
    /// them, filtered by what their fields name. The listing is still read
    /// through [`Memory::list_entities`], so a case that counts listings sees
    /// what it saw.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        super::super::validate_subject(target)?;
        let declared = self.declared_types().await?;
        let entities = self.list_entities(None).await?;
        let stored: Vec<Fact> = self.facts_view();
        // **Shared by every claim this call serves**, so what it costs does not
        // move with how many there are.
        let resolution = self.resolution();
        let grouped = self.writes_by_record();
        let mut pointing = Vec::new();
        for entity in &entities {
            let key = match &entity.badge {
                Some(badge) => EntityId(badge.clone()),
                None => entity.id.clone(),
            };
            for fact in stored.iter().filter(|f| f.subject == key || f.home == key) {
                let served = self.served_in(
                    &resolution,
                    Self::projected_from(&grouped, fact),
                    &entity.id,
                );
                if super::super::fields_name_target(&served.fields, &declared, target) {
                    pointing.push(served);
                }
            }
        }
        Ok(pointing)
    }

    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let index = self.known();
        let Some(key) = self.resolve(entity).map(|(key, _)| key) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        };
        let mut held = self.held(&key);
        self.compose_reference_fields(&mut held);
        Ok(held)
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<super::super::FieldAges, MemoryError> {
        let index = self.known();
        let Some(key) = self.resolve(entity).map(|(key, _)| key) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        };
        let facts = self.facts_view();
        let writes = self.writes_on(&key, &facts);
        let keys = super::super::folded_ages(&writes, &self.declarations());
        // The newest write of any kind: field writes of any status, and the
        // writes of the thing's records.
        let claims = self.claim_writes_view();
        let newest = writes
            .iter()
            .filter_map(|w| w.written_at)
            .chain(
                claims
                    .iter()
                    .filter(|(home, _, _)| home == &key)
                    .filter_map(|(_, _, w)| w.written_at),
            )
            .chain(
                self.entity_writes
                    .lock()
                    .expect("fake mutex poisoned")
                    .iter()
                    .filter(|(held, _)| held == &key)
                    .map(|(_, at)| *at),
            )
            .max();
        Ok(super::super::FieldAges { newest, keys })
    }

    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<crate::memory::MergeEvent>, MemoryError> {
        let wanted = match entity {
            Some(entity) => match self.resolve(entity) {
                Some((key, _)) => Some(key),
                None => return Ok(Vec::new()),
            },
            None => None,
        };
        // **The merges are copied out before any handle is looked up**: serving a
        // handle reads the entity rows, and those read the merges.
        let wanted_merges: Vec<(
            EntityId,
            EntityId,
            jiff::Timestamp,
            Option<String>,
            Vec<(FactId, FactId)>,
        )> = self
            .merges
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .filter(|m| {
                wanted
                    .as_ref()
                    .is_none_or(|key| &m.folded == key || &m.survivor == key)
            })
            .map(|m| {
                (
                    m.folded.clone(),
                    m.survivor.clone(),
                    m.at,
                    m.session.clone(),
                    m.claims.clone(),
                )
            })
            .collect();
        Ok(wanted_merges
            .into_iter()
            .map(
                |(folded, survivor, at, session, claims)| crate::memory::MergeEvent {
                    folded: self.current_handle(&folded),
                    survivor: self.current_handle(&survivor),
                    merged_at: at,
                    session,
                    claims,
                },
            )
            .collect())
    }

    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        Ok(self.moved(address))
    }

    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<crate::memory::unmapped::UnmappedClaim>, MemoryError> {
        let Some((key, _)) = self.resolve(entity) else {
            return Ok(Vec::new());
        };
        Ok(self.unmapped_among(&key, &self.facts_view()))
    }

    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<super::super::EntityVersion>, MemoryError> {
        let index = self.index();
        // **The thing is found as every read finds it**, by the handle it wears
        // or one it wore; the real store resolves a former handle the same way.
        let Some(row) = super::resolve_handle(entity, &index, &self.former()) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        };
        let key = match &row.badge {
            Some(badge) => EntityId(badge.clone()),
            None => row.id.clone(),
        };
        // **The versions of every thing folded into this one are part of its
        // history**, in the order they were written, each marked with the
        // handle its writer answers to now.
        let mut things = vec![key.clone()];
        things.extend(
            self.keys_folded_into(&key)
                .expect("the fake merges nothing into itself, so its merges never loop"),
        );
        let held: Vec<(EntityId, super::super::EntityVersion)> = self
            .entity_versions
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .filter(|(home, _)| things.contains(home))
            .cloned()
            .collect();
        Ok(held
            .into_iter()
            .enumerate()
            .map(|(at, (home, mut version))| {
                version.ordinal = at + 1;
                version.thing = self.current_handle(&home);
                // **Pointers are kept as keys and read as the handle each
                // answers to now**, as the real store reads them.
                if let Some(snapshot) = &mut version.snapshot {
                    for pointer in [&mut snapshot.parent, &mut snapshot.merged_into] {
                        if let Some(stored) = pointer.take() {
                            *pointer = Some(self.current_handle(&stored));
                        }
                    }
                }
                version
            })
            .collect())
    }

    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        let moved = self.moved(address);
        let address = moved.as_ref().unwrap_or(address);
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(&address.home, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: self.nearest_for(&address.home, &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let facts = self.facts_view();
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
        let writes = self.claim_writes_view();
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
            // **So are the references and the mark the write kept**, each
            // stored under a storage key and served under today's handle.
            if let Some(refs) = &mut write.refs {
                for object in refs.iter_mut() {
                    *object = self.current_handle(object);
                }
            }
            if let Some(marked) = &mut write.stands_for {
                for named in marked.iter_mut() {
                    *named =
                        FactAddress::new(self.current_handle(&named.home), named.local.clone());
                }
            }
        }
        Ok(mine)
    }

    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        // The same misses `claim_history` answers, then the classes the record holds.
        self.claim_history(address).await?;
        let index = self.known();
        let former = self.former();
        let entity = super::resolve_handle(&address.home, &index, &former)
            .expect("claim_history found the home");
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        Ok(self
            .held_classes(&key, &address.local)
            .into_iter()
            .filter(|(_, class)| !super::super::reaches_the_thing(*class))
            .map(|(key, _)| key)
            .collect())
    }

    /// **Every key any write on the thing named**, read off the writes
    /// themselves, so a key that was set and then taken off is asked about as
    /// well — the case a read of the records' current keys would miss.
    async fn key_trails(&self, entity: &EntityId) -> Result<crate::memory::KeyTrails, MemoryError> {
        let index = self.known();
        let former = self.former();
        let Some(resolved) = super::resolve_handle(entity, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        };
        let storage_key = match &resolved.badge {
            Some(badge) => EntityId(badge.clone()),
            None => resolved.id.clone(),
        };
        // **The keys written on the thing or on any thing folded into it**, as the
        // real store reads them, and none for a key that was folded away: what it
        // still stores is its survivor's now.
        let keys: std::collections::BTreeSet<String> = {
            let folded = self.keys_folded_into(&storage_key)?;
            let is_folded = self
                .merges
                .lock()
                .expect("fake mutex poisoned")
                .iter()
                .any(|merge| merge.folded == storage_key);
            self.writes
                .lock()
                .expect("fake mutex poisoned")
                .iter()
                .filter(|w| !is_folded && (w.entity == storage_key || folded.contains(&w.entity)))
                .map(|w| w.key.clone())
                .collect()
        };
        let mut trails = crate::memory::KeyTrails::default();
        for key in keys {
            let writes = self.history(entity, &key).await?;
            if !writes.is_empty() {
                trails.history.insert(key, writes);
            }
        }
        for fact in self.recall(entity).await? {
            let kept = self.keys_kept_off_the_thing(&fact.address()).await?;
            if !kept.is_empty() {
                trails.kept_off.insert(fact.id.clone(), kept);
            }
        }
        // **The writes that stay on their record**, as the real store keeps them:
        // each with the moment the store stamped, oldest first within a key.
        {
            let is_folded = self
                .merges
                .lock()
                .expect("fake mutex poisoned")
                .iter()
                .any(|merge| merge.folded == storage_key);
            // **Read as served**: a folded record's writes wear the id its claim has
            // on the survivor, as the real store's, so they never meet another
            // record's writes under an id both documents once held.
            let mut kept: Vec<crate::memory::RecordFieldWrite> = self
                .writes_view()
                .iter()
                .filter(|w| !is_folded && w.entity == storage_key)
                .filter(|w| !super::super::reaches_the_thing(w.class))
                .map(|w| crate::memory::RecordFieldWrite {
                    fact: w.fact.clone(),
                    key: w.key.clone(),
                    ordinal: w.ordinal,
                    value: w.value.clone(),
                    written_at: w.written_at,
                })
                .collect();
            kept.sort_by(|a, b| (&a.key, a.ordinal).cmp(&(&b.key, b.ordinal)));
            trails.record_writes = kept;
        }
        Ok(trails)
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        let index = self.known();
        let former = self.former();
        let Some(resolved) = super::resolve_handle(entity, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        };
        let storage_key = match &resolved.badge {
            Some(badge) => EntityId(badge.clone()),
            None => resolved.id.clone(),
        };
        // The record each write arrived in says when it happened and what
        // became of it, so the two are read together.
        let facts = self.facts_view();
        let writes = self.writes_view();
        let mut mine: Vec<&StoredWrite> = writes
            .iter()
            .filter(|w| w.entity == storage_key && w.key == key)
            .filter(|w| super::super::reaches_the_thing(w.class))
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
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let moved = self.moved(address);
        let address = moved.as_ref().unwrap_or(address);
        if let Some(refused) = super::super::refuses_an_unapplied_text_edit(&patch) {
            return Err(refused);
        }
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
                nearest: self.nearest_for(&address.home, &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let handle = entity.id.clone();
        let kind = entity.kind;

        let facts = self.facts_view();
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
        // **An edit made against a revision the claim has moved past is refused
        // before anything is written**, counted under the same lock the write
        // appends under.
        if patch.expected_revision.is_some() {
            let current = self
                .claim_writes_view()
                .iter()
                .filter(|(home, id, _)| home == &key && id == &address.local)
                .count();
            if let Some(refused) =
                super::super::refuses_a_stale_revision(address, patch.expected_revision, current)
            {
                return Err(refused);
            }
        }
        // **A role's own claim, renewed or re-claimed by patch, is decided
        // atomically with the write it gates** — the same check `capture`
        // runs, against the same state under the same lock, so a renewal and a
        // fresh claim are one mechanism rather than two.
        let role_state = self.role_state_for(entity, &facts);
        if let Some((role, state)) = &role_state
            && let Some(refused) = super::super::refuses_role_write(
                role,
                patch.role_move.as_ref(),
                &patch.fields,
                state,
            )
        {
            return Err(refused);
        }
        // **A renewal overwrites the lease moment and appends nothing.** See
        // [`Self::overwrite_lease_moment`] for the one exception this is, and
        // the real store's identical step.
        if role_state.is_some()
            && super::super::is_a_plain_renewal(&patch)
            && let Some(moment) = patch.fields.get(crate::session::ROLE_CLAIMED_AT)
            && self.overwrite_lease_moment(&fact.home, moment)
        {
            // The signal the real store files so that the thing reads as written
            // although no claim write was added.
            self.touched(entity, patch.session.as_deref());
            return Ok(Guarded::Written(self.served(self.projected(fact), &handle)));
        }
        // **The patch is applied to the record as it reads now**, so a set that
        // replaces a value is validated against the value it replaces — and the
        // claim goes back without fields, because the fields are the writes.
        let mut edited = self.projected(fact);
        // What the ADDRESSED RECORD carries right now — kept before the patch
        // rewrites it, because that is what decides which of the patch's clears
        // is a write and which names a key this record never had.
        let carried = edited.fields.clone();
        // **Whether this record was already a thought, read before the patch
        // rewrites it** — see the real store's identical comment on its own
        // copy of this line.
        let was_thought = edited
            .edge
            .as_ref()
            .is_some_and(|e| e.shape == crate::memory::EdgeShape::Connection);
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
                    nearest: self.nearest_for(&source.home, &self.known()),
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
                        nearest: self.nearest_for(&named.home, &self.known()),
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
        let first_session = self.first_write_session(&key, &address.local);
        // **The claim as a caller reads it**, so a value that points at a thing is
        // compared to a resend as the handle it answers to and not as the id the
        // store keeps, as the real store reads it.
        let mut as_read = edited.clone();
        self.compose_reference_fields(&mut as_read.fields);
        let patch = match super::super::settle_rewrite(
            address,
            &as_read,
            patch.clone(),
            first_session.as_deref(),
            &self.declarations(),
        ) {
            Ok(settled) => settled,
            // **A ceiling key answers first.** The refusal of a rewrite of
            // testimony names the route of archiving and capturing a correction,
            // and for a key this caller may not write that route meets the key's
            // own refusal at its end. So the key is asked now, and its refusal
            // is the one the caller gets.
            Err(refusal @ MemoryError::TestimonyRewritten { .. }) => {
                let writes = self.writes_on(&fact.home, &facts);
                let before = self.with_manager_served(&super::super::folded_fields(
                    &writes,
                    &self.declarations(),
                ));
                let after = super::super::fold_after_field_edits(&before, &patch);
                let lineage = super::super::chart_wanted_by_change(&before, &after)
                    .map(|named| self.lineage_among(&facts, &handle, named));
                return Err(super::super::refuses_unlicensed_change(
                    &handle,
                    caller,
                    &before,
                    &after,
                    lineage.as_ref(),
                )
                .unwrap_or(refusal));
            }
            Err(other) => return Err(other),
        };
        // **Which bag each key is written under**, read off what this record holds
        // now: a key stays in the bag it was first written under.
        let held_classes = self.held_classes(&key, &address.local);
        let classes = super::super::classes_for_write(
            &held_classes,
            &patch.fields,
            &patch.sets,
            &patch.derived,
            &patch.clear_fields,
            &self.declarations(),
            !self.own_fields_reach,
        )?;
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
        self.guards_numbering(
            &super::super::numbered::Change {
                holds: vec![(handle.clone(), after.clone())],
                ..Default::default()
            },
            None,
            &self.known(),
            &facts,
        )?;
        // **The ceiling and the room, both on the state this edit leaves
        // behind** — the same check the real store runs, atomically with the
        // write it gates.
        // **Permission is judged on the same served folds the chain is asked
        // on.** A restated manager is the same bot whatever form the store keeps
        // it in, and only a change in who it names is a change.
        let (before, after) = (
            self.with_manager_served(&before),
            self.with_manager_served(&after),
        );
        let lineage = super::super::chart_wanted_by_change(&before, &after)
            .map(|named| self.lineage_among(&facts, &handle, named));
        if let Some(err) = super::super::refuses_unlicensed_change(
            &handle,
            caller,
            &before,
            &after,
            lineage.as_ref(),
        ) {
            return Err(err);
        }
        let becomes_thought = edited.status == FactStatus::Active
            && edited
                .edge
                .as_ref()
                .is_some_and(|e| e.shape == crate::memory::EdgeShape::Connection);
        // **The cap runs on a content change, or on newly becoming a
        // thought — never on an edit that leaves an existing thought's
        // content exactly as it was** — see the real store's identical
        // comment on its own copy of this check.
        let capacity = after
            .get(super::super::THOUGHT_CAPACITY)
            .and_then(|v| v.trim().parse::<usize>().ok());
        if becomes_thought && (patch.content.is_some() || !was_thought) {
            let cap = after
                .get(super::super::THOUGHT_BODY_CAP)
                .and_then(|v| v.trim().parse::<usize>().ok());
            if let Some(err) =
                super::super::refuses_thought_over_cap(&handle, &edited.content, cap, capacity)
            {
                return Err(err);
            }
        }
        // **The room only grows when a record newly joins it, or comes back into
        // it** — see the real store's identical comment on its own copy of this
        // check. A thought gone quiet is outside the count, and any edit to it
        // touches it, so it rejoins the room and meets the same check.
        let was_aged = was_thought
            && super::super::is_quiet(
                self.touched_moments(&home, std::slice::from_ref(fact))
                    .get(&fact.id)
                    .copied(),
                patch.aged_before,
            );
        if becomes_thought
            && (!was_thought || was_aged)
            && let Some(capacity) = capacity
        {
            let others: Vec<Fact> = facts
                .iter()
                .filter(|f| f.home == home && f.id != edited.id)
                .cloned()
                .collect();
            let nominal = super::super::thought_room(&others);
            // **Ageing's cost is paid only when the room might be full**, as
            // `capture` pays it. With no touch recorded nothing is quiet.
            let touched = if nominal.len() >= capacity {
                self.touched_moments(&home, &nominal)
            } else {
                Default::default()
            };
            let mut counted =
                super::super::count_room(nominal, &touched, patch.aged_before, capacity);
            // **Served under the handle**, as the real store serves them: the
            // refusal's way forward is to archive one of these by address.
            counted.live = counted
                .live
                .into_iter()
                .map(|f| Fact {
                    home: handle.clone(),
                    subject: handle.clone(),
                    ..f
                })
                .collect();
            if let Some(err) = super::super::refuses_room_overflow(&handle, counted) {
                return Err(err);
            }
        }
        let id = fact.id.clone();
        self.spend_request(|| format!("{{\"address\":\"{}\"}}", address))?;
        self.rewrite_stored(
            &FactAddress::new(home.clone(), id.clone()),
            Fact {
                fields: Default::default(),
                ..edited.clone()
            },
        );
        // **An edit rewrites the row and appends a write**, which is what makes
        // what the claim used to say readable after it stops being true.
        self.append_claim_write(&edited, patch.session.clone());
        drop(facts);
        // **Guarded and stored are different questions.** The guard just
        // above validated the patch's own handle-form values, exactly as a
        // caller wrote them (rule 268 needs the shape it named to check the
        // shape); what actually lands is lowered to a permanent id per
        // reference-typed item, same as a capture's does.
        let lowered_writes = self.lower_writes(super::super::writes_of(&patch, &carried));
        self.append_writes(&home, &id, lowered_writes, &classes);
        let facts = self.facts_view();
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
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        // **Wrong by kind alone, before either side's existence is even
        // asked.** A fold naming a session steps past the same three things
        // any other memory write onto its handle would.
        validate_write_subject(folded)?;
        validate_write_subject(survivor)?;
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
        // **The sides are judged by the one function every store asks**, in its
        // order, before anything moves, so a refusal leaves the store exactly as
        // it was.
        match super::super::merge::refuses_merge_sides(folded, survivor, &index) {
            Some(super::super::merge::SideRefusal::Missing(side)) => {
                let supplied = self
                    .supplied
                    .lock()
                    .expect("fake mutex poisoned")
                    .record_for(&side)
                    .is_some();
                return Err(super::super::merge::missing_side(
                    &side,
                    supplied,
                    self.nearest_for(&side, &self.known()),
                ));
            }
            Some(super::super::merge::SideRefusal::Refused(refused)) => return Err(refused),
            None => {}
        }

        // **A document merges only into another document**, decided before
        // anything moves.
        if let Some(refused) =
            super::super::document::refuses_a_merge_of_a_document_and_a_thing(folded, survivor)
        {
            return Err(refused);
        }

        let account = merge_account(folded, survivor, reason, date)?;
        let standing = standing_of(&account);

        // **Both sides resolved to their storage keys once, up front.** Every
        // comparison below is against these — never against the raw handles a
        // caller sent — because a claim's home is the badge now, not the
        // handle, and comparing against the handle would silently move
        // nothing.
        let (folded_key, folded_handle) = self.resolve(folded).expect("checked present above");
        let (survivor_key, survivor_handle) =
            self.resolve(survivor).expect("checked present above");

        // **The guards of a fold run in the one order the domain keeps**, over
        // what this store reads. The duplicate's thoughts meet the survivor's
        // room and body cap before anything moves, served under the handles as
        // the real store serves them; asked of a copy, before the locks below
        // are taken, because serving a record reads the entity rows and those
        // locks do not nest.
        let held_facts = self.world().facts.clone();
        let held = super::super::folded_fields(
            &self.writes_on(&survivor_key, &held_facts),
            &self.declarations(),
        );
        super::super::merge::refuses_merge_guards(
            &super::super::merge::Fold {
                caller,
                folded: &folded_handle,
                survivor: &survivor_handle,
                survivor_named: survivor,
            },
            &mut FakeFoldReads {
                store: self,
                facts: &held_facts,
                folded,
                survivor,
                folded_key: &folded_key,
                survivor_key: &survivor_key,
                survivor_handle: &survivor_handle,
                held: &held,
            },
        )
        .await?;

        // **Read before the locks below are taken**, as the held facts are.
        let known_rows = self.known();

        // **A merge appends its event and moves no row.** A fact id is local to
        // the doc that holds it, so both sides own an `f1`: the event gives each
        // claim the folded thing serves an id that is free on the survivor, and
        // [`World::relocate`] serves the claim under it, exactly as the real store
        // reads its events.
        let mut next = held_facts.iter().filter(|f| f.home == survivor_key).count();
        let mut folded_claims: Vec<&Fact> =
            held_facts.iter().filter(|f| f.home == folded_key).collect();
        folded_claims.sort_by_key(|f| f.id.ordinal());
        let moved: Vec<(FactId, FactId)> = folded_claims
            .iter()
            .map(|fact| {
                next += 1;
                (fact.id.clone(), FactId(format!("f{next}")))
            })
            .collect();
        let rehomed = moved.len();
        // **The survivor serves what the duplicate held, so the numbering is
        // asked of the state the merge leaves**: the survivor's fields once the
        // duplicate's are folded in, and the duplicate no longer a member. A
        // number held by the duplicate is held on only if the survivor serves it.
        {
            let after = self.world_if_merged(StoredMerge {
                folded: folded_key.clone(),
                survivor: survivor_key.clone(),
                at: self.clock().now(),
                session: None,
                claims: moved.clone(),
            });
            let served = super::super::folded_fields(
                &Self::key_writes_on(&after.writes, &survivor_key, &after.facts),
                &self.declarations(),
            );
            // **The floor of the survivor's kind, on the same state**: a fold whose
            // newest write of a required key took it off leaves the survivor below
            // what it held.
            // **Judged as handles, as an edit is**: a reference key holds the id
            // of what it names, which is no handle and breaks the key's type.
            let (mut held_served, mut after_served) = (held.clone(), served.clone());
            self.compose_reference_fields(&mut held_served);
            self.compose_reference_fields(&mut after_served);
            super::super::guard_fit(
                survivor.kind_token(),
                &held_served,
                &after_served,
                &self.kind_keys_of(survivor.kind_token()),
            )?;
            self.guards_numbering(
                &super::super::numbered::Change {
                    holds: vec![(survivor_handle.clone(), served)],
                    leaves: vec![folded_handle.clone()],
                    // What was under the duplicate is served under the survivor.
                    moved: {
                        let fields = self.fields_by_handle(&known_rows, &held_facts);
                        super::super::numbered::descendants(&folded_handle, &known_rows)
                            .into_iter()
                            .map(|e| (e.id.clone(), fields.get(&e.id).cloned().unwrap_or_default()))
                            .collect()
                    },
                },
                Some(&survivor_handle),
                &known_rows,
                &held_facts,
            )?;
        }
        // **When both sides are documents, the folded sections follow the
        // survivor's last one**, and each chunk names its heading's new id. Planned
        // here, while the folded records are still served at the addresses they
        // were made under; the same planning the real store runs, from the domain.
        let planned = if super::super::entries::places_its_records(folded.kind()) {
            let survivor_records: Vec<Fact> = held_facts
                .iter()
                .filter(|f| f.home == survivor_key)
                .map(|f| self.projected(f))
                .collect();
            let folded_records: Vec<Fact> = held_facts
                .iter()
                .filter(|f| f.home == folded_key)
                .map(|f| self.projected(f))
                .collect();
            let placed = if folded.kind() == Some(EntityKind::LIST) {
                super::super::entries::append_after(&survivor_records, &folded_records)
            } else {
                super::super::document::append_after(&survivor_records, &folded_records, &moved)
            };
            let relocated = placed.map_err(|e| {
                MemoryError::InvalidFact(format!("the folded sections cannot be placed: {e}"))
            })?;
            // **A folded list's ticks name their entries by the new ids**, the
            // ids the merge gave the entries on the survivor.
            let ticks = if folded.kind() == Some(EntityKind::LIST) {
                super::super::entries::ticks_following(&folded_records, &moved)
            } else {
                Vec::new()
            };
            Some((folded_records, relocated, ticks))
        } else {
            None
        };
        // **The id is spent before anything is written**, the sections' new order
        // included, so a merge that finds it spent stores nothing, as the real
        // store spends it in the transaction that writes the sections and the
        // event.
        self.spend_request(|| format!("{{\"folded\":\"{folded}\",\"survivor\":\"{survivor}\"}}"))?;
        if let Some((folded_records, relocated, ticks)) = planned {
            for tick in ticks {
                let Some(record) = folded_records.iter().find(|f| f.id == tick.fact) else {
                    continue;
                };
                let classes = self.held_classes(&folded_key, &tick.fact);
                self.append_claim_write(record, session.map(str::to_string));
                self.append_writes(
                    &folded_key,
                    &tick.fact,
                    vec![(
                        super::super::entries::TICK_ENTRY.to_string(),
                        Some(tick.entry),
                    )],
                    &classes,
                );
            }
            for place in relocated {
                let Some(record) = folded_records.iter().find(|f| f.id == place.fact) else {
                    continue;
                };
                let mut wrote =
                    vec![(super::super::document::ORDER.to_string(), Some(place.order))];
                if let Some(head) = place.continues {
                    wrote.push((super::super::document::CONTINUES.to_string(), Some(head)));
                }
                // Each key keeps the class it was first written under, so the new
                // order stays on the record as the old one did.
                let classes = self.held_classes(&folded_key, &place.fact);
                self.append_claim_write(record, session.map(str::to_string));
                self.append_writes(&folded_key, &place.fact, wrote, &classes);
            }
        }
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        self.merges
            .lock()
            .expect("fake mutex poisoned")
            .push(StoredMerge {
                folded: folded_key.clone(),
                survivor: survivor_key.clone(),
                at: self.clock().now(),
                session: session.map(str::to_string),
                claims: moved.clone(),
            });

        let record = Fact {
            sets: Default::default(),
            id: FactId(format!("f{}", next + 1)),
            home: survivor_key.clone(),
            subject: survivor_key.clone(),
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            standing,
            status: account.status,
            recorded_at: account.recorded_at,
            happened_at: account.happened_at,
            happened_through: account.happened_through,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock().now()),
            stale_after: None,
            receipt: None,
        };
        self.push_stored(Fact {
            fields: Default::default(),
            ..record.clone()
        });
        self.append_claim_write(&record, session.map(str::to_string));

        // **The folded row stays and starts forwarding.** It is not deleted and
        // it is not left looking like a thing.
        for entity in entities.iter_mut() {
            if entity.id == *folded {
                entity.merged_into = Some(survivor.clone());
            }
        }
        // **A version for the folded row, and one for each child the fold serves
        // under the survivor**: the children of the folded thing and of whatever
        // was folded into it before. The merge just pushed is the last one, so
        // the earlier merges say where a parent was served until now.
        {
            let merges = self.merges.lock().expect("fake mutex poisoned");
            let before = &merges[..merges.len() - 1];
            for entity in entities.iter() {
                if entity.id == *folded {
                    self.touched(entity, session);
                }
            }
            for entity in entities.iter() {
                let Some(parent) = &entity.parent else {
                    continue;
                };
                if entity.merged_into.is_none() && ended_at(before, parent.clone()) == folded_key {
                    self.touched_naming(entity, Some(&survivor_key), session);
                }
            }
        }
        let mut survived = entities
            .iter()
            .find(|e| &e.id == survivor)
            .cloned()
            .expect("the survivor was checked present");
        naming_folded_in(&entities, &mut survived);
        drop(entities);
        self.append_writes(
            &record.home,
            &record.id,
            record
                .fields
                .iter()
                .map(|(key, value)| (key.clone(), Some(value.clone()))),
            &Default::default(),
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
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        let moved = self.moved(address);
        let address = moved.as_ref().unwrap_or(address);
        let index = self.known();
        let former = self.former();
        let Some(entity) = super::resolve_handle(&address.home, &index, &former) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: self.nearest_for(&address.home, &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        let handle = entity.id.clone();

        // Everything is decided before anything moves, so a refusal leaves the
        // row exactly as it was — the same shape `apply_fact_patch` has.
        let facts = self.facts_view();
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
            sets: Default::default(),
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
            happened_through: account.happened_through,
            edge: account.edge,
            fields: account.fields,
            refs: account.refs,
            derived_from: account.derived_from,
            stands_for: Vec::new(),
            inserted_at: Some(self.clock().now()),
            stale_after: None,
            receipt: None,
        };
        let retracted = Fact {
            status: FactStatus::Archived,
            ..target
        };
        // **The ceiling, on the state this retraction leaves behind** — the
        // same check the real store runs, atomically with the write.
        let declared = self.declarations();
        let held = self.writes_on(&key, &facts);
        let before_fold = super::super::folded_fields(&held, &declared);
        let after_fold = super::super::stood_after(
            &held,
            &retracted,
            &FactPatch::default(),
            &Default::default(),
            &declared,
        );
        let (before_fold, after_fold) = (
            self.with_manager_served(&before_fold),
            self.with_manager_served(&after_fold),
        );
        let lineage = super::super::chart_wanted_by_change(&before_fold, &after_fold)
            .map(|named| self.lineage_among(&facts, &handle, named));
        if let Some(err) = super::super::refuses_unlicensed_change(
            &handle,
            caller,
            &before_fold,
            &after_fold,
            lineage.as_ref(),
        ) {
            return Err(err);
        }
        self.spend_request(|| format!("{{\"address\":\"{address}\"}}"))?;
        self.rewrite_stored(
            &FactAddress::new(key.clone(), address.local.clone()),
            Fact {
                fields: Default::default(),
                ..retracted.clone()
            },
        );
        // The account is a new record like any other: its own keys — the one
        // naming what it takes back — are writes of its own.
        self.push_stored(Fact {
            fields: Default::default(),
            ..record.clone()
        });
        // **A retraction is two writes**: the one that marked the claim, and
        // the account's first. Taking a claim back is a write of it, so the
        // chain says when it stopped standing rather than only that it did.
        self.append_claim_write(&retracted, session.map(str::to_string));
        self.append_claim_write(&record, session.map(str::to_string));
        drop(facts);
        self.append_writes(
            &record.home,
            &record.id,
            record
                .fields
                .iter()
                .map(|(field, value)| (field.clone(), Some(value.clone()))),
            &Default::default(),
        );
        // **Served under the handle, stored under the key** — the same rule
        // capture and update_fact answer to.
        Ok(Retraction {
            retracted: self.served(self.projected(&retracted), &handle),
            record: self.served(record, &handle),
        })
    }

    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        validate_write_subject(entity)?;
        validate_prose(prose)?;
        // Never creates: a handle that names nothing is a miss with its near
        // candidates, exactly as it is for every other verb here.
        let index = self.index();
        if !index.iter().any(|e| &e.id == entity) {
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &index),
            });
        }
        // Normalized here as the real store normalizes it, so the fake cannot
        // preserve whitespace a markdown round-trip would drop.
        let stored = normalize_prose(prose);
        self.spend_request(|| format!("{{\"handle\":\"{entity}\"}}"))?;
        self.prose
            .lock()
            .expect("fake mutex poisoned")
            .insert(entity.clone(), stored.clone());
        if let Some(row) = index.iter().find(|e| &e.id == entity) {
            self.touched(row, session);
        }
        Ok(stored)
    }

    /// A doc here is its frontmatter, whatever prose was written onto it, and
    /// its facts. The **handle doubles as the doc id**: in the fake an entity's
    /// handle IS the key its facts are filed under, so it is the honest answer
    /// to "which document is this".
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        self.full_scans
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        // No Journal document: a wrap publishes nowhere, so the journal stays
        // dark until events land — there is no shared page for `search` to
        // scan here.
        Ok(self.docs_of(self.index()))
    }

    /// **One document, read without scanning the rest**, as the real store
    /// answers it. The port's default scans everything and picks one, which would
    /// make the fake pay for the whole corpus on a read the real store answers
    /// from one page.
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<search::DocScan>, MemoryError> {
        let wanted: Vec<Entity> = self
            .index()
            .into_iter()
            .filter(|row| &row.id == entity)
            .collect();
        Ok(self.docs_of(wanted).pop())
    }

    /// **The signal the real store answers with, built from the same things.**
    /// Counts and latest moments of the entity and claim writes, and a hash of
    /// each table's own content, so a row staged past the writes — the fake's
    /// equivalent of an edit made directly against the real tables — moves it
    /// too. A fake that offered no signal would be scanned on every read and
    /// could never show a case whether a read skipped the scan.
    async fn write_summary(&self) -> Result<Option<crate::memory::WriteSummary>, MemoryError> {
        use std::hash::{Hash, Hasher};
        let hashed = |text: String| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            text.hash(&mut hasher);
            format!("{:016x}", hasher.finish())
        };
        let entity_writes = self.entity_writes.lock().expect("fake mutex poisoned");
        let claim_writes = self.claim_writes.lock().expect("fake mutex poisoned");
        let entity_rows = self.entities.lock().expect("fake mutex poisoned");
        let fact_rows = self.facts.lock().expect("fake mutex poisoned");
        Ok(Some(crate::memory::WriteSummary {
            entities: (
                entity_writes.len() as i64,
                entity_writes.iter().map(|(_, at)| *at).max(),
            ),
            facts: (
                claim_writes.len() as i64,
                claim_writes
                    .iter()
                    .filter_map(|(_, _, w)| w.written_at)
                    .max(),
            ),
            entity_hash: Some(hashed(format!("{entity_rows:?}"))),
            fact_hash: Some(hashed(format!("{fact_rows:?}"))),
        }))
    }

    async fn declare_type(
        &self,
        declared: crate::memory::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<crate::memory::types::DeclaredType, MemoryError> {
        crate::memory::types::validate_type(&declared)?;
        let declared = declared.normalized();
        let mut held = self.types.lock().unwrap();
        let prior = held.iter().find(|t| t.name == declared.name).cloned();
        if let Err(MemoryError::ShippedType { name, .. }) =
            crate::memory::types::guard_replacement(&declared, prior.as_ref().map(|t| t.origin))
        {
            let displaced = self
                .displaced_types
                .lock()
                .unwrap()
                .iter()
                .find(|d| d.name == name)
                .cloned();
            return Err(MemoryError::ShippedType { name, displaced });
        }
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
        // **A shipped write landing on a caller's own declaration is not a
        // refusal** — see `guard_replacement` — so this is the one place
        // that remembers what it took. Reached at most once per name: once
        // this write lands, the name is shipped, and a caller's write over
        // it is refused before it ever reaches here again.
        if declared.origin == crate::memory::types::Origin::Shipped
            && let Some(prior) = &prior
            && prior.origin == crate::memory::types::Origin::Declared
        {
            self.displaced_types
                .lock()
                .unwrap()
                .push(crate::memory::types::Displaced {
                    name: declared.name.clone(),
                    fields: prior.fields.clone(),
                    replaced_on: self.clock().today_in(&jiff::tz::TimeZone::UTC),
                });
        }
        // Replaced whole, the way the real store replaces the rows sharing the
        // name: a type is the keys it names now.
        self.spend_request(|| format!("{{\"name\":\"{}\"}}", declared.name))?;
        held.retain(|t| t.name != declared.name);
        held.push(declared.clone());
        if declared.origin == crate::memory::types::Origin::Declared {
            self.type_writes
                .lock()
                .unwrap()
                .push((declared.name.clone(), by_session.map(str::to_string)));
        }
        Ok(declared)
    }

    async fn declared_types(&self) -> Result<Vec<crate::memory::types::DeclaredType>, MemoryError> {
        // **By name, not by when each was declared** — the real store's own
        // order, and the one a reader can build on.
        let mut types = self.types.lock().unwrap().clone();
        types.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(types)
    }

    async fn displaced_type(
        &self,
        name: &str,
    ) -> Result<Option<crate::memory::types::Displaced>, MemoryError> {
        Ok(self
            .displaced_types
            .lock()
            .unwrap()
            .iter()
            .find(|d| d.name == name)
            .cloned())
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

/// **What the fake reads to judge a fold**, from the copy of its claims taken
/// before the locks.
struct FakeFoldReads<'a> {
    store: &'a InMemoryMemory,
    facts: &'a [Fact],
    folded: &'a EntityId,
    survivor: &'a EntityId,
    folded_key: &'a EntityId,
    survivor_key: &'a EntityId,
    survivor_handle: &'a EntityId,
    held: &'a std::collections::BTreeMap<String, String>,
}

#[async_trait::async_trait]
impl super::super::merge::MergeReads for FakeFoldReads<'_> {
    async fn carried(&mut self) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let carried = super::super::folded_fields(
            &self.store.writes_on(self.folded_key, self.facts),
            &self.store.declarations(),
        );
        Ok(self.store.with_manager_served(&carried))
    }

    async fn lineage(
        &mut self,
        named: Option<EntityId>,
    ) -> Result<super::super::Lineage, MemoryError> {
        Ok(self
            .store
            .lineage_among(self.facts, self.survivor_handle, named))
    }

    async fn chain_above_survivor(&mut self) -> Result<Vec<EntityId>, MemoryError> {
        Ok(self
            .store
            .chain_above_among(self.facts, self.survivor_handle))
    }

    async fn incoming(&mut self) -> Result<Vec<Fact>, MemoryError> {
        Ok(self
            .facts
            .iter()
            .filter(|f| &f.home == self.folded_key)
            .map(|f| self.store.served(f.clone(), self.folded))
            .collect())
    }

    async fn survivors_room(&mut self) -> Result<super::super::merge::SurvivorRoom, MemoryError> {
        Ok(super::super::merge::SurvivorRoom {
            claims: self
                .facts
                .iter()
                .filter(|f| &f.home == self.survivor_key)
                .map(|f| self.store.served(f.clone(), self.survivor))
                .collect(),
            held: self.held.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;

    async fn thing(store: &InMemoryMemory, handle: &str, name: &str) -> EntityId {
        let id = EntityId(handle.to_string());
        store
            .add_entity(NewEntity::new(id.clone(), name, "fixture"))
            .await
            .expect("add")
            .written()
            .expect("not blocked");
        id
    }

    async fn say(store: &InMemoryMemory, about: &EntityId, what: &str) -> FactAddress {
        store
            .capture(NewFact::about(about.clone(), what, date(2026, 6, 2)))
            .await
            .expect("capture")
            .written()
            .expect("not blocked")
            .address()
    }

    /// **Two merges under one request id leave one event.** The first spends the
    /// id and lands; the second finds it spent and is refused, and the store holds
    /// nothing of it: the event is recorded after the id is spent, as the real
    /// store records it in the transaction that spends it. **Paired**: the first
    /// merge landed, and without an id a second merge into the same survivor
    /// lands too, so the refusal is about the id.
    #[tokio::test]
    async fn two_merges_under_one_request_id_leave_one_event() {
        use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
        let ledger = crate::request::testing::InMemoryRequests::new();
        let store = InMemoryMemory::booted().with_requests(ledger.clone());
        let survivor = thing(&store, "person:lisa", "Lisa").await;
        let first = thing(&store, "person:nelson", "Nelson").await;
        let second = thing(&store, "person:milhouse", "Milhouse").await;
        let third = thing(&store, "person:ralph", "Ralph").await;
        let caller = EntityId("bot:sigma".into());
        let under_the_id = |folded: EntityId| {
            let store = &store;
            let survivor = survivor.clone();
            let caller = caller.clone();
            async move {
                let scope = Scope::new(Request {
                    bot: EntityId("bot:sigma".into()),
                    id: RequestId::parse("merge-1").expect("a good id"),
                    verb: "merge_entities".into(),
                    digest: digest_of("{}"),
                });
                within(
                    scope,
                    armed(store.merge(&folded, &survivor, None, date(2026, 6, 3), &caller, None)),
                )
                .await
            }
        };

        under_the_id(first.clone())
            .await
            .expect("the first merge lands");
        let refused = under_the_id(second.clone())
            .await
            .expect_err("the id is spent, so the second merge is refused");
        assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");

        let events = store
            .merge_events(Some(&survivor))
            .await
            .expect("the events read");
        assert_eq!(events.len(), 1, "one event: {events:?}");
        assert_eq!(events[0].folded, first);
        let rows = store.list_entities(None).await.expect("lists");
        assert!(
            rows.iter()
                .any(|e| e.id == second && e.merged_into.is_none()),
            "the refused merge folded nothing"
        );
        // The positive: with no id, another merge into the survivor lands.
        store
            .merge(&third, &survivor, None, date(2026, 6, 3), &caller, None)
            .await
            .expect("a merge with no id lands");
        assert_eq!(
            store
                .merge_events(Some(&survivor))
                .await
                .expect("the events read")
                .len(),
            2
        );
    }

    /// A document of one section: the entity and a record that holds the section
    /// under an order, as a put writes it.
    async fn document_of_one_section(
        store: &InMemoryMemory,
        handle: &str,
        name: &str,
        text: &str,
    ) -> EntityId {
        let id = thing(store, handle, name).await;
        let sections = crate::memory::document::split(text).expect("the text splits");
        let orders = crate::memory::order::spread(None, None, sections.len()).expect("room");
        for (section, order) in sections.iter().zip(orders) {
            store
                .capture(NewFact {
                    details: (!section.body.is_empty()).then(|| section.body.clone()),
                    fields: crate::memory::document::record_fields(section, &order),
                    ..NewFact::about(id.clone(), section.first_line.clone(), date(2026, 6, 2))
                })
                .await
                .expect("capture")
                .written()
                .expect("not blocked");
        }
        id
    }

    /// The order the section with this first line holds, as the thing serves it.
    async fn order_of(store: &InMemoryMemory, doc: &EntityId, first_line: &str) -> String {
        store
            .recall(doc)
            .await
            .expect("recall")
            .into_iter()
            .find(|fact| fact.content == first_line)
            .and_then(|fact| fact.fields.get(crate::memory::document::ORDER).cloned())
            .unwrap_or_else(|| panic!("{doc} serves no section {first_line}"))
    }

    /// **A document merge that finds its request id spent writes nothing**, not
    /// even the new order of the sections it was to move: the id is spent before
    /// the first section is written, as the real store spends it in the
    /// transaction that writes them. **Paired**: the same merge with no id lands
    /// and does move the section behind the survivor's, so the order that stayed
    /// put under the spent id is the id's doing.
    #[tokio::test]
    async fn a_document_merge_under_a_spent_request_id_moves_no_section() {
        use crate::request::{Request, RequestId, Scope, armed, digest_of, within};
        let ledger = crate::request::testing::InMemoryRequests::new();
        let store = InMemoryMemory::booted().with_requests(ledger.clone());
        let survivor = document_of_one_section(
            &store,
            "document:alpha",
            "Fryer Manual",
            "# Alpha head\n\nbody\n",
        )
        .await;
        let folded = document_of_one_section(
            &store,
            "document:beta",
            "Oven Guide",
            "# Beta head\n\nbody\n",
        )
        .await;
        let other_survivor = document_of_one_section(
            &store,
            "document:homer",
            "Cellar Log",
            "# Homer head\n\nbody\n",
        )
        .await;
        let other_folded = document_of_one_section(
            &store,
            "document:lisa",
            "Attic Notes",
            "# Lisa head\n\nbody\n",
        )
        .await;
        let caller = EntityId("bot:sigma".into());
        let under_the_id = |folded: EntityId, survivor: EntityId| {
            let store = &store;
            let caller = caller.clone();
            async move {
                let scope = Scope::new(Request {
                    bot: EntityId("bot:sigma".into()),
                    id: RequestId::parse("merge-1").expect("a good id"),
                    verb: "merge_entities".into(),
                    digest: digest_of("{}"),
                });
                within(
                    scope,
                    armed(store.merge(&folded, &survivor, None, date(2026, 6, 3), &caller, None)),
                )
                .await
            }
        };

        under_the_id(folded.clone(), survivor.clone())
            .await
            .expect("the first merge lands");
        let before = order_of(&store, &other_folded, "# Lisa head").await;
        let refused = under_the_id(other_folded.clone(), other_survivor.clone())
            .await
            .expect_err("the id is spent, so the second merge is refused");
        assert!(matches!(refused, MemoryError::Conflict), "{refused:?}");
        assert_eq!(
            order_of(&store, &other_folded, "# Lisa head").await,
            before,
            "the refused merge moved a section"
        );

        store
            .merge(
                &other_folded,
                &other_survivor,
                None,
                date(2026, 6, 3),
                &caller,
                None,
            )
            .await
            .expect("a merge with no id lands");
        assert_ne!(
            order_of(&store, &other_survivor, "# Lisa head").await,
            before,
            "a landed merge places the folded section behind the survivor's"
        );
    }

    /// **A claim stood up under the folded key after the merge is served to
    /// nobody**, as the real store serves it. Paired inside the contract case
    /// with the claim the event did map.
    #[tokio::test]
    async fn a_stray_claim_under_a_folded_key_is_served_to_nobody() {
        let store = InMemoryMemory::booted();
        let survivor = thing(&store, "person:lisa", "Lisa").await;
        let folded = thing(&store, "person:nelson", "Nelson").await;
        say(&store, &folded, "owes a dollar").await;
        store
            .merge(
                &folded,
                &survivor,
                Some("one boy, filed twice"),
                date(2026, 6, 3),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("the fold lands");
        store.stray_claim_under(&folded, "a stray claim");
        contract::a_claim_under_a_folded_key_that_no_event_lists_is_served_to_nobody(
            &store,
            &survivor,
            &folded,
            "a stray claim",
            "owes a dollar",
        )
        .await;
    }

    /// **Two events for one address: the newest decides, in the rows served as
    /// well as in the address a caller is told.** The walk takes the newest, so
    /// the world built from the same events must agree with it rather than apply
    /// the first.
    #[tokio::test]
    async fn the_rows_served_follow_the_newest_event_for_an_address() {
        let store = InMemoryMemory::booted();
        let first = thing(&store, "person:lisa", "Lisa").await;
        let folded = thing(&store, "person:nelson", "Nelson").await;
        let second = thing(&store, "person:skinner", "Skinner").await;
        let made = say(&store, &folded, "owes a dollar").await;
        store
            .merge(
                &folded,
                &first,
                Some("one boy, filed twice"),
                date(2026, 6, 3),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("the fold lands");
        // The state a hand-edited store holds: a later event moves the same
        // address somewhere else.
        let (folded_key, _) = store.resolve(&folded).expect("the folded thing");
        let (second_key, _) = store.resolve(&second).expect("the other survivor");
        store
            .merges
            .lock()
            .expect("fake mutex poisoned")
            .push(StoredMerge {
                folded: folded_key,
                survivor: second_key,
                at: jiff::Timestamp::now(),
                session: None,
                claims: vec![(made.local.clone(), FactId("f77".into()))],
            });

        assert_eq!(
            store.moved(&made),
            Some(FactAddress::new(second.clone(), FactId("f77".into()))),
            "the walk takes the newest event"
        );
        let served = store.recall(&second).await.expect("recall reads");
        assert!(
            served
                .iter()
                .any(|fact| fact.id.as_str() == "f77" && fact.content == "owes a dollar"),
            "the rows are served where the walk says: {served:?}"
        );
        let left = store.recall(&first).await.expect("recall reads");
        assert!(
            left.iter().all(|fact| fact.content != "owes a dollar"),
            "and not where the older event put them: {left:?}"
        );
    }
}
