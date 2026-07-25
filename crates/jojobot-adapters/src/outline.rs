//! The Outline store — the real [`Memory`] adapter.
//!
//! jojobot IS a schema layer over markdown docs: Outline is the typed document
//! store, and this adapter reads and writes the `### ⚙ facts` table at the
//! bottom of a per-entity doc (the codec lives in [`codec`]).
//!
//! **Convention over configuration.** The adapter is never handed an Outline id.
//! Its only config is credentials. It discovers its own collection *by name*
//! (a software constant, default `jojobot`) and by an ownership marker so it
//! never adopts a user's same-named collection; it resolves each entity's doc by
//! the doc's durable embedded `id:` marker — not the user-renamable title; and a
//! concurrent double-create self-heals to one canonical (the oldest) rather than
//! forking. Everything is discovered/created at runtime; nothing authoritative
//! lives in jojobot's process.
//!
//! The HTTP surface is behind the [`api::OutlineApi`] port, so all of that logic
//! runs under fast tests against an in-memory double.

mod api;
mod codec;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, Guarded, Memory, MemoryError,
    NewEntity, NewFact, apply_entity_patch, apply_fact_patch, normalize_content, normalize_details,
    validate_content, validate_details, validate_edge, validate_entity, validate_subject,
    guard::{self, Decision},
    search::DocScan,
};

use api::{CollectionRec, DocRec, HttpOutline, OutlineApi, Unconfigured};
use codec::{
    next_fact_id, parse_entity, parse_facts_table, parse_id_marker, parse_prose, render_fact_row,
    seeded_doc, with_fact_appended, with_frontmatter_replaced, with_row_replaced,
};

/// Outline's page cap for list endpoints. The store pages until a short page, so
/// a match past the first page is never missed (a stop-at-100 bug forks docs).
const PAGE: u64 = 100;

/// The marker jojobot stamps into a collection's description on create and
/// checks on match, so it only ever adopts a collection it created — never a
/// user's own same-named one.
const OWNER_TAG: &str = "[jojobot:owned]";

// --- secret -----------------------------------------------------------------

/// An API token that never prints itself. `Debug` redacts, so the token can't
/// leak through a `#[derive(Debug)]`, a `dbg!`, or a `tracing` field.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Wrap a secret value.
    pub fn new(value: impl Into<String>) -> Self {
        Secret(value.into())
    }

    /// Borrow the raw value — only at the point it's actually used (the bearer
    /// header). Deliberately named so call sites are greppable.
    fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(\"***\")")
    }
}

// --- config -----------------------------------------------------------------

/// The store's only configuration: **credentials**. No collection id, no doc id
/// — those are discovered by convention. `Debug` is safe: the token redacts.
#[derive(Debug, Clone)]
pub struct OutlineConfig {
    /// Outline base URL, e.g. `https://wiki.example.org` (no trailing slash).
    pub base_url: String,
    /// API token (bearer). Redacted in `Debug`.
    pub token: Secret,
}

// --- the store --------------------------------------------------------------

/// The real Memory adapter, fronting an Outline collection it manages by name.
/// Stateless: it holds an API client and the collection *name*, never an id or a
/// fact.
#[derive(Clone)]
pub struct OutlineStore {
    api: Arc<dyn OutlineApi>,
    collection: String,
}

impl OutlineStore {
    /// The collection jojobot manages by default. A software constant — jojobot
    /// creates and owns this collection; it never touches the user's own.
    pub const DEFAULT_COLLECTION: &'static str = "jojobot";

    /// A store pointed at Outline via credentials, managing the default
    /// `jojobot` collection.
    pub fn new(http: reqwest::Client, config: OutlineConfig) -> Self {
        Self::with_collection(http, config, Self::DEFAULT_COLLECTION)
    }

    /// A store managing a named collection (e.g. `jojobot-test` for the gated
    /// integration test). jojobot only ever creates/manages its own collections.
    pub fn with_collection(
        http: reqwest::Client,
        config: OutlineConfig,
        collection: impl Into<String>,
    ) -> Self {
        let api = Arc::new(HttpOutline::new(http, config.base_url, config.token));
        Self::from_api(api, collection)
    }

    /// A store with no credentials yet — every verb returns
    /// [`MemoryError::NotConfigured`]. Lets the server boot (and keep serving
    /// `ping`) before Outline is wired, without shipping a toy store.
    pub fn unconfigured() -> Self {
        Self::from_api(Arc::new(Unconfigured), Self::DEFAULT_COLLECTION)
    }

    fn from_api(api: Arc<dyn OutlineApi>, collection: impl Into<String>) -> Self {
        Self {
            api,
            collection: collection.into(),
        }
    }

    /// The description jojobot stamps on a collection it creates.
    fn owner_description(&self) -> String {
        format!("Managed by jojobot — do not edit by hand. {OWNER_TAG}")
    }

