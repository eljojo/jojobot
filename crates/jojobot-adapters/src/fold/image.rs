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

use super::held::Held;
use jojobot_domain::memory::boot_reader::{BootReader, Parts, ThingParts};
use jojobot_domain::memory::claim_walk::{MergeLoop, follow_claim, moves_in};
use jojobot_domain::memory::filter::{Direction, Filter, Test};
use jojobot_domain::memory::holders::{OrderedHead, head_of};
use jojobot_domain::memory::numbered::{
    NUMBERS_BY, derived_formers, nearest_numbers, numbered_handle,
};
use jojobot_domain::memory::{
    ClaimWrite, Entity, EntityId, EntityKind, EntityVersion, FactAddress, FactId, FieldAges,
    FieldBacking, FormerHandle, KeyTrails, Memory, MemoryError, guard,
    kinds::KindSet,
    search::DocScan,
    types::{DeclaredType, Origin, ValueType},
};

pub(super) mod patch;

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
    /// Read everything held for one thing, given its scanned document. A part
    /// the bulk reads already hold for the thing is taken from them; any other
    /// part is read of the thing on its own.
    async fn read(
        inner: &dyn Memory,
        scan: DocScan,
        taken: ThingParts,
    ) -> Result<Doc, MemoryError> {
        let handle = scan
            .entity
            .as_ref()
            .map(|e| e.id.clone())
            .ok_or_else(|| MemoryError::Store("a scanned document names no thing".into()))?;
        let (fields, version) = match taken.fields {
            Some(held) => held,
            None => inner.fields_versioned(&handle).await?,
        };
        let ages = match taken.ages {
            Some(held) => held,
            None => inner.field_ages(&handle).await?,
        };
        let histories = match taken.histories {
            Some(held) => held,
            None => inner.claim_histories(&handle).await?,
        };
        let backing = match taken.backing {
            Some(held) => held,
            None => inner.backing(&handle).await?,
        };
        let trails = match taken.trails {
            Some(held) => held,
            None => inner.key_trails(&handle).await?,
        };
        let entity_versions = match taken.versions {
            Some(held) => Some(held),
            None => inner.entity_history(&handle).await.ok(),
        };
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

    /// **The document of one thing, read from the claims State alone**: the
    /// domain's scan and value parts of the thing stored under `object`. `None`
    /// when the State holds no such thing.
    pub fn from_state(
        state: &jojobot_domain::log::claims::State,
        object: &jojobot_domain::log::ObjectId,
        declared: &[DeclaredType],
    ) -> Option<Doc> {
        let scan = state.document_scan(object, declared)?;
        let parts = state.document_parts(object, declared)?;
        Some(Doc {
            scan,
            fields: parts.fields,
            version: parts.version,
            ages: parts.ages,
            histories: parts.histories,
            backing: parts.backing,
            trails: parts.trails,
            unmapped_histories: parts.unmapped_histories,
            entity_versions: Some(parts.entity_versions),
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
        Doc::read(inner, scan, ThingParts::default()).await
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
    types: std::sync::Arc<Vec<DeclaredType>>,
    kinds: Vec<(String, Origin)>,
    /// The same kinds as a set, asked at every admission the image answers.
    kind_set: KindSet,
    docs: HashMap<EntityId, Doc>,
    /// Position in `entities` of the thing wearing each handle now.
    by_id: HashMap<EntityId, usize>,
    /// Position of the first thing wearing each badge — the tier a former
    /// handle resolves through, taken in listing order as the port's own
    /// `resolve_handle` takes it.
    by_badge: HashMap<String, usize>,
    /// **What the documents hold, by key** (see [`Held`]). Kept current a
    /// document at a time, and filled again with the image.
    held: Held,
    /// **Every merge made, as the store appended it**, with the handles as they
    /// are served at the moment of the fill. A rename or a merge reads the events
    /// again (see [`patch`]), so the handles are as current as the image.
    events: Vec<jojobot_domain::memory::MergeEvent>,
}

/// **One phase of the fill, named with the time it took and how many it held.** The
/// fill is the longest step of a boot and used to log as one line, so a restart
/// that took minutes could not say which read took them. The count is what grows
/// with the store: things listed, documents read, names asked about, merges held.
fn fill_phase(phase: &'static str, started: std::time::Instant, count: usize) {
    tracing::info!(
        phase,
        count,
        elapsed_ms = started.elapsed().as_millis() as u64,
        "fold fill phase finished"
    );
}

impl Image {
    /// Fill the image from a full read of the store — the startup path.
    pub async fn load(
        inner: &Arc<dyn Memory>,
        reader: Option<&dyn BootReader>,
    ) -> Result<Image, MemoryError> {
        let started = std::time::Instant::now();
        let entities = inner.list_entities(None).await?;
        let former = inner.former_handles().await?;
        let types = inner.declared_types().await?;
        let kinds = inner.declared_kinds().await?;
        fill_phase("lists", started, entities.len());
        // **The documents come from the reader when the store has one**, a thing at
        // a time only for those it leaves out; otherwise the store scans them all.
        let started = std::time::Instant::now();
        let mut scans: HashMap<EntityId, DocScan> = match reader {
            Some(reader) => {
                let handles: Vec<EntityId> = entities.iter().map(|e| e.id.clone()).collect();
                reader.scans_many(&handles).await?
            }
            None => inner
                .scan()
                .await?
                .into_iter()
                .filter_map(|doc| Some((doc.entity.as_ref()?.id.clone(), doc)))
                .collect(),
        };

        fill_phase("documents", started, scans.len());

        // **As many things at once as the store answers at once**, never a
        // number of the fold's own, so a fill does not queue on the store's
        // connections.
        // **What the store can answer for many things at once is asked once.** A
        // thing it leaves out is read the way it always was.
        let started = std::time::Instant::now();
        let mut parts = match reader {
            Some(reader) => {
                let handles: Vec<EntityId> = entities.iter().map(|e| e.id.clone()).collect();
                Parts::read(reader, &handles).await?
            }
            None => Parts::default(),
        };
        fill_phase("bulk-parts", started, entities.len());
        let started = std::time::Instant::now();
        let width = inner.read_width().max(1);
        let mut docs = HashMap::with_capacity(entities.len());
        let mut pending = entities.iter();
        loop {
            let mut batch = tokio::task::JoinSet::new();
            for entity in pending.by_ref().take(width) {
                let held = match (scans.remove(&entity.id), reader) {
                    (Some(scan), _) => Some(scan),
                    // A thing the reader left out is scanned on its own.
                    (None, Some(_)) => inner.scan_entity(&entity.id).await?,
                    (None, None) => None,
                };
                let Some(scan) = held else {
                    return Err(MemoryError::Store(format!(
                        "{} is listed but the store's scan holds no document for it",
                        entity.id
                    )));
                };
                let inner = inner.clone();
                let id = entity.id.clone();
                let taken = parts.take(&id);
                batch.spawn(crate::writer::sql::inherit_read(async move {
                    (id, Doc::read(&*inner, scan, taken).await)
                }));
            }
            if batch.is_empty() {
                break;
            }
            while let Some(done) = batch.join_next().await {
                let (id, doc) = done.map_err(|e| MemoryError::Store(e.to_string()))?;
                docs.insert(id, doc?);
            }
        }
        fill_phase("per-thing-reads", started, docs.len());
        let started = std::time::Instant::now();
        let mut image = Image::assembled(entities, former, types, kinds, docs);
        fill_phase("assembly", started, image.docs.len());
        let started = std::time::Instant::now();
        image.events = inner.merge_events(None).await?;
        fill_phase("merge-events", started, image.events.len());
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
            types: std::sync::Arc::new(types),
            kind_set: KindSet::new(kinds.clone()),
            kinds,
            docs,
            by_id: HashMap::new(),
            by_badge: HashMap::new(),
            held: Held::default(),
            events: Vec::new(),
        };
        image.reindex();
        image.hold_documents();
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
    ///
    /// **What is kept beside the documents moves with them**: the values held by
    /// key take off what the old document held and put on what the new one holds,
    /// so a write costs the thing it touched and no more.
    pub fn install(&mut self, entities: Option<Vec<Entity>>, doc: Doc) {
        self.take_in_doc(doc);
        if let Some(entities) = entities {
            self.take_in_listing(entities);
        }
        self.derive_numbered();
    }

    /// Put one thing's re-read document in the place of the one held, and move
    /// what is kept beside the documents with it.
    fn take_in_doc(&mut self, doc: Doc) {
        if let Some(handle) = doc.scan.entity.as_ref().map(|e| e.id.clone()) {
            let fields = doc.fields.clone();
            let old = self.docs.insert(handle.clone(), doc);
            self.held
                .replace(&handle, old.as_ref().map(|old| &old.fields), &fields);
        }
    }

    /// **The things this image lists that `listed` does not**, in the order the
    /// image holds them.
    pub(super) fn entities_missing_from(&self, listed: &[Entity]) -> Vec<Entity> {
        let named: std::collections::HashSet<&EntityId> = listed.iter().map(|e| &e.id).collect();
        self.entities
            .iter()
            .filter(|held| !named.contains(&held.id))
            .cloned()
            .collect()
    }

    /// **The rows this image holds for the handles `wanted` names**, in the order
    /// the image holds them. A record the build supplies is listed with the build's
    /// name by the claims State and with the operator's name and aliases laid over
    /// it by the layer above the store; the image holds the second.
    pub(super) fn held_rows(&self, wanted: impl Fn(&EntityId) -> bool) -> Vec<Entity> {
        self.entities
            .iter()
            .filter(|held| wanted(&held.id))
            .cloned()
            .collect()
    }

    /// Put the listing read in the place of the one held, and let go of the
    /// documents of the things it no longer lists.
    fn take_in_listing(&mut self, entities: Vec<Entity>) {
        self.entities = entities;
        self.reindex();
        let live: std::collections::HashSet<&EntityId> =
            self.entities.iter().map(|e| &e.id).collect();
        let gone: Vec<EntityId> = self
            .docs
            .keys()
            .filter(|handle| !live.contains(handle))
            .cloned()
            .collect();
        for handle in gone {
            if let Some(doc) = self.docs.remove(&handle) {
                self.held.remove(&handle, &doc.fields);
            }
        }
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

    /// Fill what is kept by key from the documents held.
    fn hold_documents(&mut self) {
        let mut held = Held::declaring(&self.types);
        for (handle, doc) in &self.docs {
            held.add(handle, &doc.fields);
        }
        self.held = held;
    }

    /// **Whether what is kept beside the documents is what a build over them would
    /// make**, for the case that holds the upkeep to the full scan.
    #[cfg(test)]
    pub fn upkeep_is_what_a_build_would_make(&self) -> Result<(), String> {
        self.held.is_what_a_build_would_make(
            &self.types,
            self.docs.iter().map(|(handle, doc)| (handle, &doc.fields)),
        )
    }

    // ── who holds a key, and in what order ──────────────────────────────────

    /// Whether a thing stands in a default browse and is of `kind`, when one is
    /// named.
    fn browsable_of(&self, id: &EntityId, kind: Option<EntityKind>) -> bool {
        self.by_id
            .get(id)
            .and_then(|at| self.entities.get(*at))
            .is_some_and(|entity| entity.browsable() && kind.is_none_or(|k| entity.kind == k))
    }

    /// `keys_holding`: the things a leaf keeps, in handle order. A leaf the maps
    /// kept by key do not hold is asked of the documents.
    pub fn keys_holding(&self, key: &str, test: &Test, kind: Option<EntityKind>) -> Vec<EntityId> {
        let key = key.trim();
        match self.held.holding(key, test) {
            Some(held) => held
                .into_iter()
                .filter(|id| self.browsable_of(id, kind))
                .collect(),
            None => {
                let leaf = Filter::leaf(Some(key), test.clone());
                let mut held: Vec<EntityId> = self
                    .docs
                    .iter()
                    .filter(|(id, doc)| self.browsable_of(id, kind) && leaf.holds(&doc.fields))
                    .map(|(id, _)| (*id).clone())
                    .collect();
                held.sort();
                held
            }
        }
    }

    /// `ordered_first_k`: the front of the order of a key. A key no type declares
    /// as `holds` is placed from the documents.
    pub fn ordered_first_k(
        &self,
        key: &str,
        holds: ValueType,
        direction: Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> OrderedHead {
        use jojobot_domain::memory::filter::Orderable;
        let key = key.trim();
        let among: Option<std::collections::HashSet<&EntityId>> =
            among.map(|among| among.iter().collect());
        let counts = |id: &EntityId| {
            self.browsable_of(id, kind) && among.as_ref().is_none_or(|among| among.contains(id))
        };
        let held_by = |id: &EntityId| -> String {
            self.docs
                .get(id)
                .and_then(|doc| doc.fields.get(key))
                .cloned()
                .unwrap_or_default()
        };
        match self.held.front(key, holds, direction, k, counts) {
            Some(front) => OrderedHead {
                head: front
                    .placed
                    .into_iter()
                    .map(|(id, _)| {
                        let held = held_by(&id);
                        (id, held)
                    })
                    .collect(),
                cut: front.cut,
            },
            None => head_of(
                self.docs
                    .iter()
                    .filter(|(id, _)| counts(id))
                    .filter_map(|(id, doc)| {
                        let held = doc.fields.get(key)?;
                        Some((
                            Orderable::read_to_order(holds, held)?,
                            (*id).clone(),
                            held.clone(),
                        ))
                    })
                    .collect(),
                direction,
                k,
            ),
        }
    }

    // ── what a read asks ────────────────────────────────────────────────────

    /// The thing a handle answers to: the handle itself first, then the handle
    /// it used to wear, exactly as the port's `resolve_handle` goes, and a chain
    /// of numbers read through every collection by the same walk.
    fn entity(&self, id: &EntityId) -> Option<&Entity> {
        jojobot_domain::memory::numbered::resolve_walk(id, &|id| {
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
        })
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

    /// **Whether a declaration changes what any thing held reads as.** The keys
    /// looked at are those the replaced type and the incoming one name. A key
    /// no thing and no record holds reads the same under any declaration, so it
    /// is skipped. A key some thing holds reads the same when its fold and
    /// whether any declaration calls it a reference are the same before and
    /// after: the fold sets what a thing holds, and a reference sets how the
    /// store serves its value. How an ordered type places a value is built again
    /// from the documents in [`Image::declare`].
    pub fn declaration_changes_a_read(&self, incoming: &DeclaredType) -> bool {
        let mut after: Vec<DeclaredType> = self
            .types
            .iter()
            .filter(|held| held.name != incoming.name)
            .cloned()
            .collect();
        after.push(incoming.clone());
        let meaning = |key: &str, declared: &[DeclaredType]| {
            let reference = declared
                .iter()
                .filter_map(|d| d.field(key))
                .any(|f| f.holds == ValueType::Reference);
            // The store splits the value of a list of references per item, so
            // a key that becomes a list of references is served differently.
            let list_of_references = declared
                .iter()
                .filter_map(|d| d.field(key))
                .any(|f| f.holds == ValueType::Reference && f.list);
            (
                jojobot_domain::memory::types::fold_of(key, declared),
                reference,
                list_of_references,
            )
        };
        self.types
            .iter()
            .filter(|held| held.name == incoming.name)
            .chain(std::iter::once(incoming))
            .flat_map(|d| d.fields.iter().map(|f| f.key.as_str()))
            .any(|key| self.holds_key(key) && meaning(key, &self.types) != meaning(key, &after))
    }

    /// Whether any thing or any record held carries `key`.
    fn holds_key(&self, key: &str) -> bool {
        self.docs.values().any(|doc| {
            doc.fields.contains_key(key)
                || doc
                    .scan
                    .facts
                    .iter()
                    .any(|fact| fact.fields.contains_key(key))
        })
    }

    /// **Take in a declaration that [`Image::declaration_changes_a_read`] said
    /// changes no read.** The type takes the place of the one of its name, or
    /// joins the list in name order as the store lists them, and what is kept by
    /// key is built again from the documents already held, so no read of the
    /// store follows.
    pub fn declare(&mut self, written: DeclaredType) {
        let mut types: Vec<DeclaredType> = self
            .types
            .iter()
            .filter(|held| held.name != written.name)
            .cloned()
            .collect();
        types.push(written);
        types.sort_by(|a, b| a.name.cmp(&b.name));
        self.types = std::sync::Arc::new(types);
        self.hold_documents();
    }

    pub fn types(&self) -> Vec<DeclaredType> {
        self.types.as_ref().clone()
    }

    /// **The declared types, shared**: a read holds the image's own list and
    /// copies nothing. A new image holds a new list, so a reader keeps the one it
    /// was given for as long as it reads.
    pub fn types_shared(&self) -> std::sync::Arc<Vec<DeclaredType>> {
        self.types.clone()
    }

    pub fn kind_set(&self) -> KindSet {
        self.kind_set.clone()
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

    /// `has_prose`: whether the document `scan_one` would return holds a page,
    /// answered without copying it.
    pub fn has_prose(&self, id: &EntityId) -> bool {
        self.by_id
            .get(id)
            .and_then(|at| self.entities.get(*at))
            .and_then(|entity| self.docs.get(&entity.id))
            .is_some_and(|doc| !doc.scan.prose.trim().is_empty())
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
        jojobot_domain::memory::validate_subject_in(&self.kind_set, thread)?;
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
        let moved =
            follow_claim::<_, MergeLoop>(home, address.local.clone(), moves_in(&self.events))?;
        Ok(moved.map(|(survivor, now)| FactAddress::new(survivor, now)))
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
