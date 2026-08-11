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
    /// **How the two values are compared**, and it is licensed by what the key
    /// was declared to hold rather than chosen freely. Equality needs no
    /// declaration and is the default.
    pub compare: types::Compare,
}

impl FieldFilter {
    /// A filter asking only that the key is there.
    pub fn key(key: &str) -> Self {
        FieldFilter {
            key: key.trim().to_string(),
            value: None,
            compare: types::Compare::Equals,
        }
    }

    /// A filter asking that the key holds this value.
    pub fn holding(key: &str, value: &str) -> Self {
        FieldFilter {
            key: key.trim().to_string(),
            value: Some(value.trim().to_string()),
            compare: types::Compare::Equals,
        }
    }

    /// A filter asking that the key's value stands in some relation to this
    /// one — the ordering a declaration licenses.
    pub fn comparing(key: &str, compare: types::Compare, value: &str) -> Self {
        FieldFilter {
            compare,
            ..FieldFilter::holding(key, value)
        }
    }

    /// Does this record satisfy the filter.
    fn satisfied_by(&self, record: &BTreeMap<String, String>) -> bool {
        match (record.get(&self.key), &self.value) {
            (None, _) => false,
            (Some(_), None) => true,
            (Some(held), Some(wanted)) => self.compare.holds_between(held, wanted),
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

/// **What a walk travels along.**
///
/// Two vocabularies, and they are not the same thing. An **edge shape** is one
/// of the five names for a link nobody typed. A **relation** is a link a
/// declaration made: a key some type declared to hold a reference, which is a
/// link because the declaration says the value is another entity rather than a
/// string that looks like one.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Along {
    /// Any edge at all — "whatever it is connected to".
    #[default]
    AnyEdge,
    /// One edge shape.
    Edge(EdgeShape),
    /// **A declared relation: the KEY some type declared to hold a reference.**
    /// [`Follow::direction`] says which way to travel it.
    Relation(String),
}

/// **What a walk travelled along to reach an object.** The answer's half of
/// [`Along`], and it names one link rather than a set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Link {
    /// An edge, by its shape.
    Edge(EdgeShape),
    /// A declared relation, by the name it was followed under.
    Relation(String),
}

impl Link {
    /// The token this reads as — the shape's own, or the relation's name.
    pub fn token(&self) -> &str {
        match self {
            Link::Edge(shape) => shape.as_token(),
            Link::Relation(name) => name.as_str(),
        }
    }
}

/// **Which edges to walk, and how far.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    /// What to travel along: any edge, one shape, or a declared relation.
    pub along: Along,
    /// **Which end to leave by**, for a walk along edges. `None` is outbound.
    ///
    /// It is optional because a relation's NAME already says which way it goes,
    /// so a direction beside one is an argument this verb does not implement —
    /// refused rather than silently dropped, which needs the two to be
    /// distinguishable from each other.
    pub direction: Option<Direction>,
    /// How many hops. One is the neighbours; two is the neighbours' neighbours.
    pub depth: usize,
    /// **What the walk keeps of what it reaches.** Empty keeps everything,
    /// which is what a walk did before it could filter.
    ///
    /// Each filter describes a RECORD, exactly as a selection's do: an object
    /// is reached when one of its records answers every filter, and it arrives
    /// carrying the records that answered.
    pub keeping: Vec<FieldFilter>,
}

impl Follow {
    /// One hop along every edge, outbound.
    pub fn hop() -> Self {
        Follow {
            along: Along::AnyEdge,
            direction: None,
            depth: 1,
            keeping: Vec::new(),
        }
    }

