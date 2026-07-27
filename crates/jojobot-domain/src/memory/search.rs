//! Retrieval — the vocabulary of the `search` verb, and the port behind it.
//!
//! Ask-across ("which friends are in Shelbyville?", "what's connected to Duff Fest?") is
//! the retrieval jojobot exists to serve, and it is served by **one ranked list**
//! over four things at once: entities, facts, the **prose** a human wrote, and
//! the **messages** left in mailboxes. Mixing them is the point — a detail
//! demoted into a paragraph, or filed in a report to another session, must be
//! findable without anyone having remembered to file it as a fact.
//!
//! **This is the one place the two bounded contexts meet, and it meets them as a
//! reader.** Memory and Mailboxes share no type anywhere else and must not; here
//! a [`Hit`] carries a [`Message`] because the requirement is one ranked list and
//! one front door, not two search verbs a caller has to know to call both of. It
//! is a read-side union: nothing here writes to either context, and neither
//! context learns anything about the other from it.
//!
//! Truth stays in the store; the index is a **projection**, rebuilt by full
//! re-scan at start and updated in-process on every write. Read-back extends to
//! it: a fact captured a moment ago is findable without a restart.
//!
//! This module is pure vocabulary — no tantivy, no I/O. The index that satisfies
//! [`Search`] lives in the adapters.

use std::collections::HashSet;

use super::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, FactStatus, MemoryError, Provenance,
    validate_edge, validate_subject,
};
use crate::mailbox::Message;

/// One document as the index needs it: its prose, the entity it is (if it is
/// one), and the facts in its table. This is the shape a **full re-scan** yields,
/// and the unit an incremental update re-reads — so the index is always built
/// from the store's own text, never from a diff the writer guessed at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocScan {
    /// The store's id for the document. Opaque to the domain; it is what a hit
    /// hands back so a human can open the page.
    pub doc_id: String,
    /// The document's title — human, renamable, never used to resolve anything.
    pub title: String,
    /// The human-written prose: everything that is neither jojobot's machine
    /// block nor its fact table.
    pub prose: String,
    /// The entity this doc *is*, or `None` for a doc carrying no id marker — a
    /// page the user wrote themselves. Those are still searchable prose.
    pub entity: Option<Entity>,
    /// Every fact the doc's table holds.
    pub facts: Vec<Fact>,
}

/// The subjects in `doc`'s table that name **no known entity** — the split-brain
/// tell, deduped and in first-seen order.
///
/// A row is legitimately homed in its doc and legitimately about another entity;
/// what is never legitimate is a subject cell naming something that does not
/// exist. That is a hand edit gone wrong, and it used to be invisible: the row
/// stayed reachable through its home (which is why nothing broke) while quietly
/// projecting onto a handle no other read would ever agree on.
///
/// Counting is all this does. **A scan must never hard-fail on one, and must
/// never drop it** — the row is a fact somebody wrote. Surfacing the quarantine
/// to the caller is later work; being able to see it in a log is the floor.
pub fn orphan_subjects(doc: &DocScan, known: &HashSet<EntityId>) -> Vec<EntityId> {
    let mut seen = HashSet::new();
    doc.facts
        .iter()
        .map(|f| &f.subject)
        .filter(|s| !known.contains(*s))
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

/// The subjects in `doc`'s table that name **a different entity that exists** —
/// the doc's declared id and its own rows disagreeing, deduped and in first-seen
/// order.
///
/// This is the split-brain tell in its commoner disguise. [`orphan_subjects`]
/// only fires when a subject names *nothing*; a hand edit that retypes the cell
/// into another live handle leaves every read working — the row answers to one
/// id and lives under another, and the entity ends up readable under one and
/// writable under the other. Unlike an orphan, this **can be perfectly
/// legitimate**: a fact about one entity is often written on another's page. So
/// it is a signal, not a fault — counted, said out loud, and nothing more.
///
/// A doc that declares no entity has no id for its rows to disagree with.
pub fn foreign_subjects(doc: &DocScan, known: &HashSet<EntityId>) -> Vec<EntityId> {
    let Some(home) = doc.entity.as_ref().map(|e| &e.id) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    doc.facts
        .iter()
        .map(|f| &f.subject)
        .filter(|s| *s != home && known.contains(*s))
        .filter(|s| seen.insert((*s).clone()))
        .cloned()
        .collect()
}

/// Every entity a scan declares — the set [`orphan_subjects`] checks against.
pub fn known_entities(scan: &[DocScan]) -> HashSet<EntityId> {
    scan.iter()
        .filter_map(|d| d.entity.as_ref().map(|e| e.id.clone()))
        .collect()
}

/// Match facts by the edge they draw. `shape: None` means **any** shape pointing
/// at this object — "what's connected to X".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeFilter {
    /// Narrow to one shape, or `None` for any.
    pub shape: Option<EdgeShape>,
    /// The entity the edge must point at.
    pub object: EntityId,
}

