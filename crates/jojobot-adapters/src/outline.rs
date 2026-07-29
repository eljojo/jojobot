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
mod mailbox_codec;
mod mailboxes;
mod session_codec;
mod sessions;

pub use mailboxes::OutlineMailboxes;
pub use sessions::OutlineSessions;

use std::fmt;
use std::sync::Arc;

use async_trait::async_trait;

use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, FactStatus, Guarded,
    Memory, MemoryError, NewEntity, NewFact, Retraction, apply_entity_patch, apply_fact_patch,
    guard::{self, Decision},
    normalize_content, normalize_details, normalize_prose, retraction_of, screen_entity_patch,
    search::DocScan,
    validate_content, validate_details, validate_edge, validate_entity, validate_prose,
    validate_subject,
};

use jiff::civil::Date;

use api::{CollectionRec, DocRec, HttpOutline, OutlineApi, Unconfigured};
use codec::{
    next_fact_id, parse_entity, parse_facts_table, parse_id_marker, parse_machinery, parse_prose,
    render_fact_row, seeded_doc, with_fact_appended, with_frontmatter_replaced,
    with_prose_replaced, with_row_replaced,
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

/// **What a rollback did — a value, never a sentence.**
///
/// Every context here writes, reads back, and puts the page back when the two
/// disagree. Whether that put-back WORKED is the one thing a caller cannot
/// infer from anything else in the answer: a restored page means retry, and a
/// stranded one means a person. It was carried as prose inside a general store
/// error once, detecting it meant string-matching that prose, and rewording it
/// silently broke the detection with every test green. The `Stranded` variants
/// were the fix; the storage move brought the prose back and left them
/// unconstructed, which is the same bug wearing the same clothes.
///
/// Shared across all three contexts because all three restore identically, and
/// three copies of this decision is how one of them drifts.
pub(super) enum Restored {
    /// The page is back exactly as it was found.
    Undone,
    /// The rollback failed too, with the store's own account of why.
    Failed(String),
}

/// **The connection, the collection, and the one write lock** — everything a
/// store needs to reach jojobot's Outline collection, and the thing that makes
/// two stores over it one writer rather than two.
///
/// Memory and Sessions write different documents in the same collection. Two
/// separate mutexes would therefore exclude nobody, and "keyed on the resource"
/// would be a claim with nothing behind it. Sharing this by construction — a
/// sessions store is built *from* a memory store — is what makes it true
/// instead of remembered.
pub(crate) struct Workspace {
    api: Arc<dyn OutlineApi>,
    collection: String,
    /// **Every write here is a read-modify-write over a whole document**, and
    /// two of them overlapping is a lost update: the second builds its body
    /// from a page that no longer exists, and the read-back cannot catch it,
    /// because the page does contain what this caller wrote.
    ///
    /// So writes to a document are linearized, and the key is the DOCUMENT —
    /// not the bot, not the session, not the verb. None of those is what two
    /// racing writers have in common. The gate at the MCP layer answers a
    /// different question (one handle, one writer) and cannot answer this one;
    /// conflating the two is how a lock ends up excluding everybody except the
    /// pair it was built for.
    ///
    /// **One workspace-wide mutex, not a per-document map.** Write traffic here
    /// is low and a keyed map is an optimization with its own way to be wrong —
    /// every caller has to derive the same key, and a document is reached by
    /// title, by marker and by id. Narrow it when the simple one is shown to
    /// hurt.
    ///
    /// **What it does not cover:** writers this process cannot see — a person
    /// editing in the browser, a second instance across a deploy overlap. That
    /// is a document-revision check, a different protection, and it does not
    /// substitute for this one in either direction.
    lock: tokio::sync::Mutex<()>,
}

impl Workspace {
    fn api(&self) -> &dyn OutlineApi {
        self.api.as_ref()
    }

    /// Take the write lock. Held for the whole of a read-modify-write.
    async fn write(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.lock.lock().await
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
        if let Some(c) = pick_oldest(
            self.owned_collections().await?,
            |c| &c.created_at,
            |c| &c.id,
        ) {
            return Ok(c.id);
        }
        self.api
            .create_collection(&self.collection, &self.owner_description())
            .await?;
        pick_oldest(
            self.owned_collections().await?,
            |c| &c.created_at,
            |c| &c.id,
        )
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
}

/// The real Memory adapter, fronting an Outline collection it manages by name.
/// Stateless about CONTENT: it holds an API client and the collection *name*,
/// never an id or a fact.
#[derive(Clone)]
pub struct OutlineStore {
    ws: Arc<Workspace>,
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
            ws: Arc::new(Workspace {
                api,
                collection: collection.into(),
                lock: tokio::sync::Mutex::new(()),
            }),
        }
    }

    /// **A Sessions store over the same collection, and the same write lock.**
    ///
    /// Built from this store rather than beside it, because the two write
    /// different documents in one collection: separate locks would serialize
    /// nothing, and "two writes to the same document are linearized" would be a
    /// claim with no mechanism under it. Sharing the workspace makes it
    /// structural instead of remembered.
    pub fn sessions(&self) -> OutlineSessions {
        OutlineSessions::new(Arc::clone(&self.ws))
    }

    /// **A Mailboxes store over the same collection, and the same write lock.**
    /// Built from this store for the reason [`sessions`](Self::sessions) is:
    /// three stores now write different documents in one place, and a mutex
    /// each would exclude nobody.
    pub fn mailboxes(&self) -> OutlineMailboxes {
        OutlineMailboxes::new(Arc::clone(&self.ws))
    }

    async fn resolve_collection(&self) -> Result<String, MemoryError> {
        self.ws.resolve_collection().await
    }

    async fn all_docs(&self, collection_id: &str) -> Result<Vec<DocRec>, MemoryError> {
        self.ws.all_docs(collection_id).await
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
        docs.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });
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
    ///
    /// **A child's page is created under its parent's.** The `parent:` line in
    /// the frontmatter is what the tree is read back from, but a wiki whose
    /// pages all sit in one flat list is not one a human can navigate, and the
    /// whole point of the tree is that detail lives next to what it is about.
    /// So the two are written together, once, at the only moment parentage is
    /// ever set.
    ///
    /// A parent whose page cannot be found is a hard error, not a quiet
    /// top-level create: the guard has already established that the parent
    /// entity exists, so a missing page means the store changed under us, and
    /// filing the child at the root would leave a page whose line and position
    /// disagree with nobody told.
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
        let under = match &entity.parent {
            None => None,
            Some(parent) => Some(
                self.entity_doc(collection_id, parent)
                    .await?
                    .ok_or_else(|| {
                        MemoryError::Store(format!(
                            "{} is to sit under {parent}, which has no page to sit under",
                            entity.id
                        ))
                    })?
                    .id,
            ),
        };
        self.ws
            .api()
            .create_document(collection_id, &title, &seeded_doc(entity), under.as_deref())
            .await?;
        let doc = self
            .entity_doc(collection_id, &entity.id)
            .await?
            .ok_or_else(|| MemoryError::Store("entity doc missing after create".into()))?;
        // Read-back covers the page's POSITION too, not only its contents: the
        // write asserted where the page goes, so the write verifies it.
        if doc.parent_id.as_deref() == under.as_deref() {
            return Ok(doc);
        }

        // **Repair, then re-verify — never refuse on the first miss.**
        //
        // Refusing left the worst of both ends. The page is already written and
        // already carries the entity's marker, so the entity EXISTS: it lists,
        // `children` reports it, and every retry comes back `ExactHandle` on a
        // handle the caller believes it never created. The write said it failed
        // and the store disagreed, permanently.
        //
        // A move fixes it and costs nothing this store did not already have:
        // `documents.move` relocates a page and leaves its text untouched, both
        // verified against the live API. It is also not a delete, so putting
        // this right does not spend the no-delete rule.
        self.ws
            .api()
            .move_document(&doc.id, collection_id, under.as_deref())
            .await?;
        let moved = self
            .entity_doc(collection_id, &entity.id)
            .await?
            .ok_or_else(|| MemoryError::Store("entity doc missing after move".into()))?;
        if moved.parent_id.as_deref() != under.as_deref() {
            return Err(MemoryError::Store(format!(
                "{} was created under {:?} and could not be moved under the page of {:?} \
                 as written — it is at {:?} now, and a person has to place it",
                entity.id, doc.parent_id, entity.parent, moved.parent_id
            )));
        }
        Ok(moved)
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

    /// Put a page back the way a failed write found it. A read-back mismatch
    /// means the store transformed what was written; leaving the transformed
    /// page behind strands a half-written row for a retry to duplicate. The
    /// caller's data is not lost either way — the error carries the whole
    /// value — but the PAGE must end the call in a state a retry can trust.
    /// Best-effort: the returned clause lands in the error so the caller knows
    /// which state the page is actually in.
    /// Put the page back, and report what happened **as a value**.
    ///
    /// See [`Restored`]: this used to hand back a sentence, and every caller
    /// interpolated it into a general store error — which is the exact shape
    /// the `Stranded` variants exist to prevent, re-introduced by the storage
    /// move with every test green.
    async fn restore(&self, doc: &DocRec) -> Restored {
        match self.ws.api().update_document(&doc.id, &doc.text).await {
            Ok(()) => Restored::Undone,
            Err(e) => Restored::Failed(e.to_string()),
        }
    }

    /// The error a failed write becomes, once the rollback has been attempted.
    ///
    /// **One place decides which of the two it is**, so the "restored" and
    /// "stranded" answers cannot drift apart across four call sites — and so
    /// that adding a fifth cannot quietly pick the wrong one.
    async fn undo(
        &self,
        doc: &DocRec,
        verb: &'static str,
        stranded: Vec<String>,
        cause: String,
    ) -> MemoryError {
        match self.restore(doc).await {
            Restored::Undone => MemoryError::Store(format!(
                "{verb} failed ({cause}); the page was restored to its state before it"
            )),
            Restored::Failed(rollback) => MemoryError::Stranded {
                verb: verb.to_string(),
                stranded,
                cause,
                rollback,
            },
        }
    }

    /// Read an entity back through the read path — the verification half of
    /// every entity write.
    async fn read_entity(&self, collection_id: &str, id: &EntityId) -> Result<Entity, MemoryError> {
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
    items.sort_by(|a, b| {
        created_at(a)
            .cmp(created_at(b))
            .then_with(|| id(a).cmp(id(b)))
    });
    items.into_iter().next()
}