    /// The direction this walk leaves by. Outbound unless it says otherwise,
    /// and a relation's own name overrules it.
    fn direction(&self) -> Direction {
        self.direction.unwrap_or_default()
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

    /// **Does answering this need to know what has been declared.**
    ///
    /// A relation is a relation because a declaration says so, and an ordering
    /// is licensed by one. Nothing else here has any use for them.
    pub fn needs_declarations(&self) -> bool {
        let ordered =
            |filters: &[FieldFilter]| filters.iter().any(|f| f.compare.licensed_by().is_some());
        ordered(&self.select.fields)
            || self
                .follow
                .as_ref()
                .is_some_and(|f| matches!(f.along, Along::Relation(_)) || ordered(&f.keeping))
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
        for field in self.follow.iter().flat_map(|f| &f.keeping) {
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

/// **A relation is a KEY that some declaration says holds a reference.**
///
/// The name is derived rather than configured: it is the key itself, and the
/// direction says which way to travel it. Out of a record, the key reaches the
/// handle it holds; in to an entity, the key reaches every record pointing at
/// it.
///
/// **A declaration establishes the key IS a relation, and does nothing else
/// here.** It used to also scope the reverse walk to one type, under the name
/// `type.key`. That scope could never exclude anything: the walked key is by
/// construction one of the type's keys, and a record answers a type when it
/// holds ONE of them, so the record carrying the key always answered the type.
/// The name promised a narrowing the code could not perform, so both are gone.
///
/// **Nothing is inferred.** A value that looks like a handle under a key nobody
/// declared is a string that looks like a handle.
fn relation_key<'a>(name: &str, declarations: &'a [types::DeclaredType]) -> Option<&'a str> {
    let name = name.trim();
    declarations
        .iter()
        .find_map(|d| d.field(name))
        .filter(|f| f.holds == types::ValueType::Reference)
        .map(|f| f.key.as_str())
}

/// **Every relation these declarations make followable**, in name order — what
/// a caller who named one that is not there gets offered instead.
fn relation_names(declarations: &[types::DeclaredType]) -> Vec<String> {
    let mut found: Vec<String> = declarations
        .iter()
        .flat_map(|d| &d.fields)
        .filter(|f| f.holds == types::ValueType::Reference)
        .map(|f| f.key.clone())
        .collect();
    found.sort();
    found.dedup();
    found
}

/// **The half of a query's validation that needs to know what was declared.**
///
/// It is separate because the rest is answerable from the arguments alone and
/// runs before any read: a walk of no hops is malformed whatever the store
/// holds, while a relation nobody declared can only be judged against the
/// declarations.
fn check_declared(
    query: &GraphQuery,
    declarations: &[types::DeclaredType],
) -> Result<(), MemoryError> {
    let filters = query
        .select
        .fields
        .iter()
        .chain(query.follow.iter().flat_map(|f| &f.keeping));
    for field in filters {
        let Some(holds) = field.compare.licensed_by() else {
            continue;
        };
        let Some(wanted) = field.value.as_deref() else {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' compares against a value and none was given",
                field.compare.as_token(),
            )));
        };
        if !field.compare.can_ask_for(wanted) {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' asks about {}s and '{wanted}' is not one",
                field.compare.as_token(),
                holds.as_token(),
            )));
        }
        // **The declaration is what licenses the operator.** A key nobody
        // declared keeps equality, which is why this is refused rather than
        // quietly answered: a caller who asked for an ordering and got equality
        // would read the answer as an ordering.
        if !declarations
            .iter()
            .any(|d| d.field(&field.key).is_some_and(|f| f.holds == holds))
        {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' needs a type declaring '{}' to hold a {}. Declare one, or ask for the value \
                 itself",
                field.compare.as_token(),
                field.key,
                holds.as_token(),
            )));
        }
    }
    if let Some(Along::Relation(name)) = query.follow.as_ref().map(|f| &f.along)
        && relation_key(name, declarations).is_none()
    {
        let known = relation_names(declarations);
        return Err(MemoryError::InvalidQuery(format!(
            "no declaration makes '{name}' a relation. A relation is a KEY some type declared to \
             hold a reference: walk it outbound to reach what a record points at, or inbound to \
             reach the records pointing here. {}",
            if known.is_empty() {
                "No type declares a reference key yet".to_string()
            } else {
                format!("There is: {}", known.join(", "))
            },
        )));
    }
    Ok(())
}

