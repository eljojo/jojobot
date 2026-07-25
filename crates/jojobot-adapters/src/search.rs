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

use jojobot_domain::memory::{
    Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch, Guarded, Memory,
    MemoryError, NewEntity, NewFact,
    guard::{self, MatchReason},
    search::{DocScan, Hit, Search, SearchQuery},
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
    Entity { entity: Entity, doc_id: String },
    Fact { fact: Fact },
    Prose {
        doc_id: String,
        title: String,
        entity: Option<EntityId>,
        body: String,
    },
}

/// The hit-class token, indexed so a query can ask for one class of thing.
const CLASS_ENTITY: &str = "entity";
const CLASS_FACT: &str = "fact";
const CLASS_PROSE: &str = "prose";

/// The index's fields. One schema for all three hit classes — a mixed ranked list
/// is the requirement, and one schema is what makes it one query.
struct Fields {
    /// Which class of thing this document is.
    class: Field,
    /// Everything searchable, tokenized: handles, names, claims, details, prose.
    text: Field,
    /// The store's doc id — the unit of incremental re-indexing.
    doc_id: Field,
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
            kind: b.add_text_field("kind", STRING),
            subject: b.add_text_field("subject", STRING),
            status: b.add_text_field("status", STRING),
            provenance: b.add_text_field("provenance", STRING),
            edge_shape: b.add_text_field("edge_shape", STRING),
            edge_object: b.add_text_field("edge_object", STRING),
            payload: b.add_text_field("payload", STORED),
        };
        (b.build(), fields)
    }
}

