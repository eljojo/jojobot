//! **The graph query: say the shape you want, and get that shape back.**
//!
//! Three axes, and they combine. **Select** says which objects the answer is
//! about — one named handle, a kind, the records that answer a type, a key and
//! the value it holds. **Include** says what of each object comes back — its
//! facts, its prose, or neither. **Follow** says which edges to walk and how
//! far, and the answer NESTS: a walked object carries the objects it reached,
//! each carrying its own.
//!
//! **Nesting is the reason this exists.** A flat list of hits answers "which
//! people are in Shelbyville"; it cannot answer "which people are in
//! Shelbyville, and what does each of them eat" without the caller holding the
//! first answer and asking again per row. The shape is the answer.
//!
//! **It is vocabulary plus one pure resolution.** [`resolve`] takes the store's
//! own documents and returns objects; nothing here does I/O, so the walk is
//! testable without a store and identical whichever store answers.

use std::collections::{BTreeMap, HashSet};

use super::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, MemoryError, guard, search::DocScan,
    types, validate_subject,
};

/// **How far a walk may go.** A bound rather than a preference: an edge may
/// point back at where it came from, and a walk with no ceiling on a graph with
/// a cycle in it is one that returns when the process dies.
///
/// The visited set already stops a cycle repeating; this stops a query asking
/// for a subgraph nobody can read.
pub const MAX_DEPTH: usize = 5;

/// **One key of a record, and optionally the value it must hold.**
///
/// `value: None` asks only that the key is carried at all — which is a
/// different question from any particular value and the one to ask when you
/// want everything that records a thing rather than everything that records it
/// one way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    /// The key, exactly as a record spells it. Matching is structural, so
    /// nothing has to have declared it.
    pub key: String,
    /// The value it must hold, compared whole and trimmed. `None` matches any
    /// value.
    pub value: Option<String>,
}

impl FieldFilter {
    /// A filter asking only that the key is there.
    pub fn key(key: &str) -> Self {
        FieldFilter {
            key: key.trim().to_string(),
            value: None,
        }
    }

    /// A filter asking that the key holds this value.
    pub fn holding(key: &str, value: &str) -> Self {
        FieldFilter {
            key: key.trim().to_string(),
            value: Some(value.trim().to_string()),
        }
    }

    /// Does this record satisfy the filter.
    fn satisfied_by(&self, record: &BTreeMap<String, String>) -> bool {
        match (record.get(&self.key), &self.value) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(held), Some(wanted)) => held.trim() == wanted,
        }
    }
}

/// **Which objects the answer is about.** Every field narrows, and they
/// combine: a kind AND a type AND a key's value describe one set, not three.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Selection {
    /// One entity by handle. **A named subject always comes back**, even when
    /// the filters below match none of its facts — naming a handle asks for
    /// that object, where a filter asks which objects, and answering "no such
    /// thing" to the first because of the second would make a typo and an
    /// empty page read alike.
    pub subject: Option<EntityId>,
    /// Every entity of one kind.
    pub kind: Option<EntityKind>,
    /// Objects holding a record that answers this type, matched structurally
    /// over the keys it carries — never over what anybody declared it to be.
    pub answers_type: Option<types::DeclaredType>,
    /// Objects holding a record that carries these keys, and the values named.
    /// **Every filter must hold on ONE record**: two filters are a description
    /// of a single record, not a pair of separate questions.
    pub fields: Vec<FieldFilter>,
}

impl Selection {
    /// Is there a filter here that a fact has to answer? Kind and subject are
    /// properties of the object; these two are properties of its records.
    fn filters_facts(&self) -> bool {
        self.answers_type.is_some() || !self.fields.is_empty()
    }

    /// Does this fact answer every record filter. A fact carrying no record
    /// answers none of them.
    fn keeps(&self, fact: &Fact) -> bool {
        if !self.filters_facts() {
            return true;
        }
        let Some(event) = fact.event.as_ref() else {
            return false;
        };
        if let Some(declared) = &self.answers_type
            && declared.matched_by(&event.metadata).is_none()
        {
            return false;
        }
        self.fields.iter().all(|f| f.satisfied_by(&event.metadata))
    }
}

/// **What of each object comes back.**
///
/// Both may be off: a caller that wants the shape of the graph and not its
/// contents gets handles and the edges between them, which is the cheapest
/// useful answer and a legitimate question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Include {
    /// The object's facts.
    pub facts: bool,
    /// The object's prose — the human half of its page, **whole**. A charter is
    /// prose; so is a portrait. Nothing here knows which is which.
    pub prose: bool,
}

