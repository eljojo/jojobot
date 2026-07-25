//! Retrieval — the vocabulary of the `search` verb, and the port behind it.
//!
//! Ask-across ("which friends are in Shelbyville?", "what's connected to Duff Fest?") is
//! the retrieval jojobot exists to serve, and it is served by **one ranked list**
//! over three things at once: entities, facts, and the **prose** a human wrote.
//! Mixing them is the point — a detail demoted into a paragraph must be findable
//! without anyone having remembered to file it as a fact.
//!
//! Truth stays in the store; the index is a **projection**, rebuilt by full
//! re-scan at start and updated in-process on every write. Read-back extends to
//! it: a fact captured a moment ago is findable without a restart.
//!
//! This module is pure vocabulary — no tantivy, no I/O. The index that satisfies
//! [`Search`] lives in the adapters.

use super::{Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, FactStatus, MemoryError, Provenance, validate_subject};

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

/// Match facts by the edge they draw. `shape: None` means **any** shape pointing
/// at this object — "what's connected to X".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EdgeFilter {
    /// Narrow to one shape, or `None` for any.
    pub shape: Option<EdgeShape>,
    /// The entity the edge must point at.
    pub object: EntityId,
}

impl EdgeFilter {
    /// Does `edge` satisfy this filter?
    pub fn matches(&self, edge: &Edge) -> bool {
        edge.object == self.object && self.shape.is_none_or(|s| s == edge.shape)
    }
}

/// The default number of results — raisable by the caller. There is **no
/// pagination and no cursor**: a second page is a better query.
pub const DEFAULT_LIMIT: usize = 20;

/// What to search for. `text` is optional as long as a structural filter narrows
/// the field, because the structural questions ("every negated fact", "who is in
/// Shelbyville") have no keyword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchQuery {
    /// Free text, matched over entity handles/names, fact content/details, and
    /// prose. All terms must match.
    pub text: Option<String>,
    /// Narrow to one entity kind: an entity's own kind, a fact's subject's kind,
    /// or the kind of the entity whose doc the prose sits in.
    pub kind: Option<EntityKind>,
    /// Narrow to one lifecycle state. **`None` means active only** — superseded
    /// and negated facts are excluded unless asked for by name, and
    /// `Some(Negated)` is the anti-fact list.
    pub status: Option<FactStatus>,
    /// Narrow to testimony or inference.
    pub provenance: Option<Provenance>,
    /// Facts about one entity.
    pub subject: Option<EntityId>,
    /// Facts drawing a matching edge.
    pub edge: Option<EdgeFilter>,
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
        self.text.as_deref().map(str::trim).filter(|t| !t.is_empty())
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
        if let Some(edge) = &self.edge {
            validate_subject(&edge.object)?;
        }
        Ok(())
    }
}

/// One result. **Typed, and in one list with the others** — the caller is told
/// what each hit is rather than having to guess from its shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hit {
    /// An entity matched by handle or name.
    Entity {
        /// The entity itself.
        entity: Entity,
        /// The doc that carries it.
        doc_id: String,
    },
    /// A fact matched by content, details, or a filter. Carried **whole** — the
    /// row, not a snippet — because the answer usually IS the row, and its
    /// address is what an edit needs.
    Fact {
        /// The fact, address and all.
        fact: Fact,
    },
    /// Human prose matched inside a document body.
    Prose {
        /// The doc that carries it.
        doc_id: String,
        /// The doc's title.
        title: String,
        /// The entity whose doc this is, when it is an entity doc at all.
        entity: Option<EntityId>,
        /// The matching text with enough around it to read.
        snippet: String,
    },
}

/// The retrieval port: one ranked, mixed list. Synchronous — the index is
/// in-process, so a search is a memory read, not I/O.
pub trait Search: Send + Sync {
    /// Search entities, facts and prose at once. Ordering is the ranking: text
    /// relevance, boosted by recency, with an entity whose handle or name the
    /// query matches pinned to the top.
    fn search(&self, query: &SearchQuery) -> Result<Vec<Hit>, MemoryError>;
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

    /// A structural filter is enough on its own: "every negated fact" and "which
    /// people are in Shelbyville" carry no keyword.
    #[test]
    fn a_structural_filter_alone_is_a_valid_query() {
        let negated = SearchQuery {
            status: Some(FactStatus::Negated),
            ..Default::default()
        };
        assert!(negated.validate().is_ok());
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
            !SearchQuery { kind: Some(EntityKind::Person), ..Default::default() }.is_fact_scoped(),
            "a kind filter applies to entities and prose too"
        );
        for scoped in [
            SearchQuery { status: Some(FactStatus::Negated), ..Default::default() },
            SearchQuery { provenance: Some(Provenance::Testimony), ..Default::default() },
            SearchQuery { subject: Some(EntityId::person("alpha")), ..Default::default() },
            SearchQuery {
                edge: Some(EdgeFilter { shape: None, object: EntityId("place:x".into()) }),
                ..Default::default()
            },
        ] {
            assert!(scoped.is_fact_scoped(), "{scoped:?} names a fact-only property");
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
            edge: Some(EdgeFilter { shape: None, object: EntityId("place:a|b".into()) }),
            ..Default::default()
        };
        assert!(matches!(
            bad_object.validate(),
            Err(MemoryError::InvalidSubject(_))
        ));
    }

    #[test]
    fn a_zero_limit_is_refused() {
        let query = SearchQuery { limit: 0, ..SearchQuery::text("shelbyville") };
        assert!(matches!(query.validate(), Err(MemoryError::InvalidQuery(_))));
    }

    /// An edge filter with no shape matches any edge pointing at the object —
    /// "what's connected to X".
    #[test]
    fn an_edge_filter_without_a_shape_matches_any_shape() {
        let object = EntityId("event:winter-fest".into());
        let any = EdgeFilter { shape: None, object: object.clone() };
        for shape in EdgeShape::ALL {
            assert!(any.matches(&Edge::new(shape, object.clone())), "{shape} must match");
        }
        assert!(
            !any.matches(&Edge::new(EdgeShape::About, EntityId("topic:widgets".into()))),
            "a different object must not match"
        );

        let only_attendance = EdgeFilter {
            shape: Some(EdgeShape::Attendance),
            object: object.clone(),
        };
        assert!(only_attendance.matches(&Edge::new(EdgeShape::Attendance, object.clone())));
        assert!(
            !only_attendance.matches(&Edge::new(EdgeShape::About, object)),
            "a shape filter must exclude the other shapes"
        );
    }
}
