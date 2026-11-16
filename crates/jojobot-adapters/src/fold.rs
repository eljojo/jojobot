//! [`Folded`] — everything the memory verbs read, held in RAM above the store
//! (decision log 292).
//!
//! jojobot is one server instance and the only writer, so the claims State holds the
//! whole store and every read is answered from it. [`Folded`] wraps a [`Memory`]
//! and, once [`rebuild`](Folded::rebuild) has run, serves the reads from that State:
//! the listing and the rename history, the merge events, the declared types and
//! kinds, and for every thing its records, fields, ages and claim histories. The
//! fold holds no second copy of any of them. Every write goes to the store first and
//! the State holds it in the same step, so a caller who writes and reads sees the
//! write.
//!
//! Before the first `rebuild`, and after a refresh that failed, the fold is not
//! filled and every read goes to the store underneath: slower and still correct.
//!
//! ⚠️ **This is only correct with exactly one writer in the process.** The State
//! is filled from writes THIS `Folded` observed; a row changed by anything else
//! — another worker, a restored backup, a migration tool, a person running SQL
//! by hand — leaves it silently wrong, and nothing here would notice. jojobot
//! runs one application worker against its store today, which is the whole
//! reason this is authoritative rather than a guess. Add a second writer and
//! this assumption breaks first.

pub mod claims_view;
pub mod mail;
pub mod sessions;
mod uncertain;

use crate::writer::owned;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use jiff::civil::Date;

use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldAges, FieldBacking, FieldWrite, FormerHandle, Guarded, Landed, Memory,
    MemoryError, Merge, NewEntity, NewFact, Retraction, WriteSummary,
    search::{self, DocScan},
    types::{self, DeclaredType},
};

/// A [`Memory`] whose reads the claims State answers.
///
/// Every write verb forwards to the store underneath, then brings the image up
/// to date. Once the image is filled, every read that reads stored rows is
/// answered from it. Two reads forward to the layer beneath and read no row at
/// all: `composed_prose` and `echoed_defaults` join and compare what the build
/// supplies. Before the first fill, and after a refresh that failed, every read
/// goes to the store.
#[derive(Clone)]
pub struct Folded {
    writer: crate::writer::Writer,
    inner: Arc<dyn Memory>,
    /// **Set once a [`rebuild`](Folded::rebuild) has filled the fold**, and dropped by a
    /// refresh that failed; `None` sends every read to the store. The fold holds no copy
    /// of what the claims State answers: this is the mark that the State is the answer.
    filled: Arc<RwLock<Option<Filled>>>,
    /// Set by the first `rebuild`, whether or not its fill succeeded. A fill that
    /// failed, or a refresh that failed, leaves no image, and this is what tells
    /// the next write to fill it again.
    wanted: Arc<AtomicBool>,
    /// **The kinds the store declares, the one read the fold keeps beside the
    /// claims State**: about two dozen rows (token and origin) and the set built
    /// from them, which every admission asks. Read when the fold fills and again
    /// when a kind is written. It moves into the State when 9.3.5 makes kinds
    /// events. `None` until the first fill, and while the image is not held.
    kinds: Arc<RwLock<Option<HeldKinds>>>,
    /// **Set while the boot's repair passes write, before the claims import.** The
    /// log holds no State then, so a refresh after such a write reads the store, as
    /// every refresh did. TEMPORARY(9.3.8), architect #f9 item 3, rule 682: the
    /// import's own boot keeps the flag, and both modes go with `DoltMemory`.
    before_the_import: Arc<AtomicBool>,
    /// **How many times the image has changed**, bumped under the same lock
    /// that installs the change, so a reader that sees a number sees at least the
    /// state it counts. It is the change signal search reads ([`write_summary`]),
    /// answered from memory.
    ///
    /// [`write_summary`]: Memory::write_summary
    generation: Arc<AtomicU64>,
}

/// The kinds the store declares, as rows and as the set every admission asks.
struct HeldKinds {
    rows: Vec<(String, types::Origin)>,
    set: jojobot_domain::memory::kinds::KindSet,
}

impl HeldKinds {
    fn of(rows: Vec<(String, types::Origin)>) -> Self {
        HeldKinds {
            set: jojobot_domain::memory::kinds::KindSet::new(rows.clone()),
            rows,
        }
    }
}

/// The mark that the fold is filled: the claims State answers its reads.
struct Filled;

/// **Why a refresh got no answer from the claims State.**
enum Unseen {
    /// The fold holds no State: the store beneath keeps none and the writer holds no
    /// seam.
    Missing,
    /// The State is held but cannot be trusted: its seam is behind the log, read an
    /// unreadable member, or holds no import.
    Untrusted,
}

impl Folded {
    /// Wrap a store with **no image**. Nothing is read yet — the image is
    /// filled by [`rebuild`](Folded::rebuild), separately, so a store that is
    /// not reachable yet cannot stop the wrap itself from succeeding. Every
    /// read answers correctly before a rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Memory>) -> Self {
        Folded {
            writer: crate::writer::Writer::default(),
            inner,
            filled: Arc::new(RwLock::new(None)),
            wanted: Arc::new(AtomicBool::new(false)),
            kinds: Arc::new(RwLock::new(None)),
            before_the_import: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Say whether the boot's repair passes are writing, before the claims import
    /// has filled the State: a refresh after a write then reads the store. Set once
    /// the fold is filled and cleared once the import is done.
    pub fn before_the_import(&self, yes: bool) {
        self.before_the_import.store(yes, Ordering::SeqCst);
    }

    /// **Run `read` on the claims State**: the State of the store beneath, which a
    /// fake keeps and lends, else the seam of the shared writer, once it holds one.
    /// When there is no answer, [`Unseen`] says why. A caller never turns either
    /// reason into a read of the store.
    fn claims<T>(
        &self,
        read: impl FnOnce(&jojobot_domain::log::claims::State) -> T,
    ) -> Result<T, Unseen> {
        use jojobot_domain::log::claims::ClaimsLender;
        let mut read = Some(read);
        let mut out = None;
        let mut take = |state: &jojobot_domain::log::claims::State| {
            if let Some(read) = read.take() {
                out = Some(read(state));
            }
        };
        let answered = match (self.inner.claims_lender(), self.writer.seam()) {
            (Some(lender), _) => lender.lend(&mut take),
            (None, Some(seam)) => seam.lend(&mut take),
            (None, None) => return Err(Unseen::Missing),
        };
        match (answered, out) {
            (true, Some(out)) => Ok(out),
            _ => Err(Unseen::Untrusted),
        }
    }

    /// **The failure of a refresh that has no State to read**, which drops the
    /// image as any failed refresh does. A fold with no State is a mistake of
    /// construction and is loud under test; a State that cannot be trusted after
    /// an uncertain write is the fenced recovery's, which refills the image.
    fn without_state(unseen: Unseen) -> MemoryError {
        match unseen {
            Unseen::Missing => {
                if cfg!(test) {
                    panic!("a fold was refreshed after a write and holds no claims state");
                }
                MemoryError::Store(
                    "the fold holds no claims state to refresh from, so its image is dropped"
                        .into(),
                )
            }
            Unseen::Untrusted => MemoryError::Store(
                "the claims state cannot be trusted after an uncertain write, so the image \
                 is dropped and the fenced recovery refills it"
                    .into(),
            ),
        }
    }

    /// Share the serving process's writer with every other rail.
    pub fn with_writer(mut self, writer: crate::writer::Writer) -> Self {
        self.writer = writer;
        self
    }

    /// The owner the other serving rails must use too.
    pub fn writer(&self) -> crate::writer::Writer {
        self.writer.clone()
    }

    /// Fill the fold — the boot path: read the kinds the store declares and mark the
    /// fold filled. Returns how many things the claims State lists, or none while it
    /// holds no State.
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        let this = self.clone();
        self.writer
            .run(async move {
                if this.writer.rail_uncertain("memory") {
                    return Err(MemoryError::Store(
                        "the rail is uncertain; rebuild requires its fence".into(),
                    ));
                }
                // **Wanted from the first attempt, whether or not it succeeds**: a fill that
                // failed leaves the fold unfilled, and the next write fills it again.
                this.wanted.store(true, Ordering::SeqCst);
                this.fill().await
            })
            .await
    }

