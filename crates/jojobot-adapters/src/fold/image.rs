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
use jojobot_domain::memory::claim_walk::{ClaimMove, MergeLoop, follow_claim};
use jojobot_domain::memory::filter::{Direction, Filter, Test};
use jojobot_domain::memory::holders::{OrderedHead, head_of};
use jojobot_domain::memory::numbered::{
    NUMBERS_BY, derived_formers, nearest_numbers, numbered_handle,
};
use jojobot_domain::memory::{
    ClaimWrite, Entity, EntityId, EntityKind, EntityVersion, Fact, FactAddress, FactId, FieldAges,
    FieldBacking, FormerHandle, KeyTrails, Memory, MemoryError, guard,
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
    docs: HashMap<EntityId, Doc>,
    /// Position in `entities` of the thing wearing each handle now.
    by_id: HashMap<EntityId, usize>,
    /// Position of the first thing wearing each badge — the tier a former
    /// handle resolves through, taken in listing order as the port's own
    /// `resolve_handle` takes it.
    by_badge: HashMap<String, usize>,
    /// **Who points at what, as the store answers it.** `referring_to` for every
    /// name some record's fields point at, holders in the order
    /// [`jojobot_domain::memory::referring_holder_order`] names. The store answers
    /// from a table of the links its writes drew, which is narrower than "every
    /// record whose fields name it" — a record it never drew a link for is not in
    /// its answer — so the list is asked of the store when it is filled and patched
    /// by a write to a thing's own claims, a holder at a time, in that order.
    referring: HashMap<String, Vec<Fact>>,
    /// **What stands on what.** The claim each record was worked out from, to
    /// the records worked out from it.
    built_on: HashMap<(String, String), Vec<(EntityId, FactId)>>,
    /// **What the documents hold, by key** (see [`Held`]). Kept current a
    /// document at a time, and filled again with the image.
    held: Held,
    /// **Every merge made, as the store appended it**, with the handles as they
    /// are served at the moment of the fill. A rename or a merge reads the events
    /// again (see [`patch`]), so the handles are as current as the image.
    events: Vec<jojobot_domain::memory::MergeEvent>,
}

impl Image {
    /// Fill the image from a full read of the store — the startup path.
    pub async fn load(
        inner: &Arc<dyn Memory>,
        reader: Option<&dyn BootReader>,
    ) -> Result<Image, MemoryError> {
        let entities = inner.list_entities(None).await?;
        let former = inner.former_handles().await?;
        let types = inner.declared_types().await?;
        let kinds = inner.declared_kinds().await?;
        // **The documents come from the reader when the store has one**, a thing at
        // a time only for those it leaves out; otherwise the store scans them all.
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

        // **As many things at once as the store answers at once**, never a
        // number of the fold's own, so a fill does not queue on the store's
        // connections.
        // **What the store can answer for many things at once is asked once.** A
        // thing it leaves out is read the way it always was.
        let mut parts = match reader {
            Some(reader) => {
                let handles: Vec<EntityId> = entities.iter().map(|e| e.id.clone()).collect();
                Parts::read(reader, &handles).await?
            }
            None => Parts::default(),
        };
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
                batch.spawn(async move { (id, Doc::read(&*inner, scan, taken).await) });
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
            types: std::sync::Arc::new(types),
            kinds,
            docs,
            by_id: HashMap::new(),
            by_badge: HashMap::new(),
            referring: HashMap::new(),
            built_on: HashMap::new(),
            held: Held::default(),
            events: Vec::new(),
        };
        image.reindex();
        image.index_docs();
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
    /// key and the records worked out from a claim take off what the old document
    /// held and put on what the new one holds, so a write costs the thing it
    /// touched and no more.
    pub fn install(
        &mut self,
        entities: Option<Vec<Entity>>,
        doc: Doc,
        referring: Vec<(String, Vec<Fact>)>,
        patch_rest: Option<&Wrote>,
    ) {
        let patch = if let Some(wrote) = patch_rest {
            let asked: Vec<&str> = referring.iter().map(|(name, _)| name.as_str()).collect();
            self.referring_patch(&doc, &asked, wrote)
        } else {
            None
        };
        self.take_in_doc(doc);
        if let Some(entities) = entities {
            self.take_in_listing(entities);
        }
        self.derive_numbered();
        self.referring.extend(referring);
        self.apply_referring_patch(patch);
    }

