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
//! the same lock, so a caller who writes and reads sees the write.
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

mod held;
mod image;
pub mod mail;
pub mod sessions;
pub mod teachings;
mod uncertain;

use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use jiff::civil::Date;

use image::{Doc, Image};
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
pub struct Folded {
    inner: Arc<dyn Memory>,
    /// The image, once a [`rebuild`](Folded::rebuild) has filled it; `None`
    /// sends every read to the store.
    image: RwLock<Option<Image>>,
    /// **Held across a write and the re-read that follows it.** Two writes then
    /// reach the image in the order they reached the store, so a slower re-read
    /// of an earlier state cannot land after a faster one of a later state.
    writing: tokio::sync::Mutex<()>,
    /// Set by the first `rebuild`, whether or not its fill succeeded. A fill that
    /// failed, or a refresh that failed, leaves no image, and this is what tells
    /// the next write to fill it again.
    wanted: AtomicBool,
    /// **How many times the image has changed**, bumped under the same lock
    /// that installs the change, so a reader that sees a number sees at least the
    /// state it counts. It is the change signal search reads ([`write_summary`]),
    /// answered from memory.
    ///
    /// [`write_summary`]: Memory::write_summary
    generation: AtomicU64,
}

impl Folded {
    /// Wrap a store with **no image**. Nothing is read yet — the image is
    /// filled by [`rebuild`](Folded::rebuild), separately, so a store that is
    /// not reachable yet cannot stop the wrap itself from succeeding. Every
    /// read answers correctly before a rebuild: it goes to the store.
    pub fn new(inner: Arc<dyn Memory>) -> Self {
        Folded {
            inner,
            image: RwLock::new(None),
            writing: tokio::sync::Mutex::new(()),
            wanted: AtomicBool::new(false),
            generation: AtomicU64::new(0),
        }
    }