/// The in-RAM full-text index over entities, facts and prose.
pub struct FullTextIndex {
    index: Index,
    reader: IndexReader,
    fields: Fields,
    /// The writer is single-instance per index in tantivy, so it is held and
    /// shared rather than reopened per write.
    writer: RwLock<IndexWriter>,
    /// The entity list the **write guard's** matcher screens a query against, to
    /// decide which entity gets pinned. Kept beside the index because the guard
    /// takes entities, not postings — and reusing it is what keeps one definition
    /// of "the same thing" in the system.
    entities: RwLock<Vec<(Entity, String)>>,
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
            entities: RwLock::new(Vec::new()),
        })
    }

    /// Replace the whole index from a full scan — the boot path. A full re-scan
    /// rather than a delta: the corpus is dozens of docs, and a projection that
    /// can drift is worse than one that is rebuilt.
    pub fn ingest_all(&self, scan: &[DocScan]) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_all_documents().map_err(store_err)?;
        for doc in scan {
            self.write_doc(&writer, doc)?;
        }
        writer.commit().map_err(store_err)?;
        drop(writer);

        *self.entities.write().expect("entity mirror poisoned") = scan
            .iter()
            .filter_map(|d| d.entity.clone().map(|e| (e, d.doc_id.clone())))
            .collect();
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// Re-index one document, replacing everything previously indexed under its
    /// doc id. Called with a fresh scan of the doc, never with a guess at what
    /// changed.
    pub fn ingest_doc(&self, doc: &DocScan) -> Result<(), MemoryError> {
        let mut writer = self.writer.write().expect("index writer poisoned");
        writer.delete_term(Term::from_field_text(self.fields.doc_id, &doc.doc_id));
        self.write_doc(&writer, doc)?;
        writer.commit().map_err(store_err)?;
        drop(writer);

        let mut mirror = self.entities.write().expect("entity mirror poisoned");
        mirror.retain(|(_, id)| id != &doc.doc_id);
        if let Some(entity) = &doc.entity {
            mirror.push((entity.clone(), doc.doc_id.clone()));
        }
        drop(mirror);
        self.reader.reload().map_err(store_err)?;
        Ok(())
    }

    /// Every tantivy document one scanned doc produces: the entity it is, each
    /// fact in its table, and its prose — three classes, one index.
    fn write_doc(&self, writer: &IndexWriter, scan: &DocScan) -> Result<(), MemoryError> {
        let f = &self.fields;
        let owner_kind = scan.entity.as_ref().map(|e| e.kind);

        if let Some(entity) = &scan.entity {
            writer
                .add_document(doc!(
                    f.class => CLASS_ENTITY,
                    f.text => format!("{} {} {}", entity.id, entity.name, entity.kind),
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
            let mut document = doc!(
                f.class => CLASS_FACT,
                f.text => format!(
                    "{} {} {}",
                    fact.content,
                    fact.details.clone().unwrap_or_default(),
                    fact.subject
                ),
                f.doc_id => scan.doc_id.clone(),
                f.subject => fact.subject.to_string(),
                f.status => fact.status.as_token(),
                f.provenance => fact.provenance.as_token(),
                f.payload => payload_json(&Payload::Fact { fact: fact.clone() })?,
            );
            // A fact is filed under its SUBJECT's kind, not its home's: that is
            // what makes `kind=person` + an edge filter answer "which people".
            if let Some(kind) = fact.subject.kind() {
                document.add_text(f.kind, kind.as_token());
            }
            if let Some(edge) = &fact.edge {
                document.add_text(f.edge_shape, edge.shape.as_token());
                document.add_text(f.edge_object, edge.object.as_str());
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
        // The default is the whole point of the field: superseded and negated
        // facts stay out of an ordinary search, and `status: negated` is how the
        // anti-fact list is read.
        clauses.push(self.must_term(
            f.status,
            query.status.unwrap_or_default().as_token(),
        ));
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
            clauses.push(self.must_term(f.edge_object, edge.object.as_str()));
            if let Some(shape) = edge.shape {
                clauses.push(self.must_term(f.edge_shape, shape.as_token()));
            }
        }
        clauses
    }

    /// The clauses that select **entities and prose**. Run as a second query
    /// rather than folded into the first: a fact-only filter (a status, an edge)
    /// would otherwise exclude every non-fact hit as a side effect of the
    /// `MUST` it adds, which is not the same thing as the caller asking for facts.
    fn other_clauses(&self, query: &SearchQuery) -> Vec<(Occur, Box<dyn Query>)> {
        let f = &self.fields;
        let mut clauses = self.text_clauses(query);
        let classes: Vec<(Occur, Box<dyn Query>)> = [CLASS_ENTITY, CLASS_PROSE]
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
    fn pinned(&self, query: &SearchQuery) -> Vec<Hit> {
        let Some(text) = query.terms() else {
            return Vec::new();
        };
        // A fact-only filter says the caller wants facts; pinning an entity into
        // that answer would be noise.
        if query.is_fact_scoped() {
            return Vec::new();
        }
        let mirror = self.entities.read().expect("entity mirror poisoned");
        let index: Vec<Entity> = mirror.iter().map(|(e, _)| e.clone()).collect();
        let matches = guard::screen(&EntityId(text.to_string()), Some(text), &index);

        matches
            .into_iter()
            // Only a real naming of the entity pins it. A typo'd *name* inside a
            // longer query is a text match, not a claim about identity.
            .filter(|m| {
                matches!(
                    m.reason,
                    MatchReason::ExactHandle | MatchReason::SameName | MatchReason::SameNameOtherKind
                )
            })
            .filter(|m| query.kind.is_none_or(|k| k == m.kind))
            .filter_map(|m| {
                mirror
                    .iter()
                    .find(|(e, _)| e.id == m.handle)
                    .map(|(entity, doc_id)| Hit::Entity {
                        entity: entity.clone(),
                        doc_id: doc_id.clone(),
                    })
            })
            .collect()
    }
}

impl Search for FullTextIndex {
    fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
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
                let hit = payload.into_hit(&terms);
                (score + boost, tiebreak(&hit), hit)
            })
            .collect();
        // Deterministic to the last position: score, then a stable key. Two
        // sessions asking the same question see the same list in the same order.
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| a.1.cmp(&b.1)));

        let mut hits = self.pinned(query);
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
    fn into_hit(self, terms: &[String]) -> Hit {
        match self {
            Payload::Entity { entity, doc_id } => Hit::Entity { entity, doc_id },
            Payload::Fact { fact } => Hit::Fact { fact },
            Payload::Prose {
                doc_id,
                title,
                entity,
                body,
            } => Hit::Prose {
                doc_id,
                title,
                entity,
                snippet: snippet(&body, terms),
            },
        }
    }
}

