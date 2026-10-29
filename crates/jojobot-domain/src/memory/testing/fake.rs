use std::sync::Mutex;

use jiff::civil::Date;

use super::*;
use crate::memory::claim_walk::{
    ClaimMove, ClaimWalk, MergeLoop, folded_into_walk, follow_claim, follow_thing,
};
use crate::memory::forward_field_value;

mod classes;
mod fold_reads;
mod link_rule;
mod resolution;
mod scans;
use resolution::Resolution;
mod merge_walks;
use crate::memory::MergeEvent as StoredMerge;
use crate::memory::claim_walk::{moves_in as claim_moves, survivor_in};
use fold_reads::FakeFoldReads;
use merge_walks::{ended_at, naming_folded_in};
mod numbering;
mod stale_claim;

/// An in-memory [`Memory`] adapter for tests. Holds entities and facts in `Vec`s
/// behind `Mutex`es; mints fact ids `f1`, `f2`, … per home doc, mirroring the
/// real store's per-doc numbering. A fresh instance starts empty.
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
        merge_walks::merge_writes(&mut self.writes, merges);
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
    fn forward_values(&mut self, merges: &[StoredMerge]) {
        let survivor_of = survivor_in(merges);
        let expected = "the fake merges nothing into itself, so its merges never loop";
        for fact in self.facts.iter_mut() {
            for value in fact.fields.values_mut() {
                if let Some(now) = forward_field_value(value, &survivor_of).expect(expected) {
                    *value = now;
                }
            }
        }
        for write in self.writes.iter_mut() {
            if let Some(value) = &write.value
                && let Some(now) = forward_field_value(value, &survivor_of).expect(expected)
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
    /// **Which bot each run belongs to**, the way the real store reads it from its
    /// session table. `None` is a store that cannot tell, and then no claim reads as
    /// another bot's ([`Fact::written_by_another`]).
    run_bots: Option<std::sync::Arc<dyn crate::session::RunBots>>,
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
    /// **How many times one thing's ages have been read**, through
    /// [`Memory::field_ages`]. A walk reads them once for every object it serves;
    /// a read that serves an object twice reads them twice, and this is how a
    /// case sees it.
    age_reads: std::sync::atomic::AtomicUsize,
    /// **How many times one claim's writes have been read**, through
    /// [`Memory::claim_history`]. A walk reads them once for every claim of every
    /// object it serves, to count its revisions.
    claim_history_reads: std::sync::atomic::AtomicUsize,
    /// **The attempts apps have spent**, by app and attempt id, with the digest
    /// of the body that spent it and what it landed. The real store keeps the
    /// same in `app_write`, written in the transaction of the write.
    app_writes:
        Mutex<std::collections::BTreeMap<(String, String), (String, crate::app::AppReceipt)>>,
    /// **Whether an app's event lands and the fold behind it then fails to
    /// refresh**, so the store answers [`MemoryError::FoldBehind`] with what it
    /// wrote. The wrapper that holds the fold returns this after its inner write
    /// landed and its refresh did not; a case switches it on through
    /// [`InMemoryMemory::land_app_events_behind`].
    app_events_land_behind: std::sync::atomic::AtomicBool,
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
    /// **Who wrote each key of each kind**, as the real store's origin column
    /// holds it per row. Keyed by kind token and key.
    kind_key_origins:
        Mutex<std::collections::BTreeMap<(String, String), crate::memory::types::Origin>>,
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

/// The claim with the fields, setting keys and reaching keys its writes fold to
/// (the domain's one fold), over writes in any order.
fn with_folded_fields(
    fact: &Fact,
    writes: impl IntoIterator<Item = (u64, String, Option<String>, crate::memory::WriteClass)>,
) -> Fact {
    let mut writes: Vec<_> = writes.into_iter().collect();
    writes.sort_by_key(|write| write.0);
    let (fields, sets, reaching) = super::super::stored::fold_field_writes(
        writes
            .into_iter()
            .map(|(_, key, value, class)| (key, value, class)),
    );
    Fact {
        fields,
        sets,
        reaching,
        ..fact.clone()
    }
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

    /// How many times one thing's ages have been read. A caller measures a delta
    /// across the call it is checking.
    pub fn age_reads(&self) -> usize {
        self.age_reads.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// How many times one claim's writes have been read. A caller measures a
    /// delta across the call it is checking.
    pub fn claim_history_reads(&self) -> usize {
        self.claim_history_reads
            .load(std::sync::atomic::Ordering::SeqCst)
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

    /// **This store reading the bot of a run from `bots`.** See the field.
    pub fn with_run_bots(mut self, bots: std::sync::Arc<dyn crate::session::RunBots>) -> Self {
        self.run_bots = Some(bots);
        self
    }

    /// **Whether the run that first wrote a claim belongs to a bot other than
    /// `thing`.** `false` when the claim has no recorded session, when no run
    /// answers to it, and when this store cannot tell. The test behind
    /// [`Fact::written_by_another`], with the real store's copy in its adapter.
    fn another_bots_claim(&self, home: &EntityId, id: &FactId, thing: &EntityId) -> bool {
        let (recorded, run) = self.first_write(home, id);
        let writer = crate::memory::authorship::first_writer(recorded, || {
            let bots = self.run_bots.as_ref()?;
            run.and_then(|run| bots.bot_of_run(&run))
        });
        self.another_bot(writer.as_ref(), thing)
    }

    /// **Whether a claim on `thing` written by `writer` is another bot's.** The rule
    /// is the domain's ([`crate::memory::authorship::another_bots_claim`]); this keeps
    /// only the comparison a store needs. A session keeps the bot as the storage key
    /// the layer above resolved, as the real store's session table does, so the key
    /// is asked beside the handle.
    fn another_bot(&self, writer: Option<&EntityId>, thing: &EntityId) -> bool {
        crate::memory::authorship::another_bots_claim(thing, writer, |bot| {
            self.storage_key(thing).is_some_and(|key| *bot == key)
        })
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
                    Err(match ledger.take_refusal() {
                        Some(words) => MemoryError::Refused(words),
                        None => MemoryError::Conflict,
                    })
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

    /// **The rows a kind declaration writes**, as the port's `declare_kind` used
    /// to write them; the port method wraps this to say whether anything moved.
    fn declare_kind_rows(
        &self,
        token: &str,
        origin: crate::memory::types::Origin,
        fields: Vec<crate::memory::types::Field>,
    ) -> Result<(), MemoryError> {
        let mut kinds = self.kinds.lock().unwrap();
        if let Some((held, held_origin)) = kinds.iter_mut().find(|(held, _)| held == token) {
            crate::memory::types::guard_kind_replacement(
                held,
                origin,
                Some(*held_origin),
                !fields.is_empty(),
            )?;
            // A caller's keys added to a shipped kind leave it the software's.
            if !(origin == crate::memory::types::Origin::Declared
                && *held_origin == crate::memory::types::Origin::Shipped)
            {
                *held_origin = origin;
            }
            drop(kinds);
            return self.keys_of_kind(token, origin, fields);
        }
        kinds.push((token.to_string(), origin));
        drop(kinds);
        self.keys_of_kind(token, origin, fields)
    }

    /// **Everything the store holds for one kind**: its row, its keys and who
    /// wrote each of them.
    #[allow(clippy::type_complexity)]
    fn kind_state(
        &self,
        token: &str,
    ) -> (
        Option<crate::memory::types::Origin>,
        Option<crate::memory::types::DeclaredType>,
        Vec<(String, crate::memory::types::Origin)>,
    ) {
        let row = self
            .kinds
            .lock()
            .unwrap()
            .iter()
            .find(|(held, _)| held == token)
            .map(|(_, origin)| *origin);
        let keys = self
            .types
            .lock()
            .unwrap()
            .iter()
            .find(|held| held.name == token && self.kind_keys.lock().unwrap().contains(&held.name))
            .cloned();
        let owners = self
            .kind_key_origins
            .lock()
            .unwrap()
            .iter()
            .filter(|((kind, _), _)| kind == token)
            .map(|((_, key), origin)| (key.clone(), *origin))
            .collect();
        (row, keys, owners)
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
        // **What the kind holds now, key by key with who wrote it**, planned by
        // the one function both stores call.
        let mut origins = self.kind_key_origins.lock().unwrap();
        let prior: Vec<crate::memory::types::Field> = types
            .iter()
            .find(|held| held.name == token && self.kind_keys.lock().unwrap().contains(&held.name))
            .map(|held| held.fields.clone())
            .unwrap_or_default();
        let prior_origin = types
            .iter()
            .find(|held| held.name == token && self.kind_keys.lock().unwrap().contains(&held.name))
            .map_or(origin, |held| held.origin);
        let held: Vec<crate::memory::types::HeldKindKey> = prior
            .iter()
            .map(|field| crate::memory::types::HeldKindKey {
                key: field.key.clone(),
                origin: origins
                    .get(&(token.to_string(), field.key.clone()))
                    .copied()
                    .unwrap_or(prior_origin),
            })
            .collect();
        let plan = crate::memory::types::plan_kind_keys(token, origin, &fields, &held)?;
        origins.retain(|(kind, _), _| kind != token);
        for row in &plan.rows {
            origins.insert((token.to_string(), row.key.clone()), row.origin);
        }
        let first = plan.rows.first().map_or(origin, |row| row.origin);
        let planned: Vec<crate::memory::types::Field> = plan
            .rows
            .into_iter()
            .filter_map(|row| {
                row.written
                    .or_else(|| prior.iter().find(|field| field.key == row.key).cloned())
            })
            .collect();
        types.retain(|held| held.name != token);
        types.push(DeclaredType {
            name: token.to_string(),
            origin: first,
            fields: planned,
            must: None,
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

    /// **From now on an app's event lands and the fold then fails to refresh.**
    /// The write is stored and a repeat of its attempt answers its receipt; the
    /// call answers [`MemoryError::FoldBehind`] with what landed.
    pub fn land_app_events_behind(&self) {
        self.app_events_land_behind
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }

    /// **Stage a stored type without the declaration guard seeing it** — the
    /// state a store holds when the type was declared before a rule existed.
    ///
    /// A build that did not yet refuse the keys that describe a record could
    /// store a type naming one with a fold of its own, and the build after it
    /// still has to serve every read that type answered. No verb can write
    /// that type now, so a case about the reads enters the state the way it
    /// arose. **It is for standing up damage, never for convenience.**
    pub fn stored_type_past_the_guard(&self, declared: crate::memory::types::DeclaredType) {
        let declared = declared.normalized();
        let mut held = self.types.lock().unwrap();
        held.retain(|t| t.name != declared.name);
        held.push(declared);
    }

    /// **Stage a record's keys without the write guard seeing them** — the
    /// state a thing holds when it was written before a rule existed.
    ///
    /// A loop made before a rhythm's schedule was checked at the making can
    /// hold `advances_from` and no cadence, and the guard now refuses to
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
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            reaching: Default::default(),
            field_links: Default::default(),
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
        self.append_claim_write(&stored, None, None);
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
        let merges = self.merges.lock().expect("fake mutex poisoned");
        world.relocate(&merges);
        for merge in merges.iter() {
            world.repoint(merge);
        }
        world.forward_values(&merges);
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
        let mut merges = self.merges.lock().expect("fake mutex poisoned").clone();
        merges.push(extra);
        world.relocate(&merges);
        for merge in merges.iter() {
            world.repoint(merge);
        }
        world.forward_values(&merges);
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
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            reaching: Default::default(),
            field_links: Default::default(),
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
        self.append_claim_write(&stray, None, None);
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

    /// **A fact's address, under the handle its home answers to today** —
    /// never the raw `f.home`, which is the storage key and would print as a
    /// badge.
    fn address_under(&self, f: &Fact, handle: &EntityId) -> String {
        FactAddress::new(handle.clone(), f.id.clone()).to_string()
    }

    /// **Every reference-typed field value, lowered from whatever handle a
    /// caller wrote to the permanent id the store keeps** (rule 268, reaching
    /// a field the way it already reaches an edge's object and a ref). Each
    /// item was already checked to exist, so `storage_key` cannot miss on
    /// one; the fallback only covers a caller-supplied value that never
    /// looked like a handle in the first place.
    fn lower_reference_fields(&self, fields: &mut std::collections::BTreeMap<String, String>) {
        self.lower_marked_fields(fields)
    }

    /// The thing `address` points into as it stands today, with the key its claims
    /// are filed under, or the entity miss with the near candidates that explain it.
    fn home_of(&self, address: &FactAddress) -> Result<(Entity, EntityId), MemoryError> {
        let index = self.known();
        let Some(entity) = super::resolve_handle(&address.home, &index, &self.former()) else {
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: self.nearest_for(&address.home, &index),
            });
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        Ok((entity.clone(), key))
    }

    /// The key `entity` is held under, or the refusal that names what is nearest.
    fn key_of(&self, entity: &EntityId) -> Result<EntityId, MemoryError> {
        self.resolve(entity)
            .map(|(key, _)| key)
            .ok_or_else(|| MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: self.nearest_for(entity, &self.known()),
            })
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
        self.compose_marked_in(resolution, fields)
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
        // **Read against the stored key, before it is served under the handle**: the
        // first write's session is kept under the key the claim was made under.
        f.written_by_another = crate::memory::authorship::is_connection(f.edge.as_ref())
            && self.another_bots_claim(&f.home, &f.id, handle);
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
        f.field_links = self.marked_links_in(resolution, &f.fields);
        self.compose_reference_fields_in(resolution, &mut f.fields);
        f
    }

    /// **Keep this write of the claim**, beside the row it just rewrote — the
    /// same act the real store takes in `write_fact`, so every verb that
    /// produces a claim leaves a write behind here too.
    fn append_claim_write(&self, fact: &Fact, session: Option<String>, bot: Option<EntityId>) {
        // **The bot is kept as the id it is filed under**, the one function both
        // stores take the writer they record from (rule 268).
        let bot = bot.map(|bot| {
            super::super::authorship::stored_writer(&bot, |named| self.storage_key(named))
        });
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
                bot,
                ..ClaimWrite::of(fact, ordinal, Some(self.clock().now()))
            },
        ));
    }

    /// **The bot and the session of the claim's FIRST write**: the bot recorded at
    /// the write, and the session that brought the claim in. Either is `None` when
    /// that write kept none.
    ///
    /// **Read where the claim is stored**, which is not where it is served once a
    /// merge moved it: the writes keep the key and id the claim was made under.
    fn first_write(&self, home: &EntityId, id: &FactId) -> (Option<EntityId>, Option<String>) {
        let stored = self.stored_address(&FactAddress::new(home.clone(), id.clone()));
        self.claim_writes
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .find(|(held, held_id, write)| {
                *held == stored.home && *held_id == stored.local && write.ordinal == 1
            })
            .map(|(_, _, write)| (write.bot.clone(), write.session.clone()))
            .unwrap_or_default()
    }

    /// **The session of the claim's FIRST write**, which is the session that
    /// brought the claim in — `None` when that write had none.
    fn first_write_session(&self, home: &EntityId, id: &FactId) -> Option<String> {
        self.first_write(home, id).1
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
            let links = value
                .as_deref()
                .map_or_else(Vec::new, super::super::field_link_targets);
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
                    links,
                });
        }
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
            with_folded_fields(
                fact,
                mine.iter()
                    .map(|w| (w.ordinal, w.key.clone(), w.value.clone(), w.class)),
            )
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
        with_folded_fields(
            fact,
            mine.iter()
                .map(|w| (w.ordinal, w.key.clone(), w.value.clone(), w.class)),
        )
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
        // **Who writes a thing is the domain's one answer**, the thoughts of a
        // thread and the fills of any thing together (rule 598).
        let thoughts = if subject.kind() == Some(EntityKind::THREAD) {
            self.writers_among(facts, subject)
        } else {
            Vec::new()
        };
        let writers = super::super::thread_writers::thread_writers(
            &thoughts,
            &self.fill_writers_among(facts, subject),
        );
        let above_writers = super::super::thread_writers::above_the_writers(
            writers
                .iter()
                .map(|writer| self.chain_above_among(facts, writer)),
        );
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

    /// **The bots that wrote a live record of `thread`**: the bot behind the run
    /// that made the first write of each fill. A write with no run behind it, or a
    /// run no bot answers to, names nobody, and an archived bot writes nothing.
    fn fill_writers_among(&self, facts: &[Fact], thread: &EntityId) -> Vec<EntityId> {
        let Some(key) = self.storage_key(thread) else {
            return Vec::new();
        };
        let known = self.known();
        let settings = super::super::thread_writers::settings_among(
            self.writes
                .lock()
                .expect("fake mutex poisoned")
                .iter()
                .filter(|write| {
                    write.entity == key
                        && super::super::GUARDED_KEYS
                            .iter()
                            .any(|rule| rule.key == write.key)
                })
                .map(|write| write.fact.clone()),
        );
        let mut writers: Vec<EntityId> = Vec::new();
        for fact in facts
            .iter()
            .filter(|fact| fact.home == key)
            .map(|fact| self.projected(fact))
            .filter(|fact| super::super::thread_writers::fills(fact, &settings))
        {
            // **The bot the first write recorded, else the bot behind its run**
            // (the domain's one rule, [`first_writer`]): a run no session row
            // answers to still names its bot through the write. Either is kept as
            // the storage key, so the handle it answers to now names the writer.
            let (recorded, run) = self.first_write(&fact.home, &fact.id);
            let Some(bot) = super::super::authorship::first_writer(recorded, || {
                let bots = self.run_bots.as_ref()?;
                run.and_then(|run| bots.bot_of_run(&run))
            })
            .map(|stored| self.writer_handle(&stored)) else {
                continue;
            };
            let standing = known
                .iter()
                .find(|entity| entity.id == bot)
                .is_none_or(|entity| entity.archived.is_none());
            if standing && !writers.contains(&bot) {
                writers.push(bot);
            }
        }
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
        // A manager folded into another is the one it became.
        super::super::with_manager_served(fold, |stored| self.writer_handle(stored))
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
                    recorded_by: carried.app_stamp.clone(),
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
    /// **The things this write pointed at, under the types declared when it was
    /// made**: the rows the real store's link table draws for it, kept for good.
    links: Vec<String>,
}

#[async_trait::async_trait]
impl Memory for InMemoryMemory {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        Some(
            self.key_of(entity)
                .ok()
                .and_then(|id| self.held(&id).get(key).cloned()),
        )
    }

    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let session = new.session.clone();
        let parent = new.parent.as_ref().map(|p| self.row_handle(p));
        validate_entity(
            &new.id,
            &new.name,
            &new.aliases,
            &new.source,
            new.crm.as_deref(),
            parent.as_ref(),
        )?;
        let index = self.known_by_name();
        if let Decision::Block(candidates) = guard::decide_creating(
            &new.id,
            &new.labels(),
            parent.as_ref(),
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
            parent,
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
    async fn record_app_event(
        &self,
        event: crate::app::AppEvent,
    ) -> Result<crate::app::AppOutcome, MemoryError> {
        let key = (event.app.clone(), event.attempt.clone());
        if let Some((digest, receipt)) = self
            .app_writes
            .lock()
            .expect("fake mutex poisoned")
            .get(&key)
        {
            return Ok(if *digest == event.digest {
                crate::app::AppOutcome::Repeat(receipt.clone())
            } else {
                crate::app::AppOutcome::Conflict
            });
        }
        let digest = event.digest.clone();
        let landed = self
            .add_entity_with_first_claim(event.entity, event.first)
            .await?;
        if let Guarded::Written((entity, fact)) = &landed {
            self.app_writes.lock().expect("fake mutex poisoned").insert(
                key,
                (
                    digest,
                    crate::app::AppReceipt {
                        handle: entity.id.to_string(),
                        address: fact.address().to_string(),
                    },
                ),
            );
        }
        if let Guarded::Written((entity, fact)) = &landed
            && self
                .app_events_land_behind
                .load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(MemoryError::FoldBehind {
                landed: crate::memory::Landed::Creation(Box::new((entity.clone(), fact.clone()))),
                behind: search::Behind::Stale,
                source: Box::new(MemoryError::Store("the fold's re-read failed".into())),
            });
        }
        Ok(crate::app::AppOutcome::Landed(Box::new(landed)))
    }

    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        mut first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        if first.subject != new.id {
            return Err(MemoryError::InvalidFact(format!(
                "the first claim is about {}, and the thing being made is {}",
                first.subject, new.id
            )));
        }
        // **The engine's own records are written in this act**, after the first claim.
        let also = std::mem::take(&mut first.also_written);
        if let Some(wrong) = also.iter().find(|record| record.subject != new.id) {
            return Err(MemoryError::InvalidFact(format!(
                "a record written with the first claim is about {}, and the thing being made is {}",
                wrong.subject, new.id
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
                for record in also {
                    match self.capture(record).await {
                        Ok(Guarded::Written(_)) => {}
                        Ok(Guarded::Blocked {
                            attempted,
                            candidates,
                        }) => {
                            take_back();
                            return Ok(Guarded::Blocked {
                                attempted,
                                candidates,
                            });
                        }
                        Err(e) => {
                            take_back();
                            return Err(e);
                        }
                    }
                }
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
        // A drop can rewrite an existing claim; truncating appended rows cannot undo it.
        let entities_before = self.entities.lock().expect("fake mutex poisoned").clone();
        let facts_before = self.facts.lock().expect("fake mutex poisoned").clone();
        let writes_before = self.writes.lock().expect("fake mutex poisoned").clone();
        let claim_writes_before = self
            .claim_writes
            .lock()
            .expect("fake mutex poisoned")
            .clone();
        let entity_writes_before = self
            .entity_writes
            .lock()
            .expect("fake mutex poisoned")
            .clone();
        let entity_versions_before = self
            .entity_versions
            .lock()
            .expect("fake mutex poisoned")
            .clone();
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
            *self.entities.lock().expect("fake mutex poisoned") = entities_before.clone();
            *self.facts.lock().expect("fake mutex poisoned") = facts_before.clone();
            *self.writes.lock().expect("fake mutex poisoned") = writes_before.clone();
            *self.claim_writes.lock().expect("fake mutex poisoned") = claim_writes_before.clone();
            *self.entity_writes.lock().expect("fake mutex poisoned") = entity_writes_before.clone();
            *self.entity_versions.lock().expect("fake mutex poisoned") =
                entity_versions_before.clone();
            *self.world_cache.lock().expect("fake mutex poisoned") = None;
        };
        match self.capture(claim).await {
            Ok(Guarded::Written(fact)) => {
                if let Err(e) = self.spend_claim(request, || {
                    crate::request::entity_claim_landed(&entity, &fact)
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
        let handle = &self.row_handle(handle);
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
        let id = &self.row_handle(id);
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == id) else {
            if self.is_supplied(id) {
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
        let id = &self.row_handle(id);
        let mut entities = self.entities.lock().expect("fake mutex poisoned");
        let Some(entity) = entities.iter_mut().find(|e| &e.id == id) else {
            if self.is_supplied(id) {
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
        let from = &self.row_handle(from);
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
        let parent = parent.as_ref().map(|p| self.row_handle(p));
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
        self.spend_request(|| format!("{{\"handle\":\"{to}\"}}"))?;
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
        self.touched_naming(&entity, stored_parent.as_ref(), session);
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
        validate_new_fact(&fact)?;
        // **The receipt is judged after the fields**, in the order the real store
        // asks, so one call with two faults is told the same fault by both.
        let receipt = fact
            .receipt
            .clone()
            .map(crate::memory::Receipt::validated)
            .transpose()?;
        let standing = standing_of(&fact);
        crate::memory::receipt::rides_only_settled(receipt.as_ref(), standing)?;
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
            && let Decision::Block(candidates) = self.decide_edge(edge, &index)?
        {
            return Ok(Guarded::Blocked {
                attempted: edge.object.clone(),
                candidates,
            });
        }
        // **A reference key names an entity, so it faces the same rule the
        // edge's object faces** — beside the real store's copy, because a rule
        // that holds in one adapter holds until somebody switches adapters.
        for object in self.named_by(&fact.fields)? {
            if let Decision::Block(candidates) = self.decide_named(&object, &index) {
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
            if let Decision::Block(candidates) = self.decide_named(object, &index) {
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
        // **The thought a drop archives is written only once every guard has
        // passed**, as the real store's transaction rolls the archive back when a
        // later guard refuses. The copy of the records this call reads is
        // archived at once, so the later guards see the room as it will stand.
        let mut dropped: Option<(FactAddress, Fact)> = None;
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
        // **A claim another bot writes on this thing is no thought of the thing's**,
        // so it faces neither the body cap nor the room: it lands as an ordinary
        // claim (see [`Fact::written_by_another`]).
        let writer = fact.written_by_bot.clone().map(|bot| {
            super::super::authorship::stored_writer(&bot, |named| self.storage_key(named))
        });
        let writer = writer.or_else(|| {
            self.run_bots
                .as_ref()
                .zip(fact.session.as_deref())
                .and_then(|(bots, run)| bots.bot_of_run(run))
        });
        let written_by_another = self.another_bot(writer.as_ref(), &subject_entity.id);
        // TEMPORARY(9.3.3) this store's witness that it calls the domain's rule on
        // a capture; the write path moves into the domain's pipeline there.
        if crate::memory::authorship::is_thought_of(edge.as_ref(), written_by_another) {
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
                let existing_owned: Vec<Fact> = facts
                    .iter()
                    .filter(|f| f.home == home)
                    // **Served whole, as the real store's room is**: the fields
                    // the claim holds and every pointer under the handle it
                    // answers to now.
                    .map(|f| self.served(self.projected(f), &subject_entity.id))
                    .collect();
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
                                dropped = Some((
                                    FactAddress::new(home.clone(), victim.local.clone()),
                                    archived,
                                ));
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
                crate::memory::capacity::admit_capture(
                    crate::memory::capacity::Container {
                        handle: &subject_entity.id,
                        kind: Some(subject_entity.kind),
                        held: &held,
                    },
                    &live,
                    &fact.fields,
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
            app_stamp: fact.app_stamp.clone(),
            written_by_another,
            sets: Default::default(),
            reaching: Default::default(),
            field_links: Default::default(),
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
        self.guard_fit_resolved(
            subject_entity.kind.as_token(),
            &super::super::folded_fields(&writes, &declared),
            &after,
            &governs,
        )?;
        // **A rule a type declares for its claims**, judged on the record's own
        // fields as they will stand, beside the real store's copy. The engine's
        // own records are exempt.
        if stored.app_stamp.is_none() {
            super::super::must::judge(
                &declared,
                subject_entity.id.as_str(),
                None,
                &stored.fields,
                &[],
                fact.must_override.as_ref(),
                fact.must_day.unwrap_or(fact.recorded_at),
            )?;
        }
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
        if let Some((address, archived)) = dropped {
            self.rewrite_stored(
                &address,
                Fact {
                    fields: Default::default(),
                    ..archived.clone()
                },
            );
            self.append_claim_write(&archived, fact.session.clone(), fact.written_by_bot.clone());
        }
        // **The claim is kept without its fields and the fields are kept as
        // writes.** One body of data, projected on the way out.
        self.push_stored(Fact {
            fields: Default::default(),
            ..stored.clone()
        });
        // **A capture is the claim's first write.** Kept here as the real store
        // keeps it, so a claim nobody has corrected answers with one write.
        self.append_claim_write(&stored, fact.session.clone(), fact.written_by_bot.clone());
        self.append_writes(&stored.home, &stored.id, wrote, &classes);
        // **The keys this record set on its thing**, as the bags were decided above
        // and as a read of it will answer.
        let stored = Fact {
            sets: classes
                .iter()
                .filter(|(_, class)| **class == super::super::WriteClass::Sets)
                .map(|(key, _)| key.clone())
                .collect(),
            reaching: classes
                .iter()
                .filter(|(_, class)| super::super::reaches_the_thing(**class))
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
        let entities = self.list_entities(None).await?;
        let stored: Vec<Fact> = self.facts_view();
        // **Shared by every claim this call serves**, so what it costs does not
        // move with how many there are.
        let resolution = self.resolution();
        let grouped = self.writes_by_record();
        let mut pointing = Vec::new();
        // **Holders in the order of the key their rows are filed under**, as the
        // real store's link index answers them, and not in the order of the
        // listing.
        let mut keyed: Vec<(EntityId, &Entity)> = entities
            .iter()
            .map(|entity| {
                let key = match &entity.badge {
                    Some(badge) => EntityId(badge.clone()),
                    None => entity.id.clone(),
                };
                (key, entity)
            })
            .collect();
        keyed.sort_by(|a, b| super::super::referring_holder_order(a.0.as_str(), b.0.as_str()));
        // **A thing is a holder when a write of its own drew a link to the target**,
        // or to a thing folded into it; its records are then judged by today's types.
        let named: std::collections::HashSet<String> = {
            let key = self
                .resolve(target)
                .map_or_else(|| target.clone(), |(key, _)| key);
            let folded = self.keys_folded_into(&key).unwrap_or_default();
            std::iter::once(key)
                .chain(folded)
                .map(|key| key.as_str().to_string())
                .collect()
        };
        let holders: std::collections::HashSet<EntityId> = self.with_writes(|writes| {
            writes
                .iter()
                .filter(|w| w.links.iter().any(|link| named.contains(link)))
                .map(|w| w.entity.clone())
                .collect()
        });
        for (key, entity) in keyed.iter().filter(|(key, _)| holders.contains(key)) {
            for fact in stored
                .iter()
                .filter(|f| f.subject == *key || f.home == *key)
            {
                let served = self.served_in(
                    &resolution,
                    Self::projected_from(&grouped, fact),
                    &entity.id,
                );
                if super::super::fact_names_target(&served, target) {
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
        let key = self.key_of(entity)?;
        let mut held = self.held(&key);
        self.compose_reference_fields(&mut held);
        Ok(held)
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<super::super::FieldAges, MemoryError> {
        self.age_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let key = self.key_of(entity)?;
        let facts = self.facts_view();
        let writes = self.writes_on(&key, &facts);
        let keys = super::super::folded_ages(&writes, &self.declarations());
        // The newest write of any kind: field writes of any status, and the
        // writes of the thing's records.
        let claims = self.claim_writes_view();
        let ledgers = super::super::review::ledger_records(
            self.writes_view()
                .iter()
                .filter(|w| w.entity == key)
                .map(|w| (w.key.as_str(), w.value.as_deref(), w.fact.clone()))
                .collect::<Vec<_>>(),
        );
        let newest = writes
            .iter()
            .filter(|w| super::super::review::counts_as_a_change(Some(&w.key), &w.fact, &ledgers))
            .filter_map(|w| w.written_at)
            .chain(
                claims
                    .iter()
                    .filter(|(home, fact, _)| {
                        home == &key
                            && super::super::review::counts_as_a_change(None, fact, &ledgers)
                    })
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
        let wanted_merges: Vec<StoredMerge> = self
            .merges
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .filter(|m| {
                wanted
                    .as_ref()
                    .is_none_or(|key| &m.folded == key || &m.survivor == key)
            })
            .cloned()
            .collect();
        Ok(wanted_merges
            .into_iter()
            .map(|merge| StoredMerge {
                folded: self.current_handle(&merge.folded),
                survivor: self.current_handle(&merge.survivor),
                ..merge
            })
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
        self.claim_history_reads
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let moved = self.moved(address);
        let address = moved.as_ref().unwrap_or(address);
        let (entity, key) = self.home_of(address)?;
        let facts = self.facts_view();
        if !facts.iter().any(|f| f.home == key && f.id == address.local) {
            if let Some(err) = super::super::already_merged(&address.home, &entity) {
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
            if let Some(edge) = &mut write.edge {
                edge.object = self.current_handle(&edge.object);
            }
            // **The bot that made the write is served under the handle it wears
            // today**, as every other pointer is.
            if let Some(bot) = &write.bot {
                write.bot = Some(self.writer_handle(bot));
            }
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
                    written_at: write.written_at,
                    recorded_by: carried.app_stamp.clone(),
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
            if let Decision::Block(candidates) = self.decide_edge(edge, &self.known())? {
                return Ok(Guarded::Blocked {
                    attempted: edge.object.clone(),
                    candidates,
                });
            }
        }
        // **The same rule on the edit path**, over the keys this patch sets:
        // a reference names an entity, and nothing a write names is created as
        // a side effect of being named.
        for object in self.named_by(&patch.fields)? {
            if let Decision::Block(candidates) = self.decide_named(&object, &self.known()) {
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
        let (entity, key) = self.home_of(address)?;
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
            if let Some(err) = super::super::already_merged(&address.home, &entity) {
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
        self.refuses_a_stale_claim(&key, &facts, address, &patch)?;
        // **A role's own claim, renewed or re-claimed by patch, is decided
        // atomically with the write it gates** — the same check `capture`
        // runs, against the same state under the same lock, so a renewal and a
        // fresh claim are one mechanism rather than two.
        let role_state = self.role_state_for(&entity, &facts);
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
            self.touched(&entity, patch.session.as_deref());
            return Ok(Guarded::Written(self.served(self.projected(fact), &handle)));
        }
        // **The patch is applied to the record as it reads now**, so a set that
        // replaces a value is validated against the value it replaces — and the
        // claim goes back without fields, because the fields are the writes.
        let mut edited = self.projected(fact);
        // **Whether another bot first wrote this claim**, which an edit by anyone
        // does not change: the owner's keep or edit never adopts it.
        edited.written_by_another = self.another_bots_claim(&fact.home, &fact.id, &handle);
        // What the ADDRESSED RECORD carries right now — kept before the patch
        // rewrites it, because that is what decides which of the patch's clears
        // is a write and which names a key this record never had.
        let carried = edited.fields.clone();
        // **Whether this record was already a thought, read before the patch
        // rewrites it** — see the real store's identical comment on its own
        // copy of this line.
        let was_thought = super::super::is_thought(&edited);
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
            // **A record that another active shape names as a source cannot be a
            // shape itself**: the other order of the one-layer rule below.
            let holders: Vec<FactAddress> = facts
                .iter()
                .filter(|f| {
                    f.status == FactStatus::Active
                        && f.stands_for
                            .iter()
                            .any(|s| s.home == key && s.local == address.local)
                })
                .map(|f| FactAddress::new(self.current_handle(&f.home), f.id.clone()))
                .collect();
            if let Some(refused) = super::super::refuses_marking_a_source(address, &holders) {
                return Err(refused);
            }
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
        let fields_before = edited.fields.clone();
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
        self.guard_fit_resolved(kind.as_token(), &before, &after, &governs)?;
        // **A rule a type declares for its claims**, judged on the record's own
        // fields before the edit and as it leaves them, beside the real store's
        // copy. The engine's own records are exempt.
        if edited.app_stamp.is_none() {
            let named: Vec<String> = patch
                .fields
                .keys()
                .chain(patch.clear_fields.iter())
                .cloned()
                .collect();
            super::super::must::judge(
                &declared,
                &address.to_string(),
                Some(&fields_before),
                &edited.fields,
                &named,
                patch.must_override.as_ref(),
                patch
                    .must_day
                    .unwrap_or_else(|| self.clock().today_in(&jiff::tz::TimeZone::UTC)),
            )?;
        }
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
        let becomes_thought =
            edited.status == FactStatus::Active && super::super::is_thought(&edited);
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
                .map(|mut f| {
                    f.written_by_another = self.another_bots_claim(&f.home, &f.id, &handle);
                    f
                })
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
        self.spend_request(|| {
            crate::request::claim_landed(&address.to_string(), home.as_str(), None)
        })?;
        self.rewrite_stored(
            &FactAddress::new(home.clone(), id.clone()),
            Fact {
                fields: Default::default(),
                ..edited.clone()
            },
        );
        // **An edit rewrites the row and appends a write**, which is what makes
        // what the claim used to say readable after it stops being true.
        self.append_claim_write(&edited, patch.session.clone(), None);
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
                merged_at: self.clock().now(),
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
                self.append_claim_write(record, session.map(str::to_string), None);
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
                self.append_claim_write(record, session.map(str::to_string), None);
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
                merged_at: self.clock().now(),
                session: session.map(str::to_string),
                claims: moved.clone(),
            });

        let record = Fact {
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            // Unclassed keys of an engine-written record are own fields that reach.
            reaching: account.fields.keys().cloned().collect(),
            field_links: Default::default(),
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
        self.append_claim_write(&record, session.map(str::to_string), None);

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
        let (entity, key) = self.home_of(address)?;
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
            if let Some(err) = super::super::already_merged(&address.home, &entity) {
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
            app_stamp: None,
            written_by_another: false,
            sets: Default::default(),
            // Unclassed keys of an engine-written record are own fields that reach.
            reaching: account.fields.keys().cloned().collect(),
            field_links: Default::default(),
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
        self.append_claim_write(&retracted, session.map(str::to_string), None);
        self.append_claim_write(&record, session.map(str::to_string), None);
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
        // **Neither half writes over the other's keys**, and the same line
        // `keys_of_kind` holds from the other side: the two share this place
        // and the name alone cannot say which of them wrote a row.
        //
        // **Asked before the shipped-type guard.** This list holds a kind's
        // rows beside the types', so the guard below would answer a kind's
        // name with a sentence about a type. The real store keeps the two apart
        // by owner and answers with this sentence.
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
    ) -> Result<bool, MemoryError> {
        // **Whether the store holds anything different afterwards**, read off
        // the same places the real store keeps a kind: its row, its keys and who
        // wrote each key.
        let before = self.kind_state(token);
        self.declare_kind_rows(token, origin, fields)?;
        Ok(self.kind_state(token) != before)
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
        self.kind_key_origins
            .lock()
            .expect("fake mutex poisoned")
            .retain(|(kind, _), _| kind != token);
        self.types
            .lock()
            .expect("fake mutex poisoned")
            .retain(|t| t.name != token);
        Ok(())
    }
}

#[cfg(test)]
mod tests;
