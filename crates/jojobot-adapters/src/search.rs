//! The search projection — jojobot's front door, and the [`Memory`] decorator
//! that keeps it honest.
//!
//! Two pieces, deliberately separable:
//!
//! * [`FullTextIndex`] — an in-RAM tantivy index over **entities, facts and
//!   prose at once**, satisfying the domain's [`Search`] port. Truth stays in the
//!   store; this is a projection, and it is allowed to be one only because it is
//!   rebuilt from a full re-scan and never written to directly.
//! * [`IndexedMemory`] — the same Memory port, wrapped so that **read-back
//!   extends to the index**: after any successful write, the touched document is
//!   re-scanned *from the store* and re-indexed, so a fact captured a moment ago
//!   is findable with no restart. Re-reading rather than patching is the point —
//!   a partial-update bug has nowhere to live.
//!
//! Ranking is hardcoded, not configurable: text relevance, a small recency
//! boost, and an entity whose handle or name the query matches pinned to the top
//! — pinned by the **write guard's own matcher**, so search and the guard can
//! never disagree about what "that's the same thing" means.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, TEXT, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term, doc};

use jiff::civil::Date;
use jojobot_domain::mailbox::{MailboxError, Mailboxes, Message};
use jojobot_domain::memory::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    Guarded, Memory, MemoryError, NewEntity, NewFact, Retraction,
    guard::{self, MatchReason},
    search::{self, Behind, Coverage, DocScan, EntityRef, Hit, Search, SearchQuery},
};

/// How much a fresh fact is worth against text relevance. Small on purpose: it
/// breaks ties and pulls a newer fact past an equally-relevant older one, and it
/// never buries a better match.
const RECENCY_WEIGHT: f32 = 0.1;

/// How many candidates to pull per hit class before the recency re-rank, so an
/// item that the boost would have lifted into the page isn't cut before it can be.
fn candidate_depth(limit: usize) -> usize {
    limit.saturating_mul(3).saturating_add(10)
}

/// What the index stores per document, and hands back verbatim. Not the wire
/// format and not [`Hit`]: keeping it separate means the response shape can
/// change without a reindex, and prose can carry its whole body here while the
/// hit carries only a snippet.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
enum Payload {
    Entity {
        entity: Entity,
        doc_id: String,
    },
    Fact {
        fact: Fact,
    },
    Prose {
        doc_id: String,
        title: String,
        entity: Option<EntityId>,
        body: String,
    },
    Message {
        message: Message,
    },
}

/// The hit-class token, indexed so a query can ask for one class of thing.
const CLASS_ENTITY: &str = "entity";
const CLASS_FACT: &str = "fact";
const CLASS_PROSE: &str = "prose";
const CLASS_MESSAGE: &str = "message";

/// The index's fields. One schema for all three hit classes — a mixed ranked list
/// is the requirement, and one schema is what makes it one query.
struct Fields {
    /// Which class of thing this document is.
    class: Field,
    /// Everything searchable, tokenized: handles, names, claims, details, prose.
    text: Field,
    /// The store's doc id — the unit of incremental re-indexing.
    doc_id: Field,
    /// A message's id — the mail half's unit of incremental re-indexing. Its own
    /// field rather than `doc_id`: the two ids come from two different stores
    /// and share no namespace, so one field would make an Outline page id and a
    /// card id capable of evicting each other.
    message_id: Field,
    /// The entity kind this document is filed under.
    kind: Field,
    /// A fact's subject handle.
    subject: Field,
    /// A fact's lifecycle state.
    status: Field,
    /// A fact's provenance.
    provenance: Field,
    /// A fact's edge shape, and the handle its edge points at.
    edge_shape: Field,
    edge_object: Field,
    /// **One term per edge, shape and object together.**
    ///
    /// A fact can carry several edges now — its own, plus one per entity its
    /// event payload points at — and the two fields above are independent, so
    /// `shape=location AND object=person:alpha` would match a fact holding a
    /// location edge to somewhere else and an unrelated link to alpha. Neither
    /// field is wrong; the pair is what the caller actually asked about.
    edge_pair: Field,
    /// The stored [`Payload`], as JSON.
    payload: Field,
}

impl Fields {
    fn build() -> (Schema, Self) {
        let mut b = Schema::builder();
        let fields = Fields {
            class: b.add_text_field("class", STRING),
            text: b.add_text_field("text", TEXT),
            doc_id: b.add_text_field("doc_id", STRING),
            message_id: b.add_text_field("message_id", STRING),
            kind: b.add_text_field("kind", STRING),
            subject: b.add_text_field("subject", STRING),
            status: b.add_text_field("status", STRING),
            provenance: b.add_text_field("provenance", STRING),
            edge_shape: b.add_text_field("edge_shape", STRING),
            edge_object: b.add_text_field("edge_object", STRING),
            edge_pair: b.add_text_field("edge_pair", STRING),
            payload: b.add_text_field("payload", STORED),
        };
        (b.build(), fields)
    }
}

/// What the index remembers about one scanned doc **beside its postings**: the
/// entity it declares, and the edges its rows draw.
///
/// Two jobs, one mirror. The write guard's matcher screens a query against
/// entities rather than postings, and a hit has to arrive with its surroundings
/// — the name behind a handle, the edges around an entity. Both are lookups by
/// id over a corpus of dozens of docs, and neither is a text search, so neither
/// belongs in tantivy.
///
/// Resolution happens on the way **out**, never at ingest: a renamed entity has
/// to change every hit that names it, not just the hits re-indexed since.
struct DocMirror {
    /// The store's id for the doc — the key everything is retained by.
    doc_id: String,
    /// The entity this doc declares, if it declares one.
    entity: Option<Entity>,
    /// Each row's subject and the edge it draws, for the rows that draw one.
    edges: Vec<(EntityId, Edge)>,
    /// The scan these postings were written from.
    ///
    /// Kept so a later scan can be compared against what the index actually
    /// holds. That comparison is what lets a refresh write only where the store
    /// has moved — see [`FullTextIndex::ingest_changes`] — and it is a
    /// comparison of two full readings rather than a guess at a delta.
    scanned: DocScan,
}

impl DocMirror {
    fn of(scan: &DocScan) -> Self {
        DocMirror {
            doc_id: scan.doc_id.clone(),
            entity: scan.entity.clone(),
            edges: scan
                .facts
                .iter()
                .filter_map(|f| f.edge.clone().map(|e| (f.subject.clone(), e)))
                .collect(),
            scanned: scan.clone(),
        }
    }
}

/// **The comparison both halves of the index make before they write anything.**
///
/// Given what a scan holds and what the index holds, both keyed by the id their
/// postings were written under: the records to rewrite, and the keys to evict.
/// A record is rewritten when the scan and the index disagree about it, and
/// evicted when the scan no longer carries it at all — which is the same answer
/// for a record deleted by hand, one that became unreadable, and one that left
/// any other way. Nothing here asks which happened, so nothing has to be taught
/// a new way for a record to go missing.
///
/// One definition, two callers: the memory half keys on the store's doc id and
/// the mail half on the message id, and that is the whole of the difference
/// between them.
/// The rewrites come back **owned**, because the comparison reads the mirror
/// under its lock and the writing happens after that lock is dropped. The set
/// is empty on the ordinary read, so the clone is paid only when the store has
/// actually moved.
fn changed_and_gone<R: PartialEq + Clone>(
    arriving: &[(&str, &R)],
    held: &[(&str, &R)],
) -> (Vec<R>, Vec<String>) {
    use std::collections::{HashMap, HashSet};
    let held_by_key: HashMap<&str, &R> = held.iter().copied().collect();
    let arriving_keys: HashSet<&str> = arriving.iter().map(|(k, _)| *k).collect();
    (
        arriving
            .iter()
            .filter(|(k, r)| held_by_key.get(k) != Some(r))
            .map(|(_, r)| (*r).clone())
            .collect(),
        held.iter()
            .filter(|(k, _)| !arriving_keys.contains(k))
            .map(|(k, _)| (*k).to_string())
            .collect(),
    )
}

/// The in-RAM full-text index over entities, facts and prose.
pub struct FullTextIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    /// The writer is single-instance per index in tantivy, so it is held and
    /// shared rather than reopened per write.
    writer: RwLock<IndexWriter>,
    /// One entry per scanned doc — see [`DocMirror`]. Kept beside the index
    /// because the guard takes entities, not postings, and reusing it is what
    /// keeps one definition of "the same thing" in the system.
    docs: RwLock<Vec<DocMirror>>,
    /// Whether the mail half was ever loaded from a **board read**.
    ///
    /// **Not "are there messages in it".** An empty board that was read and a
    /// mailbox world that never answered look identical in the postings and are
    /// opposite answers to the caller — see [`Coverage`].
    mail_loaded: std::sync::atomic::AtomicBool,
    /// Whether any single message has been indexed since this index opened.
    ///
    /// Tracked apart from the board read because the two disagree exactly where
    /// it matters: after a failed boot scan, every message this process posts or
    /// delivers still lands in the index and still comes back as a hit, while
    /// no board was ever read. Reporting that as "no mail is searchable" made an
    /// answer carry message hits and deny having searched any.
    mail_touched: std::sync::atomic::AtomicBool,
    /// Whether the memory half was ever loaded from a **full scan**, and whether
    /// any single doc has been indexed since — the mail half's two flags, asked
    /// of the other store and for the same reason.
    memory_loaded: std::sync::atomic::AtomicBool,
    memory_touched: std::sync::atomic::AtomicBool,
    /// The entities whose documents the index knows it is holding an old version
    /// of: a refresh after a committed write that could not be run.
    ///
    /// **Kept by entity rather than by doc id**, because the refresh that failed
    /// is the one that would have learned the doc id — a write to an entity the
    /// index has never seen has no doc id anywhere to key on.
    behind: RwLock<std::collections::BTreeSet<EntityId>>,
    /// Whether the last whole-corpus refresh failed to reach the store.
    ///
    /// The [`behind`](Self::behind) set answers the same question about one
    /// entity, and it cannot answer it about the corpus: a refresh of everything
    /// that never reached the store learned no handles, so there is nothing to
    /// put in a set keyed by them. Both mean the index holds an older version
    /// than the store, so both read out as the same word to a caller.
    memory_refresh_failed: std::sync::atomic::AtomicBool,
    /// The messages these postings were written from — the mail half's mirror,
    /// and the same thing [`docs`](Self::docs) is for the memory half: what a
    /// later board read is compared against so a refresh writes only where the
    /// store has moved.
    messages: RwLock<Vec<Message>>,
    /// Whether the last board read failed to reach the store. The mail half's
    /// [`memory_refresh_failed`](Self::memory_refresh_failed), for the same
    /// reason and read out as the same word.
    mail_refresh_failed: std::sync::atomic::AtomicBool,
}

impl FullTextIndex {
    /// An empty index, ready to be filled by a scan.
    pub fn open() -> Result<Self, MemoryError> {
        let (schema, fields) = Fields::build();
        let index = Index::create_in_ram(schema);
        let writer = index.writer(15_000_000).map_err(store_err)?;
        let reader = index.reader().map_err(store_err)?;
        Ok(FullTextIndex {
            index,
            reader,
            fields,
            writer: RwLock::new(writer),
            docs: RwLock::new(Vec::new()),
            mail_loaded: std::sync::atomic::AtomicBool::new(false),
            mail_touched: std::sync::atomic::AtomicBool::new(false),
            memory_loaded: std::sync::atomic::AtomicBool::new(false),
            memory_touched: std::sync::atomic::AtomicBool::new(false),
            behind: RwLock::new(std::collections::BTreeSet::new()),
            memory_refresh_failed: std::sync::atomic::AtomicBool::new(false),
            messages: RwLock::new(Vec::new()),
            mail_refresh_failed: std::sync::atomic::AtomicBool::new(false),
        })
    }

    /// Record that the board read behind this answer could not reach the store,
    /// so the mail half is a version behind. Cleared by the next board read that
    /// lands.
    pub fn mail_refresh_failed(&self) {
        self.mail_refresh_failed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Record that the refresh this answer should have been built on could not
    /// reach the store, so the whole memory half is a version behind. Cleared by
    /// the next [`ingest_all`](Self::ingest_all), which is the only thing that
    /// can clear it: nothing smaller than a full scan knows the corpus is whole.
    pub fn refresh_failed(&self) {
        self.memory_refresh_failed
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Record that this entity's document is indexed as it stands in the store.
    pub fn current(&self, entity: &EntityId) {
        self.behind.write().expect("behind poisoned").remove(entity);
    }

    /// Record that the index holds an older version of this entity's document
    /// than the store does — a refresh after a committed write that could not
    /// run. Cleared by the next refresh that does.
    pub fn behind(&self, entity: &EntityId) {
        self.behind
            .write()
            .expect("behind poisoned")
            .insert(entity.clone());
    }

    /// The entities the index knows it is behind on.
    ///
    /// **A test observer, and gated so it cannot become anything else.** What a
    /// caller is told is `Behind::Stale` on the coverage — that something is
    /// behind, not which entity — so nothing in an answer or a log reads these
    /// handles. This is how a test asserts the mark goes on and comes off.
    #[cfg(test)]
    fn behind_now(&self) -> Vec<EntityId> {
        self.behind
            .read()
            .expect("behind poisoned")
            .iter()
            .cloned()
            .collect()
    }

    /// Replace the whole **mail** half of the index from a board read — the
    /// boot path for messages, and the mirror of [`ingest_all`](Self::ingest_all)
    /// for the other half.
    ///
    /// The two halves are replaced independently on purpose: they come from two
    /// stores, either one can be down while the other is fine, and a rebuild of
    /// one must never evict the other's hits.
    pub fn ingest_mail(&self, messages: &[Message]) -> Result<(), MemoryError> {
        self.ingest_mail_changes(messages).map(|_| ())
    }

    /// Bring the mail half to what this board read says the board is, writing
    /// only where the two differ — the memory half's
    /// [`ingest_changes`](Self::ingest_changes), asked of the other store.
    ///
    /// A message the board read no longer carries leaves the index: removed by
    /// hand, or on a card that has become unreadable and so is absent from
    /// every board read. Both are the same absence and get the same answer.
    pub fn ingest_mail_changes(&self, messages: &[Message]) -> Result<usize, MemoryError> {
        let (rewrite, evict) = {
            let mirror = self.messages.read().expect("mail mirror poisoned");
            changed_and_gone(
                &messages
                    .iter()
                    .map(|m| (m.id.as_str(), m))
                    .collect::<Vec<_>>(),
                &mirror
                    .iter()
                    .map(|m| (m.id.as_str(), m))
                    .collect::<Vec<_>>(),
            )
        };
        let changed = rewrite.len() + evict.len();

        if changed > 0 {
            let mut writer = self.writer.write().expect("index writer poisoned");
            for id in evict.iter().chain(rewrite.iter().map(|m| &m.id.0)) {
                writer.delete_term(Term::from_field_text(self.fields.message_id, id));
            }
            for message in &rewrite {
                self.write_message(&writer, message)?;
            }
            // **Set before the commit, deliberately.** Whichever side of the
            // commit this lands on, a concurrent searcher can see one and not
            // the other — but the two orders are not equally wrong. Claiming
            // coverage a moment early costs an answer that says it searched mail
            // and returns nothing yet; claiming it a moment late is the one
            // shape the invariant forbids, an answer carrying message hits while
            // denying it searched any.
            self.mail_loaded
                .store(true, std::sync::atomic::Ordering::Release);
            writer.commit().map_err(store_err)?;
            drop(writer);
            *self.messages.write().expect("mail mirror poisoned") = messages.to_vec();
        }
        // Again for the path that wrote nothing: the read still reached the
        // board, which is the only thing this flag reports.
        self.mail_loaded
            .store(true, std::sync::atomic::Ordering::Release);
        self.mail_refresh_failed
            .store(false, std::sync::atomic::Ordering::Release);
        if changed > 0 {
            self.reader.reload().map_err(store_err)?;
        }
        Ok(changed)
    }

    /// Re-index one message, replacing whatever was indexed under its id. What
    /// makes a posted message findable on the next call rather than after a
    /// restart — and what keeps a hit's `state` honest, since every verb that
    /// moves a message re-indexes it.
    pub fn ingest_message(&self, message: &Message) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(
            self.fields.message_id,
            message.id.as_str(),
        ));
        self.write_message(&writer, message)?;
        // Before the commit, for the reason `ingest_mail` sets its flag early.
        self.mail_touched
            .store(true, std::sync::atomic::Ordering::Release);
        writer.commit().map_err(store_err)?;
        drop(writer);
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// One message as the index holds it. **Everything on the envelope is
    /// searchable**, not only the body: the box and the sender are how a reader
    /// asks "what did the pm box say about the kiln" in one query, and a subject
    /// is a title precisely so it can be found by.
    fn write_message(&self, writer: &IndexWriter, message: &Message) -> Result<(), MemoryError> {
        let f = &self.fields;
        writer
            .add_document(doc!(
                f.class => CLASS_MESSAGE,
                f.text => format!(
                    "{} {} {} {}",
                    message.subject.clone().unwrap_or_default(),
                    message.body,
                    message.sender,
                    message.mailbox,
                ),
                f.message_id => message.id.as_str(),
                f.payload => payload_json(&Payload::Message { message: message.clone() })?,
            ))
            .map_err(store_err)?;
        Ok(())
    }

    /// State that this scan is the whole memory corpus.
    ///
    /// **Scoped to the memory classes, never `delete_all_documents`.** The two
    /// halves come from two stores; wiping the whole index here evicted every
    /// message while leaving the flag saying mail was loaded, so a rebuild of
    /// Memory silently emptied `search`'s mail half and then vouched for it.
    /// Only the boot ordering in `main.rs` — untested, and no invariant —
    /// happened to hide it.
    pub fn ingest_all(&self, scan: &[DocScan]) -> Result<(), MemoryError> {
        self.ingest_changes(scan).map(|_| ())
    }

    /// Bring the index to what this scan says the corpus is, and return how many
    /// documents had to be written or evicted to get there.
    ///
    /// **This is not a delta the writer guessed at.** It compares two full
    /// readings of the store — the scan in hand, and the mirror of the scan the
    /// postings were written from — so a document is rewritten when its scanned
    /// content differs and for no other reason. A projection that drifts is
    /// still worse than one that is rebuilt; nothing here decides what changed,
    /// it only declines to rewrite what did not.
    ///
    /// **Declining matters because this runs on every read.** A full rewrite
    /// takes the writer lock, commits the index and rebuilds the mirror, so
    /// running one per answer would serialize every search in the process behind
    /// a commit apiece, and would grow with the corpus rather than with what
    /// changed. When nothing changed — the ordinary case — no lock is taken, no
    /// commit runs and the reader is not reloaded.
    ///
    /// The coverage flags are set either way: reaching the store is what they
    /// report, and a scan that found nothing new still reached it.
    pub fn ingest_changes(&self, scan: &[DocScan]) -> Result<usize, MemoryError> {
        let (rewrite, evict) = {
            let mirror = self.docs.read().expect("doc mirror poisoned");
            changed_and_gone(
                &scan
                    .iter()
                    .map(|d| (d.doc_id.as_str(), d))
                    .collect::<Vec<_>>(),
                &mirror
                    .iter()
                    .map(|d| (d.doc_id.as_str(), &d.scanned))
                    .collect::<Vec<_>>(),
            )
        };
        let changed = rewrite.len() + evict.len();

        if changed > 0 {
            let mut writer = self.writer.write().expect("index writer poisoned");
            for doc_id in evict.iter().chain(rewrite.iter().map(|d| &d.doc_id)) {
                writer.delete_term(Term::from_field_text(self.fields.doc_id, doc_id));
            }
            for doc in &rewrite {
                self.write_doc(&writer, doc)?;
            }
            // Before the commit, for the reason `ingest_mail` sets its flag early.
            self.memory_loaded
                .store(true, std::sync::atomic::Ordering::Release);
            writer.commit().map_err(store_err)?;
            drop(writer);
            *self.docs.write().expect("doc mirror poisoned") =
                scan.iter().map(DocMirror::of).collect();
        }
        // Again, for the path that wrote nothing: the scan still reached the
        // store, which is the only thing this flag reports.
        self.memory_loaded
            .store(true, std::sync::atomic::Ordering::Release);

        // A full re-scan reads every doc from the store, so nothing is behind
        // any more — including a doc a failed refresh marked before the restart
        // or the rebuild that has just replaced it, and an earlier whole-corpus
        // refresh that never reached the store.
        self.behind.write().expect("behind poisoned").clear();
        self.memory_refresh_failed
            .store(false, std::sync::atomic::Ordering::Release);
        if changed > 0 {
            self.reader.reload().map_err(store_err)?;
        }
        Ok(changed)
    }

    /// Re-index one document, replacing everything previously indexed under its
    /// doc id. Called with a fresh scan of the doc, never with a guess at what
    /// changed.
    pub fn ingest_doc(&self, doc: &DocScan) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(self.fields.doc_id, &doc.doc_id));
        self.write_doc(&writer, doc)?;
        // Before the commit, for the reason `ingest_mail` sets its flag early.
        self.memory_touched
            .store(true, std::sync::atomic::Ordering::Release);
        writer.commit().map_err(store_err)?;
        drop(writer);