    /// The read behind [`rebuild`](Folded::rebuild), for a caller that already
    /// runs in the writer task. A failure leaves the fold unfilled.
    async fn fill(&self) -> Result<usize, MemoryError> {
        let filled = crate::writer::sql::reading(Box::pin(self.inner.declared_kinds())).await;
        match filled {
            Ok(kinds) => {
                let held = self
                    .claims(|state| state.stored_listing().len())
                    .unwrap_or(0);
                *self.kinds.write().expect("kinds lock") = Some(HeldKinds::of(kinds));
                {
                    let mut slot = self.filled.write().expect("fold lock");
                    *slot = Some(Filled);
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                self.publish_things(Vec::new(), true);
                Ok(held)
            }
            Err(e) => {
                if !self.writer.rail_uncertain("memory") {
                    let mut slot = self.filled.write().expect("fold lock");
                    *slot = None;
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                self.publish_things(Vec::new(), true);
                Err(e)
            }
        }
    }

    /// **The set of kinds every admission asks**, while the fold holds an image;
    /// `None` sends the read to the store.
    fn held_kind_set(&self) -> Option<jojobot_domain::memory::kinds::KindSet> {
        self.held(|_| ())?;
        self.kinds
            .read()
            .expect("kinds lock")
            .as_ref()
            .map(|held| held.set.clone())
    }

    /// **Read the kinds again after a kind was written**, and nothing else: the
    /// claims State serves every read, so no thing is read again.
    async fn refresh_kinds(&self) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            return Ok(());
        }
        match self.inner.declared_kinds().await {
            Ok(rows) => {
                *self.kinds.write().expect("kinds lock") = Some(HeldKinds::of(rows));
                self.generation.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            Err(failed) => {
                // **No kind set is held after a failed read**: the set held is the one
                // from before the write, and a read admitted by it would refuse the kind
                // just written. With none held, the reads that need it go to the store.
                *self.kinds.write().expect("kinds lock") = None;
                self.generation.fetch_add(1, Ordering::SeqCst);
                Err(failed)
            }
        }
    }

    /// Answer when the fold is filled; `None` sends the read to the store.
    fn held<T>(&self, read: impl FnOnce(&Filled) -> T) -> Option<T> {
        self.filled.read().expect("fold lock").as_ref().map(read)
    }

    /// Bring the image up to date with a write that touched only `handle`:
    /// the listing, which an entity's own row is part of, and that thing's
    /// document are read again. **Re-reading rather than computing the new value
    /// from the write just made**, so the image holds what the store answers.
    async fn refresh(&self, handle: &EntityId) -> Result<(), MemoryError> {
        self.refresh_listing(handle, true).await
    }

    /// [`refresh`](Self::refresh) for a write that cannot have changed any
    /// thing's own row: a claim written, edited or taken back. The listing is
    /// not read again, so the cost of such a write does not grow with the number
    /// of things.
    async fn refresh_claims(&self, handle: &EntityId) -> Result<(), MemoryError> {
        self.refresh_listing(handle, false).await
    }

    /// [`refresh_listing_inner`](Self::refresh_listing_inner), then the change
    /// is published, whatever the image did: a subscriber reads the thing from
    /// the State, which answers from the store when the image could not follow.
    async fn refresh_listing(&self, handle: &EntityId, relist: bool) -> Result<(), MemoryError> {
        let done = Box::pin(self.refresh_listing_inner(handle, relist)).await;
        self.publish_things(vec![handle.clone()], false);
        done
    }

    /// Tell the writer that these things changed. The generation is the State's
    /// change count now, when an image holds it.
    fn publish_things(&self, touched: Vec<EntityId>, names_changed: bool) {
        let generation = self
            .held(|_| self.generation.load(Ordering::SeqCst))
            .filter(|_| !touched.is_empty());
        self.writer.publish(crate::writer::publish::Change::Things {
            touched,
            names_changed,
            generation,
        });
    }

    async fn refresh_listing_inner(
        &self,
        handle: &EntityId,
        relist: bool,
    ) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            return Ok(());
        }
        if self.held(|_| ()).is_none() {
            return self.fill().await.map(|_| ());
        }
        // **Nothing is read again**: the claims State holds the write, and the listing, the
        // rename history and a thing's document are read from it when a read asks. What a
        // refresh checks is that the State can still be had: one that cannot fails the
        // refresh ([`Folded::without_state`]) and leaves the fold unfilled, so every read
        // goes to the store. **A record the build supplies is no decline**: the State holds it
        // under its supply identity and the layer over the fold lays the shipped half on top.
        let before_the_import = self.before_the_import.load(Ordering::SeqCst);
        if !before_the_import
            && (relist || !self.is_supplied_only(handle))
            && let Err(unseen) = self.claims(|_| ())
        {
            let mut slot = self.filled.write().expect("fold lock");
            *slot = None;
            self.generation.fetch_add(1, Ordering::SeqCst);
            return Err(Self::without_state(unseen));
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    /// Keep the last published image until an uncertain write's rail has been
    /// fenced and reloaded through its boot loader.
    async fn scoped<T>(
        &self,
        write: impl std::future::Future<Output = Result<T, MemoryError>>,
    ) -> Result<T, MemoryError> {
        // With no image to count the write in, a subscriber compares the store's
        // own signal from before the write with the one it holds.
        if self.held(|_| ()).is_none()
            && let Ok(Some(began)) = self.inner.write_summary().await
        {
            self.writer
                .publish(crate::writer::publish::Change::Began(began));
        }
        crate::writer::recovery::guarded(&self.writer, "memory", write, || self.fill()).await
    }

    /// **Where an address went when its thing was merged away**, followed through a
    /// survivor merged later, from the claims State: the thing a spelling names, folded
    /// or not, and the merge events. `None` when the fold is not filled or holds no State;
    /// `Some(Ok(None))` when no event moved the address.
    fn moved_by_state(
        &self,
        address: &FactAddress,
    ) -> Option<Result<Option<FactAddress>, jojobot_domain::memory::claim_walk::MergeLoop>> {
        use jojobot_domain::memory::claim_walk::{MergeLoop, follow_claim, moves_in};
        self.held(|_| ())?;
        self.claims(|state| {
            let Some(home) = state.own_handle_named(&address.home) else {
                return Ok(None);
            };
            let events = state.merge_events(None);
            let moved =
                follow_claim::<_, MergeLoop>(home, address.local.clone(), moves_in(&events))?;
            Ok(moved.map(|(survivor, now)| FactAddress::new(survivor, now)))
        })
        .ok()
    }

    /// **A rename or a merge moves a handle, and nothing is read again**: the claims
    /// State holds the write. The index is told the things the write names and the things
    /// that point at them, worked out from the State's records.
    async fn followed(&self, named: [&EntityId; 2]) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            self.publish_things(Vec::new(), true);
            return Ok(());
        }
        if self.held(|_| ()).is_none() {
            return self.fill().await.map(|_| ());
        }
        self.generation.fetch_add(1, Ordering::SeqCst);
        let mut touched: Vec<EntityId> = named.iter().map(|id| (*id).clone()).collect();
        for id in named {
            if let Ok(holders) = self.claims(|state| state.holders_of(id)) {
                for holder in holders {
                    if !touched.contains(&holder) {
                        touched.push(holder);
                    }
                }
            }
        }
        self.publish_things(touched, true);
        Ok(())
    }

    /// A write that has no [`Landed`] to carry its own answer, whose image could
    /// not follow it. The write stands; the image is gone, so every read goes to
    /// the store until the next write fills it again.
    fn follow(verb: &str, followed: Result<(), MemoryError>) {
        if let Err(error) = followed {
            tracing::warn!(
                verb,
                %error,
                "the in-memory image could not follow a write, so reads go to the store until \
                 the next write fills it again"
            );
        }
    }
    /// **What stands on a claim, read from the claims State** in the domain's one
    /// claim order. The kind of the address is checked against the store's own
    /// kinds, as every read of the image is. With no image, or no State to read,
    /// the store answers.
    async fn built_on_here(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        let Some(kinds) = self.held_kind_set() else {
            return self.inner.built_on(source).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, &source.home)?;
        match self.claims(|state| state.built_on(source)) {
            Ok(facts) => Ok(facts),
            Err(_) => {
                self.unheld()?;
                self.inner.built_on(source).await
            }
        }
    }

    /// **One thing's read from the claims State**, the thing a spelling answers to:
    /// the handle it wears now, one it wore, or a number under a collection. `None`
    /// sends the read to the store, which answers it as it always did: no image or
    /// no State, a spelling nothing answers to, and the two declines the State
    /// does not serve. TEMPORARY(9.3.4) `q-934-merge-version-row-in-state` for a
    /// thing a merge touched, and TEMPORARY(9.3.4)
    /// `q-934-role-lease-read-from-heartbeat-home` for a role.
    fn stated<T>(
        &self,
        id: &EntityId,
        read: impl FnOnce(&jojobot_domain::log::claims::State, &Entity, &[DeclaredType]) -> Option<T>,
    ) -> Option<T> {
        self.held(|_| ())?;
        self.claims(|state| {
            let entity = state.resolved_entity(id)?;
            let object = state.resolve(&entity.id.0)?;
            if state.was_merged(object) || state.holds_a_lease(object) {
                return None;
            }
            read(state, &entity, &state.declared_types_shared())
        })
        .ok()
        .flatten()
    }

    /// **A thing's document as the claims State serves it**: `Some(None)` for a
    /// spelling that is not the thing's own handle, `None` when the State declines
    /// the thing (see [`stated`](Folded::stated)).
    fn scan_from_state(&self, entity: &EntityId) -> Option<Option<DocScan>> {
        self.stated(entity, |state, found, declared| {
            if found.id != *entity {
                return Some(None);
            }
            let object = state.resolve(&found.id.0)?;
            Some(Some(state.document_scan(object, declared)?))
        })
    }

    /// **One piece of a thing's history, worked out from the claims State at read
    /// time**: the State builds that piece and no other part of the document.
    /// `None` under the same declines as [`stated`](Folded::stated).
    fn part_of<T>(
        &self,
        id: &EntityId,
        pick: impl FnOnce(
            &jojobot_domain::log::claims::State,
            &jojobot_domain::log::ObjectId,
            &[DeclaredType],
        ) -> T,
    ) -> Option<T> {
        self.stated(id, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            Some(pick(state, object, declared))
        })
    }

    /// **The writes of the claim at `address`, or the miss the port gives**: none
    /// is no record, because a claim that stands has at least one write. `None`
    /// when the State cannot say.
    fn chain_of(&self, address: &FactAddress) -> Option<Result<Vec<ClaimWrite>, MemoryError>> {
        self.stated(&address.home, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            if let Some(chain) = state.claim_chain_of(object, &address.local)
                && !chain.is_empty()
            {
                return Some(Ok(chain));
            }
            let scan = state.document_scan(object, declared)?;
            Some(Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: scan.facts.iter().map(|f| f.address().to_string()).collect(),
            }))
        })
    }

    /// **The miss of a read of a spelling nothing answers to**, made from the
    /// claims State, so a miss reaches no store either. `None` when the spelling
    /// answers to a thing, or there is no image or State to say it does not.
    fn missed(&self, id: &EntityId) -> Option<MemoryError> {
        self.held(|_| ())?;
        self.claims(|state| {
            state
                .resolved_entity(id)
                .is_none()
                .then(|| state.unknown_entity(id))
        })
        .ok()
        .flatten()
    }

    /// **Whether the handle names a record the build supplies that no stored row
    /// wears**: the State resolves it to an object under a supply identity, which
    /// holds no document in the image.
    fn is_supplied_only(&self, handle: &EntityId) -> bool {
        self.claims(|state| {
            state.resolve(&handle.0).is_some_and(|object| {
                object
                    .0
                    .starts_with(jojobot_domain::log::claims::import::SUPPLY_PREFIX)
            })
        })
        .unwrap_or(false)
    }

    fn readable(&self) -> Result<(), MemoryError> {
        if self.writer.rail_uncertain("memory") && self.filled.read().expect("fold lock").is_none()
        {
            self.unheld()?;
        }
        Ok(())
    }

    fn unheld(&self) -> Result<(), MemoryError> {
        if self.writer.rail_uncertain("memory") {
            return Err(MemoryError::Store(
                "the uncertain rail has no published image".into(),
            ));
        }
        Ok(())
    }
}