    /// Every collection that is both named ours AND carries the ownership tag —
    /// paged in full.
    async fn owned_collections(&self) -> Result<Vec<CollectionRec>, MemoryError> {
        let mut owned = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.api.list_collections(offset, PAGE).await?;
            let count = page.len() as u64;
            owned.extend(
                page.into_iter()
                    .filter(|c| c.name == self.collection && c.description.contains(OWNER_TAG)),
            );
            if count < PAGE {
                break;
            }
            offset += PAGE;
        }
        Ok(owned)
    }

    /// The id of jojobot's collection, creating it if absent. After a create it
    /// re-lists and picks the canonical (oldest) owned collection, so a
    /// concurrent double-create converges to one rather than forking.
    async fn resolve_collection(&self) -> Result<String, MemoryError> {
        if let Some(c) = pick_oldest(self.owned_collections().await?, |c| &c.created_at, |c| &c.id) {
            return Ok(c.id);
        }
        self.api
            .create_collection(&self.collection, &self.owner_description())
            .await?;
        pick_oldest(self.owned_collections().await?, |c| &c.created_at, |c| &c.id)
            .map(|c| c.id)
            .ok_or_else(|| MemoryError::Store("collection missing after create".into()))
    }

    /// Every doc in the collection — paged in full. A match past the first page
    /// is never missed (a stop-at-100 bug forks docs).
    async fn all_docs(&self, collection_id: &str) -> Result<Vec<DocRec>, MemoryError> {
        let mut docs = Vec::new();
        let mut offset = 0;
        loop {
            let page = self.api.list_documents(collection_id, offset, PAGE).await?;
            let count = page.len() as u64;
            docs.extend(page);
            if count < PAGE {
                break;
            }
            offset += PAGE;
        }
        Ok(docs)
    }

    /// Every doc whose embedded `id:` marker is `subject`. Resolution keys on
    /// the marker, never the title, so a renamed doc is never orphaned.
    async fn entity_docs(
        &self,
        collection_id: &str,
        subject: &EntityId,
    ) -> Result<Vec<DocRec>, MemoryError> {
        Ok(self
            .all_docs(collection_id)
            .await?
            .into_iter()
            .filter(|d| parse_id_marker(&d.text).as_deref() == Some(subject.as_str()))
            .collect())
    }

    /// The entity index the write guard screens against: one entity per handle,
    /// the canonical (oldest) doc winning where a double-create left two.
    async fn entity_index(&self, collection_id: &str) -> Result<Vec<Entity>, MemoryError> {
        let mut docs = self.all_docs(collection_id).await?;
        docs.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));
        let mut seen = std::collections::HashSet::new();
        Ok(docs
            .iter()
            .filter_map(|d| parse_entity(&d.text))
            .filter(|e| seen.insert(e.id.clone()))
            .collect())
    }

    /// The canonical doc for an entity, or `None` if it has none.
    async fn entity_doc(
        &self,
        collection_id: &str,
        subject: &EntityId,
    ) -> Result<Option<DocRec>, MemoryError> {
        Ok(pick_oldest(
            self.entity_docs(collection_id, subject).await?,
            |d| &d.created_at,
            |d| &d.id,
        ))
    }

    /// Create an entity's doc and return the canonical one afterwards —
    /// re-listing so a concurrent double-create converges on the oldest rather
    /// than forking. The title is the human's handle on the doc and is purely
    /// cosmetic; the marker inside is what resolves it.
    async fn create_entity_doc(
        &self,
        collection_id: &str,
        entity: &Entity,
    ) -> Result<DocRec, MemoryError> {
        let title = if entity.name.trim().is_empty() {
            entity.id.to_string()
        } else {
            entity.name.clone()
        };
        self.api
            .create_document(collection_id, &title, &seeded_doc(entity))
            .await?;
        self.entity_doc(collection_id, &entity.id)
            .await?
            .ok_or_else(|| MemoryError::Store("entity doc missing after create".into()))
    }

    /// Read one addressed fact back through the read path — the verification
    /// half of every fact write.
    ///
    /// **An address must identify exactly one row.** Taking the first match
    /// would make read-back theatre: it would happily confirm a write that had
    /// landed on the wrong one of two rows sharing an id. If a doc somehow holds
    /// a duplicate, that is a corrupt page and the write says so rather than
    /// picking a winner.
    async fn read_back_fact(&self, address: &FactAddress) -> Result<Fact, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        let facts = match self.entity_doc(&collection_id, &address.home).await? {
            None => Vec::new(),
            Some(doc) => parse_facts_table(&doc.text),
        };
        let mut matching = facts.into_iter().filter(|f| f.id == address.local);
        let seen = matching
            .next()
            .ok_or_else(|| MemoryError::Store(format!("fact {address} did not read back")))?;
        if matching.next().is_some() {
            return Err(MemoryError::Store(format!(
                "fact {address} is ambiguous: its doc holds more than one row with that id"
            )));
        }
        Ok(seen)
    }

    /// Read an entity back through the read path — the verification half of
    /// every entity write.
    async fn read_entity(
        &self,
        collection_id: &str,
        id: &EntityId,
    ) -> Result<Entity, MemoryError> {
        self.entity_doc(collection_id, id)
            .await?
            .and_then(|d| parse_entity(&d.text))
            .ok_or_else(|| MemoryError::Store(format!("entity {id} did not read back")))
    }
}

/// The deterministic canonical winner: oldest by `created_at`, ties broken by
/// `id`. Both are stable across list calls, so every session agrees.
fn pick_oldest<T>(
    mut items: Vec<T>,
    created_at: impl Fn(&T) -> &String,
    id: impl Fn(&T) -> &String,
) -> Option<T> {
    items.sort_by(|a, b| created_at(a).cmp(created_at(b)).then_with(|| id(a).cmp(id(b))));
    items.into_iter().next()
}

#[async_trait]
impl Memory for OutlineStore {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        validate_entity(&new.id, &new.name, &new.aliases, &new.source, new.crm.as_deref())?;
        let collection_id = self.resolve_collection().await?;

        let index = self.entity_index(&collection_id).await?;
        if let Decision::Block(candidates) =
            guard::decide(&new.id, &new.labels(), &index, new.create_new)
        {
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
            boot: new.boot,
        };
        self.create_entity_doc(&collection_id, &entity).await?;