    /// Fill the image from a full read of the store — the boot path. Returns
    /// how many things were held.
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        let _gate = self.writing.lock().await;
        // **Wanted from the first attempt, whether or not it succeeds**: a fill that
        // failed leaves no image, and the next write fills it again.
        self.wanted.store(true, Ordering::SeqCst);
        self.fill().await
    }

    /// The read behind [`rebuild`](Folded::rebuild), for a caller that already
    /// holds the write gate. A failure leaves no image rather than a half one.
    async fn fill(&self) -> Result<usize, MemoryError> {
        match Image::load(&self.inner).await {
            Ok(image) => {
                let held = image.len();
                {
                    let mut slot = self.image.write().expect("fold lock");
                    *slot = Some(image);
                    self.generation.fetch_add(1, Ordering::SeqCst);
                }
                Ok(held)
            }
            Err(e) => {
                let mut slot = self.image.write().expect("fold lock");
                *slot = None;
                self.generation.fetch_add(1, Ordering::SeqCst);
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
        self.refresh_listing(handle, None).await
    }

    /// [`refresh`](Self::refresh) for a write that cannot have changed any
    /// thing's own row: a claim written, edited or taken back. The listing is
    /// not read again, so the cost of such a write does not grow with the number
    /// of things.
    async fn refresh_claims(
        &self,
        handle: &EntityId,
        wrote: image::Wrote,
    ) -> Result<(), MemoryError> {
        self.refresh_listing(handle, Some(wrote)).await
    }

    async fn refresh_listing(
        &self,
        handle: &EntityId,
        wrote: Option<image::Wrote>,
    ) -> Result<(), MemoryError> {
        let relist = wrote.is_none();
        if !self.wanted.load(Ordering::SeqCst) {
            return Ok(());
        }
        if self.held(|_| ()).is_none() {
            return self.fill().await.map(|_| ());
        }
        let read = async {
            let entities = if relist {
                Some(self.inner.list_entities(None).await?)
            } else {
                None
            };
            let doc = Doc::load(&*self.inner, handle).await?;
            // **A write to this thing's claims asks the store nothing about who
            // points where, bar a name an engine record of the thing points at.** The image patches its lists from the thing's old and
            // new documents ([`Image::install`]); asking `referring_to` for each
            // name would read the claims of every other thing that names it, which
            // costs the store and not the write. A write that can move any
            // thing's links asks for every name.
            let names = self
                .held(|image| {
                    if relist {
                        image.names_touched_by(&doc)
                    } else {
                        image.names_to_ask(&doc)
                    }
                })
                .unwrap_or_default();
            let referring = image::ask_referring(&self.inner, names).await?;
            Ok::<_, MemoryError>((entities, doc, referring))
        }
        .await;
        match read {
            Ok((entities, doc, referring)) => {
                if let Some(image) = self.image.write().expect("fold lock").as_mut() {
                    image.install(entities, doc, referring, wrote.as_ref());
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

    /// **Run a write and the image's following it inside the one scope every
    /// write of every fold shares**: a write whose future is dropped, or that ends
    /// with the store's outcome unknown, drops the image, and the next write fills
    /// it again (see [`uncertain::guarded`]).
    async fn scoped<T>(
        &self,
        write: impl std::future::Future<Output = Result<T, MemoryError>>,
    ) -> Result<T, MemoryError> {
        uncertain::guarded(&self.image, Some(&self.generation), "memory", write).await
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
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let _gate = self.writing.lock().await;
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
        let _gate = self.writing.lock().await;
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
        use jojobot_domain::app::AppOutcome;
        let _gate = self.writing.lock().await;
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
        let _gate = self.writing.lock().await;
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
        match self.held(|image| image.list(kind)) {
            Some(listing) => Ok(listing),
            None => self.inner.list_entities(kind).await,
        }
    }

    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
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
        let _gate = self.writing.lock().await;
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
        let _gate = self.writing.lock().await;
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
                Folded::follow("rename_entity", self.patched_or_reloaded(patch).await);
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
        let _gate = self.writing.lock().await;
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
        let _gate = self.writing.lock().await;
        self.scoped(async {
            let restored = self.inner.restore_entity(id, session).await?;
            Folded::follow("restore_entity", self.refresh(&restored.0.id).await);
            Ok(restored)
        })
        .await
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let _gate = self.writing.lock().await;
        let sent: Vec<String> = fact.fields.keys().cloned().collect();
        self.scoped(async {
            match self.inner.capture(fact).await? {
                Guarded::Written(fact) => match self
                    .refresh_claims(&fact.home, vec![(fact.id.clone(), sent)])
                    .await
                {
                    Ok(()) => Ok(Guarded::Written(fact)),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        match self.held(|image| image.doc(subject).map(|doc| doc.scan.facts.clone())) {
            Some(answer) => answer,
            None => self.inner.recall(subject).await,
        }
    }

    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        match self.held(|image| image.history(entity, key)) {
            Some(answer) => answer,
            None => self.inner.history(entity, key).await,
        }
    }

    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        match self.held(|image| image.merge_events(entity)) {
            Some(answer) => Ok(answer),
            None => self.inner.merge_events(entity).await,
        }
    }

    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
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
        match self.held(|image| image.unmapped(entity)) {
            Some(answer) => Ok(answer),
            None => self.inner.unmapped_claims(entity).await,
        }
    }

    /// **An address from before a merge is moved by the image's own copy of the
    /// events**, so a claim read costs the store nothing: the image names no
    /// claim of a folded thing, and the event's map says where the claim went.
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        match self.held(|image| image.claim_history(address)) {
            Some(Err(merged @ MemoryError::AlreadyMerged { .. })) => {
                match self.held(|image| image.moved(address)) {
                    Some(Err(looped)) => Err(looped.into()),
                    Some(Ok(Some(moved))) => match self.held(|image| image.claim_history(&moved)) {
                        Some(answer) => answer,
                        None => self.inner.claim_history(address).await,
                    },
                    // **A claim the survivor names as one it cannot serve** is
                    // answered from the chain the image read when it filled the
                    // survivor's document. A chain that could not be read then is
                    // read where it is stored.
                    _ if self.held(|image| image.names_as_unmapped(address)) == Some(true) => {
                        match self.held(|image| image.unmapped_history(address)) {
                            Some(Some(chain)) => Ok(chain),
                            _ => self.inner.claim_history(address).await,
                        }
                    }
                    _ => Err(merged),
                }
            }
            Some(answer) => answer,
            None => self.inner.claim_history(address).await,
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
        match self.held(|image| image.entity_history(entity)) {
            Some(Ok(Some(versions))) => Ok(versions),
            Some(Err(unknown)) => Err(unknown),
            Some(Ok(None)) | None => self.inner.entity_history(entity).await,
        }
    }

    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        match self.held(|image| image.kept_off(address)) {
            Some(answer) => answer,
            None => self.inner.keys_kept_off_the_thing(address).await,
        }
    }

    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        match self.held(|image| image.trails(entity)) {
            Some(answer) => answer,
            None => self.inner.key_trails(entity).await,
        }
    }

    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
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
        let _gate = self.writing.lock().await;
        let sent: Vec<String> = jojobot_domain::memory::writes_of(&patch, &BTreeMap::new())
            .into_iter()
            .filter_map(|(key, value)| value.map(|_| key))
            .collect();
        self.scoped(async {
            match self.inner.update_fact(address, patch, caller).await? {
                Guarded::Written(fact) => match self
                    .refresh_claims(&fact.home, vec![(fact.id.clone(), sent)])
                    .await
                {
                    Ok(()) => Ok(Guarded::Written(fact)),
                    Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
                },
                blocked => Ok(blocked),
            }
        })
        .await
    }

    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        match self.held(|image| image.doc(entity).map(|doc| doc.fields.clone())) {
            Some(answer) => answer,
            None => self.inner.fields(entity).await,
        }
    }

    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        match self.held(|image| {
            image
                .doc(entity)
                .map(|doc| (doc.fields.clone(), doc.version))
        }) {
            Some(answer) => answer,
            None => self.inner.fields_versioned(entity).await,
        }
    }

    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
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
        let _gate = self.writing.lock().await;
        self.scoped(async {
            let taken_back = self
                .inner
                .retract(address, reason, date, caller, session)
                .await?;
            match self
                .refresh_claims(&taken_back.retracted.home, Vec::new())
                .await
            {
                Ok(()) => Ok(taken_back),
                Err(source) => Err(fold_behind(
                    Landed::Retraction(Box::new(taken_back)),
                    source,
                )),
            }
        })
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
        let _gate = self.writing.lock().await;
        self.scoped(async {
            let done = self
                .inner
                .merge(folded, survivor, reason, date, caller, session)
                .await?;
            let patch = image::patch::Change::Merged { folded, survivor };
            match self.patched_or_reloaded(patch).await {
                Ok(()) => Ok(done),
                Err(source) => Err(fold_behind(Landed::Merge(Box::new(done)), source)),
            }
        })
        .await
    }

    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        let _gate = self.writing.lock().await;
        self.scoped(async {
            let stored = self.inner.set_prose(entity, prose, session).await?;
            Folded::follow("set_prose", self.refresh(entity).await);
            Ok(stored)
        })
        .await
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        match self.held(Image::scan) {
            Some(scanned) => Ok(scanned),
            None => self.inner.scan().await,
        }
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        match self.held(|image| image.scan_one(entity)) {
            Some(doc) => Ok(doc),
            None => self.inner.scan_entity(entity).await,
        }
    }

    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        match self.held(|image| image.has_prose(entity)) {
            Some(has) => Ok(has),
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
        match self.held(|image| image.backing(entity)) {
            Some(answer) => answer,
            None => self.inner.backing(entity).await,
        }
    }

    /// **Selected on the pointer in memory**, not by reading every record.
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        // An address on a thing merged away is moved first, when the merge kept
        // an event.
        match self.held(|image| image.moved(source)) {
            Some(Err(looped)) => return Err(looped.into()),
            Some(Ok(Some(moved))) => {
                return match self.held(|image| image.built_on(&moved)) {
                    Some(answer) => answer,
                    None => self.inner.built_on(&moved).await,
                };
            }
            _ => {}
        }
        match self.held(|image| image.built_on(source)) {
            Some(answer) => answer,
            None => self.inner.built_on(source).await,
        }
    }

    /// **Selected on what a record's fields name, in memory**, not by reading
    /// every record.
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        match self.held(|image| image.referring_to(target)) {
            Some(answer) => answer,
            None => self.inner.referring_to(target).await,
        }
    }

    /// **Straight through**: the width of a read is the inner store's.
    fn read_width(&self) -> usize {
        self.inner.read_width()
    }
    /// **Answered from the image**, which holds each bot's records with their
    /// edges. The store's own check inside a ceiling write is its own and does
    /// not pass here.
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        match self.held(|image| image.thought_writers(thread)) {
            Some(answer) => answer,
            None => self.inner.thought_writers(thread).await,
        }
    }

    /// **Answered from the image**, which keeps what each key holds by value.
    async fn keys_holding(
        &self,
        key: &str,
        test: &jojobot_domain::memory::filter::Test,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        match self.held(|image| image.keys_holding(key, test, kind)) {
            Some(answer) => Ok(answer),
            None => self.inner.keys_holding(key, test, kind).await,
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
        match self.held(|image| image.ordered_first_k(key, holds, direction, kind, among, k)) {
            Some(answer) => Ok(answer),
            None => {
                self.inner
                    .ordered_first_k(key, holds, direction, kind, among, k)
                    .await
            }
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
        let _gate = self.writing.lock().await;
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
        match self.held(Image::types) {
            Some(declared) => Ok(declared),
            None => self.inner.declared_types().await,
        }
    }

    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<DeclaredType>>, MemoryError> {
        match self.held(Image::types_shared) {
            Some(declared) => Ok(declared),
            None => Ok(std::sync::Arc::new(self.inner.declared_types().await?)),
        }
    }

    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
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
        let _gate = self.writing.lock().await;
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
        match self.held(Image::kinds) {
            Some(kinds) => Ok(kinds),
            None => self.inner.declared_kinds().await,
        }
    }

    /// **Taking a kind back changes folding too**: a key that summed while
    /// the kind held it falls back to newest-write-wins once reclaimed, for
    /// every entity that answered to it.
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        let _gate = self.writing.lock().await;
        self.scoped(async {
            self.inner.reclaim_kind(token).await?;
            Self::follow("reclaim_kind", self.reload().await);
            Ok(())
        })
        .await
    }
}

#[cfg(test)]
mod stalled_read;

#[cfg(test)]
mod tests {
    use super::stalled_read::StallFirstVersionedRead;
    use super::*;
    use jiff::civil::date;
    use jojobot_domain::memory::testing::{InMemoryMemory, contract};
    use jojobot_domain::memory::types::{Field, ValueType};

    fn alpha() -> EntityId {
        EntityId::person("person:alpha")
    }

    fn beta() -> EntityId {
        EntityId::person("person:beta")
    }

    async fn seeded(store: &Folded, id: &EntityId, name: &str) {
        store
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }

