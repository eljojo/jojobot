//! [`Folded`] — everything the memory verbs read, held in RAM above the store
//! (decision log 292).
//!
//! jojobot is one server instance and the only writer, so it holds the whole
//! store in memory and answers a read from there. [`Folded`] wraps a
//! [`Memory`] and, once [`rebuild`](Folded::rebuild) has run, serves the reads
//! named below from an [`Image`](image::Image) filled by the store's own reads:
//! the listing and the rename history, the declared types and kinds, and for
//! every thing its records, fields, ages and claim histories. Every write goes
//! to the store first and then re-reads what it touched into the image under
//! the same writer task, so a caller who writes and reads sees the write.
//!
//! **The image is filled from the store's answers, never from arithmetic of its
//! own.** `Memory::fields` folds a thing's field writes down to one value per
//! key — the newest write wins, an older one stands back up when the write that
//! beat it is retracted — and the fold is correct wherever it runs. A held copy
//! of that answer is the answer; a second copy of the fold would be one more
//! thing to disagree with the first.
//!
//! Before the first `rebuild`, and after a refresh that failed, no image is
//! held and every read goes to the store underneath: slower and still correct.
//!
//! ⚠️ **This is only correct with exactly one writer in the process.** The image
//! is filled from writes THIS `Folded` observed; a row changed by anything else
//! — another worker, a restored backup, a migration tool, a person running SQL
//! by hand — leaves it silently wrong, and nothing here would notice. jojobot
//! runs one application worker against its store today, which is the whole
//! reason this is authoritative rather than a guess. Add a second writer and
//! this assumption breaks first.

pub mod claims_view;
mod image;
pub mod mail;
pub mod sessions;
pub mod shadow;
mod uncertain;

use crate::writer::owned;
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use jiff::civil::Date;

use image::{Doc, Image};
use jojobot_domain::memory::boot_reader::BootReader;
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldAges, FieldBacking, FieldWrite, FormerHandle, Guarded, Landed, Memory,
    MemoryError, Merge, NewEntity, NewFact, Retraction, WriteSummary,
    search::{self, DocScan},
    types::{self, DeclaredType},
};

/// A [`Memory`] wrapped in a RAM image of everything its reads answer.
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
    /// What the fill asks for many things at once, when the store has it. Used by
    /// the fill alone: no read reaches it.
    boot_reader: Option<Arc<dyn BootReader>>,
    /// The image, once a [`rebuild`](Folded::rebuild) has filled it; `None`
    /// sends every read to the store.
    image: Arc<RwLock<Option<Image>>>,
    /// Set by the first `rebuild`, whether or not its fill succeeded. A fill that
    /// failed, or a refresh that failed, leaves no image, and this is what tells
    /// the next write to fill it again.
    wanted: Arc<AtomicBool>,
    /// **Set while the boot's repair passes write, before the claims import.** The
    /// log holds no State then, so a refresh after such a write reads the store, as
    /// every refresh did. TEMPORARY(9.3.3.1) `q-9331-no-memory-image`: the passes
    /// go when their data is read from the log.
    before_the_import: Arc<AtomicBool>,
    /// **How many times the image has changed**, bumped under the same lock
    /// that installs the change, so a reader that sees a number sees at least the
    /// state it counts. It is the change signal search reads ([`write_summary`]),
    /// answered from memory.
    ///
    /// [`write_summary`]: Memory::write_summary
    generation: Arc<AtomicU64>,
    /// The projection families whose reads are computed two ways and compared
    /// ([`shadow`]); none unless the configuration names one.
    shadow: Arc<shadow::Shadow>,
    /// Told the address of the next selected fact edit, before it queues for the
    /// writer.
    #[cfg(any(test, feature = "testing"))]
    fact_edit: Arc<FactEditObserver>,
}