    /// **The names the patch cannot answer for**: a name an engine record of the
    /// thing points at. The stores disagree on whether such a record is a holder
    /// (a merge's account of what it folded is one in the fake and none in the
    /// real store), so the store is asked for that name.
    pub fn names_to_ask(&self, doc: &Doc) -> Vec<String> {
        self.names_touched_by(doc)
            .into_iter()
            .filter(|name| {
                let target = EntityId(name.clone());
                jojobot_domain::memory::validate_subject(&target).is_ok()
                    && doc.scan.facts.iter().any(|fact| {
                        fact.fields
                            .contains_key(jojobot_domain::memory::capacity::ENGINE_RECORD)
                            && jojobot_domain::memory::fact_names_target(fact, &self.types, &target)
                    })
            })
            .collect()
    }

    /// **What the store would answer for each name this thing's records point at,
    /// worked out from the thing's own old and new documents.** For each such
    /// name: the thing's records whose fields name it now. Taken before the new
    /// document replaces the old one, which is what names the old records' names.
    ///
    /// Every name some record points at already has a list, because the image
    /// asks for each one when it fills and this patch keeps the rule: a name the
    /// thing's new records introduce has no other holder.
    fn referring_patch(&self, doc: &Doc, asked: &[&str], wrote: &Wrote) -> Option<ReferringPatch> {
        let handle = doc.scan.entity.as_ref()?.id.clone();
        let names: Vec<String> = self
            .names_touched_by(doc)
            .into_iter()
            .filter(|name| {
                !asked.contains(&name.as_str())
                    && jojobot_domain::memory::validate_subject(&EntityId(name.clone())).is_ok()
            })
            .collect();
        // **The store lists a thing as a holder of a name when a write of its own
        // drew a link to it, under the types declared at that write, and then
        // lists each of its records that names the target under today's types.**
        // The image keeps no write time, so the thing is a holder when it is on
        // the list already or when a key this write SENT names the target now. The
        // keys come from the write, never from a comparison of the records: an
        // edit that sends the same tags again links, and an edit that sends only
        // another field does not. A declaration made since changes no list.
        let patch = names
            .into_iter()
            .map(|name| {
                let target = EntityId(name.clone());
                let on =
                    jojobot_domain::memory::link_rule() == jojobot_domain::memory::LinkRule::On;
                let names_it = |fact: &Fact| {
                    jojobot_domain::memory::fact_names_target(fact, &self.types, &target)
                };
                let sent_it = |fact: &Fact| {
                    wrote.iter().any(|(id, keys)| {
                        *id == fact.id
                            && keys.iter().any(|key| match on {
                                true => fact
                                    .field_links
                                    .get(key.trim())
                                    .is_some_and(|ids| ids.contains(&target)),
                                false => fact.fields.get(key.trim()).is_some_and(|value| {
                                    jojobot_domain::memory::fields_name_target(
                                        &BTreeMap::from([(key.trim().to_string(), value.clone())]),
                                        &self.types,
                                        &target,
                                    )
                                }),
                            })
                    })
                };
                let holder = self
                    .referring
                    .get(&name)
                    .is_some_and(|list| list.iter().any(|fact| fact.home == handle))
                    || doc.scan.facts.iter().any(sent_it);
                let holding: Vec<Fact> = match holder {
                    true => doc
                        .scan
                        .facts
                        .iter()
                        .filter(|fact| names_it(fact))
                        .cloned()
                        .collect(),
                    false => Vec::new(),
                };
                (name, holding)
            })
            .collect();
        Some((handle, patch))
    }