/// The default number of results — raisable by the caller. There is **no
/// pagination and no cursor**: a second page is a better query.
pub const DEFAULT_LIMIT: usize = 20;

/// What to search for. `text` is optional as long as a structural filter narrows
/// the field, because the structural questions ("every superseded fact", "who is
/// in Shelbyville") have no keyword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// Free text, matched over entity handles/names, fact content/details, and
    /// prose. All terms must match.
    pub text: Option<String>,
    /// Narrow to one entity kind: an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose doc the prose sits in.
    pub kind: Option<EntityKind>,
    /// Narrow to one lifecycle state. **`None` means active only** — a
    /// superseded fact is excluded unless asked for by name, because a claim the
    /// store has already moved past coming back as current truth is worse than
    /// no memory at all.
    pub status: Option<FactStatus>,
    /// Narrow to testimony or inference.
    pub provenance: Option<Provenance>,
    /// Facts about one entity.
    pub subject: Option<EntityId>,
    /// Facts drawing a matching edge.
    pub edge: Option<EdgeFilter>,
    /// Whether messages are in the answer. **True by default**, because the
    /// whole point is a session finding context it did not know where to look
    /// for: excluded-by-default would rebuild the blindness with an extra step
    /// in front of it. Set it false to keep work-queue traffic out of a recall
    /// that is asking about the operator's life rather than about the sessions
    /// serving it.
    pub include_mail: bool,
    /// How many results to return.
    pub limit: usize,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            text: None,
            kind: None,
            status: None,
            provenance: None,
            subject: None,
            edge: None,
            include_mail: true,
            limit: DEFAULT_LIMIT,
        }
    }
}

impl SearchQuery {
    /// A query for free text, everything else defaulted.
    pub fn text(text: impl Into<String>) -> Self {
        SearchQuery {
            text: Some(text.into()),
            ..Default::default()
        }
    }

    /// The trimmed text, or `None` if there is none worth matching.
    pub fn terms(&self) -> Option<&str> {
        self.text
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
    }

    /// Is this query scoped to **facts alone**? `status`, `provenance`, `subject`
    /// and `edge` are properties only a fact has, so naming one is a statement
    /// that entities and prose are not what the caller is looking for.
    ///
    /// The *default* status (active only) does not count — a default must not
    /// silently narrow a search to one hit type.
    pub fn is_fact_scoped(&self) -> bool {
        self.status.is_some()
            || self.provenance.is_some()
            || self.subject.is_some()
            || self.edge.is_some()
    }

    /// Reject a query that cannot be served, before any index work: no text and
    /// no filter is a request for "everything", which is not a search; and the
    /// entity references it carries must be well-formed ids.
    pub fn validate(&self) -> Result<(), MemoryError> {
        if self.terms().is_none() && self.kind.is_none() && !self.is_fact_scoped() {
            return Err(MemoryError::InvalidQuery(
                "give a query, or at least one filter (kind, status, provenance, subject, edge)"
                    .into(),
            ));
        }
        if self.limit == 0 {
            return Err(MemoryError::InvalidQuery("limit must be at least 1".into()));
        }
        if let Some(subject) = &self.subject {
            validate_subject(subject)?;
        }
        // The same rule the write path applies, reused rather than restated: a
        // filter combination no write could ever produce must read as the
        // caller's mistake, not as an honest empty answer.
        match &self.edge {
            Some(EdgeFilter {
                shape: Some(shape),
                object,
            }) => validate_edge(&Edge::new(*shape, object.clone()))?,
            Some(EdgeFilter {
                shape: None,
                object,
            }) => validate_subject(object)?,
            None => {}
        }
        Ok(())
    }
}