    /// **What is kept beside the documents by key follows every kind of write.**
    /// After each write the values held by key, the places kept for a date or a
    /// number, the items of a list and the records worked out from a claim are
    /// what a build over the documents makes: a write takes off what the thing
    /// held and puts on what it holds, and leaves an empty entry nowhere.
    #[tokio::test]
    async fn what_is_kept_by_key_follows_every_kind_of_write() {
        use jojobot_domain::memory::FactPatch;
        let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
        folded.rebuild().await.expect("rebuild ok");
        for declared in [
            DeclaredType::new(
                "kept",
                vec![
                    Field::new("due", ValueType::Date),
                    Field::new("points", ValueType::Number),
                    Field::listing("tags", ValueType::Text),
                ],
            ),
            // One key two types read two ways keeps a place for each.
            DeclaredType::new("kept-too", vec![Field::new("points", ValueType::Date)]),
        ] {
            folded.declare_type(declared, None).await.expect("declared");
        }
        let check = |after: &'static str| {
            let folded = &folded;
            async move {
                let checked = folded
                    .held(|image| image.upkeep_is_what_a_build_would_make())
                    .expect("the image is held");
                if let Err(differs) = checked {
                    panic!("after {after}: {differs}");
                }
            }
        };
        let (gamma, delta) = (
            EntityId::person("person:gamma"),
            EntityId::person("person:kappa"),
        );
        for (id, name) in [
            (alpha(), "Alpha"),
            (beta(), "Beta"),
            (gamma.clone(), "Gamma"),
        ] {
            seeded(&folded, &id, name).await;
        }
        check("three things are added").await;

        let set = |id: &EntityId, what: &str, held: &[(&str, &str)]| {
            contract::state(NewFact {
                fields: held
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                ..NewFact::about(id.clone(), what, date(2026, 9, 1))
            })
        };
        let first = folded
            .capture(set(
                &alpha(),
                "alpha holds",
                &[
                    ("due", "2026-09-14"),
                    ("points", "3"),
                    ("tags", "kitchen, garden"),
                ],
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        check("a capture").await;
        let beta_holds = folded
            .capture(set(
                &beta(),
                "beta holds",
                &[
                    ("due", "2026-09-14"),
                    ("points", "lots"),
                    ("tags", "kitchen"),
                ],
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        check("a capture that ties and holds a word where a number goes").await;
        let mut built_on = set(&gamma, "gamma stands on alpha", &[("tags", "")]);
        built_on.derived_from = Some(FactAddress::new(alpha(), first.id.clone()));
        folded
            .capture(built_on)
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        check("a capture worked out from a claim").await;

        for (after, held) in [
            (
                "an edit that moves a date and trims a list",
                [("due", "2026-10-01"), ("points", "3"), ("tags", "kitchen")].as_slice(),
            ),
            (
                "an edit that adds an item to a list",
                [
                    ("due", "2026-10-01"),
                    ("points", "3"),
                    ("tags", "kitchen, shed"),
                ]
                .as_slice(),
            ),
            (
                "an edit that takes an item off a list",
                [("due", "2026-10-01"), ("points", "3"), ("tags", "shed")].as_slice(),
            ),
        ] {
            folded
                .update_fact(
                    &first.address(),
                    contract::state_patch(FactPatch {
                        fields: held
                            .iter()
                            .map(|(key, value)| (key.to_string(), value.to_string()))
                            .collect(),
                        ..FactPatch::default()
                    }),
                    &EntityId("bot:sigma".into()),
                )
                .await
                .expect("update ok")
                .written()
                .expect("not blocked");
            check(after).await;
        }

        folded
            .retract(
                &beta_holds.address(),
                Some("not so"),
                date(2026, 9, 9),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("retract ok");
        check("a retraction").await;

        folded
            .archive_entity(&gamma, "set aside", None)
            .await
            .expect("archive ok");
        check("an archive").await;
        folded
            .restore_entity(&gamma, None)
            .await
            .expect("restore ok");
        check("a restore").await;

        folded
            .set_prose(&alpha(), "alpha, in words", None)
            .await
            .expect("prose ok");
        check("a page").await;

        let notes = EntityId("document:contract-notes".into());
        seeded(&folded, &notes, "Contract Notes").await;
        for section in ["# Heading", "The words under it."] {
            folded
                .capture(NewFact::about(notes.clone(), section, date(2026, 9, 9)))
                .await
                .expect("capture ok")
                .written()
                .expect("not blocked");
        }
        check("a document put as sections").await;

        folded
            .rename_entity(
                &gamma,
                &delta,
                None,
                date(2026, 9, 10),
                jojobot_domain::memory::Override::none(),
                None,
            )
            .await
            .expect("rename ok")
            .written()
            .expect("not blocked");
        check("a rename").await;

        folded
            .merge(
                &delta,
                &alpha(),
                Some("the same"),
                date(2026, 9, 11),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("merge ok");
        check("a merge").await;

        folded
            .declare_type(
                DeclaredType::new(
                    "kept-three",
                    vec![Field::new("shed_count", ValueType::Number)],
                ),
                None,
            )
            .await
            .expect("declared");
        check("a declaration").await;
    }

    /// **The boot path.** A store already holding a write before `Folded`
    /// ever wraps it is folded correctly once `rebuild` runs — the cache is
    /// not only ever filled by write-through.
    #[tokio::test]
    async fn rebuild_folds_what_the_store_already_held() {
        let inner = Arc::new(InMemoryMemory::booted());
        inner
            .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
        inner
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(
                    alpha(),
                    "was here before the fold wrapped it",
                    date(2026, 9, 1),
                )
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let folded = Folded::new(inner);
        let before_rebuild = folded.fields(&alpha()).await.expect("fields ok");
        assert_eq!(
            before_rebuild.get("due_on").map(String::as_str),
            Some("2026-09-14"),
            "a cache miss falls through to the store, so this reads right even before rebuild"
        );

        let folded_count = folded.rebuild().await.expect("rebuild ok");
        assert_eq!(folded_count, 1, "one entity to fold");
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
    }

    /// **A first fill that failed is filled again by the next write.** The store
    /// was unreachable when the server booted and recovered a moment later. The
    /// next write fills the image, and the read after it comes from the image,
    /// makes no call to the store, and carries that write.
    #[tokio::test]
    async fn a_first_fill_that_failed_is_filled_again_by_the_next_write() {
        use crate::counted::Counted;
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        counted.set_unreachable(true);
        folded
            .rebuild()
            .await
            .expect_err("the store cannot be reached");
        counted.set_unreachable(false);

        seeded(&folded, &alpha(), "Alpha").await;
        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(
                    alpha(),
                    "written after the store came back",
                    date(2026, 9, 1),
                )
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        counted.reset();
        let fields = folded.fields(&alpha()).await.expect("fields ok");
        assert_eq!(
            counted.calls(),
            0,
            "the read reached the store: {:?}",
            counted.seen()
        );
        assert_eq!(
            fields.get("due_on").map(String::as_str),
            Some("2026-09-14"),
            "the image does not carry the write"
        );
    }

    /// **A capture whose caller stopped waiting after the store committed is in
    /// the image.** The write landed and the future that would have followed it
    /// was dropped. A read from the image must still answer as the store does.
    #[tokio::test]
    async fn a_capture_dropped_after_the_store_committed_is_in_the_image() {
        use crate::counted::Counted;
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("an empty store fills");
        seeded(&folded, &alpha(), "Alpha").await;

        counted.hang_after_next("capture");
        let waited = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            folded.capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "written, then the caller left", date(2026, 9, 1))
            })),
        )
        .await;
        assert!(waited.is_err(), "the capture is cancelled: {waited:?}");
        let in_store = store.fields(&alpha()).await.expect("the store answers");
        assert_eq!(
            in_store.get("due_on").map(String::as_str),
            Some("2026-09-14"),
            "the capture committed in the store"
        );

        let in_image = folded.fields(&alpha()).await.expect("the image answers");
        assert_eq!(in_image, in_store, "the image misses a committed capture");
        let claims = folded.recall(&alpha()).await.expect("recall ok");
        assert!(
            claims.iter().any(|f| f.content.contains("the caller left")),
            "the image does not hold the claim: {claims:?}"
        );
    }