#[async_trait]
impl Memory for OutlineStore {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
        validate_entity(
            &new.id,
            &new.name,
            &new.aliases,
            &new.source,
            new.crm.as_deref(),
            new.parent.as_ref(),
        )?;
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
            parent: new.parent,
            boot: new.boot,
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
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
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

        // Changing what an entity is CALLED is an entity-touching write, so it
        // faces the same gate a creation does — display name and aliases alike.
        // Otherwise the guard is side-steppable: add under a throwaway name,
        // then move the contested name on afterwards.
        if let Decision::Block(candidates) = screen_entity_patch(&entity, &patch, &index) {
            return Ok(Guarded::Blocked {
                attempted: handle.clone(),
                candidates,
            });
        }
        apply_entity_patch(&mut entity, &patch)?;

        let updated = with_frontmatter_replaced(&doc.text, &entity);
        self.ws.api().update_document(&doc.id, &updated).await?;

        let seen = self.read_entity(&collection_id, handle).await?;
        if seen != entity {
            return Err(self
                .undo(
                    &doc,
                    "update_entity",
                    vec![handle.to_string()],
                    format!("entity {handle} read back changed: wrote {entity:?}, read {seen:?}"),
                )
                .await);
        }
        Ok(Guarded::Written(seen))
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
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
        // **An event's refs are named entities like any other**, screened here
        // rather than only in the fake — the hatch is ungated on its TYPE, and
        // that is the only thing about it that is loose.
        for object in fact.event.iter().flat_map(|e| &e.refs) {
            validate_subject(object)?;
            if let Decision::Block(candidates) = guard::decide_existing(object, &index) {
                return Ok(Guarded::Blocked {
                    attempted: object.clone(),
                    candidates,
                });
            }
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
            // **Built field by field, so a field forgotten here is a field the
            // store never sees.** This one was, and read-back could not tell:
            // it compares the row against `stored`, and `stored` was missing
            // the payload in exactly the same way the row was. The invariant
            // that catches a dropped field is the caller's record against the
            // read one, which is a contract spec rather than anything here.
            event: fact.event,
        };
        let updated = with_fact_appended(&doc.text, &render_fact_row(&stored));
        self.ws.api().update_document(&doc.id, &updated).await?;

        // Read-back: a capture succeeds only if the read path returns the fact,
        // byte-identical. Writing is not recording.
        let seen = self.read_back_fact(&stored.address()).await?;
        if seen != stored {
            return Err(self
                .undo(
                    &doc,
                    "capture",
                    vec![stored.address().to_string()],
                    format!(
                        "fact {} read back changed: wrote {stored:?}, read {seen:?}",
                        stored.address()
                    ),
                )
                .await);
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
            // An unknown entity is a miss with its near candidates — never an
            // empty page. The production smoke test caught a bad handle and an
            // empty-but-real entity answering identically; the guard already
            // knew the difference, recall just never surfaced it.
            None => {
                let index = self.entity_index(&collection_id).await?;
                Err(MemoryError::UnknownEntity {
                    attempted: subject.to_string(),
                    nearest: guard::screen(subject, &[], &index),
                })
            }
            Some(doc) => Ok(parse_facts_table(&doc.text)),
        }
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
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
        // **Where one-way is actually enforced.** `retract` refusing a second
        // pass is only half of it: without this, a status flip back to active
        // is an ordinary patch away, and the ceremony on the retract verb would
        // be guarding a door with the window open.
        if fact.status == FactStatus::Retracted {
            return Err(MemoryError::NotRetractable {
                attempted: address.to_string(),
                why: "it is retracted, and a retracted record is not editable — retraction is \
                      one-way. Capture what is so now as a new record"
                    .to_string(),
            });
        }
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
        self.ws.api().update_document(&doc.id, &updated).await?;