impl Default for Include {
    /// Facts, and no prose. Facts are what this verb has always answered with,
    /// and prose is a page rather than a row: shipping every object's page
    /// unasked is the cost a caller cannot decline.
    fn default() -> Self {
        Include {
            facts: true,
            prose: false,
        }
    }
}

/// **Which end of an edge a walk leaves by.**
///
/// An edge is written on the record that draws it, so the two directions are
/// two different questions. From a person, `Out` reaches the party they are
/// attending. From the party, `In` reaches its guests. A walk that could only
/// go one way would answer half the questions and give no sign of the half it
/// dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    /// The edges this object's own records draw.
    #[default]
    Out,
    /// The edges other objects' records draw AT this one.
    In,
}

impl Direction {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Direction::Out => "out",
            Direction::In => "in",
        }
    }

    /// The direction a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Direction> {
        match token.trim() {
            "out" => Some(Direction::Out),
            "in" => Some(Direction::In),
            _ => None,
        }
    }
}

/// **Which edges to walk, and how far.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    /// Narrow to one shape, or `None` for any edge — "whatever it is connected
    /// to".
    pub shape: Option<EdgeShape>,
    /// Which end to leave by.
    pub direction: Direction,
    /// How many hops. One is the neighbours; two is the neighbours' neighbours.
    pub depth: usize,
}

impl Follow {
    /// One hop along every shape, outbound.
    pub fn hop() -> Self {
        Follow {
            shape: None,
            direction: Direction::Out,
            depth: 1,
        }
    }
}

/// **The whole query.** Three axes, and they are independent: any selection may
/// carry any include and any follow.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GraphQuery {
    /// Which objects.
    pub select: Selection,
    /// What of each.
    pub include: Include,
    /// Which edges to walk. `None` walks none, and the answer is flat.
    pub follow: Option<Follow>,
}

impl GraphQuery {
    /// The query for one entity's facts — what this verb answered before it
    /// could walk, and still the commonest thing asked of it.
    pub fn subject(subject: EntityId) -> Self {
        GraphQuery {
            select: Selection {
                subject: Some(subject),
                ..Selection::default()
            },
            ..GraphQuery::default()
        }
    }

    /// **Refuse a query that cannot be served, before any read.**
    ///
    /// A selection that narrows nothing is a request for the whole store,
    /// which is not a question. Everything else refused here is a malformed
    /// argument rather than an unlucky one: an id that is no id, a declaration
    /// no store would keep, a key that is no key, a walk of no hops.
    pub fn validate(&self) -> Result<(), MemoryError> {
        let select = &self.select;
        if select.subject.is_none() && select.kind.is_none() && !select.filters_facts() {
            return Err(MemoryError::InvalidQuery(
                "name what to recall: a subject, a kind, a type, or a key".into(),
            ));
        }
        if let Some(subject) = &select.subject {
            validate_subject(subject)?;
        }
        // The same validator the declaring path uses, for the reason `search`
        // reuses it: a declaration no store would have kept is the caller's
        // mistake, and it must read as one rather than as an honest empty
        // answer.
        if let Some(declared) = &select.answers_type {
            types::validate_type(declared)?;
        }
        for field in &select.fields {
            if field.key.trim().is_empty() {
                return Err(MemoryError::InvalidQuery(
                    "a key filter names no key".into(),
                ));
            }
        }
        if let Some(follow) = &self.follow {
            if follow.depth == 0 {
                return Err(MemoryError::InvalidQuery(
                    "a walk of no hops follows nothing: give a depth of at least 1, or ask for no \
                     walk at all"
                        .into(),
                ));
            }
            if follow.depth > MAX_DEPTH {
                return Err(MemoryError::InvalidQuery(format!(
                    "a walk goes at most {MAX_DEPTH} hops"
                )));
            }
        }
        Ok(())
    }
}

/// **How a walk reached an object.** Absent on a root, which nothing reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Via {
    /// The edge's shape.
    pub shape: EdgeShape,
    /// Which way it was walked to get here.
    pub direction: Direction,
}

/// **One object in the answer**, and the objects it reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// The entity itself.
    pub entity: Entity,
    /// How the walk got here; `None` on a root.
    pub via: Option<Via>,
    /// Its facts — the ones that answered the record filters, or all of them
    /// when none were given. Empty when facts were not asked for.
    pub facts: Vec<Fact>,
    /// Its prose, whole. `None` when prose was not asked for, so a caller can
    /// tell "not asked for" from "the page is blank".
    pub prose: Option<String>,
    /// The objects one hop further out, each carrying its own.
    pub connected: Vec<Object>,
    /// **This object has edges nobody followed.** Either the walk ran out of
    /// hops here, or the objects beyond it were already in the answer.
    ///
    /// Without it an empty `connected` says two different things — the walk
    /// stopped, or there is nothing there — and a reader who has to infer
    /// which will eventually infer wrong. A caller that wants the rest asks
    /// again with a deeper walk, or from this handle.
    pub unwalked: bool,
}