    /// **A creation whose caller stopped waiting after the store committed is in
    /// the image.** The same scope guards every write verb, not only `capture`.
    #[tokio::test]
    async fn an_add_entity_dropped_after_the_store_committed_is_in_the_image() {
        use crate::counted::Counted;
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("an empty store fills");

        counted.hang_after_next("add_entity");
        let waited = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            folded.add_entity(NewEntity::new(alpha(), "Alpha", "user-named")),
        )
        .await;
        assert!(waited.is_err(), "the creation is cancelled: {waited:?}");
        let in_store = store.list_entities(None).await.expect("the store answers");
        assert_eq!(in_store.len(), 1, "the creation committed in the store");

        let in_image = folded.list_entities(None).await.expect("the image answers");
        assert_eq!(in_image, in_store, "the image misses a committed creation");
    }

    /// **The writes of a claim a survivor cannot serve are answered from the
    /// image.** The image names such a claim; its writes are read when the
    /// document is filled, so asking for them makes no call to the store. Paired
    /// with the answer being the store's own, and non-empty.
    #[tokio::test]
    async fn the_writes_of_a_claim_a_survivor_cannot_serve_are_read_from_the_image() {
        use crate::counted::Counted;
        let store = Arc::new(InMemoryMemory::booted());
        let counted = Arc::new(Counted::new(store.clone()));
        let folded = Folded::new(counted.clone());
        folded.rebuild().await.expect("an empty store fills");
        seeded(&folded, &alpha(), "Alpha").await;
        seeded(&folded, &beta(), "Beta").await;
        folded
            .capture(NewFact::about(beta(), "plays the cornet", date(2026, 9, 1)))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        folded
            .merge(
                &beta(),
                &alpha(),
                Some("filed twice"),
                date(2026, 9, 2),
                &EntityId("bot:sigma".into()),
                Some("run"),
            )
            .await
            .expect("merge ok");
        // A claim lands under the folded key past the fold, as damage does; a
        // restart reads it.
        let stray = store.stray_claim_under(&beta(), "a stray claim");
        folded.rebuild().await.expect("the image fills again");
        assert!(
            folded.unmapped_claims(&alpha()).await.expect("reads").len() == 1,
            "the survivor names the stray claim, so the case reaches the branch"
        );

        counted.reset();
        let from_image = folded
            .claim_history(&stray)
            .await
            .expect("the address reads");
        assert_eq!(
            counted.calls(),
            0,
            "the writes of an unmapped claim reached the store: {:?}",
            counted.seen()
        );
        assert_eq!(
            from_image,
            store.claim_history(&stray).await.expect("the store reads"),
            "the image answers what the store answers"
        );
        assert_eq!(from_image.len(), 1, "the claim has its one write");
    }

    /// **Write-through keeps the fold current, and reordering after a
    /// retraction is where a fold built from arithmetic instead of a re-read
    /// would go wrong.** Two writes name the same key; retracting the newer
    /// one must bring the older, still-active write back — not merely blank
    /// the key, and not still show the retracted value.
    #[tokio::test]
    async fn capture_and_retract_keep_the_fold_equal_to_the_store() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        let first = folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "first due date", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the newest write wins, positive case"
        );

        let second = folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
                ..NewFact::about(alpha(), "moved to next week", date(2026, 9, 8))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-21".to_string()),
            "the second write is now newest"
        );
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );

        folded
            .retract(
                &FactAddress {
                    home: alpha(),
                    local: second.id.clone(),
                },
                Some("moved back"),
                date(2026, 9, 9),
                &EntityId("bot:sigma".into()),
                None,
            )
            .await
            .expect("retract ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "retracting the newer write must bring the older active one back, not blank the key"
        );
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
        let _ = first;
    }

    /// **A merge moves fields, not only claims — the fold has to follow to
    /// both sides.** The folded thing's key must stop reading and the
    /// survivor's must start.
    #[tokio::test]
    async fn merge_moves_the_fold_to_the_survivor() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;
        seeded(&folded, &beta(), "Beta").await;

        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "alpha's own due date", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        folded
            .merge(
                &alpha(),
                &beta(),
                Some("same thing"),
                date(2026, 9, 10),
                &EntityId::person("person:gamma"),
                None,
            )
            .await
            .expect("merge ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            None,
            "the folded handle's own fields must not still show what moved, negative case"
        );
        assert_eq!(
            folded
                .fields(&beta())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the survivor must now hold what moved, positive case"
        );
        assert_eq!(
            folded.fields(&beta()).await.expect("fields ok"),
            folded
                .inner
                .fields(&beta())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
    }

    /// **`fields` answers from the cache, not by re-asking the store every
    /// time.** A write made straight to the wrapped store — bypassing
    /// `Folded`, the shape a second writer would take — is invisible to a
    /// `fields` call that hits the cache, exactly the load-bearing
    /// single-writer assumption this module's doc warns about. Without this
    /// case, `fields` falling straight through to the store on every call —
    /// correct, but pointless — would look identical to every other test
    /// here passing.
    #[tokio::test]
    async fn fields_are_served_from_the_cache_not_rederived_on_every_call() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        folded.rebuild().await.expect("an empty store fills");
        seeded(&folded, &alpha(), "Alpha").await;
        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "cached", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        // Straight to the wrapped store, never through `Folded::capture` —
        // the cache is never told this happened.
        folded
            .inner
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
                ..NewFact::about(alpha(), "written around the fold", date(2026, 9, 8))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-21".to_string()),
            "the store itself already sees the bypassed write"
        );
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the fold still answers with what it cached, not what the store now holds"
        );
    }

    /// **The behavioural contract holds wrapped in a fold, not only bare.**
    /// `Folded` forwards every guard, every read and every write it does not
    /// itself refresh straight to the store underneath, so the whole existing
    /// suite is the regression test for that claim — a decorator that changed
    /// what any of it answers would fail here, with no case written for the
    /// fold itself.
    /// **The same contract, read through a loaded image.** Every case writes
    /// through the fold from an empty store, so each read it makes is one the
    /// image answers. Dealt into parts that run side by side, because a read
    /// through the fake costs as much as the store is large and a single store
    /// holding every case's rows would take minutes.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn fake_satisfies_the_contract_when_folded_and_loaded() {
        let mut parts = Vec::new();
        for part in 0..contract::PARTS {
            parts.push(tokio::spawn(async move {
                let folded = Folded::new(Arc::new(InMemoryMemory::booted()));
                folded.rebuild().await.expect("an empty store fills");
                contract::run_part(&folded, part, contract::PARTS).await
            }));
        }
        let mut ran = 0;
        for part in parts {
            ran += part.await.expect("a part of the contract held");
        }
        assert_eq!(
            ran,
            contract::case_count(),
            "a case was dropped between parts"
        );
    }

    /// **The same contract through a fold that was never filled**, one test per
    /// part, each on a store of its own.
    async fn folded_fake_contract_part(part: usize) {
        let ran = contract::run_part(
            &Folded::new(Arc::new(InMemoryMemory::booted())),
            part,
            contract::PARTS,
        )
        .await;
        contract::assert_part_ran(
            "the folded fake",
            part,
            ran,
            contract::part_share(contract::case_count(), part),
        );
    }

    jojobot_domain::contract_parts!(tokio::test, folded_fake_contract_part:
        fake_satisfies_the_contract_when_folded_part_00 = 0,
        fake_satisfies_the_contract_when_folded_part_01 = 1,
        fake_satisfies_the_contract_when_folded_part_02 = 2,
        fake_satisfies_the_contract_when_folded_part_03 = 3,
        fake_satisfies_the_contract_when_folded_part_04 = 4,
        fake_satisfies_the_contract_when_folded_part_05 = 5,
        fake_satisfies_the_contract_when_folded_part_06 = 6,
        fake_satisfies_the_contract_when_folded_part_07 = 7,
        fake_satisfies_the_contract_when_folded_part_08 = 8,
        fake_satisfies_the_contract_when_folded_part_09 = 9,
        fake_satisfies_the_contract_when_folded_part_10 = 10,
        fake_satisfies_the_contract_when_folded_part_11 = 11,
        fake_satisfies_the_contract_when_folded_part_12 = 12,
        fake_satisfies_the_contract_when_folded_part_13 = 13,
        fake_satisfies_the_contract_when_folded_part_14 = 14,
        fake_satisfies_the_contract_when_folded_part_15 = 15,
    );

    /// The fold forwards which keys of a record the store holds off the thing.
    #[tokio::test]
    async fn the_fold_forwards_the_keys_a_switched_store_holds_off_a_thing() {
        contract::own_fields_stay_while_settings_and_computed_keys_reach(&Folded::new(Arc::new(
            InMemoryMemory::booted(),
        )))
        .await;
    }

    /// **A store whose WRITE succeeds and whose immediate re-read fails** —
    /// exactly what [`Folded::refresh`] sees when the inner write lands and
    /// the re-read after it cannot run. Every other fake in this crate fails
    /// everything or fails nothing, so this branch of `Folded` had never run
    /// before this double existed.
    struct WritesThenFailsToReadFields(Arc<InMemoryMemory>);

    #[async_trait]
    impl Memory for WritesThenFailsToReadFields {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.0.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
            self.0.list_entities(kind).await
        }
        async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
            self.0.former_handles().await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.0.update_entity(handle, patch).await
        }
        async fn archive_entity(
            &self,
            id: &EntityId,
            reason: &str,
            session: Option<&str>,
        ) -> Result<Entity, MemoryError> {
            self.0.archive_entity(id, reason, session).await
        }
        async fn restore_entity(
            &self,
            id: &EntityId,
            session: Option<&str>,
        ) -> Result<(Entity, Archived), MemoryError> {
            self.0.restore_entity(id, session).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            resemblance: jojobot_domain::memory::Override<'_>,
            session: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.0
                .rename_entity(from, to, parent, date, resemblance, session)
                .await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.0.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.0.recall(subject).await
        }
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.0.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.0.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
            caller: &EntityId,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.0.update_fact(address, patch, caller).await
        }
        /// **The one method this double exists to break.** Always fails,
        /// whatever the write just did — this is what `Folded::refresh` calls.
        async fn fields(
            &self,
            _entity: &EntityId,
        ) -> Result<BTreeMap<String, String>, MemoryError> {
            Err(MemoryError::Store(
                "read failed right after the write landed".into(),
            ))
        }
        async fn retract(
            &self,
            address: &FactAddress,
            reason: Option<&str>,
            date: Date,
            caller: &EntityId,
            session: Option<&str>,
        ) -> Result<Retraction, MemoryError> {
            self.0.retract(address, reason, date, caller, session).await
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
            self.0
                .merge(folded, survivor, reason, date, caller, session)
                .await
        }
        async fn set_prose(
            &self,
            entity: &EntityId,
            prose: &str,
            session: Option<&str>,
        ) -> Result<String, MemoryError> {
            self.0.set_prose(entity, prose, session).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.0.scan().await
        }
        async fn declare_type(
            &self,
            declared: DeclaredType,
            by_session: Option<&str>,
        ) -> Result<DeclaredType, MemoryError> {
            self.0.declare_type(declared, by_session).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.0.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<bool, MemoryError> {
            self.0.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.0.declared_kinds().await
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.0.reclaim_kind(token).await
        }
    }

    /// **The write says it landed, never that it failed, when only the fold's
    /// own re-read could not run.** Before this fix, `capture` propagated the
    /// refresh's error straight out via `?`, so a caller of a write that
    /// genuinely succeeded was told it had not.
    #[tokio::test]
    async fn a_write_that_lands_and_a_refresh_that_fails_answers_landed_not_failed() {
        let inner = Arc::new(InMemoryMemory::booted());
        let flaky = Arc::new(WritesThenFailsToReadFields(inner.clone()));
        let folded = Folded::new(flaky);
        folded.rebuild().await.expect("an empty store fills");
        seeded(&folded, &alpha(), "Alpha").await;

        let err = folded
            .capture(NewFact::about(
                alpha(),
                "captured while the fold could not refresh",
                date(2026, 9, 14),
            ))
            .await
            .expect_err("the inner refresh always fails in this double, so this cannot be Ok");

        let MemoryError::FoldBehind { landed, behind, .. } = err else {
            panic!("a write that landed must answer FoldBehind, never a bare error: {err:?}");
        };
        assert_eq!(
            behind,
            search::Behind::Stale,
            "a write-path fold gap is always Stale, never Unscanned"
        );
        let Landed::Fact(fact) = landed else {
            panic!("capture's landed payload is a Fact");
        };
        assert_eq!(fact.content, "captured while the fold could not refresh");

        // **The positive half.** Without this, a write that never happened
        // would look identical to one that did — both come back as
        // `FoldBehind`, and only reading the store proves the row is real.
        let recalled = inner.recall(&alpha()).await.expect("recall ok");
        assert!(
            recalled.iter().any(|f| f.id == fact.id),
            "the write must be visible in the store even though the fold refresh failed: \
             {recalled:?}",
        );
    }

    /// **A declaration that landed is never answered as a failure, whatever the
    /// fold's re-read did** (rule 130). Each of the three verbs below changes how
    /// every entity folds, so each reads the whole image again after it lands; the
    /// double lets the write through and fails that read. The answer is success,
    /// the store holds the declaration, and the image is gone so reads go to the
    /// store (see [`Folded::follow`]). Paired with the store read in the same
    /// case, so an answer of success cannot be a write that never landed.
    async fn a_fold_whose_reread_fails_after_a_declaration() -> (Arc<InMemoryMemory>, Folded) {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(Arc::new(WritesThenFailsToReadFields(inner.clone())));
        folded.rebuild().await.expect("an empty store fills");
        seeded(&folded, &alpha(), "Alpha").await;
        (inner, folded)
    }

    #[tokio::test]
    async fn a_type_declared_while_the_fold_cannot_reread_answers_landed_not_failed() {
        let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
        let declared = folded
            .declare_type(
                types::DeclaredType::new(
                    "fold-behind-type",
                    vec![types::Field::new("laps", types::ValueType::Text)],
                ),
                None,
            )
            .await
            .expect("a declaration that landed is not a failure");
        assert_eq!(declared.name, "fold-behind-type");
        let held = inner.declared_types().await.expect("types read");
        assert!(
            held.iter().any(|t| t.name == "fold-behind-type"),
            "the declaration is in the store: {held:?}"
        );
    }

    #[tokio::test]
    async fn a_kind_declared_while_the_fold_cannot_reread_answers_landed_not_failed() {
        let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
        folded
            .declare_kind("theta", types::Origin::Declared, Vec::new())
            .await
            .expect("a declaration that landed is not a failure");
        let held = inner.declared_kinds().await.expect("kinds read");
        assert!(
            held.iter().any(|(token, _)| token == "theta"),
            "the kind is in the store: {held:?}"
        );
    }

    #[tokio::test]
    async fn a_kind_reclaimed_while_the_fold_cannot_reread_answers_landed_not_failed() {
        let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
        inner
            .declare_kind("theta", types::Origin::Shipped, Vec::new())
            .await
            .expect("the kind is shipped into the store");
        folded
            .reclaim_kind("theta")
            .await
            .expect("a reclaim that landed is not a failure");
        let held = inner.declared_kinds().await.expect("kinds read");
        assert!(
            held.iter().all(|(token, _)| token != "theta"),
            "the kind is gone from the store: {held:?}"
        );
    }

    /// **Counts calls to the whole-corpus `scan`, apart from `scan_entity`.**
    /// `scan_entity` is answered directly here, from `self.inner.scan()`
    /// called by hand rather than through this double's own counted `scan`
    /// method — `InMemoryMemory` has the identical gap this double exists to
    /// catch `Folded` having, so forwarding to `self.inner.scan_entity`
    /// would fall through to `InMemoryMemory`'s own default and be
    /// indistinguishable from the whole-corpus path this counts.
    struct CountingScan {
        inner: Arc<InMemoryMemory>,
        scan_calls: Arc<std::sync::atomic::AtomicUsize>,
    }

    #[async_trait]
    impl Memory for CountingScan {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
            self.inner.list_entities(kind).await
        }
        async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
            self.inner.former_handles().await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.update_entity(handle, patch).await
        }
        async fn archive_entity(
            &self,
            id: &EntityId,
            reason: &str,
            session: Option<&str>,
        ) -> Result<Entity, MemoryError> {
            self.inner.archive_entity(id, reason, session).await
        }
        async fn restore_entity(
            &self,
            id: &EntityId,
            session: Option<&str>,
        ) -> Result<(Entity, Archived), MemoryError> {
            self.inner.restore_entity(id, session).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            resemblance: jojobot_domain::memory::Override<'_>,
            session: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner
                .rename_entity(from, to, parent, date, resemblance, session)
                .await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.inner.recall(subject).await
        }
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.inner.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.inner.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
            caller: &EntityId,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.update_fact(address, patch, caller).await
        }
        async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
            self.inner.fields(entity).await
        }
        async fn retract(
            &self,
            address: &FactAddress,
            reason: Option<&str>,
            date: Date,
            caller: &EntityId,
            session: Option<&str>,
        ) -> Result<Retraction, MemoryError> {
            self.inner
                .retract(address, reason, date, caller, session)
                .await
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
            self.inner
                .merge(folded, survivor, reason, date, caller, session)
                .await
        }
        async fn set_prose(
            &self,
            entity: &EntityId,
            prose: &str,
            session: Option<&str>,
        ) -> Result<String, MemoryError> {
            self.inner.set_prose(entity, prose, session).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.scan_calls
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.inner.scan().await
        }
        async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
            Ok(self
                .inner
                .scan()
                .await?
                .into_iter()
                .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity)))
        }
        async fn declare_type(
            &self,
            declared: DeclaredType,
            by_session: Option<&str>,
        ) -> Result<DeclaredType, MemoryError> {
            self.inner.declare_type(declared, by_session).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.inner.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<bool, MemoryError> {
            self.inner.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.inner.declared_kinds().await
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.inner.reclaim_kind(token).await
        }
    }

    /// **A store that says how wide it can be read, and counts how wide it was.**
    struct Gauged {
        inner: Arc<InMemoryMemory>,
        width: usize,
        in_flight: std::sync::atomic::AtomicUsize,
        peak: std::sync::atomic::AtomicUsize,
    }

    #[async_trait]
    impl Memory for Gauged {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
            self.inner.list_entities(kind).await
        }
        async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
            self.inner.former_handles().await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.update_entity(handle, patch).await
        }
        async fn archive_entity(
            &self,
            id: &EntityId,
            reason: &str,
            session: Option<&str>,
        ) -> Result<Entity, MemoryError> {
            self.inner.archive_entity(id, reason, session).await
        }
        async fn restore_entity(
            &self,
            id: &EntityId,
            session: Option<&str>,
        ) -> Result<(Entity, Archived), MemoryError> {
            self.inner.restore_entity(id, session).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            resemblance: jojobot_domain::memory::Override<'_>,
            session: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner
                .rename_entity(from, to, parent, date, resemblance, session)
                .await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.inner.recall(subject).await
        }
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.inner.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.inner.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
            caller: &EntityId,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.update_fact(address, patch, caller).await
        }
        async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
            self.inner.fields(entity).await
        }
        async fn retract(
            &self,
            address: &FactAddress,
            reason: Option<&str>,
            date: Date,
            caller: &EntityId,
            session: Option<&str>,
        ) -> Result<Retraction, MemoryError> {
            self.inner
                .retract(address, reason, date, caller, session)
                .await
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
            self.inner
                .merge(folded, survivor, reason, date, caller, session)
                .await
        }
        async fn set_prose(
            &self,
            entity: &EntityId,
            prose: &str,
            session: Option<&str>,
        ) -> Result<String, MemoryError> {
            self.inner.set_prose(entity, prose, session).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.inner.scan().await
        }
        async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
            self.inner.scan_entity(entity).await
        }
        async fn declare_type(
            &self,
            declared: DeclaredType,
            by_session: Option<&str>,
        ) -> Result<DeclaredType, MemoryError> {
            self.inner.declare_type(declared, by_session).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.inner.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<bool, MemoryError> {
            self.inner.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.inner.declared_kinds().await
        }
        fn read_width(&self) -> usize {
            self.width
        }
        async fn fields_versioned(
            &self,
            entity: &EntityId,
        ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
            use std::sync::atomic::Ordering::SeqCst;
            let now = self.in_flight.fetch_add(1, SeqCst) + 1;
            self.peak.fetch_max(now, SeqCst);
            // Long enough for every loader the fold starts to be in here at once.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let answer = self.inner.fields_versioned(entity).await;
            self.in_flight.fetch_sub(1, SeqCst);
            answer
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.inner.reclaim_kind(token).await
        }
    }

    /// **The fold's startup read never has more things in flight than the store
    /// says it can answer at once**, and does reach that many. The width is the
    /// store's own answer, so a store with a smaller pool is read more narrowly
    /// and the fold holds no number of its own.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn the_startup_read_never_has_more_in_flight_than_the_store_answers() {
        for width in [2usize, 3] {
            let inner = Arc::new(InMemoryMemory::booted());
            for handle in [
                "alpha", "kappa", "omega", "sigma", "lambda", "rho", "tau", "psi",
            ]
            .into_iter()
            {
                inner
                    .add_entity(NewEntity::new(
                        EntityId::person(handle),
                        handle.to_uppercase(),
                        "user-named",
                    ))
                    .await
                    .expect("add ok")
                    .written()
                    .expect("not blocked");
            }
            let gauged = Arc::new(Gauged {
                inner,
                width,
                in_flight: Default::default(),
                peak: Default::default(),
            });
            let folded = Folded::new(gauged.clone());
            folded.rebuild().await.expect("the image fills");
            let peak = gauged.peak.load(std::sync::atomic::Ordering::SeqCst);
            assert_eq!(
                peak, width,
                "eight things were read with at most {width} in flight, and that many at once"
            );
        }
    }

    /// **A store whose `field_ages` answer is taken and then held back**, so a
    /// test can let a write land between a reader's store read and the moment
    /// that reader installs what it read.
    struct AgesBehindAGate {
        inner: Arc<InMemoryMemory>,
        /// The next `field_ages` call reads the store, tells the first of these
        /// it has, and waits for the second before it answers.
        gate: std::sync::Mutex<Option<(Arc<tokio::sync::Notify>, Arc<tokio::sync::Notify>)>>,
    }

    #[async_trait]
    impl Memory for AgesBehindAGate {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
            self.inner.list_entities(kind).await
        }
        async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
            self.inner.former_handles().await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.update_entity(handle, patch).await
        }
        async fn archive_entity(
            &self,
            id: &EntityId,
            reason: &str,
            session: Option<&str>,
        ) -> Result<Entity, MemoryError> {
            self.inner.archive_entity(id, reason, session).await
        }
        async fn restore_entity(
            &self,
            id: &EntityId,
            session: Option<&str>,
        ) -> Result<(Entity, Archived), MemoryError> {
            self.inner.restore_entity(id, session).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            resemblance: jojobot_domain::memory::Override<'_>,
            session: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner
                .rename_entity(from, to, parent, date, resemblance, session)
                .await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.inner.recall(subject).await
        }
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.inner.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.inner.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
            caller: &EntityId,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.inner.update_fact(address, patch, caller).await
        }
        async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
            self.inner.fields(entity).await
        }
        async fn retract(
            &self,
            address: &FactAddress,
            reason: Option<&str>,
            date: Date,
            caller: &EntityId,
            session: Option<&str>,
        ) -> Result<Retraction, MemoryError> {
            self.inner
                .retract(address, reason, date, caller, session)
                .await
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
            self.inner
                .merge(folded, survivor, reason, date, caller, session)
                .await
        }
        async fn set_prose(
            &self,
            entity: &EntityId,
            prose: &str,
            session: Option<&str>,
        ) -> Result<String, MemoryError> {
            self.inner.set_prose(entity, prose, session).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.inner.scan().await
        }
        /// **The store read happens, then the call waits.** Whatever the store
        /// answers is already taken when the gate holds, so a write that lands
        /// while it waits is one the answer cannot include.
        async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
            let answered = self.inner.field_ages(entity).await?;
            let gate = self.gate.lock().expect("gate lock").take();
            if let Some((reached, release)) = gate {
                reached.notify_one();
                release.notified().await;
            }
            Ok(answered)
        }
        async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
            Ok(self
                .inner
                .scan()
                .await?
                .into_iter()
                .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity)))
        }
        async fn declare_type(
            &self,
            declared: DeclaredType,
            by_session: Option<&str>,
        ) -> Result<DeclaredType, MemoryError> {
            self.inner.declare_type(declared, by_session).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.inner.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<bool, MemoryError> {
            self.inner.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.inner.declared_kinds().await
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.inner.reclaim_kind(token).await
        }
    }

    /// **A reader that read the store before a write cannot install what it
    /// read after the write has cleared the cache.** The write drops every cached
    /// age when it finishes, and a reader that was between its store read and its
    /// insert would put the pre-write ages back, to be served as current until the
    /// next write. The double holds the reader exactly there, the write passes
    /// through, and the reader is let go. A reader that comes after must be told
    /// the ages the store holds now, not the ones the late reader carried.
    #[tokio::test]
    async fn a_reader_that_read_before_a_write_does_not_cache_the_pre_write_ages() {
        let inner = Arc::new(InMemoryMemory::booted());
        let gated = Arc::new(AgesBehindAGate {
            inner: inner.clone(),
            gate: std::sync::Mutex::new(None),
        });
        let folded = Arc::new(Folded::new(gated.clone()));
        seeded(&folded, &alpha(), "Alpha").await;
        let write = |colour: &str, content: &str| {
            jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("colour".to_string(), colour.to_string())]),
                ..NewFact::about(alpha(), content.to_string(), date(2026, 9, 1))
            })
        };
        folded
            .capture(write("green", "first colour"))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let reached = Arc::new(tokio::sync::Notify::new());
        let release = Arc::new(tokio::sync::Notify::new());
        *gated.gate.lock().expect("gate lock") = Some((reached.clone(), release.clone()));
        let reader = tokio::spawn({
            let folded = folded.clone();
            async move { folded.field_ages(&alpha()).await }
        });
        // The reader has read the store and not yet installed what it read.
        reached.notified().await;
        folded
            .capture(write("blue", "repainted"))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        release.notify_one();
        let late = reader
            .await
            .expect("the reader finished")
            .expect("ages read");

        let served = folded.field_ages(&alpha()).await.expect("ages read");
        let held = inner.field_ages(&alpha()).await.expect("ages read");
        assert_eq!(
            served, held,
            "the ages served after the write are the ones the store holds now"
        );
        assert!(
            served.keys["colour"] > late.keys["colour"],
            "the late reader carried the pre-write age and it was served: \
             late {late:?}, served {served:?}"
        );
    }

    /// **A held image says whether a page holds prose, and the store is not
    /// asked.** One person has a page and one has none; both are answered from
    /// the image after the boot fills it.
    #[tokio::test]
    async fn has_prose_is_answered_from_the_image() {
        let counted = Arc::new(crate::counted::Counted::new(Arc::new(
            InMemoryMemory::booted(),
        )));
        let folded = Folded::new(counted.clone());
        let milhouse = EntityId::person("person:milhouse");
        let nelson = EntityId::person("person:nelson");
        for (id, name) in [(&milhouse, "Milhouse"), (&nelson, "Nelson")] {
            counted
                .add_entity(NewEntity::new(id.clone(), name, "user-named"))
                .await
                .expect("add ok")
                .written()
                .expect("not blocked");
        }
        counted
            .set_prose(&milhouse, "Milhouse lives next door.", None)
            .await
            .expect("the page is set");
        folded.rebuild().await.expect("the image fills");
        counted.reset();

        assert!(folded.has_prose(&milhouse).await.expect("has_prose ok"));
        assert!(!folded.has_prose(&nelson).await.expect("has_prose ok"));
        assert_eq!(
            counted.calls(),
            0,
            "the store is not asked: {:?}",
            counted.seen()
        );
    }

    /// 🚨 **One write's own store work must not grow with how much is
    /// already stored.** Before `Folded::scan_entity` forwarded, it fell to
    /// [`Memory::scan_entity`]'s own default: read every entity's every fact
    /// through `scan`, then keep one row and discard the rest — a cost that
    /// grows with the whole store regardless of which entity was asked
    /// about. The count that proves it: how many times the whole-corpus
    /// `scan` ran to answer one `scan_entity` call, asked once against a
    /// small corpus and once against a large one. A count that does not move
    /// is the proof, and here it does not even have to move off zero.
    #[tokio::test]
    async fn scan_entity_does_not_pay_for_the_whole_corpus() {
        async fn scan_calls_for_one_entity(handles: &[&str]) -> usize {
            let inner = Arc::new(InMemoryMemory::booted());
            for handle in handles {
                let new = NewEntity::new(
                    EntityId::person(*handle),
                    format!("Person {handle}"),
                    "user-named",
                );
                match inner.add_entity(new.clone()).await.expect("add ok") {
                    Guarded::Written(_) => {}
                    Guarded::Blocked {
                        attempted,
                        candidates,
                    } => {
                        let token =
                            jojobot_domain::memory::guard::override_token(&attempted, &candidates);
                        inner
                            .add_entity(NewEntity {
                                override_token: Some(token),
                                override_reason: Some("a fixture sharing a name".into()),
                                ..new
                            })
                            .await
                            .expect("add ok")
                            .written()
                            .expect("the override clears the near-miss");
                    }
                }
            }
            let scan_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
            let counted = Arc::new(CountingScan {
                inner,
                scan_calls: scan_calls.clone(),
            });
            let folded = Folded::new(counted);
            folded
                .scan_entity(&EntityId::person("person:alpha"))
                .await
                .expect("scan_entity ok");
            scan_calls.load(std::sync::atomic::Ordering::SeqCst)
        }

        const SMALL: &[&str] = &["person:alpha", "person:beta"];
        const LARGE: &[&str] = &[
            "person:alpha",
            "person:beta",
            "person:gamma",
            "person:kappa",
            "person:delta",
            "person:epsilon",
            "person:zeta",
            "person:eta",
            "person:theta",
            "person:iota",
        ];
        let small = scan_calls_for_one_entity(SMALL).await;
        let large = scan_calls_for_one_entity(LARGE).await;
        assert_eq!(
            small,
            large,
            "one write's store work must not depend on how many things are already stored: \
             {small} whole-corpus scans at {} entities, {large} at {}",
            SMALL.len(),
            LARGE.len(),
        );
        assert_eq!(
            small, 0,
            "scan_entity must not read the whole corpus to answer about one entity: {small} calls",
        );
    }

    /// **Two writes reach the image in the order they reached the store.**
    ///
    /// A writes `due_on = A`; its re-read stalls. While it is stalled, B tries
    /// to write `due_on = B`. B must wait for A's write and A's re-read to
    /// finish: if it ran through, A's older snapshot could land after B's and
    /// the image would serve the earlier value for ever, with the store
    /// holding the later one. Only after A is released does B write, so the
    /// store and the image both end at `B`.
    #[tokio::test]
    async fn a_second_write_waits_for_the_first_ones_reread_so_the_image_ends_at_the_later_value() {
        let inner = Arc::new(InMemoryMemory::booted());
        inner
            .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");

        let stalled = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(tokio::sync::Notify::new());
        let double = Arc::new(StallFirstVersionedRead {
            inner: inner.clone(),
            written: std::sync::atomic::AtomicU64::new(0),
            stalled: stalled.clone(),
            released: released.clone(),
            first: std::sync::atomic::AtomicBool::new(false),
        });
        let folded = Arc::new(Folded::new(double.clone()));
        folded.rebuild().await.expect("the image fills");
        double
            .first
            .store(true, std::sync::atomic::Ordering::SeqCst);

        let a_folded = folded.clone();
        let a_task = tokio::spawn(async move {
            a_folded
                .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                    fields: BTreeMap::from([("due_on".to_string(), "A".to_string())]),
                    ..NewFact::about(alpha(), "A writes first", date(2026, 9, 1))
                }))
                .await
        });

        // A's write has landed and its re-read is holding a snapshot open.
        stalled.notified().await;

        let b_folded = folded.clone();
        let b_task = tokio::spawn(async move {
            b_folded
                .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                    fields: BTreeMap::from([("due_on".to_string(), "B".to_string())]),
                    ..NewFact::about(
                        alpha(),
                        "B writes second, while A is stalled",
                        date(2026, 9, 2),
                    )
                }))
                .await
        });
        for _ in 0..50 {
            tokio::task::yield_now().await;
        }
        assert!(
            !b_task.is_finished(),
            "B ran through while A's re-read was still open"
        );
        assert_eq!(
            inner
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"A".to_string()),
            "B has not reached the store while A holds the gate"
        );

        released.notify_one();
        a_task
            .await
            .expect("the task did not panic")
            .expect("capture ok")
            .written()
            .expect("not blocked");
        b_task
            .await
            .expect("the task did not panic")
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"B".to_string()),
            "the image ends at the later write"
        );
        assert_eq!(
            inner
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"B".to_string()),
            "and so does the store"
        );
    }

    /// **Declaring a fold changes what already-cached writes answer, not
    /// only writes made afterwards.** `folded_fields` interprets a thing's
    /// existing writes against the CURRENT declarations every time it runs;
    /// a cache that answered from a snapshot taken under the old
    /// declaration would go on answering wrongly for ever, since nothing
    /// ever touches this entity's own key again to trigger a normal
    /// refresh.
    ///
    /// The worked example: write, write, read (newest-write-wins, since
    /// nothing is declared), declare `laps` a counter, read again (the SAME
    /// two writes now sum).
    #[tokio::test]
    async fn declaring_a_counter_changes_what_the_same_two_writes_already_fold_to() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
                ..NewFact::about(alpha(), "lap one", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
                ..NewFact::about(alpha(), "lap two", date(2026, 9, 2))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("laps"),
            Some(&"1".to_string()),
            "undeclared, laps folds newest-write-wins: the second write alone",
        );

        folded
            .declare_type(
                types::DeclaredType::new("running", vec![types::Field::summing("laps")]),
                None,
            )
            .await
            .expect("declare ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("laps"),
            Some(&"2".to_string()),
            "declared a counter, the same two writes now sum to two",
        );
    }

    /// **The store resolves a stale handle through its own rename history;
    /// the cache answered first, and answered wrong.** Read once under a
    /// handle, rename, write again under the new one, then ask by the OLD
    /// handle again: the answer must be what the thing holds NOW, not a
    /// photograph of what it held the moment before the rename — with no
    /// error and no notice, since the miss this relies on is silent by
    /// design.
    #[tokio::test]
    async fn a_rename_drops_the_old_handles_entry_so_it_answers_current_data() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "before".to_string())]),
                ..NewFact::about(alpha(), "before the rename", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        // Read under the old handle before the rename — the same read a
        // caller holding it would make, and what populates the cache entry
        // this fix has to drop.
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"before".to_string()),
        );

        let gamma = EntityId::person("person:gamma");
        folded
            .rename_entity(
                &alpha(),
                &gamma,
                None,
                date(2026, 9, 2),
                jojobot_domain::memory::Override::none(),
                None,
            )
            .await
            .expect("rename ok")
            .written()
            .expect("not blocked");

        // A write under the CURRENT handle, after the rename.
        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "after".to_string())]),
                ..NewFact::about(gamma.clone(), "after the rename", date(2026, 9, 3))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"after".to_string()),
            "the old handle must resolve through history to CURRENT data, not the snapshot the \
             cache took before the rename",
        );
    }

    /// **A rename leaves no cached copy of a field that names the old handle.**
    /// A field naming a thing is served under the handle the thing answers to
    /// now, and the fold caches what it served, so the copy cached for the thing
    /// that holds the field still says the old spelling after the rename. The
    /// fold drops it, which is why a read of a thing's fields needs nothing
    /// further to resolve a reference.
    #[tokio::test]
    async fn a_rename_drops_the_cached_fields_that_name_the_old_handle() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner.clone());
        folded
            .declare_type(
                DeclaredType::new(
                    "fold-pointing",
                    vec![Field::new("friend", ValueType::Reference)],
                ),
                None,
            )
            .await
            .expect("the type is declared");
        let gamma = EntityId::person("person:gamma");
        let holder = EntityId::person("person:kenny");
        for (id, name) in [(&alpha(), "Alpha"), (&holder, "Kenny")] {
            seeded(&folded, id, name).await;
        }
        folded
            .capture(jojobot_domain::memory::testing::contract::state(NewFact {
                fields: BTreeMap::from([("friend".to_string(), alpha().to_string())]),
                ..NewFact::about(holder.clone(), "has a friend", date(2026, 9, 1))
            }))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        // The holder is read once, so it has a cached copy to be dropped.
        assert_eq!(
            folded
                .fields(&holder)
                .await
                .expect("fields ok")
                .get("friend"),
            Some(&alpha().to_string()),
        );

        folded
            .rename_entity(
                &alpha(),
                &gamma,
                None,
                date(2026, 9, 2),
                jojobot_domain::memory::Override::none(),
                None,
            )
            .await
            .expect("rename ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&holder)
                .await
                .expect("fields ok")
                .get("friend"),
            Some(&gamma.to_string()),
            "the holder's cached copy still names the handle the friend wore before the rename"
        );
    }
}