/// The stable secondary sort key for a hit — its own address, so ordering never
/// depends on which segment tantivy happened to return first.
fn tiebreak(hit: &Hit) -> String {
    match hit {
        Hit::Entity { entity, .. } => entity.id.to_string(),
        Hit::Fact { fact } => fact.address().to_string(),
        Hit::Prose { doc_id, .. } => doc_id.clone(),
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
    serde_json::to_string(payload)
        .map_err(|e| MemoryError::Store(format!("indexing payload: {e}")))
}

fn store_err(e: impl std::fmt::Display) -> MemoryError {
    MemoryError::Store(format!("search index: {e}"))
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
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        let scan = self.inner.scan().await?;
        self.index.ingest_all(&scan)?;
        Ok(scan.len())
    }

    /// The index, for handing to whatever serves the `search` verb.
    pub fn index(&self) -> Arc<FullTextIndex> {
        self.index.clone()
    }

    /// Re-index one entity's doc by **re-reading it from the store**. A doc that
    /// has vanished is dropped from the index rather than left as a ghost.
    async fn reindex(&self, entity: &EntityId) -> Result<(), MemoryError> {
        match self.inner.scan_entity(entity).await? {
            Some(scan) => self.index.ingest_doc(&scan),
            None => self.index.ingest_doc(&DocScan {
                doc_id: entity.to_string(),
                title: String::new(),
                prose: String::new(),
                entity: None,
                facts: Vec::new(),
            }),
        }
    }
}

#[async_trait]
impl Memory for IndexedMemory {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        let written = self.inner.add_entity(new).await?;
        if let Guarded::Written(entity) = &written {
            self.reindex(&entity.id).await?;
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
            self.reindex(&entity.id).await?;
        }
        Ok(written)
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.capture(fact).await?;
        if let Guarded::Written(fact) = &written {
            self.reindex(&fact.home).await?;
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
            self.reindex(&fact.home).await?;
        }
        Ok(written)
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }

    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.inner.scan_entity(entity).await
    }
}