#[cfg(any(test, feature = "testing"))]
type FactEditObserver =
    std::sync::Mutex<Option<tokio::sync::oneshot::Sender<jojobot_domain::memory::FactAddress>>>;

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
            boot_reader: None,
            image: Arc::new(RwLock::new(None)),
            wanted: Arc::new(AtomicBool::new(false)),
            before_the_import: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
            shadow: Arc::new(shadow::Shadow::default()),
            #[cfg(any(test, feature = "testing"))]
            fact_edit: Arc::new(std::sync::Mutex::new(None)),
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

    /// Observe the next selected fact edit before it waits behind a writer.
    /// Race fixtures can release that writer after selection, without a sleep.
    #[cfg(any(test, feature = "testing"))]
    pub fn signals_the_next_fact_edit(
        &self,
    ) -> tokio::sync::oneshot::Receiver<jojobot_domain::memory::FactAddress> {
        let (send, receive) = tokio::sync::oneshot::channel();
        *self.fact_edit.lock().expect("the observer is not poisoned") = Some(send);
        receive
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

    /// **Give the fill a reader that answers for many things at once.** The
    /// fill takes what it holds for a thing and reads anything else of that thing
    /// as it always did, so a reader that answers for none changes nothing.
    pub fn with_boot_reader(mut self, reader: Arc<dyn BootReader>) -> Self {
        self.boot_reader = Some(reader);
        self
    }

    /// **Shadow the families a configuration value names** (see [`shadow`]), and
    /// refuse a name no family answers to.
    pub fn shadowing(mut self, families: &str) -> Result<Self, String> {
        self.shadow = Arc::new(shadow::Shadow::named(families)?);
        Ok(self)
    }

    /// The shadow the serving rails report their comparisons to.
    pub fn shadow(&self) -> Arc<shadow::Shadow> {
        self.shadow.clone()
    }

    /// What each shadowed family has counted.
    pub fn shadow_counts(&self) -> Vec<shadow::Counts> {
        self.shadow.counts()
    }

    /// Fill the image from a full read of the store — the boot path. Returns
    /// how many things were held.
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
                // failed leaves no image, and the next write fills it again.
                this.wanted.store(true, Ordering::SeqCst);
                this.fill().await
            })
            .await
    }

    /// The read behind [`rebuild`](Folded::rebuild), for a caller that already
    /// runs in the writer task. A failure leaves no image rather than a half one.
    async fn fill(&self) -> Result<usize, MemoryError> {
        match crate::writer::sql::reading(Box::pin(Image::load(
            &self.inner,
            self.boot_reader.as_deref(),
        )))
        .await
        {
            Ok(image) => {
                let held = image.len();
                {
                    let mut slot = self.image.write().expect("fold lock");
                    *slot = Some(image);
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                self.publish_things(Vec::new(), true);
                Ok(held)
            }
            Err(e) => {
                if !self.writer.rail_uncertain("memory") {
                    let mut slot = self.image.write().expect("fold lock");
                    *slot = None;
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                self.publish_things(Vec::new(), true);
                Err(e)
            }
        }
    }

    /// Answer from the image when one is held; `None` sends the read to the
    /// store.
    fn held<T>(&self, read: impl FnOnce(&Image) -> T) -> Option<T> {
        self.image.read().expect("fold lock").as_ref().map(read)
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
        // **The thing's document, and the listing after an entity write, are built
        // from the claims State this write has just changed.** A State that cannot
        // be had fails the refresh ([`Folded::without_state`]). Three things are
        // read from the store, each a named decline and not a missing State:
        // - a thing a merge touched: TEMPORARY(9.3.4), owner card
        //   `q-934-merge-version-row-in-state`, the merge's own row. The store
        //   lists that row as a version of the folded handle and the State holds
        //   no version for it yet, so the entity history of a merge survivor and
        //   the document of a folded handle come from the store;
        // - a role: its lease renewals are heartbeats kept off the log, so the State
        //   does not hold what a read of it serves: TEMPORARY(9.3.4), owner card
        //   `q-934-role-lease-read-from-heartbeat-home`;
        // - a record the State does not know, which is a record the build supplies
        //   and the overlay serves: TEMPORARY(9.3.5), until supplied records are
        //   supplied events.
        // **A thing the build supplies content at** is a named decline too: the
        // layer over the store lays that content on what the store holds, and the
        // State holds none of it. TEMPORARY(9.3.5), until supplied records are
        // supplied events.
        let from_the_store =
            self.before_the_import.load(Ordering::SeqCst) || self.inner.supplies_at(handle);
        let stated = if from_the_store {
            Ok(None)
        } else {
            self.claims(|state| {
                let object = state.resolve(&handle.0)?;
                if state.was_merged(object) || state.holds_a_lease(object) {
                    return None;
                }
                let doc = Doc::from_state(state, object, &state.declared_types_shared())?;
                Some((relist.then(|| state.listing()), doc))
            })
        };
        let read = async {
            let (entities, doc) = match stated {
                Err(unseen) => return Err(Self::without_state(unseen)),
                Ok(Some((entities, doc))) => {
                    // **A thing the State has never heard of is a record the build
                    // supplies**, and the State's listing leaves it out: the image
                    // keeps what it lists of those. TEMPORARY(9.3.5), until supplied
                    // records are supplied events.
                    let entities = match entities {
                        Some(mut listed) => {
                            let missing = self
                                .held(|image| image.entities_missing_from(&listed))
                                .unwrap_or_default();
                            let supplied = self
                                .claims(|state| {
                                    missing
                                        .into_iter()
                                        .filter(|entity| state.resolve(&entity.id.0).is_none())
                                        .collect::<Vec<_>>()
                                })
                                .map_err(Self::without_state)?;
                            listed.extend(supplied);
                            // **A record the build supplies is in the State's listing
                            // too, as the build ships it**: the operator's name and
                            // aliases are laid over it by the layer over the store and
                            // are in no State row. The write that brought this relist
                            // was about another thing (one about the record itself is
                            // read from the store above), so the row the image holds
                            // for it is still the one served. TEMPORARY(9.3.5), until
                            // supplied records are supplied events.
                            for held in self
                                .held(|image| image.held_rows(|id| self.inner.supplies_at(id)))
                                .unwrap_or_default()
                            {
                                if let Some(row) = listed.iter_mut().find(|row| row.id == held.id) {
                                    *row = held;
                                }
                            }
                            Some(listed)
                        }
                        None => None,
                    };
                    (entities, doc)
                }
                Ok(None) => {
                    let entities = if relist {
                        Some(self.inner.list_entities(None).await?)
                    } else {
                        None
                    };
                    (entities, Doc::load(&*self.inner, handle).await?)
                }
            };
            Ok::<_, MemoryError>((entities, doc))
        }
        .await;
        match read {
            Ok((entities, doc)) => {
                if let Some(image) = self.image.write().expect("fold lock").as_mut() {
                    image.install(entities, doc);
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                Ok(())
            }
            Err(e) => {
                let mut slot = self.image.write().expect("fold lock");
                *slot = None;
                self.generation.fetch_add(1, Ordering::SeqCst);
                Err(e)
            }
        }
    }

    /// Bring the image up to date with a write that may have touched any thing:
    /// a rename, a merge, a declaration. The whole image is read again.
    async fn reload(&self) -> Result<(), MemoryError> {
        if !self.wanted.load(Ordering::SeqCst) {
            return Ok(());
        }
        self.fill().await.map(|_| ())
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
        let Some(kinds) = self.held(|image| image.kind_set()) else {
            return self.inner.built_on(source).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, &source.home)?;
        match self.claims(|state| state.built_on(source)) {
            Ok(facts) => Ok(facts),
            Err(_) => self.inner.built_on(source).await,
        }
    }

    /// **One thing's read from the claims State**, the thing a spelling answers to:
    /// the handle it wears now, one it wore, or a number under a collection. `None`
    /// sends the read to the store, which answers it as it always did: no image or
    /// no State, a spelling nothing answers to, and the three declines the State
    /// does not serve. TEMPORARY(9.3.4) `q-934-merge-version-row-in-state` for a
    /// thing a merge touched, TEMPORARY(9.3.4)
    /// `q-934-role-lease-read-from-heartbeat-home` for a role, and TEMPORARY(9.3.5)
    /// for a handle the build supplies content at.
    fn stated<T>(
        &self,
        id: &EntityId,
        read: impl FnOnce(&jojobot_domain::log::claims::State, &Entity, &[DeclaredType]) -> Option<T>,
    ) -> Option<T> {
        self.held(|_| ())?;
        if self.inner.supplies_at(id) {
            return None;
        }
        self.claims(|state| {
            let entity = state.resolved_entity(id)?;
            let object = state.resolve(&entity.id.0)?;
            if state.was_merged(object)
                || state.holds_a_lease(object)
                || self.inner.supplies_at(&entity.id)
            {
                return None;
            }
            read(state, &entity, &state.declared_types_shared())
        })
        .ok()
        .flatten()
    }

    /// **The miss of a read of a spelling nothing answers to**, made from the
    /// claims State, so a miss reaches no store either. `None` when the spelling
    /// answers to a thing, or there is no image or State to say it does not, or the
    /// build supplies content at it.
    fn missed(&self, id: &EntityId) -> Option<MemoryError> {
        self.held(|_| ())?;
        if self.inner.supplies_at(id) {
            return None;
        }
        self.claims(|state| {
            state
                .resolved_entity(id)
                .is_none()
                .then(|| state.unknown_entity(id))
        })
        .ok()
        .flatten()
    }

    fn readable(&self) -> Result<(), MemoryError> {
        if self.writer.rail_uncertain("memory") && self.image.read().expect("fold lock").is_none() {
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

#[async_trait]
impl Memory for Folded {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        self.held(|image| {
            image
                .doc(entity)
                .ok()
                .and_then(|doc| doc.fields.get(key).cloned())
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
        match self.held(|image| image.list(kind)) {
            Some(listing) => Ok(listing),
            None => self.inner.list_entities(kind).await,
        }
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.readable()?;
        match self.held(Image::former) {
            Some(former) => Ok(former),
            None => self.inner.former_handles().await,
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
    /// no other (see [`image::patch`]).
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
                let patch = image::patch::Change::Renamed {
                    from,
                    to: &entity.id,
                };
                let followed = self.patched_or_reloaded(patch).await;
                // **An image that could not follow still tells the index which
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
                None => self.inner.recall(subject).await,
            },
        }
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.history(entity, key)) {
            Some(answer) => answer,
            None => self.inner.history(entity, key).await,
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
            Err(_) => self.inner.merge_events(entity).await,
        }
    }

    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.moved(address)) {
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
        let stated = self.stated(entity, |state, found, declared| {
            if found.id != *entity {
                return Some(Vec::new());
            }
            let object = state.resolve(&found.id.0)?;
            Some(state.document_scan(object, declared)?.unmapped)
        });
        match stated {
            Some(unmapped) => Ok(unmapped),
            None if self.missed(entity).is_some() => Ok(Vec::new()),
            None => self.inner.unmapped_claims(entity).await,
        }
    }

    /// **An address from before a merge is moved by the image's own copy of the
    /// events**, so a claim read costs the store nothing: the image names no
    /// claim of a folded thing, and the event's map says where the claim went.
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.claim_history(address)) {
            Some(Err(merged @ MemoryError::AlreadyMerged { .. })) => {
                match self.held(|image| image.moved(address)) {
                    Some(Err(looped)) => Err(looped.into()),
                    Some(Ok(Some(moved))) => match self.held(|image| image.claim_history(&moved)) {
                        Some(answer) => answer,
                        None => {
                            self.unheld()?;
                            self.inner.claim_history(address).await
                        }
                    },
                    // **A claim the survivor names as one it cannot serve** is
                    // answered from the chain the image read when it filled the
                    // survivor's document. A chain that could not be read then is
                    // read where it is stored.
                    _ if self.held(|image| image.names_as_unmapped(address)) == Some(true) => {
                        match self.held(|image| image.unmapped_history(address)) {
                            Some(Some(chain)) => Ok(chain),
                            _ => {
                                self.unheld()?;
                                self.inner.claim_history(address).await
                            }
                        }
                    }
                    _ => Err(merged),
                }
            }
            Some(answer) => answer,
            None => {
                self.unheld()?;
                self.inner.claim_history(address).await
            }
        }
    }

    /// **Answered from the image**, which holds every version of each thing's own
    /// row beside its claims, read when the document is filled and again after
    /// each write that refreshes it. A thing whose versions the store could not
    /// answer at the fill is read where it is stored, as is every read before the
    /// first fill.
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.entity_history(entity)) {
            Some(Ok(Some(versions))) => Ok(versions),
            Some(Err(unknown)) => Err(unknown),
            Some(Ok(None)) | None => {
                self.unheld()?;
                self.inner.entity_history(entity).await
            }
        }
    }

    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.kept_off(address)) {
            Some(answer) => answer,
            None => self.inner.keys_kept_off_the_thing(address).await,
        }
    }

    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.readable()?;
        match self.held(|image| image.trails(entity)) {
            Some(answer) => answer,
            None => self.inner.key_trails(entity).await,
        }
    }

    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.readable()?;
        match self.held(|image| image.doc(entity).map(|doc| doc.histories.clone())) {
            Some(answer) => answer,
            None => self.inner.claim_histories(entity).await,
        }
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        #[cfg(any(test, feature = "testing"))]
        if let Some(reached) = self
            .fact_edit
            .lock()
            .expect("the observer is not poisoned")
            .take()
        {
            let _ = reached.send(address.clone());
        }
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
            Some(state.scan(object, declared)?.fields)
        });
        match stated {
            Some(fields) => Ok(fields),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => self.inner.fields(entity).await,
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
                state.scan(object, declared)?.fields,
                state.write_marker(object),
            ))
        });
        match stated {
            Some(versioned) => Ok(versioned),
            None => match self.missed(entity) {
                Some(unknown) => Err(unknown),
                None => self.inner.fields_versioned(entity).await,
            },
        }
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        self.readable()?;
        match self.held(|image| image.doc(entity).map(|doc| doc.ages.clone())) {
            Some(answer) => answer,
            None => self.inner.field_ages(entity).await,
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
    /// document (see [`image::patch`]).
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
            let patch = image::patch::Change::Merged { folded, survivor };
            match self.patched_or_reloaded(patch).await {
                Ok(()) => Ok(done),
                Err(source) => {
                    // The index re-reads both things itself when the image could
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
                Guarded::Written((done, fact)) => {
                    let patch = image::patch::Change::Merged { folded, survivor };
                    match self.patched_or_reloaded(patch).await {
                        Ok(()) => Ok(Guarded::Written((done, fact))),
                        Err(source) => {
                            self.publish_things(vec![folded.clone(), survivor.clone()], true);
                            Err(fold_behind(Landed::Merge(Box::new(done)), source))
                        }
                    }
                }
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
        match self.held(Image::scan) {
            Some(scanned) => Ok(scanned),
            None => self.inner.scan().await,
        }
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, declared| {
            if found.id != *entity {
                return Some(None);
            }
            let object = state.resolve(&found.id.0)?;
            Some(Some(state.document_scan(object, declared)?))
        });
        match stated {
            Some(doc) => Ok(doc),
            None if self.missed(entity).is_some() => Ok(None),
            None => self.inner.scan_entity(entity).await,
        }
    }

    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        self.readable()?;
        let stated = self.stated(entity, |state, found, declared| {
            if found.id != *entity {
                return Some(false);
            }
            let object = state.resolve(&found.id.0)?;
            let doc = state.document_scan(object, declared)?;
            Some(!doc.prose.trim().is_empty())
        });
        match stated {
            Some(has) => Ok(has),
            None if self.missed(entity).is_some() => Ok(false),
            None => self.inner.has_prose(entity).await,
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
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.readable()?;
        let held = {
            let slot = self.image.read().expect("fold lock");
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
                None => self.inner.backing(entity).await,
            },
        }
    }

    /// **Selected on the pointer in memory**, not by reading every record.
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.readable()?;
        // An address on a thing merged away is moved first, when the merge kept
        // an event.
        match self.held(|image| image.moved(source)) {
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
        let Some(kinds) = self.held(|image| image.kind_set()) else {
            return self.inner.referring_to(target).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, target)?;
        let Ok(mine) = self.claims(|state| state.referring(target)) else {
            return self.inner.referring_to(target).await;
        };
        // **Shadowed: the store is asked as well and the two are compared**; the
        // State's answer is the one served.
        if self.shadow.watches(shadow::Family::ReferringTo)
            && !self.writer.rail_uncertain("memory")
            && let Ok(theirs) = self.inner.referring_to(target).await
        {
            self.shadow.compared_referring_to(target, &mine, &theirs);
        }
        Ok(mine)
    }

    /// **Straight through**: the width of a read is the inner store's.
    fn read_width(&self) -> usize {
        self.inner.read_width()
    }
    /// **Straight through**: the State is the inner store's.
    fn claims_lender(&self) -> Option<Arc<dyn jojobot_domain::log::claims::ClaimsLender>> {
        self.inner.claims_lender()
    }
    fn supplies_at(&self, handle: &EntityId) -> bool {
        self.inner.supplies_at(handle)
    }
    /// **Answered from the claims State**, which holds each bot's records with
    /// their edges. The store's own check inside a ceiling write is its own and
    /// does not pass here.
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.readable()?;
        let Some(kinds) = self.held(|image| image.kind_set()) else {
            return self.inner.thought_writers(thread).await;
        };
        jojobot_domain::memory::validate_subject_in(&kinds, thread)?;
        match self.claims(|state| state.thought_writers(thread, &state.declared_types_shared())) {
            Ok(writers) => Ok(writers),
            Err(_) => self.inner.thought_writers(thread).await,
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
        match self.claims(|state| state.keys_holding(key, test, kind)) {
            Ok(held) => Ok(held),
            Err(_) => self.inner.keys_holding(key, test, kind).await,
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
            let written = self.inner.declare_type(declared, by_session).await?;
            // **A declaration the image already holds, word for word, folds nothing
            // differently.** A boot declares every type the build ships, so on a
            // store that has them each of those would read the whole image again.
            if self
                .held(|image| image.types().contains(&written))
                .unwrap_or(false)
            {
                return Ok(written);
            }
            // **A declaration that changes what no thing held reads as needs no
            // read of the store**: the image takes the type in and builds what it
            // keeps by key again from the documents it holds. A fold that moves
            // for a key something holds, or a reference or ordered type given to
            // one, changes those documents, so that case reads the image again.
            let patched = match self.image.write().expect("fold lock").as_mut() {
                Some(image) if !image.declaration_changes_a_read(&written) => {
                    image.declare(written.clone());
                    self.generation.fetch_add(1, Ordering::SeqCst);
                    true
                }
                _ => false,
            };
            if !patched {
                Self::follow("declare_type", self.reload().await);
            }
            Ok(written)
        })
        .await
    }

    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        Ok(self.declared_types_shared().await?.as_ref().clone())
    }

    /// **The declarations in force, from the claims State's own shared list**, which a
    /// declaration rebuilds and a read only clones the pointer of. With no image, or
    /// no State to read, the store answers.
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
                Self::follow("declare_kind", self.reload().await);
            }
            Ok(changed)
        })
        .await
    }

    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.readable()?;
        match self.held(Image::kinds) {
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
            Self::follow("reclaim_kind", self.reload().await);
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
mod tests;