        let mut mirror = self.docs.write().expect("doc mirror poisoned");
        mirror.retain(|d| d.doc_id != doc.doc_id);
        mirror.push(DocMirror::of(doc));
        drop(mirror);
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// Every entity the index currently holds — the set an incremental reindex
    /// checks a doc's subjects against.
    pub fn known_entities(&self) -> std::collections::HashSet<EntityId> {
        self.docs
            .read()
            .expect("doc mirror poisoned")
            .iter()
            .filter_map(|d| d.entity.as_ref().map(|e| e.id.clone()))
            .collect()
    }

    /// Drop everything indexed under `entity`'s document — the doc is gone from
    /// the store, so its hits must go with it.
    ///
    /// **Eviction keys on the store's doc id**, looked up in the entity mirror,
    /// because that is what the postings were written under. Deleting by the
    /// handle instead matched nothing in the real store, where a doc id is an
    /// Outline UUID: the page was deleted in the wiki and every hit it ever had
    /// went on being served from the last scan, indefinitely.
    pub fn forget(&self, entity: &EntityId) -> Result<(), MemoryError> {
        let doc_id = self
            .docs
            .read()
            .expect("doc mirror poisoned")
            .iter()
            .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity))
            .map(|d| d.doc_id.clone());
        // Nothing indexed under it: a doc that was never scanned leaves no ghost.
        let Some(doc_id) = doc_id else { return Ok(()) };

        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(self.fields.doc_id, &doc_id));
        writer.commit().map_err(store_err)?;
        drop(writer);

        self.docs
            .write()
            .expect("doc mirror poisoned")
            .retain(|d| d.doc_id != doc_id);
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// Every tantivy document one scanned doc produces: the entity it is, each
    /// fact in its table, and its prose — three classes, one index.
    fn write_doc(&self, writer: &IndexWriter, scan: &DocScan) -> Result<(), MemoryError> {
        let f = &self.fields;
        let owner_kind = scan.entity.as_ref().map(|e| e.kind);
        // Every name the doc's entity answers to, indexed as one string: the
        // nickname the user actually says has to find the thing, or the aliases
        // are a field nobody can reach.
        let owner_labels = scan
            .entity
            .as_ref()
            .map(|e| e.labels().join(" "))
            .unwrap_or_default();

        if let Some(entity) = &scan.entity {
            writer
                .add_document(doc!(
                    f.class => CLASS_ENTITY,
                    f.text => format!("{} {} {}", entity.id, owner_labels, entity.kind),
                    f.doc_id => scan.doc_id.clone(),
                    f.kind => entity.kind.as_token(),
                    f.payload => payload_json(&Payload::Entity {
                        entity: entity.clone(),
                        doc_id: scan.doc_id.clone(),
                    })?,
                ))
                .map_err(store_err)?;
        }

        for fact in &scan.facts {
            // A fact carries the names of the entity whose page it sits on, so
            // asking by nickname reaches the claims and not only the entity
            // record. Only the home doc's labels: a fact about someone else,
            // written here, keeps their handle and not their nickname — the doc
            // does not know it, and looking it up would mean a global pass on
            // every write.
            let subject_labels = if scan.entity.as_ref().is_some_and(|e| e.id == fact.subject) {
                owner_labels.as_str()
            } else {
                ""
            };
            let mut document = doc!(
                f.class => CLASS_FACT,
                f.text => format!(
                    "{} {} {} {}",
                    fact.content,
                    fact.details.clone().unwrap_or_default(),
                    fact.subject,
                    subject_labels
                ),
                f.doc_id => scan.doc_id.clone(),
                f.subject => fact.subject.to_string(),
                f.status => fact.status.as_token(),
                f.provenance => fact.provenance.as_token(),
                f.payload => payload_json(&Payload::Fact { fact: fact.clone() })?,
            );
            // Home-doc membership counts alongside the subject column, exactly as
            // `recall` counts it: a row is reachable under the id its doc
            // declares, so a mistyped subject cell cannot hide a doc's own facts
            // from a subject filter for that doc's entity.
            if fact.home != fact.subject {
                document.add_text(f.subject, fact.home.as_str());
            }
            // A fact is filed under its SUBJECT's kind, not its home's: that is
            // what makes `kind=person` + an edge filter answer "which people".
            if let Some(kind) = fact.subject.kind() {
                document.add_text(f.kind, kind.as_token());
            }
            // **Every edge this fact draws**, its own and its event's. An
            // event's links are `connection`s: the pointer is real, and what
            // the link MEANS is deliberately unrecorded rather than guessed.
            //
            // What makes a payload value one of these is that it IS a handle,
            // not the key it sits under — `ref=` is the unnamed case and a
            // later named field is the same link annotated. Keying this on the
            // literal word `ref` would work today and drop every named
            // reference the day the first type ships.
            let linked = fact
                .event
                .iter()
                .flat_map(|e| e.linked())
                .map(|object| Edge::new(EdgeShape::Connection, object));
            for edge in fact.edge.iter().cloned().chain(linked) {
                document.add_text(f.edge_shape, edge.shape.as_token());
                document.add_text(f.edge_object, edge.object.as_str());
                document.add_text(
                    f.edge_pair,
                    format!("{}={}", edge.shape.as_token(), edge.object),
                );
            }
            writer.add_document(document).map_err(store_err)?;
        }

        if !scan.prose.trim().is_empty() {
            let mut document = doc!(
                f.class => CLASS_PROSE,
                f.text => format!("{} {}", scan.title, scan.prose),
                f.doc_id => scan.doc_id.clone(),
                f.payload => payload_json(&Payload::Prose {
                    doc_id: scan.doc_id.clone(),
                    title: scan.title.clone(),
                    entity: scan.entity.as_ref().map(|e| e.id.clone()),
                    body: scan.prose.clone(),
                })?,
            );
            if let Some(kind) = owner_kind {
                document.add_text(f.kind, kind.as_token());
            }
            writer.add_document(document).map_err(store_err)?;
        }
        Ok(())
    }

    /// The query's text, split the way the index split its own — via the index's
    /// tokenizer, so a query term and an indexed term are the same string.
    ///
    /// Deliberately **not** tantivy's `QueryParser`: our handles are `kind:slug`,
    /// and the parser reads `person:` as a field name and errors. Matching term
    /// by term also means no query syntax to escape and none to be surprised by.
    fn terms_of(&self, text: &str) -> Vec<String> {
        let Some(mut analyzer) = self.index.tokenizers().get("default") else {
            return Vec::new();
        };
        let mut tokens = Vec::new();
        let mut stream = analyzer.token_stream(text);
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        tokens
    }

    /// A `MUST` clause per query term: every term has to appear. Conjunction over
    /// disjunction on purpose — a search that quietly matches "any of these
    /// words" is how a precise question gets a vague answer.
    fn text_clauses(&self, query: &SearchQuery) -> Vec<(Occur, Box<dyn Query>)> {
        query
            .terms()
            .map(|text| {
                self.terms_of(text)
                    .into_iter()
                    .map(|term| {
                        let q: Box<dyn Query> = Box::new(TermQuery::new(
                            Term::from_field_text(self.fields.text, &term),
                            IndexRecordOption::WithFreqs,
                        ));
                        (Occur::Must, q)
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    fn must_term(&self, field: Field, value: &str) -> (Occur, Box<dyn Query>) {
        let q: Box<dyn Query> = Box::new(TermQuery::new(
            Term::from_field_text(field, value),
            IndexRecordOption::Basic,
        ));
        (Occur::Must, q)
    }

    /// The clauses that select **facts**: the hit class, the status default
    /// (active only, unless a status was named), and every filter the caller gave.
    fn fact_clauses(&self, query: &SearchQuery) -> Vec<(Occur, Box<dyn Query>)> {
        let f = &self.fields;
        let mut clauses = self.text_clauses(query);
        clauses.push(self.must_term(f.class, CLASS_FACT));
        // The default is the whole point of the field: a superseded fact stays
        // out of an ordinary search, and `status: superseded` is how it is
        // reached deliberately.
        clauses.push(self.must_term(f.status, query.status.unwrap_or_default().as_token()));
        if let Some(kind) = query.kind {
            clauses.push(self.must_term(f.kind, kind.as_token()));
        }
        if let Some(provenance) = query.provenance {
            clauses.push(self.must_term(f.provenance, provenance.as_token()));
        }
        if let Some(subject) = &query.subject {
            clauses.push(self.must_term(f.subject, subject.as_str()));
        }
        if let Some(edge) = &query.edge {
            // **The PAIR when a shape is named**, because the two halves are
            // independent fields and a fact can carry several edges: matching
            // them separately would answer "has a location edge, and separately
            // touches alpha" to a caller who asked "is located at alpha".
            match edge.shape {
                Some(shape) => clauses.push(self.must_term(
                    f.edge_pair,
                    &format!("{}={}", shape.as_token(), edge.object),
                )),
                None => clauses.push(self.must_term(f.edge_object, edge.object.as_str())),
            }
        }
        clauses
    }

    /// The clauses that select **entities, prose and messages**. Run as a second
    /// query rather than folded into the first: a fact-only filter (a status, an
    /// edge) would otherwise exclude every non-fact hit as a side effect of the
    /// `MUST` it adds, which is not the same thing as the caller asking for facts.
    fn other_clauses(&self, query: &SearchQuery) -> Vec<(Occur, Box<dyn Query>)> {
        let f = &self.fields;
        let mut clauses = self.text_clauses(query);
        // Mail is in unless the caller took it out. A `kind` filter takes it out
        // too, one clause down: a message has no entity kind, so asking for one
        // excludes it exactly as it excludes prose in nobody's doc.
        let mut classes = vec![CLASS_ENTITY, CLASS_PROSE];
        if query.include_mail {
            classes.push(CLASS_MESSAGE);
        }
        let classes: Vec<(Occur, Box<dyn Query>)> = classes
            .into_iter()
            .map(|class| {
                let q: Box<dyn Query> = Box::new(TermQuery::new(
                    Term::from_field_text(f.class, class),
                    IndexRecordOption::Basic,
                ));
                (Occur::Should, q)
            })
            .collect();
        clauses.push((Occur::Must, Box::new(BooleanQuery::new(classes))));
        if let Some(kind) = query.kind {
            // Prose in a doc that is nobody's entity has no kind, so a kind
            // filter excludes it — asking for one kind is asking about entities.
            clauses.push(self.must_term(f.kind, kind.as_token()));
        }
        clauses
    }

    /// Run one clause set and return `(score, payload)` per match.
    fn collect(
        &self,
        clauses: Vec<(Occur, Box<dyn Query>)>,
        limit: usize,
    ) -> Result<Vec<(f32, Payload)>, MemoryError> {
        let searcher = self.reader.searcher();
        // No clauses at all means "everything that matches the filters", and the
        // filters are the clauses — so this only happens for a bare kind-less
        // query, which validation refuses. AllQuery keeps it total anyway.
        let query: Box<dyn Query> = if clauses.is_empty() {
            Box::new(AllQuery)
        } else {
            Box::new(BooleanQuery::new(clauses))
        };
        let top = searcher
            .search(&query, &TopDocs::with_limit(limit))
            .map_err(store_err)?;

        let mut out = Vec::with_capacity(top.len());
        for (score, address) in top {
            let document: TantivyDocument = searcher.doc(address).map_err(store_err)?;
            let raw = document
                .get_first(self.fields.payload)
                .and_then(|v| v.as_str())
                .ok_or_else(|| MemoryError::Store("indexed document lost its payload".into()))?;
            let payload: Payload = serde_json::from_str(raw)
                .map_err(|e| MemoryError::Store(format!("indexed payload: {e}")))?;
            out.push((score, payload));
        }
        Ok(out)
    }

    /// The entities the query names outright — screened by the **write guard's**
    /// matcher, so "close enough to be the same thing" means one thing in this
    /// system, not two. Strongest match first.
    fn pinned(&self, query: &SearchQuery, mirror: &[DocMirror]) -> Vec<Hit> {
        let Some(text) = query.terms() else {
            return Vec::new();
        };
        // A fact-only filter says the caller wants facts; pinning an entity into
        // that answer would be noise.
        if query.is_fact_scoped() {
            return Vec::new();
        }
        let index: Vec<Entity> = mirror.iter().filter_map(|d| d.entity.clone()).collect();
        let matches = guard::screen(&EntityId(text.to_string()), &[text], &index);

        matches
            .into_iter()
            // Only a real naming of the entity pins it. A typo'd *name* inside a
            // longer query is a text match, not a claim about identity.
            .filter(|m| {
                matches!(
                    m.reason,
                    MatchReason::ExactHandle
                        | MatchReason::SameName
                        | MatchReason::SameNameOtherKind
                )
            })
            .filter(|m| query.kind.is_none_or(|k| k == m.kind))
            .filter_map(|m| {
                mirror
                    .iter()
                    .find(|d| d.entity.as_ref().is_some_and(|e| e.id == m.handle))
                    .map(|d| Hit::Entity {
                        entity: d.entity.clone().expect("filtered to docs with an entity"),
                        doc_id: d.doc_id.clone(),
                        edges: edges_of(mirror, &m.handle),
                    })
            })
            .collect()
    }
}

/// The entity a handle names, as far as the mirror knows it. An id that resolves
/// to nothing comes back **unresolved rather than invented** — the orphan case,
/// and the reader is entitled to see it as one.
fn resolve(mirror: &[DocMirror], id: &EntityId) -> EntityRef {
    mirror
        .iter()
        .filter_map(|d| d.entity.as_ref())
        .find(|e| &e.id == id)
        .map(EntityRef::resolved)
        .unwrap_or_else(|| EntityRef::unresolved(id.clone()))
}

/// Where an entity sits in the graph: the edges drawn by the facts **about** it,
/// wherever those rows are homed, deduped and in first-seen order.
///
/// Subject rather than home, deliberately: an edge belongs to the claim, the
/// claim belongs to its subject, and a fact about someone written on another
/// entity's page is ordinary. Homing it elsewhere must not move where the edge
/// appears to point from.
fn edges_of(mirror: &[DocMirror], id: &EntityId) -> Vec<Edge> {
    let mut edges: Vec<Edge> = Vec::new();
    for (subject, edge) in mirror.iter().flat_map(|d| d.edges.iter()) {
        if subject == id && !edges.contains(edge) {
            edges.push(edge.clone());
        }
    }
    edges
}

/// The projection's own reads.
///
/// **Deliberately not the [`Search`] port.** The port promises an answer backed
/// by a scan taken for it, and this type holds no store to take one from — only
/// [`IndexedMemory`] does. Leaving these as the index's own methods makes that a
/// compile error rather than a convention: nothing can be wired to the bare
/// projection and quietly serve whatever it last saw.
impl FullTextIndex {
    /// Two ways in reach [`Stale`](Behind::Stale) on the memory half and one
    /// does here: a board read that could not reach the store. The mail path
    /// indexes the record the store hands back rather than re-reading it, so
    /// there is no write-path way to be behind and none is invented.
    pub fn mail_coverage(&self) -> Coverage {
        use std::sync::atomic::Ordering::Acquire;
        match (
            self.mail_loaded.load(Acquire),
            self.mail_touched.load(Acquire),
        ) {
            (true, _) if self.mail_refresh_failed.load(Acquire) => Coverage::Partial(Behind::Stale),
            (true, _) => Coverage::Loaded,
            (false, true) => Coverage::Partial(Behind::Unscanned),
            (false, false) => Coverage::Unread,
        }
    }

    /// The mail half's question, asked of the memory half — with one more way
    /// in. Mail is behind when the board was never read; memory is behind for
    /// that reason **and** when a doc's refresh after a committed write could
    /// not run, which is a state a full scan at boot never reaches.
    ///
    /// **The scan is answered for first, because it is the wider claim.** When
    /// the boot read never ran, the index holds only what this process has
    /// written, and a doc marked behind on top of that is a detail inside a far
    /// bigger absence. Reporting the detail there described an index that is
    /// nearly complete when almost nothing was in it.
    ///
    /// Two ways in reach [`Stale`](Behind::Stale) and they are one claim: a doc
    /// whose re-read after a write could not run, and a whole-corpus refresh
    /// that could not reach the store. Both say the index holds an older version
    /// than the store does, which is the only thing a caller can act on.
    pub fn memory_coverage(&self) -> Coverage {
        use std::sync::atomic::Ordering::Acquire;
        let behind_now = self.memory_refresh_failed.load(Acquire)
            || !self.behind.read().expect("behind poisoned").is_empty();
        match (
            self.memory_loaded.load(Acquire),
            self.memory_touched.load(Acquire),
        ) {
            (false, true) => Coverage::Partial(Behind::Unscanned),
            (false, false) => Coverage::Unread,
            (true, _) if behind_now => Coverage::Partial(Behind::Stale),
            (true, _) => Coverage::Loaded,
        }
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        query.validate()?;
        let depth = candidate_depth(query.limit);

        let mut scored = self.collect(self.fact_clauses(query), depth)?;
        if !query.is_fact_scoped() {
            scored.extend(self.collect(self.other_clauses(query), depth)?);
        }

        // Recency is measured against the newest fact in the candidate set, not a
        // clock: the domain stays clock-free, and the same corpus ranks the same
        // way tomorrow.
        let newest = scored
            .iter()
            .filter_map(|(_, p)| match p {
                Payload::Fact { fact } => Some(fact.date),
                _ => None,
            })
            .max();

        let terms = query.terms().map(|t| self.terms_of(t)).unwrap_or_default();
        // One read guard for the whole answer: every hit in a list resolves
        // against the same mirror, so two hits can never disagree about a name.
        let mirror = self.docs.read().expect("doc mirror poisoned");
        let mut ranked: Vec<(f32, String, Hit)> = scored
            .into_iter()
            .map(|(score, payload)| {
                let boost = match (&payload, newest) {
                    (Payload::Fact { fact }, Some(newest)) => {
                        let age_days = (newest - fact.date).get_days().max(0) as f32;
                        RECENCY_WEIGHT / (1.0 + age_days / 365.25)
                    }
                    _ => 0.0,
                };
                let hit = payload.into_hit(&terms, &mirror);
                (score + boost, tiebreak(&hit), hit)
            })
            .collect();
        // Deterministic to the last position: score, then a stable key. Two
        // sessions asking the same question see the same list in the same order.
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        let mut hits = self.pinned(query, &mirror);
        for (_, _, hit) in ranked {
            if !hits.contains(&hit) {
                hits.push(hit);
            }
        }
        hits.truncate(query.limit);
        Ok(hits)
    }
}

impl Payload {
    /// Turn a stored payload into a hit, **resolved against the mirror**. The
    /// payload holds handles because that is what the doc it came from holds;
    /// the neighborhood is assembled here, at read time, so a rename shows up in
    /// every hit and not only in the docs re-indexed since.
    fn into_hit(self, terms: &[String], mirror: &[DocMirror]) -> Hit {
        match self {
            Payload::Entity { entity, doc_id } => Hit::Entity {
                edges: edges_of(mirror, &entity.id),
                entity,
                doc_id,
            },
            Payload::Fact { fact } => Hit::Fact {
                subject: resolve(mirror, &fact.subject),
                home: resolve(mirror, &fact.home),
                fact,
            },
            Payload::Prose {
                doc_id,
                title,
                entity,
                body,
            } => {
                let owner = entity.and_then(|id| {
                    mirror
                        .iter()
                        .filter_map(|d| d.entity.as_ref())
                        .find(|e| e.id == id)
                        .cloned()
                });
                Hit::Prose {
                    edges: owner
                        .as_ref()
                        .map_or_else(Vec::new, |e| edges_of(mirror, &e.id)),
                    entity: owner,
                    doc_id,
                    title,
                    snippet: snippet(&body, terms),
                }
            }
            // Nothing to resolve: a message's surroundings are its own envelope,
            // which it already carries. Mail draws no edges and names no
            // entities — the contexts stay apart everywhere but in this list.
            Payload::Message { message } => Hit::Message {
                snippet: snippet(&message.body, terms),
                message,
            },
        }
    }
}

/// The stable secondary sort key for a hit — its own address, so ordering never
/// depends on which segment tantivy happened to return first.
fn tiebreak(hit: &Hit) -> String {
    match hit {
        Hit::Entity { entity, .. } => entity.id.to_string(),
        Hit::Fact { fact, .. } => fact.address().to_string(),
        Hit::Prose { doc_id, .. } => doc_id.clone(),
        Hit::Message { message, .. } => format!("{}/{}", message.mailbox, message.id),
    }
}

/// How much prose rides around a match.
const SNIPPET_RADIUS: usize = 120;

/// The matching text with enough around it to read. Hand-rolled rather than
/// tantivy's snippet generator: the window is around the **first matching term**,
/// which is what a reader wants, and with no query it is simply the opening of
/// the doc.
fn snippet(body: &str, terms: &[String]) -> String {
    let lower = body.to_lowercase();
    let at = terms
        .iter()
        .filter_map(|t| lower.find(t.as_str()))
        .min()
        .unwrap_or(0);

    let start = floor_boundary(body, at.saturating_sub(SNIPPET_RADIUS));
    let end = ceil_boundary(body, (at + SNIPPET_RADIUS).min(body.len()));
    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.push_str(body[start..end].trim());
    if end < body.len() {
        out.push('…');
    }
    out
}

/// Round `at` down to a char boundary — a snippet must never split a multi-byte
/// character, and prose is full of them.
fn floor_boundary(s: &str, mut at: usize) -> usize {
    while at > 0 && !s.is_char_boundary(at) {
        at -= 1;
    }
    at
}

/// Round `at` up to a char boundary.
fn ceil_boundary(s: &str, mut at: usize) -> usize {
    while at < s.len() && !s.is_char_boundary(at) {
        at += 1;
    }
    at
}

fn payload_json(payload: &Payload) -> Result<String, MemoryError> {
    serde_json::to_string(payload).map_err(|e| MemoryError::Store(format!("indexing payload: {e}")))
}

fn store_err(e: impl std::fmt::Display) -> MemoryError {
    MemoryError::Store(format!("search index: {e}"))
}

/// Say out loud that a doc holds rows whose subject names no entity — the
/// split-brain tell a hand edit leaves behind.
///
/// **Never a failure, never a drop.** The rows stay indexed and reachable
/// through their home; the only thing wrong with them before was that nobody
/// could tell. Surfacing the quarantine to the caller is later work — being able
/// to see it at all is the floor, and a scan that quietly normalizes a
/// corruption is how the corruption becomes permanent.
fn report_orphans(doc: &DocScan, known: &std::collections::HashSet<EntityId>) {
    let orphans = search::orphan_subjects(doc, known);
    if orphans.is_empty() {
        return;
    }
    let subjects: Vec<&str> = orphans.iter().map(EntityId::as_str).collect();
    tracing::warn!(
        doc = %doc.doc_id,
        entity = %doc.entity.as_ref().map_or("-", |e| e.id.as_str()),
        count = orphans.len(),
        subjects = ?subjects,
        "fact rows name a subject that is no known entity; reachable through their home doc, \
         not dropped — a hand-edited subject cell is the usual cause"
    );
}

/// Say out loud that a doc's declared id and its own rows' subjects disagree —
/// the consistency check [`report_orphans`] cannot make, because these subjects
/// name entities that **exist**.
///
/// This is the shape the split brain actually arrived in: a retyped subject cell
/// landing on another live handle, so nothing was orphaned, every read went on
/// working, and the entity ended up readable under one id and writable under the
/// other. It is also, routinely, nothing at all — a fact about one entity written
/// on another's page is ordinary. Hence a count and a line, never a verdict.
fn report_foreign_subjects(doc: &DocScan, known: &std::collections::HashSet<EntityId>) {
    let foreign = search::foreign_subjects(doc, known);
    if foreign.is_empty() {
        return;
    }
    let subjects: Vec<&str> = foreign.iter().map(EntityId::as_str).collect();
    tracing::warn!(
        doc = %doc.doc_id,
        entity = %doc.entity.as_ref().map_or("-", |e| e.id.as_str()),
        count = foreign.len(),
        subjects = ?subjects,
        "fact rows in this doc are about a different entity that exists; often legitimate, but \
         a doc whose declared id disagrees with its own rows is how one entity becomes readable \
         under one handle and writable under another"
    );
}

/// Everything a scan of one doc has to say about its own consistency. One call
/// site for both counters, so a new scan path cannot pick up one and forget the
/// other.
fn report_consistency(doc: &DocScan, known: &std::collections::HashSet<EntityId>) {
    report_orphans(doc, known);
    report_foreign_subjects(doc, known);
}

/// A [`Memory`] with a live search projection behind it.
///
/// Every verb delegates to the store, and every **successful** write re-scans the
/// document it touched and re-indexes it. That is what makes read-back cover
/// search too: writing a fact that search can't find is the same class of failure
/// as writing one `recall` can't return.
pub struct IndexedMemory {
    inner: Arc<dyn Memory>,
    index: Arc<FullTextIndex>,
}

impl IndexedMemory {
    /// Wrap a store with an **empty** index. The index is filled by
    /// [`rebuild`](IndexedMemory::rebuild) — separately, so a store that isn't
    /// reachable yet can't stop the server from booting.
    pub fn new(inner: Arc<dyn Memory>) -> Result<Self, MemoryError> {
        Ok(IndexedMemory {
            inner,
            index: Arc::new(FullTextIndex::open()?),
        })
    }

    /// Rebuild the whole index from a full re-scan of the store — the boot path.
    /// Returns how many documents were indexed.
    ///
    /// **The boot path, and only it, reports what the scan saw.** The
    /// consistency report names rows a hand edit left pointing at nothing; it is
    /// a diagnostic about the corpus, so it is worth one reading of the corpus
    /// and not one per answer. Every read re-scans now, and repeating the report
    /// on each would bury the reading that is worth having.
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        let scan = self.rescan().await?;
        let known = search::known_entities(&scan);
        for doc in &scan {
            report_consistency(doc, &known);
        }
        Ok(scan.len())
    }

    /// Replace the projection from a full scan of the store, and hand back what
    /// the scan saw. The one refresh — the boot path and the read path run the
    /// same one, so there is no second way for the index to be filled that could
    /// drift from this.
    async fn rescan(&self) -> Result<Vec<DocScan>, MemoryError> {
        let scan = self.inner.scan().await?;
        self.index.ingest_all(&scan)?;
        Ok(scan)
    }

    /// The index, for handing to whatever serves the `search` verb.
    pub fn index(&self) -> Arc<FullTextIndex> {
        self.index.clone()
    }

    /// Re-index one entity's doc by **re-reading it from the store**. A doc that
    /// has vanished is dropped from the index rather than left as a ghost — by
    /// the id its postings were stored under, which is the store's, not the
    /// handle (see [`FullTextIndex::forget`]).
    async fn reindex(&self, entity: &EntityId) -> Result<(), MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(scan) => {
                self.index.ingest_doc(&scan)?;
                report_consistency(&scan, &self.index.known_entities());
                Ok(())
            }
            None => self.index.forget(entity),
        }
    }

    /// Refresh the projection for a document a **committed** write just changed.
    ///
    /// **A failure here is not the write failing.** The store took the write and
    /// read it back; what could not be done is re-reading the page to refresh a
    /// projection of it. Failing the verb for that told the caller nothing was
    /// written and to try again, and a caller who obeys records the same thing
    /// twice.
    ///
    /// So the write is reported as what it is, and the index records that it is
    /// holding an older version of this document than the store — which is what
    /// makes `search` say so on every answer afterwards. Swallowing the failure
    /// without that mark would trade a wrong answer to one caller for a wrong
    /// answer to every later one.
    async fn refresh(&self, entity: &EntityId) {
        match self.reindex(entity).await {
            Ok(()) => self.index.current(entity),
            Err(e) => {
                self.index.behind(entity);
                tracing::warn!(
                    %entity,
                    error = %e,
                    "SEARCH INDEX BEHIND — this document was written and the index could not \
                     re-read it, so `search` serves the version before that write and reports \
                     itself partial until a write to this document succeeds or the server \
                     restarts. The write itself landed; the memory verbs are unaffected."
                );
            }
        }
    }
}

#[async_trait]
impl Memory for IndexedMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let written = self.inner.add_entity(new).await?;
        if let Guarded::Written(entity) = &written {
            self.refresh(&entity.id).await;
        }
        Ok(written)
    }

    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }

    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let written = self.inner.update_entity(handle, patch).await?;
        if let Guarded::Written(entity) = &written {
            self.refresh(&entity.id).await;
        }
        Ok(written)
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.capture(fact).await?;
        if let Guarded::Written(fact) = &written {
            self.refresh(&fact.home).await;
        }
        Ok(written)
    }

    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }

    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.update_fact(address, patch).await?;
        if let Guarded::Written(fact) = &written {
            self.refresh(&fact.home).await;
        }
        Ok(written)
    }

    /// One reindex, not two: a retraction writes both rows into the same
    /// document, so re-reading that document once is what makes the marked row
    /// and the account of it visible together. Re-reading twice would only
    /// re-read the same page.
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let taken_back = self.inner.retract(address, reason, date).await?;
        self.refresh(&taken_back.retracted.home).await;
        Ok(taken_back)
    }

    /// Prose is indexed material, so a charter written here is findable on the
    /// next call — the same "reindex the doc the store just wrote" step every
    /// other write takes, and for the same reason: without it, the one part of
    /// a bot that is pure prose would be the one part search could not see.
    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        let stored = self.inner.set_prose(entity, prose).await?;
        self.refresh(entity).await;
        Ok(stored)
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.inner.scan_entity(entity).await
    }
}