/// A handle, resolved as far as the index can resolve it — **the cure for a bare
/// hit**. A result that says only `person:homer-simpson` makes the reader spend a second
/// call to learn whether that is Homer Simpson, and a third to learn he is also Cosme Fulanito.
///
/// `name` is `None` when the handle resolves to no entity the index holds. That
/// is the orphan case ([`orphan_subjects`]) and it is left visibly empty rather
/// than filled with the handle: a missing name is a fact about the store, and
/// papering over it is how the split brain stayed invisible in the first place.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRef {
    /// The handle itself — always present, always what an edit or a follow-up
    /// query takes.
    pub id: EntityId,
    /// The kind the handle declares. `None` only for an id whose grammar is
    /// broken, which a tolerant reader can still hand back.
    pub kind: Option<EntityKind>,
    /// The display name, when the handle names an entity the index knows.
    pub name: Option<String>,
    /// The other names it answers to. Empty is the ordinary case — and also
    /// what an unresolved handle carries, because there are none to report,
    /// not because they are unknown. That is why this is a list where `name` is
    /// an option: absent and none-at-all are the same answer here.
    pub aliases: Vec<String>,
}

impl EntityRef {
    /// A handle nobody has resolved: the kind its grammar declares, no name.
    pub fn unresolved(id: EntityId) -> Self {
        EntityRef {
            kind: id.kind(),
            id,
            name: None,
            aliases: Vec::new(),
        }
    }

    /// A handle resolved against the entity it names.
    ///
    /// The aliases, **not** [`Entity::labels`]: labels lead with the display
    /// name, which is already `name` here, and repeating it would make one
    /// label read as two.
    pub fn resolved(entity: &Entity) -> Self {
        EntityRef {
            id: entity.id.clone(),
            kind: Some(entity.kind),
            name: Some(entity.name.clone()),
            aliases: entity.aliases.clone(),
        }
    }
}

/// One result. **Typed, and in one list with the others** — the caller is told
/// what each hit is rather than having to guess from its shape.
///
/// Every variant arrives **with its surroundings**: an answer that is only a row
/// leaves the reader to go and find out what it is attached to, and the reader
/// is an assistant that will simply not bother. So a fact names the entities it
/// is about and sits on, and an entity carries the edges its facts draw.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// An entity matched by handle or name.
    Entity {
        /// The entity itself.
        entity: Entity,
        /// The doc that carries it.
        doc_id: String,
        /// The edges its facts draw — where this entity sits in the graph,
        /// deduped and in first-seen order.
        edges: Vec<Edge>,
    },
    /// A fact matched by content, details, or a filter. Carried **whole** — the
    /// row, not a snippet — because the answer usually IS the row, and its
    /// address is what an edit needs.
    Fact {
        /// The fact, address and all.
        fact: Fact,
        /// The entity the fact is about, resolved.
        subject: EntityRef,
        /// The entity whose doc holds the row, resolved. Usually the same as
        /// `subject`; when it is not, that difference is the thing worth seeing.
        home: EntityRef,
    },
    /// A message matched in a mailbox — **unmistakably mail**, and carrying the
    /// whole envelope: which box, what state it is in, who sent it, and the id
    /// `read_message` takes. Without those, a mail hit reads as an anonymous
    /// paragraph and a reader cannot tell live work from an archived report.
    ///
    /// The body arrives as a snippet, not whole: a message is often pages, and
    /// its id is right there for taking delivery of the rest.
    Message {
        /// The message, envelope and all. Its `body` is the whole stored text;
        /// what a caller is shown around the match is `snippet`.
        message: Message,
        /// The matching text with enough around it to read.
        snippet: String,
    },
    /// Human prose matched inside a document body.
    Prose {
        /// The doc that carries it.
        doc_id: String,
        /// The doc's title.
        title: String,
        /// The entity whose doc this is, when it is an entity doc at all —
        /// whole, not a bare handle.
        entity: Option<Entity>,
        /// The edges that entity's facts draw; empty for a doc that is nobody's.
        edges: Vec<Edge>,
        /// The matching text with enough around it to read.
        snippet: String,
    },
}

/// How much of the mailbox board the projection actually holds — **the honesty
/// half of degrade-don't-error**, and three states rather than two because the
/// middle one is reachable and was being reported as one of the others.
///
/// A search is a read of an in-process index, so a mailbox world that was
/// unreachable when the index was built does not make searching fail; it makes
/// mail missing. "No message says that" and "jojobot has read no messages" are
/// different claims, and a caller acts on both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MailCoverage {
    /// The board has never been read and nothing has indexed a message: no
    /// message is searchable at all, and an empty answer means nothing.
    Unread,
    /// The board has never been read, but messages written **through this
    /// process since** are indexed. Hits are real and findable; anything older
    /// than this process is missing, and a caller who is looking for an old
    /// message has to be told that rather than shown an empty list.
    ///
    /// This is what a failed boot scan leaves behind, and it is exactly the
    /// state that used to report [`Unread`](Self::Unread) — an answer carrying
    /// message hits while saying no message was searched.
    Partial,
    /// The board was read: everything on it is searchable.
    Loaded,
}