impl Search for IndexedMemory {
    fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError> {
        self.index.search(query)
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use jojobot_domain::memory::search::{EdgeFilter, DEFAULT_LIMIT};
    use jojobot_domain::memory::testing::{InMemoryMemory, contract};
    use jojobot_domain::memory::{
        Boot, Edge, EdgeShape, FactStatus, Provenance, validate_subject,
    };

    use super::*;

    /// The whole contract — Memory *and* retrieval — against the in-memory fake
    /// behind the index. The fast loop.
    #[tokio::test]
    async fn the_contract_holds_over_the_fake() {
        let store = IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens");
        contract::run_all_searchable(&store).await;
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
            source: "user-named".into(),
            crm: None,
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
            status: FactStatus::Active,
            date: on,
            edge: None,
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
            vec![fact("person:alpha", "f1", "plays go on Tuesdays", date(2026, 7, 1))],
        )]);

        let hits = index
            .search(&SearchQuery::text("penicillin"))
            .expect("search ok");
        let prose: Vec<&Hit> = hits
            .iter()
            .filter(|h| matches!(h, Hit::Prose { .. }))
            .collect();
        assert_eq!(prose.len(), 1, "the prose match must be a hit: {hits:?}");
        let Some(Hit::Prose { doc_id, entity: owner, snippet, .. }) = prose.first().copied() else {
            unreachable!("filtered to prose");
        };
        assert_eq!(doc_id, "doc-1", "a prose hit says which doc to open");
        assert_eq!(owner.as_ref(), Some(&alpha.id), "…and whose entity doc it is");
        assert!(
            snippet.to_lowercase().contains("penicillin"),
            "the snippet must carry the match: {snippet:?}"
        );

        // …and the same query, in one list, still reaches the fact and the entity.
        let mixed = index.search(&SearchQuery::text("alpha")).expect("search ok");
        assert!(mixed.iter().any(|h| matches!(h, Hit::Entity { .. })), "{mixed:?}");
        assert!(mixed.iter().any(|h| matches!(h, Hit::Prose { .. })), "{mixed:?}");
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
        let hits = index.search(&SearchQuery::text("pass closed")).expect("search ok");
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
        let hits = index.search(&SearchQuery::text("kayak trip")).expect("search ok");
        let ids: Vec<String> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { fact } => Some(fact.id.to_string()),
                _ => None,
            })
            .collect();
        assert_eq!(ids, vec!["f2", "f1"], "same words, same length — the newer one leads");
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
                fact("person:alpha", "f1", "bakes sourdough bread", date(2026, 1, 1)),
                fact("person:alpha", "f2", "bakes almond cake", date(2026, 1, 2)),
            ],
        )]);
        let hits = index
            .search(&SearchQuery::text("bakes sourdough"))
            .expect("search ok");
        let contents: Vec<String> = hits
            .iter()
            .filter_map(|h| match h {
                Hit::Fact { fact } => Some(fact.content.clone()),
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
        let guild = entity("org:guild", "Guild of the Northern Riverside Makers and Menders");
        let index = index_of(vec![scan(
            "doc-1",
            Some(guild.clone()),
            "",
            vec![
                fact("org:guild", "f1", "guild", date(2026, 1, 1)),
                fact("org:guild", "f2", "guild night", date(2026, 1, 2)),
            ],
        )]);

        let hits = index.search(&SearchQuery::text("org:guild")).expect("search ok");
        assert!(
            matches!(hits.first(), Some(Hit::Entity { entity, .. }) if entity.id == guild.id),
            "the named entity must lead, whatever the facts score: {hits:?}"
        );
        assert!(
            hits.iter().any(|h| matches!(h, Hit::Fact { .. })),
            "…and the facts are still in the same list: {hits:?}"
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

    /// A fact-only filter narrows to facts. Asking for "negated" and getting an
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
            .map(|n| fact("person:alpha", &format!("f{n}"), "repeated claim", date(2026, 1, 1)))
            .collect();
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            facts,
        )]);
        assert_eq!(
            index.search(&SearchQuery::text("repeated")).expect("search ok").len(),
            DEFAULT_LIMIT
        );
        assert_eq!(
            index
                .search(&SearchQuery { limit: 3, ..SearchQuery::text("repeated") })
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
            vec![fact("person:alpha", "f1", "keeps a ferret", date(2026, 1, 1))],
        );
        index.ingest_all(&[before]).expect("ingest");
        assert_eq!(
            index.search(&SearchQuery::text("ferret")).expect("search ok").len(),
            1
        );

        let after = scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "",
            vec![fact("person:alpha", "f1", "keeps a tortoise", date(2026, 1, 1))],
        );
        index.ingest_all(&[after]).expect("re-ingest");
        assert!(
            index.search(&SearchQuery::text("ferret")).expect("search ok").is_empty(),
            "the old row must be gone from the index, not left beside the new one"
        );
        assert_eq!(
            index.search(&SearchQuery::text("tortoise")).expect("search ok").len(),
            1
        );
    }

    /// An edited fact is re-indexed in place: one hit, saying the new thing. The
    /// incremental path re-reads the doc, so a stale copy can't survive an edit.
    #[tokio::test]
    async fn an_edit_leaves_one_indexed_copy_saying_the_new_thing() {
        let store = IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens");
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
            store.search(&SearchQuery::text("old place")).expect("search ok").is_empty(),
            "the superseded text must be gone from the index"
        );
        assert_eq!(
            store.search(&SearchQuery::text("new place")).expect("search ok").len(),
            1
        );
    }

    /// A blocked write indexes nothing — the guard said nothing was written, and
    /// the projection has to agree.
    #[tokio::test]
    async fn a_blocked_write_indexes_nothing() {
        let store = IndexedMemory::new(Arc::new(InMemoryMemory::new())).expect("index opens");
        store
            .add_entity(NewEntity::new(EntityId::person("zenith"), "Zenith", "user-named"))
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
                .search(&SearchQuery::text("should not be indexed"))
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
            .capture(NewFact::about(
                EntityId::person("alpha"),
                "was here before the server started",
                date(2026, 7, 1),
            ))
            .await
            .expect("capture ok");

        let store = IndexedMemory::new(inner).expect("index opens");
        assert!(
            store.search(&SearchQuery::text("before")).expect("search ok").is_empty(),
            "nothing is indexed until the scan runs"
        );
        assert_eq!(store.rebuild().await.expect("rebuild"), 1, "one doc scanned");
        assert_eq!(
            store.search(&SearchQuery::text("before")).expect("search ok").len(),
            1
        );
    }

    /// An edge filter is a filter, not a text match: it finds the fact carrying
    /// the edge and not the one that merely names the object.
    #[tokio::test]
    async fn an_edge_filter_beats_a_prose_mention() {
        let edged = Fact {
            edge: Some(Edge::new(EdgeShape::Location, EntityId("place:shelbyville".into()))),
            ..fact("person:alpha", "f1", "spending the winter away", date(2026, 1, 1))
        };
        let index = index_of(vec![scan(
            "doc-1",
            Some(entity("person:alpha", "Alpha")),
            "Alpha talks about shelbyville constantly and has never been.",
            vec![
                edged.clone(),
                fact("person:alpha", "f2", "wants to visit shelbyville someday", date(2026, 1, 2)),
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
        assert_eq!(hits, vec![Hit::Fact { fact: edged }], "got {hits:?}");
    }
}