/// One half of the corpus behind `search`, able to bring itself level with the
/// store it mirrors.
///
/// **This is what makes one mechanism serve two stores.** Memory and mail are
/// two halves of one index fed by two stores, and the promise the [`Search`]
/// port makes — an answer backed by a scan taken for it — is the same promise
/// on both. A half implements it for its own store; nothing above has to know
/// which stores exist, so a third one later is a third implementor rather than
/// an edit here.
#[async_trait]
pub trait Refresh: Send + Sync {
    /// Take a scan and bring this half of the index to what it says.
    ///
    /// **Infallible on purpose.** A refresh that cannot reach its store does
    /// not fail the search: it records that this half is behind, and the last
    /// scan that succeeded goes on answering. Degrading is the promise; the
    /// coverage is where it is admitted.
    async fn refresh(&self);
}

/// The [`Search`] port: the index, and every half that can refresh itself.
///
/// **The port lives here rather than on either decorator**, because an answer
/// spans both halves and neither decorator can reach the other's store. Putting
/// it on the Memory decorator and handing that a mailbox store would be the
/// wrong object holding a second store to make a port reachable; putting the
/// refresh in the caller would move the port's own promise out of the port.
pub struct Retrieval {
    index: Arc<FullTextIndex>,
    halves: Vec<Arc<dyn Refresh>>,
}