        let seen = self.read_back_fact(address).await?;
        if seen != fact {
            return Err(self
                .undo(
                    &doc,
                    "update_fact",
                    vec![address.to_string()],
                    format!("fact {address} read back changed: wrote {fact:?}, read {seen:?}"),
                )
                .await);
        }
        Ok(Guarded::Written(seen))
    }

    /// **Two rows, one document write.** The marked row and the account of why
    /// it was marked share a home page, so they go up in a single PUT — which
    /// is the whole reason this is a port verb rather than an edit followed by
    /// a capture. Two calls could leave the state this verb must never produce:
    /// a record taken back with nothing anywhere saying why.
    async fn retract(
        &self,
        address: &FactAddress,
        reason: &str,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
        validate_subject(&address.home)?;
        let collection_id = self.resolve_collection().await?;

        let Some(doc) = self.entity_doc(&collection_id, &address.home).await? else {
            let index = self.entity_index(&collection_id).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: address.home.to_string(),
                nearest: guard::screen(&address.home, &[], &index),
            });
        };
        let facts = parse_facts_table(&doc.text);
        let Some(target) = facts.iter().find(|f| f.id == address.local).cloned() else {
            return Err(MemoryError::UnknownFact {
                attempted: address.to_string(),
                nearest: facts.iter().map(|f| f.address().to_string()).collect(),
            });
        };
        // Everything that can refuse, refuses before the page is touched.
        let account = retraction_of(&target, reason, date)?;

        let retracted = Fact {
            status: FactStatus::Retracted,
            ..target
        };
        let marked = with_row_replaced(
            &doc.text,
            &address.home,
            &address.local,
            &render_fact_row(&retracted),
        )
        .ok_or_else(|| MemoryError::UnknownFact {
            attempted: address.to_string(),
            nearest: facts.iter().map(|f| f.address().to_string()).collect(),
        })?;

        // Minted off the page as it will stand WITH the row already marked —
        // the id comes from the doc's text, and the text is what changed.
        let record = Fact {
            id: next_fact_id(&marked),
            home: address.home.clone(),
            subject: account.subject,
            content: account.content,
            details: account.details,
            provenance: account.provenance,
            status: account.status,
            date: account.date,
            edge: account.edge,
            event: account.event,
        };
        let updated = with_fact_appended(&marked, &render_fact_row(&record));
        self.ws.api().update_document(&doc.id, &updated).await?;

        // **Both rows are read back, because both were written.** Confirming
        // only the mark would confirm exactly half of the one state this verb
        // must not leave behind.
        let seen_retracted = self.read_back_fact(address).await?;
        let seen_record = self.read_back_fact(&record.address()).await?;
        if seen_retracted != retracted || seen_record != record {
            return Err(self
                .undo(
                    &doc,
                    "retract",
                    vec![address.to_string(), record.address().to_string()],
                    format!(
                        "retraction of {address} read back changed: wrote {retracted:?} and \
                         {record:?}, read {seen_retracted:?} and {seen_record:?}"
                    ),
                )
                .await);
        }
        Ok(Retraction {
            retracted: seen_retracted,
            record: seen_record,
        })
    }

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        // Serialized against every other write to this collection's
        // documents — see [`OutlineStore::lock`].
        let _writing = self.ws.write().await;
        validate_subject(entity)?;
        validate_prose(prose)?;
        let collection_id = self.resolve_collection().await?;

        // Never creates a doc to hold the text: an unknown handle is a miss
        // with its near candidates, the rule every verb here follows.
        let Some(doc) = self.entity_doc(&collection_id, entity).await? else {
            let index = self.entity_index(&collection_id).await?;
            return Err(MemoryError::UnknownEntity {
                attempted: entity.to_string(),
                nearest: guard::screen(entity, &[], &index),
            });
        };

        let stored = normalize_prose(prose);
        let updated = with_prose_replaced(&doc.text, &stored).ok_or_else(|| {
            MemoryError::InvalidEntity(format!(
                "the prose could not be written to {entity}. Either it carries a line reserved \
                 for the fact table's header — every fact below such a line would stop being \
                 read as a fact — or this page was written by hand and is not yet in the shape \
                 jojobot rewrites, in which case any ordinary metadata edit (update_entity) puts \
                 it right and the prose can then be set"
            ))
        })?;
        self.ws.api().update_document(&doc.id, &updated).await?;

        // Read-back: the prose is only written once the read path returns it,
        // byte-identical — the same invariant a fact write carries.
        let seen = self
            .entity_doc(&collection_id, entity)
            .await?
            .map(|d| parse_prose(&d.text))
            .ok_or_else(|| MemoryError::Store(format!("entity {entity} lost its doc mid-write")))?;
        if seen != stored {
            return Err(self
                .undo(
                    &doc,
                    "set_prose",
                    vec![entity.to_string()],
                    format!("prose on {entity} read back changed: wrote {stored:?}, read {seen:?}"),
                )
                .await);
        }
        Ok(seen)
    }

    /// Every doc in the collection, whole — including docs that are **not**
    /// entities. A page the user wrote by hand carries no marker, so it is no
    /// entity and holds no facts; its prose is still worth finding, which is why
    /// it comes back rather than being filtered out here.
    ///
    /// **The one exception is jojobot's own machinery** — a bot's sessions page,
    /// which is a child of the bot's page and so lives in this collection like
    /// everything else. That generosity is the reason it has to be excluded by
    /// name: a page with no marker is content by default, and jojobot's
    /// bookkeeping would qualify. A search about the operator's life must not
    /// come back with a session's focus line.
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        let collection_id = self.resolve_collection().await?;
        let mut docs: Vec<DocRec> = self
            .all_docs(&collection_id)
            .await?
            .into_iter()
            .filter(|d| parse_machinery(&d.text).is_none())
            .collect();
        // Canonical-first, so a double-created doc's twin can't shadow it.
        docs.sort_by(|a, b| {
            a.created_at
                .cmp(&b.created_at)
                .then_with(|| a.id.cmp(&b.id))
        });

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
    use jojobot_domain::memory::{Edge, EdgeShape, FactStatus, Provenance};

    use super::codec::{TABLE_HEADER, TABLE_SEP, escape_cell, split_cells};
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
        /// Arms [`with_last_cell_dropped`] for the next `update_document` — the
        /// induced fault behind the restore-on-mismatch contract.
        poison: std::sync::atomic::AtomicBool,
    }

    /// What the real Outline does to a markdown table on save: the editor model
    /// re-serializes every table RECTANGULAR AT THE HEADER'S WIDTH — long rows
    /// lose their tail, short rows are padded with empty cells. The production
    /// edge-loss bug lived exactly in the gap between this and a verbatim fake,
    /// so the fake is hostile on purpose. Seeds stay verbatim: a seed models
    /// whatever history already left on disk.
    fn rectangularized(text: &str) -> String {
        let lines: Vec<&str> = text.lines().collect();
        let mut out: Vec<String> = Vec::with_capacity(lines.len());
        let mut i = 0;
        let mut fenced = false;
        while i < lines.len() {
            // **Inside a fence nothing is a table.** The editor model treats
            // fenced content as literal, verified against live Outline: a
            // pipe-leading line inside a code block comes back exactly as
            // written. A fake that rectangularized it would be wrong rather
            // than hostile — failing a write production accepts, which is the
            // mirror of the bug this whole function exists to catch.
            if lines[i].trim_start().starts_with("```") {
                fenced = !fenced;
                out.push(lines[i].to_string());
                i += 1;
                continue;
            }
            if fenced || !lines[i].trim_start().starts_with('|') {
                out.push(lines[i].to_string());
                i += 1;
                continue;
            }
            let width = split_cells(lines[i]).len();
            while i < lines.len()
                && lines[i].trim_start().starts_with('|')
                && !lines[i].trim_start().starts_with("```")
            {
                let mut cells = split_cells(lines[i]);
                cells.resize(width, String::new());
                let cells: Vec<String> = cells.iter().map(|c| escape_cell(c)).collect();
                // **A lone `-` comes back `\-`, and that is not cosmetic.**
                // The editor model escapes a cell that would otherwise read as
                // markdown, and a bare dash is one — verified against live
                // Outline, where a message posted with no subject read back
                // with the subject `\-`. The codecs write `-` for an absent
                // optional, so a fake that left it alone would call every
                // absent field present. This is the same gap the
                // rectangularization above exists for, one character wide.
                let cells: Vec<String> = cells
                    .iter()
                    .map(|c| match c.trim() {
                        "-" => "\\-".to_string(),
                        _ => c.clone(),
                    })
                    .collect();
                out.push(format!("| {} |", cells.join(" | ")));
                i += 1;
            }
        }
        out.join("\n")
    }

    /// One write mangled at a layer the codec doesn't control — the induced
    /// fault for the restore contract: every data row loses its last cell.
    fn with_last_cell_dropped(text: &str) -> String {
        text.lines()
            .map(|l| {
                if !l.trim_start().starts_with('|') {
                    return l.to_string();
                }
                let mut cells = split_cells(l);
                let first = cells
                    .first()
                    .map(|c| c.trim().to_string())
                    .unwrap_or_default();
                let is_header = first.eq_ignore_ascii_case("id");
                let is_sep = !first.is_empty() && first.chars().all(|c| c == '-');
                if is_header || is_sep {
                    return l.to_string();
                }
                // **Drop the last cell that HAS something in it.**
                //
                // This is a fault injector, not a model of the store: its job
                // is to produce a corruption the read-back guard must notice.
                // Dropping the literal last cell stopped doing that the moment
                // a column was added whose value is usually empty — the mangle
                // removed nothing, the read-back matched, and two tests about
                // failed writes started reporting success. A trailing empty
                // cell is exactly the case where truncation is invisible, which
                // is true of the real store too and precisely why it is the
                // wrong thing to inject.
                let last = cells.iter().rposition(|c| !c.trim().is_empty());
                match last {
                    Some(at) => {
                        cells.remove(at);
                    }
                    None => {
                        cells.pop();
                    }
                }
                let cells: Vec<String> = cells.iter().map(|c| escape_cell(c)).collect();
                format!("| {} |", cells.join(" | "))
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    impl FakeOutline {
        fn new() -> Arc<Self> {
            Arc::new(Self::default())
        }

        /// Mangle the next `update_document` before it lands.
        fn poison_next_update(&self) {
            self.poison.store(true, Ordering::SeqCst);
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

        /// Pre-seed a document at the top of a collection; returns its id.
        fn seed_document(&self, collection_id: &str, title: &str, text: &str) -> String {
            self.seed_document_under(collection_id, title, text, None)
        }

        /// Pre-seed a document, nested under `parent_id` when there is one.
        fn seed_document_under(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
            parent_id: Option<&str>,
        ) -> String {
            let s = self.stamp();
            let id = format!("doc-{s}");
            self.documents.lock().unwrap().push((
                collection_id.into(),
                DocRec {
                    id: id.clone(),
                    title: title.into(),
                    text: text.into(),
                    created_at: s,
                    parent_id: parent_id.map(str::to_string),
                },
            ));
            id
        }

        fn rename_document(&self, id: &str, new_title: &str) {
            let mut docs = self.documents.lock().unwrap();
            let d = docs
                .iter_mut()
                .find(|(_, d)| d.id == id)
                .expect("doc exists");
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
            parent_id: Option<&str>,
        ) -> Result<DocRec, MemoryError> {
            // **`documents.list` returns nested docs too**, and the whole index
            // rests on that: a fake that filed children somewhere the listing
            // could not see would make every child vanish from `entity_index`
            // while the suite stayed green. They go in the one flat list, each
            // carrying the parent it hangs off — which is what the real
            // endpoint returns.
            let id =
                self.seed_document_under(collection_id, title, &rectangularized(text), parent_id);
            Ok(self
                .documents
                .lock()
                .unwrap()
                .iter()
                .find(|(_, d)| d.id == id)
                .map(|(_, d)| d.clone())
                .unwrap())
        }

        /// **Append the way Outline appends, which is not the way a caller
        /// hopes.** Observed against the live API rather than assumed: the
        /// appended text lands as its own BLOCK, joined to what was there with
        /// a blank line, and both sides are trimmed on the way through —
        /// `"LINE ONE\n"` + `"LINE TWO\n"` came back `"LINE ONE\n\nLINE TWO"`,
        /// and a leading newline changed nothing. The document is re-serialized
        /// too, so a table already on the page comes back padded.
        ///
        /// A polite fake that concatenated the bytes would let an adapter ship
        /// believing it could append a table row.
        async fn append_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            let mut docs = self.documents.lock().unwrap();
            let d = docs
                .iter_mut()
                .find(|(_, d)| d.id == id)
                .ok_or_else(|| MemoryError::Store(format!("append_document: no doc {id}")))?;
            let joined = format!("{}\n\n{}", d.1.text.trim_end(), text.trim());
            d.1.text = rectangularized(&joined);
            Ok(())
        }
        /// A move relocates the page and touches nothing else — the live API's
        /// behaviour, and what makes it usable to repair a mis-nested create.
        async fn move_document(
            &self,
            id: &str,
            _collection_id: &str,
            parent_id: Option<&str>,
        ) -> Result<(), MemoryError> {
            let mut docs = self.documents.lock().unwrap();
            let d = docs
                .iter_mut()
                .find(|(_, d)| d.id == id)
                .ok_or_else(|| MemoryError::Store(format!("move_document: no doc {id}")))?;
            d.1.parent_id = parent_id.map(str::to_string);
            Ok(())
        }

        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            let text = if self.poison.swap(false, Ordering::SeqCst) {
                with_last_cell_dropped(text)
            } else {
                text.to_string()
            };
            let text = rectangularized(&text);
            let mut docs = self.documents.lock().unwrap();
            match docs.iter_mut().find(|(_, d)| d.id == id) {
                Some((_, d)) => {
                    d.text = text;
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
            parent: None,
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

    /// **A child's page is nested under its parent's page.** The frontmatter
    /// line is what jojobot reads the tree back from, but a wiki whose pages
    /// all sit in one flat list is not a wiki anybody can navigate: the point
    /// of the tree is that detail lives next to what it is about, and in
    /// Outline "next to" means underneath. So the two agree at creation — the
    /// line says it and the page is there.
    ///
    /// A root is created at the top of the collection, under nothing.
    #[tokio::test]
    async fn a_childs_page_is_created_under_its_parents_page() {
        let fake = FakeOutline::new();
        let store = store(fake.clone());
        let parent = EntityId::new(EntityKind::Project, "atlas");
        let child = EntityId::new(EntityKind::Place, "riverbend");

        ensure(&store, &parent).await;
        store
            .add_entity(NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(child.clone(), "Riverbend", "test-fixture")
            })
            .await
            .expect("add_entity should succeed")
            .written()
            .expect("the guard must not block this child");

        let coll = store
            .resolve_collection()
            .await
            .expect("the collection resolves");
        let docs = fake.docs_in(&coll);
        let doc_for = |id: &EntityId| {
            docs.iter()
                .find(|d| parse_id_marker(&d.text).as_deref() == Some(id.as_str()))
                .unwrap_or_else(|| panic!("{id} has a doc"))
        };

        assert_eq!(
            doc_for(&parent).parent_id,
            None,
            "a root sits at the top of the collection"
        );
        assert_eq!(
            doc_for(&child).parent_id.as_deref(),
            Some(doc_for(&parent).id.as_str()),
            "the child's page hangs off the parent's, not off the collection"
        );
    }

    /// A transport that files every page at the top of the collection, however
    /// it was asked to nest it — Outline's own behaviour when the parent it is
    /// handed is one it will not nest under. Silent: the create succeeds and
    /// returns a document.
    /// A transport that files every page at the top of the collection however it
    /// was asked to nest it — Outline's own behaviour for a parent it will not
    /// nest under. Silent: the create succeeds and returns a document. **A move
    /// through it works**, which is what lets the repair path be exercised.
    struct Flattening(Arc<dyn OutlineApi>);

    #[async_trait]
    impl OutlineApi for Flattening {
        async fn list_collections(
            &self,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<CollectionRec>, MemoryError> {
            self.0.list_collections(offset, limit).await
        }
        async fn create_collection(
            &self,
            name: &str,
            description: &str,
        ) -> Result<CollectionRec, MemoryError> {
            self.0.create_collection(name, description).await
        }
        async fn list_documents(
            &self,
            collection_id: &str,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<DocRec>, MemoryError> {
            self.0.list_documents(collection_id, offset, limit).await
        }
        async fn create_document(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
            _parent_id: Option<&str>,
        ) -> Result<DocRec, MemoryError> {
            self.0
                .create_document(collection_id, title, text, None)
                .await
        }
        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.0.update_document(id, text).await
        }
        async fn append_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.0.append_document(id, text).await
        }
        async fn move_document(
            &self,
            id: &str,
            collection_id: &str,
            parent_id: Option<&str>,
        ) -> Result<(), MemoryError> {
            self.0.move_document(id, collection_id, parent_id).await
        }
    }

    /// [`Flattening`], and a move that reports success without doing anything —
    /// the shape a store takes when it accepts a placement it will not honour.
    /// The page can never be got where it belongs.
    struct Immovable(Arc<dyn OutlineApi>);

    #[async_trait]
    impl OutlineApi for Immovable {
        async fn list_collections(
            &self,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<CollectionRec>, MemoryError> {
            self.0.list_collections(offset, limit).await
        }
        async fn create_collection(
            &self,
            name: &str,
            description: &str,
        ) -> Result<CollectionRec, MemoryError> {
            self.0.create_collection(name, description).await
        }
        async fn list_documents(
            &self,
            collection_id: &str,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<DocRec>, MemoryError> {
            self.0.list_documents(collection_id, offset, limit).await
        }
        async fn create_document(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
            _parent_id: Option<&str>,
        ) -> Result<DocRec, MemoryError> {
            self.0
                .create_document(collection_id, title, text, None)
                .await
        }
        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.0.update_document(id, text).await
        }
        async fn append_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.0.append_document(id, text).await
        }
        async fn move_document(
            &self,
            _: &str,
            _: &str,
            _: Option<&str>,
        ) -> Result<(), MemoryError> {
            Ok(())
        }
    }

    /// **A page that did not land where it was put is MOVED there, not
    /// refused.**
    ///
    /// Refusing left the worst of both: the page was already written and
    /// carried the entity's marker, so the entity existed, `children` reported
    /// it, and every retry came back `ExactHandle` on a handle the caller
    /// believed it had never created. The write said "failed" and the store
    /// disagreed forever.
    ///
    /// Repair is available because a move is available — verified against the
    /// live API, which moves a flat page under a parent and leaves its text
    /// untouched. And a move is not a delete, so nothing here spends the
    /// no-delete rule to buy it.
    #[tokio::test]
    async fn a_page_that_missed_its_parent_is_moved_there_rather_than_refused() {
        let fake = FakeOutline::new();
        let store = OutlineStore::from_api(Arc::new(Flattening(fake.clone())), COLL);
        let parent = EntityId::new(EntityKind::Project, "atlas");
        let child = EntityId::new(EntityKind::Place, "riverbend");
        ensure(&store, &parent).await;

        store
            .add_entity(NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(child.clone(), "Riverbend", "test-fixture")
            })
            .await
            .expect("the create is repaired, not refused")
            .written()
            .expect("the guard must not block this child");

        let coll = store.resolve_collection().await.expect("collection");
        let docs = fake.docs_in(&coll);
        let doc_for = |id: &EntityId| {
            docs.iter()
                .find(|d| parse_id_marker(&d.text).as_deref() == Some(id.as_str()))
                .unwrap_or_else(|| panic!("{id} has a doc"))
        };
        assert_eq!(
            doc_for(&child).parent_id.as_deref(),
            Some(doc_for(&parent).id.as_str()),
            "the page ends up under its parent, by the store's own account"
        );
    }

    /// **…and if the move cannot get it there either, that is still a failed
    /// write.** The read-back is not traded away for the repair: repair first,
    /// and only report success once the page is actually where the write said.
    #[tokio::test]
    async fn a_page_the_store_will_not_move_is_still_a_failed_write() {
        let fake = FakeOutline::new();
        let store = OutlineStore::from_api(Arc::new(Immovable(fake.clone())), COLL);
        let parent = EntityId::new(EntityKind::Project, "atlas");
        ensure(&store, &parent).await;

        let err = store
            .add_entity(NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(
                    EntityId::new(EntityKind::Place, "riverbend"),
                    "Riverbend",
                    "test-fixture",
                )
            })
            .await
            .expect_err("a page that cannot be got where it belongs is not a success");
        assert!(
            matches!(&err, MemoryError::Store(m) if m.contains("under")),
            "the error says what did not happen: {err}"
        );
    }

    /// **A chronology entry that quotes a table survives being one.**
    ///
    /// The fake re-serializes tables because the real store does — that is the
    /// quirk the production edge-loss bug lived in. But the real store applies
    /// it to *tables*, and a pipe-leading line inside a fenced block is not one:
    /// verified against live Outline, which leaves it exactly as written. A fake
    /// that rectangularized it would be wrong rather than hostile, and would
    /// fail a write that production accepts — the mirror image of the bug the
    /// rectangularization exists to catch, and just as expensive.
    #[tokio::test]
    async fn an_entry_quoting_a_table_is_not_re_serialized_as_one() {
        use jojobot_domain::session::Sessions as _;

        let sessions = OutlineStore::from_api(FakeOutline::new(), COLL).sessions();
        let begun = sessions
            .begin(jojobot_domain::session::NewSession {
                bot: EntityId::new(EntityKind::Bot, "gamma"),
                sid: jojobot_domain::session::Sid("ab12".into()),
                focus: "the first run".into(),
                started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
            })
            .await
            .expect("begin should succeed");

        // Deliberately RAGGED. A tidy table survives rectangularization by
        // accident, so quoting one would prove nothing: this one loses a cell
        // and gains a padded one the moment the fake treats it as a table.
        let quoted = "the counts were:\n| kind | n |\n| --- | --- |\n\
                      | fact | 3 | dropped |\n| bare |\nand that was all";
        sessions
            .append(
                &begun.id,
                jojobot_domain::session::NewEntry::manual(
                    quoted,
                    "2026-07-28T00:01:00Z".parse().expect("a timestamp"),
                ),
            )
            .await
            .expect("append should succeed");

        let read = sessions
            .read_session(&begun.id)
            .await
            .expect("read should succeed");
        assert_eq!(
            read.entries[0].text, quoted,
            "a table inside somebody's entry is their prose, not the page's table"
        );
    }

    /// **Two runs of one bot beginning at once do not collide.** Both reads see
    /// the same page, both mint the next id off it, and both write the whole
    /// table back — so without the lock the second write is built from a page
    /// that no longer exists and one of the two sessions is simply gone, with
    /// each caller holding a `Session` that says otherwise.
    ///
    /// Run through the yielding transport, which suspends **after** a write
    /// commits as well as before, because that is where the network suspends: a
    /// real create is a round trip and the page has changed server-side before
    /// the response arrives. A double that only yields before the call would
    /// pass this on broken code.
    #[tokio::test]
    async fn two_runs_of_one_bot_beginning_at_once_both_survive() {
        use jojobot_domain::session::Sessions as _;

        let fake = FakeOutline::new();
        let sessions = Arc::new(OutlineStore::from_api(Arc::new(Yielding(fake)), COLL).sessions());
        let bot = EntityId::new(EntityKind::Bot, "gamma");

        let begin = |sid: &'static str, focus: &'static str| {
            let sessions = Arc::clone(&sessions);
            let bot = bot.clone();
            async move {
                sessions
                    .begin(jojobot_domain::session::NewSession {
                        bot,
                        sid: jojobot_domain::session::Sid(sid.into()),
                        focus: focus.into(),
                        started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
                    })
                    .await
                    .expect("begin should succeed")
            }
        };
        let (one, two) = tokio::join!(begin("ab12", "the first run"), begin("cd34", "the second"));

        assert_ne!(one.id, two.id, "two runs are two rows, not one id twice");
        let all = sessions.all_sessions().await.expect("all_sessions");
        assert_eq!(all.len(), 2, "neither write was lost: {all:?}");
        for begun in [&one, &two] {
            let seen = all
                .iter()
                .find(|s| s.id == begun.id)
                .unwrap_or_else(|| panic!("{} is not on the page", begun.id));
            assert_eq!(seen.sid, begun.sid, "and each kept its own handle");
            assert_eq!(seen.focus, begun.focus);
        }
    }

    /// …and the same contract **including retrieval**, with the search projection
    /// over the real store logic. The fake satisfies this suite too, which is what
    /// stops the two from drifting.
    #[tokio::test]
    async fn the_indexed_outline_store_satisfies_the_whole_contract() {
        let indexed = IndexedMemory::new(Arc::new(store(FakeOutline::new()))).expect("index opens");
        contract::run_all_searchable(&indexed).await;
    }

    /// **The production edge-loss bug, end to end.** A doc provisioned before
    /// the edges column carries a 7-column header; the store re-serializes
    /// tables at the header's width, so an appended 8-cell row lost its last
    /// cell — the edge — while every fresh-doc test stayed green. A write now
    /// migrates the whole table first, so the row survives the hostile store
    /// and the page is healed for good.
    #[tokio::test]
    async fn a_capture_into_a_legacy_doc_keeps_its_edge_and_heals_the_table() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let legacy = seeded_doc(&person("alpha"))
            .replace(
                TABLE_HEADER,
                "| id | subject | content | details | provenance | status | date |",
            )
            .replace(TABLE_SEP, "| --- | --- | --- | --- | --- | --- | --- |")
            + "| f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |\n";
        fake.seed_document(&coll, "Alpha", &legacy);

        let store = store(fake.clone());
        ensure(&store, &EntityId("place:shelbyville".into())).await;
        let edge = Edge::new(EdgeShape::Location, EntityId("place:shelbyville".into()));
        let written = store
            .capture(NewFact {
                subject: EntityId::person("alpha"),
                content: "spending the winter away".into(),
                details: None,
                provenance: Provenance::Testimony,
                status: FactStatus::Active,
                date: date(2026, 7, 2),
                edge: Some(edge.clone()),
                event: None,
            })
            .await
            .expect("capture succeeds against the hostile store")
            .written()
            .expect("not blocked");
        assert_eq!(
            written.edge,
            Some(edge.clone()),
            "the edge survives the save"
        );

        let facts = store
            .recall(&EntityId::person("alpha"))
            .await
            .expect("recall ok");
        assert_eq!(facts.len(), 2, "the legacy row and the new one both read");
        assert_eq!(
            facts[1].edge,
            Some(edge),
            "the edge is on the page, not just in the reply"
        );

        let doc = fake
            .docs_in(&coll)
            .into_iter()
            .find(|d| d.text.contains("id: person:alpha"))
            .expect("alpha doc");
        assert!(
            doc.text.contains(TABLE_HEADER),
            "the narrow header was migrated on write"
        );
    }

    /// An [`OutlineApi`] that suspends on **both sides of every call**, which
    /// is where a round trip suspends: the request is on the server before the
    /// caller resumes, and the answer arrives later still.
    ///
    /// [`FakeOutline`] completes each call under a std lock and so is atomic
    /// against anything — two writers cannot interleave inside it, which is
    /// exactly the interleaving under test. A write here is a read, a gap where
    /// another task runs, then a whole-body PUT built from the earlier
    /// snapshot: the real shape, and the one that loses an update.
    struct Yielding(Arc<FakeOutline>);

    #[async_trait]
    impl OutlineApi for Yielding {
        async fn list_collections(
            &self,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<CollectionRec>, MemoryError> {
            tokio::task::yield_now().await;
            let out = self.0.list_collections(offset, limit).await;
            tokio::task::yield_now().await;
            out
        }
        async fn create_collection(
            &self,
            name: &str,
            description: &str,
        ) -> Result<CollectionRec, MemoryError> {
            tokio::task::yield_now().await;
            let out = self.0.create_collection(name, description).await;
            tokio::task::yield_now().await;
            out
        }
        async fn list_documents(
            &self,
            collection_id: &str,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<DocRec>, MemoryError> {
            tokio::task::yield_now().await;
            let out = self.0.list_documents(collection_id, offset, limit).await;
            tokio::task::yield_now().await;
            out
        }
        async fn create_document(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
            parent_id: Option<&str>,
        ) -> Result<DocRec, MemoryError> {
            tokio::task::yield_now().await;
            let out = self
                .0
                .create_document(collection_id, title, text, parent_id)
                .await;
            tokio::task::yield_now().await;
            out
        }
        async fn append_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.0.append_document(id, text).await
        }
        async fn move_document(
            &self,
            id: &str,
            collection_id: &str,
            parent_id: Option<&str>,
        ) -> Result<(), MemoryError> {
            tokio::task::yield_now().await;
            let out = self.0.move_document(id, collection_id, parent_id).await;
            tokio::task::yield_now().await;
            out
        }
        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            tokio::task::yield_now().await;
            let out = self.0.update_document(id, text).await;
            tokio::task::yield_now().await;
            out
        }
    }

    /// **Two writes touching one document are linearized, whoever started
    /// them.** Every write here is a read-modify-write over the whole document:
    /// read the page, build a new body from what was read, PUT the lot back,
    /// read it back. Two of them overlapping and the second's body is built
    /// from a page that no longer exists — so the first write is gone, silently,
    /// and the read-back that would have caught it passes because the page does
    /// contain what this caller wrote.
    ///
    /// **The lock is keyed on the RESOURCE, not on who is writing.** A bot, a
    /// session, a verb — none of them is what two racing writers have in
    /// common. The document is. The gate at the MCP layer is a different job
    /// (one handle, one writer) and cannot do this one.
    ///
    /// Three shapes, because they lose differently: two facts onto one entity's
    /// page, a fact racing a prose rewrite of the same page, and two stories
    /// racing onto the Journal — the last being the case the review proved,
    /// where the loser's `restore()` puts back a snapshot that erases an entry
    /// the winner had already committed and verified.
    #[tokio::test]
    async fn two_writes_to_one_document_do_not_lose_an_update() {
        let racing = || {
            let fake = FakeOutline::new();
            (
                fake.clone(),
                OutlineStore::from_api(Arc::new(Yielding(fake)), COLL),
            )
        };

        // ── two facts onto one page ──────────────────────────────────────
        let (_, store) = racing();
        ensure(&store, &EntityId::person("alpha")).await;
        let (first, second) = tokio::join!(
            store.capture(NewFact::about(
                EntityId::person("alpha"),
                "plays go",
                date(2026, 7, 27)
            )),
            store.capture(NewFact::about(
                EntityId::person("alpha"),
                "keeps bees",
                date(2026, 7, 27)
            )),
        );
        first.expect("capture ok").written().expect("not blocked");
        second.expect("capture ok").written().expect("not blocked");
        let facts = store
            .recall(&EntityId::person("alpha"))
            .await
            .expect("recall ok");
        let claims: Vec<&str> = facts.iter().map(|f| f.content.as_str()).collect();
        for expected in ["plays go", "keeps bees"] {
            assert!(
                claims.contains(&expected),
                "a fact was lost to a racing write on the same page: {claims:?}"
            );
        }

        // ── a fact racing a prose rewrite of the same page ───────────────
        let (_, store) = racing();
        ensure(&store, &EntityId::person("alpha")).await;
        let alpha = EntityId::person("alpha");
        let (fact, prose) = tokio::join!(
            store.capture(NewFact::about(alpha.clone(), "plays go", date(2026, 7, 27))),
            store.set_prose(&alpha, "a paragraph somebody wrote"),
        );
        fact.expect("capture ok").written().expect("not blocked");
        prose.expect("set_prose ok");
        let scanned = store
            .scan_entity(&EntityId::person("alpha"))
            .await
            .expect("scan ok")
            .expect("a doc");
        assert!(
            scanned.prose.contains("a paragraph somebody wrote"),
            "the prose was lost to a racing capture: {:?}",
            scanned.prose
        );
        assert!(
            scanned.facts.iter().any(|f| f.content == "plays go"),
            "the fact was lost to a racing prose write: {:?}",
            scanned.facts
        );
    }

    /// **The write mangles, and then the rollback fails too.**
    ///
    /// The one double that can reach a stranded record: the first
    /// `update_document` goes through the poisoned fake, so the read-back
    /// mismatches and a restore is attempted; every update after that is
    /// refused, so the restore is the one that fails. A double that failed the
    /// FIRST write would never reach a rollback at all, which is why this
    /// counts rather than simply erroring.
    struct RollbackFails {
        inner: Arc<FakeOutline>,
        armed: std::sync::atomic::AtomicBool,
        mangled: std::sync::atomic::AtomicBool,
    }

    impl RollbackFails {
        fn over(inner: Arc<FakeOutline>) -> Arc<Self> {
            Arc::new(RollbackFails {
                inner,
                armed: std::sync::atomic::AtomicBool::new(false),
                mangled: std::sync::atomic::AtomicBool::new(false),
            })
        }

        /// **Armed by the test, after its fixture is in place.** Setting the
        /// trap at construction would spring it on whatever the setup writes,
        /// and the write under test would never reach a rollback at all.
        fn arm(&self) {
            self.armed.store(true, Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl OutlineApi for RollbackFails {
        async fn list_collections(
            &self,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<CollectionRec>, MemoryError> {
            self.inner.list_collections(offset, limit).await
        }
        async fn create_collection(
            &self,
            name: &str,
            description: &str,
        ) -> Result<CollectionRec, MemoryError> {
            self.inner.create_collection(name, description).await
        }
        async fn list_documents(
            &self,
            collection_id: &str,
            offset: u64,
            limit: u64,
        ) -> Result<Vec<DocRec>, MemoryError> {
            self.inner
                .list_documents(collection_id, offset, limit)
                .await
        }
        async fn create_document(
            &self,
            collection_id: &str,
            title: &str,
            text: &str,
            parent_id: Option<&str>,
        ) -> Result<DocRec, MemoryError> {
            self.inner
                .create_document(collection_id, title, text, parent_id)
                .await
        }
        async fn update_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            if !self.armed.load(Ordering::SeqCst) {
                return self.inner.update_document(id, text).await;
            }
            // The write under test: it lands, mangled, so the read-back
            // mismatches and a rollback is attempted.
            if !self.mangled.swap(true, Ordering::SeqCst) {
                self.inner.poison_next_update();
                return self.inner.update_document(id, text).await;
            }
            // …and the rollback is the write that fails.
            Err(MemoryError::Store("the store refuses this write".into()))
        }
        async fn append_document(&self, id: &str, text: &str) -> Result<(), MemoryError> {
            self.inner.append_document(id, text).await
        }
        async fn move_document(
            &self,
            id: &str,
            collection_id: &str,
            parent_id: Option<&str>,
        ) -> Result<(), MemoryError> {
            self.inner.move_document(id, collection_id, parent_id).await
        }
    }

    /// **A failed rollback is a VARIANT, not a sentence.**
    ///
    /// `MemoryError::Stranded` exists because the last time this was carried as
    /// prose inside a general store error, detecting it meant string-matching
    /// that prose — so rewording it silently broke the detection with every
    /// test green. The storage move re-introduced exactly that: `restore`
    /// returned a sentence and every call site interpolated it into a
    /// `Store(...)`, and the variant went unconstructed. Same failure, same
    /// clothes, same place.
    ///
    /// What makes this the test that matters is that it asserts on the SHAPE. A
    /// version that gets the words right and the variant wrong fails here.
    #[tokio::test]
    async fn a_write_whose_rollback_also_fails_comes_back_as_the_stranded_variant() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        fake.seed_document(&coll, "Alpha", &seeded_doc(&person("alpha")));

        let api = RollbackFails::over(fake);
        let store = OutlineStore::from_api(api.clone(), COLL);
        // **The edge is load-bearing in the fixture.** The induced fault drops
        // every row's LAST cell and the store re-pads it; on a row whose edge
        // cell is already empty that is a no-op, the read-back matches, and the
        // write simply succeeds — no rollback, nothing to strand.
        ensure(&store, &EntityId("place:shelbyville".into())).await;
        api.arm();
        let outcome = store
            .capture(NewFact {
                subject: EntityId::person("alpha"),
                content: "spending the winter away".into(),
                details: None,
                provenance: Provenance::Testimony,
                status: FactStatus::Active,
                date: date(2026, 7, 2),
                edge: Some(Edge::new(
                    EdgeShape::Location,
                    EntityId("place:shelbyville".into()),
                )),
                event: None,
            })
            .await;

        let Err(err) = outcome else {
            panic!("a mangled write with a failed rollback must not report success");
        };
        let MemoryError::Stranded {
            verb,
            stranded,
            rollback,
            ..
        } = &err
        else {
            panic!(
                "a failed rollback must be its own variant, not a sentence inside a store \
                 error — that is the bug this variant exists to prevent: {err:?}"
            );
        };
        assert_eq!(verb, "capture");
        assert!(
            !stranded.is_empty(),
            "the caller has to be told WHAT is left mid-write: {err:?}"
        );
        assert!(
            !rollback.is_empty(),
            "…and why it could not be put back: {err:?}"
        );
    }

    /// **The same shape, in the mailbox context.** Three contexts restore
    /// identically and each has its own `Stranded`; a test in one of them
    /// proves nothing about the other two, and it was all three that had the
    /// variant sitting unconstructed.
    ///
    /// `notes` is the last column of a message row, which is what makes
    /// `mark_processed` with a note the write the induced fault can spoil.
    #[tokio::test]
    async fn a_mailbox_write_whose_rollback_also_fails_is_stranded_too() {
        use jojobot_domain::mailbox::Mailboxes as _;

        let fake = FakeOutline::new();
        let api = RollbackFails::over(fake);
        let outline = OutlineStore::from_api(api.clone(), COLL);
        let owner = EntityId::new(EntityKind::Bot, "gamma");
        ensure(&outline, &owner).await;
        let mailboxes = outline.mailboxes();
        let name = jojobot_domain::mailbox::MailboxName("gamma".into());
        mailboxes
            .create_mailbox(&name, &owner, false)
            .await
            .expect("the box opens")
            .written()
            .expect("not blocked");
        let posted = mailboxes
            .post_message(jojobot_domain::mailbox::NewMessage {
                mailbox: name,
                body: "the shipment landed".into(),
                subject: None,
                sender: "bot:delta".into(),
                sent_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
                in_reply_to: None,
            })
            .await
            .expect("post ok")
            .written()
            .expect("not blocked");

        api.arm();
        let outcome = mailboxes.mark_processed(&posted.id, Some("acted on")).await;

        let Err(err) = outcome else {
            panic!("a mangled write with a failed rollback must not report success");
        };
        assert!(
            matches!(err, jojobot_domain::mailbox::MailboxError::Stranded { .. }),
            "a failed rollback must be its own variant, not a sentence inside a store error: \
             {err:?}"
        );
    }

    /// …and in the session context, where `focus` is the last column and
    /// `begin` is the write that fills it.
    #[tokio::test]
    async fn a_session_write_whose_rollback_also_fails_is_stranded_too() {
        use jojobot_domain::session::Sessions as _;

        let fake = FakeOutline::new();
        let api = RollbackFails::over(fake);
        let sessions = OutlineStore::from_api(api.clone(), COLL).sessions();
        let bot = EntityId::new(EntityKind::Bot, "gamma");
        // One run first, so the page and its table exist before the trap is set.
        sessions
            .begin(jojobot_domain::session::NewSession {
                bot: bot.clone(),
                sid: jojobot_domain::session::Sid("ab12".into()),
                focus: "the first run".into(),
                started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
            })
            .await
            .expect("begin ok");

        api.arm();
        let outcome = sessions
            .begin(jojobot_domain::session::NewSession {
                bot,
                sid: jojobot_domain::session::Sid("cd34".into()),
                focus: "the second run".into(),
                started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
            })
            .await;

        let Err(err) = outcome else {
            panic!("a mangled write with a failed rollback must not report success");
        };
        assert!(
            matches!(err, jojobot_domain::session::SessionError::Stranded { .. }),
            "a failed rollback must be its own variant, not a sentence inside a store error: \
             {err:?}"
        );
    }

    /// A write whose read-back mismatches restores the page it found. The
    /// production incident stranded a half-written row behind the error — a
    /// retry would have duplicated it. Data the caller handed us is not lost
    /// either way: the error itself still carries the whole fact.
    #[tokio::test]
    async fn a_failed_write_leaves_the_page_as_it_found_it() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc0 = with_fact_appended(
            &seeded_doc(&person("alpha")),
            // Full-width, because this test asserts a BYTE-IDENTICAL restore.
            // The store pads any short row out to the header on write, so a
            // legacy-width seed comes back one cell wider than it went in and
            // the restore looks like a corruption when it is the ordinary
            // migration doing its job.
            "| f1 | person:alpha | plays go |  | testimony | active | 2026-07-01 |  |  |",
        );
        let id = fake.seed_document(&coll, "Alpha", &doc0);

        let store = store(fake.clone());
        ensure(&store, &EntityId("place:shelbyville".into())).await;
        let before = fake
            .docs_in(&coll)
            .into_iter()
            .find(|d| d.id == id)
            .expect("alpha doc")
            .text;

        fake.poison_next_update();
        let outcome = store
            .capture(NewFact {
                subject: EntityId::person("alpha"),
                content: "spending the winter away".into(),
                details: None,
                provenance: Provenance::Testimony,
                status: FactStatus::Active,
                date: date(2026, 7, 2),
                edge: Some(Edge::new(
                    EdgeShape::Location,
                    EntityId("place:shelbyville".into()),
                )),
                event: None,
            })
            .await;
        assert!(
            outcome.is_err(),
            "a mangled write must not report success: {outcome:?}"
        );

        let after = fake
            .docs_in(&coll)
            .into_iter()
            .find(|d| d.id == id)
            .expect("alpha doc")
            .text;
        assert_eq!(after, before, "the page is restored to its pre-write state");
        let facts = store
            .recall(&EntityId::person("alpha"))
            .await
            .expect("recall ok");
        assert_eq!(
            facts.len(),
            1,
            "no half-written row remains for a retry to duplicate"
        );
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
        assert_eq!(
            indexed.rebuild().await.expect("rebuild"),
            1,
            "one doc scanned"
        );

        let hits = indexed
            .search(&SearchQuery::text("penicillin"))
            .expect("search ok");
        let prose: Vec<&Hit> = hits
            .iter()
            .filter(|h| matches!(h, Hit::Prose { .. }))
            .collect();
        assert_eq!(prose.len(), 1, "the prose match must be findable: {hits:?}");
        let Some(Hit::Prose {
            entity, snippet, ..
        }) = prose.first().copied()
        else {
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
        let facts = indexed
            .search(&SearchQuery::text("chess"))
            .expect("search ok");
        assert!(
            facts
                .iter()
                .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
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

        let hits = indexed
            .search(&SearchQuery::text("pass closed"))
            .expect("search ok");
        assert!(
            hits.iter().any(
                |h| matches!(h, Hit::Prose { snippet, .. } if snippet.contains("pass was closed"))
            ),
            "the note must come back as a prose hit: {hits:?}"
        );
        // …and the fact beside it is untouched by the wider prose boundary.
        let facts = indexed
            .search(&SearchQuery::text("chess"))
            .expect("search ok");
        assert!(
            facts
                .iter()
                .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
            "got {facts:?}"
        );
    }

    /// **A prose hit's neighborhood, through the real reader.** The edges come
    /// off the fact table parsed from the same page, so a doc whose answer lives
    /// only in its prose still says where its entity sits in the graph. Until
    /// now this was pinned by a hand-built hit at the MCP boundary — a test that
    /// proves the renderer copies a field it was handed, and never runs the code
    /// that fills it.
    #[tokio::test]
    async fn a_prose_hit_carries_the_edges_its_docs_facts_draw() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let doc = with_fact_appended(
            &format!(
                "Keeps a spare key under the third flowerpot — it came up once and never got \
                 filed.\n\n{}",
                seeded_doc(&person("ned-flanders"))
            ),
            "| f1 | person:ned-flanders | opens on the first Sunday |  | testimony | active | \
             2026-07-01 | location=place:leftorium |",
        );
        fake.seed_document(&coll, "Ned Flanders", &doc);

        let indexed = IndexedMemory::new(Arc::new(store(fake))).expect("index opens");
        assert_eq!(
            indexed.rebuild().await.expect("rebuild"),
            1,
            "one doc scanned"
        );

        let hits = indexed
            .search(&SearchQuery::text("flowerpot"))
            .expect("search ok");
        let Some(Hit::Prose { entity, edges, .. }) =
            hits.iter().find(|h| matches!(h, Hit::Prose { .. }))
        else {
            panic!("the prose match must come back: {hits:?}")
        };
        assert_eq!(
            entity.as_ref().map(|e| &e.id),
            Some(&EntityId::person("ned-flanders"))
        );
        assert_eq!(
            edges,
            &vec![Edge::new(
                EdgeShape::Location,
                EntityId("place:leftorium".into())
            )],
            "a prose hit carries the edges its doc's facts draw: {edges:?}"
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
        assert_eq!(
            facts.len(),
            1,
            "the doc's own row must be reachable: {facts:?}"
        );
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
                FactPatch {
                    content: Some("plays go".into()),
                    ..Default::default()
                },
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
            hits.iter()
                .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays chess")),
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
        assert!(
            !scanned[0].doc_id.is_empty(),
            "a scan always says which doc"
        );
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
                FactPatch {
                    content: Some("takes the 7am train".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("the addressed row updates");

        let text = &fake.docs_in(&coll)[0].text;
        assert!(
            text.contains("allergic to penicillin"),
            "the row the caller never saw must survive untouched: {text}"
        );
        assert!(text.contains("takes the 7am train"));
        assert!(
            !text.contains("takes the 8am train"),
            "the edit rewrote its own row"
        );
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
                &FactAddress::new(
                    EntityId::person("alpha"),
                    jojobot_domain::memory::FactId("f1".into()),
                ),
                FactPatch {
                    content: Some("should not land".into()),
                    ..Default::default()
                },
            )
            .await
            .expect_err("an unreadable row is not addressable");
        assert!(
            matches!(err, MemoryError::UnknownFact { .. }),
            "got {err:?}"
        );
        assert!(
            fake.docs_in(&coll)[0]
                .text
                .contains("allergic to penicillin"),
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
        capture(
            &store,
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
        )
        .await;

        let facts = store.recall(&subject).await.unwrap();
        assert_eq!(
            facts.len(),
            2,
            "both facts live in the one table: {facts:?}"
        );
        assert!(
            fake.docs_in(&coll)[0]
                .text
                .contains("note: do not edit below")
        );
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
                EntityPatch {
                    name: Some("Alpha Renamed".into()),
                    ..Default::default()
                },
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

        capture(
            &store,
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 2)),
        )
        .await;
        assert_eq!(fake.docs_in(&coll).len(), 1, "no second doc was forked");
        assert_eq!(
            store.recall(&subject).await.unwrap().len(),
            2,
            "both facts reachable"
        );
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
        assert_eq!(
            before.len(),
            2,
            "the page's own facts must be readable: {before:?}"
        );

        capture(
            &store,
            NewFact::about(subject.clone(), "learning Rust", date(2026, 7, 3)),
        )
        .await;

        let after = store.recall(&subject).await.expect("recall");
        assert_eq!(
            after.len(),
            3,
            "the new fact lands beside the old ones: {after:?}"
        );
        assert_eq!(
            after[2].id.as_str(),
            "f3",
            "the ids already on the page are taken"
        );
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

        assert_eq!(
            fake.owned_named(COLL),
            1,
            "jojobot made its own owned collection"
        );
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

        assert_eq!(
            fake.owned_named(COLL),
            1,
            "must find the paged-past match, not fork"
        );
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
                event: None,
            }),
        );
        fake.seed_document(&coll, "Totally Unrelated Title", &text);

        let facts = store(fake)
            .recall(&EntityId::person("alpha"))
            .await
            .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].content, "plays go");
    }

    #[tokio::test]
    async fn a_renamed_title_does_not_orphan_or_duplicate_the_doc() {
        let fake = FakeOutline::new();
        let subject = EntityId::person("alpha");

        // First capture creates the doc.
        capture(
            &store(fake.clone()),
            NewFact::about(subject.clone(), "plays go", date(2026, 7, 1)),
        )
        .await;
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

        assert_eq!(
            fake.docs_in(&coll).len(),
            1,
            "no duplicate doc spawned on rename"
        );
        let facts = store(fake).recall(&subject).await.unwrap();
        assert_eq!(facts.len(), 2, "both facts live in the one doc");
    }

    #[tokio::test]
    async fn reconciles_duplicate_docs_to_the_oldest_canonical() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let marker = &seeded_doc(&person("alpha"));
        let older = with_fact_appended(
            marker,
            "| f1 | person:alpha | older fact |  | testimony | active | 2026-07-01 |",
        );
        let newer = with_fact_appended(
            marker,
            "| f1 | person:alpha | newer fact |  | testimony | active | 2026-07-02 |",
        );
        fake.seed_document(&coll, "a", &older);
        fake.seed_document(&coll, "b", &newer);

        let facts = store(fake)
            .recall(&EntityId::person("alpha"))
            .await
            .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(
            facts[0].content, "older fact",
            "the oldest doc is canonical"
        );
    }

    #[tokio::test]
    async fn pages_beyond_100_docs_before_concluding_absent() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        for i in 0..120 {
            fake.seed_document(
                &coll,
                &format!("other-{i}"),
                &seeded_doc(&person(&format!("other-{i}"))),
            );
        }
        let target = with_fact_appended(
            &seeded_doc(&person("alpha")),
            // A row in the pre-`details` format — the paged-past doc is also the
            // legacy-row regression, read through the real store.
            "| f1 | person:alpha | found me | testimony | active | 2026-07-01 |",
        );
        fake.seed_document(&coll, "entity doc", &target);

        let facts = store(fake)
            .recall(&EntityId::person("alpha"))
            .await
            .unwrap();
        assert_eq!(facts.len(), 1, "must find the paged-past doc");
        assert_eq!(facts[0].content, "found me");
    }

    // ── restored: deleted by 64d54bf, which was about `documents.move` ──────
    //
    // A rewrite of this module took 204 lines out and put 212 back, and four
    // tests went with them. The commit message says nothing about tests, so
    // nothing marked their absence: `make check` stayed green over an adapter
    // with no coverage at any tier. Two of them are the ONLY thing standing
    // between the operator's mail and markdown normalization.

    /// **The Mailboxes contract, unchanged, over Outline.** Same claim as the
    /// sessions one: the spec is untouched, so this was a storage move.
    #[tokio::test]
    async fn the_outline_mailbox_store_satisfies_the_contract() {
        jojobot_domain::mailbox::testing::contract::run_all(|| async {
            // **The owners are written first, because this store resolves them
            // by reading Memory.** A box belongs to a bot by construction, so
            // `create_mailbox` refuses an owner it cannot find — which is the
            // contract's stated precondition and the reason its factory is
            // async. The fake meets it in its constructor; here it is I/O.
            let outline = store(FakeOutline::new());
            for owner in jojobot_domain::mailbox::testing::contract::OWNERS {
                outline
                    .add_entity(jojobot_domain::memory::NewEntity {
                        id: jojobot_domain::memory::EntityId((*owner).to_string()),
                        name: owner.trim_start_matches("bot:").to_string(),
                        aliases: Vec::new(),
                        source: "user-named".into(),
                        crm: None,
                        parent: None,
                        boot: Default::default(),
                        create_new: false,
                    })
                    .await
                    .expect("the owner is written")
                    .written()
                    .expect("not blocked");
            }
            outline.mailboxes()
        })
        .await;
    }

    /// **The write lock is what makes two posts into one box two messages, and
    /// nothing in this context was holding it to that.**
    ///
    /// Every mailbox write is a read-modify-write over a whole page: read it,
    /// mint the next id off what is on it, append the body, then put the WHOLE
    /// table back. Two posts running at once both read the same page, both mint
    /// the same next id, and both write a table built from a page that no
    /// longer exists — so the second put erases the first message and both
    /// callers hold a `Message` saying otherwise. Nothing on the surface can
    /// then find it: it is not `new`, not `read`, not quarantined, not
    /// anywhere.
    ///
    /// **Deleting the lock from this whole context left the suite green.** That
    /// is the finding — the mailbox tier had no test that could tell a
    /// linearized store from a racing one, so the mechanism the rule rests on
    /// was load-bearing and unguarded. Sessions had this test; mailboxes did
    /// not, and they share the lock precisely because they write different
    /// documents in one collection.
    ///
    /// Run through the yielding transport, which suspends **after** a write
    /// commits as well as before, because that is where the network suspends: a
    /// real put is a round trip and the page has changed server-side before the
    /// response arrives. A double that only yielded before the call would pass
    /// this on broken code.
    #[tokio::test]
    async fn two_messages_posted_at_once_into_one_box_both_survive() {
        use jojobot_domain::mailbox::Mailboxes as _;

        let fake = FakeOutline::new();
        let outline = OutlineStore::from_api(Arc::new(Yielding(fake)), COLL);
        let owner = EntityId::new(EntityKind::Bot, "gamma");
        outline
            .add_entity(jojobot_domain::memory::NewEntity {
                id: owner.clone(),
                name: "gamma".into(),
                aliases: Vec::new(),
                source: "user-named".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                create_new: false,
            })
            .await
            .expect("the owner is written")
            .written()
            .expect("not blocked");
        let mailboxes = Arc::new(outline.mailboxes());
        let name = jojobot_domain::mailbox::MailboxName("gamma".into());
        mailboxes
            .create_mailbox(&name, &owner, false)
            .await
            .expect("the box opens")
            .written()
            .expect("not blocked");

        let post = |sender: &'static str, body: &'static str| {
            let mailboxes = Arc::clone(&mailboxes);
            let mailbox = name.clone();
            async move {
                mailboxes
                    .post_message(jojobot_domain::mailbox::NewMessage {
                        mailbox,
                        body: body.into(),
                        subject: None,
                        sender: sender.into(),
                        sent_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
                        in_reply_to: None,
                    })
                    .await
                    .expect("post_message should succeed")
                    .written()
                    .expect("not blocked")
            }
        };
        let (one, two) = tokio::join!(
            post("bot:delta", "the first shipment landed"),
            post("bot:epsilon", "the second shipment landed")
        );

        assert_ne!(one.id, two.id, "two messages are two ids, not one id twice");
        let all = mailboxes.scan_messages().await.expect("scan_messages");
        assert_eq!(all.len(), 2, "neither write was lost: {all:?}");
        for posted in [&one, &two] {
            let seen = all
                .iter()
                .find(|m| m.id == posted.id)
                .unwrap_or_else(|| panic!("{} is not on the page: {all:?}", posted.id));
            assert_eq!(seen.body, posted.body, "…and each kept its own body");
            assert_eq!(seen.sender, posted.sender);
        }
    }

    /// **The Sessions contract, unchanged, over Outline.** The same spec the
    /// fake satisfies and the Vikunja adapter satisfied — that it passes here
    /// with no edit to it is the whole proof that this was a storage move and
    /// not a redesign.
    #[tokio::test]
    async fn the_outline_sessions_store_satisfies_the_contract() {
        jojobot_domain::session::testing::contract::run_all(|| {
            store(FakeOutline::new()).sessions()
        })
        .await;
    }

    /// **Mail is in the index; the page it lives on is not.** Both halves, in
    /// one test, because getting either wrong is silent and they fail in
    /// opposite directions.
    ///
    /// Sessions are excluded from search on purpose and mail is included on
    /// purpose, and both now live on pages in the collection the boot scan
    /// reads. So the exclusion has to be surgical: exclude the page's raw
    /// markdown as content, and let the messages through by their own path.
    /// Exclude too much and mail vanishes from search; exclude too little and a
    /// question about the operator's life comes back with the raw markdown of a
    /// box, bodies quoted out of their envelopes.
    #[tokio::test]
    async fn mail_reaches_the_index_but_the_page_it_sits_on_does_not() {
        use jojobot_domain::mailbox::{MailboxName, Mailboxes as _, NewMessage};
        use jojobot_domain::memory::search::{Hit, Search, SearchQuery};

        let outline = store(FakeOutline::new());
        let index = IndexedMemory::new(Arc::new(outline.clone())).expect("index opens");
        let mail =
            crate::search::IndexedMailboxes::new(Arc::new(outline.mailboxes()), index.index());

        // A box has an owner now — it belongs to a bot and is named for it.
        // That is the one adaptation this recovered test needed; everything it
        // asserts about the index is untouched.
        let inbox = MailboxName("gamma".into());
        let owner = jojobot_domain::memory::EntityId("bot:gamma".into());
        outline
            .add_entity(jojobot_domain::memory::NewEntity {
                id: owner.clone(),
                name: "Gamma".into(),
                aliases: Vec::new(),
                source: "user-named".into(),
                crm: None,
                parent: None,
                boot: Default::default(),
                create_new: false,
            })
            .await
            .expect("the owner exists")
            .written()
            .expect("not blocked");
        mail.create_mailbox(&inbox, &owner, false)
            .await
            .expect("create ok")
            .written()
            .expect("not blocked");
        mail.post_message(NewMessage {
            mailbox: inbox.clone(),
            body: "the monorail contract needs a decision".into(),
            subject: Some("monorail".into()),
            sender: "gamma".into(),
            sent_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
            in_reply_to: None,
        })
        .await
        .expect("post ok")
        .written()
        .expect("not blocked");

        // **Direction one, through the BOOT path.** Searching the index this
        // process has been writing to proves only that the incremental write
        // works — it survives `scan_messages` returning nothing, which is the
        // failure that matters: a restart rebuilds from that read, and a broken
        // one loses every message older than the process while looking fine.
        // So this is a restart: a fresh index, both halves rebuilt from the
        // store, and only then the question.
        let restarted = IndexedMemory::new(Arc::new(outline.clone())).expect("index opens");
        restarted.rebuild().await.expect("memory rebuild ok");
        let restarted_mail =
            crate::search::IndexedMailboxes::new(Arc::new(outline.mailboxes()), restarted.index());
        restarted_mail.rebuild().await.expect("mail rebuild ok");

        let hits = restarted
            .search(&SearchQuery {
                text: Some("monorail".into()),
                ..Default::default()
            })
            .expect("search ok");
        assert!(
            hits.iter().any(|h| matches!(h, Hit::Message { .. })),
            "mail survives a restart and is in the one ranked list: {hits:?}"
        );

        // Direction two, on that same rebuilt index: the page carrying the mail
        // is not content. The rebuild is what reads every document, so this is
        // the path where a leak would appear.
        let after = restarted
            .search(&SearchQuery {
                text: Some("monorail".into()),
                ..Default::default()
            })
            .expect("search ok");
        assert!(
            !after
                .iter()
                .any(|h| matches!(h, Hit::Prose { .. } | Hit::Entity { .. })),
            "the raw page must never surface as content: {after:?}"
        );
    }

    /// **The sessions half of the same exclusion — a session page is machinery,
    /// not content.**
    ///
    /// `64d54bf` deleted `jojobots_own_machinery_is_not_scanned_into_the_index`,
    /// which covered BOTH flavours. Its sibling above covers mail; this covers
    /// sessions, and without it nothing at HEAD asserted the property for the
    /// pages that carry a run's whole record.
    ///
    /// **What leaks if this breaks is not a stray marker.** A session page holds
    /// every focus line, every chronology entry and the closing story of every
    /// run of that bot. One unguarded filter in `scan` is all that keeps them
    /// out, and a question about the operator's life would come back answered
    /// with an agent's private working notes.
    ///
    /// Written through the real path — `begin` then `append`, so the page is
    /// produced exactly as production produces it — rather than by seeding
    /// markdown, which would prove only that a hand-written marker is honoured.
    #[tokio::test]
    async fn a_sessions_page_is_machinery_and_never_scanned_into_the_index() {
        use jojobot_domain::memory::search::{Search, SearchQuery};
        use jojobot_domain::session::{NewEntry, NewSession, Sessions as _, Sid};

        let outline = store(FakeOutline::new());
        let sessions = outline.sessions();

        // Two strings that exist nowhere else, so a hit can only have come off
        // the session page: one in the focus, one in a chronology entry.
        const FOCUS: &str = "chasing the monorail flake";
        const ENTRY: &str = "ruled out the escaping, it is the read-back";

        let begun = sessions
            .begin(NewSession {
                bot: EntityId::new(EntityKind::Bot, "gamma"),
                sid: Sid("ab12".into()),
                focus: FOCUS.into(),
                started_at: "2026-07-28T00:00:00Z".parse().expect("a timestamp"),
            })
            .await
            .expect("begin ok");
        sessions
            .append(
                &begun.id,
                NewEntry::manual(ENTRY, "2026-07-28T01:00:00Z".parse().expect("a timestamp")),
            )
            .await
            .expect("append ok");

        // A restart: the index is rebuilt by reading every document, which is
        // the path a leak appears on. Searching an index this process has been
        // writing to would prove nothing about what the scan admits.
        let restarted = IndexedMemory::new(Arc::new(outline.clone())).expect("index opens");
        restarted.rebuild().await.expect("rebuild ok");

        for secret in [FOCUS, ENTRY] {
            let hits = restarted
                .search(&SearchQuery {
                    text: Some(secret.into()),
                    ..Default::default()
                })
                .expect("search ok");
            assert!(
                hits.is_empty(),
                "a session's own record must never be reachable as content — {secret:?} came \
                 back as {hits:?}"
            );
        }
    }

    /// The fake stores what the real Outline would store: the editor model
    /// re-serializes every markdown table RECTANGULAR AT THE HEADER'S WIDTH —
    /// long rows lose their tail, short rows are padded. Pinned so the fake can
    /// never quietly regress to the verbatim store that hid the production
    /// edge-loss bug from 217 green tests.
    #[tokio::test]
    async fn the_fake_rectangularizes_tables_like_the_real_store() {
        let fake = FakeOutline::new();
        let coll = fake.seed_collection(COLL, &owned_desc());
        let id = fake.seed_document(&coll, "Alpha", "seed");
        fake.update_document(
            &id,
            "| id | subject | content |\n\
             | --- | --- | --- |\n\
             | f1 | person:alpha | plays go | EXTRA |\n\
             | f2 | person:alpha |",
        )
        .await
        .expect("update ok");

        let doc = fake
            .docs_in(&coll)
            .into_iter()
            .find(|d| d.id == id)
            .expect("doc");
        let lines: Vec<&str> = doc.text.lines().collect();
        assert!(
            !lines[2].contains("EXTRA"),
            "a cell past the header's width is truncated on save: {:?}",
            lines[2]
        );
        assert_eq!(
            split_cells(lines[3]).len(),
            3,
            "a short row is padded to the header's width: {:?}",
            lines[3]
        );
    }
}