/// **How a walk reached an object.** Absent on a root, which nothing reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Via {
    /// What the walk travelled along.
    pub link: Link,
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
pub fn resolve(
    scanned: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<Vec<Object>, MemoryError> {
    query.validate()?;
    check_declared(query, declarations)?;
    let ctx = Ctx::of(scanned, declarations);
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
    /// Every fact once, whatever page it sits on — what a reverse relation
    /// reads, because it asks who points here and the answer is on their pages
    /// rather than on this one.
    all: Vec<&'a Fact>,
    /// What has been declared, which is what makes a key a relation.
    declarations: &'a [types::DeclaredType],
}

impl<'a> Ctx<'a> {
    fn of(scanned: &'a [DocScan], declarations: &'a [types::DeclaredType]) -> Self {
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

        // **A record can arrive twice.** These documents are assembled from
        // per-entity reads, and a record about one entity that sits on
        // another's page belongs to both — so both reads return it. Its
        // address is what says it is one record.
        for bucket in facts.values_mut() {
            let mut seen = HashSet::new();
            bucket.retain(|f| seen.insert((f.home.clone(), f.id.clone())));
        }
        for bucket in inbound.values_mut() {
            let mut seen = HashSet::new();
            bucket.retain(|link| seen.insert(link.clone()));
        }

        let mut all: Vec<&Fact> = scanned.iter().flat_map(|doc| &doc.facts).collect();
        let mut seen = HashSet::new();
        all.retain(|f| seen.insert((f.home.clone(), f.id.clone())));

        let mut index: Vec<Entity> = entities.values().map(|e| (*e).clone()).collect();
        index.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ctx {
            entities,
            index,
            prose,
            facts,
            inbound,
            all,
            declarations,
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
        // **The selection describes the roots; the walk's own filters describe
        // what it reaches.** Both work the same way — an object arrives with
        // the records that answered — and a walk that keeps everything hands a
        // neighbour its page whole, which is what a walk did before it could
        // filter.
        let kept: Vec<&Fact> = if via.is_none() {
            self.kept_facts(id, &query.select).collect()
        } else {
            self.reached_facts(id, query.follow.as_ref())
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
                for (link, direction, reached) in reachable {
                    // An object the walk's filters do not keep is one this
                    // answer does not carry, so the object that pointed at it
                    // says an edge of its own went unfollowed — the same claim
                    // the ceiling makes, for a different reason.
                    //
                    // **A walk that filters nothing admits everything**, an
                    // object with no records at all included: having nothing
                    // written about it is not the same as failing a filter, and
                    // an admission test that could not tell them apart would
                    // drop the far end of every edge into a bare place.
                    if !follow.keeping.is_empty()
                        && self.reached_facts(&reached, Some(follow)).is_empty()
                    {
                        unwalked = true;
                        continue;
                    }
                    // The visited set is per root, so a cycle stops and two
                    // roots that share a neighbour each still report it — and
                    // the object whose edge was not followed says so.
                    if !seen.insert(reached.clone()) {
                        unwalked = true;
                        continue;
                    }
                    let via = Some(Via { link, direction });
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

    /// The facts a reached object brings: those answering the walk's own
    /// filters, or all of them when it keeps everything.
    ///
    /// It is also the test of whether the object is reached at all — an object
    /// with no record answering is not in the neighbourhood the caller
    /// described.
    fn reached_facts(&self, id: &EntityId, follow: Option<&Follow>) -> Vec<&'a Fact> {
        let mine = self.facts.get(id).into_iter().flatten().copied();
        match follow.map(|f| f.keeping.as_slice()).unwrap_or_default() {
            [] => mine.collect(),
            keeping => mine
                .filter(|f| {
                    f.event
                        .as_ref()
                        .is_some_and(|e| keeping.iter().all(|k| k.satisfied_by(&e.metadata)))
                })
                .collect(),
        }
    }

    /// The entities one hop from `id`, in handle order, each with the link the
    /// walk travelled along and the way it went.
    ///
    /// A link pointing at a handle no entity answers to is skipped. The write
    /// guard refuses an edge like that at the door, so a dangling one is damage
    /// from outside jojobot; a read verb reports what is there and is not where
    /// that gets repaired. **A reference key is not guarded that way at all** —
    /// nothing checks a record's values against a declaration — so a key
    /// holding a handle nobody has created is ordinary and drops out here.
    fn neighbours(
        &self,
        id: &EntityId,
        from: &[&Fact],
        follow: &Follow,
    ) -> Vec<(Link, Direction, EntityId)> {
        let mut found: Vec<(Link, Direction, EntityId)> = match &follow.along {
            Along::Relation(name) => self.along_relation(id, from, name, follow.direction()),
            along => {
                let shape = match along {
                    Along::Edge(shape) => Some(*shape),
                    _ => None,
                };
                let direction = follow.direction();
                match direction {
                    Direction::Out => from
                        .iter()
                        .filter_map(|f| f.edge.as_ref())
                        .filter(|e| shape.is_none_or(|s| e.shape == s))
                        .map(|e| (Link::Edge(e.shape), direction, e.object.clone()))
                        .collect(),
                    Direction::In => self
                        .inbound
                        .get(id)
                        .into_iter()
                        .flatten()
                        .filter(|(shape_at, _)| shape.is_none_or(|s| *shape_at == s))
                        .map(|(shape_at, drawn_by)| {
                            (Link::Edge(*shape_at), direction, drawn_by.clone())
                        })
                        .collect(),
                }
            }
        };
        found.retain(|(_, _, reached)| self.entities.contains_key(reached));
        found.sort_by(|a, b| {
            a.2.as_str()
                .cmp(b.2.as_str())
                .then_with(|| a.0.token().cmp(b.0.token()))
        });
        found.dedup();
        found
    }

    /// The entities a declared relation reaches from `id`, in the direction
    /// asked for.
    ///
    /// **Out** reads this object's own records: the key holds a handle, and
    /// that handle is where the link goes. **In** reads everybody else's: every
    /// record pointing that key at this object, and the link goes to whoever
    /// each record is about.
    ///
    /// **The inbound walk is scoped by the KEY and by nothing else.** It reaches
    /// every record pointing here through that key, whatever else the record is
    /// — which is what "everything pointing at me through `owner`" means, and is
    /// not the same question as "my pets".
    fn along_relation(
        &self,
        id: &EntityId,
        from: &[&Fact],
        name: &str,
        direction: Direction,
    ) -> Vec<(Link, Direction, EntityId)> {
        // ⚠️ **UNREACHABLE, AND DELIBERATELY KEPT.** No test reaches this
        // branch, and the comment exists so the next reader knows the coverage
        // is absent rather than assuming it: `check_declared` refuses a
        // relation no declaration backs before any walk starts, so a name that
        // gets this far has already resolved once.
        //
        // It stays because the refusal and the lookup are in different
        // functions, and an empty walk is the right answer if they ever
        // disagree. Nothing downstream depends on it.
        let Some(key) = relation_key(name, self.declarations) else {
            return Vec::new();
        };
        let link = || Link::Relation(key.to_string());
        match direction {
            Direction::Out => from
                .iter()
                .filter_map(|f| f.event.as_ref())
                .filter_map(|e| e.metadata.get(key))
                .map(|handle| (link(), direction, EntityId(handle.trim().to_string())))
                .collect(),
            Direction::In => self
                .all
                .iter()
                .filter(|f| {
                    f.event.as_ref().is_some_and(|e| {
                        e.metadata.get(key).is_some_and(|v| v.trim() == id.as_str())
                    })
                })
                .map(|f| (link(), direction, f.subject.clone()))
                .collect(),
        }
    }
}

/// **The walk, against a store — through TARGETED reads, never the boot scan.**
///
/// The scan is the search index's read, and reading it here would tie this verb
/// to the index's fate: a store the index cannot scan is exactly when a caller
/// needs the verb that reads past it, and one built on `scan` would fail in the
/// same breath. So this assembles what the query needs out of
/// [`list_entities`](super::Memory::list_entities),
/// [`recall`](super::Memory::recall) and
/// [`scan_entity`](super::Memory::scan_entity).
///
/// **It reads only what the query asks for.** One handle costs the entity index
/// and that handle's records. A walk costs every entity's records, because
/// which objects it reaches is not known until it has been walked, and an
/// inbound walk has to ask who points here.
pub async fn walk<M>(store: &M, query: &GraphQuery) -> Result<Vec<Object>, MemoryError>
where
    M: super::Memory + ?Sized,
{
    query.validate()?;
    // The entity index: what exists, what a kind selects, and what a handle
    // that names nothing is screened against.
    let entities = store.list_entities(None).await?;
    let select = &query.select;
    if let Some(subject) = &select.subject
        && !entities.iter().any(|e| &e.id == subject)
    {
        return Err(MemoryError::UnknownEntity {
            attempted: subject.to_string(),
            nearest: guard::screen(subject, &[], &entities),
        });
    }

    // **Whose records are needed.** A walk cannot know where it will go, and a
    // filter over records cannot choose objects without reading theirs — both
    // need all of them. Everything else needs only what it named.
    let everyones = query.follow.is_some() || select.filters_facts();
    let wanted: Vec<&Entity> = entities
        .iter()
        .filter(|e| {
            everyones
                || select.subject.as_ref() == Some(&e.id)
                || (select.subject.is_none() && select.kind.is_none_or(|k| e.kind == k))
        })
        .collect();

    // A document per entity, so a kind selection and a near-miss screen see
    // everything that exists — with records and prose filled in only for the
    // entities this query actually reads.
    let mut scanned: Vec<DocScan> = Vec::with_capacity(entities.len());
    for entity in &entities {
        let read = wanted.iter().any(|w| w.id == entity.id);
        let facts = if read {
            store.recall(&entity.id).await?
        } else {
            Vec::new()
        };
        let prose = if read && query.include.prose {
            store
                .scan_entity(&entity.id)
                .await?
                .map(|doc| doc.prose)
                .unwrap_or_default()
        } else {
            String::new()
        };
        scanned.push(DocScan {
            doc_id: entity.id.to_string(),
            title: entity.name.clone(),
            prose,
            facts,
            entity: Some(entity.clone()),
        });
    }
    // **Read only for the questions that need it.** A declaration is what makes
    // a key a relation and what licenses an ordering, and a query asking for
    // neither is answered without it.
    let declarations = if query.needs_declarations() {
        store.declared_types().await?
    } else {
        Vec::new()
    };
    resolve(&scanned, &declarations, query)
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
        let found = resolve(&scanned, &[], &query).expect("a kind is a selection");
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
            &[],
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
            resolve(&scanned, &[], &query).expect("a key filter is a selection")
        };
        assert_eq!(handles(&by_value("yes")), vec!["person:patana"]);
        assert_eq!(handles(&by_value("no")), vec!["person:barney-gumble"]);

        let any = resolve(
            &scanned,
            &[],
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
            handles(&resolve(&scanned, &[], &query(EntityKind::Person)).expect("both filters")),
            vec!["person:barney-gumble", "person:patana"],
        );
        assert!(
            resolve(&scanned, &[], &query(EntityKind::Place))
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
            &[],
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
                &[],
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
                &[],
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
                        along: Along::Edge(EdgeShape::Attendance),
                        direction: Some(direction),
                        depth: 1,
                        keeping: Vec::new(),
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
                link: Link::Edge(EdgeShape::Attendance),
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
            &[],
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
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("event:birthday-party".into())),
                    ..Selection::default()
                },
                include: Include::default(),
                follow: Some(Follow {
                    along: Along::Edge(EdgeShape::Attendance),
                    direction: Some(Direction::In),
                    depth: 1,
                    keeping: Vec::new(),
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
                &[],
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
                        along: Along::AnyEdge,
                        direction: Some(Direction::Out),
                        depth,
                        keeping: Vec::new(),
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
                link: Link::Edge(EdgeShape::Location),
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
                &[],
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
                        along: Along::AnyEdge,
                        direction: Some(Direction::Out),
                        depth,
                        keeping: Vec::new(),
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
            &[],
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
                    along: Along::AnyEdge,
                    direction: Some(Direction::Out),
                    depth: MAX_DEPTH,
                    keeping: Vec::new(),
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
            &[],
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
            &[],
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
            resolve(&store(), &[], &query).expect_err("this query cannot be served");
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
            &[],
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

    /// The type the relation cases declare: a pet, whose `owner` is a
    /// reference and whose `born` is a date.
    fn pet() -> types::DeclaredType {
        types::DeclaredType::new(
            "pet",
            vec![
                types::Field::new("name", types::ValueType::Text),
                types::Field::new("born", types::ValueType::Date),
                types::Field::new("weight", types::ValueType::Number),
                types::Field::new("owner", types::ValueType::Reference),
            ],
        )
    }

    /// One owner and two pets, each pet's record pointing at the owner through
    /// a key the declaration calls a reference.
    fn kennel() -> Vec<DocScan> {
        let pet_record = |home: &str, id: &str, content: &str, born: &str, weight: &str| Fact {
            event: Some(Event {
                kind: "pet".into(),
                metadata: [
                    ("name".to_string(), home.to_string()),
                    ("born".to_string(), born.to_string()),
                    ("weight".to_string(), weight.to_string()),
                    ("owner".to_string(), "person:bart".to_string()),
                ]
                .into_iter()
                .collect(),
                refs: Vec::new(),
            }),
            ..fact(home, id, content)
        };
        vec![
            doc(entity("person:bart", "Bart"), "Bart's page.", Vec::new()),
            doc(
                entity("pet:santas-little-helper", "Santa's Little Helper"),
                "The greyhound's page.",
                vec![pet_record(
                    "pet:santas-little-helper",
                    "f1",
                    "the greyhound",
                    "2019-04-15",
                    "27",
                )],
            ),
            doc(
                entity("pet:snowball", "Snowball"),
                "The cat's page.",
                vec![pet_record(
                    "pet:snowball",
                    "f1",
                    "the cat",
                    "2024-11-02",
                    "4",
                )],
            ),
            // A thing that is no pet and carries the same key anyway. It is
            // what makes the inbound walk's scope legible: `owner` reaches it,
            // because `owner` is what the walk was asked for.
            doc(
                entity("thing:red-bike", "The Red Bike"),
                "The bike's page.",
                vec![Fact {
                    event: Some(Event {
                        kind: "repair".into(),
                        metadata: [
                            ("fitted".to_string(), "2026-02-01".to_string()),
                            ("owner".to_string(), "person:bart".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                        refs: Vec::new(),
                    }),
                    ..fact("thing:red-bike", "f1", "needs new brake pads")
                }],
            ),
            // …and one carrying no record at all, so "the relation reached it"
            // still means something.
            doc(
                entity("thing:floor-pump", "The Floor Pump"),
                "The pump's page.",
                vec![fact("thing:floor-pump", "f1", "reseated the hose")],
            ),
        ]
    }

    /// **A declared reference key IS a walkable link, both ways.**
    ///
    /// One name and two directions: out of the pet, `owner` reaches the person;
    /// in to the person, `owner` reaches the records naming them. Nobody
    /// declares an inverse, because there is nothing to declare — the same key
    /// walked the other way is the other question.
    #[test]
    fn a_declared_reference_key_is_a_walkable_link_both_ways() {
        let scanned = kennel();
        let declarations = vec![pet()];
        let walk = |from: &str, relation: &str, direction: Direction| {
            resolve(
                &scanned,
                &declarations,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId(from.into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                },
            )
            .expect("a declared relation is followable")
        };

        let forward = walk("pet:santas-little-helper", "owner", Direction::Out);
        assert_eq!(
            handles(&forward[0].connected),
            vec!["person:bart"],
            "the key reaches what it points at: {forward:?}",
        );
        assert_eq!(
            forward[0].connected[0].via,
            Some(Via {
                link: Link::Relation("owner".into()),
                direction: Direction::Out,
            }),
            "and the reached object says it came along the relation, outbound",
        );

        let reverse = walk("person:bart", "owner", Direction::In);
        assert_eq!(
            handles(&reverse[0].connected),
            vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
            "and the same key walked inbound reaches every record pointing here: {reverse:?}",
        );
        assert_eq!(
            reverse[0].connected[0].via.as_ref().map(|v| v.direction),
            Some(Direction::In),
            "which the walk went inbound to reach",
        );
    }

    /// **A relation is a relation because a declaration says so.** The same
    /// store, the same records, the same key — and with nothing declared, the
    /// name reaches nothing and says so rather than answering empty.
    ///
    /// This is the case that stops the walk inferring a link from a value that
    /// looks like a handle.
    #[test]
    fn nothing_is_a_relation_until_a_declaration_says_it_is() {
        let query = GraphQuery {
            select: Selection {
                subject: Some(EntityId("pet:santas-little-helper".into())),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: false,
            },
            follow: Some(Follow {
                along: Along::Relation("owner".into()),
                ..Follow::hop()
            }),
        };
        resolve(&kennel(), &[], &query).expect_err(
            "with nothing declared, a key holding a handle is a string that looks like one",
        );

        // The same name, declared as ordinary text rather than a reference: the
        // value is identical and it is still not a link.
        let as_text = types::DeclaredType::new(
            "pet",
            vec![types::Field::new("owner", types::ValueType::Text)],
        );
        resolve(&kennel(), std::slice::from_ref(&as_text), &query)
            .expect_err("a key declared to hold text is not a relation");

        // …and the positive in the same case, so the two refusals are about the
        // declaration and not about relations.
        resolve(&kennel(), &[pet()], &query).expect("declared as a reference, it is one");
    }

    /// **Two reference keys of one type stay apart, because they are two
    /// KEYS.** A trip's `from` and its `to` both point at places, and each name
    /// reaches its own end — inbound as well as outbound.
    #[test]
    fn two_reference_keys_of_one_type_do_not_collide() {
        let trip = types::DeclaredType::new(
            "trip",
            vec![
                types::Field::new("from", types::ValueType::Reference),
                types::Field::new("to", types::ValueType::Reference),
            ],
        );
        let travelling = Fact {
            event: Some(Event {
                kind: "trip".into(),
                metadata: [
                    ("from".to_string(), "place:springfield".to_string()),
                    ("to".to_string(), "place:shelbyville".to_string()),
                ]
                .into_iter()
                .collect(),
                refs: Vec::new(),
            }),
            ..fact("person:bart", "f1", "went over for the day")
        };
        let scanned = vec![
            doc(entity("person:bart", "Bart"), "", vec![travelling]),
            doc(entity("place:springfield", "Springfield"), "", Vec::new()),
            doc(entity("place:shelbyville", "Shelbyville"), "", Vec::new()),
        ];

        let reached = |from: &str, relation: &str, direction: Direction| {
            let found = resolve(
                &scanned,
                std::slice::from_ref(&trip),
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId(from.into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                },
            )
            .expect("both keys are declared references");
            handles(&found[0].connected)
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            reached("person:bart", "from", Direction::Out),
            vec!["place:springfield"],
        );
        assert_eq!(
            reached("person:bart", "to", Direction::Out),
            vec!["place:shelbyville"],
        );
        assert_eq!(
            reached("place:springfield", "from", Direction::In),
            vec!["person:bart"],
            "walked inbound, one key reaches the traveller",
        );
        assert!(
            reached("place:springfield", "to", Direction::In).is_empty(),
            "and the OTHER key does not — the two keys are what keeps the ends apart",
        );
    }

    /// **A relation is scoped by its KEY, and the direction chooses the way.**
    ///
    /// The inbound walk reaches every record pointing here through that key,
    /// whatever else each record is. The bike's repair record carries `owner`
    /// and is reached — which is what the walk was asked for, and is why the
    /// name no longer claims to be about pets.
    ///
    /// **This is the case the old code could not have failed.** The reverse
    /// walk used to also require the record to answer the declared type, under
    /// the name `pet.owner`. That could never exclude anything: the walked key
    /// is one of the type's keys, and holding one key is what answering a type
    /// means. So the check is gone, and so is the name that advertised it.
    #[test]
    fn a_relation_is_scoped_by_its_key_and_the_direction_chooses_the_way() {
        let scanned = kennel();
        let declarations = vec![pet()];
        let walk = |from: &str, relation: &str, direction: Direction| {
            resolve(
                &scanned,
                &declarations,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId(from.into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                },
            )
        };

        let pointing_here = walk("person:bart", "owner", Direction::In)
            .expect("a declared key is followable inbound");
        assert_eq!(
            handles(&pointing_here[0].connected),
            vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
            "everything pointing here through `owner`, and the bike is no pet: {pointing_here:?}",
        );
        assert_eq!(
            pointing_here[0].connected[2].via,
            Some(Via {
                link: Link::Relation("owner".into()),
                direction: Direction::In,
            }),
            "and it says it arrived along the key, inbound",
        );

        // The same key the other way, off the record that is not a pet: one
        // name, and the direction is what picks the question.
        let points_at = walk("thing:red-bike", "owner", Direction::Out)
            .expect("a declared key is followable outbound");
        assert_eq!(handles(&points_at[0].connected), vec!["person:bart"]);

        // …and the object with no record at all is reached by neither, so the
        // hits above are about the key rather than about every object.
        assert!(
            walk("thing:floor-pump", "owner", Direction::Out).expect("served")[0]
                .connected
                .is_empty(),
        );

        // The qualified name is gone, and the refusal offers the key instead.
        let refused = walk("person:bart", "pet.owner", Direction::In)
            .expect_err("`type.key` is no longer a relation name");
        match refused {
            MemoryError::InvalidQuery(why) => assert!(
                why.contains("owner"),
                "the refusal names the relation that does exist: {why}",
            ),
            other => panic!("a name no declaration backs is a malformed query: {other:?}"),
        }
    }

    /// **A key may be spelled like an edge shape, and the two do not cross.**
    ///
    /// Bare key names make this reachable: nothing stops a type declaring a key
    /// called `location`, and there is an edge shape of that name. They are
    /// different vocabularies and `follow` says which one it means by which
    /// argument it carries, so the same word reaches two different places.
    ///
    /// Both halves in one case, because either alone passes on a build that
    /// collapses the two into one lookup.
    #[test]
    fn a_key_named_like_an_edge_shape_is_still_a_key() {
        let declared = types::DeclaredType::new(
            "posting",
            vec![types::Field::new("location", types::ValueType::Reference)],
        );
        let by_key = Fact {
            event: Some(Event {
                kind: "posting".into(),
                metadata: [("location".to_string(), "place:shelbyville".to_string())]
                    .into_iter()
                    .collect(),
                refs: Vec::new(),
            }),
            ..fact("person:bart", "f1", "posted from over there")
        };
        let by_edge = edged(
            "person:bart",
            "f2",
            "actually lives here",
            EdgeShape::Location,
            "place:springfield",
        );
        let scanned = vec![
            doc(entity("person:bart", "Bart"), "", vec![by_key, by_edge]),
            doc(entity("place:springfield", "Springfield"), "", Vec::new()),
            doc(entity("place:shelbyville", "Shelbyville"), "", Vec::new()),
        ];

        let along = |along: Along| {
            let found = resolve(
                &scanned,
                std::slice::from_ref(&declared),
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:bart".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                    },
                    follow: Some(Follow {
                        along,
                        ..Follow::hop()
                    }),
                },
            )
            .expect("both vocabularies are followable");
            handles(&found[0].connected)
                .into_iter()
                .map(str::to_string)
                .collect::<Vec<_>>()
        };

        assert_eq!(
            along(Along::Relation("location".into())),
            vec!["place:shelbyville"],
            "the KEY reaches what the record's value names",
        );
        assert_eq!(
            along(Along::Edge(EdgeShape::Location)),
            vec!["place:springfield"],
            "and the SHAPE reaches what the edge points at — same word, two vocabularies",
        );
    }

    /// **The declared value type licenses the operator.** A key declared to
    /// hold a date can be asked what is before a date; the same key with
    /// nothing declared has equality and the ordering is refused.
    ///
    /// Both halves matter: refusing is what stops a caller reading an equality
    /// answer as an ordering one.
    #[test]
    fn a_declaration_licenses_an_ordering_and_nothing_else_does() {
        let scanned = kennel();
        let born_before = |declarations: &[types::DeclaredType]| {
            resolve(
                &scanned,
                declarations,
                &GraphQuery {
                    select: Selection {
                        fields: vec![FieldFilter::comparing(
                            "born",
                            types::Compare::Before,
                            "2020-01-01",
                        )],
                        ..Selection::default()
                    },
                    ..GraphQuery::default()
                },
            )
        };
        let found = born_before(&[pet()]).expect("the declaration licenses it");
        assert_eq!(
            handles(&found),
            vec!["pet:santas-little-helper"],
            "the older pet is before the date and the younger one is not: {found:?}",
        );

        born_before(&[]).expect_err("with nothing declared, an ordering is refused");

        // The number half of the same rule, and its own negative: `less` is
        // licensed by a declared number, and asking it of the date key is not.
        let lighter = resolve(
            &scanned,
            &[pet()],
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::comparing("weight", types::Compare::Less, "10")],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a declared number licenses an ordering");
        assert_eq!(handles(&lighter), vec!["pet:snowball"]);

        resolve(
            &scanned,
            &[pet()],
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::comparing("born", types::Compare::Less, "10")],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect_err("a date key does not license a number's ordering");
    }

    /// **An undeclared record keeps equality.** Matching stays structural: a
    /// record is found by the keys it carries whether or not anybody declared a
    /// type for it, and nothing about the ordering above narrows that.
    #[test]
    fn an_undeclared_record_keeps_equality() {
        let found = resolve(
            &kennel(),
            &[],
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::holding("born", "2024-11-02")],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("equality needs no declaration");
        assert_eq!(handles(&found), vec!["pet:snowball"]);
    }

    /// **A walk can filter what it reaches**, which is what the record filters
    /// could not do: they describe the roots.
    ///
    /// The acceptance case, end to end: from the owner, walk the has-many, and
    /// keep only the pets born before a date. Paired with the same walk
    /// unfiltered, so the filter is doing the work rather than the graph being
    /// that shape anyway.
    #[test]
    fn a_walk_keeps_only_what_answers_its_filters() {
        let scanned = kennel();
        let declarations = vec![pet()];
        let from_bart = |keeping: Vec<FieldFilter>| {
            resolve(
                &scanned,
                &declarations,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:bart".into())),
                        ..Selection::default()
                    },
                    include: Include::default(),
                    follow: Some(Follow {
                        along: Along::Relation("owner".into()),
                        direction: Some(Direction::In),
                        keeping,
                        ..Follow::hop()
                    }),
                },
            )
            .expect("a walk with filters on what it reaches")
        };

        let all = from_bart(Vec::new());
        assert_eq!(
            handles(&all[0].connected),
            vec!["pet:santas-little-helper", "pet:snowball", "thing:red-bike"],
            "unfiltered, the walk reaches everything pointing here through the key: {all:?}",
        );

        let older = from_bart(vec![FieldFilter::comparing(
            "born",
            types::Compare::Before,
            "2020-01-01",
        )]);
        assert_eq!(
            handles(&older[0].connected),
            vec!["pet:santas-little-helper"],
            "and filtered, it reaches only the pet born before the date: {older:?}",
        );
        assert!(
            older[0].unwalked,
            "the pet it did not keep is an edge nobody followed, and the object says so: {:?}",
            older[0],
        );
        assert!(
            older[0].connected[0]
                .facts
                .iter()
                .all(|f| f.content == "the greyhound"),
            "a kept object arrives with the records that answered: {:?}",
            older[0].connected[0],
        );
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
            resolve(&scanned, &[], &GraphQuery::subject(EntityId(handle.into())))
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
