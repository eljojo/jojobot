//! The search projection — jojobot's front door, and the [`Memory`] decorator
//! that keeps it honest.
//!
//! Four pieces, deliberately separable:
//!
//! * [`FullTextIndex`] — an in-RAM tantivy index over **entities, facts, prose
//!   and messages at once**. Truth stays in the store; this is a projection, and
//!   it is allowed to be one only because it is rebuilt from a full re-scan and
//!   never written to directly. It is **not** the domain's [`Search`] port: it
//!   holds no store, so it cannot take the reading the port promises an answer
//!   is backed by.
//! * [`Retrieval`] — the [`Search`] port itself, over that index plus one
//!   [`Refresh`] half per store. It refreshes every half before it answers, so
//!   an answer is backed by a reading taken for it.
//! * [`IndexedMemory`] — the same Memory port, wrapped so that **read-back
//!   extends to the index**: after any successful write, the touched document is
//!   re-scanned *from the store* and re-indexed, so a fact captured a moment ago
//!   is findable with no restart. Re-reading rather than patching is the point —
//!   a partial-update bug has nowhere to live. It is also the memory
//!   [`Refresh`] half, so the same re-scan runs before every answer.
//! * [`IndexedMailboxes`] — the mail half, and the same two jobs: every verb
//!   that changes a message re-indexes it, and a board read runs before every
//!   answer.
//!
//! Ranking is hardcoded, not configurable: text relevance, a small recency
//! boost, and an entity whose handle or name the query matches pinned to the top
//! — pinned by the **write guard's own matcher**, so search and the guard can
//! never disagree about what "that's the same thing" means.

use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use tantivy::collector::TopDocs;
use tantivy::query::{AllQuery, BooleanQuery, Occur, Query, TermQuery};
use tantivy::schema::{Field, IndexRecordOption, STORED, STRING, Schema, Value};
use tantivy::{Index, IndexReader, IndexWriter, TantivyDocument, Term, doc};

use jiff::civil::Date;
use jojobot_domain::mailbox::{MailboxError, Mailboxes, Message};
use std::collections::BTreeMap;

use jojobot_domain::memory::{
    ClaimWrite, Edge, EdgeShape, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress,
    FactId, FactPatch, FactStatus, FieldWrite, FormerHandle, Guarded, Memory, MemoryError, Merge,
    NewEntity, NewFact, Retraction, WriteSummary,
    guard::{self, MatchReason},
    kinds,
    search::{
        self, Behind, Coverage, DocScan, EntityRef, Hit, RankClock, Search, SearchQuery,
        SourceStanding,
    },
    types::{DeclaredType, Displaced},
};

mod decorators;
pub use decorators::{IndexedMailboxes, IndexedMemory, IndexedSessions, Refresh};

/// How much a fresh fact is worth against text relevance. Small on purpose: it
/// breaks ties and pulls a newer fact past an equally-relevant older one, and it
/// never buries a better match.
const RECENCY_WEIGHT: f32 = 0.1;

/// How many candidates to pull per hit class before the recency re-rank, so an
/// item that the boost would have lifted into the page isn't cut before it can be.
fn candidate_depth(limit: usize) -> usize {
    limit.saturating_mul(3).saturating_add(10)
}

/// **One fact's earlier wordings, concatenated, keyed by the fact's local
/// id.** What `write_doc` adds to `history_text`. A fact nobody has
/// corrected has no entry — matching the concatenated string a query would
/// otherwise search over an empty field.
type FactHistoryTerms = std::collections::HashMap<FactId, String>;

/// [`FactHistoryTerms`] for every entity a scan reached, keyed by the
/// entity's own id — what `ingest_all`/`ingest_changes` look each document's
/// terms up in.
type CorpusHistoryTerms = std::collections::HashMap<EntityId, FactHistoryTerms>;

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
    /// One beat of a session's chronology, with the run it belongs to and the
    /// bot that owns it. **The owner rides the payload as well as the index
    /// field**, because a hit has to say whose it is to a reader, not only to
    /// the query that found it.
    Session {
        session: jojobot_domain::session::SessionId,
        bot: EntityId,
        working_on: Option<String>,
        text: String,
    },
}

/// The hit-class token, indexed so a query can ask for one class of thing.
const CLASS_ENTITY: &str = "entity";
const CLASS_FACT: &str = "fact";
const CLASS_PROSE: &str = "prose";
const CLASS_MESSAGE: &str = "message";
/// A beat from a session's chronology. **Owner-scoped at query time**, never
/// at ingest: what a bot may read is a property of the asker, and an index that
/// held only one bot's runs could not serve the next one.
const CLASS_SESSION: &str = "session";

/// The index's fields. One schema for all three hit classes — a mixed ranked list
/// is the requirement, and one schema is what makes it one query.
struct Fields {
    /// Which class of thing this document is.
    class: Field,
    /// Everything searchable, tokenized: handles, names, claims, details, prose.
    text: Field,
    /// **A fact's earlier wordings, tokenized like `text`.** Never in the
    /// unconditional match — gated at query time by `SearchQuery`'s
    /// `include_history` — because search's ordinary promise is over current
    /// content, and this field exists to widen a query past that promise on
    /// request, not to change what an ordinary query finds. Empty on every
    /// document that is not a fact, and on a fact nobody has ever corrected.
    history_text: Field,
    /// The store's doc id — the unit of incremental re-indexing.
    doc_id: Field,
    /// A message's id — the mail half's unit of incremental re-indexing. Its own
    /// field rather than `doc_id`: an entity's badge and a message's own id come
    /// from different tables and share no namespace, so one field would make
    /// them capable of evicting each other.
    message_id: Field,
    /// The entity kind this document is filed under.
    kind: Field,
    /// A fact's subject handle.
    subject: Field,
    /// A fact's lifecycle state.
    status: Field,
    /// A fact's provenance.
    provenance: Field,
    /// A fact's standing — the other certainty axis.
    standing: Field,
    /// A fact's edge shape, and the handle its edge points at.
    edge_shape: Field,
    edge_object: Field,
    /// **One term per edge, shape and object together.**
    ///
    /// A fact can carry several edges now — its own, plus one per entity its
    /// fields point at — and the two fields above are independent, so
    /// `shape=location AND object=person:alpha` would match a fact holding a
    /// location edge to somewhere else and an unrelated link to alpha. Neither
    /// field is wrong; the pair is what the caller actually asked about.
    edge_pair: Field,
    /// **One term per key a fact carries — and, on an entity document, per key
    /// the THING carries across its records.**
    ///
    /// A type is a set of key names and a thing answers it by carrying at least
    /// one of them, so the question is set membership over strings — which is
    /// what a term index is. The values are deliberately not here: selecting
    /// asks which keys are held, never what is in them, and saying HOW a thing
    /// answers is done from the mirror where the values live.
    meta_key: Field,
    /// A session's own id — its own namespace again, for the same reason
    /// `message_id` is not `doc_id`: three stores, three id spaces, and one
    /// shared field would let a page id evict a run.
    session_id: Field,
    /// **One journal entry's own id** — its own namespace again, for the same
    /// reason `session_id` is not `doc_id`. This is what lets an eviction
    /// target one beat instead of a whole run: without it, the only term a
    /// beat's document could be deleted by was the run it belonged to, so
    /// clearing one beat meant clearing all of them.
    entry_id: Field,
    /// **The bot a session belongs to**, and the only field any query scopes an
    /// answer by. Entities, facts and prose are the operator's and carry none.
    owner: Field,
    /// The stored [`Payload`], as JSON.
    payload: Field,
}

