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
    validate_content, validate_details, validate_entity, validate_subject,
    guard::{self, Decision},
};

use api::{CollectionRec, DocRec, HttpOutline, OutlineApi, Unconfigured};
use codec::{
    next_fact_id, parse_entity, parse_facts_table, parse_id_marker, render_fact_row, seeded_doc,
    with_fact_appended, with_frontmatter_replaced, with_row_replaced,
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
        validate_entity(&new.id, &new.name, &new.source, new.crm.as_deref())?;
        let collection_id = self.resolve_collection().await?;

        let index = self.entity_index(&collection_id).await?;
        if let Decision::Block(candidates) =
            guard::decide(&new.id, Some(&new.name), &index, new.create_new)
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
    ) -> Result<Entity, MemoryError> {
        validate_subject(handle)?;
        let collection_id = self.resolve_collection().await?;

        let Some(doc) = self.entity_doc(&collection_id, handle).await? else {
            let index = self.entity_index(&collection_id).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: handle.to_string(),
                nearest: guard::screen(handle, None, &index),
            });
        };
        let mut entity = parse_entity(&doc.text)
            .ok_or_else(|| MemoryError::Store(format!("doc for {handle} lost its marker")))?;
        apply_entity_patch(&mut entity, &patch)?;

        let updated = with_frontmatter_replaced(&doc.text, &entity);
        self.api.update_document(&doc.id, &updated).await?;

        let seen = self.read_entity(&collection_id, handle).await?;
        if seen != entity {
            return Err(MemoryError::Store(format!(
                "entity {handle} read back changed: wrote {entity:?}, read {seen:?}"
            )));
        }
        Ok(seen)
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        validate_subject(&fact.subject)?;
        validate_content(&fact.content)?;
        validate_details(fact.details.as_deref())?;
        let collection_id = self.resolve_collection().await?;

        // A subject with no doc yet is a new entity, so it passes the guard
        // before anything is written. One that resolves exactly is already
        // known — guarding it would make every second fact need confirming.
        let doc = match self.entity_doc(&collection_id, &fact.subject).await? {
            Some(doc) => doc,
            None => {
                let index = self.entity_index(&collection_id).await?;
                if let Decision::Block(candidates) =
                    guard::decide(&fact.subject, None, &index, fact.create_new)
                {
                    return Ok(Guarded::Blocked {
                        attempted: fact.subject,
                        candidates,
                    });
                }
                let provisioned = Entity {
                    kind: fact.subject.kind().expect("a validated id has a kind"),
                    id: fact.subject.clone(),
                    name: String::new(),
                    // Existence is sourced, never invented: this entity exists
                    // because a fact arrived about it, and says so.
                    source: "capture".into(),
                    crm: None,
                    boot: Default::default(),
                };
                self.create_entity_doc(&collection_id, &provisioned).await?
            }
        };

        // Read-modify-write. Outline has no atomic append, so two captures
        // racing on the same doc could collide an id or lose a row — acceptable
        // for a single-session assistant; noted for a later revision guard.
        let existing = parse_facts_table(&doc.text);
        let stored = Fact {
            id: next_fact_id(&existing),
            home: fact.subject.clone(),
            subject: fact.subject,
            content: normalize_content(&fact.content),
            details: normalize_details(fact.details.as_deref()),
            provenance: fact.provenance,
            status: fact.status,
            date: fact.date,
        };
        let updated = with_fact_appended(&doc.text, &render_fact_row(&stored));
        self.api.update_document(&doc.id, &updated).await?;

        // Read-back: a capture succeeds only if the read path returns the fact,
        // byte-identical. Writing is not recording.
        let seen = self
            .recall(&stored.subject)
            .await?
            .into_iter()
            .find(|f| f.id == stored.id)
            .ok_or_else(|| {
                MemoryError::Store(format!("fact {} did not read back", stored.address()))
            })?;
        if seen != stored {
            return Err(MemoryError::Store(format!(
                "fact {} read back changed: wrote {stored:?}, read {seen:?}",
                stored.address()
            )));
        }
        Ok(Guarded::Written(seen))
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        match self.entity_doc(&collection_id, subject).await? {
            None => Ok(Vec::new()),
            Some(doc) => Ok(parse_facts_table(&doc.text)
                .into_iter()
                .filter(|f| &f.subject == subject)
                .collect()),
        }
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Fact, MemoryError> {
        validate_subject(&address.home)?;
        let collection_id = self.resolve_collection().await?;

        let unknown = |nearest: Vec<String>| MemoryError::UnknownFact {
            attempted: address.to_string(),
            nearest,
        };
        let Some(doc) = self.entity_doc(&collection_id, &address.home).await? else {
            return Err(unknown(Vec::new()));
        };
        let facts = parse_facts_table(&doc.text);
        let Some(mut fact) = facts.iter().find(|f| f.id == address.local).cloned() else {
            return Err(unknown(
                facts.iter().map(|f| f.address().to_string()).collect(),
            ));
        };
        apply_fact_patch(&mut fact, &patch)?;

        // The row is rewritten where it stands — fix the source, never an
        // addendum beside it.
        let updated = with_row_replaced(&doc.text, &address.local, &render_fact_row(&fact))
            .ok_or_else(|| unknown(facts.iter().map(|f| f.address().to_string()).collect()))?;
        self.api.update_document(&doc.id, &updated).await?;

        let seen = self
            .entity_doc(&collection_id, &address.home)
            .await?
            .map(|d| parse_facts_table(&d.text))
            .unwrap_or_default()
            .into_iter()
            .find(|f| f.id == address.local)
            .ok_or_else(|| MemoryError::Store(format!("fact {address} did not read back")))?;
        if seen != fact {
            return Err(MemoryError::Store(format!(
                "fact {address} read back changed: wrote {fact:?}, read {seen:?}"
            )));
        }
        Ok(seen)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicU64, Ordering};

    use jiff::civil::date;
    use jojobot_domain::memory::testing::contract;

    use super::*;

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
            source: "capture".into(),
            crm: None,
            boot: Default::default(),
        }
    }

    /// Capture through the store, asserting the guard waved it through.
    async fn capture(store: &OutlineStore, fact: NewFact) -> Fact {
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