/// **The walk, over a store's own documents.** Pure: no I/O, no store, so the
/// graph semantics are one function that every adapter answers for identically.
///
/// The selection chooses where the walk STARTS. What the walk reaches is not
/// filtered again — a neighbour is in the answer because something pointed at
/// it, and re-applying the caller's filters at every hop would return a set
/// that is neither the neighbourhood nor the selection.
pub fn resolve(scanned: &[DocScan], query: &GraphQuery) -> Result<Vec<Object>, MemoryError> {
    query.validate()?;
    let ctx = Ctx::of(scanned);
    let roots = ctx.roots(&query.select)?;
    Ok(roots
        .into_iter()
        .map(|id| {
            let mut seen = HashSet::from([id.clone()]);
            ctx.expand(&id, None, query, query.follow.as_ref(), &mut seen)
        })
        .collect())
}

/// The store's documents, indexed the three ways a walk reads them.
struct Ctx<'a> {
    /// Every entity, by handle.
    entities: BTreeMap<&'a EntityId, &'a Entity>,
    /// Every entity, in handle order — what the near-miss screen reads.
    index: Vec<Entity>,
    /// Each entity's prose.
    prose: BTreeMap<&'a EntityId, &'a str>,
    /// The facts belonging to each entity: those ABOUT it, and those homed in
    /// its document whatever their subject says. The same rule
    /// [`recall`](super::Memory::recall) answers by, so one record does not
    /// belong to an entity through one verb and not another.
    facts: BTreeMap<&'a EntityId, Vec<&'a Fact>>,
    /// For each entity, the edges drawn AT it: the shape, and the entity whose
    /// record draws it. The reverse of the edge cell, built once.
    inbound: BTreeMap<EntityId, Vec<(EdgeShape, EntityId)>>,
}

impl<'a> Ctx<'a> {
    fn of(scanned: &'a [DocScan]) -> Self {
        let mut entities = BTreeMap::new();
        let mut prose = BTreeMap::new();
        let mut facts: BTreeMap<&EntityId, Vec<&Fact>> = BTreeMap::new();
        let mut inbound: BTreeMap<EntityId, Vec<(EdgeShape, EntityId)>> = BTreeMap::new();

        for doc in scanned {
            if let Some(entity) = doc.entity.as_ref() {
                entities.insert(&entity.id, entity);
                prose.insert(&entity.id, doc.prose.as_str());
            }
            for fact in &doc.facts {
                facts.entry(&fact.subject).or_default().push(fact);
                if fact.home != fact.subject {
                    facts.entry(&fact.home).or_default().push(fact);
                }
                if let Some(Edge { shape, object }) = fact.edge.as_ref() {
                    inbound
                        .entry(object.clone())
                        .or_default()
                        .push((*shape, fact.subject.clone()));
                }
            }
        }

        let mut index: Vec<Entity> = entities.values().map(|e| (*e).clone()).collect();
        index.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ctx {
            entities,
            index,
            prose,
            facts,
            inbound,
        }
    }

    /// The handles the query starts from, in handle order so two reads of an
    /// unchanged store agree.
    fn roots(&self, select: &Selection) -> Result<Vec<EntityId>, MemoryError> {
        if let Some(subject) = &select.subject {
            // A handle that names nothing is a miss with candidates, never an
            // empty answer: an empty answer reads as "nothing is recorded" and
            // a caller cannot repair a typo from it.
            if !self.entities.contains_key(subject) {
                return Err(MemoryError::UnknownEntity {
                    attempted: subject.to_string(),
                    nearest: guard::screen(subject, &[], &self.index),
                });
            }
            return Ok(vec![subject.clone()]);
        }
        let mut found: Vec<EntityId> = self
            .entities
            .values()
            .filter(|e| select.kind.is_none_or(|k| e.kind == k))
            .filter(|e| !select.filters_facts() || self.kept_facts(&e.id, select).next().is_some())
            .map(|e| e.id.clone())
            .collect();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(found)
    }