/// The retrieval port: one ranked, mixed list. Synchronous — the index is
/// in-process, so a search is a memory read, not I/O.
pub trait Search: Send + Sync {
    /// Search entities, facts, prose and messages at once. Ordering is the
    /// ranking: text relevance, boosted by recency, with an entity whose handle
    /// or name the query matches pinned to the top.
    fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError>;

    /// How much of the mail board this projection holds — see [`MailCoverage`].
    /// Memory results come back whatever it says; this is what lets an answer
    /// tell a caller which kind of silence they are looking at.
    fn mail_coverage(&self) -> MailCoverage;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_query_with_neither_text_nor_a_filter_is_refused() {
        let empty = SearchQuery::default();
        assert!(matches!(
            empty.validate(),
            Err(MemoryError::InvalidQuery(_))
        ));
        // Whitespace is not a query either.
        assert!(SearchQuery::text("   ").validate().is_err());
    }

    /// A structural filter is enough on its own: "every superseded fact" and
    /// "which people are in Shelbyville" carry no keyword.
    #[test]
    fn a_structural_filter_alone_is_a_valid_query() {
        let superseded = SearchQuery {
            status: Some(FactStatus::Superseded),
            ..Default::default()
        };
        assert!(superseded.validate().is_ok());
        let edged = SearchQuery {
            kind: Some(EntityKind::Person),
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: EntityId("place:far-country".into()),
            }),
            ..Default::default()
        };
        assert!(edged.validate().is_ok());
    }

    /// The fact-only filters say "facts, please"; `kind` and free text do not.
    #[test]
    fn only_the_fact_properties_scope_a_query_to_facts() {
        assert!(!SearchQuery::text("shelbyville").is_fact_scoped());
        assert!(
            !SearchQuery {
                kind: Some(EntityKind::Person),
                ..Default::default()
            }
            .is_fact_scoped(),
            "a kind filter applies to entities and prose too"
        );
        for scoped in [
            SearchQuery {
                status: Some(FactStatus::Superseded),
                ..Default::default()
            },
            SearchQuery {
                provenance: Some(Provenance::Testimony),
                ..Default::default()
            },
            SearchQuery {
                subject: Some(EntityId::person("alpha")),
                ..Default::default()
            },
            SearchQuery {
                edge: Some(EdgeFilter {
                    shape: None,
                    object: EntityId("place:x".into()),
                }),
                ..Default::default()
            },
        ] {
            assert!(
                scoped.is_fact_scoped(),
                "{scoped:?} names a fact-only property"
            );
        }
    }

    /// A malformed entity reference in a filter is refused, exactly as it is on a
    /// write: an id is structured, never free text.
    #[test]
    fn a_malformed_reference_in_a_filter_is_refused() {
        let bad_subject = SearchQuery {
            subject: Some(EntityId("not-an-id".into())),
            ..Default::default()
        };
        assert!(matches!(
            bad_subject.validate(),
            Err(MemoryError::InvalidSubject(_))
        ));
        let bad_object = SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: EntityId("place:a|b".into()),
            }),
            ..Default::default()
        };
        assert!(matches!(
            bad_object.validate(),
            Err(MemoryError::InvalidSubject(_))
        ));
    }

    /// The shape→kind rule binds the **read** path as hard as the write path.
    /// `{shape: location, object: person:x}` is a combination no write can
    /// produce, so serving it returns zero hits — and zero hits reads as "nobody
    /// is there", not "you asked something impossible". The caller's mis-drawn
    /// filter has to come back as their mistake.
    #[test]
    fn an_edge_filter_whose_object_is_wrong_for_its_shape_is_refused() {
        let impossible = SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::Location),
                object: EntityId::person("alpha"),
            }),
            ..Default::default()
        };
        assert!(
            matches!(impossible.validate(), Err(MemoryError::InvalidEdge(_))),
            "a location edge points at a place, on the query path too"
        );
        // A shapeless filter is "what's connected to X" — every kind is fair game.
        let any_shape = SearchQuery {
            edge: Some(EdgeFilter {
                shape: None,
                object: EntityId::person("alpha"),
            }),
            ..Default::default()
        };
        assert!(any_shape.validate().is_ok());
        // …and `about` is the open shape, so it accepts a person too.
        let open = SearchQuery {
            edge: Some(EdgeFilter {
                shape: Some(EdgeShape::About),
                object: EntityId::person("alpha"),
            }),
            ..Default::default()
        };
        assert!(open.validate().is_ok());
    }

    /// A subject naming an entity nobody declares is counted; one naming a real
    /// entity — this doc's own, or another doc's — is not. The row is never
    /// dropped and never fails the scan: it is a fact somebody wrote, and the
    /// point is only that it stops being invisible.
    #[test]
    fn a_subject_that_names_no_known_entity_is_counted_once() {
        use crate::memory::{Boot, FactId};
        use jiff::civil::date;

        let entity = |id: &str| Entity {
            id: EntityId(id.into()),
            kind: EntityId(id.into())
                .kind()
                .expect("test ids are well-formed"),
            name: String::new(),
            aliases: Vec::new(),
            source: "test".into(),
            crm: None,
            mailbox: None,
            boot: Boot::OnDemand,
        };
        let row = |id: &str, subject: &str| Fact {
            id: FactId(id.into()),
            home: EntityId::person("alpha"),
            subject: EntityId(subject.into()),
            content: "a claim".into(),
            details: None,
            provenance: Provenance::Inference,
            status: FactStatus::Active,
            date: date(2026, 7, 1),
            edge: None,
        };

        let doc = DocScan {
            doc_id: "doc-1".into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha")),
            facts: vec![
                row("f1", "person:alpha"),  // its own entity
                row("f2", "person:beta"),   // another doc's entity, legitimately
                row("f3", "person:alphaa"), // names nothing — the hand-edit tell
                row("f4", "person:alphaa"), // …twice, reported once
            ],
        };
        let known: HashSet<EntityId> = [EntityId::person("alpha"), EntityId::person("beta")]
            .into_iter()
            .collect();

        assert_eq!(
            orphan_subjects(&doc, &known),
            vec![EntityId::person("alphaa")],
            "only the subject naming no entity, and only once"
        );
        assert_eq!(
            known_entities(std::slice::from_ref(&doc)),
            [EntityId::person("alpha")]
                .into_iter()
                .collect::<HashSet<_>>(),
            "a scan's known set is the entities its docs declare"
        );
    }

    /// The **other** half of the split-brain tell, and the one the Cosme incident
    /// actually wore: a row whose subject names a real entity that is not the doc
    /// it sits in. A hand edit retyped the subject cell into another live handle,
    /// so nothing was orphaned — the row simply answered to one id and lived under
    /// another, and the orphan counter (which only fires on a subject naming
    /// *nothing*) had nothing to say about it.
    #[test]
    fn a_subject_naming_another_existing_entity_is_counted_apart_from_an_orphan() {
        use crate::memory::{Boot, FactId};
        use jiff::civil::date;

        let entity = |id: &str| Entity {
            id: EntityId(id.into()),
            kind: EntityId(id.into())
                .kind()
                .expect("test ids are well-formed"),
            name: String::new(),
            aliases: Vec::new(),
            source: "test".into(),
            crm: None,
            mailbox: None,
            boot: Boot::OnDemand,
        };
        let row = |id: &str, subject: &str| Fact {
            id: FactId(id.into()),
            home: EntityId::person("alpha"),
            subject: EntityId(subject.into()),
            content: "a claim".into(),
            details: None,
            provenance: Provenance::Inference,
            status: FactStatus::Active,
            date: date(2026, 7, 1),
            edge: None,
        };
        let doc = DocScan {
            doc_id: "doc-1".into(),
            title: "Alpha".into(),
            prose: String::new(),
            entity: Some(entity("person:alpha")),
            facts: vec![
                row("f1", "person:alpha"),  // its own entity
                row("f2", "person:beta"),   // a different entity that exists
                row("f3", "person:beta"),   // …twice, reported once
                row("f4", "person:alphaa"), // names nothing: an orphan, not this
            ],
        };
        let known: HashSet<EntityId> = [EntityId::person("alpha"), EntityId::person("beta")]
            .into_iter()
            .collect();

        assert_eq!(
            foreign_subjects(&doc, &known),
            vec![EntityId::person("beta")],
            "only the subject naming another live entity, and only once"
        );
        assert_eq!(
            orphan_subjects(&doc, &known),
            vec![EntityId::person("alphaa")],
            "the two counters must not swallow each other's case"
        );

        // A doc that declares no entity has no id to disagree with.
        let loose = DocScan {
            entity: None,
            ..doc
        };
        assert!(foreign_subjects(&loose, &known).is_empty());
    }

    #[test]
    fn a_zero_limit_is_refused() {
        let query = SearchQuery {
            limit: 0,
            ..SearchQuery::text("shelbyville")
        };
        assert!(matches!(
            query.validate(),
            Err(MemoryError::InvalidQuery(_))
        ));
    }
}
