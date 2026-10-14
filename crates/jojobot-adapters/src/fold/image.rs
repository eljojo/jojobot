//! [`Image`] — everything the memory verbs read, held in RAM (decision log 292).
//!
//! jojobot is one server instance and the only writer, so the image can be the
//! answer to a read rather than a cache in front of one. It is filled from the
//! store's own reads, **never from arithmetic of its own**: a value held here is
//! the value the store above it gave when it was asked, and a second copy of
//! the fold that learned to disagree with the first is the defect this avoids.
//! What is held per thing is a [`Doc`]; what is held for all things is the
//! listing, the rename history, the declared types and the kinds.
//!
//! A miss is answered here as well — an unknown handle is the same
//! [`MemoryError::UnknownEntity`], with the same candidates, that the store
//! gives — so a read of something that is not there makes no call either.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use jojobot_domain::memory::claim_walk::{ClaimMove, MergeLoop, follow_claim};
use jojobot_domain::memory::numbered::{
    NUMBERS_BY, derived_formers, nearest_numbers, numbered_handle,
};
use jojobot_domain::memory::{
    ClaimWrite, Entity, EntityId, EntityKind, EntityVersion, Fact, FactAddress, FactId, FieldAges,
    FieldBacking, FormerHandle, KeyTrails, Memory, MemoryError, guard, names_in_fields,
    search::DocScan,
    types::{DeclaredType, Origin},
};

/// How many things are read from the store at once while the image fills.
const LOAD_WIDTH: usize = 8;

/// What the image holds for one thing, each part as the store answered it.
#[derive(Clone)]
pub(super) struct Doc {
    /// The thing's document: its row, its prose, its records and its fields.
    /// `recall` answers with its records: a scan and a recall are one read of
    /// the same rows, which the battery asserts on every store.
    pub scan: DocScan,
    /// `fields_versioned` of the thing: what it holds, and the marker of the
    /// writes behind it.
    pub fields: BTreeMap<String, String>,
    pub version: u64,
    /// `field_ages` of the thing.
    pub ages: FieldAges,
    /// `claim_histories` of the thing.
    pub histories: HashMap<FactId, Vec<ClaimWrite>>,
    /// `backing` of the thing.
    pub backing: BTreeMap<String, FieldBacking>,
    /// `history` of every key and `keys_kept_off_the_thing` of every record.
    pub trails: KeyTrails,
    /// **The writes of each claim this thing cannot serve**, read when the
    /// document is filled. The claim sits under a folded key and no event maps
    /// it, so no ordinary history holds it; its address reads it. A claim whose
    /// chain could not be read is absent here, and a read of it goes to the
    /// store, which answers as it can.
    pub unmapped_histories: HashMap<FactAddress, Vec<ClaimWrite>>,
    /// **Every version of the thing's own row**, the store's `entity_history`:
    /// the name, aliases, prose and archive state it held before each change, the
    /// session that made it, and the versions of every thing folded into it.
    /// Read when the document is filled and read again with it, so a write to the
    /// thing keeps it current as a claim write keeps the claims. `None` when the
    /// store could not answer at the fill, and a read of it goes to the store.
    pub entity_versions: Option<Vec<EntityVersion>>,
}

impl Doc {
    /// Read everything held for one thing, given its scanned document.
    async fn read(inner: &dyn Memory, scan: DocScan) -> Result<Doc, MemoryError> {
        let handle = scan
            .entity
            .as_ref()
            .map(|e| e.id.clone())
            .ok_or_else(|| MemoryError::Store("a scanned document names no thing".into()))?;
        let (fields, version) = inner.fields_versioned(&handle).await?;
        let ages = inner.field_ages(&handle).await?;
        let histories = inner.claim_histories(&handle).await?;
        let backing = inner.backing(&handle).await?;
        let trails = inner.key_trails(&handle).await?;
        let entity_versions = inner.entity_history(&handle).await.ok();
        let mut unmapped_histories = HashMap::new();
        for claim in &scan.unmapped {
            if let Ok(chain) = inner.claim_history(&claim.address).await {
                unmapped_histories.insert(claim.address.clone(), chain);
            }
        }
        Ok(Doc {
            scan,
            fields,
            version,
            ages,
            histories,
            backing,
            trails,
            unmapped_histories,
            entity_versions,
        })
    }