        // Read-back: the entity is only added once the read path returns it.
        let seen = self.read_entity(&collection_id, &entity.id).await?;
        if seen != entity {
            return Err(MemoryError::Store(format!(
                "entity {} read back changed: wrote {entity:?}, read {seen:?}",
                entity.id
            )));
        }
        Ok(Guarded::Written(seen))
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        Ok(self
            .entity_index(&collection_id)
            .await?
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
        let collection_id = self.resolve_collection().await?;
        let index = self.entity_index(&collection_id).await?;

        let Some(doc) = self.entity_doc(&collection_id, handle).await? else {
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, &[], &index),
            });
        };
        let mut entity = parse_entity(&doc.text)
            .ok_or_else(|| MemoryError::Store(format!("doc for {handle} lost its marker")))?;

        // A rename is an entity-touching write, so it faces the same gate a
        // creation does — otherwise the guard is side-steppable: add under a
        // throwaway name, then rename onto the collision.
        if let Some(new_name) = &patch.name
            && let Decision::Block(candidates) =
                guard::decide_rename(handle, new_name, &entity.name, &index, patch.create_new)
        {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates,
            });
        }
        apply_entity_patch(&mut entity, &patch)?;

        let updated = with_frontmatter_replaced(&doc.text, &entity);
        self.api.update_document(&doc.id, &updated).await?;

        let seen = self.read_entity(&collection_id, handle).await?;
        if seen != entity {
            return Err(MemoryError::Store(format!(
                "entity {handle} read back changed: wrote {entity:?}, read {seen:?}"
            )));
        }
        Ok(Guarded::Written(seen))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        validate_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;
        if let Some(edge) = &fact.edge {
            validate_edge(edge)?;
        }
        let collection_id = self.resolve_collection().await?;

        // Every entity this write names must already exist — the subject first,
        // then the edge's object. **Nothing here provisions.** A novel subject
        // used to spawn a nameless doc, so every typo and every plausible handle
        // an AI produced became a permanent entity nobody chose.
        let index = self.entity_index(&collection_id).await?;
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

        // The gate passed, so the entity is in the index — and an entity is in
        // the index because a doc carries its marker. A miss here is a store
        // that changed under us mid-write, not a subject to provision.
        let doc = self
            .entity_doc(&collection_id, &fact.subject)
            .await?
            .ok_or_else(|| {
                MemoryError::Store(format!("entity {} lost its doc mid-write", fact.subject))
            })?;

        // Read-modify-write, and the lost-update hazard it carries. Outline has
        // no atomic append or compare-and-set, so EVERY write here — this
        // `capture`, `update_fact`, and `update_entity` — reads the doc, then
        // PUTs a whole new document body built from that snapshot. A concurrent
        // write between the read and the PUT is erased: an already-verified
        // fact can vanish because a later updater rewrote the doc it never saw.
        //
        // **Read-back does not detect this.** Each verb re-reads only the row or
        // the entity it just wrote, and that one always looks right; nothing
        // compares the rest of the document against what was there before.
        //
        // Accepted for a single-session assistant. The revision guard
        // (If-Match / document revision) is deliberately deferred, not
        // forgotten.
        //
        // The id is minted off the doc's text, not off the parsed facts: a row
        // this reader can't parse still holds its id, and handing that id out a
        // second time would alias two rows onto one address.
        let stored = Fact {
            id: next_fact_id(&doc.text),
            home: fact.subject.clone(),
            subject: fact.subject,
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
            edge: fact.edge,
        };
        let updated = with_fact_appended(&doc.text, &render_fact_row(&stored));
        self.api.update_document(&doc.id, &updated).await?;

        // Read-back: a capture succeeds only if the read path returns the fact,
        // byte-identical. Writing is not recording.
        let seen = self.read_back_fact(&stored.address()).await?;
        if seen != stored {
            return Err(MemoryError::Store(format!(
                "fact {} read back changed: wrote {stored:?}, read {seen:?}",
                stored.address()
            )));
        }
        Ok(Guarded::Written(seen))
    }

    /// **Home-doc membership counts, not only the subject column.** Every row in
    /// this entity's doc is homed here, so every row comes back — a subject cell
    /// a hand edit mistyped can hide a doc's facts from nobody, least of all from
    /// the entity whose page they sit on. Filtering on the subject column was the
    /// split brain: the entity was readable under one id and writable under
    /// another, and the rows nobody could see were the ones needing repair.
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        match self.entity_doc(&collection_id, subject).await? {
            None => Ok(Vec::new()),
            Some(doc) => Ok(parse_facts_table(&doc.text)),
        }
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        validate_subject(&address.home)?;
        let collection_id = self.resolve_collection().await?;

        // An edit that attaches an edge names an entity, so it is screened before
        // the row is rewritten — the guard is on every write path, not just create.
        if let Some(edge) = &patch.edge {
            validate_edge(edge)?;
            let index = self.entity_index(&collection_id).await?;
            if let Decision::Block(candidates) = guard::decide_existing(&edge.object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: edge.object.clone(),
                    candidates,
                });
            }
        }

        let unknown = |nearest: Vec<String>| MemoryError::UnknownFact {
            attempted: address.to_string(),
            nearest,
        };
        // A miss on the HANDLE is an entity miss, with the near candidates that
        // explain it — not a fact miss trailing an empty address list.
        let Some(doc) = self.entity_doc(&collection_id, &address.home).await? else {
            let index = self.entity_index(&collection_id).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let facts = parse_facts_table(&doc.text);
        let Some(mut fact) = facts.iter().find(|f| f.id == address.local).cloned() else {
            return Err(unknown(
                facts.iter().map(|f| f.address().to_string()).collect(),
            ));
        };
        apply_fact_patch(&mut fact, &patch)?;

        // The row is rewritten where it stands — fix the source, never an
        // addendum beside it. `with_row_replaced` targets only a row this same
        // reader can parse, so an edit can never land on a row the caller never
        // saw.
        let updated = with_row_replaced(
            &doc.text,
            &address.home,
            &address.local,
            &render_fact_row(&fact),
        )
        .ok_or_else(|| unknown(facts.iter().map(|f| f.address().to_string()).collect()))?;
        self.api.update_document(&doc.id, &updated).await?;

        let seen = self.read_back_fact(address).await?;
        if seen != fact {
            return Err(MemoryError::Store(format!(
                "fact {address} read back changed: wrote {fact:?}, read {seen:?}"
            )));
        }
        Ok(Guarded::Written(seen))
    }

    /// Every doc in the collection, whole — including docs that are **not**
    /// entities. A page the user wrote by hand carries no marker, so it is no
    /// entity and holds no facts; its prose is still worth finding, which is why
    /// it comes back rather than being filtered out here.
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        let mut docs = self.all_docs(&collection_id).await?;
        // Canonical-first, so a double-created doc's twin can't shadow it.
        docs.sort_by(|a, b| a.created_at.cmp(&b.created_at).then_with(|| a.id.cmp(&b.id)));

        let mut seen = std::collections::HashSet::new();
        Ok(docs
            .into_iter()
            .map(|doc| {
                let entity = parse_entity(&doc.text).filter(|e| seen.insert(e.id.clone()));
                DocScan {
                    doc_id: doc.id,
                    title: doc.title,
                    prose: parse_prose(&doc.text),
                    facts: match &entity {
                        Some(_) => parse_facts_table(&doc.text),
                        // No marker (or a shadowed twin) means no address, and an
                        // unaddressable fact is one nobody could ever correct.
                        None => Vec::new(),
                    },
                    entity,
                }
            })
            .collect())
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        Ok(self
            .entity_doc(&collection_id, entity)
            .await?
            .map(|doc| DocScan {
                doc_id: doc.id,
                title: doc.title,
                prose: parse_prose(&doc.text),
                facts: parse_facts_table(&doc.text),
                entity: parse_entity(&doc.text),
            }))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use jiff::civil::date;
    use jojobot_domain::memory::search::{Hit, Search, SearchQuery};
    use jojobot_domain::memory::testing::contract;

    use super::*;
    use crate::search::IndexedMemory;

    /// In-memory [`OutlineApi`] double. Ids/`created_at` are a monotonic counter
    /// (zero-padded so lexicographic = chronological) — no clock, deterministic.
    #[derive(Default)]
    struct FakeOutline {
        seq: AtomicU64,
        collections: Mutex<Vec<CollectionRec>>,
        // (collection_id, doc)
        documents: Mutex<Vec<(String, DocRec)>>,
    }

    impl FakeOutline {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        fn stamp(&self) -> String {
            format!("{:020}", self.seq.fetch_add(1, Ordering::SeqCst))
        }

        /// Pre-seed a collection; returns its id.
        fn seed_collection(&self, name: &str, description: &str) -> String {
            let s = self.stamp();
            let id = format!("col-{s}");
            self.collections.lock().unwrap().push(CollectionRec {
                id: id.clone(),
                name: name.into(),
                description: description.into(),
                created_at: s,
            });
            id
        }

        /// Pre-seed a document; returns its id.
        fn seed_document(&self, collection_id: &str, title: &str, text: &str) -> String {
            let s = self.stamp();
            let id = format!("doc-{s}");
            self.documents.lock().unwrap().push((
                collection_id.into(),
                DocRec {
                    id: id.clone(),
                    title: title.into(),
                    text: text.into(),
                    created_at: s,
                },
            ));
            id
        }

        fn rename_document(&self, id: &str, new_title: &str) {
            let mut docs = self.documents.lock().unwrap();
            let d = docs.iter_mut().find(|(_, d)| d.id == id).expect("doc exists");
            d.1.title = new_title.into();
        }

        fn collections_named(&self, name: &str) -> Vec<CollectionRec> {
            self.collections
                .lock()
                .unwrap()
                .iter()
                .filter(|c| c.name == name)
                .cloned()
                .collect()
        }

        fn owned_named(&self, name: &str) -> usize {
            self.collections_named(name)
                .iter()
                .filter(|c| c.description.contains(OWNER_TAG))
                .count()
        }

        fn docs_in(&self, collection_id: &str) -> Vec<DocRec> {
            self.documents
                .lock()
                .unwrap()
                .iter()
                .filter(|(cid, _)| cid == collection_id)
                .map(|(_, d)| d.clone())
                .collect()
        }
    }

    #[async_trait]
    impl OutlineApi for FakeOutline {
        async fn list_collections(
            &self,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<CollectionRec>, MemoryError> {
            let all = self.collections.lock().unwrap();
            Ok(all
                .iter()
                .skip(offset as usize)
                .take(limit as usize)
                .cloned()
                .collect())
        }

        async fn create_collection(
            &self,
            name: &str,
            description: &str,
        ) -> Result<CollectionRec, MemoryError> {
            let id = self.seed_collection(name, description);
            Ok(self
                .collections
                .lock()
                .unwrap()
                .iter()
                .find(|c| c.id == id)
                .cloned()
                .unwrap())
        }

        async fn list_documents(
            &self,
            collection_id: &str,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<DocRec>, MemoryError> {
            let all = self.documents.lock().unwrap();
            Ok(all
                .iter()
                .filter(|(cid, _)| cid == collection_id)
                .map(|(_, d)| d.clone())
                .skip(offset as usize)
                .take(limit as usize)
                .collect())
        }

        async fn create_document(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
        ) -> Result<DocRec, MemoryError> {
            let id = self.seed_document(collection_id, title, text);
            Ok(self
                .documents
                .lock()
                .unwrap()
                .iter()
                .find(|(_, d)| d.id == id)
                .map(|(_, d)| d.clone())
                .unwrap())
        }

        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            let mut docs = self.documents.lock().unwrap();
            match docs.iter_mut().find(|(_, d)| d.id == id) {
                Some((_, d)) => {
                    d.text = text.into();
                    Ok(())
                }
                None => Err(MemoryError::Store(format!("update_document: no doc {id}"))),
            }
        }
    }

    const COLL: &str = "jojobot-test";

    fn store(fake: Arc<FakeOutline>) -> OutlineStore {
        OutlineStore::from_api(fake, COLL)
    }

    fn owned_desc() -> String {
        format!("Managed by jojobot. {OWNER_TAG}")
    }

    /// The person entity a doc fixture is seeded for.
    fn person(handle: &str) -> Entity {
        let id = EntityId::person(handle);
        Entity {
            kind: EntityKind::Person,
            id,
            name: String::new(),
            aliases: Vec::new(),
            source: "capture".into(),
            crm: None,
            boot: Default::default(),
        }
    }

    /// Make sure a subject exists, so the write guard's existence gate is not
    /// what a spec about collections or docs trips over.
    async fn ensure(store: &OutlineStore, id: &EntityId) {
        let known = store.list_entities(None).await.expect("list_entities ok");
        if known.iter().any(|e| &e.id == id) {
            return;
        }
        store
            .add_entity(NewEntity::new(id.clone(), id.slug(), "test-fixture"))
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("a fixture entity must not be blocked");
    }

    /// Capture through the store, asserting the guard waved it through —
    /// provisioning the subject first, because every write that names an entity
    /// now requires one that exists.
    async fn capture(store: &OutlineStore, fact: NewFact) -> Fact {
        ensure(store, &fact.subject).await;
        store
            .capture(fact)
            .await
            .expect("capture should succeed")
            .written()
            .expect("the guard must not block this capture")
    }

    /// The whole real store logic (provisioning + codec) against a fake
    /// transport — the fast/CI coverage that used to exist only in the gated
    /// integration test.
    #[tokio::test]
    async fn outline_store_satisfies_the_contract() {
        contract::run_all(&store(FakeOutline::new())).await;
    }

    /// …and the same contract **including retrieval**, with the search projection
    /// over the real store logic. The fake satisfies this suite too, which is what
    /// stops the two from drifting.
    #[tokio::test]
    async fn the_indexed_outline_store_satisfies_the_whole_contract() {
        let indexed = IndexedMemory::new(Arc::new(store(FakeOutline::new()))).expect("index opens");
        contract::run_all_searchable(&indexed).await;
    }

    /// **The read-side leak, through the real reader.** A doc where the answer
    /// lives only in the prose a human typed — nobody filed it as a fact — is
    /// found, in the same list as the fact hits. This is the scenario the codec's
    /// prose reader exists for, exercised end to end: seed the page as a user
    /// would leave it, scan it through the store, search it.
    #[tokio::test]
    async fn prose_a_human_typed_is_searchable_beside_the_facts() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = with_fact_appended(
            &format!(
                "Alpha is allergic to penicillin — it came up once and never got filed.\n\n{}",
                seeded_doc(&person("alpha"))
            ),
            "| f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |  |",
        );
        fake.seed_document(&coll, "Alpha", &doc);

        let indexed = IndexedMemory::new(Arc::new(store(fake))).expect("index opens");
        assert_eq!(indexed.rebuild().await.expect("rebuild"), 1, "one doc scanned");

        let hits = indexed
            .search(&SearchQuery::text("penicillin"))
            .expect("search ok");
        let prose: Vec<&Hit> = hits.iter().filter(|h| matches!(h, Hit::Prose { .. })).collect();
        assert_eq!(prose.len(), 1, "the prose match must be findable: {hits:?}");
        let Some(Hit::Prose { entity, snippet, .. }) = prose.first().copied() else {
            unreachable!("filtered to prose");
        };
        assert_eq!(
            entity.as_ref().map(|e| &e.id),
            Some(&EntityId::person("alpha")),
            "a prose hit says whose entity doc it is"
        );
        assert!(snippet.contains("penicillin"), "got {snippet:?}");
        assert!(
            !snippet.contains("id: person:alpha"),
            "jojobot's own machine block is not prose: {snippet:?}"
        );

        // The fact in the same doc is still reachable by its own words.
        let facts = indexed.search(&SearchQuery::text("chess")).expect("search ok");
        assert!(
            facts.iter().any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
            "got {facts:?}"
        );
    }

    /// **A note typed in the gap under `### ⚙ facts` is findable.** The reader
    /// tolerates it and the writer preserves it — so before this, that text was
    /// kept forever and searchable never: it belonged to no hit class at all.
    /// The user's most likely place to leave a note was the one place the front
    /// door could not see.
    #[tokio::test]
    async fn a_note_under_the_facts_header_is_findable_as_prose() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        // Written out literally, as a user would leave the page.
        let doc = "```yaml\nid: person:alpha\nkind: person\nname: \nsource: capture\nboot: on-demand\n```\n\n\
                   ### ⚙ facts\n\nnote: the pass was closed on Tuesday\n\n\
                   | id | subject | content | details | provenance | status | date | edges |\n\
                   | --- | --- | --- | --- | --- | --- | --- | --- |\n\
                   | f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |  |\n";
        fake.seed_document(&coll, "alpha", doc);

        let indexed = IndexedMemory::new(Arc::new(store(fake))).expect("index opens");
        indexed.rebuild().await.expect("rebuild");

        let hits = indexed.search(&SearchQuery::text("pass closed")).expect("search ok");
        assert!(
            hits.iter().any(|h| matches!(h, Hit::Prose { snippet, .. } if snippet.contains("pass was closed"))),
            "the note must come back as a prose hit: {hits:?}"
        );
        // …and the fact beside it is untouched by the wider prose boundary.
        let facts = indexed.search(&SearchQuery::text("chess")).expect("search ok");
        assert!(
            facts.iter().any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
            "got {facts:?}"
        );
    }

    /// **The split brain.** A hand edit leaves a doc whose declared `id:` marker
    /// and its rows' `subject` cells disagree. Recall resolved through the rows
    /// and `list_entities` through the marker, so the entity was readable under
    /// one id and writable under another: its own facts were invisible on its own
    /// page, and every repair had to be aimed at a handle nothing else agreed on.
    ///
    /// A row homed in a doc is now reachable under the id that doc declares,
    /// full stop. A typo in the subject column can hide a doc's facts from
    /// nobody — least of all from the entity whose page they are sitting on.
    #[tokio::test]
    async fn a_docs_own_rows_are_reachable_under_the_id_it_declares() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = with_fact_appended(
            &seeded_doc(&person("alpha")),
            // The subject cell names someone else — one hand-typed character.
            "| f1 | person:alphaa | plays chess |  | testimony | active | 2026-07-01 |  |",
        );
        fake.seed_document(&coll, "alpha", &doc);
        let store = store(fake.clone());
        let alpha = EntityId::person("alpha");

        let facts = store.recall(&alpha).await.expect("recall");
        assert_eq!(facts.len(), 1, "the doc's own row must be reachable: {facts:?}");
        assert_eq!(facts[0].home, alpha, "the doc it lives in is its home");
        assert_eq!(
            facts[0].address().to_string(),
            "person:alpha#f1",
            "…and its address is the one the reader can act on"
        );

        // Reachable means repairable: the address recall handed back edits the row.
        store
            .update_fact(
                &facts[0].address(),
                FactPatch { content: Some("plays go".into()), ..Default::default() },
            )
            .await
            .expect("the row must be editable through the address recall gave")
            .written()
            .expect("not blocked");
        assert!(fake.docs_in(&coll)[0].text.contains("plays go"));
    }

    /// …and the search projection agrees: `subject: person:alpha` finds the rows
    /// on alpha's page. A projection that disagreed with recall about which
    /// entity a row belongs to is a second, quieter split brain.
    #[tokio::test]
    async fn a_subject_filter_finds_the_rows_homed_in_that_entitys_doc() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = with_fact_appended(
            &seeded_doc(&person("alpha")),
            "| f1 | person:alphaa | plays chess |  | testimony | active | 2026-07-01 |  |",
        );
        fake.seed_document(&coll, "alpha", &doc);

        let indexed = IndexedMemory::new(Arc::new(store(fake))).expect("index opens");
        indexed.rebuild().await.expect("rebuild");

        let hits = indexed
            .search(&SearchQuery {
                subject: Some(EntityId::person("alpha")),
                ..Default::default()
            })
            .expect("search ok");
        assert!(
            hits.iter().any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
            "a row homed in alpha's doc must answer a subject filter for alpha: {hits:?}"
        );
    }

    /// A doc carrying no id marker is nobody's entity — and its prose is still
    /// scanned, because a page the user wrote by hand is exactly the page worth
    /// finding. Its (absent) facts are not invented.
    #[tokio::test]
    async fn scan_returns_a_non_entity_doc_as_prose_only() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        fake.seed_document(&coll, "Trip notes", "The pass was closed on Tuesday.");

        let scanned = store(fake).scan().await.expect("scan ok");
        assert_eq!(scanned.len(), 1);
        assert_eq!(scanned[0].entity, None, "no marker, no entity");
        assert_eq!(scanned[0].prose, "The pass was closed on Tuesday.");
        assert!(scanned[0].facts.is_empty());
        assert!(!scanned[0].doc_id.is_empty(), "a scan always says which doc");
    }

    // --- hand-edited docs: the adversarial-review regressions -----------------
    //
    // These pages are user-visible wiki docs, so a retyped date or a stray note
    // is normal. Each of these scenarios used to destroy data AND report
    // success, because the write path read the doc with a looser predicate than
    // the read path did — so the verification confirmed the wrong row.

    /// A row this reader can't parse must be inert: it keeps its id (so nothing
    /// collides with it) and it is never the row an edit lands on.
    #[tokio::test]
    async fn a_hand_broken_row_is_neither_reused_nor_overwritten() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let broken = with_fact_appended(
            &seeded_doc(&person("alpha")),
            // A date a human retyped in Outline — unreadable to the parser.
            "| f1 | person:alpha | allergic to penicillin |  | testimony | active | July 1, 2026 |",
        );
        fake.seed_document(&coll, "alpha", &broken);
        let store = store(fake.clone());
        let subject = EntityId::person("alpha");

        let captured = capture(
            &store,
            NewFact::about(subject.clone(), "takes the 8am train", date(2026, 7, 2)),
        )
        .await;
        assert_eq!(
            captured.id.as_str(),
            "f2",
            "the unreadable row's id is taken, so the new fact must not reuse it"
        );

        store
            .update_fact(
                &captured.address(),
                FactPatch { content: Some("takes the 7am train".into()), ..Default::default() },
            )
            .await
            .expect("the addressed row updates");

        let text = &fake.docs_in(&coll)[0].text;
        assert!(
            text.contains("allergic to penicillin"),
            "the row the caller never saw must survive untouched: {text}"
        );
        assert!(text.contains("takes the 7am train"));
        assert!(!text.contains("takes the 8am train"), "the edit rewrote its own row");
    }

    /// An address that no readable row answers to is a miss — never a silent
    /// rewrite of the unreadable row that happens to carry that id.
    #[tokio::test]
    async fn an_address_matching_only_an_unreadable_row_is_a_miss() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let broken = with_fact_appended(
            &seeded_doc(&person("alpha")),
            "| f1 | person:alpha | allergic to penicillin |  | testimony | active | July 1, 2026 |",
        );
        fake.seed_document(&coll, "alpha", &broken);

        let err = store(fake.clone())
            .update_fact(
                &FactAddress::new(EntityId::person("alpha"), jojobot_domain::memory::FactId("f1".into())),
                FactPatch { content: Some("should not land".into()), ..Default::default() },
            )
            .await
            .expect_err("an unreadable row is not addressable");
        assert!(matches!(err, MemoryError::UnknownFact { .. }), "got {err:?}");
        assert!(
            fake.docs_in(&coll)[0].text.contains("allergic to penicillin"),
            "a missed address must write nothing"
        );
    }

    /// A note typed under the facts header must not hide the table from `recall`
    /// nor make `capture` start a second one above it.
    #[tokio::test]
    async fn a_note_under_the_facts_header_does_not_orphan_the_facts() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        // Written out literally, exactly as a user would leave the page. Building
        // this fixture with with_fact_appended would let the code under test
        // choose where the table goes, and the test would pass either way.
        let doc = "```yaml\nid: person:alpha\nkind: person\nname: \nsource: capture\nboot: on-demand\n```\n\n\
                   ### ⚙ facts\n\nnote: do not edit below\n\n\
                   | id | subject | content | details | provenance | status | date |\n\
                   | --- | --- | --- | --- | --- | --- | --- |\n\
                   | f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |\n";
        fake.seed_document(&coll, "alpha", doc);
        let store = store(fake.clone());
        let subject = EntityId::person("alpha");

        assert_eq!(
            store.recall(&subject).await.unwrap().len(),
            1,
            "the note must not hide the existing fact"
        );
        capture(&store, NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2))).await;

        let facts = store.recall(&subject).await.unwrap();
        assert_eq!(facts.len(), 2, "both facts live in the one table: {facts:?}");
        assert!(fake.docs_in(&coll)[0].text.contains("note: do not edit below"));
    }

    /// Editing an entity must rewrite jojobot's own machine block — not a fenced
    /// block the user wrote in the prose above it.
    #[tokio::test]
    async fn update_entity_leaves_a_users_own_fenced_block_alone() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = format!(
            "Prose about this entity.\n\n```\nimportant snippet the user wrote\n```\n\n{}",
            seeded_doc(&person("alpha"))
        );
        fake.seed_document(&coll, "alpha", &doc);

        let updated = store(fake.clone())
            .update_entity(
                &EntityId::person("alpha"),
                EntityPatch { name: Some("Alpha Renamed".into()), ..Default::default() },
            )
            .await
            .expect("update_entity should succeed")
            .written()
            .expect("an uncontested rename is not blocked");
        assert_eq!(updated.name, "Alpha Renamed");

        let text = &fake.docs_in(&coll)[0].text;
        assert!(
            text.contains("important snippet the user wrote"),
            "the user's own fenced block must survive: {text}"
        );
        assert_eq!(
            text.matches("id: person:alpha").count(),
            1,
            "no stale second machine block: {text}"
        );
    }

    /// A YAML snippet pasted into a doc must not take that doc's identity. When
    /// it did, the entity stopped resolving: recall went to zero, the entity
    /// vanished from the index, and the next capture forked a SECOND doc — the
    /// original facts unreachable from then on.
    #[tokio::test]
    async fn a_pasted_yaml_snippet_cannot_hijack_a_docs_identity() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = format!(
            "Prose the user wrote.\n\n```yaml\nid: my-service\nversion: 2\n```\n\n{}",
            seeded_doc(&person("alpha"))
        );
        let doc = with_fact_appended(
            &doc,
            "| f1 | person:alpha | plays chess |  | testimony | active | 2026-07-01 |",
        );
        fake.seed_document(&coll, "alpha", &doc);
        let store = store(fake.clone());
        let subject = EntityId::person("alpha");

        assert_eq!(
            store.recall(&subject).await.unwrap().len(),
            1,
            "the doc still resolves to its entity"
        );
        assert!(
            store
                .list_entities(None)
                .await
                .unwrap()
                .iter()
                .any(|e| e.id == subject),
            "the entity is still in the index"
        );

        capture(&store, NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2))).await;
        assert_eq!(fake.docs_in(&coll).len(), 1, "no second doc was forked");
        assert_eq!(store.recall(&subject).await.unwrap().len(), 2, "both facts reachable");
    }

    /// **A slice-1 page, through the real store.** Its table predates both the
    /// `provenance` and `details` columns, so every row on it read as unparseable
    /// and vanished: `recall` came back empty, and a capture landed a lone new
    /// row on a page that looked, to jojobot, like it had never held anything.
    /// The page had to be repaired by hand before the store could see it.
    #[tokio::test]
    async fn a_slice_one_page_recalls_its_facts_and_takes_a_new_one() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        // Literally as slice 1 left it: no provenance column (a trailing ❓ meant
        // inference), no details column, a blank status cell for active.
        let doc = "```yaml\nid: person:alpha\nkind: person\n```\n\n### ⚙ facts\n\n\
                   | id | subject | content | status | date | edges |\n\
                   | --- | --- | --- | --- | --- | --- |\n\
                   | f1 | person:alpha | plays go ❓ | active | 2026-07-01 |  |\n\
                   | f2 | person:alpha | speaks two languages |  | 2026-07-02 |  |\n";
        fake.seed_document(&coll, "alpha", doc);
        let store = store(fake.clone());
        let subject = EntityId::person("alpha");

        let before = store.recall(&subject).await.expect("recall");
        assert_eq!(before.len(), 2, "the page's own facts must be readable: {before:?}");

        capture(&store, NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 3))).await;

        let after = store.recall(&subject).await.expect("recall");
        assert_eq!(after.len(), 3, "the new fact lands beside the old ones: {after:?}");
        assert_eq!(after[2].id.as_str(), "f3", "the ids already on the page are taken");
        assert_eq!(fake.docs_in(&coll).len(), 1, "no second doc was forked");
    }

    #[tokio::test]
    async fn creates_an_owned_collection_when_absent() {
        let fake = FakeOutline::new();
        capture(
            &store(fake.clone()),
            NewFact::about(EntityId::person("alpha"), "x", date(2026, 7, 24)),
        )
        .await;
        assert_eq!(fake.owned_named(COLL), 1, "exactly one owned collection");
    }

    #[tokio::test]
    async fn never_adopts_a_users_unowned_same_named_collection() {
        let fake = FakeOutline::new();
        // A user's own collection that happens to share the name — no owner tag.
        let user_coll = fake.seed_collection(COLL, "my personal notes");

        capture(
            &store(fake.clone()),
            NewFact::about(EntityId::person("alpha"), "x", date(2026, 7, 24)),
        )
        .await;

        assert_eq!(fake.owned_named(COLL), 1, "jojobot made its own owned collection");
        assert!(
            fake.docs_in(&user_coll).is_empty(),
            "the user's collection must be left untouched"
        );
    }

    #[tokio::test]
    async fn reconciles_duplicate_owned_collections_to_the_oldest() {
        let fake = FakeOutline::new();
        let older = fake.seed_collection(COLL, &owned_desc());
        let _newer = fake.seed_collection(COLL, &owned_desc());

        capture(
            &store(fake.clone()),
            NewFact::about(EntityId::person("alpha"), "x", date(2026, 7, 24)),
        )
        .await;

        assert_eq!(fake.owned_named(COLL), 2, "no third collection created");
        assert_eq!(fake.docs_in(&older).len(), 1, "the fact went to the oldest");
    }

    #[tokio::test]
    async fn pages_beyond_100_collections_before_concluding_absent() {
        let fake = FakeOutline::new();
        for i in 0..120 {
            fake.seed_collection(&format!("other-{i}"), "unrelated");
        }
        // The one owned match sits past the first page.
        let owned = fake.seed_collection(COLL, &owned_desc());

        capture(
            &store(fake.clone()),
            NewFact::about(EntityId::person("alpha"), "x", date(2026, 7, 24)),
        )
        .await;

        assert_eq!(fake.owned_named(COLL), 1, "must find the paged-past match, not fork");
        assert_eq!(fake.docs_in(&owned).len(), 1);
    }

    #[tokio::test]
    async fn resolves_a_doc_by_marker_despite_an_unrelated_title() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let text = with_fact_appended(
            &seeded_doc(&person("alpha")),
            &render_fact_row(&Fact {
                id: jojobot_domain::memory::FactId("f1".into()),
                home: EntityId::person("alpha"),
                subject: EntityId::person("alpha"),
                content: "plays go".into(),
                details: None,
                provenance: jojobot_domain::memory::Provenance::Testimony,
                status: Default::default(),
                date: date(2026, 7, 1),
                edge: None,
            }),
        );
        fake.seed_document(&coll, "Totally Unrelated Title", &text);

        let facts = store(fake).recall(&EntityId::person("alpha")).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "plays go");
    }

    #[tokio::test]
    async fn a_renamed_title_does_not_orphan_or_duplicate_the_doc() {
        let fake = FakeOutline::new();
        let subject = EntityId::person("alpha");

        // First capture creates the doc.
        capture(&store(fake.clone()), NewFact::about(subject.clone(), "plays go", date(2026, 7, 1))).await;
        let coll = fake.collections_named(COLL)[0].id.clone();
        let doc_id = fake.docs_in(&coll)[0].id.clone();

        // The user renames the doc's title — the marker is untouched.
        fake.rename_document(&doc_id, "Renamed By Hand 🎉");

        // Second capture must land in the SAME doc, found by marker.
        capture(
            &store(fake.clone()),
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
        )
        .await;

        assert_eq!(fake.docs_in(&coll).len(), 1, "no duplicate doc spawned on rename");
        let facts = store(fake).recall(&subject).await.unwrap();
        assert_eq!(facts.len(), 2, "both facts live in the one doc");
    }

    #[tokio::test]
    async fn reconciles_duplicate_docs_to_the_oldest_canonical() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let marker = &seeded_doc(&person("alpha"));
        let older = with_fact_appended(marker, "| f1 | person:alpha | older fact |  | testimony | active | 2026-07-01 |");
        let newer = with_fact_appended(marker, "| f1 | person:alpha | newer fact |  | testimony | active | 2026-07-02 |");
        fake.seed_document(&coll, "a", &older);
        fake.seed_document(&coll, "b", &newer);

        let facts = store(fake).recall(&EntityId::person("alpha")).await.unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "older fact", "the oldest doc is canonical");
    }

    #[tokio::test]
    async fn pages_beyond_100_docs_before_concluding_absent() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        for i in 0..120 {
            fake.seed_document(&coll, &format!("other-{i}"), &seeded_doc(&person(&format!("other-{i}"))));
        }
        let target = with_fact_appended(
            &seeded_doc(&person("alpha")),
            // A row in the pre-`details` format — the paged-past doc is also the
            // legacy-row regression, read through the real store.
            "| f1 | person:alpha | found me | testimony | active | 2026-07-01 |",
        );
        fake.seed_document(&coll, "entity doc", &target);

        let facts = store(fake).recall(&EntityId::person("alpha")).await.unwrap();
        assert_eq!(facts.len(), 1, "must find the paged-past doc");
        assert_eq!(facts[0].content, "found me");
    }
}