impl Fields {
    fn build() -> (Schema, Self) {
        let mut b = Schema::builder();
        let fields = Fields {
            class: b.add_text_field("class", STRING),
            // **The two tokenized fields, and the only ones that stem.** Every
            // other field here is `STRING` — stored and matched whole — so a
            // handle, a kind and a status are untouched by any of this.
            text: b.add_text_field(
                "text",
                tantivy::schema::TextOptions::default().set_indexing_options(
                    tantivy::schema::TextFieldIndexing::default()
                        .set_tokenizer(ANALYZER)
                        .set_index_option(IndexRecordOption::WithFreqs),
                ),
            ),
            history_text: b.add_text_field(
                "history_text",
                tantivy::schema::TextOptions::default().set_indexing_options(
                    tantivy::schema::TextFieldIndexing::default()
                        .set_tokenizer(ANALYZER)
                        .set_index_option(IndexRecordOption::WithFreqs),
                ),
            ),
            doc_id: b.add_text_field("doc_id", STRING),
            message_id: b.add_text_field("message_id", STRING),
            kind: b.add_text_field("kind", STRING),
            subject: b.add_text_field("subject", STRING),
            status: b.add_text_field("status", STRING),
            provenance: b.add_text_field("provenance", STRING),
            standing: b.add_text_field("standing", STRING),
            edge_shape: b.add_text_field("edge_shape", STRING),
            edge_object: b.add_text_field("edge_object", STRING),
            edge_pair: b.add_text_field("edge_pair", STRING),
            meta_key: b.add_text_field("meta_key", STRING),
            session_id: b.add_text_field("session_id", STRING),
            entry_id: b.add_text_field("entry_id", STRING),
            owner: b.add_text_field("owner", STRING),
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

/// **When a reading of the store began**, as a position in the sequence of
/// behind marks.
///
/// A whole-corpus reading ends by saying "nothing is behind any more". That is
/// only ever true of the store **as the reading found it**, and a reading is
/// I/O: a write can commit, fail its own re-read and mark the index behind while
/// the reading is still in flight. Clearing every mark on arrival then vouches
/// for a write the reading never saw.
///
/// So the point is taken **before** the read and handed to the ingest after it,
/// which clears only the marks that were already there. A mark made in between
/// survives — the reading cannot speak for it either way, and over-reporting
/// `Stale` costs an answer that hedges, while under-reporting it is the answer
/// that vouches for a document the index does not hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ReadingPoint(u64);

/// **Test-only fault injection for the write path.** Every knob defaults to
/// inert, and a production `FullTextIndex` never arms one — nothing outside
/// `#[cfg(any(test, feature = "testing"))]` code can reach the setters below.
/// `search()` never reads this: the fields it touches are `index` and
/// `reader`, and this struct is checked only from the write path
/// (`write_message`, and `ingest_mail_changes`'s own commit and reload),
/// which is what lets a real caller's real staging-then-commit order run
/// under an injected failure, rather than a stand-in replacing the whole
/// function.
#[derive(Debug, Default)]
struct WriteFault {
    /// Fail the Nth call to `add_document` (0-indexed) rather than let it
    /// reach tantivy. `None`: never fail.
    fail_add_document_at: RwLock<Option<usize>>,
    add_document_calls: std::sync::atomic::AtomicUsize,
    /// Fail the next `commit`, once, then go inert again.
    fail_commit: std::sync::atomic::AtomicBool,
    /// Fail the next `reload`, once, then go inert again.
    fail_reload: std::sync::atomic::AtomicBool,
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
    ///
    /// **Each mark carries when it was made**, counted in
    /// [`mark_seq`](Self::mark_seq). A whole-corpus reading clears the marks its
    /// own reading covered and no others, so a mark made while that reading was
    /// in flight survives it — see [`ReadingPoint`].
    behind: RwLock<std::collections::BTreeMap<EntityId, u64>>,
    /// Counts the marks made, so each one can be placed in time against a
    /// reading of the store. Only ever compared, never displayed.
    mark_seq: std::sync::atomic::AtomicU64,
    /// Whether the last whole-corpus refresh failed to reach the store.
    ///
    /// The [`behind`](Self::behind) set answers the same question about one
    /// entity, and it cannot answer it about the corpus: a refresh of everything
    /// that never reached the store learned no handles, so there is nothing to
    /// put in a set keyed by them. Both mean the index holds an older version
    /// than the store, so both read out as the same word to a caller.
    ///
    /// **Stamped in [`mark_seq`](Self::mark_seq), like a behind mark, and zero
    /// when there is none.** A reading of the whole corpus clears it only when
    /// its own reading began after the failure — a bare flag was cleared by any
    /// reading that completed, so one already in flight landed afterwards and
    /// vouched for a store it had never seen in that state.
    memory_refresh_failed_at: std::sync::atomic::AtomicU64,
    /// The messages these postings were written from — the mail half's mirror,
    /// and the same thing [`docs`](Self::docs) is for the memory half: what a
    /// later board read is compared against so a refresh writes only where the
    /// store has moved.
    messages: RwLock<Vec<Message>>,
    /// Whether the last board read failed to reach the store. The mail half's
    /// [`memory_refresh_failed_at`](Self::memory_refresh_failed_at), for the same
    /// reason and read out as the same word.
    ///
    /// **Stamped in [`mark_seq`](Self::mark_seq) and cleared by comparison**,
    /// exactly as [`memory_refresh_failed_at`](Self::memory_refresh_failed_at)
    /// is: without it, a board read already in flight when another fails lands
    /// afterwards and clears a failure its own snapshot predates.
    mail_refresh_failed_at: std::sync::atomic::AtomicU64,
    /// The runs these postings were written from — the session half's mirror,
    /// and the same thing [`docs`](Self::docs) and [`messages`](Self::messages)
    /// are for the other two halves. This half is refreshed before every
    /// answer, so what it is compared against decides whether an ordinary
    /// search commits the shared index or touches nothing.
    sessions: RwLock<Vec<jojobot_domain::session::Session>>,
    /// How many session-entry documents [`ingest_sessions`](Self::ingest_sessions)
    /// has written, test-only. Counts a write regardless of whether the entry
    /// was new or a replacement — see [`session_entries_deleted`](Self::session_entries_deleted)
    /// for the other half of that distinction.
    session_entries_written: std::sync::atomic::AtomicUsize,
    /// How many session-entry documents [`ingest_sessions`](Self::ingest_sessions)
    /// has deleted, test-only — one entry's own document replaced or removed
    /// counts once; a run evicted outright counts once per entry it held. What
    /// proves the eviction is scoped to what changed rather than to the whole
    /// run: siblings of a changed entry must not move this number.
    session_entries_deleted: std::sync::atomic::AtomicUsize,
    /// Whether the sessions half has ever been loaded from a real read of the
    /// store — [`mail_loaded`](Self::mail_loaded)'s question, asked of the
    /// third store. This half has no separate "touched" state: nothing writes
    /// a session document except [`ingest_sessions`](Self::ingest_sessions)
    /// itself, so there is no route to "partly filled by writes, never read
    /// whole" the way mail and memory each have one.
    session_loaded: std::sync::atomic::AtomicBool,
    /// Whether the last read behind the sessions half failed to reach the
    /// store — [`mail_refresh_failed_at`](Self::mail_refresh_failed_at)'s
    /// question, asked of the third store, and cleared the same way: stamped
    /// in [`mark_seq`](Self::mark_seq), zero when there is none, and cleared
    /// only by a reading whose own [`ReadingPoint`] began after the failure
    /// landed — never by one already in flight when it happened.
    session_refresh_failed_at: std::sync::atomic::AtomicU64,
    /// See [`WriteFault`]. Inert in every production index.
    write_fault: WriteFault,
}

impl FullTextIndex {
    /// An empty index, ready to be filled by a scan.
    pub fn open() -> Result<Self, MemoryError> {
        let (schema, fields) = Fields::build();
        let index = Index::create_in_ram(schema);
        // **Registered before anything is written**, because a document indexed
        // under one analyzer and queried under another matches nothing, and the
        // two would drift apart silently.
        index.tokenizers().register(
            ANALYZER,
            tantivy::tokenizer::TextAnalyzer::builder(
                tantivy::tokenizer::SimpleTokenizer::default(),
            )
            .filter(tantivy::tokenizer::RemoveLongFilter::limit(40))
            .filter(tantivy::tokenizer::LowerCaser)
            .filter(tantivy::tokenizer::Stemmer::new(
                tantivy::tokenizer::Language::English,
            ))
            .build(),
        );
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
            behind: RwLock::new(std::collections::BTreeMap::new()),
            mark_seq: std::sync::atomic::AtomicU64::new(0),
            memory_refresh_failed_at: std::sync::atomic::AtomicU64::new(0),
            messages: RwLock::new(Vec::new()),
            mail_refresh_failed_at: std::sync::atomic::AtomicU64::new(0),
            sessions: RwLock::new(Vec::new()),
            session_entries_written: std::sync::atomic::AtomicUsize::new(0),
            session_entries_deleted: std::sync::atomic::AtomicUsize::new(0),
            session_loaded: std::sync::atomic::AtomicBool::new(false),
            session_refresh_failed_at: std::sync::atomic::AtomicU64::new(0),
            write_fault: WriteFault::default(),
        })
    }

    /// How many session-entry documents have been written. Test-only.
    #[cfg(any(test, feature = "testing"))]
    pub fn session_entries_written(&self) -> usize {
        self.session_entries_written
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// How many session-entry documents have been deleted. Test-only.
    #[cfg(any(test, feature = "testing"))]
    pub fn session_entries_deleted(&self) -> usize {
        self.session_entries_deleted
            .load(std::sync::atomic::Ordering::Acquire)
    }

    /// **Fail the Nth call to `add_document` (0-indexed), instead of letting
    /// it reach tantivy.** Test-only — reached only from a test or the
    /// `testing` feature, never from production code. Checked from inside
    /// `write_message`, on the real write path, so a real caller's real
    /// staging-then-commit order runs under the injected failure.
    #[cfg(any(test, feature = "testing"))]
    pub fn fail_add_document_at(&self, n: usize) {
        *self
            .write_fault
            .fail_add_document_at
            .write()
            .expect("write fault poisoned") = Some(n);
    }

    /// **Fail the next `commit`, once.** Test-only, see
    /// [`fail_add_document_at`](Self::fail_add_document_at).
    #[cfg(any(test, feature = "testing"))]
    pub fn fail_next_commit(&self) {
        self.write_fault
            .fail_commit
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// **Fail the next `reload`, once.** Test-only, see
    /// [`fail_add_document_at`](Self::fail_add_document_at).
    #[cfg(any(test, feature = "testing"))]
    pub fn fail_next_reload(&self) {
        self.write_fault
            .fail_reload
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// Record that the board read behind this answer could not reach the store,
    /// so the mail half is a version behind. Cleared by the next board read that
    /// lands.
    pub fn mail_refresh_failed(&self) {
        let at = self
            .mark_seq
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.mail_refresh_failed_at
            .store(at, std::sync::atomic::Ordering::Release);
    }

    /// Record that the refresh this answer should have been built on could not
    /// reach the store, so the whole memory half is a version behind. Cleared by
    /// the next [`ingest_all`](Self::ingest_all), which is the only thing that
    /// can clear it: nothing smaller than a full scan knows the corpus is whole.
    ///
    /// **The failure is stamped as it goes on**, the same way a behind mark is,
    /// so a reading that began earlier cannot clear it. See [`ReadingPoint`].
    pub fn refresh_failed(&self) {
        let at = self
            .mark_seq
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.memory_refresh_failed_at
            .store(at, std::sync::atomic::Ordering::Release);
    }

    /// Record that the read behind this answer could not reach the sessions
    /// store, so the sessions half is serving whatever it last read.
    ///
    /// **Stamped in [`mark_seq`](Self::mark_seq), like the other two halves'
    /// own failure marks** — see [`ingest_sessions_changes`](Self::ingest_sessions_changes)
    /// for how that is what lets a reading already in flight survive a
    /// failure that lands after it, rather than being wiped out by a success
    /// that started before the failure and knows nothing of it.
    pub fn session_refresh_failed(&self) {
        let at = self
            .mark_seq
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.session_refresh_failed_at
            .store(at, std::sync::atomic::Ordering::Release);
    }

    /// Record that this entity's document is indexed as it stands in the store.
    pub fn current(&self, entity: &EntityId) {
        self.behind.write().expect("behind poisoned").remove(entity);
    }

    /// Record that the index holds an older version of this entity's document
    /// than the store does — a refresh after a committed write that could not
    /// run. Cleared by the next whole-corpus reading that covers it.
    ///
    /// **The mark is stamped as it goes on**, so a reading that started earlier
    /// cannot clear it. See [`ReadingPoint`].
    pub fn behind(&self, entity: &EntityId) {
        let at = self
            .mark_seq
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel)
            + 1;
        self.behind
            .write()
            .expect("behind poisoned")
            .insert(entity.clone(), at);
    }

    /// **Where the mark sequence stands right now.** Take this BEFORE reading the
    /// store and hand it to the ingest that follows, so the ingest clears what
    /// its own reading covered and nothing newer.
    pub fn reading_begins(&self) -> ReadingPoint {
        ReadingPoint(self.mark_seq.load(std::sync::atomic::Ordering::Acquire))
    }

    /// Whether a whole-corpus refresh has failed and nothing has cleared it
    /// yet.
    ///
    /// **A caller about to skip a real read checks this first.** Only a real
    /// [`ingest_all`](Self::ingest_all) clears the mark, so skipping while one
    /// is on record would leave it stuck — every later answer reporting the
    /// memory half stale even after the store is reachable again and nothing
    /// has changed on it.
    pub(crate) fn memory_refresh_pending(&self) -> bool {
        self.memory_refresh_failed_at
            .load(std::sync::atomic::Ordering::Acquire)
            != 0
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
            .keys()
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
    /// **Test-only, because it takes its own reading point.** A caller that
    /// has just read the board must take the point BEFORE that read and hand
    /// it to [`ingest_mail_changes`](Self::ingest_mail_changes); this one is
    /// handed its messages directly, so there is no read for a point to
    /// precede.
    #[cfg(test)]
    pub fn ingest_mail(&self, messages: &[Message]) -> Result<(), MemoryError> {
        let began = self.reading_begins();
        self.ingest_mail_changes(messages, began).map(|_| ())
    }

    /// Bring the mail half to what this board read says the board is, writing
    /// only where the two differ — the memory half's
    /// [`ingest_changes`](Self::ingest_changes), asked of the other store.
    ///
    /// A message the board read no longer carries leaves the index: removed by
    /// hand, or on a card that has become unreadable and so is absent from
    /// every board read. Both are the same absence and get the same answer.
    /// `began` is where the mark sequence stood **before** the board read that
    /// produced `messages` — see [`ReadingPoint`]. It is what lets this clear a
    /// failure it began after and leave one that happened while it was reading.
    ///
    /// **Assumes `messages` never carries one id twice, and does not check
    /// it** — the same assumption [`ingest_changes`](Self::ingest_changes)
    /// carries, on this half's own store. It holds today because every real
    /// caller builds `messages` from one plain board read —
    /// `Mailboxes::scan_messages`, one row per message id — never from
    /// concatenating two reads.
    pub fn ingest_mail_changes(
        &self,
        messages: &[Message],
        began: ReadingPoint,
    ) -> Result<usize, MemoryError> {
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
            // **No staged operation outlives a failed ingest** — see
            // [`Self::rollback_on_err`]. A delete staged above, or a document
            // written by an earlier iteration of this same loop, is
            // uncommitted tantivy state sitting in the shared writer, and
            // the next commit from ANYWHERE, mail or memory, would otherwise
            // apply it alongside whatever that unrelated call actually
            // meant to write.
            for message in &rewrite {
                let wrote = self.write_message(&writer, message);
                Self::rollback_on_err(&mut writer, wrote)?;
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
            self.commit_or_rollback(&mut writer)?;
            drop(writer);
            *self.messages.write().expect("mail mirror poisoned") = messages.to_vec();
        }
        // Again for the path that wrote nothing: the read still reached the
        // board, which is the only thing this flag reports.
        self.mail_loaded
            .store(true, std::sync::atomic::Ordering::Release);
        // The memory half's rule, on the other half's flag: this read clears a
        // failure it began after and leaves one that landed while it was in
        // flight. Exchanged against the value just read, so a failure arriving
        // in between is kept rather than overwritten.
        let failed_at = self
            .mail_refresh_failed_at
            .load(std::sync::atomic::Ordering::Acquire);
        if failed_at != 0 && failed_at <= began.0 {
            let _ = self.mail_refresh_failed_at.compare_exchange(
                failed_at,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            );
        }
        if changed > 0 {
            if self
                .write_fault
                .fail_reload
                .swap(false, std::sync::atomic::Ordering::AcqRel)
            {
                return Err(store_err("test fault: reload"));
            }
            self.reader.reload().map_err(store_err)?;
        }
        Ok(changed)
    }

    /// Re-index one message, replacing whatever was indexed under its id. What
    /// makes a posted message findable on the next call rather than after a
    /// restart — and what keeps a hit's `state` honest, since every verb that
    /// moves a message re-indexes it.
    ///
    /// **The mirror is written with the postings**, exactly as
    /// [`ingest_doc`](Self::ingest_doc) does for the other half. The mirror is
    /// what a later board read is compared against, so a message that reached
    /// the postings by this path and not the mirror is one the board can lose
    /// without the index noticing.
    pub fn ingest_message(&self, message: &Message) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(
            self.fields.message_id,
            message.id.as_str(),
        ));
        let wrote = self.write_message(&writer, message);
        Self::rollback_on_err(&mut writer, wrote)?;
        // Before the commit, for the reason `ingest_mail` sets its flag early.
        self.mail_touched
            .store(true, std::sync::atomic::Ordering::Release);
        self.commit_or_rollback(&mut writer)?;
        drop(writer);

        let mut mirror = self.messages.write().expect("mail mirror poisoned");
        mirror.retain(|m| m.id != message.id);
        mirror.push(message.clone());
        drop(mirror);
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// One message as the index holds it. **Everything on the envelope is
    /// searchable**, not only the body: the box and the sender are how a reader
    /// asks "what did the pm box say about the kiln" in one query, and a subject
    /// is a title precisely so it can be found by.
    /// **The one fault check every write in the index shares** —
    /// `write_message`, `write_doc` and `write_session_entry` all ask this
    /// before their own `add_document`, so the Nth call counted is the Nth
    /// document the index tries to add, whichever of the three is making
    /// it. Always inert outside a test that has armed it.
    fn should_fail_this_write(&self) -> bool {
        let called = self
            .write_fault
            .add_document_calls
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        *self
            .write_fault
            .fail_add_document_at
            .read()
            .expect("write fault poisoned")
            == Some(called)
    }

    /// **The one place every ingest rolls back.** If `result` is an error,
    /// the writer is rolled back to its last commit before the error is
    /// returned — so nothing this call staged before the failing write (a
    /// `delete_term`, a document an earlier iteration of its own loop
    /// already added) survives it to be picked up by a later, unrelated
    /// commit from anywhere else. `IndexWriter::rollback` replaces the
    /// writer in place (`*self = new_index_writer`), so the same instance
    /// stays usable for the caller's next real write.
    fn rollback_on_err<T>(
        writer: &mut IndexWriter,
        result: Result<T, MemoryError>,
    ) -> Result<T, MemoryError> {
        if result.is_err() {
            let _ = writer.rollback();
        }
        result
    }

    /// **The one place every ingest commits.** A commit that errors leaves
    /// its staged deletes and documents in the shared writer, so the writer
    /// is rolled back before the error returns — the same guarantee
    /// [`Self::rollback_on_err`] gives a staging error. The injected fault
    /// stands in for a failed `commit` and does NOT roll back itself, so a
    /// site that skipped this helper would leave its staging behind under
    /// test exactly as it would in production.
    fn commit_or_rollback(&self, writer: &mut IndexWriter) -> Result<(), MemoryError> {
        let committed = if self
            .write_fault
            .fail_commit
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            Err(store_err("test fault: commit"))
        } else {
            writer.commit().map(|_| ()).map_err(store_err)
        };
        Self::rollback_on_err(writer, committed)
    }

    fn write_message(&self, writer: &IndexWriter, message: &Message) -> Result<(), MemoryError> {
        if self.should_fail_this_write() {
            return Err(store_err("test fault: add_document"));
        }
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

    /// Write one beat of a session's chronology.
    ///
    /// **One document per entry, not per run.** A run is a list of beats a
    /// month apart; indexing it whole would make a hit point at the run and
    /// leave a reader to find the line, and would put a snippet around text
    /// that is mostly not what matched.
    fn write_session_entry(
        &self,
        writer: &IndexWriter,
        session: &jojobot_domain::session::Session,
        entry: &jojobot_domain::session::JournalEntry,
    ) -> Result<(), MemoryError> {
        if self.should_fail_this_write() {
            return Err(store_err("test fault: add_document"));
        }
        let f = &self.fields;
        writer
            .add_document(doc!(
                f.class => CLASS_SESSION,
                f.text => format!("{} {}", entry.text, session.focus),
                f.session_id => session.id.0.as_str(),
                f.entry_id => entry.id.0.as_str(),
                f.owner => session.bot.to_string(),
                f.payload => payload_json(&Payload::Session {
                    session: session.id.clone(),
                    bot: session.bot.clone(),
                    working_on: Some(session.focus.clone()).filter(|f| !f.is_empty()),
                    text: entry.text.clone(),
                })?,
            ))
            .map_err(store_err)?;
        self.session_entries_written
            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        Ok(())
    }

    /// Bring the **session** half of the index to what this read of the runs
    /// says, and return how many runs had to be written or evicted.
    ///
    /// Every bot's runs go in. **The scoping is at query time and nowhere
    /// else**: an index holding only the asker's runs would have to be rebuilt
    /// per caller, and the first bot to search would decide what the second one
    /// could find.
    ///
    /// **Incremental, for the reason [`ingest_changes`](Self::ingest_changes)
    /// is.** This half is refreshed before every answer, so an ingest that
    /// rewrote it wholesale would take the writer lock, commit the shared index
    /// and reload the reader once per search — work that grows with every run
    /// every bot has ever had rather than with what changed. When nothing
    /// moved, which is the ordinary case, no lock is taken and nothing is
    /// committed.
    ///
    /// The comparison is of two full readings, keyed by run: a run is rewritten
    /// when this reading and the mirror disagree about it, and evicted when the
    /// reading no longer holds it at all.
    ///
    /// **A rewritten run is not rewritten whole.** Which run changed is still
    /// decided by comparing the whole record — that is the detection cost, and
    /// it is unchanged here. What changed is what happens next: each of that
    /// run's own beats is compared against what it held before, by the beat's
    /// own id, so a run with one new beat writes one document rather than
    /// deleting and re-adding every beat it has ever had. A run evicted
    /// outright still goes by its own id — the whole run is gone, so nothing
    /// is left for a beat-level comparison to save.
    ///
    /// **Test-only, because it takes its own reading point.** A caller that
    /// has just read the sessions store must take the point BEFORE that read
    /// and hand it to
    /// [`ingest_sessions_changes`](Self::ingest_sessions_changes); this one is
    /// handed its sessions directly, so there is no read for a point to
    /// precede — [`ingest_mail`](Self::ingest_mail)'s own reason.
    #[cfg(test)]
    pub fn ingest_sessions(
        &self,
        sessions: &[jojobot_domain::session::Session],
    ) -> Result<usize, MemoryError> {
        let began = self.reading_begins();
        self.ingest_sessions_changes(sessions, began)
    }

    /// Bring the sessions half to what this read of the runs says, and return
    /// how many runs had to be written or evicted — [`ingest_sessions`]'s own
    /// work, given the [`ReadingPoint`] its caller took before reading the
    /// store, so a failure that lands while this reading is still in flight
    /// survives it rather than being cleared by a reading that never saw it.
    ///
    /// **Assumes `sessions` never carries one run id twice, and does not
    /// check it** — the same assumption [`ingest_changes`](Self::ingest_changes)
    /// carries, on this half's own store, and sharper here: each session in
    /// `rewrite` diffs its own beats against the mirror independently, so two
    /// entries sharing a run id would each re-add every beat they hold,
    /// including one the other already added — a search test in this module
    /// builds exactly that batch by hand to prove the read-side dedupe
    /// survives it. It holds in production because every real caller builds
    /// `sessions` from one plain read of the store — `Sessions::all_sessions`,
    /// one row per run id — never from concatenating two reads.
    pub fn ingest_sessions_changes(
        &self,
        sessions: &[jojobot_domain::session::Session],
        began: ReadingPoint,
    ) -> Result<usize, MemoryError> {
        use std::collections::{HashMap, HashSet};

        enum EntryChange {
            Add(jojobot_domain::session::JournalEntry),
            Replace(jojobot_domain::session::JournalEntry),
            Remove(String),
        }

        let (rewrite, evict, evicted_entry_counts, entry_changes) = {
            let mirror = self.sessions.read().expect("session mirror poisoned");
            let (rewrite, evict) = changed_and_gone(
                &sessions
                    .iter()
                    .map(|s| (s.id.0.as_str(), s))
                    .collect::<Vec<_>>(),
                &mirror
                    .iter()
                    .map(|s| (s.id.0.as_str(), s))
                    .collect::<Vec<_>>(),
            );
            let old_by_id: HashMap<&str, &jojobot_domain::session::Session> =
                mirror.iter().map(|s| (s.id.0.as_str(), s)).collect();
            let evicted_entry_counts: Vec<usize> = evict
                .iter()
                .map(|id| old_by_id.get(id.as_str()).map_or(0, |s| s.entries.len()))
                .collect();
            let entry_changes: Vec<(jojobot_domain::session::Session, Vec<EntryChange>)> = rewrite
                .iter()
                .map(|session| {
                    let old_entries: HashMap<&str, &jojobot_domain::session::JournalEntry> =
                        old_by_id
                            .get(session.id.0.as_str())
                            .map(|s| s.entries.iter().map(|e| (e.id.0.as_str(), e)).collect())
                            .unwrap_or_default();
                    let new_ids: HashSet<&str> =
                        session.entries.iter().map(|e| e.id.0.as_str()).collect();
                    let mut changes = Vec::new();
                    for entry in &session.entries {
                        match old_entries.get(entry.id.0.as_str()) {
                            Some(old) if **old == *entry => {}
                            Some(_) => changes.push(EntryChange::Replace(entry.clone())),
                            None => changes.push(EntryChange::Add(entry.clone())),
                        }
                    }
                    for id in old_entries.keys() {
                        if !new_ids.contains(id) {
                            changes.push(EntryChange::Remove((*id).to_string()));
                        }
                    }
                    (session.clone(), changes)
                })
                .collect();
            (rewrite, evict, evicted_entry_counts, entry_changes)
        };
        let changed = rewrite.len() + evict.len();
        // **Set whether or not anything changed**, the memory and mail halves'
        // own rule: reaching the store is what this reports, and a read that
        // found nothing new still reached it.
        self.session_loaded
            .store(true, std::sync::atomic::Ordering::Release);
        // The other two halves' own rule, on this half's flag: this reading
        // clears a failure it began after and leaves one that landed while it
        // was in flight. Exchanged against the value just read, so a failure
        // arriving in between is kept rather than overwritten.
        let failed_at = self
            .session_refresh_failed_at
            .load(std::sync::atomic::Ordering::Acquire);
        if failed_at != 0 && failed_at <= began.0 {
            let _ = self.session_refresh_failed_at.compare_exchange(
                failed_at,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            );
        }
        if changed == 0 {
            return Ok(0);
        }

        let mut writer = self.writer.write().expect("index writer poisoned");
        // **A whole run gone deletes by the run's own id**, the one term every
        // one of its beats carries — nothing survives a run's own eviction for
        // a beat-level comparison to save.
        for (id, held) in evict.iter().zip(evicted_entry_counts) {
            writer.delete_term(Term::from_field_text(self.fields.session_id, id));
            self.session_entries_deleted
                .fetch_add(held, std::sync::atomic::Ordering::AcqRel);
        }
        // **A changed run touches only the beats that moved**, by each beat's
        // own id — an unrelated sibling keeps the document it already has.
        for (session, changes) in &entry_changes {
            for change in changes {
                match change {
                    EntryChange::Add(entry) => {
                        let wrote = self.write_session_entry(&writer, session, entry);
                        Self::rollback_on_err(&mut writer, wrote)?;
                    }
                    EntryChange::Replace(entry) => {
                        writer.delete_term(Term::from_field_text(
                            self.fields.entry_id,
                            entry.id.0.as_str(),
                        ));
                        self.session_entries_deleted
                            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                        let wrote = self.write_session_entry(&writer, session, entry);
                        Self::rollback_on_err(&mut writer, wrote)?;
                    }
                    EntryChange::Remove(id) => {
                        writer.delete_term(Term::from_field_text(self.fields.entry_id, id));
                        self.session_entries_deleted
                            .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                    }
                }
            }
        }
        self.commit_or_rollback(&mut writer)?;
        drop(writer);
        *self.sessions.write().expect("session mirror poisoned") = sessions.to_vec();
        self.reader.reload().map_err(store_err)?;
        Ok(changed)
    }

    /// State that this scan is the whole memory corpus.
    ///
    /// **Scoped to the memory classes, never `delete_all_documents`.** The two
    /// halves come from two stores; wiping the whole index here evicted every
    /// message while leaving the flag saying mail was loaded, so a rebuild of
    /// Memory silently emptied `search`'s mail half and then vouched for it.
    /// Only the boot ordering in `main.rs` — untested, and no invariant —
    /// happened to hide it.
    ///
    /// **`scan` must never carry one doc id twice** — see
    /// [`ingest_changes`](Self::ingest_changes)'s own doc for why that holds
    /// today and what would break it.
    pub fn ingest_all(
        &self,
        scan: &[DocScan],
        began: ReadingPoint,
        history: &CorpusHistoryTerms,
    ) -> Result<(), MemoryError> {
        self.ingest_changes(scan, began, history).map(|_| ())
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
    ///
    /// **Assumes `scan` never carries one doc id twice, and does not check
    /// it.** `changed_and_gone` compares the whole list against the
    /// mirror, never against itself, so two entries sharing a doc id inside
    /// one call would both be written — the case this whole module's own
    /// `identity`-based read-side dedupe exists to survive, but at the write
    /// side nothing here would catch it. It holds today because every real
    /// caller builds `scan` from one plain read of the store — `Memory::scan`
    /// reads the `entity` table by its own primary key, one row per handle —
    /// never by concatenating two reads or two partial scans into one call.
    /// A caller that batched a catch-up read on top of an earlier one, rather
    /// than replacing it, would need this to check.
    pub fn ingest_changes(
        &self,
        scan: &[DocScan],
        began: ReadingPoint,
        history: &CorpusHistoryTerms,
    ) -> Result<usize, MemoryError> {
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
        let none = FactHistoryTerms::new();

        if changed > 0 {
            let mut writer = self.writer.write().expect("index writer poisoned");
            for doc_id in evict.iter().chain(rewrite.iter().map(|d| &d.doc_id)) {
                writer.delete_term(Term::from_field_text(self.fields.doc_id, doc_id));
            }
            for doc in &rewrite {
                let terms = doc
                    .entity
                    .as_ref()
                    .and_then(|e| history.get(&e.id))
                    .unwrap_or(&none);
                let wrote = self.write_doc(&writer, doc, terms);
                Self::rollback_on_err(&mut writer, wrote)?;
            }
            // Before the commit, for the reason `ingest_mail` sets its flag early.
            self.memory_loaded
                .store(true, std::sync::atomic::Ordering::Release);
            self.commit_or_rollback(&mut writer)?;
            drop(writer);
            *self.docs.write().expect("doc mirror poisoned") =
                scan.iter().map(DocMirror::of).collect();
        }
        // Again, for the path that wrote nothing: the scan still reached the
        // store, which is the only thing this flag reports.
        self.memory_loaded
            .store(true, std::sync::atomic::Ordering::Release);

        // This reading covered every doc in the store, so every mark that was
        // already on when it began is answered — a doc a failed refresh marked
        // before the restart or the rebuild that has just replaced it included.
        //
        // **Only those.** A mark made while this reading was in flight belongs
        // to a write this snapshot predates, and clearing it would report
        // complete coverage over a document the index does not hold. See
        // [`ReadingPoint`].
        self.behind
            .write()
            .expect("behind poisoned")
            .retain(|_, at| *at > began.0);
        // The same rule as the marks above, on the flag that speaks for the
        // whole corpus: this reading clears a failure it began after, and
        // leaves one that happened while it was in flight. The exchange is
        // against the value just read, so a failure landing in between is not
        // lost.
        let failed_at = self
            .memory_refresh_failed_at
            .load(std::sync::atomic::Ordering::Acquire);
        if failed_at != 0 && failed_at <= began.0 {
            let _ = self.memory_refresh_failed_at.compare_exchange(
                failed_at,
                0,
                std::sync::atomic::Ordering::AcqRel,
                std::sync::atomic::Ordering::Acquire,
            );
        }
        if changed > 0 {
            self.reader.reload().map_err(store_err)?;
        }
        Ok(changed)
    }

    /// Re-index one document, replacing everything previously indexed under its
    /// doc id. Called with a fresh scan of the doc, never with a guess at what
    /// changed.
    pub fn ingest_doc(&self, doc: &DocScan, history: &FactHistoryTerms) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(self.fields.doc_id, &doc.doc_id));
        let wrote = self.write_doc(&writer, doc, history);
        Self::rollback_on_err(&mut writer, wrote)?;
        // Before the commit, for the reason `ingest_mail` sets its flag early.
        self.memory_touched
            .store(true, std::sync::atomic::Ordering::Release);
        self.commit_or_rollback(&mut writer)?;
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
    /// handle matches nothing in a store whose doc id is not the handle: the
    /// page goes from the store and every hit it ever had goes on being served
    /// from the last scan, indefinitely.
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
        self.commit_or_rollback(&mut writer)?;
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
    ///
    /// `history` carries this doc's facts' earlier wordings, when the caller
    /// fetched any — empty for a scan that skipped it (a hand-built fixture,
    /// most tests) or found none.
    fn write_doc(
        &self,
        writer: &IndexWriter,
        scan: &DocScan,
        history: &FactHistoryTerms,
    ) -> Result<(), MemoryError> {
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

        // **Out of the ordinary browse, mirroring `list_entities`'s own
        // default read.** An archived entity gets no ENTITY-class document,
        // so a query can never surface it — the same broad-door/direct-door
        // split its own field declares. Its facts are indexed regardless,
        // below: archiving is the SUBJECT's state, never a claim about it.
        if let Some(entity) = scan.entity.as_ref().filter(|e| e.browsable()) {
            let mut document = doc!(
                f.class => CLASS_ENTITY,
                f.text => format!("{} {} {}", entity.id, owner_labels, entity.kind),
                f.doc_id => scan.doc_id.clone(),
                f.kind => entity.kind.as_token(),
                f.payload => payload_json(&Payload::Entity {
                    entity: entity.clone(),
                    doc_id: scan.doc_id.clone(),
                })?,
            );
            // **Every key this THING carries**, so a type filter is a clause
            // over things rather than a pass over a ranked page. A thing is
            // described a piece at a time, so the keys that answer a type are
            // spread across its rows and no one row's postings can stand in for
            // them.
            //
            // Taken from what the scan says the thing holds, never folded from
            // the rows beside it: a key the store has taken off the thing must
            // stop being a posting, and a row still carrying it does not say so.
            for key in scan.fields.keys() {
                document.add_text(f.meta_key, key.trim());
            }
            if self.should_fail_this_write() {
                return Err(store_err("test fault: add_document"));
            }
            writer.add_document(document).map_err(store_err)?;
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
                f.standing => fact.standing.as_token(),
                f.payload => payload_json(&Payload::Fact { fact: fact.clone() })?,
            );
            // **Only when there is one.** An empty field and an absent one are
            // the same to a query that never asks for `history_text`, and
            // writing one on every fact would cost every reindex for a field
            // most documents never carry anything in.
            if let Some(terms) = history.get(&fact.id) {
                document.add_text(f.history_text, terms);
            }
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
            // **Every edge this fact draws**, its own and the ones its fields
            // point with. A field's link is a `connection`: the pointer is
            // real, and what the link MEANS is deliberately unrecorded rather
            // than guessed.
            //
            // What makes a field value one of these is that it IS a handle, not
            // the key it sits under — the unnamed list is one case and a named
            // key is the same link annotated. Keying this on the unnamed list
            // alone would drop every named reference.
            let linked = fact
                .linked()
                .into_iter()
                .map(|object| Edge::new(EdgeShape::Connection, object));
            for edge in fact.edge.iter().cloned().chain(linked) {
                document.add_text(f.edge_shape, edge.shape.as_token());
                document.add_text(f.edge_object, edge.object.as_str());
                document.add_text(
                    f.edge_pair,
                    format!("{}={}", edge.shape.as_token(), edge.object),
                );
            }
            // **Every key the record carries, so a type filter is a clause.**
            // A type is answered by the keys a record holds, and holding one is
            // the whole of the match — so the question the caller asks is
            // answerable from postings, and asking it here means the answer is
            // drawn from the whole corpus rather than from the page the limit
            // happened to buy.
            for key in fact.fields.keys() {
                document.add_text(f.meta_key, key.trim());
            }
            if self.should_fail_this_write() {
                return Err(store_err("test fault: add_document"));
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
            if self.should_fail_this_write() {
                return Err(store_err("test fault: add_document"));
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
        let Some(mut analyzer) = self.index.tokenizers().get(ANALYZER) else {
            return Vec::new();
        };
        let mut tokens = Vec::new();
        let mut stream = analyzer.token_stream(text);
        while stream.advance() {
            tokens.push(stream.token().text.clone());
        }
        tokens
    }

    /// **Most of the query's terms, not all of them** — one clause, so the
    /// looseness stays inside the text half and the filters beside it stay
    /// absolute.
    ///
    /// Every term had to appear. That is right for a phrase somebody quotes and
    /// wrong for the way a question is actually typed: a caller writing two
    /// words they half-remember got nothing at all, because no one document
    /// held both. **Nothing is worse than something ranked low** — the ranking
    /// already puts a document matching every term above one matching half.
    ///
    /// **A majority, rounded up**, so one term still means that term and two
    /// unrelated words do not pull in the whole corpus. ⚠️ **This is the other
    /// half of what makes matching approximate**, and it is why an answer says
    /// a miss is possible.
    fn text_clauses(&self, query: &SearchQuery) -> Vec<(Occur, Box<dyn Query>)> {
        let Some(text) = query.terms() else {
            return Vec::new();
        };
        let terms = self.terms_of(text);
        if terms.is_empty() {
            return Vec::new();
        }
        // 🚨 **A handle is an address, not a description, so it stays
        // strict.** `kind:slug` tokenizes to two terms and the KIND half is
        // shared by every entity of that kind — so a majority of two would let
        // a handle nobody holds match every person in the store. Measured: it
        // did, and the case below is what caught it.
        let wanted = if EntityId(text.trim().to_string()).kind().is_some() {
            terms.len()
        } else {
            terms.len().div_ceil(2)
        };
        let any_of: Vec<Box<dyn Query>> = terms
            .into_iter()
            .map(|term| self.term_in_text_or_history(&term, query.include_history))
            .collect();
        let q: Box<dyn Query> = Box::new(BooleanQuery::with_minimum_required_clauses(
            any_of.into_iter().map(|q| (Occur::Should, q)).collect(),
            wanted,
        ));
        vec![(Occur::Must, q)]
    }

    /// **One term's clause — the current wording alone, or widened to a
    /// fact's earlier wordings too.**
    ///
    /// The widening is an OR on the term, not a second pass: a document
    /// satisfies this one term by matching it in `text` OR in
    /// `history_text`, and the surrounding minimum-required-clauses count in
    /// [`text_clauses`](Self::text_clauses) is unaffected — this only
    /// changes where ONE term is allowed to be found, never how many have
    /// to be.
    fn term_in_text_or_history(&self, term: &str, include_history: bool) -> Box<dyn Query> {
        let current: Box<dyn Query> = Box::new(TermQuery::new(
            Term::from_field_text(self.fields.text, term),
            IndexRecordOption::WithFreqs,
        ));
        if !include_history {
            return current;
        }
        let historical: Box<dyn Query> = Box::new(TermQuery::new(
            Term::from_field_text(self.fields.history_text, term),
            IndexRecordOption::WithFreqs,
        ));
        Box::new(BooleanQuery::new(vec![
            (Occur::Should, current),
            (Occur::Should, historical),
        ]))
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
        // The default is the whole point of the field: an archived fact stays
        // out of an ordinary search, and `status: archived` is how it is
        // reached deliberately.
        clauses.push(self.must_term(f.status, query.status.unwrap_or_default().as_token()));
        if let Some(kind) = query.kind {
            clauses.push(self.must_term(f.kind, kind.as_token()));
        }
        if let Some(provenance) = query.provenance {
            clauses.push(self.must_term(f.provenance, provenance.as_token()));
        }
        if let Some(standing) = query.standing {
            clauses.push(self.must_term(f.standing, standing.as_token()));
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

    /// **The type filter, as a clause.** A thing answers a type by carrying at
    /// least one of its keys, so the clause is a disjunction over the key names
    /// and the index narrows to the things that answer before the depth cut
    /// ever applies. Filtering the ranked page instead would drop every answer
    /// that did not happen to rank in the top few — and for a type-only query
    /// nothing ranks, so which things survived would be an arbitrary tie-break
    /// reported as "nothing answers this type".
    ///
    /// **The strict question selects on every key.** A thing fits a type only
    /// if it carries all of them, so the clause is a conjunction and the depth
    /// cut keeps the best of the things that can fit rather than the best of
    /// everything carrying one key. What the clause cannot see is a key held
    /// with a value of the wrong kind; [`whole`] still drops those after the
    /// cut.
    fn type_clause(&self, declared: &DeclaredType, strictly: bool) -> (Occur, Box<dyn Query>) {
        let occur = if strictly { Occur::Must } else { Occur::Should };
        let keys: Vec<(Occur, Box<dyn Query>)> = declared
            .fields
            .iter()
            .map(|field| {
                let q: Box<dyn Query> = Box::new(TermQuery::new(
                    Term::from_field_text(self.fields.meta_key, field.key.trim()),
                    IndexRecordOption::Basic,
                ));
                (occur, q)
            })
            .collect();
        (Occur::Must, Box::new(BooleanQuery::new(keys)))
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
        let mut classes: Vec<(Occur, Box<dyn Query>)> = classes
            .into_iter()
            .map(|class| {
                let q: Box<dyn Query> = Box::new(TermQuery::new(
                    Term::from_field_text(f.class, class),
                    IndexRecordOption::Basic,
                ));
                (Occur::Should, q)
            })
            .collect();
        // **Sessions come in only for a caller that says who it is, and only
        // that caller's own.** The owner term is MUST *inside* this class's
        // clause rather than beside it: a global one would demand an owner of
        // entities and prose, which carry none, and quietly empty the answer.
        //
        // No `asked_by` means no session hit — not every session. A caller
        // with no identity is not everybody.
        if let Some(asker) = query.asked_by.as_ref() {
            let mine: Box<dyn Query> = Box::new(BooleanQuery::new(vec![
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(f.class, CLASS_SESSION),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                ),
                (
                    Occur::Must,
                    Box::new(TermQuery::new(
                        Term::from_field_text(f.owner, &asker.to_string()),
                        IndexRecordOption::Basic,
                    )) as Box<dyn Query>,
                ),
            ]));
            classes.push((Occur::Should, mine));
        }
        clauses.push((Occur::Must, Box::new(BooleanQuery::new(classes))));
        if let Some(kind) = query.kind {
            // Prose in a doc that is nobody's entity has no kind, so a kind
            // filter excludes it — asking for one kind is asking about entities.
            clauses.push(self.must_term(f.kind, kind.as_token()));
        }
        // **A type narrows this half**, because a type is answered by a thing.
        // Prose and messages carry no keys, so the same clause that selects the
        // things that answer excludes them — which is the honest answer to "is
        // a message one of my services".
        if let Some((declared, strictly)) = query.typed() {
            clauses.push(self.type_clause(declared, strictly));
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
            let payload: Payload = serde_json::from_str(raw).map_err(|e| {
                // **A parse that failed because this process loaded no kinds
                // is not a damaged index** (rule 68). Every stored record
                // carries a handle, so an empty set fails all of them at once,
                // and reporting that as the store breaking sends a caller to a
                // person for a fault a boot repairs.
                if kinds::all().is_empty() {
                    MemoryError::KindsNeverLoaded { attempted: None }
                } else {
                    MemoryError::Store(format!("indexed payload: {e}"))
                }
            })?;
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
        // **Pinning is a second path to the same `Hit::Entity` `write_doc`
        // guards on the way into the index** — this one reads the mirror
        // directly rather than a query, so the same archived-exclusion has
        // to be repeated here rather than inherited from there.
        let index: Vec<Entity> = mirror
            .iter()
            .filter_map(|d| d.entity.clone())
            .filter(Entity::browsable)
            .collect();
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
                        answers: None,
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
/// **What became of the claim a derivation was worked out from** — read off the
/// same mirror every other neighbourhood is assembled from, so a hit cannot
/// disagree with the answer it arrives in.
///
/// `None` for a claim derived from nothing: an absence, never a verdict.
///
/// ⚠️ **A source the mirror does not hold is [`SourceStanding::Unreadable`] and
/// never [`SourceStanding::Stands`]**. The store refuses a pointer at a claim
/// nobody wrote, so a source that cannot be found here means this half of the
/// index has not read it — and vouching for a record nobody read is the one
/// thing this marker exists to stop.
fn standing_of(mirror: &[DocMirror], source: Option<&FactAddress>) -> Option<SourceStanding> {
    let source = source?;
    Some(
        mirror
            .iter()
            .flat_map(|doc| doc.scanned.facts.iter())
            .find(|held| &held.address() == source)
            .map_or(SourceStanding::Unreadable, |held| match held.status {
                // **Archived is not standing**, whether the row was taken
                // back or replaced by a later claim — the row is kept so
                // references survive, but it is no longer current.
                FactStatus::Archived => SourceStanding::Archived,
                FactStatus::Active => SourceStanding::Stands,
            }),
    )
}

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
/// [`Retrieval`] does, which is why the port lives there and holds one
/// [`Refresh`] half per store. Leaving these as the index's own methods makes
/// that a compile error rather than a convention: nothing can be wired to the
/// bare projection and quietly serve whatever it last saw.
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
            (true, _) if self.mail_refresh_failed_at.load(Acquire) != 0 => {
                Coverage::Partial(Behind::Stale)
            }
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
        let behind_now = self.memory_refresh_failed_at.load(Acquire) != 0
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

    /// The sessions half's question, asked the simpler way: this half has
    /// only one route in and one route behind, so there is no `touched` case
    /// to fold in the way [`memory_coverage`](Self::memory_coverage) and
    /// [`mail_coverage`](Self::mail_coverage) each have one.
    pub fn session_coverage(&self) -> Coverage {
        use std::sync::atomic::Ordering::Acquire;
        match (
            self.session_loaded.load(Acquire),
            self.session_refresh_failed_at.load(Acquire) != 0,
        ) {
            (false, _) => Coverage::Unread,
            (true, true) => Coverage::Partial(Behind::Stale),
            (true, false) => Coverage::Loaded,
        }
    }

    pub fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        query.validate()?;
        let depth = candidate_depth(query.limit);

        // **A type query has no fact half.** The question is which THINGS
        // carry the keys, and a row is not a thing: returning its rows beside
        // the answer would crowd the things it asked about out of the limit.
        let mut scored = if query.is_thing_scoped() {
            Vec::new()
        } else {
            self.collect(self.fact_clauses(query), depth)?
        };
        if !query.is_fact_scoped() {
            scored.extend(self.collect(self.other_clauses(query), depth)?);
        }

        // **Which day a fact's own recency is measured on** — the operator's
        // ruling: `recorded_at` unless the caller asks to rank by
        // `happened_at` instead. A claim with no `happened_at` falls back to
        // `recorded_at`, the same day it would rank on by default.
        let rank_date = |fact: &Fact| -> Date {
            match query.rank_clock {
                RankClock::RecordedAt => fact.recorded_at,
                RankClock::HappenedAt => fact.happened_at.unwrap_or(fact.recorded_at),
            }
        };

        // Recency is measured against the newest fact in the candidate set, not a
        // clock: the domain stays clock-free, and the same corpus ranks the same
        // way tomorrow.
        let newest = scored
            .iter()
            .filter_map(|(_, p)| match p {
                Payload::Fact { fact } => Some(rank_date(fact)),
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
                    // **A derivation never speaks over its own source.** It is
                    // still an answer and still reachable — the demotion is a
                    // subtraction, so a gloss that is the only match still
                    // comes back.
                    (Payload::Fact { fact }, _) if fact.derived_from.is_some() => -DERIVED_DEMOTION,
                    (Payload::Fact { fact }, Some(newest)) => {
                        let age_days = (newest - rank_date(fact)).get_days().max(0) as f32;
                        RECENCY_WEIGHT / (1.0 + age_days / 365.25)
                    }
                    // **A session ranks below everything else it shares an
                    // answer with.** It is context rather than an answer:
                    // reachable when it is what you are looking for, never
                    // crowding out what a search is usually for. The demotion
                    // is a subtraction rather than a filter, so a session that
                    // is the only match still comes back.
                    (Payload::Session { .. }, _) => -SESSION_DEMOTION,
                    _ => 0.0,
                };
                let hit = payload.into_hit(&terms, &mirror);
                (score + boost, tiebreak(&hit), hit)
            })
            .collect();
        // Deterministic to the last position: score, then a stable key. Two
        // sessions asking the same question see the same list in the same order.
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        // **How each thing answers, written onto the hit.** The selecting was
        // done by the clause in `other_clauses`, over the whole corpus; this
        // says how, which is what the caller asked for and which only the
        // mirror can answer. Nothing is expected to be dropped here — a thing
        // reaching this point carries a declared key — and the filter stays a
        // filter so that one which somehow does not cannot arrive claiming an
        // answer it has not got.
        // **Selecting is one clause; which of the two questions was asked
        // decides what survives it.** The tolerant one keeps a thing carrying
        // some of the keys and says which it lacks; the strict one keeps only
        // the things with nothing lacking. Both say how the thing answered,
        // because a caller that asked the strict question still wants to see
        // what it got rather than a bare list.
        //
        // **Applied to every hit, whichever half it arrived on.** A pin is
        // selected by the text alone, so a type filter reaching the ranked half
        // alone is defeated by any caller who also types a name, and hands back
        // a thing that does not fit with no answer on it to say so.
        let mut governed = |hit: &mut Hit| match query.typed() {
            Some((declared, strictly)) => {
                answer_with(declared, hit, &mirror) && (!strictly || whole(hit))
            }
            None => true,
        };

        // **Pinned first, and filtered like anything else.** An exact naming
        // still leads the answer when it survives the question, and it carries
        // the same answer a ranked hit would, because the same step writes it.
        let mut hits = self.pinned(query, &mirror);
        hits.retain_mut(&mut governed);
        ranked.retain_mut(|(_, _, hit)| governed(hit));

        // **Deduped by identity, never by the whole hit.** The same thing can
        // reach the answer twice, once pinned and once ranked, and what hangs
        // off it — the type answer — is written after the pin was built. A
        // comparison that included it called two copies of one thing two
        // things.
        let mut seen: Vec<(&'static str, String)> = hits.iter().map(identity).collect();
        for (_, _, hit) in ranked {
            let id = identity(&hit);
            if !seen.contains(&id) {
                seen.push(id);
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
                // **Filled in later, by the one step that knows the type.**
                // Turning a payload into a hit does not know what was asked,
                // and a thing answers a type only in the context of a query
                // that named one.
                answers: None,
            },
            Payload::Fact { fact } => Hit::Fact {
                subject: resolve(mirror, &fact.subject),
                home: resolve(mirror, &fact.home),
                source: standing_of(mirror, fact.derived_from.as_ref()),
                fact: Box::new(fact),
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
            // A run's own beat. Like mail it resolves nothing: what surrounds
            // it is its own run, which it carries.
            Payload::Session {
                session,
                bot,
                working_on,
                text,
            } => Hit::Session {
                snippet: snippet(&text, terms),
                session,
                bot,
                working_on,
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
        Hit::Session { session, bot, .. } => format!("{bot}/{}", session.0),
    }
}

/// **Which thing a hit is about** — its class and what names it, and nothing
/// that was computed for this answer.
///
/// What makes two hits the same hit is that they name the same thing, not that
/// every field on them agrees: the type answer is written onto a hit after it
/// was built, so two copies of one entity can differ in it while still being
/// one entity. The class is in the key because the addresses are drawn from
/// different spaces — a doc id and a handle are free to collide, and two hits
/// of different classes are never the same hit.
///
/// **Deliberately not [`tiebreak`].** That is a sort key and only has to be
/// stable; a run's id is enough to order its beats and nowhere near enough to
/// tell them apart.
fn identity(hit: &Hit) -> (&'static str, String) {
    match hit {
        Hit::Entity { entity, .. } => (CLASS_ENTITY, entity.id.to_string()),
        Hit::Fact { fact, .. } => (CLASS_FACT, fact.address().to_string()),
        Hit::Prose { doc_id, .. } => (CLASS_PROSE, doc_id.clone()),
        Hit::Message { message, .. } => {
            (CLASS_MESSAGE, format!("{}/{}", message.mailbox, message.id))
        }
        // **A beat has no address of its own** — the id on it names the run,
        // and a run is many beats — so what tells two apart is what they say.
        // Keying on the run alone made a whole chronology read as one line.
        Hit::Session {
            session,
            bot,
            snippet,
            ..
        } => (CLASS_SESSION, format!("{bot}/{}/{snippet}", session.0)),
    }
}

/// **How far a session hit is pushed down the list.** Large enough that a
/// session loses to any other hit it shares an answer with, and a subtraction
/// rather than a filter so a run that is the only match is still reachable —
/// a rank so low it never surfaces would satisfy "lower priority" and defeat
/// the point of indexing sessions at all.
const SESSION_DEMOTION: f32 = 1_000.0;

/// **How far a derivation is pushed below the words it was worked out from.**
///
/// A gloss of somebody's own message once came back complete while the message
/// itself came back truncated, and the gloss went on ranking first months after
/// it had stopped being true. **A summary never outranks what it summarises.**
///
/// A subtraction rather than a filter, for the same reason the session demotion
/// is one: a derivation is a legitimate answer and a gloss that is the only
/// match must still be reachable. **Smaller than the session demotion**, so the
/// order is an answer, then a derivation, then a run — a derived claim is still
/// about the world, where a session is about the work.
const DERIVED_DEMOTION: f32 = 100.0;

/// **The one analyzer, named once.** Indexing and querying both read it by this
/// name: a document written under one analyzer and asked for under another
/// matches nothing, and nothing would report the mismatch.
///
/// It stems, so a question in the singular finds a claim in the plural. **That
/// makes matching approximate rather than exact, which is why an answer says a
/// miss is possible** — an empty answer must not read as *nobody ever said
/// this*.
const ANALYZER: &str = "jojobot";

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
/// through their home; what is wrong with them is that nobody can tell.
/// Surfacing the quarantine to the caller is later work — being able to see it
/// at all is the floor, and a scan that quietly normalizes a corruption is how
/// the corruption becomes permanent.
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
/// This is the shape a split brain arrives in: a retyped subject cell landing on
/// another live handle, so nothing is orphaned, every read goes on working, and
/// the entity is readable under one id and writable under the other. It is also,
/// routinely, nothing at all — a fact about one entity written
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

/// **Does this THING answer the type, and if so, say how — on the hit.**
///
/// Structural: nothing is ever asked what it was declared to be, only what its
/// records carry. A hit that is not an entity answers no type — prose and mail
/// are not things and have no fields to answer with.
///
/// **Read from the mirror rather than from the postings.** The index knows
/// which keys a thing carries, which is enough to select it; saying how it
/// answers needs the values too, and those live with the records the mirror
/// holds.
///
/// A thing carrying none of the type's keys is dropped, because that is not a
/// weak match, it is not a match. One carrying some is kept and says which it
/// lacks: hiding partial matches would hide exactly the things worth finding.
fn answer_with(declared: &DeclaredType, hit: &mut Hit, mirror: &[DocMirror]) -> bool {
    let Hit::Entity {
        entity, answers, ..
    } = hit
    else {
        return false;
    };
    match declared.matched_by(&fields_of(mirror, &entity.id)) {
        Some(found) => {
            *answers = Some(Box::new(found));
            true
        }
        None => false,
    }
}

/// **Does this hit answer its type with nothing lacking.**
///
/// Read off the answer the hit is already carrying rather than matched a second
/// time: two matchers over one question is how the filter and the report come
/// to disagree about the same thing.
fn whole(hit: &Hit) -> bool {
    match hit {
        Hit::Entity { answers, .. } => answers.as_deref().is_some_and(|found| found.complete()),
        _ => false,
    }
}

/// **What a thing holds, as the scan that mirrored its doc read it.**
///
/// The mirror keeps each doc's whole scan, so this is the store's own answer
/// rather than one folded a second time here — which is the only way it can be
/// had: the rows beside it have already projected away which write was newest
/// on the thing and which key was taken off it.
fn fields_of(mirror: &[DocMirror], id: &EntityId) -> BTreeMap<String, String> {
    mirror
        .iter()
        .find(|d| d.scanned.entity.as_ref().is_some_and(|e| &e.id == id))
        .map(|d| d.scanned.fields.clone())
        .unwrap_or_default()
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

    fn session_coverage(&self) -> Coverage {
        self.index.session_coverage()
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

    /// **The coverage the port reports, not the coverage the projection holds.**
    /// [`Retrieval`] is the object `main.rs` hands the MCP layer, so an
    /// assertion that reads [`FullTextIndex::memory_coverage`] straight off the
    /// projection holds identically on a port that answers something else
    /// entirely. The word a caller acts on is this one.
    fn memory_coverage_via_port(&self) -> Coverage;
}

#[cfg(test)]
#[async_trait]
impl SearchViaPort for Arc<IndexedMemory> {
    async fn search_via_port(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        Retrieval::new(self.index(), vec![self.clone()])
            .search(query)
            .await
    }

    fn memory_coverage_via_port(&self) -> Coverage {
        Retrieval::new(self.index(), vec![self.clone()]).memory_coverage()
    }
}

#[cfg(test)]
mod tests;