    /// The document of one thing, read from the store: the incremental half of
    /// [`Image::load`], taken after a write that touched only this thing.
    pub async fn load(inner: &dyn Memory, handle: &EntityId) -> Result<Doc, MemoryError> {
        let Some(scan) = inner.scan_entity(handle).await? else {
            return Err(MemoryError::Store(format!(
                "{handle} was written and then read back as nothing"
            )));
        };
        Doc::read(inner, scan).await
    }
}

/// The whole image.
pub(super) struct Image {
    entities: Vec<Entity>,
    former: Vec<FormerHandle>,
    /// **The handles the held numbers make**, served as former handles and
    /// derived from the documents held, so a write that moves a number moves
    /// its handle with it. Never read from the store: it is the rename history
    /// alone that `former` holds.
    numbered: Vec<FormerHandle>,
    types: Vec<DeclaredType>,
    kinds: Vec<(String, Origin)>,
    docs: HashMap<EntityId, Doc>,
    /// Position in `entities` of the thing wearing each handle now.
    by_id: HashMap<EntityId, usize>,
    /// Position of the first thing wearing each badge — the tier a former
    /// handle resolves through, taken in listing order as the port's own
    /// `resolve_handle` takes it.
    by_badge: HashMap<String, usize>,
    /// **Who points at what, as the store answers it.** `referring_to` for every
    /// name some record's fields point at. The store answers from a table of the
    /// links its writes drew, which is narrower than "every record whose fields
    /// name it" — a record it never drew a link for is not in its answer — so the
    /// answer is asked of the store and held, and not worked out here.
    referring: HashMap<String, Vec<Fact>>,
    /// **What stands on what.** The claim each record was worked out from, to
    /// the records worked out from it.
    built_on: HashMap<(String, String), Vec<(EntityId, FactId)>>,
    /// **Every merge made, as the store appended it**, with the handles as they
    /// are served at the moment of the fill. A rename or a merge reads the whole
    /// image again, so the handles are as current as the image.
    events: Vec<jojobot_domain::memory::MergeEvent>,
}

impl Image {
    /// Fill the image from a full read of the store — the startup path.
    pub async fn load(inner: &Arc<dyn Memory>) -> Result<Image, MemoryError> {
        let entities = inner.list_entities(None).await?;
        let former = inner.former_handles().await?;
        let types = inner.declared_types().await?;
        let kinds = inner.declared_kinds().await?;
        let mut scans: HashMap<EntityId, DocScan> = inner
            .scan()
            .await?
            .into_iter()
            .filter_map(|doc| Some((doc.entity.as_ref()?.id.clone(), doc)))
            .collect();

        let mut docs = HashMap::with_capacity(entities.len());
        let mut pending = entities.iter();
        loop {
            let mut batch = tokio::task::JoinSet::new();
            for entity in pending.by_ref().take(LOAD_WIDTH) {
                let Some(scan) = scans.remove(&entity.id) else {
                    return Err(MemoryError::Store(format!(
                        "{} is listed but the store's scan holds no document for it",
                        entity.id
                    )));
                };
                let inner = inner.clone();
                let id = entity.id.clone();
                batch.spawn(async move { (id, Doc::read(&*inner, scan).await) });
            }
            if batch.is_empty() {
                break;
            }
            while let Some(done) = batch.join_next().await {
                let (id, doc) = done.map_err(|e| MemoryError::Store(e.to_string()))?;
                docs.insert(id, doc?);
            }
        }
        let mut image = Image::assembled(entities, former, types, kinds, docs);
        let names = image.names_in(&image.docs.values().collect::<Vec<_>>());
        image.referring = ask_referring(inner, names).await?.into_iter().collect();
        image.events = inner.merge_events(None).await?;
        Ok(image)
    }