    /// This entity's facts that answer the record filters.
    fn kept_facts<'q>(
        &'q self,
        id: &EntityId,
        select: &'q Selection,
    ) -> impl Iterator<Item = &'a Fact> + 'q {
        self.facts
            .get(id)
            .into_iter()
            .flatten()
            .copied()
            .filter(|f| select.keeps(f))
    }

    /// Build one object, and everything it reaches within the remaining hops.
    ///
    /// Recursive because the answer is: an object holds objects, and a walk
    /// that flattened them would be the list this verb already had.
    fn expand(
        &self,
        id: &EntityId,
        via: Option<Via>,
        query: &GraphQuery,
        follow: Option<&Follow>,
        seen: &mut HashSet<EntityId>,
    ) -> Object {
        let entity = (*self
            .entities
            .get(id)
            .expect("expand is only ever called with a handle the index holds"))
        .clone();
        // The record filters describe the roots. A neighbour is in the answer
        // because something points at it, so it brings its own facts whole.
        let kept: Vec<&Fact> = if via.is_none() {
            self.kept_facts(id, &query.select).collect()
        } else {
            self.facts.get(id).into_iter().flatten().copied().collect()
        };

        let mut connected = Vec::new();
        let mut unwalked = false;
        if let Some(follow) = follow {
            let reachable = self.neighbours(id, &kept, follow);
            if follow.depth == 0 {
                // The ceiling. The neighbours are computed and not returned,
                // which is the one case where an empty `connected` would
                // otherwise read as "nothing is there".
                unwalked = !reachable.is_empty();
            } else {
                let next = Follow {
                    depth: follow.depth - 1,
                    ..follow.clone()
                };
                for (shape, reached) in reachable {
                    // The visited set is per root, so a cycle stops and two
                    // roots that share a neighbour each still report it — and
                    // the object whose edge was not followed says so.
                    if !seen.insert(reached.clone()) {
                        unwalked = true;
                        continue;
                    }
                    let via = Some(Via {
                        shape,
                        direction: follow.direction,
                    });
                    connected.push(self.expand(&reached, via, query, Some(&next), seen));
                }
            }
        }

        Object {
            entity,
            via,
            facts: if query.include.facts {
                kept.into_iter().cloned().collect()
            } else {
                Vec::new()
            },
            prose: query
                .include
                .prose
                .then(|| self.prose.get(id).copied().unwrap_or_default().to_string()),
            connected,
            unwalked,
        }
    }

    /// The entities one hop from `id`, in handle order.
    ///
    /// An edge pointing at a handle no entity answers to is skipped. The write
    /// guard refuses one at the door, so a dangling edge is damage from outside
    /// jojobot; a read verb reports what is there and is not where that gets
    /// repaired.
    fn neighbours(
        &self,
        id: &EntityId,
        from: &[&Fact],
        follow: &Follow,
    ) -> Vec<(EdgeShape, EntityId)> {
        let mut found: Vec<(EdgeShape, EntityId)> = match follow.direction {
            Direction::Out => from
                .iter()
                .filter_map(|f| f.edge.as_ref())
                .filter(|e| follow.shape.is_none_or(|s| e.shape == s))
                .map(|e| (e.shape, e.object.clone()))
                .collect(),
            Direction::In => self
                .inbound
                .get(id)
                .into_iter()
                .flatten()
                .filter(|(shape, _)| follow.shape.is_none_or(|s| *shape == s))
                .cloned()
                .collect(),
        };
        found.retain(|(_, reached)| self.entities.contains_key(reached));
        found.sort_by(|a, b| {
            a.1.as_str()
                .cmp(b.1.as_str())
                .then_with(|| a.0.as_token().cmp(b.0.as_token()))
        });
        found.dedup();
        found
    }
}