    /// Take the thing's records off each list they were on, and put on the ones
    /// that name the target now.
    fn apply_referring_patch(&mut self, patch: Option<ReferringPatch>) {
        let Some((handle, patch)) = patch else {
            return;
        };
        for (name, holding) in patch {
            let mut list = self.referring.remove(&name).unwrap_or_default();
            // **A list is in the holders' order**
            // ([`jojobot_domain::memory::referring_holder_order`]). A holder already
            // on the list keeps its place; a new one goes in before the first
            // holder that order puts after it.
            let key = self.storage_key(&handle);
            let at = list
                .iter()
                .position(|fact| fact.home == handle)
                .or_else(|| {
                    list.iter().position(|fact| {
                        jojobot_domain::memory::referring_holder_order(
                            &self.storage_key(&fact.home),
                            &key,
                        )
                        .is_gt()
                    })
                })
                .unwrap_or(list.len());
            list.retain(|fact| fact.home != handle);
            let at = at.min(list.len());
            list.splice(at..at, holding);
            self.referring.insert(name, list);
        }
    }

    /// Put one thing's re-read document in the place of the one held, and move
    /// what is kept beside the documents with it.
    fn take_in_doc(&mut self, doc: Doc) {
        if let Some(handle) = doc.scan.entity.as_ref().map(|e| e.id.clone()) {
            let facts = doc.scan.facts.clone();
            let fields = doc.fields.clone();
            let old = self.docs.insert(handle.clone(), doc);
            self.held
                .replace(&handle, old.as_ref().map(|old| &old.fields), &fields);
            if let Some(old) = &old {
                self.forget_built_on(&handle, old);
            }
            self.remember_built_on(&handle, &facts);
        }
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
                self.forget_built_on(&handle, &doc);
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

    /// Build the records-worked-out-from lookup from the documents held. A single
    /// pass over the records held, paid by a fill and never by a write or a read.
    fn index_docs(&mut self) {
        self.built_on.clear();
        let handles: Vec<EntityId> = self.entities.iter().map(|e| e.id.clone()).collect();
        for handle in handles {
            if let Some(facts) = self.docs.get(&handle).map(|doc| doc.scan.facts.clone()) {
                self.remember_built_on(&handle, &facts);
            }
        }
    }

    /// Put on the records of `handle` that were worked out from a claim.
    fn remember_built_on(&mut self, handle: &EntityId, facts: &[Fact]) {
        for fact in facts {
            if let Some(source) = &fact.derived_from {
                self.built_on
                    .entry((source.home.0.clone(), source.local.0.clone()))
                    .or_default()
                    .push((handle.clone(), fact.id.clone()));
            }
        }
    }

    /// Take off the records of `handle` that were worked out from a claim, as its
    /// document `old` held them. A source with no record left is dropped.
    fn forget_built_on(&mut self, handle: &EntityId, old: &Doc) {
        for fact in &old.scan.facts {
            let Some(source) = &fact.derived_from else {
                continue;
            };
            let key = (source.home.0.clone(), source.local.0.clone());
            if let Some(standing) = self.built_on.get_mut(&key) {
                standing.retain(|(holder, _)| holder != handle);
                if standing.is_empty() {
                    self.built_on.remove(&key);
                }
            }
        }
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
        )?;
        let mut built = Image::assembled(
            self.entities.clone(),
            self.former.clone(),
            self.types.as_ref().clone(),
            self.kinds.clone(),
            self.docs.clone(),
        );
        let normal = |map: &HashMap<(String, String), Vec<(EntityId, FactId)>>| {
            let mut rows: Vec<_> = map
                .iter()
                .map(|(key, standing)| {
                    let mut standing = standing.clone();
                    standing.sort();
                    (key.clone(), standing)
                })
                .collect();
            rows.sort();
            rows
        };
        built.index_docs();
        if normal(&built.built_on) == normal(&self.built_on) {
            Ok(())
        } else {
            Err("the records worked out from a claim differ from a build".into())
        }
    }

    /// **Every name the records of these documents point at**, which are the
    /// names `referring_to` has to be asked again after a write to them.
    fn names_in(&self, docs: &[&Doc]) -> Vec<String> {
        let mut names = std::collections::BTreeSet::new();
        for doc in docs {
            for fact in &doc.scan.facts {
                names.extend(jojobot_domain::memory::names_in_fact(fact, &self.types));
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

/// The keys a write sent, by the record that carried them.
pub type Wrote = Vec<(FactId, Vec<String>)>;

/// One thing's records for each name they point at, to put on that name's list.
type ReferringPatch = (EntityId, Vec<(String, Vec<Fact>)>);

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
    for chunk in names.chunks(inner.read_width().max(1)) {
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