    fn assembled(
        entities: Vec<Entity>,
        former: Vec<FormerHandle>,
        types: Vec<DeclaredType>,
        kinds: Vec<(String, Origin)>,
        docs: HashMap<EntityId, Doc>,
    ) -> Image {
        // The store's list of former handles carries the numbered ones it
        // derived at the moment of the read; the image derives its own, from the
        // documents it holds, so they stay current as the documents change.
        let former = former
            .into_iter()
            .filter(|f| numbered_handle(&f.former).is_none())
            .collect();
        let mut image = Image {
            entities,
            former,
            numbered: Vec::new(),
            types,
            kinds,
            docs,
            by_id: HashMap::new(),
            by_badge: HashMap::new(),
            referring: HashMap::new(),
            built_on: HashMap::new(),
            events: Vec::new(),
        };
        image.reindex();
        image.index_docs();
        image.derive_numbered();
        image
    }

    fn reindex(&mut self) {
        self.by_id.clear();
        self.by_badge.clear();
        for (at, entity) in self.entities.iter().enumerate() {
            self.by_id.entry(entity.id.clone()).or_insert(at);
            if let Some(badge) = &entity.badge {
                self.by_badge.entry(badge.clone()).or_insert(at);
            }
        }
    }

    /// Take in one thing's re-read document and the listing read beside it.
    /// `None` for the listing leaves it as it was.
    pub fn install(
        &mut self,
        entities: Option<Vec<Entity>>,
        doc: Doc,
        referring: Vec<(String, Vec<Fact>)>,
    ) {
        if let Some(handle) = doc.scan.entity.as_ref().map(|e| e.id.clone()) {
            self.docs.insert(handle, doc);
        }
        if let Some(entities) = entities {
            self.entities = entities;
            self.reindex();
            let live: std::collections::HashSet<&EntityId> =
                self.entities.iter().map(|e| &e.id).collect();
            self.docs.retain(|handle, _| live.contains(handle));
        }
        self.index_docs();
        self.derive_numbered();
        self.referring.extend(referring);
    }

    /// **Each thing's folded fields by handle**, as the documents hold them.
    fn fields_by_handle(&self) -> BTreeMap<EntityId, BTreeMap<String, String>> {
        self.docs
            .iter()
            .map(|(handle, doc)| (handle.clone(), doc.fields.clone()))
            .collect()
    }

    /// Derive the handles the held numbers make. A pass over the documents,
    /// paid by a write and never by a read, and skipped altogether while no
    /// thing numbers its members.
    fn derive_numbered(&mut self) {
        if !self
            .docs
            .values()
            .any(|doc| doc.fields.contains_key(NUMBERS_BY))
        {
            self.numbered.clear();
            return;
        }
        self.numbered = derived_formers(&self.entities, &self.fields_by_handle());
    }

    /// Rebuild the two lookups from the documents held. A single pass over the
    /// records held, paid by a write and never by a read.
    fn index_docs(&mut self) {
        self.built_on.clear();
        for entity in &self.entities {
            let Some(doc) = self.docs.get(&entity.id) else {
                continue;
            };
            for fact in &doc.scan.facts {
                if let Some(source) = &fact.derived_from {
                    self.built_on
                        .entry((source.home.0.clone(), source.local.0.clone()))
                        .or_default()
                        .push((entity.id.clone(), fact.id.clone()));
                }
            }
        }
    }