impl Retrieval {
    /// The port over an index, refreshed by each half before it answers.
    pub fn new(index: Arc<FullTextIndex>, halves: Vec<Arc<dyn Refresh>>) -> Self {
        Retrieval { index, halves }
    }
}

#[async_trait]
impl Search for Retrieval {
    /// **Refresh every half, then answer.** Each half reaches its own store, so
    /// a record removed outside jojobot — which rule 60 makes the only way one
    /// leaves at all — is noticed on the read path, the only place left that
    /// can notice it.
    async fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        for half in &self.halves {
            half.refresh().await;
        }
        self.index.search(query)
    }

    fn mail_coverage(&self) -> Coverage {
        self.index.mail_coverage()
    }

    fn memory_coverage(&self) -> Coverage {
        self.index.memory_coverage()
    }
}

#[async_trait]
impl Refresh for IndexedMemory {
    async fn refresh(&self) {
        if self.rescan().await.is_err() {
            self.index.refresh_failed();
        }
    }
}

/// **Search one store through the real port**, for tests that hold only the
/// Memory decorator.
///
/// The decorator is not the port and must not become one: a search spans both
/// halves of the index, and a port over the memory half alone would answer
/// without ever refreshing mail. So a test that wants to search builds the
/// [`Retrieval`] a caller builds, rather than calling a method the production
/// path does not have.
#[cfg(test)]
#[async_trait]
pub(crate) trait SearchViaPort {
    async fn search_via_port(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError>;
}

#[cfg(test)]
#[async_trait]
impl SearchViaPort for Arc<IndexedMemory> {
    async fn search_via_port(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        Retrieval::new(self.index(), vec![self.clone()])
            .search(query)
            .await
    }
}

/// A [`Mailboxes`] with the search projection behind it — the mail half's
/// [`IndexedMemory`], and it exists for the same reason.
///
/// Every verb delegates to the store, and every one that **changes** a message
/// re-indexes it: posting makes it findable on the next call rather than after a
/// restart, and a delivery or a retirement keeps the `state` on a search hit
/// honest. A hit that says `new` for a message somebody drained an hour ago is
/// worse than no hit, because a reader acts on it.
///
/// **The limit of that, stated rather than left to be discovered.** This is a
/// boot-loaded projection updated by the verbs that pass through it, so a
/// message that changes any other way keeps its indexed state until some verb
/// touches it again: a person moving a card on the board by hand, and — inside
/// jojobot — a message a delivery deliberately excluded, which by definition is
/// one somebody else moved under it. Neither is a live view, and neither can be
/// made one without polling the board. The state on a hit is what jojobot last
/// saw, which is why a hit carries the id: `read_message` reads the store.
pub struct IndexedMailboxes {
    inner: Arc<dyn Mailboxes>,
    index: Arc<FullTextIndex>,
}

impl IndexedMailboxes {
    /// Wrap a store, writing into the index the Memory half already uses. **One
    /// index, not two** — that is what makes one ranked list possible at all.
    pub fn new(inner: Arc<dyn Mailboxes>, index: Arc<FullTextIndex>) -> Self {
        IndexedMailboxes { inner, index }
    }

    /// Load the mail half from a full board read — the boot path. Returns how
    /// many messages were indexed.
    pub async fn rebuild(&self) -> Result<usize, MailboxError> {
        let messages = self.inner.scan_messages().await?;
        self.index.ingest_mail(&messages).map_err(indexing)?;
        Ok(messages.len())
    }

    fn reindex(&self, message: &Message) -> Result<(), MailboxError> {
        self.index.ingest_message(message).map_err(indexing)
    }
}

#[async_trait]
impl Refresh for IndexedMailboxes {
    /// **`scan_messages`, never a delivering read.** Reading IS delivery on the
    /// verbs that deliver, so a refresh built on one would drain a box as a side
    /// effect of answering a question. This board read takes nothing and moves
    /// nothing.
    async fn refresh(&self) {
        match self.inner.scan_messages().await {
            Ok(messages) => {
                let _ = self.index.ingest_mail_changes(&messages);
            }
            Err(_) => self.index.mail_refresh_failed(),
        }
    }
}

/// An index failure, in the mailbox context's vocabulary. The seam between the
/// two contexts is exactly here and nowhere else.
fn indexing(e: MemoryError) -> MailboxError {
    MailboxError::Store(format!("search index: {e}"))
}

#[async_trait]
impl Mailboxes for IndexedMailboxes {
    async fn create_mailbox(
        &self,
        name: &jojobot_domain::mailbox::MailboxName,
        owner: &jojobot_domain::memory::EntityId,
        override_token: Option<&str>,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Mailbox>, MailboxError>
    {
        // A box holds no text of its own — nothing to index until a message
        // lands in it.
        self.inner.create_mailbox(name, owner, override_token).await
    }

    async fn list_mailboxes(&self) -> Result<Vec<jojobot_domain::mailbox::Mailbox>, MailboxError> {
        self.inner.list_mailboxes().await
    }

    async fn scan_messages(&self) -> Result<Vec<Message>, MailboxError> {
        self.inner.scan_messages().await
    }

    async fn post_message(
        &self,
        message: jojobot_domain::mailbox::NewMessage,
    ) -> Result<jojobot_domain::mailbox::Guarded<Message>, MailboxError> {
        let written = self.inner.post_message(message).await?;
        if let jojobot_domain::mailbox::Guarded::Written(message) = &written {
            self.reindex(message)?;
        }
        Ok(written)
    }

    async fn read_mailbox(
        &self,
        name: &jojobot_domain::mailbox::MailboxName,
    ) -> Result<jojobot_domain::mailbox::Guarded<jojobot_domain::mailbox::Delivery>, MailboxError>
    {
        let delivered = self.inner.read_mailbox(name).await?;
        if let jojobot_domain::mailbox::Guarded::Written(delivery) = &delivered {
            for message in &delivery.messages {
                self.reindex(&message.message)?;
            }
        }
        Ok(delivered)
    }

    async fn read_message(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
    ) -> Result<jojobot_domain::mailbox::Delivered, MailboxError> {
        let delivered = self.inner.read_message(id).await?;
        self.reindex(&delivered.message)?;
        Ok(delivered)
    }

    async fn mark_processed(
        &self,
        id: &jojobot_domain::mailbox::MessageId,
        notes: Option<&str>,
    ) -> Result<Message, MailboxError> {
        let processed = self.inner.mark_processed(id, notes).await?;
        self.reindex(&processed)?;
        Ok(processed)
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use jojobot_domain::mailbox::testing::{InMemoryMailboxes, contract as mail_contract};
    use jojobot_domain::mailbox::{MailboxName, Message, MessageId, MessageState};
    use jojobot_domain::memory::event::Event;
    use jojobot_domain::memory::search::{DEFAULT_LIMIT, EdgeFilter, EntityRef};
    use jojobot_domain::memory::testing::{InMemoryMemory, contract};
    use jojobot_domain::memory::{
        Boot, Edge, EdgeShape, FactStatus, Provenance, Standing, validate_subject,
    };

    use super::*;

    /// The whole contract — Memory *and* retrieval — against the in-memory fake
    /// behind the index. The fast loop.
    #[tokio::test]
    async fn the_contract_holds_over_the_fake() {
        let store =
            Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens"));
        contract::run_all_searchable(
            store.as_ref(),
            &Retrieval::new(store.index(), vec![store.clone()]),
        )
        .await;
    }

    // --- the index as a projection --------------------------------------------

    /// A doc built by hand, so the index's behaviour can be examined without a
    /// store underneath it.
    fn scan(doc_id: &str, entity: Option<Entity>, prose: &str, facts: Vec<Fact>) -> DocScan {
        DocScan {
            doc_id: doc_id.into(),
            title: entity.as_ref().map(|e| e.name.clone()).unwrap_or_default(),
            prose: prose.into(),
            entity,
            facts,
        }
    }

    fn entity(id: &str, name: &str) -> Entity {
        let id = EntityId(id.into());
        assert!(validate_subject(&id).is_ok(), "test ids are well-formed");
        Entity {
            kind: id.kind().expect("a well-formed id has a kind"),
            id,
            name: name.into(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        }
    }

    fn fact(home: &str, id: &str, content: &str, on: jiff::civil::Date) -> Fact {
        Fact {
            id: jojobot_domain::memory::FactId(id.into()),
            home: EntityId(home.into()),
            subject: EntityId(home.into()),
            content: content.into(),
            details: None,
            provenance: Provenance::Inference,
            standing: Standing::Open,
            status: FactStatus::Active,
            date: on,
            edge: None,
            event: None,
            derived_from: None,
        }
    }

    /// A free-text query that asks for mail as well. **Mail is opt-in** — the
    /// bare query leaves it out — and the tests below are about what the mail
    /// half of the index holds, so they ask for it rather than relying on a
    /// default that would hide the flag from every one of them.
    fn asking_for_mail(text: &str) -> SearchQuery {
        SearchQuery {
            include_mail: true,
            ..SearchQuery::text(text)
        }
    }

    fn index_of(scans: Vec<DocScan>) -> FullTextIndex {
        let index = FullTextIndex::open().expect("index opens");
        index.ingest_all(&scans).expect("ingest");
        index
    }

    /// **The read-side leak, closed.** A detail that lives only in a doc's prose —
    /// nobody filed it as a fact — comes back in the same ranked list as the fact
    /// and entity hits. Without this, "it's in the doc" means "it is gone".
    #[tokio::test]
    async fn a_match_only_in_prose_comes_back_beside_the_other_hits() {
        let alpha = entity("person:alpha", "Alpha");
        let index = index_of(vec![scan(
            "doc-1",
            Some(alpha.clone()),
            "Alpha is allergic to penicillin, which came up once and never got filed.",
            vec![fact(
                "person:alpha",
                "f1",
                "plays go on Tuesdays",
                date(2026, 7, 1),
            )],
        )]);

        let hits = index
            .search(&SearchQuery::text("penicillin"))
            .expect("search ok");
        let prose: Vec<&Hit> = hits
            .iter()
            .filter(|h| matches!(h, Hit::Prose { .. }))
            .collect();
        assert_eq!(prose.len(), 1, "the prose match must be a hit: {hits:?}");
        let Some(Hit::Prose {
            doc_id,
            entity: owner,
            snippet,
            ..
        }) = prose.first().copied()
        else {
            unreachable!("filtered to prose");
        };
        assert_eq!(doc_id, "doc-1", "a prose hit says which doc to open");
        assert_eq!(
            owner.as_ref().map(|e| &e.id),
            Some(&alpha.id),
            "…and whose entity doc it is"
        );
        assert!(
            snippet.to_lowercase().contains("penicillin"),
            "the snippet must carry the match: {snippet:?}"
        );

        // …and the same query, in one list, still reaches the fact and the entity.
        let mixed = index
            .search(&SearchQuery::text("alpha"))
            .expect("search ok");
        assert!(
            mixed.iter().any(|h| matches!(h, Hit::Entity { .. })),
            "{mixed:?}"
        );
        assert!(
            mixed.iter().any(|h| matches!(h, Hit::Prose { .. })),
            "{mixed:?}"
        );
    }

    /// Prose in a doc that is nobody's entity is still searchable — a page the
    /// user wrote by hand is exactly the page worth finding.
    #[tokio::test]
    async fn prose_in_a_doc_that_is_no_entity_is_still_found() {
        let index = index_of(vec![scan(
            "doc-loose",
            None,
            "Notes from the trip: the pass was closed on Tuesday.",
            Vec::new(),
        )]);
        let hits = index
            .search(&SearchQuery::text("pass closed"))
            .expect("search ok");
        assert!(
            matches!(hits.first(), Some(Hit::Prose { entity: None, doc_id, .. }) if doc_id == "doc-loose"),
            "{hits:?}"
        );
    }

    /// Recency breaks a tie with teeth: two facts of equal text relevance come
    /// back newest first.
    #[tokio::test]
    async fn equal_relevance_ranks_the_newer_fact_first() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![
                fact("person:alpha", "f1", "winter kayak trip", date(2024, 1, 1)),
                fact("person:alpha", "f2", "kayak winter trip", date(2026, 1, 1)),
            ],
        )]);
        let hits = index
            .search(&SearchQuery::text("kayak trip"))
            .expect("search ok");
        let ids: Vec<String> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { fact, .. } => Some(fact.id.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(
            ids,
            vec!["f2", "f1"],
            "same words, same length — the newer one leads"
        );
    }