/// **The walk, against a store.** One read: the query needs every document's
/// entity, prose, facts and edges, and a walk assembled from several reads
/// could answer from a graph that was never all true at once.
pub async fn walk<M>(store: &M, query: &GraphQuery) -> Result<Vec<Object>, MemoryError>
where
    M: super::Memory + ?Sized,
{
    query.validate()?;
    let scanned = store.scan().await?;
    resolve(&scanned, query)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::{Boot, FactId, FactStatus, Provenance, Standing, event::Event};

    fn entity(handle: &str, name: &str) -> Entity {
        let id = EntityId(handle.to_string());
        Entity {
            kind: id.kind().expect("the fixture uses well-formed handles"),
            id,
            name: name.to_string(),
            aliases: Vec::new(),
            source: "user-named".into(),
            crm: None,
            parent: None,
            boot: Boot::OnDemand,
        }
    }

    fn fact(home: &str, id: &str, content: &str) -> Fact {
        Fact {
            id: FactId(id.to_string()),
            home: EntityId(home.to_string()),
            subject: EntityId(home.to_string()),
            content: content.to_string(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            date: "2026-08-10".parse().expect("a civil date"),
            edge: None,
            event: None,
            derived_from: None,
        }
    }

    /// A doc holding one entity, its prose and its rows.
    fn doc(entity: Entity, prose: &str, facts: Vec<Fact>) -> DocScan {
        DocScan {
            doc_id: entity.id.to_string(),
            title: entity.name.clone(),
            prose: prose.to_string(),
            entity: Some(entity),
            facts,
        }
    }

    /// A fact drawing one edge.
    fn edged(home: &str, id: &str, content: &str, shape: EdgeShape, object: &str) -> Fact {
        Fact {
            edge: Some(Edge::new(shape, EntityId(object.to_string()))),
            ..fact(home, id, content)
        }
    }

    /// Two people at one party held at a place, one page of prose each, and a
    /// record with keys on it. Everything the cases below select, walk and
    /// read.
    ///
    /// **It holds a chain and a cycle on purpose.** Patana attends the party,
    /// the party is at the tavern, and the party points back at Patana — so an
    /// outbound walk has somewhere to go twice, and somewhere to loop.
    fn store() -> Vec<DocScan> {
        let attending = |home: &str, id: &str, content: &str, rsvp: &str| Fact {
            event: Some(Event {
                kind: "rsvp".into(),
                metadata: [("rsvp".to_string(), rsvp.to_string())]
                    .into_iter()
                    .collect(),
                refs: Vec::new(),
            }),
            ..edged(
                home,
                id,
                content,
                EdgeShape::Attendance,
                "event:birthday-party",
            )
        };
        vec![
            doc(
                entity("place:moes", "Moe's Tavern"),
                "The tavern's page.",
                Vec::new(),
            ),
            doc(
                entity("event:birthday-party", "Birthday Party"),
                "The one at Moe's.",
                vec![
                    fact("event:birthday-party", "f1", "starts at eight"),
                    edged(
                        "event:birthday-party",
                        "f2",
                        "held at the tavern",
                        EdgeShape::Location,
                        "place:moes",
                    ),
                    edged(
                        "event:birthday-party",
                        "f3",
                        "Patana is organising it",
                        EdgeShape::About,
                        "person:patana",
                    ),
                ],
            ),
            doc(
                entity("person:patana", "Patana"),
                "Patana's page.",
                vec![
                    attending("person:patana", "f1", "coming to the party", "yes"),
                    fact("person:patana", "f2", "vegetarian"),
                ],
            ),
            doc(
                entity("person:barney-gumble", "Barney Gumble"),
                "Barney's page.",
                vec![attending(
                    "person:barney-gumble",
                    "f1",
                    "cannot make it, away that weekend",
                    "no",
                )],
            ),
            doc(
                entity("person:ned-flanders", "Ned Flanders"),
                "Ned's page.",
                vec![fact("person:ned-flanders", "f1", "left-handed")],
            ),
        ]
    }

    fn handles(found: &[Object]) -> Vec<&str> {
        found.iter().map(|o| o.entity.id.as_str()).collect()
    }

    /// **A kind selects every object of it, and prose comes back whole.**
    ///
    /// The acceptance case, in the general words that serve it: objects of a
    /// kind, with their pages. Nothing here is a bot and nothing is a charter —
    /// a charter IS an entity's prose, so the case falls out of `kind` and
    /// `prose` rather than out of anything that knows what a charter is.
    ///
    /// Paired with the negative it rests on: without `prose` the same query
    /// answers `None`, so "the page came back" is a fact about this query and
    /// not about every query.
    #[test]
    fn a_kind_selects_its_objects_and_prose_comes_back_whole() {
        let scanned = store();
        let query = GraphQuery {
            select: Selection {
                kind: Some(EntityKind::Person),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: true,
            },
            follow: None,
        };
        let found = resolve(&scanned, &query).expect("a kind is a selection");
        assert_eq!(
            handles(&found),
            vec![
                "person:barney-gumble",
                "person:ned-flanders",
                "person:patana"
            ],
            "every person and nothing else, in handle order",
        );
        assert_eq!(
            found[1].prose.as_deref(),
            Some("Ned's page."),
            "the page comes back whole: {:?}",
            found[1],
        );
        assert!(
            found[1].facts.is_empty(),
            "facts were not asked for: {:?}",
            found[1],
        );

        let unasked = resolve(
            &scanned,
            &GraphQuery {
                include: Include::default(),
                ..query.clone()
            },
        )
        .expect("the same selection");
        assert_eq!(
            unasked[1].prose, None,
            "prose that was not asked for is absent rather than empty: {:?}",
            unasked[1],
        );
    }

    /// **A key and its value select the objects holding such a record**, and
    /// the value half is what makes it a filter rather than a key check.
    ///
    /// Both directions in one case: the same key with the other value selects
    /// the other object. A build that matched on the key alone passes the
    /// first assertion and fails the second.
    #[test]
    fn a_key_and_its_value_select_the_objects_that_hold_it() {
        let scanned = store();
        let by_value = |value: &str| {
            let query = GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::holding("rsvp", value)],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            };
            resolve(&scanned, &query).expect("a key filter is a selection")
        };
        assert_eq!(handles(&by_value("yes")), vec!["person:patana"]);
        assert_eq!(handles(&by_value("no")), vec!["person:barney-gumble"]);

        let any = resolve(
            &scanned,
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::key("rsvp")],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a key alone is a selection");
        assert_eq!(
            handles(&any),
            vec!["person:barney-gumble", "person:patana"],
            "the key with no value asked for holds for either value",
        );
        assert_eq!(
            any[1].facts.len(),
            1,
            "the object comes back with the records that answered, not its whole page: {:?}",
            any[1],
        );
    }

    /// **A filter and a kind combine into one set**, rather than being two
    /// questions asked in turn.
    #[test]
    fn a_kind_and_a_key_narrow_one_set() {
        let scanned = store();
        let query = |kind: EntityKind| GraphQuery {
            select: Selection {
                kind: Some(kind),
                fields: vec![FieldFilter::key("rsvp")],
                ..Selection::default()
            },
            ..GraphQuery::default()
        };
        assert_eq!(
            handles(&resolve(&scanned, &query(EntityKind::Person)).expect("both filters")),
            vec!["person:barney-gumble", "person:patana"],
        );
        assert!(
            resolve(&scanned, &query(EntityKind::Place))
                .expect("both filters")
                .is_empty(),
            "the kind narrows the same set the key does, so a kind holding no such record is empty",
        );
    }

    /// **A type selects structurally**, over the keys a record carries and
    /// never over what anybody declared it to be.
    #[test]
    fn a_type_selects_the_records_that_answer_it() {
        let scanned = store();
        let declared = types::DeclaredType::new(
            "reply",
            vec![types::Field::new("rsvp", types::ValueType::Text)],
        );
        let found = resolve(
            &scanned,
            &GraphQuery {
                select: Selection {
                    answers_type: Some(declared),
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a type is a selection");
        assert_eq!(
            handles(&found),
            vec!["person:barney-gumble", "person:patana"],
        );

        let unheld = types::DeclaredType::new(
            "shipment",
            vec![types::Field::new("weight", types::ValueType::Number)],
        );
        assert!(
            resolve(
                &scanned,
                &GraphQuery {
                    select: Selection {
                        answers_type: Some(unheld),
                        ..Selection::default()
                    },
                    ..GraphQuery::default()
                },
            )
            .expect("a type is a selection")
            .is_empty(),
            "a type no record answers selects nothing — or 'it matched' says nothing",
        );
    }

    /// **The walk goes both ways, and the answer nests.**
    ///
    /// From the party inbound reaches its guests; from a guest outbound reaches
    /// the party. One edge, two questions, and a walk that could only go one
    /// way would answer one of them.
    #[test]
    fn a_walk_leaves_by_either_end_of_an_edge() {
        let scanned = store();
        let from_party = |direction: Direction| {
            resolve(
                &scanned,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("event:birthday-party".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        shape: Some(EdgeShape::Attendance),
                        direction,
                        depth: 1,
                    }),
                },
            )
            .expect("a subject with a walk")
        };

        let inbound = from_party(Direction::In);
        assert_eq!(
            handles(&inbound[0].connected),
            vec!["person:barney-gumble", "person:patana"],
            "the guests hang off the party: {:?}",
            inbound[0],
        );
        assert_eq!(
            inbound[0].connected[0].via,
            Some(Via {
                shape: EdgeShape::Attendance,
                direction: Direction::In,
            }),
            "a reached object says how the walk got to it",
        );
        assert_eq!(inbound[0].via, None, "a root was reached by nothing");

        assert!(
            from_party(Direction::Out)[0].connected.is_empty(),
            "the party's own record draws no edge, so outbound reaches nobody",
        );

        let from_guest = resolve(
            &scanned,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:patana".into())),
                    ..Selection::default()
                },
                follow: Some(Follow::hop()),
                ..GraphQuery::default()
            },
        )
        .expect("a subject with a walk");
        assert_eq!(
            handles(&from_guest[0].connected),
            vec!["event:birthday-party"],
            "and outbound from the guest reaches the party",
        );
    }

    /// **The question a flat list cannot answer, in one call**: what do my
    /// guests eat. One hop from the party reaches the guests, and each of them
    /// arrives carrying their own page.
    ///
    /// The load-bearing half is that a reached object brings ITS facts, not
    /// the root's — a walk that returned bare handles would leave the caller
    /// asking again per guest, which is the two-call shape this replaces.
    #[test]
    fn a_reached_object_brings_its_own_facts() {
        let found = resolve(
            &store(),
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("event:birthday-party".into())),
                    ..Selection::default()
                },
                include: Include::default(),
                follow: Some(Follow {
                    shape: Some(EdgeShape::Attendance),
                    direction: Direction::In,
                    depth: 1,
                }),
            },
        )
        .expect("a walk of one hop");

        let guests = &found[0].connected;
        assert_eq!(
            handles(guests),
            vec!["person:barney-gumble", "person:patana"],
        );
        let patana = guests
            .iter()
            .find(|o| o.entity.id.as_str() == "person:patana")
            .expect("the guest who is coming");
        assert!(
            patana.facts.iter().any(|f| f.content == "vegetarian"),
            "the guest arrives with her own page: {patana:?}",
        );
        assert!(
            found[0]
                .facts
                .iter()
                .any(|f| f.content == "starts at eight"),
            "and the root still carries its own: {:?}",
            found[0],
        );
    }

    /// **A second hop is a second hop.** From a guest, to the party she is
    /// attending, to the place it is held — three objects nested two deep, in
    /// one call.
    ///
    /// Paired with the same walk one hop shallower, which reaches the party
    /// and stops. Without that negative the case passes on a build that walks
    /// forever and on one that ignores `depth` entirely.
    #[test]
    fn a_two_hop_walk_returns_the_shape_and_not_a_list() {
        let scanned = store();
        let from_patana = |depth: usize| {
            resolve(
                &scanned,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:patana".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        shape: None,
                        direction: Direction::Out,
                        depth,
                    }),
                },
            )
            .expect("a walk out of a subject")
        };

        let two = from_patana(2);
        assert_eq!(handles(&two[0].connected), vec!["event:birthday-party"]);
        assert_eq!(
            handles(&two[0].connected[0].connected),
            vec!["place:moes"],
            "the second hop reaches where the party is held: {two:?}",
        );
        assert_eq!(
            two[0].connected[0].connected[0].via,
            Some(Via {
                shape: EdgeShape::Location,
                direction: Direction::Out,
            }),
            "and it says which edge it came along",
        );

        assert!(
            from_patana(1)[0].connected[0].connected.is_empty(),
            "one hop stops at the party: {:?}",
            from_patana(1),
        );
    }

    /// **A walk that stopped says so, and one that ran out of graph does
    /// not.** An empty `connected` otherwise means two different things —
    /// the hops ran out, or there is nothing there — and a reader who has to
    /// infer which will eventually infer wrong.
    ///
    /// Three states in one case, because the flag is only worth anything if it
    /// is off when it should be: cut short at the ceiling, cut short by a
    /// neighbour already in the answer, and genuinely at the end of the graph.
    #[test]
    fn an_object_says_when_it_has_edges_nobody_followed() {
        let scanned = store();
        let from_patana = |depth: usize| {
            resolve(
                &scanned,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:patana".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        shape: None,
                        direction: Direction::Out,
                        depth,
                    }),
                },
            )
            .expect("a walk out of a subject")
        };

        let one = from_patana(1);
        let party = &one[0].connected[0];
        assert!(
            party.connected.is_empty() && party.unwalked,
            "the party was reached on the last hop and its own edges were not followed: {party:?}",
        );
        assert!(
            !one[0].unwalked,
            "and the root's one edge WAS followed, so nothing was left out there: {:?}",
            one[0],
        );

        let two = from_patana(2);
        let party = &two[0].connected[0];
        assert!(
            party.unwalked,
            "at two hops the party still points back at Patana, who is already in the answer: \
             {party:?}",
        );
        assert!(
            !party.connected[0].unwalked,
            "and the tavern draws no edge at all, so nothing was left out there: {party:?}",
        );
    }

    /// **A cycle stops.** Patana attends the party and the party points back
    /// at Patana, so an outbound walk at the ceiling would loop forever if
    /// nothing held what it had already reached.
    ///
    /// It asserts where the walk actually ends rather than only that it
    /// returned: a build that stopped after one hop for the wrong reason would
    /// satisfy "it terminated".
    #[test]
    fn a_walk_over_a_cycle_terminates() {
        let found = resolve(
            &store(),
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:patana".into())),
                    ..Selection::default()
                },
                include: Include {
                    facts: false,
                    prose: false,
                },
                follow: Some(Follow {
                    shape: None,
                    direction: Direction::Out,
                    depth: MAX_DEPTH,
                }),
            },
        )
        .expect("a walk at the ceiling");

        let party = &found[0].connected;
        assert_eq!(handles(party), vec!["event:birthday-party"]);
        assert_eq!(
            handles(&party[0].connected),
            vec!["place:moes"],
            "the party points back at Patana and at the tavern; only the tavern is new: {found:?}",
        );
        assert!(
            party[0].connected[0].connected.is_empty(),
            "and the tavern draws no edge, so the walk ends there: {found:?}",
        );
    }

    /// **A named subject comes back even when the filters keep none of its
    /// facts** — naming a handle asks for that object, and a filter asks which
    /// objects. Paired with the miss it must not be confused with.
    #[test]
    fn a_named_subject_is_not_filtered_away_and_an_unknown_one_is_a_miss() {
        let scanned = store();
        let found = resolve(
            &scanned,
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:ned-flanders".into())),
                    fields: vec![FieldFilter::holding("rsvp", "yes")],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a named subject");
        assert_eq!(handles(&found), vec!["person:ned-flanders"]);
        assert!(
            found[0].facts.is_empty(),
            "and it comes back with the records that answered, which is none: {:?}",
            found[0],
        );

        let missed = resolve(
            &scanned,
            &GraphQuery::subject(EntityId("person:ned-flander".into())),
        )
        .expect_err("a handle that names nothing is a miss");
        match missed {
            MemoryError::UnknownEntity { attempted, nearest } => {
                assert_eq!(attempted, "person:ned-flander");
                assert_eq!(
                    nearest.first().map(|n| n.handle.as_str()),
                    Some("person:ned-flanders"),
                    "the near candidate comes back: {nearest:?}",
                );
            }
            other => panic!("a miss must name the handle and its candidates: {other:?}"),
        }
    }

    /// **A query that narrows nothing is refused**, and so is a walk of no
    /// hops and one past the ceiling. Each is a malformed argument rather than
    /// an unlucky one, so each reads as the caller's mistake instead of as an
    /// honest empty answer.
    #[test]
    fn a_query_that_narrows_nothing_is_refused() {
        let refused = |query: GraphQuery| {
            resolve(&store(), &query).expect_err("this query cannot be served");
        };
        refused(GraphQuery::default());
        refused(GraphQuery {
            select: Selection {
                kind: Some(EntityKind::Person),
                ..Selection::default()
            },
            follow: Some(Follow {
                depth: 0,
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        });
        refused(GraphQuery {
            select: Selection {
                kind: Some(EntityKind::Person),
                ..Selection::default()
            },
            follow: Some(Follow {
                depth: MAX_DEPTH + 1,
                ..Follow::hop()
            }),
            ..GraphQuery::default()
        });

        // The positive the three refusals rest on: the same walk one hop
        // shallower is served, so "refused" is about the argument and not
        // about walks.
        resolve(
            &store(),
            &GraphQuery {
                select: Selection {
                    kind: Some(EntityKind::Person),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    depth: MAX_DEPTH,
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a walk within the ceiling is served");
    }

    /// **A fact homed on one page and about another belongs to both.** The
    /// rule `recall` already answers by, kept here so one record does not
    /// belong to an entity through one verb and not the other.
    #[test]
    fn a_fact_belongs_to_its_subject_and_to_the_page_it_sits_on() {
        let mut scanned = store();
        let visiting = Fact {
            subject: EntityId("person:patana".into()),
            ..fact("event:birthday-party", "f2", "brings the pudding")
        };
        scanned[0].facts.push(visiting);

        let says = |handle: &str| {
            resolve(&scanned, &GraphQuery::subject(EntityId(handle.into())))
                .expect("a subject")
                .swap_remove(0)
                .facts
                .iter()
                .any(|f| f.content == "brings the pudding")
        };
        assert!(says("person:patana"), "it is about her");
        assert!(says("event:birthday-party"), "and it sits on the party");
    }
}