    /// **Every name the records of these documents point at**, which are the
    /// names `referring_to` has to be asked again after a write to them.
    fn names_in(&self, docs: &[&Doc]) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        for doc in docs {
            for fact in &doc.scan.facts {
                names.extend(names_in_fields(&fact.fields, &self.types));
            }
        }
        names.into_iter().collect()
    }

    /// The names to ask the store about after `doc` replaces what is held for
    /// its thing: those the old records named and those the new ones name.
    pub fn names_touched_by(&self, doc: &Doc) -> Vec<String> {
        let old = doc.scan.entity.as_ref().and_then(|e| self.docs.get(&e.id));
        let mut both: Vec<&Doc> = vec![doc];
        both.extend(old);
        self.names_in(&both)
    }

    // ── what a read asks ────────────────────────────────────────────────────

    /// The thing a handle answers to: the handle itself first, then the handle
    /// it used to wear, exactly as the port's `resolve_handle` goes.
    fn entity(&self, id: &EntityId) -> Option<&Entity> {
        if let Some(at) = self.by_id.get(id) {
            return self.entities.get(*at);
        }
        let badge = &self
            .former
            .iter()
            .chain(self.numbered.iter())
            .find(|f| &f.former == id)?
            .badge;
        self.by_badge
            .get(badge)
            .and_then(|at| self.entities.get(*at))
    }

    /// The miss, as the store gives it: an unknown thing with what it nearly
    /// was.
    fn unknown(&self, id: &EntityId) -> MemoryError {
        MemoryError::UnknownEntity {
            attempted: id.to_string(),
            nearest: nearest_numbers(id, &self.entities, &self.fields_by_handle())
                .filter(|held| !held.is_empty())
                .unwrap_or_else(|| guard::screen(id, &[], &self.entities)),
        }
    }

    pub fn doc(&self, id: &EntityId) -> Result<&Doc, MemoryError> {
        self.entity(id)
            .and_then(|entity| self.docs.get(&entity.id))
            .ok_or_else(|| self.unknown(id))
    }

    /// How many things the image holds.
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    pub fn list(&self, kind: Option<EntityKind>) -> Vec<Entity> {
        self.entities
            .iter()
            .filter(|e| kind.is_none_or(|k| e.kind == k))
            .cloned()
            .collect()
    }

    pub fn former(&self) -> Vec<FormerHandle> {
        self.former
            .iter()
            .chain(self.numbered.iter())
            .cloned()
            .collect()
    }

    pub fn types(&self) -> Vec<DeclaredType> {
        self.types.clone()
    }

    pub fn kinds(&self) -> Vec<(String, Origin)> {
        self.kinds.clone()
    }

    /// `scan`: every document, in the listing's order.
    pub fn scan(&self) -> Vec<DocScan> {
        self.entities
            .iter()
            .filter_map(|e| self.docs.get(&e.id))
            .map(|doc| doc.scan.clone())
            .collect()
    }

    /// `scan_entity`: the document of the thing wearing exactly this handle,
    /// which is how the store's own row lookup asks it.
    pub fn scan_one(&self, id: &EntityId) -> Option<DocScan> {
        let at = self.by_id.get(id)?;
        let entity = self.entities.get(*at)?;
        self.docs.get(&entity.id).map(|doc| doc.scan.clone())
    }

    /// **The claims of a thing that cannot be served**, as the store named them
    /// when it filled the image: stored under a folded key, listed in no event.
    pub fn unmapped(&self, id: &EntityId) -> Vec<jojobot_domain::memory::unmapped::UnmappedClaim> {
        self.scan_one(id)
            .map(|doc| doc.unmapped)
            .unwrap_or_default()
    }

    /// **Whether some thing names this address as a claim it cannot serve.**
    pub fn names_as_unmapped(&self, address: &FactAddress) -> bool {
        self.docs.values().any(|doc| {
            doc.scan
                .unmapped
                .iter()
                .any(|claim| &claim.address == address)
        })
    }

    /// **The writes of a claim some thing cannot serve**, as they were read when
    /// that thing's document was filled. `None` when no thing names the address,
    /// or its chain could not be read then.
    pub fn unmapped_history(&self, address: &FactAddress) -> Option<Vec<ClaimWrite>> {
        self.docs
            .values()
            .find_map(|doc| doc.unmapped_histories.get(address))
            .cloned()
    }

    /// **The bots that have written a thought into `thread`**, off the records
    /// the image holds: a claim on a bot that draws a `connection` edge at the
    /// thread, whatever has become of the claim. A bot that is archived writes
    /// nothing, so it is not a writer. A thread nothing answers to, and one
    /// folded into another, have none; the thread is found as every other read
    /// finds a thing, by its handle or one it wore.
    pub fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        jojobot_domain::memory::validate_subject(thread)?;
        let Some(thread) = self.entity(thread) else {
            return Ok(Vec::new());
        };
        if thread.merged_into.is_some() {
            return Ok(Vec::new());
        }
        // **In the order the store keeps its rows**: by the key each bot is
        // filed under, which is its badge, or its handle when it has none.
        let mut bots: Vec<&Entity> = self
            .entities
            .iter()
            .filter(|entity| entity.kind == EntityKind::BOT && entity.archived.is_none())
            .collect();
        bots.sort_by_key(|bot| bot.badge.as_deref().unwrap_or(bot.id.as_str()));
        let mut writers: Vec<EntityId> = Vec::new();
        for bot in bots {
            let Some(doc) = self.docs.get(&bot.id) else {
                continue;
            };
            for writer in jojobot_domain::memory::writers_in(&thread.id, &doc.scan.facts) {
                if !writers.contains(&writer) {
                    writers.push(writer);
                }
            }
        }
        Ok(writers)
    }

    /// `entity_history`: the versions of one thing's own row as they were read
    /// when its document was filled, or the miss the store gives for a handle
    /// that names nothing. `Ok(None)` is a thing whose versions could not be read
    /// then.
    pub fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Option<Vec<EntityVersion>>, MemoryError> {
        Ok(self.doc(entity)?.entity_versions.clone())
    }

    /// `claim_history`: the writes of one claim, or the miss the store gives —
    /// a claim that stands has at least one write, so none is no record.
    pub fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        let doc = self.doc(&address.home)?;
        if let Some(chain) = doc.histories.get(&address.local)
            && !chain.is_empty()
        {
            return Ok(chain.clone());
        }
        if let Some(resolved) = self.entity(&address.home)
            && let Some(err) = jojobot_domain::memory::already_merged(&address.home, resolved)
        {
            return Err(err);
        }
        Err(MemoryError::UnknownFact {
            attempted: address.to_string(),
            nearest: doc
                .scan
                .facts
                .iter()
                .map(|f| f.address().to_string())
                .collect(),
        })
    }

    /// `history`: every write of one key, oldest first; a key nobody wrote on a
    /// thing that exists is an empty list.
    pub fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, MemoryError> {
        Ok(self
            .doc(entity)?
            .trails
            .history
            .get(key)
            .cloned()
            .unwrap_or_default())
    }

    /// `keys_kept_off_the_thing`.
    pub fn kept_off(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        Ok(self
            .doc(&address.home)?
            .trails
            .kept_off
            .get(&address.local)
            .cloned()
            .unwrap_or_default())
    }

    /// `referring_to`: the records whose fields name `target`, as the store
    /// answered it. A name no record's fields point at has no entry, and the
    /// store's answer for it is the empty list.
    pub fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        jojobot_domain::memory::validate_subject(target)?;
        Ok(self
            .referring
            .get(target.as_str())
            .cloned()
            .unwrap_or_default())
    }

    /// `built_on`: the records worked out from one claim, in the order of the
    /// storage key each is filed under, then by id.
    /// Every merge made, oldest first, or only those that folded `entity` away or
    /// took it in.
    pub fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Vec<jojobot_domain::memory::MergeEvent> {
        let Some(entity) = entity else {
            return self.events.clone();
        };
        let Some(wanted) = self.entity(entity).map(|e| e.id.clone()) else {
            return Vec::new();
        };
        self.events
            .iter()
            .filter(|e| e.folded == wanted || e.survivor == wanted)
            .cloned()
            .collect()
    }

    /// **Where an address went when its thing was merged away**, followed
    /// through a survivor merged later; `None` when no event moved it.
    pub fn moved(&self, address: &FactAddress) -> Result<Option<FactAddress>, MergeLoop> {
        let Some(home) = self.entity(&address.home).map(|entity| entity.id.clone()) else {
            return Ok(None);
        };
        let moved = follow_claim::<_, MergeLoop>(home, address.local.clone(), |home, local| {
            Ok(self
                .events
                .iter()
                .enumerate()
                .filter(|(_, event)| &event.folded == home)
                .filter_map(|(order, event)| {
                    event
                        .claims
                        .iter()
                        .find(|(was, _)| was == local)
                        .map(|(_, now)| ClaimMove {
                            order: order as u64,
                            survivor: event.survivor.clone(),
                            survivor_id: now.clone(),
                        })
                })
                .collect())
        })?;
        Ok(moved.map(|(survivor, now)| FactAddress::new(survivor, now)))
    }

    pub fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        jojobot_domain::memory::validate_subject(&source.home)?;
        let home = self
            .entity(&source.home)
            .map(|e| e.id.clone())
            .unwrap_or_else(|| source.home.clone());
        let Some(standing) = self.built_on.get(&(home.0, source.local.0.clone())) else {
            return Ok(Vec::new());
        };
        let mut rows: Vec<(String, &FactId, &Fact)> = Vec::new();
        for (holder, id) in standing {
            if let Some(fact) = self
                .docs
                .get(holder)
                .and_then(|doc| doc.scan.facts.iter().find(|f| &f.id == id))
            {
                rows.push((self.storage_key(holder), id, fact));
            }
        }
        rows.sort_by(|a, b| (&a.0, a.1).cmp(&(&b.0, b.1)));
        Ok(rows.into_iter().map(|(_, _, fact)| fact.clone()).collect())
    }

    /// The key a thing's rows are filed under: its badge, or its handle when it
    /// wears none.
    fn storage_key(&self, handle: &EntityId) -> String {
        self.by_id
            .get(handle)
            .and_then(|at| self.entities.get(*at))
            .and_then(|e| e.badge.clone())
            .unwrap_or_else(|| handle.0.clone())
    }

    /// `backing`.
    pub fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        Ok(self.doc(entity)?.backing.clone())
    }

    /// `key_trails`.
    pub fn trails(&self, entity: &EntityId) -> Result<KeyTrails, MemoryError> {
        Ok(self.doc(entity)?.trails.clone())
    }
}

/// **`referring_to` of each name, asked of the store**, a few at a time. A name
/// that is no well-formed handle is skipped: nothing can ask for it, and the
/// store would refuse the question.
pub(super) async fn ask_referring(
    inner: &Arc<dyn Memory>,
    names: Vec<String>,
) -> Result<Vec<(String, Vec<Fact>)>, MemoryError> {
    let names: Vec<String> = names
        .into_iter()
        .filter(|name| jojobot_domain::memory::validate_subject(&EntityId(name.clone())).is_ok())
        .collect();
    let mut asked = Vec::with_capacity(names.len());
    for chunk in names.chunks(LOAD_WIDTH) {
        let mut batch = tokio::task::JoinSet::new();
        for name in chunk {
            let inner = inner.clone();
            let name = name.clone();
            batch.spawn(async move {
                let answer = inner.referring_to(&EntityId(name.clone())).await;
                (name, answer)
            });
        }
        while let Some(done) = batch.join_next().await {
            let (name, answer) = done.map_err(|e| MemoryError::Store(e.to_string()))?;
            asked.push((name, answer?));
        }
    }
    Ok(asked)
}