    /// Every term has to match. A search that quietly ORs its words turns a
    /// precise question into a vague answer.
    #[tokio::test]
    async fn all_query_terms_must_match() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![
                fact(
                    "person:alpha",
                    "f1",
                    "bakes sourdough bread",
                    date(2026, 1, 1),
                ),
                fact("person:alpha", "f2", "bakes almond cake", date(2026, 1, 2)),
            ],
        )]);
        let hits = index
            .search(&SearchQuery::text("bakes sourdough"))
            .expect("search ok");
        let contents: Vec<String> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { fact, .. } => Some(fact.content.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(contents, vec!["bakes sourdough bread"], "got {hits:?}");
    }

    /// **Naming an entity outranks matching its text.** A fact that repeats the
    /// query's words scores higher on relevance alone; asking for `org:guild` is
    /// asking about the guild, so the guild leads. The pin is decided by the write
    /// guard's matcher, so "close enough to be the same thing" has one definition
    /// in this system rather than two.
    #[tokio::test]
    async fn naming_an_entity_pins_it_above_a_more_relevant_fact() {
        // A long name spreads the entity's relevance thin; a terse fact about it
        // concentrates the same words — so BM25 alone ranks the fact first.
        let guild = entity(
            "org:guild",
            "Guild of the Northern Riverside Makers and Menders",
        );
        let index = index_of(vec![scan(
            "doc-1",
            Some(guild.clone()),
            "",
            vec![
                fact("org:guild", "f1", "guild", date(2026, 1, 1)),
                fact("org:guild", "f2", "guild night", date(2026, 1, 2)),
            ],
        )]);

        let hits = index
            .search(&SearchQuery::text("org:guild"))
            .expect("search ok");
        assert!(
            matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == guild.id),
            "the named entity must lead, whatever the facts score: {hits:?}"
        );
        assert!(
            hits.iter().any(|h| matches!(h, Hit::Fact { .. })),
            "…and the facts are still in the same list: {hits:?}"
        );
    }

    /// **A name the user actually says finds the thing.** An entity known as
    /// Homer Simpson and called Cosme Fulanito has to answer to "Cosme Fulanito" — the entity itself, and
    /// the facts on its page, which is what the question was really about.
    ///
    /// A fact carries the labels of the entity **whose page it sits on**. That
    /// is what the doc knows at ingest; a fact about X written on Y's page keeps
    /// X's handle and not X's nickname, because resolving that would mean a
    /// global pass on every write.
    #[tokio::test]
    async fn a_query_on_an_alias_finds_the_entity_and_the_facts_on_its_page() {
        let homer = Entity {
            aliases: vec!["Cosme Fulanito".into()],
            ..entity("person:homer", "Homer Simpson")
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(homer.clone()),
            "",
            vec![fact(
                "person:homer",
                "f1",
                "plays the bass",
                date(2026, 1, 1),
            )],
        )]);

        let hits = index
            .search(&SearchQuery::text("Cosme Fulanito"))
            .expect("search ok");
        assert!(
            matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == homer.id),
            "the entity that wears the nickname leads: {hits:?}"
        );
        assert!(
            hits.iter()
                .any(|h| matches!(h, Hit::Fact { fact, .. } if fact.content == "plays the bass")),
            "…and the facts on its page come with it: {hits:?}"
        );

        // The display name still works, and the two are not different questions.
        // The handle deliberately does NOT spell the display name out: one that
        // did would put every token of the name into the fact's text through the
        // subject alone, and this assertion would then hold with the labels
        // stripped out entirely — passing while proving nothing.
        assert!(
            index
                .search(&SearchQuery::text("Homer Simpson"))
                .expect("search ok")
                .iter()
                .any(|h| matches!(h, Hit::Fact { .. })),
            "a label is a label, preferred or not"
        );

        // The alias has to be in the POSTINGS, not only in the pin. Pinning
        // fires on a query that names the entity outright ("Cosme Fulanito"); a query
        // that merely contains the nickname among other words can only be
        // answered by the index, and that is the common case.
        assert!(
            index
                .search(&SearchQuery::text("Cosme Fulanito person"))
                .expect("search ok")
                .iter()
                .any(|h| matches!(h, Hit::Entity { entity, .. } if entity.id == homer.id)),
            "the entity record itself is indexed under every name it answers to"
        );
    }

    /// **The two halves a fact hit has to keep apart.** A row about Beta written
    /// on Alpha's page names both, resolved — that difference is precisely what a
    /// reader has to be able to see, and it is invisible if either side comes
    /// back as a bare handle.
    ///
    /// And a subject naming nothing comes back with **no name rather than an
    /// invented one**. Filling it with the handle would make the orphan look
    /// exactly like a resolved hit, which is how the split brain stayed
    /// undetected for a milestone.
    #[tokio::test]
    async fn a_fact_hit_resolves_a_home_and_a_subject_that_differ() {
        let alpha = entity("person:alpha", "Alpha");
        let beta = entity("person:beta", "Beta");
        let guest = Fact {
            subject: beta.id.clone(),
            ..fact(
                "person:alpha",
                "f1",
                "brought the sourdough",
                date(2026, 1, 1),
            )
        };
        let orphan = Fact {
            subject: EntityId("person:ghost".into()),
            ..fact(
                "person:alpha",
                "f2",
                "brought the sourdough too",
                date(2026, 1, 2),
            )
        };
        let index = index_of(vec![
            scan("doc-1", Some(alpha.clone()), "", vec![guest, orphan]),
            scan("doc-2", Some(beta.clone()), "", vec![]),
        ]);

        let hits = index
            .search(&SearchQuery::text("sourdough"))
            .expect("search ok");
        let refs: Vec<(&EntityRef, &EntityRef)> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { subject, home, .. } => Some((subject, home)),
                _ => None,
            })
            .collect();

        let resolved = refs
            .iter()
            .find(|(s, _)| s.id == beta.id)
            .expect("the row about beta must come back");
        assert_eq!(resolved.0.name.as_deref(), Some("Beta"), "who it is about");
        assert_eq!(resolved.1.id, alpha.id, "…and whose page it sits on");
        assert_eq!(resolved.1.name.as_deref(), Some("Alpha"));

        let ghost = refs
            .iter()
            .find(|(s, _)| s.id.as_str() == "person:ghost")
            .expect("the orphaned row is indexed, not dropped");
        assert_eq!(
            ghost.0.kind,
            Some(EntityKind::Person),
            "the handle still declares a kind"
        );
        assert_eq!(
            ghost.0.name, None,
            "a subject that names nothing must read as unresolved, not as itself"
        );
        assert!(
            ghost.0.aliases.is_empty(),
            "…and it answers to nothing either: an unresolvable handle reports no \
             names rather than inventing one from its own slug"
        );
        assert_eq!(
            ghost.1.name.as_deref(),
            Some("Alpha"),
            "its home still resolves"
        );
    }

    /// An entity's edges are the ones its **facts** draw, wherever those rows are
    /// homed — and only its own. A row about someone else, sitting on this page,
    /// belongs to that someone else's neighborhood.
    #[tokio::test]
    async fn entity_hits_carry_the_edges_of_their_own_facts_only() {
        let alpha = entity("person:alpha", "Alpha");
        let beta = entity("person:beta", "Beta");
        let shelbyville = Edge::new(EdgeShape::Location, EntityId("place:shelbyville".into()));
        let guild = Edge::new(EdgeShape::Membership, EntityId("org:guild".into()));
        let index = index_of(vec![
            scan(
                "doc-1",
                Some(alpha.clone()),
                "",
                vec![
                    Fact {
                        edge: Some(shelbyville.clone()),
                        event: None,
                        ..fact("person:alpha", "f1", "wintering", date(2026, 1, 1))
                    },
                    // Beta's row, homed on Alpha's page: Beta's edge, not Alpha's.
                    Fact {
                        subject: beta.id.clone(),
                        edge: Some(guild.clone()),
                        event: None,
                        ..fact("person:alpha", "f2", "joined up", date(2026, 1, 2))
                    },
                ],
            ),
            scan("doc-2", Some(beta.clone()), "", vec![]),
        ]);

        let edges_for = |handle: &EntityId| -> Vec<Edge> {
            index
                .search(&SearchQuery::text(handle.as_str()))
                .expect("search ok")
                .iter()
                .find_map(|h| match h {
                    Hit::Entity { entity, edges, .. } if &entity.id == handle => {
                        Some(edges.clone())
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("{handle} must come back as an entity hit"))
        };

        assert_eq!(
            edges_for(&alpha.id),
            vec![shelbyville],
            "its own claim's edge"
        );
        assert_eq!(
            edges_for(&beta.id),
            vec![guild],
            "an edge follows the claim's SUBJECT, not the page the row happens to sit on"
        );
    }

    /// A prose hit carries its doc's entity's neighborhood too. Prose is where
    /// this is easiest to lose: the stored payload holds a bare handle, so the
    /// entity and its edges are assembled on the way out or not at all.
    #[tokio::test]
    async fn prose_hits_carry_the_edges_of_their_docs_entity() {
        let neighbor = entity("person:ned-flanders", "Ned Flanders");
        let shop = Edge::new(EdgeShape::Location, EntityId("place:leftorium".into()));
        let index = index_of(vec![scan(
            "doc-prose-edge",
            Some(neighbor.clone()),
            "Keeps a spare key under the third flowerpot; it came up once and never got filed.",
            vec![Fact {
                edge: Some(shop.clone()),
                event: None,
                ..fact(
                    "person:ned-flanders",
                    "f1",
                    "opens on the first Sunday",
                    date(2026, 1, 1),
                )
            }],
        )]);

        let hits = index
            .search(&SearchQuery::text("flowerpot"))
            .expect("search ok");
        let Some(Hit::Prose {
            entity: owner,
            edges,
            ..
        }) = hits.iter().find(|h| matches!(h, Hit::Prose { .. }))
        else {
            panic!("the prose match must come back: {hits:?}")
        };
        assert_eq!(owner.as_ref().map(|e| &e.id), Some(&neighbor.id));
        assert_eq!(
            edges,
            &vec![shop],
            "a prose hit sits in the graph too: {edges:?}"
        );
    }

    /// **A rename changes every hit that names the entity** — not only the hits
    /// whose own doc was re-indexed since.
    ///
    /// This is the claim that justifies resolving on the way OUT rather than
    /// freezing a name into the stored payload, and it is only testable where a
    /// row lives somewhere other than its subject's page: renaming an entity
    /// re-indexes that entity's doc alone, so a fact homed elsewhere is never
    /// re-read. If the name were stored at ingest, that hit would go on
    /// answering with the old one until something unrelated touched its page.
    #[tokio::test]
    async fn a_rename_reaches_a_hit_whose_own_doc_was_never_reindexed() {
        let renamed = entity("person:milhouse", "Milhouse Van Houten");
        let guest = Fact {
            subject: renamed.id.clone(),
            ..fact(
                "person:alpha",
                "f1",
                "brought the sourdough",
                date(2026, 1, 1),
            )
        };
        let store = Arc::new(
            IndexedMemory::new(Scanned::new(vec![
                scan(
                    "doc-alpha",
                    Some(entity("person:alpha", "Alpha")),
                    "",
                    vec![guest],
                ),
                scan("doc-milhouse", Some(renamed.clone()), "", Vec::new()),
            ]))
            .expect("index opens"),
        );
        store.rebuild().await.expect("rebuild");

        // Asked for by the fact's own CONTENT: a row about someone else, sitting
        // on this page, is deliberately not indexed under their labels, so
        // querying the name would fail for an unrelated reason.
        async fn named(store: &Arc<IndexedMemory>, who: &EntityId) -> Option<String> {
            store
                .search_via_port(&SearchQuery::text("sourdough"))
                .await
                .expect("search ok")
                .iter()
                .find_map(|h| match h {
                    Hit::Fact { subject, .. } if &subject.id == who => subject.name.clone(),
                    _ => None,
                })
        }
        assert_eq!(
            named(&store, &renamed.id).await.as_deref(),
            Some("Milhouse Van Houten")
        );

        store
            .update_entity(
                &renamed.id,
                EntityPatch {
                    name: Some("Thrillhouse".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("rename ok")
            .written()
            .expect("this double does not guard");

        assert_eq!(
            named(&store, &renamed.id).await.as_deref(),
            Some("Thrillhouse"),
            "the row on the OTHER doc still names them, and must name them correctly"
        );
    }

    /// A `kind:slug` handle is an ordinary query, not query syntax. tantivy's own
    /// parser reads `person:` as a field name and errors — which would make the
    /// most natural query in this system a hard failure.
    #[tokio::test]
    async fn a_handle_shaped_query_is_not_query_syntax() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact("person:alpha", "f1", "plays go", date(2026, 1, 1))],
        )]);
        for query in ["person:alpha", "AND", "a(b", "\"unclosed", "-alpha"] {
            assert!(
                index.search(&SearchQuery::text(query)).is_ok(),
                "{query:?} must be treated as text, not syntax"
            );
        }
    }

    /// A fact-only filter narrows to facts. Asking for "superseded" and getting an
    /// entity back — entities have no lifecycle — would be noise dressed as a hit.
    #[tokio::test]
    async fn a_fact_only_filter_returns_facts_alone() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha writes about alpha things.",
            vec![fact("person:alpha", "f1", "alpha claim", date(2026, 1, 1))],
        )]);
        let hits = index
            .search(&SearchQuery {
                provenance: Some(Provenance::Inference),
                ..SearchQuery::text("alpha")
            })
            .expect("search ok");
        assert!(!hits.is_empty());
        assert!(
            hits.iter().all(|h| matches!(h, Hit::Fact { .. })),
            "a fact-only filter must not surface entities or prose: {hits:?}"
        );
    }

    /// The limit is honoured, and defaults to twenty.
    #[tokio::test]
    async fn the_limit_caps_the_list_and_defaults_to_twenty() {
        let facts: Vec<Fact> = (1..=30)
            .map(|n| {
                fact(
                    "person:alpha",
                    &format!("f{n}"),
                    "repeated claim",
                    date(2026, 1, 1),
                )
            })
            .collect();
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            facts,
        )]);
        assert_eq!(
            index
                .search(&SearchQuery::text("repeated"))
                .expect("search ok")
                .len(),
            DEFAULT_LIMIT
        );
        assert_eq!(
            index
                .search(&SearchQuery {
                    limit: 3,
                    ..SearchQuery::text("repeated")
                })
                .expect("search ok")
                .len(),
            3
        );
    }

    /// A rebuild replaces the projection wholesale: what the store no longer says
    /// is no longer findable. A projection that accumulates is a second, wrong
    /// source of truth.
    #[tokio::test]
    async fn a_rebuild_drops_what_the_store_no_longer_says() {
        let index = FullTextIndex::open().expect("index opens");
        let before = scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
        );
        index.ingest_all(&[before]).expect("ingest");
        assert_eq!(
            index
                .search(&SearchQuery::text("ferret"))
                .expect("search ok")
                .len(),
            1
        );

        let after = scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a tortoise",
                date(2026, 1, 1),
            )],
        );
        index.ingest_all(&[after]).expect("re-ingest");
        assert!(
            index
                .search(&SearchQuery::text("ferret"))
                .expect("search ok")
                .is_empty(),
            "the old row must be gone from the index, not left beside the new one"
        );
        assert_eq!(
            index
                .search(&SearchQuery::text("tortoise"))
                .expect("search ok")
                .len(),
            1
        );
    }

    /// An edited fact is re-indexed in place: one hit, saying the new thing. The
    /// incremental path re-reads the doc, so a stale copy can't survive an edit.
    #[tokio::test]
    async fn an_edit_leaves_one_indexed_copy_saying_the_new_thing() {
        let store =
            Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens"));
        store
            .add_entity(NewEntity::new(
                EntityId::person("alpha"),
                "Alpha",
                "user-named",
            ))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
        let captured = store
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "works at the old place",
                date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        store
            .update_fact(
                &captured.address(),
                FactPatch {
                    content: Some("works at the new place".into()),
                    ..Default::default()
                },
            )
            .await
            .expect("update ok")
            .written()
            .expect("not blocked");

        assert!(
            store
                .search_via_port(&SearchQuery::text("old place"))
                .await
                .expect("search ok")
                .is_empty(),
            "the superseded text must be gone from the index"
        );
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("new place"))
                .await
                .expect("search ok")
                .len(),
            1
        );
    }

    /// A blocked write indexes nothing — the guard said nothing was written, and
    /// the projection has to agree.
    #[tokio::test]
    async fn a_blocked_write_indexes_nothing() {
        let store =
            Arc::new(IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens"));
        store
            .add_entity(NewEntity::new(
                EntityId::person("zenith"),
                "Zenith",
                "user-named",
            ))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");

        let blocked = store
            .capture(NewFact::about(
                EntityId::person("zenit"),
                "should not be indexed",
                date(2026, 7, 1),
            ))
            .await
            .expect("call ok");
        assert!(matches!(blocked, Guarded::Blocked { .. }));
        assert!(
            store
                .search_via_port(&SearchQuery::text("should not be indexed"))
                .await
                .expect("search ok")
                .is_empty(),
            "a blocked capture must leave nothing in the index either"
        );
    }

    /// The boot path: a store already holding docs is indexed by one full re-scan.
    #[tokio::test]
    async fn rebuild_indexes_a_store_that_was_already_full() {
        let inner = Arc::new(InMemoryMemory::new());
        inner
            .add_entity(NewEntity::new(
                EntityId::person("alpha"),
                "Alpha",
                "user-named",
            ))
            .await
            .expect("add ok");
        inner
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "was here before the server started",
                date(2026, 7, 1),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let store = Arc::new(IndexedMemory::new(inner).expect("index opens"));
        // An index nobody has filled yet still answers from the store, because
        // the read takes its own scan. This used to assert the opposite — that
        // nothing is searchable until the boot scan runs — which was a statement
        // about the projection rather than about what the store holds.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("before"))
                .await
                .expect("search ok")
                .len(),
            1,
            "the store holds it, so a read finds it with no boot scan"
        );
        assert_eq!(
            store.rebuild().await.expect("rebuild"),
            1,
            "one doc scanned"
        );
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("before"))
                .await
                .expect("search ok")
                .len(),
            1
        );
    }

    /// **A claim taken back before this server started is not served by it.**
    ///
    /// The shared contract drives both states against a live index, so what is
    /// left to ask is the path it does not travel: a record marked in the store
    /// and read back by the BOOT SCAN, rather than by the refresh that follows a
    /// write. Taking something back is the one move whose whole purpose is that
    /// it stops being served, so the state has to survive the journey out to the
    /// store and back rather than only the write that set it.
    ///
    /// Both of the states that leave a row standing are driven here, because
    /// they are set by different verbs: `retract` marks an event, and an
    /// ordinary edit is what moves a fact to superseded.
    #[tokio::test]
    async fn a_claim_taken_back_before_the_scan_is_not_served_after_it() {
        let inner = Arc::new(InMemoryMemory::new());
        inner
            .add_entity(NewEntity::new(
                EntityId::person("alpha"),
                "Alpha",
                "user-named",
            ))
            .await
            .expect("add ok");
        let stands = inner
            .capture(NewFact {
                event: Some(jojobot_domain::memory::event::Event::of("a-rehearsal")),
                ..NewFact::about(
                    EntityId::person("alpha"),
                    "the quartet rehearsed",
                    date(2026, 7, 1),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        let taken_back = inner
            .capture(NewFact {
                event: Some(jojobot_domain::memory::event::Event::of("a-rehearsal")),
                ..NewFact::about(
                    EntityId::person("alpha"),
                    "the quartet rehearsed twice",
                    date(2026, 7, 2),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        inner
            .retract(
                &taken_back.address(),
                Some("it never happened"),
                date(2026, 7, 3),
            )
            .await
            .expect("retract ok");
        // The card's claim names both states, so both are driven: they leave a
        // row standing for different reasons and are marked by different verbs.
        let moved_past = inner
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "the quartet rehearsed on Tuesdays",
                date(2026, 7, 4),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        inner
            .update_fact(
                &moved_past.address(),
                FactPatch {
                    status: Some(FactStatus::Superseded),
                    ..Default::default()
                },
            )
            .await
            .expect("edit ok");

        let store = Arc::new(IndexedMemory::new(inner).expect("index opens"));
        store.rebuild().await.expect("rebuild");

        let seen: Vec<String> = store
            .search_via_port(&SearchQuery::text("rehearsed"))
            .await
            .expect("search ok")
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { fact, .. } => Some(fact.address().to_string()),
                _ => None,
            })
            .collect();
        // The positive first: "the retracted one is absent" passes identically
        // on a scan that indexed nothing at all.
        assert!(
            seen.contains(&stands.address().to_string()),
            "the claim that still stands is served: {seen:?}"
        );
        assert!(
            !seen.contains(&taken_back.address().to_string()),
            "the claim that was taken back is not: {seen:?}"
        );
        assert!(
            !seen.contains(&moved_past.address().to_string()),
            "and neither is the claim the record has moved past: {seen:?}"
        );
    }

    /// Two documents, one of which is about to be removed from the store
    /// behind jojobot's back. Two rather than one so that every assertion
    /// below has a survivor to pair with.
    fn a_store_of_two() -> Arc<Scanned> {
        Scanned::new(vec![
            DocScan {
                doc_id: Scanned::DOC_ID.into(),
                title: "Alpha".into(),
                prose: "Alpha is allergic to penicillin.".into(),
                entity: Some(entity("person:alpha", "Alpha")),
                facts: vec![fact(
                    "person:alpha",
                    "f1",
                    "keeps a ferret",
                    date(2026, 1, 1),
                )],
            },
            DocScan {
                doc_id: "outline-uuid-9b2c".into(),
                title: "Beta".into(),
                prose: "Beta plays the sousaphone.".into(),
                entity: Some(entity("person:beta", "Beta")),
                facts: vec![fact("person:beta", "f1", "keeps a gecko", date(2026, 1, 1))],
            },
        ])
    }

    /// **A record the store lost stops being served, with no write to prompt
    /// it.** The removal happens outside jojobot — rule 60 makes true deletion
    /// a human act — so nothing calls a refresh, and the read path is the only
    /// place left that can notice.
    ///
    /// Every assertion is a pair: the survivor is asserted present in the same
    /// read that asserts the removed one absent, because "the stale record is
    /// gone" passes identically against a search wired to nothing.
    #[tokio::test]
    async fn a_record_removed_from_the_store_stops_being_served() {
        let inner = a_store_of_two();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");
        for present in ["ferret", "penicillin", "gecko", "sousaphone"] {
            assert_eq!(
                store
                    .search_via_port(&SearchQuery::text(present))
                    .await
                    .unwrap()
                    .len(),
                1,
                "{present:?} is served while both pages exist"
            );
        }

        inner.lose(Scanned::DOC_ID);

        for gone in ["ferret", "penicillin"] {
            assert!(
                store
                    .search_via_port(&SearchQuery::text(gone))
                    .await
                    .unwrap()
                    .is_empty(),
                "{gone:?} left the store, so search must stop serving it"
            );
        }
        for kept in ["gecko", "sousaphone"] {
            assert_eq!(
                store
                    .search_via_port(&SearchQuery::text(kept))
                    .await
                    .unwrap()
                    .len(),
                1,
                "{kept:?} is still in the store and must still be served"
            );
        }
        assert!(
            store
                .search_via_port(&SearchQuery::text("person:alpha"))
                .await
                .unwrap()
                .is_empty(),
            "the entity goes with its page"
        );
        assert!(
            !store
                .search_via_port(&SearchQuery::text("person:beta"))
                .await
                .unwrap()
                .is_empty(),
            "…and the entity whose page remains does not"
        );
    }

    /// **The answer never claims to be complete while it is coming from a scan
    /// that could not be taken.** Rule 130: a wrong answer is a defect, and a
    /// wrong answer that vouches for itself is worse, because the caller has
    /// nothing to act on.
    ///
    /// The pair here is hits-and-coverage in one read. An implementation that
    /// answered with nothing and reported itself partial would satisfy the
    /// coverage assertion alone, and it would be a worse store than the one
    /// this fixes.
    #[tokio::test]
    async fn coverage_stops_claiming_loaded_when_the_read_cannot_reach_the_store() {
        let inner = a_store_of_two();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Loaded,
            "a scan that just succeeded is complete coverage"
        );

        inner.blinded();

        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ferret"))
                .await
                .unwrap()
                .len(),
            1,
            "degrade, don't error: the last good scan still answers"
        );
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Partial(Behind::Stale),
            "…and the answer says it is holding an older version than the store"
        );

        inner.sighted();

        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ferret"))
                .await
                .unwrap()
                .len(),
            1,
            "the record is still there and still served"
        );
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Loaded,
            "a scan that reaches the store again clears the mark"
        );
    }

    /// A store that just hands back the docs it was given, and can drop them.
    ///
    /// Its doc ids are deliberately **not** entity handles — the real store's
    /// shape, where Outline mints a UUID per page. The fake keys facts by
    /// handle, so anything that turns on the gap between the two ids is
    /// invisible to it and shows up only here.
    struct Scanned {
        docs: RwLock<Vec<DocScan>>,
        /// The store takes writes and cannot be read back — a transient fault on
        /// the re-read the decorator makes after a write has already committed.
        blind: std::sync::atomic::AtomicBool,
    }

    impl Scanned {
        /// Nothing like a handle: the ghost bug was deleting by one id having
        /// stored under the other.
        const DOC_ID: &'static str = "outline-uuid-7f3a";

        fn new(docs: Vec<DocScan>) -> Arc<Self> {
            Arc::new(Scanned {
                docs: RwLock::new(docs),
                blind: std::sync::atomic::AtomicBool::new(false),
            })
        }

        /// The page is deleted in the wiki.
        fn vanish(&self) {
            self.docs.write().expect("docs poisoned").clear();
        }

        /// One page is deleted and the rest of the store is untouched — a hand
        /// deletion, as opposed to a store that went away.
        fn lose(&self, doc_id: &str) {
            self.docs
                .write()
                .expect("docs poisoned")
                .retain(|d| d.doc_id != doc_id);
        }

        /// Writes still land; reads fail from here until [`sighted`](Self::sighted).
        fn blinded(&self) {
            self.blind.store(true, std::sync::atomic::Ordering::SeqCst);
        }

        fn sighted(&self) {
            self.blind.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    }

    #[async_trait]
    impl Memory for Scanned {
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            if self.blind.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(MemoryError::Store("the store cannot be read".into()));
            }
            Ok(self.docs.read().expect("docs poisoned").clone())
        }

        /// Append a row to the subject's page and hand it back. **No guard** —
        /// the gates have their own specs, and what this double exists to
        /// exercise is what the decorator does *after* a write lands.
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            let mut docs = self.docs.write().expect("docs poisoned");
            let doc = docs
                .iter_mut()
                .find(|d| d.entity.as_ref().is_some_and(|e| e.id == fact.subject))
                .ok_or_else(|| MemoryError::Store("this double writes to pages it holds".into()))?;
            let stored = Fact {
                id: jojobot_domain::memory::FactId(format!("f{}", doc.facts.len() + 1)),
                home: fact.subject.clone(),
                subject: fact.subject,
                content: fact.content,
                details: fact.details,
                provenance: fact.provenance,
                standing: Standing::Open,
                status: fact.status,
                date: fact.date,
                edge: fact.edge,
                event: None,
                derived_from: fact.derived_from,
            };
            doc.facts.push(stored.clone());
            Ok(Guarded::Written(stored))
        }

        async fn add_entity(&self, _: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            unimplemented!("this double only scans")
        }
        async fn list_entities(&self, _: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
            unimplemented!("this double only scans")
        }
        /// Rename the entity on the page that declares it. **No guard**, for the
        /// same reason `capture` has none: what is under test is what the
        /// decorator does after a write lands, not whether the write was allowed.
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            let mut docs = self.docs.write().expect("docs poisoned");
            let doc = docs
                .iter_mut()
                .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == handle))
                .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
            let entity = doc.entity.as_mut().expect("found by its entity");
            jojobot_domain::memory::apply_entity_patch(entity, &patch)?;
            doc.title = entity.name.clone();
            Ok(Guarded::Written(entity.clone()))
        }
        async fn recall(&self, _: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            unimplemented!("this double only scans")
        }
        /// Replace the prose on the page that declares this entity. **No
        /// guard**, for the reason `capture` has none.
        async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
            let mut docs = self.docs.write().expect("docs poisoned");
            let doc = docs
                .iter_mut()
                .find(|d| d.entity.as_ref().is_some_and(|e| &e.id == entity))
                .ok_or_else(|| MemoryError::Store("this double edits pages it holds".into()))?;
            doc.prose = prose.trim().to_string();
            Ok(doc.prose.clone())
        }
        async fn update_fact(
            &self,
            _: &FactAddress,
            _: FactPatch,
        ) -> Result<Guarded<Fact>, MemoryError> {
            unimplemented!("this double only scans")
        }

        async fn retract(
            &self,
            _: &FactAddress,
            _: Option<&str>,
            _: Date,
        ) -> Result<Retraction, MemoryError> {
            unimplemented!("this double only scans")
        }
    }

    /// One page with one entity on it, for the tests that turn on what the
    /// decorator does after a write lands.
    fn one_page() -> Arc<Scanned> {
        Scanned::new(vec![DocScan {
            doc_id: Scanned::DOC_ID.into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha", "Alpha")),
            facts: Vec::new(),
        }])
    }

    /// One row to write onto that page.
    fn ferret() -> NewFact {
        NewFact {
            subject: EntityId::person("alpha"),
            content: "keeps a ferret".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: None,
            status: FactStatus::Active,
            date: date(2026, 1, 1),
            edge: None,
            event: None,
            derived_from: None,
        }
    }

    /// **A write the store took is not failed by the projection behind it.**
    ///
    /// The re-index re-reads the doc AFTER the store has committed, so a
    /// transient fault on that read used to fail the whole verb — and the
    /// sentence a caller got said nothing was written and told them to try
    /// again. They try again, and the fact is recorded twice. The store is the
    /// truth; an index that could not refresh is a stale projection, not a
    /// failed write.
    #[tokio::test]
    async fn a_write_the_store_took_is_not_failed_by_a_refresh_that_could_not_run() {
        let inner = one_page();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");

        inner.blinded();
        let written = store
            .capture(ferret())
            .await
            .expect("the store took the write, so the verb reports it");

        // The positive the whole case rests on: the write is reported, and it is
        // the write that was made. Asserting only that no error came back would
        // pass on a verb that returned an empty answer.
        let fact = written.written().expect("this double does not guard");
        assert_eq!(fact.content, "keeps a ferret");
        assert_eq!(
            inner
                .docs
                .read()
                .expect("docs poisoned")
                .iter()
                .flat_map(|d| d.facts.iter())
                .map(|f| f.content.as_str())
                .collect::<Vec<_>>(),
            vec!["keeps a ferret"],
            "the row is on the page: the write committed and stayed committed"
        );
    }

    /// **An index that could not refresh says so, on every answer.**
    ///
    /// Keeping the write and saying nothing would move the wrong answer from the
    /// caller who wrote to every session that reads afterwards — and those have
    /// no way to know. `search` is the only verb served from the index, so this
    /// is where it has to be said.
    #[tokio::test]
    async fn a_doc_the_index_could_not_re_read_makes_the_memory_half_partial() {
        let inner = one_page();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Loaded,
            "a scan that read the store holds all of it"
        );

        inner.blinded();
        store
            .capture(ferret())
            .await
            .expect("the store took the write");

        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Partial(Behind::Stale),
            "the index holds the version before that write and has to say so"
        );
        assert_eq!(
            store.index().behind_now(),
            vec![EntityId::person("alpha")],
            "and it knows which document it is behind on"
        );
        // What `Partial` claims, and the half a bare "not Loaded" assertion would
        // miss: the hits that ARE there are real, and the new row is not among
        // them.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("person:alpha"))
                .await
                .expect("search ok")
                .len(),
            1,
            "the entity is still findable — partial is not empty"
        );
        assert!(
            store
                .search_via_port(&SearchQuery::text("ferret"))
                .await
                .expect("search ok")
                .is_empty(),
            "and the row the index could not read is the part that is missing"
        );
    }

    /// **The other way into `Partial`, and it takes away far more.**
    ///
    /// A boot scan that never ran leaves the index holding nothing except what
    /// this process has written since. A stale doc leaves it holding everything
    /// except one write. Those are opposite claims about an empty result, and a
    /// caller decides whether to believe one on exactly that difference — so
    /// `Partial` has to say which state it is reporting.
    #[tokio::test]
    async fn a_boot_scan_that_never_ran_is_partial_for_the_other_reason() {
        let inner = Scanned::new(vec![
            DocScan {
                doc_id: Scanned::DOC_ID.into(),
                title: "Alpha".into(),
                prose: String::new(),
                entity: Some(entity("person:alpha", "Alpha")),
                facts: Vec::new(),
            },
            DocScan {
                doc_id: "outline-uuid-b2c9".into(),
                title: "Beta".into(),
                prose: String::new(),
                entity: Some(entity("person:beta", "Beta")),
                facts: vec![fact(
                    "person:beta",
                    "f1",
                    "restrings the harp",
                    date(2026, 1, 1),
                )],
            },
        ]);
        inner.blinded();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store
            .rebuild()
            .await
            .expect_err("the scan cannot read the store");
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Unread,
            "nothing was read and nothing has been written"
        );

        inner.sighted();
        store
            .capture(ferret())
            .await
            .expect("the store took the write");

        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Partial(Behind::Unscanned),
            "the scan never ran, so the index holds only the page this write touched"
        );
        assert!(
            store.index().behind_now().is_empty(),
            "and no document is behind: this write was re-read, which is the other state"
        );
        // Blind again to read the state itself. A read takes its own scan, so
        // "the scan never ran" is only observable while it still cannot run —
        // and that is the state this test is about.
        inner.blinded();
        // The positive the negative below depends on. Without it, "beta is
        // missing" passes just as well on an index that holds nothing at all,
        // which is the state this one is being told apart from.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ferret"))
                .await
                .expect("search ok")
                .len(),
            1,
            "what this process wrote is indexed and findable"
        );
        assert!(
            store
                .search_via_port(&SearchQuery::text("harp"))
                .await
                .expect("search ok")
                .is_empty(),
            "and the page the scan never read is the part that is missing"
        );
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Partial(Behind::Unscanned),
            "the wider claim still wins: almost nothing is searchable"
        );

        // And it is not a state that lasts until a restart. The first read that
        // reaches the store fills the half that was never scanned.
        inner.sighted();
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("harp"))
                .await
                .expect("search ok")
                .len(),
            1,
            "the page arrives on the first read that can reach the store"
        );
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Loaded,
            "…and the answer stops hedging, because the scan behind it ran"
        );
    }

    /// **A refresh that runs takes the mark off.** The state is "the index is
    /// behind on this document", not "something went wrong once" — so the write
    /// that catches it up clears it, and the answers stop hedging.
    #[tokio::test]
    async fn a_refresh_that_runs_clears_the_mark_and_the_hedge() {
        let inner = one_page();
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");

        inner.blinded();
        store.capture(ferret()).await.expect("the store took it");
        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Partial(Behind::Stale)
        );

        inner.sighted();
        store
            .capture(NewFact {
                content: "plays the sousaphone".into(),
                ..ferret()
            })
            .await
            .expect("the store took it");

        assert_eq!(
            store.index().memory_coverage(),
            Coverage::Loaded,
            "the doc was re-read whole, so nothing on it is behind any more"
        );
        assert!(store.index().behind_now().is_empty());
        // The re-read is of the whole document, so the row written while the
        // store was unreadable comes back with it.
        for missed in ["ferret", "sousaphone"] {
            assert_eq!(
                store
                    .search_via_port(&SearchQuery::text(missed))
                    .await
                    .expect("search ok")
                    .len(),
                1,
                "{missed} is findable once the refresh runs"
            );
        }
    }

    /// **A vanished doc leaves no ghost.** Eviction has to key on the id the
    /// postings were written under — the store's doc id — not the entity handle.
    /// It keyed on the handle, so in the real store (where a doc id is an Outline
    /// UUID) the delete matched nothing: the page was gone from the wiki and every
    /// one of its hits was still being served, forever, from the last scan.
    #[tokio::test]
    async fn reindexing_a_vanished_doc_evicts_every_hit_it_had() {
        let inner = Scanned::new(vec![DocScan {
            doc_id: Scanned::DOC_ID.into(),
            title: "Alpha".into(),
            prose: "Alpha is allergic to penicillin.".into(),
            entity: Some(entity("person:alpha", "Alpha")),
            facts: vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
        }]);
        let store = Arc::new(IndexedMemory::new(inner.clone()).expect("index opens"));
        store.rebuild().await.expect("rebuild");

        let alpha = EntityId::person("alpha");
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ferret"))
                .await
                .expect("search ok")
                .len(),
            1
        );
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("penicillin"))
                .await
                .expect("search ok")
                .len(),
            1
        );
        assert!(
            store
                .search_via_port(&SearchQuery::text("person:alpha"))
                .await
                .expect("search ok")
                .iter()
                .any(|h| matches!(h, Hit::Entity { .. }))
        );

        inner.vanish();
        store.reindex(&alpha).await.expect("reindex ok");

        for gone in ["ferret", "penicillin"] {
            assert!(
                store
                    .search_via_port(&SearchQuery::text(gone))
                    .await
                    .expect("search ok")
                    .is_empty(),
                "{gone:?} must be gone from the index with its doc"
            );
        }
        assert!(
            store
                .search_via_port(&SearchQuery::text("person:alpha"))
                .await
                .expect("search ok")
                .is_empty(),
            "…and so must the entity, pin and all"
        );
    }

    /// The crate's one log sink. **Shared, not local**: other stores also
    /// reports things whose only surface is a log line, and a process gets
    /// exactly one global subscriber — so the sink lives beside both of them.
    use crate::log_capture::log_sink;

    /// **A row whose subject names nothing is counted and said out loud.** This
    /// is the split-brain tell a hand edit leaves: the row stays reachable
    /// through its home doc, so nothing breaks and nothing looks wrong — which
    /// is exactly the problem. A scan that silently normalizes a corruption is
    /// how the corruption becomes permanent.
    ///
    /// Never a failure, never a drop: the fact is still indexed and still found.
    #[tokio::test]
    async fn a_scan_counts_and_logs_a_subject_that_names_no_entity() {
        let logged = log_sink();

        let orphan = Fact {
            subject: EntityId::person("alphaa"),
            ..fact("person:alpha", "f1", "plays chess", date(2026, 1, 1))
        };
        // Its own doc id, unique in this binary. The sink is the binary's — one
        // subscriber shared by every test in it — so anything counted over its
        // text has to be counted by something no other test can log.
        const REPORTED_DOC: &str = "outline-uuid-orphan-report";
        let store = Arc::new(
            IndexedMemory::new(Scanned::new(vec![scan(
                REPORTED_DOC,
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![
                    orphan,
                    fact("person:alpha", "f2", "plays go", date(2026, 1, 2)),
                ],
            )]))
            .expect("index opens"),
        );
        store.rebuild().await.expect("rebuild");

        let text = logged.text();
        assert!(
            text.contains(REPORTED_DOC),
            "the log must say which doc: {text}"
        );
        assert!(text.contains("person:alphaa"), "…and which subject: {text}");
        assert!(text.contains("count=1"), "…and how many: {text}");
        assert!(
            !text.contains("person:alpha\""),
            "the doc's own well-formed subject is not an orphan: {text}"
        );

        // …and the row is still there. Reporting it is not quarantining it.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("chess"))
                .await
                .expect("search ok")
                .len(),
            1,
            "a counted row is still indexed and still findable"
        );
        // That read re-scanned the store, and re-scanning is not re-reporting.
        // The report is a reading of the corpus; one per answer would bury it.
        assert_eq!(
            logged.text().matches(REPORTED_DOC).count(),
            1,
            "a read refreshes the index without repeating the report"
        );
    }

    /// **The count survives the incremental path too.** A boot scan is not the
    /// only way a doc gets read: every write re-reads the page it touched, and
    /// that is the read most likely to be looking at a page a human just edited
    /// by hand. A counter wired only into `rebuild` would go quiet for as long as
    /// the server stayed up — exactly the window in which the damage is done.
    ///
    /// Nothing here calls `rebuild`, so the only thing that can have written to
    /// the log is the write.
    #[tokio::test]
    async fn a_write_re_reads_its_doc_and_counts_the_orphans_it_finds() {
        let logged = log_sink();
        const DOC: &str = "outline-uuid-re1nd3x";

        let hand_edited = Fact {
            subject: EntityId::person("ghostly"),
            ..fact(
                "person:alpha",
                "f1",
                "subject cell retyped by hand",
                date(2026, 1, 1),
            )
        };
        let store = Arc::new(
            IndexedMemory::new(Scanned::new(vec![scan(
                DOC,
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![hand_edited],
            )]))
            .expect("index opens"),
        );

        store
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "an ordinary fact, written now",
                date(2026, 1, 2),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("this double does not guard");

        let text = logged.text();
        assert!(text.contains(DOC), "the reindex must say which doc: {text}");
        assert!(
            text.contains("person:ghostly"),
            "…and which subject: {text}"
        );

        // …and the write is findable, which is the reindex having run at all.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ordinary"))
                .await
                .expect("search ok")
                .len(),
            1
        );
    }

    /// The **other** counter, on the same path. A subject retyped onto a handle
    /// that EXISTS orphans nothing, so the orphan reporter has nothing to say
    /// about it — which is exactly the disguise the Cosme split brain wore. Both
    /// counters have to survive a write, not just a boot scan; only the boot
    /// scan was pinned, so dropping the foreign one from the write path left the
    /// whole suite green.
    #[tokio::test]
    async fn a_write_re_reads_its_doc_and_counts_the_foreign_subjects_it_finds() {
        let logged = log_sink();
        const DOC: &str = "outline-uuid-f0r31gn";

        let elsewhere = Fact {
            subject: EntityId::person("kappa"),
            ..fact(
                "person:alpha",
                "f1",
                "subject cell retyped onto a live handle",
                date(2026, 1, 1),
            )
        };
        let store = Arc::new(
            IndexedMemory::new(Scanned::new(vec![
                scan(
                    DOC,
                    Some(entity("person:alpha", "Alpha")),
                    "",
                    vec![elsewhere],
                ),
                scan(
                    "outline-uuid-kappa",
                    Some(entity("person:kappa", "Kappa")),
                    "",
                    Vec::new(),
                ),
            ]))
            .expect("index opens"),
        );

        // Make kappa known the way a running server does — one doc at a time.
        // Never a rebuild: a rebuild reports this doc itself, and the assertion
        // would stop being about the write path.
        store
            .reindex(&EntityId::person("kappa"))
            .await
            .expect("reindex ok");

        store
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "another ordinary fact, written now",
                date(2026, 1, 2),
            ))
            .await
            .expect("capture ok")
            .written()
            .expect("this double does not guard");

        // Per LINE, not per buffer: the sink is process-wide and append-only, so
        // a substring match would happily find another test's event.
        let text = logged.text();
        assert!(
            text.lines().any(|l| l.contains(DOC)
                && l.contains("person:kappa")
                && l.contains("about a different entity that exists")),
            "the write path must count the foreign subject, not only the orphans: {text}"
        );
    }

    /// **A row about another live entity is counted too** — the consistency check
    /// the orphan counter cannot make. The Cosme incident wore exactly this
    /// shape: a hand edit retyped a subject cell into a handle that *exists*, so
    /// nothing was orphaned and every read went on working, while the entity
    /// quietly became readable under one id and writable under another.
    ///
    /// Legitimate as often as not — a fact about one entity is frequently
    /// written on another's page — so this is a signal, never a fault: counted,
    /// logged with its doc, and the row left exactly where it is.
    #[tokio::test]
    async fn a_scan_counts_and_logs_a_row_about_another_entity() {
        let logged = log_sink();
        const DOC: &str = "outline-uuid-c05m3";

        let elsewhere = Fact {
            subject: EntityId::person("beta"),
            ..fact("person:alpha", "f1", "took the ferry", date(2026, 1, 1))
        };
        let store = Arc::new(
            IndexedMemory::new(Scanned::new(vec![
                scan(
                    DOC,
                    Some(entity("person:alpha", "Alpha")),
                    "",
                    vec![
                        elsewhere,
                        fact("person:alpha", "f2", "took the bus", date(2026, 1, 2)),
                    ],
                ),
                scan(
                    "outline-uuid-beta",
                    Some(entity("person:beta", "Beta")),
                    "",
                    Vec::new(),
                ),
            ]))
            .expect("index opens"),
        );
        store.rebuild().await.expect("rebuild");

        let text = logged.text();
        assert!(text.contains(DOC), "the log must say which doc: {text}");
        assert!(text.contains("person:beta"), "…and which subject: {text}");
        assert!(
            text.contains("count=1"),
            "…and how many — the doc's own subject is not one of them: {text}"
        );

        // Counted is not quarantined: the row is still indexed and still found.
        assert_eq!(
            store
                .search_via_port(&SearchQuery::text("ferry"))
                .await
                .expect("search ok")
                .len(),
            1
        );
    }

    // --- mail in the one list -------------------------------------------------

    fn message(
        id: &str,
        mailbox: &str,
        sender: &str,
        subject: Option<&str>,
        body: &str,
        state: MessageState,
    ) -> Message {
        Message {
            id: MessageId(id.into()),
            mailbox: MailboxName(mailbox.into()),
            body: body.into(),
            subject: subject.map(str::to_string),
            sender: sender.into(),
            sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
            state,
            notes: None,
            in_reply_to: None,
        }
    }

    /// **The write-only rail, opened.** A finding filed in a message comes back
    /// in the same ranked list as the fact, entity and prose hits — which is the
    /// whole slice: a later session finds context it did not know to look for.
    /// And it arrives unmistakably as mail: its box, its state, its sender, and
    /// the id `read_message` takes.
    #[tokio::test]
    async fn a_message_comes_back_in_the_same_ranked_list() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "runs the kiln on Tuesdays",
                date(2026, 1, 1),
            )],
        )]);
        index
            .ingest_mail(&[message(
                "42",
                "pm",
                "dev (implementer)",
                Some("the kiln slice"),
                "The kiln rebuild landed; the damper is still hand-cut.",
                MessageState::Read,
            )])
            .expect("ingest mail");

        let hits = index.search(&asking_for_mail("damper")).expect("search ok");
        let Some(Hit::Message { message, snippet }) =
            hits.iter().find(|h| matches!(h, Hit::Message { .. }))
        else {
            panic!("the message must be a hit: {hits:?}")
        };
        assert_eq!(
            message.id.as_str(),
            "42",
            "…carrying the id read_message takes"
        );
        assert_eq!(message.mailbox.as_str(), "pm", "…and which box it is in");
        assert_eq!(
            message.state,
            MessageState::Read,
            "…and what state it is in"
        );
        assert_eq!(message.sender, "dev (implementer)");
        assert_eq!(message.subject.as_deref(), Some("the kiln slice"));
        assert!(snippet.to_lowercase().contains("damper"), "got {snippet:?}");

        // One list: the same query reaches mail and memory together.
        let mixed = index.search(&asking_for_mail("kiln")).expect("search ok");
        assert!(
            mixed.iter().any(|h| matches!(h, Hit::Fact { .. })),
            "{mixed:?}"
        );
        assert!(
            mixed.iter().any(|h| matches!(h, Hit::Message { .. })),
            "{mixed:?}"
        );
    }

    /// **Every state is searchable, `processed` included** — an archive is
    /// exactly where an old report lives, and the state is on the hit, so a
    /// caller can tell live work from history without a second call.
    #[tokio::test]
    async fn mail_is_searchable_in_every_state_and_the_hit_says_which() {
        let index = index_of(Vec::new());
        index
            .ingest_mail(&[
                message(
                    "1",
                    "pm",
                    "dev",
                    None,
                    "the crates are stacked",
                    MessageState::New,
                ),
                message(
                    "2",
                    "pm",
                    "dev",
                    None,
                    "the crates were counted",
                    MessageState::Read,
                ),
                message(
                    "3",
                    "pm",
                    "dev",
                    None,
                    "the crates went out",
                    MessageState::Processed,
                ),
            ])
            .expect("ingest mail");

        let hits = index.search(&asking_for_mail("crates")).expect("search ok");
        let mut states: Vec<&str> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Message { message, .. } => Some(message.state.as_token()),
                _ => None,
            })
            .collect();
        states.sort_unstable();
        assert_eq!(
            states,
            vec!["new", "processed", "read"],
            "an archived message is still findable: {hits:?}"
        );
    }

    /// Mail is in by default and out when the caller says so — a parameter, not
    /// a mode. Excluding it must not touch the memory half of the answer.
    #[tokio::test]
    async fn include_mail_is_a_filter_the_caller_holds() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "the shipment is late",
                date(2026, 1, 1),
            )],
        )]);
        index
            .ingest_mail(&[message(
                "7",
                "pm",
                "dev",
                None,
                "the shipment is late again",
                MessageState::New,
            )])
            .expect("ingest mail");

        // **Mail is opt-in.** A caller that has not asked for it does not get
        // somebody's message back from the verb the surface tells it to reach
        // for first (rule 62).
        let by_default = index
            .search(&SearchQuery::text("shipment"))
            .expect("search ok");
        assert!(
            !by_default.iter().any(|h| matches!(h, Hit::Message { .. })),
            "mail is not searched unless it is asked for: {by_default:?}"
        );
        assert!(
            by_default.iter().any(|h| matches!(h, Hit::Fact { .. })),
            "…and the memory half is untouched by that: {by_default:?}"
        );

        // Its pair: asked for, it is there. Without this the assertion above
        // passes on an index that cannot return a message at all.
        let asked = index
            .search(&SearchQuery {
                include_mail: true,
                ..SearchQuery::text("shipment")
            })
            .expect("search ok");
        assert!(
            asked.iter().any(|h| matches!(h, Hit::Message { .. })),
            "asked for, mail is in the one list: {asked:?}"
        );
    }

    /// A fact-only filter still returns facts alone, and a kind filter is a
    /// question about entities — a message has neither a lifecycle nor a kind,
    /// so it is out of both answers exactly as nobody's prose is.
    #[tokio::test]
    async fn a_structural_filter_leaves_mail_out_the_way_it_leaves_prose_out() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha wrote about the shipment.",
            vec![fact(
                "person:alpha",
                "f1",
                "the shipment is late",
                date(2026, 1, 1),
            )],
        )]);
        index
            .ingest_mail(&[message(
                "7",
                "pm",
                "dev",
                None,
                "the shipment is late",
                MessageState::New,
            )])
            .expect("ingest mail");

        let fact_scoped = index
            .search(&SearchQuery {
                provenance: Some(Provenance::Inference),
                ..SearchQuery::text("shipment")
            })
            .expect("search ok");
        assert!(!fact_scoped.is_empty());
        assert!(
            fact_scoped.iter().all(|h| matches!(h, Hit::Fact { .. })),
            "a fact-only filter must not surface mail either: {fact_scoped:?}"
        );

        let by_kind = index
            .search(&SearchQuery {
                kind: Some(EntityKind::Person),
                ..SearchQuery::text("shipment")
            })
            .expect("search ok");
        assert!(
            !by_kind.iter().any(|h| matches!(h, Hit::Message { .. })),
            "a message has no entity kind, so asking for one excludes it: {by_kind:?}"
        );
    }

    /// **The projection is a projection here too.** A re-ingest replaces the
    /// mail half wholesale — a message that has since been processed must not
    /// come back beside its own older copy — and it leaves memory alone.
    #[tokio::test]
    async fn re_ingesting_mail_replaces_it_and_leaves_memory_alone() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
        )]);
        index
            .ingest_mail(&[message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::New,
            )])
            .expect("ingest mail");
        index
            .ingest_mail(&[message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::Processed,
            )])
            .expect("re-ingest mail");

        let hits = index
            .search(&asking_for_mail("shipment"))
            .expect("search ok");
        let states: Vec<&str> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Message { message, .. } => Some(message.state.as_token()),
                _ => None,
            })
            .collect();
        assert_eq!(
            states,
            vec!["processed"],
            "one copy, saying the new thing: {hits:?}"
        );
        assert_eq!(
            index
                .search(&asking_for_mail("ferret"))
                .expect("search ok")
                .len(),
            1,
            "rebuilding the mail half must not evict memory"
        );
    }

    /// One message, re-indexed in place — the read-back that makes a posted
    /// message findable on the next call rather than after a restart.
    #[tokio::test]
    async fn one_message_is_re_indexed_in_place() {
        let index = index_of(Vec::new());
        index
            .ingest_mail(&[message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::New,
            )])
            .expect("ingest mail");
        index
            .ingest_message(&message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::Processed,
            ))
            .expect("ingest one");

        let hits = index
            .search(&asking_for_mail("shipment"))
            .expect("search ok");
        assert_eq!(hits.len(), 1, "one copy, not two: {hits:?}");
        assert!(
            matches!(hits.first(), Some(Hit::Message { message, .. }) if message.state == MessageState::Processed)
        );
    }

    /// **A mailbox world that never loaded says so.** An index with no mail in
    /// it answers memory questions exactly as before — degrade, don't error —
    /// but it must not let "no message says that" stand in for "jojobot has
    /// read no messages", which is a different claim and the one a caller would
    /// act on wrongly.
    #[tokio::test]
    async fn an_index_with_no_mail_says_mail_is_not_searchable() {
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact(
                "person:alpha",
                "f1",
                "keeps a ferret",
                date(2026, 1, 1),
            )],
        )]);
        assert_eq!(
            index.mail_coverage(),
            Coverage::Unread,
            "nothing has loaded mail"
        );
        assert_eq!(
            index
                .search(&SearchQuery::text("ferret"))
                .expect("search ok")
                .len(),
            1,
            "the memory half still answers"
        );

        index
            .ingest_mail(&[])
            .expect("an empty board is still a board");
        assert_eq!(
            index.mail_coverage(),
            Coverage::Loaded,
            "a board that was read and holds nothing is not the same as a board nobody read"
        );
    }

    /// **The state a failed boot actually leaves.** The board read never
    /// happened, but every message this process posts or delivers is still
    /// indexed and still comes back as a hit — so reporting "no mail is
    /// searchable" made one answer carry message hits and deny having searched
    /// any. That is a third state, not one of the two.
    #[tokio::test]
    async fn mail_indexed_after_a_failed_board_read_is_partial_not_absent() {
        let index = index_of(Vec::new());
        assert_eq!(index.mail_coverage(), Coverage::Unread);

        // No ingest_mail — the boot read failed. A verb indexes one message.
        index
            .ingest_message(&message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::New,
            ))
            .expect("ingest one");

        assert_eq!(
            index.mail_coverage(),
            Coverage::Partial(Behind::Unscanned),
            "a message that IS findable must never be reported as no mail at all"
        );
        assert_eq!(
            index
                .search(&asking_for_mail("shipment"))
                .expect("search ok")
                .len(),
            1,
            "…and it is findable, which is the whole reason the claim was wrong"
        );

        // A board read later promotes it: now everything is there.
        index.ingest_mail(&[]).expect("the board comes back");
        assert_eq!(index.mail_coverage(), Coverage::Loaded);
    }

    /// **Rebuilding one half must not empty the other.** `ingest_all` wiped the
    /// whole index — mail included — while leaving the flag saying mail was
    /// loaded, so a Memory rebuild silently emptied the mail half and then
    /// vouched for it. Nothing but the boot ordering in `main.rs` stood between
    /// that and production, and boot ordering is not an invariant.
    ///
    /// Both orders, because the bug is exactly an order-dependence.
    #[tokio::test]
    async fn rebuilding_either_half_leaves_the_other_alone() {
        let mail = || {
            message(
                "1",
                "pm",
                "dev",
                None,
                "the shipment landed",
                MessageState::New,
            )
        };
        let docs = || {
            vec![scan(
                "doc-1",
                Some(entity("person:alpha", "Alpha")),
                "",
                vec![fact(
                    "person:alpha",
                    "f1",
                    "keeps a ferret",
                    date(2026, 1, 1),
                )],
            )]
        };
        let both_survive = |index: &FullTextIndex, order: &str| {
            assert_eq!(
                index
                    .search(&asking_for_mail("shipment"))
                    .expect("search ok")
                    .len(),
                1,
                "the mail half is gone after {order}"
            );
            assert_eq!(
                index
                    .search(&asking_for_mail("ferret"))
                    .expect("search ok")
                    .len(),
                1,
                "the memory half is gone after {order}"
            );
            assert_eq!(
                index.mail_coverage(),
                Coverage::Loaded,
                "…and the coverage claim still matches what is actually in there"
            );
        };

        let mail_first = FullTextIndex::open().expect("index opens");
        mail_first.ingest_mail(&[mail()]).expect("ingest mail");
        mail_first.ingest_all(&docs()).expect("ingest docs");
        both_survive(&mail_first, "mail then memory");

        let memory_first = FullTextIndex::open().expect("index opens");
        memory_first.ingest_all(&docs()).expect("ingest docs");
        memory_first.ingest_mail(&[mail()]).expect("ingest mail");
        both_survive(&memory_first, "memory then mail");
    }

    /// **Read-back covers mail too.** A message posted a moment ago is findable
    /// on the next call, without a restart — writing a message search cannot
    /// find is the same class of failure as writing a fact `recall` cannot
    /// return. And the state on the hit follows the message: once it is
    /// processed, the hit says so, because a reader acts on that word.
    #[tokio::test]
    async fn a_posted_message_is_findable_at_once_and_its_state_follows_it() {
        let index = Arc::new(FullTextIndex::open().expect("index opens"));
        let store = IndexedMailboxes::new(Arc::new(InMemoryMailboxes::new()), index.clone());
        mail_contract::create(&store, "pm").await;

        let posted =
            mail_contract::post(&store, "pm", "dev", "the damper is still hand-cut", 0).await;
        let state_of = |index: &FullTextIndex| -> Option<MessageState> {
            index
                .search(&asking_for_mail("damper"))
                .expect("search ok")
                .iter()
                .find_map(|h| match h {
                    Hit::Message { message, .. } => Some(message.state),
                    _ => None,
                })
        };
        assert_eq!(
            state_of(&index),
            Some(MessageState::New),
            "a posted message is findable before anything rebuilds"
        );

        store
            .mark_processed(&posted.id, Some("filed"))
            .await
            .expect("mark_processed ok");
        assert_eq!(
            state_of(&index),
            Some(MessageState::Processed),
            "the hit's state follows the message, or a reader acts on a stale word"
        );
    }

    /// A board with two messages behind the `search` port, ready to lose one.
    /// Two rather than one so every negative below has a survivor to pair with.
    async fn a_board_of_two() -> (Arc<InMemoryMailboxes>, Arc<IndexedMailboxes>, Retrieval) {
        let inner = Arc::new(InMemoryMailboxes::new());
        mail_contract::create(inner.as_ref(), "pm").await;
        mail_contract::post(inner.as_ref(), "pm", "dev", "the kiln is relined", 0).await;
        mail_contract::post(inner.as_ref(), "pm", "dev", "the damper is hand-cut", 1).await;

        let index = Arc::new(FullTextIndex::open().expect("index opens"));
        let mail = Arc::new(IndexedMailboxes::new(inner.clone(), index.clone()));
        mail.rebuild().await.expect("rebuild");
        let port = Retrieval::new(index, vec![mail.clone()]);
        (inner, mail, port)
    }

    /// **A message removed from the store stops being served, with no write to
    /// prompt it.** Rule 60 puts true removal outside jojobot, so nothing calls
    /// a refresh and the read path is the only place that can notice.
    #[tokio::test]
    async fn a_message_removed_from_the_store_stops_being_served() {
        let (inner, _mail, port) = a_board_of_two().await;
        for present in ["kiln", "damper"] {
            assert_eq!(
                port.search(&asking_for_mail(present))
                    .await
                    .expect("search ok")
                    .len(),
                1,
                "{present:?} is served while both messages are on the board"
            );
        }

        inner.lose(&MessageId("1".into()));

        assert!(
            port.search(&asking_for_mail("kiln"))
                .await
                .expect("search ok")
                .is_empty(),
            "the message left the board, so search must stop serving it"
        );
        assert_eq!(
            port.search(&asking_for_mail("damper"))
                .await
                .expect("search ok")
                .len(),
            1,
            "…and the one still on the board is still served"
        );
    }

    /// **A card that has become unreadable is the same absence**, and it costs
    /// no machinery of its own: a board read leaves a quarantined card out, so
    /// it arrives at the index as a message the scan no longer carries.
    #[tokio::test]
    async fn a_quarantined_card_stops_being_served_too() {
        let (inner, _mail, port) = a_board_of_two().await;

        inner.quarantine(
            &MailboxName("pm".into()),
            &MessageId("1".into()),
            "the card cannot be read",
        );

        assert!(
            port.search(&asking_for_mail("kiln"))
                .await
                .expect("search ok")
                .is_empty(),
            "jojobot cannot read it, so search must not go on serving its content"
        );
        assert_eq!(
            port.search(&asking_for_mail("damper"))
                .await
                .expect("search ok")
                .len(),
            1,
            "…and the readable one beside it is untouched"
        );
    }

    /// **A search takes no delivery.** Reading IS delivery on the verbs that
    /// deliver, so a refresh that used one would drain a box as a side effect of
    /// answering a question — a data-loss defect wearing a correctness fix, and
    /// the kind a coverage assertion would pass straight over.
    #[tokio::test]
    async fn a_search_leaves_every_message_state_as_it_found_it() {
        let (inner, _mail, port) = a_board_of_two().await;
        let before = mail_contract::counts(inner.as_ref(), "pm").await;
        assert_eq!(
            before.map(|c| c.new),
            Some(2),
            "both messages start new and nobody has taken them"
        );

        assert_eq!(
            port.search(&asking_for_mail("kiln"))
                .await
                .expect("search ok")
                .len(),
            1,
            "the search really ran and really answered"
        );

        assert_eq!(
            mail_contract::counts(inner.as_ref(), "pm").await,
            before,
            "…and every state is exactly as it was, the untaken ones included"
        );
    }

    /// **The mail half stops claiming complete while it is serving a board it
    /// could not re-read.** Rule 130: the wrong answer is the defect, and the
    /// wrong answer that vouches for itself is worse.
    #[tokio::test]
    async fn mail_coverage_stops_claiming_loaded_when_the_read_cannot_reach_the_store() {
        let (inner, _mail, port) = a_board_of_two().await;
        assert_eq!(
            port.mail_coverage(),
            Coverage::Loaded,
            "a board read that just succeeded is complete coverage"
        );

        inner.blind();

        assert_eq!(
            port.search(&asking_for_mail("kiln"))
                .await
                .expect("search ok")
                .len(),
            1,
            "degrade, don't error: the last good board read still answers"
        );
        assert_eq!(
            port.mail_coverage(),
            Coverage::Partial(Behind::Stale),
            "…and the answer says it is holding an older board than the store"
        );

        inner.sighted();

        assert_eq!(
            port.search(&asking_for_mail("kiln"))
                .await
                .expect("search ok")
                .len(),
            1,
            "the message is still there and still served"
        );
        assert_eq!(
            port.mail_coverage(),
            Coverage::Loaded,
            "a board read that reaches the store again clears the mark"
        );
    }

    /// A rebuild loads the board that was already there — the boot path — and a
    /// blocked post leaves nothing behind, exactly as a blocked capture does.
    #[tokio::test]
    async fn a_rebuild_loads_the_board_and_a_blocked_post_indexes_nothing() {
        let inner = Arc::new(InMemoryMailboxes::new());
        mail_contract::create(inner.as_ref(), "pm").await;
        mail_contract::post(
            inner.as_ref(),
            "pm",
            "dev",
            "written before the server started",
            0,
        )
        .await;

        let index = Arc::new(FullTextIndex::open().expect("index opens"));
        let store = IndexedMailboxes::new(inner, index.clone());
        assert!(
            index
                .search(&asking_for_mail("started"))
                .expect("search ok")
                .is_empty(),
            "nothing is indexed until the board is read"
        );
        assert_eq!(store.rebuild().await.expect("rebuild"), 1);
        assert_eq!(
            index
                .search(&asking_for_mail("started"))
                .expect("search ok")
                .len(),
            1
        );

        let blocked = store
            .post_message(jojobot_domain::mailbox::NewMessage {
                mailbox: MailboxName("pmm".into()),
                body: "should not be indexed".into(),
                subject: None,
                sender: "dev".into(),
                sent_at: jiff::Timestamp::from_second(1_780_000_000).expect("a fixed instant"),
                in_reply_to: None,
            })
            .await
            .expect("a blocked post is a result, not a failure");
        assert!(matches!(
            blocked,
            jojobot_domain::mailbox::Guarded::Blocked { .. }
        ));
        assert!(
            index
                .search(&asking_for_mail("should not be indexed"))
                .expect("search ok")
                .is_empty(),
            "a blocked post must leave nothing in the index either"
        );
    }

    /// **The payload and the index agree about what is a link — checked, not
    /// conventional.**
    ///
    /// An event's references live in the payload and are projected into the
    /// index; the row's own edge column knows nothing about them. That split is
    /// the right one — the payload is the general form and the edge column the
    /// special case left from before fields existed — but a split held together
    /// by convention reads fine today and is a bug in eighteen months. So it is
    /// an invariant with a test on it: every payload value that is an entity
    /// handle is walkable, and every value that is not is not.
    ///
    /// **Whatever its key**, which is the half that is invisible today. `ref` is
    /// the unnamed member of a family; when types land, `mechanic=person:x` is
    /// the same link with the key doing the annotating. Nothing in this slice
    /// has a named field, so a projection keyed on the literal word `ref` would
    /// pass every test here and silently drop every named reference the day the
    /// first type ships.
    #[tokio::test]
    async fn every_payload_value_that_is_a_handle_is_walkable_and_nothing_else_is() {
        let event = Fact {
            event: Some(Event {
                kind: "a-thing-that-happened".into(),
                metadata: [
                    // Named, and it must walk exactly as the unnamed one does.
                    ("mechanic".to_string(), "person:milhouse".to_string()),
                    // Not handles: these must NOT become edges.
                    ("mood".to_string(), "delighted".to_string()),
                    ("nearly".to_string(), "person:".to_string()),
                ]
                .into_iter()
                .collect(),
                refs: vec![EntityId("place:north-trail".into())],
            }),
            ..fact("person:alpha", "f1", "the kiln was lit", date(2026, 1, 1))
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha's page.",
            vec![event.clone()],
        )]);
        let walks = |object: &str| {
            !index
                .search(&SearchQuery {
                    edge: Some(EdgeFilter {
                        shape: None,
                        object: EntityId(object.into()),
                    }),
                    ..Default::default()
                })
                .expect("search ok")
                .is_empty()
        };

        assert!(
            walks("place:north-trail"),
            "the unnamed reference is walkable"
        );
        assert!(
            walks("person:milhouse"),
            "…and so is the NAMED one, which is the case nothing else here covers"
        );
        // **The "and nothing else" half is asserted in the domain**, by
        // `any_value_that_is_a_handle_is_a_link_whatever_its_key`, and it has
        // to be: a value like "delighted" or "person:" is not a well-formed
        // handle, so the query layer refuses to be asked about it at all. That
        // refusal is the right behaviour and it means the index cannot be
        // interrogated for a link it should never have made — `linked()` is
        // where that boundary is observable.

        // …and they are `connection`s: the link is admitted, never asserted.
        assert!(
            index
                .search(&SearchQuery {
                    edge: Some(EdgeFilter {
                        shape: Some(EdgeShape::About),
                        object: EntityId("person:milhouse".into()),
                    }),
                    ..Default::default()
                })
                .expect("search ok")
                .is_empty(),
            "an event's link must not answer a query for an asserted `about`"
        );
    }

    /// **A shape and an object are asked about together, because a fact can now
    /// carry several edges.**
    ///
    /// The two were independent fields, which was harmless while a fact had at
    /// most one edge and is not any more: a fact located at one place and
    /// linked by its payload to a person would match `shape=location AND
    /// object=that person`. Neither half is wrong on its own, which is exactly
    /// why the pair is what gets indexed.
    #[tokio::test]
    async fn a_shape_filter_does_not_match_a_different_edges_object() {
        let mixed = Fact {
            // **`about`, deliberately.** The kind-constrained shapes cannot
            // demonstrate this: `location` + a person is refused by the query
            // guard before any matching happens, so the confusion is
            // unreachable there. `about` and `connection` both take any kind,
            // which is exactly where an uncorrelated shape and object would
            // quietly answer the wrong question.
            edge: Some(Edge::new(
                EdgeShape::About,
                EntityId("topic:widgets".into()),
            )),
            event: Some(Event {
                kind: "a-thing-that-happened".into(),
                metadata: Default::default(),
                refs: vec![EntityId("person:milhouse".into())],
            }),
            ..fact("person:alpha", "f1", "the kiln was lit", date(2026, 1, 1))
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha's page.",
            vec![mixed],
        )]);
        let ask = |shape, object: &str| {
            index
                .search(&SearchQuery {
                    edge: Some(EdgeFilter {
                        shape: Some(shape),
                        object: EntityId(object.into()),
                    }),
                    ..Default::default()
                })
                .expect("search ok")
        };

        assert!(
            !ask(EdgeShape::About, "topic:widgets").is_empty(),
            "the pair that exists is found"
        );
        assert!(
            !ask(EdgeShape::Connection, "person:milhouse").is_empty(),
            "…and so is the other one"
        );
        assert!(
            ask(EdgeShape::About, "person:milhouse").is_empty(),
            "a shape from one edge must not combine with an object from another"
        );
        assert!(
            ask(EdgeShape::Connection, "topic:widgets").is_empty(),
            "…in either direction"
        );
    }

    /// **An untyped edge is walkable, or rule 98 is unimplemented.**
    ///
    /// The whole claim of the fifth shape is that deferring what a link MEANS
    /// does not defer that it EXISTS. A `connection` that could not be followed
    /// would be a note-to-self rather than an edge, and "the pointer is real,
    /// only its nature is deferred" would be a sentence with nothing under it.
    ///
    /// **Both ways of asking**, because they fail differently. Asking for the
    /// shape by name proves it was indexed under its own token; asking with no
    /// shape at all proves it is in the general connected-to answer rather than
    /// only findable by somebody who already knew to ask for `connection` —
    /// which is the query a reader actually writes when the question is "what
    /// touches this?".
    #[tokio::test]
    async fn an_untyped_edge_is_walkable_like_any_other() {
        let linked = Fact {
            edge: Some(Edge::new(
                EdgeShape::Connection,
                EntityId("place:shelbyville".into()),
            )),
            ..fact(
                "person:alpha",
                "f1",
                "was there when it happened",
                date(2026, 1, 1),
            )
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha's page.",
            vec![linked.clone()],
        )]);
        let alpha = EntityRef::resolved(&entity("person:alpha", "Alpha"));
        let expected = vec![Hit::Fact {
            fact: linked,
            subject: alpha.clone(),
            home: alpha,
        }];

        for shape in [Some(EdgeShape::Connection), None] {
            let hits = index
                .search(&SearchQuery {
                    edge: Some(EdgeFilter {
                        shape,
                        object: EntityId("place:shelbyville".into()),
                    }),
                    ..Default::default()
                })
                .expect("search ok");
            assert_eq!(hits, expected, "shape filter {shape:?} did not walk it");
        }

        // **And it is not quietly an `about`.** Asking for the asserted shape
        // must not return the admitted one, or the distinction the fifth shape
        // exists for is erased at exactly the point somebody reads it.
        let as_about = index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape: Some(EdgeShape::About),
                    object: EntityId("place:shelbyville".into()),
                }),
                ..Default::default()
            })
            .expect("search ok");
        assert!(
            as_about.is_empty(),
            "an admitted link answered a query for an asserted one: {as_about:?}"
        );
    }

    /// An edge filter is a filter, not a text match: it finds the fact carrying
    /// the edge and not the one that merely names the object.
    #[tokio::test]
    async fn an_edge_filter_beats_a_prose_mention() {
        let edged = Fact {
            edge: Some(Edge::new(
                EdgeShape::Location,
                EntityId("place:shelbyville".into()),
            )),
            ..fact(
                "person:alpha",
                "f1",
                "spending the winter away",
                date(2026, 1, 1),
            )
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha talks about shelbyville constantly and has never been.",
            vec![
                edged.clone(),
                fact(
                    "person:alpha",
                    "f2",
                    "wants to visit shelbyville someday",
                    date(2026, 1, 2),
                ),
            ],
        )]);

        let hits = index
            .search(&SearchQuery {
                edge: Some(EdgeFilter {
                    shape: Some(EdgeShape::Location),
                    object: EntityId("place:shelbyville".into()),
                }),
                ..Default::default()
            })
            .expect("search ok");
        let alpha = EntityRef::resolved(&entity("person:alpha", "Alpha"));
        assert_eq!(
            hits,
            vec![Hit::Fact {
                fact: edged,
                subject: alpha.clone(),
                home: alpha,
            }],
            "got {hits:?}"
        );
    }
}