/// **The write already landed; only the re-read after it failed.** Every
/// write method below reaches for this instead of propagating `source`
/// straight through `?`, which would tell the caller the write itself had
/// failed — false, since `landed` is only ever built from what the inner
/// store already returned as written.
fn fold_behind(landed: Landed, source: MemoryError) -> MemoryError {
    MemoryError::FoldBehind {
        landed,
        behind: search::Behind::Stale,
        source: Box::new(source),
    }
}

/// **The fold as a store a layer can wrap by value.** The layer that supplies what
/// the build ships takes its store by value, and the caller keeps its own `Arc` for
/// boot work such as the fold's `rebuild`, so every call goes through to the same
/// fold.
pub struct SharedFold(pub Arc<Folded>);

// TEMPORARY(9.3.8) `deploy-9-3-8`: a forwarder of the `Memory` port, which rule 682 removes at 9.3.8.
#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for SharedFold {}

#[async_trait]
impl Memory for Folded {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        self.stated(entity, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            Some(state.fields(object, declared).get(key).cloned())
        })
    }

    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        owned!(self, this, [], this.add_entity(new).await);
        self.scoped(async {
            let made = self.inner.add_entity(new).await?;
            if let Guarded::Written(entity) = &made {
                Folded::follow("add_entity", self.refresh(&entity.id).await);
            }
            Ok(made)
        })
        .await
    }

    /// **A creation that writes its first claim refreshes the image for the new
    /// thing, as a capture does.** Forwarded and not left to the port's default,
    /// which refuses.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        owned!(
            self,
            this,
            [],
            this.add_entity_with_first_claim(new, first).await
        );
        self.scoped(async {
            match self.inner.add_entity_with_first_claim(new, first).await? {
                Guarded::Written((entity, fact)) => match self.refresh(&fact.home).await {
                    Ok(()) => Ok(Guarded::Written((entity, fact))),
                    Err(source) => Err(fold_behind(
                        Landed::Creation(Box::new((entity, fact))),
                        source,
                    )),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    /// **An app's event refreshes the image for the event it made, as a creation
    /// does.** A repeat or a conflict wrote nothing and refreshes nothing.
    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        owned!(self, this, [], this.record_app_event(event).await);
        use jojobot_domain::app::AppOutcome;
        self.scoped(async {
            match self.inner.record_app_event(event).await? {
                AppOutcome::Landed(landed) => match *landed {
                    Guarded::Written((entity, fact)) => match self.refresh(&fact.home).await {
                        Ok(()) => Ok(AppOutcome::Landed(Box::new(Guarded::Written((
                            entity, fact,
                        ))))),
                        Err(source) => Err(fold_behind(
                            Landed::Creation(Box::new((entity, fact))),
                            source,
                        )),
                    },
                    blocked => Ok(AppOutcome::Landed(Box::new(blocked))),
                },
                other => Ok(other),
            }
        })
        .await
    }

    /// **An edit that writes a claim refreshes the image for the thing, as a
    /// capture does.** Forwarded and not left to the port's default, which
    /// refuses.
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        owned!(
            self,
            this,
            [handle = handle.clone()],
            this.update_entity_with_claim(&handle, patch, claim).await
        );
        self.scoped(async {
            match self
                .inner
                .update_entity_with_claim(handle, patch, claim)
                .await?
            {
                Guarded::Written((entity, fact)) => match self.refresh(&fact.home).await {
                    Ok(()) => Ok(Guarded::Written((entity, fact))),
                    Err(source) => Err(fold_behind(
                        Landed::Creation(Box::new((entity, fact))),
                        source,
                    )),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_some() {
            match self.claims(|state| state.stored_listing()) {
                Ok(listing) => {
                    return Ok(listing
                        .into_iter()
                        .filter(|entity| kind.is_none_or(|kind| entity.kind == kind))
                        .collect());
                }
                Err(_) => self.unheld()?,
            }
        }
        self.inner.list_entities(kind).await
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_none() {
            return self.inner.former_handles().await;
        }
        // **The rename history and the handles the held numbers make are read from the
        // State when a read asks and stored nowhere**, so a number that moves takes its
        // handle with it.
        let derived = self.claims(|state| {
            let mut former: Vec<FormerHandle> = state
                .former_handles()
                .into_iter()
                .filter(|f| jojobot_domain::memory::numbered::numbered_handle(&f.former).is_none())
                .collect();
            let listing = state.stored_listing();
            let declared = state.declared_types_shared();
            let fields: std::collections::BTreeMap<_, _> = listing
                .iter()
                .filter_map(|entity| {
                    let object = state.resolve(&entity.id.0)?;
                    Some((entity.id.clone(), state.fields(object, &declared)))
                })
                .collect();
            if fields
                .values()
                .any(|held| held.contains_key(jojobot_domain::memory::numbered::NUMBERS_BY))
            {
                former.extend(jojobot_domain::memory::numbered::derived_formers(
                    &listing, &fields,
                ));
            }
            former
        });
        match derived {
            Ok(former) => Ok(former),
            Err(_) => {
                self.unheld()?;
                self.inner.former_handles().await
            }
        }
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        owned!(
            self,
            this,
            [handle = handle.clone()],
            this.update_entity(&handle, patch).await
        );
        self.scoped(async {
            let edited = self.inner.update_entity(handle, patch).await?;
            if let Guarded::Written(entity) = &edited {
                Folded::follow("update_entity", self.refresh(&entity.id).await);
            }
            Ok(edited)
        })
        .await
    }

    /// **A rename reads again the things whose documents spell the old handle.**
    /// The handle a thing answers to, the parent of each thing under it, and every
    /// record, field and mention that names it are all served under the handle it
    /// wears now, so every document that spells the old handle is read again, and
    /// no other.
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        owned!(
            self,
            this,
            [
                from = from.clone(),
                to = to.clone(),
                owned_token = resemblance.token.map(str::to_string),
                owned_reason = resemblance.reason.map(str::to_string),
                session = session.map(str::to_string)
            ],
            this.rename_entity(
                &from,
                &to,
                parent,
                date,
                jojobot_domain::memory::Override::new(
                    owned_token.as_deref(),
                    owned_reason.as_deref(),
                ),
                session.as_deref(),
            )
            .await
        );
        self.scoped(async {
            let renamed = self
                .inner
                .rename_entity(from, to, parent, date, resemblance, session)
                .await?;
            if let Guarded::Written(entity) = &renamed {
                let followed = self.followed([from, &entity.id]).await;
                // **A fold that could not follow still tells the index which
                // things the rename touched**, so it re-reads them itself.
                if followed.is_err() {
                    self.publish_things(vec![from.clone(), entity.id.clone()], true);
                }
                Folded::follow("rename_entity", followed);
            }
            Ok(renamed)
        })
        .await
    }

    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        owned!(
            self,
            this,
            [
                id = id.clone(),
                reason = reason.to_string(),
                session = session.map(str::to_string)
            ],
            this.archive_entity(&id, &reason, session.as_deref()).await
        );
        self.scoped(async {
            let archived = self.inner.archive_entity(id, reason, session).await?;
            Folded::follow("archive_entity", self.refresh(&archived.id).await);
            Ok(archived)
        })
        .await
    }

    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        owned!(
            self,
            this,
            [id = id.clone(), session = session.map(str::to_string)],
            this.restore_entity(&id, session.as_deref()).await
        );
        self.scoped(async {
            let restored = self.inner.restore_entity(id, session).await?;
            Folded::follow("restore_entity", self.refresh(&restored.0.id).await);
            Ok(restored)
        })
        .await
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        owned!(self, this, [], this.capture(fact).await);
        self.scoped(async {
            match self.inner.capture(fact).await? {
                Guarded::Written(fact) => match self.refresh_claims(&fact.home).await {
                    Ok(()) => Ok(Guarded::Written(fact)),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    /// **The companion claim of an edit of a record the build supplies**, written
    /// as a capture is: the store takes it, and the thing it is about is refreshed
    /// from the State. The layer over the fold answers the edit with its own
    /// entity and chooses nothing here.
    async fn capture_companion(&self, claim: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        owned!(self, this, [], this.capture_companion(claim).await);
        self.scoped(async {
            match self.inner.capture_companion(claim).await? {
                Guarded::Written(fact) => match self.refresh_claims(&fact.home).await {
                    Ok(()) => Ok(Guarded::Written(fact)),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.readable()?;
        let stated = self.stated(subject, |state, entity, declared| {
            let object = state.resolve(&entity.id.0)?;
            Some(state.document_scan(object, declared)?.facts)
        });
        match stated {
            Some(facts) => Ok(facts),
            None => match self.missed(subject) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.recall(subject).await
                }
            },
        }
    }

    /// **Answered from the claims State**, with each stored link forwarded through
    /// the merges as the store forwards it.
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.readable()?;
        match self.part_of(entity, |state, object, _| state.key_history(object, key)) {
            Some(writes) => Ok(writes),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.history(entity, key).await
                }
            },
        }
    }

    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_none() {
            return self.inner.merge_events(entity).await;
        }
        let read = self.claims(|state| match entity {
            Some(spelling) => state.merge_events_of(spelling),
            None => state.merge_events(None),
        });
        match read {
            Ok(events) => Ok(events),
            Err(_) => {
                self.unheld()?;
                self.inner.merge_events(entity).await
            }
        }
    }

    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.readable()?;
        match self.moved_by_state(address) {
            Some(answer) => Ok(answer?),
            None => self.inner.moved_address(address).await,
        }
    }

    /// **Answered from the image**, which holds what the store named when it
    /// filled the thing's document; no read goes to the store.
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::unmapped::UnmappedClaim>, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, _declared| {
            if found.id != *entity {
                return Some(Vec::new());
            }
            let object = state.resolve(&found.id.0)?;
            Some(state.unmapped_claims_of(object))
        });
        match stated {
            Some(unmapped) => Ok(unmapped),
            None if self.missed(entity).is_some() => Ok(Vec::new()),
            None => {
                self.unheld()?;
                self.inner.unmapped_claims(entity).await
            }
        }
    }

    /// **The writes of a claim, answered from the claims State.** An address from
    /// before a merge, and a claim on a thing a merge touched, are the store's:
    /// it keeps the merge's own version rows, which the State does not hold yet.
    /// TEMPORARY(9.3.4) `q-934-merge-version-row-in-state`.
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.readable()?;
        if let Some(answer) = self.chain_of(address) {
            return answer;
        }
        // **A claim a merge left under the folded thing's key** is no claim of the
        // folded thing's own now: the survivor names it, and its writes are built for
        // that claim alone.
        if self.held(|_| ()).is_some()
            && let Ok(Some(chain)) = self.claims(|state| {
                let object = state.resolve(&address.home.0)?;
                state.unmapped_chains_of(object).remove(address)
            })
        {
            return Ok(chain);
        }
        if let Some(unknown) = self.missed(&address.home) {
            return Err(unknown);
        }
        self.unheld()?;
        self.inner.claim_history(address).await
    }

    /// **Answered from the claims State**: the versions of one thing's own row.
    /// A child a merge moved is the store's, which holds the version the move
    /// appended. TEMPORARY(9.3.4) `q-934-merge-version-row-in-state`.
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.readable()?;
        match self.part_of(entity, |state, object, _| state.entity_versions_of(object)) {
            Some(versions) => Ok(versions),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.entity_history(entity).await
                }
            },
        }
    }

    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        self.readable()?;
        match self.part_of(&address.home, |state, object, _| {
            state.keys_kept_off(object, &address.local)
        }) {
            Some(kept) => Ok(kept),
            None => match self.missed(&address.home) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.keys_kept_off_the_thing(address).await
                }
            },
        }
    }

    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.readable()?;
        match self.part_of(entity, |state, object, _| state.key_trails_served(object)) {
            Some(trails) => Ok(trails),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.key_trails(entity).await
                }
            },
        }
    }

    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.readable()?;
        match self.part_of(entity, |state, object, _| state.claim_chains_of(object)) {
            Some(histories) => Ok(histories),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.claim_histories(entity).await
                }
            },
        }
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        owned!(
            self,
            this,
            [address = address.clone(), caller = caller.clone()],
            this.update_fact(&address, patch, &caller).await
        );
        self.scoped(async {
            match self.inner.update_fact(address, patch, caller).await? {
                Guarded::Written(fact) => match self.refresh_claims(&fact.home).await {
                    Ok(()) => Ok(Guarded::Written(fact)),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            Some(state.fields_served(object, declared))
        });
        match stated {
            Some(fields) => Ok(fields),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.fields(entity).await
                }
            },
        }
    }

    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            Some((
                state.fields_served(object, declared),
                state.write_marker(object),
            ))
        });
        match stated {
            Some(versioned) => Ok(versioned),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.fields_versioned(entity).await
                }
            },
        }
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        self.readable()?;
        match self.part_of(entity, |state, object, declared| {
            state.field_ages_of(object, declared)
        }) {
            Some(ages) => Ok(ages),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.field_ages(entity).await
                }
            },
        }
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        owned!(
            self,
            this,
            [
                address = address.clone(),
                reason = reason.map(str::to_string),
                caller = caller.clone(),
                session = session.map(str::to_string)
            ],
            this.retract(
                &address,
                reason.as_deref(),
                date,
                &caller,
                session.as_deref(),
            )
            .await
        );
        self.scoped(async {
            let taken_back = self
                .inner
                .retract(address, reason, date, caller, session)
                .await?;
            match self.refresh_claims(&taken_back.retracted.home).await {
                Ok(()) => Ok(taken_back),
                Err(source) => Err(fold_behind(
                    Landed::Retraction(Box::new(taken_back)),
                    source,
                )),
            }
        })
        .await
    }

    /// **An edit that writes a claim refreshes the image for the thing, as an edit
    /// does.** Forwarded and not left to the port's default, which refuses.
    async fn update_fact_with_claim(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
        claim: NewFact,
    ) -> Result<Guarded<(Fact, Fact)>, MemoryError> {
        owned!(
            self,
            this,
            [address = address.clone(), caller = caller.clone()],
            this.update_fact_with_claim(&address, patch, &caller, claim)
                .await
        );
        self.scoped(Box::pin(async {
            match self
                .inner
                .update_fact_with_claim(address, patch, caller, claim)
                .await?
            {
                Guarded::Written((edited, fact)) => match self.refresh_claims(&edited.home).await {
                    Ok(()) => Ok(Guarded::Written((edited, fact))),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(edited)), source)),
                },
                blocked => Ok(blocked),
            }
        }))
        .await
    }

    /// **A retraction that writes a claim refreshes the image for the thing, as a
    /// retraction does.** Forwarded and not left to the port's default, which
    /// refuses.
    async fn retract_with_claim(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
        claim: NewFact,
    ) -> Result<Guarded<(Retraction, Fact)>, MemoryError> {
        owned!(
            self,
            this,
            [
                address = address.clone(),
                reason = reason.map(str::to_string),
                caller = caller.clone(),
                session = session.map(str::to_string)
            ],
            this.retract_with_claim(
                &address,
                reason.as_deref(),
                date,
                &caller,
                session.as_deref(),
                claim,
            )
            .await
        );
        self.scoped(Box::pin(async {
            match self
                .inner
                .retract_with_claim(address, reason, date, caller, session, claim)
                .await?
            {
                Guarded::Written((taken_back, fact)) => {
                    match self.refresh_claims(&taken_back.retracted.home).await {
                        Ok(()) => Ok(Guarded::Written((taken_back, fact))),
                        Err(source) => Err(fold_behind(
                            Landed::Retraction(Box::new(taken_back)),
                            source,
                        )),
                    }
                }
                blocked => Ok(blocked),
            }
        }))
        .await
    }

    /// **The documents that spell either handle are read again.** The folded
    /// thing's claims are served from the survivor (see [`Memory::merge`]), and
    /// every record that named the folded thing now reads as naming the survivor,
    /// so the two ends and every holder in between are read again, and no other
    /// document.
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        owned!(
            self,
            this,
            [
                folded = folded.clone(),
                survivor = survivor.clone(),
                reason = reason.map(str::to_string),
                caller = caller.clone(),
                session = session.map(str::to_string)
            ],
            this.merge(
                &folded,
                &survivor,
                reason.as_deref(),
                date,
                &caller,
                session.as_deref(),
            )
            .await
        );
        self.scoped(async {
            let done = self
                .inner
                .merge(folded, survivor, reason, date, caller, session)
                .await?;
            match self.followed([folded, survivor]).await {
                Ok(()) => Ok(done),
                Err(source) => {
                    // The index re-reads both things itself when the fold could
                    // not follow the merge.
                    self.publish_things(vec![folded.clone(), survivor.clone()], true);
                    Err(fold_behind(Landed::Merge(Box::new(done)), source))
                }
            }
        })
        .await
    }

    /// **A merge that writes a claim reads the documents again as a merge does.**
    /// Forwarded and not left to the port's default, which refuses.
    async fn merge_with_claim(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        claim: NewFact,
    ) -> Result<Guarded<(Merge, Fact)>, MemoryError> {
        owned!(
            self,
            this,
            [
                folded = folded.clone(),
                survivor = survivor.clone(),
                reason = reason.map(str::to_string),
                caller = caller.clone()
            ],
            this.merge_with_claim(&folded, &survivor, reason.as_deref(), date, &caller, claim)
                .await
        );
        self.scoped(Box::pin(async {
            match self
                .inner
                .merge_with_claim(folded, survivor, reason, date, caller, claim)
                .await?
            {
                Guarded::Written((done, fact)) => match self.followed([folded, survivor]).await {
                    Ok(()) => Ok(Guarded::Written((done, fact))),
                    Err(source) => {
                        self.publish_things(vec![folded.clone(), survivor.clone()], true);
                        Err(fold_behind(Landed::Merge(Box::new(done)), source))
                    }
                },
                blocked => Ok(blocked),
            }
        }))
        .await
    }

    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        owned!(
            self,
            this,
            [
                entity = entity.clone(),
                prose = prose.to_string(),
                session = session.map(str::to_string)
            ],
            this.set_prose(&entity, &prose, session.as_deref()).await
        );
        self.scoped(async {
            let stored = self.inner.set_prose(entity, prose, session).await?;
            Folded::follow("set_prose", self.refresh(entity).await);
            Ok(stored)
        })
        .await
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.readable()?;
        // **Each listed thing's document is its own scan**, built from the State
        // and not held; a thing the State declines is read from the store, as its
        // own scan reads it. TEMPORARY(9.3.4) `q-934-merge-version-row-in-state` and
        // `q-934-role-lease-read-from-heartbeat-home`: **the loss is that a scan which
        // lists such a thing is refused whole while the rail is uncertain**, since the
        // one read for it may not cross the rail (pinned by
        // `a_scan_listing_a_declined_thing_is_refused_whole_while_the_rail_is_uncertain`).
        let listed = match self.held(|_| ()) {
            Some(()) => match self.claims(|state| state.stored_listing()) {
                Ok(listed) => Some(listed),
                Err(_) => {
                    self.unheld()?;
                    None
                }
            },
            None => None,
        };
        let Some(listed) = listed else {
            return self.inner.scan().await;
        };
        // **Most things declined: one read of the store for the lot.** Before the
        // ledgers open the State declines every thing, and a read of the store for
        // each made a boot last minutes. A read of the whole store costs about one
        // read of a thing a row (measured over a real store), so it is cheaper than
        // reading each declined thing only when nearly all of them are declined;
        // with a few declined (a role, a thing a merge touched) each is read on its
        // own, and a row the State serves is never read from the store.
        let slots: Vec<Result<Option<DocScan>, &EntityId>> = listed
            .iter()
            .map(|entity| match self.scan_from_state(&entity.id) {
                Some(doc) => Ok(doc),
                None if self.missed(&entity.id).is_some() => Ok(None),
                None => Err(&entity.id),
            })
            .collect();
        let declined = slots.iter().filter(|slot| slot.is_err()).count();
        let started = std::time::Instant::now();
        // **A whole read that fails reads nothing**: it builds every document, one the
        // State serves included, so an unreadable row elsewhere in the store would
        // otherwise fail a scan that needs only the declined things. Each of those is
        // then read on its own, and only an unreadable declined thing fails the scan.
        let mut from_store = HashMap::new();
        if declined * 6 > slots.len() * 5 {
            self.unheld()?;
            match self.inner.scan().await {
                Ok(whole) => {
                    from_store = whole
                        .into_iter()
                        .filter_map(|doc| {
                            let id = doc.entity.as_ref()?.id.clone();
                            Some((id, doc))
                        })
                        .collect();
                }
                Err(error) => tracing::warn!(
                    %error,
                    declined,
                    "A WHOLE READ OF THE STORE FAILED for a scan, so each declined thing is read \
                     on its own, which costs a read of the store for each."
                ),
            }
        }
        let mut scanned = Vec::with_capacity(slots.len());
        for slot in slots {
            let doc = match slot {
                Ok(doc) => doc,
                Err(id) => match from_store.remove(id) {
                    Some(doc) => Some(doc),
                    None => self.scan_entity(id).await?,
                },
            };
            scanned.extend(doc);
        }
        if declined > 0 {
            tracing::info!(
                listed = scanned.len(),
                declined,
                elapsed_ms = started.elapsed().as_millis() as u64,
                "fold scan: the things the State declines were read from the store"
            );
        }
        Ok(scanned)
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.readable()?;
        let stated = self.scan_from_state(entity);
        match stated {
            Some(doc) => Ok(doc),
            None if self.missed(entity).is_some() => Ok(None),
            None => {
                self.unheld()?;
                self.inner.scan_entity(entity).await
            }
        }
    }

    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, _declared| {
            if found.id != *entity {
                return Some(false);
            }
            let object = state.resolve(&found.id.0)?;
            Some(state.has_prose(object))
        });
        match stated {
            Some(has) => Ok(has),
            None if self.missed(entity).is_some() => Ok(false),
            None => {
                self.unheld()?;
                self.inner.has_prose(entity).await
            }
        }
    }

    /// **Forwarded, and no read of a stored row.** The layer beneath joins the
    /// prose the build ships to `own`, and a handle the build ships nothing for
    /// gets the store's default, `own` trimmed. There is no stored row for the
    /// image to hold.
    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.inner.composed_prose(entity, own).await
    }

    /// **The change signal, answered from memory.** The number of times the
    /// image has changed stands in for the counts and the hashes of the tables:
    /// jojobot is the only writer, so every change to what a read answers goes
    /// through the image. A row changed past this process is seen after a
    /// restart, as the image itself is. With no image held the store answers.
    /// **The entities a set of texts names, answered from the claims State by key.**
    /// Names, badges, merges and numbering are logged events, so this read has no decline
    /// and never reaches the store; with no image held it answers `None` and the caller
    /// reads the listing, as before.
    async fn names(
        &self,
        wanted: &jojobot_domain::memory::Wanted,
    ) -> Result<Option<jojobot_domain::memory::Names>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_none() {
            return Ok(None);
        }
        match self.claims(|state| state.names_read(wanted)) {
            Ok(names) => Ok(Some(names)),
            Err(_) => {
                self.unheld()?;
                Ok(None)
            }
        }
    }

    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.readable()?;
        let held = {
            let slot = self.filled.read().expect("fold lock");
            slot.as_ref()
                .map(|_| self.generation.load(Ordering::SeqCst) as i64)
        };
        match held {
            Some(changes) => Ok(Some(WriteSummary {
                entities: (changes, None),
                facts: (changes, None),
                entity_hash: None,
                fact_hash: None,
            })),
            None => self.inner.write_summary().await,
        }
    }

    /// **Forwarded, and no read of a stored row.** Only `Provisioned` beneath
    /// this layer holds what the build ships, and the comparison is made against
    /// that, so a default body here would answer "no echoes" for every write in
    /// production.
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.inner.echoed_defaults(entity, fields).await
    }

    /// **Held as the store answered it.** A left-over default here would be
    /// served in production over the store's own read.
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, declared| {
            let object = state.resolve(&found.id.0)?;
            Some(state.backing(object, declared))
        });
        match stated {
            Some(backing) => Ok(backing),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => {
                    self.unheld()?;
                    self.inner.backing(entity).await
                }
            },
        }
    }

    /// **Selected on the pointer in memory**, not by reading every record.
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.readable()?;
        // An address on a thing merged away is moved first, when the merge kept
        // an event.
        match self.moved_by_state(source) {
            Some(Err(looped)) => return Err(looped.into()),
            Some(Ok(Some(moved))) => {
                return self.built_on_here(&moved).await;
            }
            _ => {}
        }
        self.built_on_here(source).await
    }

    /// **Selected on what a record's fields name, from the claims State**, not by
    /// reading every record. The kind of the target is checked against the store's
    /// own kinds. With no image, or no State to read, the store answers.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.readable()?;
        let Some(kinds) = self.held_kind_set() else {
            return self.inner.referring_to(target).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, target)?;
        match self.claims(|state| state.referring(target)) {
            Ok(mine) => Ok(mine),
            Err(_) => self.inner.referring_to(target).await,
        }
    }

    /// **Straight through**: the width of a read is the inner store's.
    fn read_width(&self) -> usize {
        self.inner.read_width()
    }
    /// **Straight through**: the State is the inner store's.
    fn claims_lender(&self) -> Option<Arc<dyn jojobot_domain::log::claims::ClaimsLender>> {
        self.inner.claims_lender()
    }
    /// **Answered from the claims State**, which holds each bot's records with
    /// their edges. The store's own check inside a ceiling write is its own and
    /// does not pass here.
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.readable()?;
        let Some(kinds) = self.held_kind_set() else {
            return self.inner.thought_writers(thread).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, thread)?;
        match self.claims(|state| state.thought_writers(thread)) {
            Ok(writers) => Ok(writers),
            Err(_) => {
                self.unheld()?;
                self.inner.thought_writers(thread).await
            }
        }
    }

    /// **Answered from the image**, which keeps what each key holds by value.
    async fn keys_holding(
        &self,
        key: &str,
        test: &jojobot_domain::memory::filter::Test,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_none() {
            return self.inner.keys_holding(key, test, kind).await;
        }
        match self.claims(|state| {
            (
                state.keys_holding(key, test, kind),
                state.declined_among(kind),
            )
        }) {
            Ok((mut held, declined)) => {
                // **A thing the State declines is read from the store**, by the one
                // predicate every read of the fold shares: a role, whose plain renewal
                // overwrites the lease moment without an event, and a thing a merge
                // touched, which may carry a role's value onto the survivor. Each is read
                // through the fields the fold serves for it, which come from the store and
                // refuse while the rail is uncertain. The State answers for every other
                // thing. TEMPORARY(9.3.4) `q-934-role-lease-read-from-heartbeat-home` and
                // `q-934-merge-version-row-in-state`, which end the declines.
                if !declined.is_empty() {
                    let leaf = jojobot_domain::memory::filter::Filter::leaf(
                        Some(key.trim()),
                        test.clone(),
                    );
                    held.retain(|id| !declined.contains(id));
                    for id in declined {
                        if leaf.holds(&self.fields(&id).await?) {
                            held.push(id);
                        }
                    }
                    held.sort();
                }
                Ok(held)
            }
            Err(_) => {
                self.unheld()?;
                self.inner.keys_holding(key, test, kind).await
            }
        }
    }

    /// **Answered from the image**, which keeps the values of a date or a number
    /// key in order.
    async fn ordered_first_k(
        &self,
        key: &str,
        holds: types::ValueType,
        direction: jojobot_domain::memory::filter::Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> Result<jojobot_domain::memory::holders::OrderedHead, MemoryError> {
        self.readable()?;
        let inner = || {
            self.inner
                .ordered_first_k(key, holds, direction, kind, among, k)
        };
        if self.held(|_| ()).is_none() {
            return inner().await;
        }
        match self.claims(|state| state.ordered_first_k(key, holds, direction, kind, among, k)) {
            Ok(head) => Ok(head),
            Err(_) => inner().await,
        }
    }

    /// **A declaration can change how EVERY entity's existing writes fold**
    /// — `newest` against a counter — so there is no one handle to refresh
    /// the way a capture refreshes its own home. The whole image is read
    /// again. The write already landed; only the re-read is what this can
    /// fail to report.
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        owned!(
            self,
            this,
            [by_session = by_session.map(str::to_string)],
            this.declare_type(declared, by_session.as_deref()).await
        );
        self.scoped(async {
            // **A declaration the State already holds, word for word, changes no
            // read.** A boot declares every type the build ships, so on a store that
            // has them each of those must not move the generation.
            let held_already = self
                .claims(|state| state.declared_types_shared().contains(&declared))
                .unwrap_or(false);
            let written = self.inner.declare_type(declared, by_session).await?;
            // **A declaration reads nothing of the store**: the claims State holds
            // the types and builds what a thing holds under them when a read asks.
            if !held_already {
                self.generation.fetch_add(1, Ordering::SeqCst);
            }
            Ok(written)
        })
        .await
    }

    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        Ok(self.declared_types_shared().await?.as_ref().clone())
    }

    /// **The declarations in force, from the claims State's own shared list**, which a
    /// declaration rebuilds and a read only clones the pointer of: the declarations
    /// have one home in memory. With no image, or no State to read, the store
    /// answers.
    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<DeclaredType>>, MemoryError> {
        self.readable()?;
        if self.held(|_| ()).is_some()
            && let Ok(declared) = self.claims(|state| state.declared_types_shared())
        {
            return Ok(declared);
        }
        Ok(std::sync::Arc::new(self.inner.declared_types().await?))
    }

    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.unheld()?;
        self.inner.displaced_type(name).await
    }

    /// **The same reason [`declare_type`](Self::declare_type) reloads rather than
    /// refreshes**: a kind's keys fold exactly like a declared type's, over
    /// every entity that answers to it.
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        owned!(
            self,
            this,
            [token = token.to_string()],
            this.declare_kind(&token, origin, fields).await
        );
        self.scoped(async {
            let changed = self.inner.declare_kind(token, origin, fields).await?;
            // **The image is read again only when the store holds something
            // different.** The seed declares every shipped kind at every start, and
            // a declaration that left the store as it was folds nothing differently.
            if changed {
                Self::follow("declare_kind", self.refresh_kinds().await);
            }
            Ok(changed)
        })
        .await
    }

    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.readable()?;
        let held = self.held(|_| ()).and_then(|()| {
            self.kinds
                .read()
                .expect("kinds lock")
                .as_ref()
                .map(|held| held.rows.clone())
        });
        match held {
            Some(kinds) => Ok(kinds),
            None => self.inner.declared_kinds().await,
        }
    }

    /// **Taking a kind back changes folding too**: a key that summed while
    /// the kind held it falls back to newest-write-wins once reclaimed, for
    /// every entity that answered to it.
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        owned!(
            self,
            this,
            [token = token.to_string()],
            this.reclaim_kind(&token).await
        );
        self.scoped(async {
            self.inner.reclaim_kind(token).await?;
            Self::follow("reclaim_kind", self.refresh_kinds().await);
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod built_on_order_tests;
#[cfg(test)]
mod counting_scan;
#[cfg(test)]
mod from_state_tests;
#[cfg(test)]
mod kind_tests;
#[cfg(test)]
mod landed_tests;
#[cfg(test)]
mod refresh_from_state_tests;
#[cfg(test)]
mod stalled_read;
#[cfg(test)]
mod state_lists_tests;
#[cfg(test)]
mod supplied_over_the_fold_tests;
#[cfg(test)]
mod uncertain_serving_tests;

#[cfg(test)]
mod tests;
