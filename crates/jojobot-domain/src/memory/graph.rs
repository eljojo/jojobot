//! **The graph query: say the shape you want, and get that shape back.**
//!
//! Three axes, and they combine. **Select** says which objects the answer is
//! about — one named handle, a kind, the things that answer a type, a key and
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

use std::collections::{BTreeMap, BTreeSet, HashSet};

use super::filter::{Filter, Order, Ordered, Test};

use super::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, FactAddress, FactId, FactStatus,
    MemoryError, guard, mention, search::DocScan, types, validate_subject,
};

/// **How far a walk may go.** A bound rather than a preference: an edge may
/// point back at where it came from, and a walk with no ceiling on a graph with
/// a cycle in it is one that returns when the process dies.
///
/// The visited set already stops a cycle repeating; this stops a query asking
/// for a subgraph nobody can read.
pub const MAX_DEPTH: usize = 5;

/// **What a key filter is asked OF**, which is two different questions under
/// one shape.
///
/// *Which friends have eaten three or more donuts* is about what each friend
/// HOLDS — the newest write of the key, or the total when the key is a counter.
/// *Which visits cost more than fifty* is about occurrences, and no folding
/// answers it: it wants the individual record.
///
/// **They were one question once and the answer was the record's**, which made
/// the first question quietly unanswerable: it returned the friends holding a
/// single record saying three and left out the friend with three records of
/// one. It returned rows and read like an answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Scope {
    /// **What the thing holds**, over its writes folded together — the same map
    /// the type question is asked of. The default, because it is the question a
    /// caller asking about a THING is asking.
    #[default]
    Thing,
    /// **What one record says.** An object is selected when a single record of
    /// its answers every filter of this scope, and it arrives carrying the
    /// records that answered.
    Record,
}

impl Scope {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Scope::Thing => "thing",
            Scope::Record => "record",
        }
    }

    /// The scope a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Scope> {
        match token.trim() {
            "thing" => Some(Scope::Thing),
            "record" => Some(Scope::Record),
            _ => None,
        }
    }
}

/// **One key, and optionally the value it must hold.**
///
/// `value: None` asks only that the key is carried at all — which is a
/// different question from any particular value and the one to ask when you
/// want everything that records a thing rather than everything that records it
/// one way.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldFilter {
    /// The key, exactly as it is spelled. Matching is structural, so nothing
    /// has to have declared it.
    ///
    /// **`None` asks about the VALUE under any key at all** — *is this held as
    /// a value anywhere on this thing*, as opposed to sitting in its prose.
    /// That is what *did somebody record this so it can be asked for later*
    /// reduces to, and it is the question a caller has when it knows what was
    /// written and not where it went.
    ///
    /// A filter naming neither a key nor a value asks nothing and is refused.
    pub key: Option<String>,
    /// The value it must hold, compared whole and trimmed. `None` matches any
    /// value.
    pub value: Option<String>,
    /// **How the two values are compared**, and it is licensed by what the key
    /// was declared to hold rather than chosen freely. Equality needs no
    /// declaration and is the default.
    pub compare: types::Compare,
    /// **What this is asked of** — what the thing holds, or what one record
    /// says. [`Scope::Thing`] unless the caller says otherwise.
    pub scope: Scope,
    /// **A whole expression, as one filter.** When it is here the filter IS this
    /// tree, asked in `scope`, and `key`, `value` and `compare` say nothing. It is
    /// how an expression a caller wrote rides where a key filter rides, so it
    /// narrows a selection and a walk the way a key filter does.
    pub tree: Option<Filter>,
}

impl FieldFilter {
    /// A filter asking only that the key is there.
    pub fn key(key: &str) -> Self {
        FieldFilter {
            key: Some(key.trim().to_string()),
            value: None,
            compare: types::Compare::Equals,
            scope: Scope::Thing,
            tree: None,
        }
    }

    /// **A filter asking that SOME key holds this value**, without naming
    /// which.
    ///
    /// **It reads the fields and never the prose**, which is the whole of the
    /// question: a string somebody wrote into a sentence is not a value
    /// anything can be asked for later, and an answer that could not tell the
    /// two apart would be the breadth verb rather than this one.
    pub fn anywhere(value: &str) -> Self {
        FieldFilter {
            key: None,
            value: Some(value.trim().to_string()),
            compare: types::Compare::Equals,
            scope: Scope::Thing,
            tree: None,
        }
    }

    /// A filter asking that the key holds this value.
    pub fn holding(key: &str, value: &str) -> Self {
        FieldFilter {
            value: Some(value.trim().to_string()),
            ..FieldFilter::key(key)
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

    /// **A whole filter tree, asked in one scope** — what an expression reads
    /// into.
    pub fn expression(tree: Filter, scope: Scope) -> Self {
        FieldFilter {
            key: None,
            value: None,
            compare: types::Compare::Equals,
            scope,
            tree: Some(tree),
        }
    }

    /// The same filter asked of one record rather than of the thing.
    pub fn on_a_record(self) -> Self {
        FieldFilter {
            scope: Scope::Record,
            ..self
        }
    }

    /// **This filter as a leaf of the filter tree**, the one thing a key filter
    /// is evaluated by. A key with no value asks only that the key is carried;
    /// a key with a value asks the comparison; no key asks about the value under
    /// any key at all.
    pub fn as_filter(&self) -> Filter {
        if let Some(tree) = &self.tree {
            return tree.clone();
        }
        match (&self.key, &self.value) {
            (Some(key), None) => Filter::exists(key),
            (Some(key), Some(wanted)) => Filter::comparing(key, self.compare, wanted),
            (None, Some(wanted)) => Filter::anywhere(Test::of(self.compare, wanted)),
            // Refused before it reaches here; false rather than true so a
            // filter that asked nothing cannot select everything.
            (None, None) => Filter::Any(Vec::new()),
        }
    }
}

/// **Every filter of one scope, as one tree.** An AND of leaves, which is how a
/// list of key filters has always combined.
fn all_of_scope<'f>(filters: impl Iterator<Item = &'f FieldFilter>, scope: Scope) -> Filter {
    Filter::All(
        filters
            .filter(|f| f.scope == scope)
            .map(FieldFilter::as_filter)
            .collect(),
    )
}

/// **A filter has to ask something.**
///
/// A key spelled empty is a caller mistake, and so is a filter naming neither a
/// key nor a value: it selects everything or nothing depending on which way the
/// predicate happens to fall, and neither is an answer anybody asked for.
fn validate_filter(field: &FieldFilter) -> Result<(), MemoryError> {
    if field.tree.is_some() {
        return Ok(());
    }
    match (&field.key, &field.value) {
        (Some(key), _) if key.trim().is_empty() => Err(MemoryError::InvalidQuery(
            "a key filter names no key".into(),
        )),
        (None, None) => Err(MemoryError::InvalidQuery(
            "a key filter names neither a key nor a value, so it asks nothing. Name the key to \
             ask what it holds, or name the value to ask whether anything holds it"
                .into(),
        )),
        _ => Ok(()),
    }
}

/// **Do this thing's folded fields answer every filter asked of the thing.**
///
/// `held` is `None` for a thing the store has no fields row for, and a filter
/// naming a key is not answered by a thing carrying no keys. An empty set of
/// filters is answered by anything, including that thing — asking nothing is
/// not the same as asking for a key nobody wrote.
fn holds_all<'f>(
    held: Option<&BTreeMap<String, String>>,
    filters: impl Iterator<Item = &'f FieldFilter>,
) -> bool {
    let tree = all_of_scope(filters, Scope::Thing);
    matches!(&tree, Filter::All(leaves) if leaves.is_empty())
        || held.is_some_and(|fields| tree.holds(fields))
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
    /// **Only the direct children of this entity.** A property of the object
    /// itself, like its kind, so it combines with `kind`: the roles of one bot
    /// is a kind and a parent. One level, never a subtree, for the reason
    /// `list_entities`' own parent argument is.
    pub parent: Option<EntityId>,
    /// **Objects that answer this type**, matched structurally over the keys
    /// their records carry — never over what anybody declared them to be.
    ///
    /// Asked of the THING: the newest write of each key on it, so an object
    /// described over several records answers a type that no one of them
    /// answers alone. See [`super::folded_fields`].
    ///
    /// A partial answer is an answer. An object carrying some of the keys comes
    /// back saying which it lacks, because a filter that kept only whole ones
    /// would hide exactly the objects worth finding.
    pub answers_type: Option<types::DeclaredType>,
    /// **A pre-narrowed set of objects, resolved by the caller before this
    /// walk runs.** When present, this is the WHOLE of which objects get
    /// their records read — `kind` and `answers_type` still narrow what the
    /// answer says about them (a type carried here still reports what each
    /// one lacks), but neither reopens the full entity list to decide who is
    /// read.
    ///
    /// **What this exists for:** `answers_type` alone forces reading every
    /// entity's records to find the ones that answer it — `filters_facts`'s
    /// own reason for existing. A caller that already knows the answer from
    /// an index built for exactly this question (a structural match over the
    /// same keys, kept in memory) has no reason to pay for that read twice.
    /// This is how it hands the narrowed set back in rather than asking this
    /// walk to rediscover it.
    ///
    /// `None` — the ordinary case — changes nothing here: every existing
    /// selection shape reads exactly as it did before this field existed.
    pub candidates: Option<Vec<EntityId>>,
    /// Objects carrying these keys and the values named. **Each filter says
    /// what it is asked of** — what the thing holds, or what one record says
    /// (see [`Scope`]) — and the ones asked of a record must all hold on the
    /// SAME record, because they describe a single record rather than a pair of
    /// separate questions.
    pub fields: Vec<FieldFilter>,
    /// **The order the selected objects come back in**: a key and a direction,
    /// applied to the whole selection before any cut, so a page of the answer is
    /// a page of the order. `None` keeps handle order. See [`Order`].
    pub order: Option<Order>,
    /// **Records around a day**, on the clock this names. See [`Nearness`].
    pub near: Option<Nearness>,
    /// **Who is asking**, filled from the session handle by the verb and never
    /// from an argument.
    ///
    /// ⭐ **A caller cannot ask for somebody else's objects because there is
    /// nowhere to say so** — the access rule is the absence of an argument
    /// rather than a check on one. `None` is a caller with no identity, which
    /// reaches everything unowned and nothing owned.
    pub asked_by: Option<EntityId>,
}

impl Selection {
    /// **Does this selection choose anything at all?**
    ///
    /// A selection that narrows nothing is a request for the whole store,
    /// which is not a question — and it is also what a caller who named only a
    /// record to trace has sent, so the two readers of this condition are the
    /// refusal and the fill that makes the refusal unnecessary. One definition,
    /// because a second could come to disagree about what counts as a
    /// question.
    pub fn narrows_nothing(&self) -> bool {
        self.subject.is_none()
            && self.kind.is_none()
            && self.parent.is_none()
            && !self.filters_facts()
    }

    /// Is there a filter here beyond the object's own properties? Kind and
    /// subject are properties of the object itself.
    fn filters_facts(&self) -> bool {
        self.answers_type.is_some() || !self.fields.is_empty() || self.near.is_some()
    }

    /// Is there a filter here that ONE record has to answer?
    ///
    /// The type is not one of them: it is asked of the thing, so it narrows
    /// which objects come back and never which of an object's records do. A
    /// record that carries none of a type's keys still belongs to a thing that
    /// answers it. **Nor is a key filter, unless it says it is** — asked of the
    /// thing, a key filter is answered by the fold and no record has to carry
    /// anything.
    fn filters_records(&self) -> bool {
        self.fields.iter().any(|f| f.scope == Scope::Record) || self.near.is_some()
    }

    /// Does this fact answer every filter asked of a record. A fact carrying no
    /// field answers none of them.
    fn keeps(&self, fact: &Fact) -> bool {
        all_of_scope(self.fields.iter(), Scope::Record).holds(&fact.fields)
            && self.near.is_none_or(|near| near.holds(fact))
    }
}

/// **Which of a claim's clocks a neighbourhood read compares.**
///
/// The two answer different questions and a read that silently picked one
/// would mislead. *What did Milhouse say back in August* asks when the claim is
/// true OF. *What was filed that week* asks when jojobot took it in.
///
/// ⚠️ **The staleness date is deliberately not here.** Its own definition is a
/// fact about our knowledge rather than about the world, and nothing fires on
/// it, so it places no claim in time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Clock {
    /// **The day the claim was MADE** — the default, because *what did they say
    /// back in August* is a question about when things were said.
    ///
    /// ⛔️ **Not the day the thing happened.** That is its own field now, and
    /// this clock does not read it: a claim recorded in October about a summer
    /// event sits in October here, which is where a caller asking what was
    /// recorded that week expects to find it.
    #[default]
    RecordedOn,
    /// **The day jojobot took the record in.**
    ///
    /// ⚠️ **Not every record carries one.** A record written before the stamp
    /// existed has none and keeps none, so a read on this clock cannot place
    /// it. Those are counted and reported rather than dropped: a smaller answer
    /// that says nothing about what it could not reach is the failure this
    /// whole read exists to avoid.
    TakenIn,
    /// **The day the thing HAPPENED**, [`Fact::happened_at`] rather than
    /// either clock above. *What was going on around then* asks this one:
    /// a claim recorded in October about a summer trip sits in summer here,
    /// which is where a caller asking what was happening that week expects
    /// to find it — the opposite placement from [`Clock::RecordedOn`].
    ///
    /// ⚠️ **Not every claim carries one.** A claim with no event day of its
    /// own cannot be placed on this clock, and is counted rather than
    /// silently dropped — the same contract [`Clock::TakenIn`] already
    /// keeps.
    HappenedAt,
}

/// **What else was recorded around a day.**
///
/// Things near in time cue one another, and real questions arrive shaped as
/// *when plus who*. Every claim already carries its dates; this is what lets
/// them be asked associatively.
///
/// ⛔️ **Not a query language and not inference.** A window over dates the
/// store already holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nearness {
    /// The day the question is asked around.
    pub day: jiff::civil::Date,
    /// **How far either side counts as near**, in days. Symmetric: what came
    /// just before a day and just after it are equally what surrounded it.
    pub within_days: u32,
    /// Which clock is compared.
    pub clock: Clock,
}

impl Nearness {
    /// **Where a fact sits on the chosen clock**, or nothing when this clock
    /// cannot place it.
    fn placed(&self, fact: &Fact) -> Option<jiff::civil::Date> {
        match self.clock {
            Clock::RecordedOn => Some(fact.recorded_at),
            Clock::TakenIn => fact
                .inserted_at
                .map(|at| at.to_zoned(jiff::tz::TimeZone::UTC).date()),
            Clock::HappenedAt => fact.happened_at,
        }
    }

    /// Is this fact inside the window.
    ///
    /// **On [`Clock::HappenedAt`], a fact carrying [`Fact::happened_through`]
    /// occupies the whole closed range from `happened_at` to it** — the day
    /// asked about is near when it falls inside that range, or within the
    /// window of whichever end it is closer to. A fact with no
    /// `happened_through` has a range of one day, which is
    /// [`Fact::happened_at`] alone: the single-day case is unchanged.
    fn holds(&self, fact: &Fact) -> bool {
        if self.clock == Clock::HappenedAt {
            if fact.happened_covers(self.day) {
                return true;
            }
            let Some(start) = fact.happened_at else {
                return false;
            };
            let end = fact.happened_through.unwrap_or(start);
            let nearest = if self.day < start { start } else { end };
            let span = self.day.since(nearest).map(|s| s.get_days().abs());
            return span.is_ok_and(|days| days <= i64::from(self.within_days) as i32);
        }
        let Some(at) = self.placed(fact) else {
            return false;
        };
        self.holds_day(at)
    }

    /// Is a day inside the window, either side of the day asked about.
    fn holds_day(&self, at: jiff::civil::Date) -> bool {
        let span = self.day.since(at).map(|s| s.get_days().abs());
        span.is_ok_and(|days| days <= i64::from(self.within_days) as i32)
    }

    /// **A fact this clock cannot place at all.** Counted, never silently
    /// dropped.
    fn unplaceable(&self, fact: &Fact) -> bool {
        self.placed(fact).is_none()
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
    /// **Serve a shape's sources alongside it, on the same listing.**
    ///
    /// Off by default: a shape already carries its sources' content in its own
    /// words, so the listing that also carries the shape leaves them out and
    /// says how many and how to reach them. This only ever narrows
    /// [`Object::facts`] — a record asked for by name (`history_record`, or a
    /// mark's own address) is still served whole, because elision applies to
    /// the listing rather than to the record.
    pub stood_for: bool,
}

impl Default for Include {
    /// Facts, and no prose. Facts are what this verb has always answered with,
    /// and prose is a page rather than a row: shipping every object's page
    /// unasked is the cost a caller cannot decline.
    fn default() -> Self {
        Include {
            facts: true,
            prose: false,
            stood_for: false,
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
/// declaration made: a key some type declared to hold a reference, which a walk
/// can follow by the key's own name. A value that is a handle is a link under
/// any key, declared or not; the declaration adds the kind the value must point
/// at and that walk by name, and never decides whether the link exists.
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
    /// **A handle written into a claim's own words** — an admission that the
    /// link is there, weaker even than [`EdgeShape::Connection`]'s: nobody
    /// drew it on purpose, an entity's name just appears in a sentence. Never
    /// rendered as an edge, so a reader cannot mistake one for the other.
    Mention,
    /// **A claim's `refs`** — the entities it touches with no claim about how.
    Ref,
    /// **A field whose value is another thing's handle**, under any key. The
    /// value being a handle is what makes it a link; declaring the key a
    /// reference adds the kind it must point at and a walk by the key's name,
    /// never whether the link exists.
    Field,
}

impl Link {
    /// The token this reads as — the shape's own, the relation's name, or a
    /// fixed word for the two kinds of link with no name of their own.
    pub fn token(&self) -> &str {
        match self {
            Link::Edge(shape) => shape.as_token(),
            Link::Relation(name) => name.as_str(),
            Link::Mention => "mention",
            Link::Ref => "ref",
            Link::Field => "field",
        }
    }
}

/// **Which edges to walk, and how far.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Follow {
    /// What to travel along: any edge, one shape, or a declared relation.
    pub along: Along,
    /// **Which end to leave by**, for a relation as much as for an edge shape.
    /// `None` is outbound.
    ///
    /// A relation is followed by its KEY, and one key name cannot encode two
    /// ways. The same entity can both hold a key and be pointed at by one: out
    /// of a record, `owner` reaches the handle that record holds; in to an
    /// entity, `owner` reaches every record pointing at it. Without a direction
    /// beside the name the walk answers one of those two questions and gives no
    /// sign of the other.
    pub direction: Option<Direction>,
    /// How many hops. One is the neighbours; two is the neighbours' neighbours.
    pub depth: usize,
    /// **What the walk keeps of what it reaches.** Empty keeps everything,
    /// which is what a walk did before it could filter.
    ///
    /// The same filters a selection takes, asked the same two ways: one asked
    /// of the THING is answered by its folded fields, and one asked of a RECORD
    /// keeps an object when a single record of its answers every filter of that
    /// scope, which then arrives with it. See [`Scope`] — a filter that meant
    /// one thing here and another in a selection would be the defect the scope
    /// exists to name, wearing a second name.
    pub keeping: Vec<FieldFilter>,
    /// **Keep only what FITS this type** — an object whose records carry every
    /// key the type names, between them.
    ///
    /// Fitting is not the same question as [`Selection::answers_type`], which
    /// keeps an object carrying SOME of the keys and reports which it lacks. A
    /// walk narrowed by a partial match could not exclude anything it reached
    /// through one of the type's own keys, which is most of what a walk is for;
    /// and what a caller means by "which of these are pets" is the ones that
    /// are, not the ones that share a field with one.
    pub fits_type: Option<types::DeclaredType>,
}

impl Follow {
    /// One hop along every edge, outbound.
    pub fn hop() -> Self {
        Follow {
            along: Along::AnyEdge,
            direction: None,
            depth: 1,
            keeping: Vec::new(),
            fits_type: None,
        }
    }

    /// The direction this walk leaves by. Outbound unless it says otherwise,
    /// whether the walk follows an edge shape or a relation's key.
    fn direction(&self) -> Direction {
        self.direction.unwrap_or_default()
    }

    /// Is there a filter here that ONE record of what the walk reaches has to
    /// answer? A filter asked of the thing is answered by the fold, and no
    /// record has to carry anything for it.
    fn filters_records(&self) -> bool {
        self.keeping.iter().any(|f| f.scope == Scope::Record)
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
    /// **A key whose writes come back on every object in the answer**, oldest
    /// first, and how many of them. `None` asks for none, which is the ordinary
    /// read: current truth, one value per key.
    ///
    /// It is read from the store rather than from the objects' records, because
    /// what a record carries is the projection — the writes it replaced are not
    /// on it any more.
    pub history: Option<History>,
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
        let ordered = |filters: &[FieldFilter]| {
            filters
                .iter()
                .any(|f| f.tree.is_some() || f.compare.licensed_by().is_some())
        };
        self.select.order.is_some()
            || ordered(&self.select.fields)
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
        if select.narrows_nothing() {
            return Err(MemoryError::InvalidQuery(
                "name what to recall: a subject, a kind, a parent, a type, a key, a where \
                 line, or a record's own address — history_record or built_on"
                    .into(),
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
            validate_filter(field)?;
        }
        for field in self.follow.iter().flat_map(|f| &f.keeping) {
            validate_filter(field)?;
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

/// **What a view's keys say to ask** — the one reader of a view's vocabulary.
///
/// A view is a record and its keys ARE its question: `selects` names the kind
/// it looks at, `shows` names what of each one comes back, and `asks` names the
/// one question a selection cannot express.
///
/// **Both callers read it here** (rule 51). The served verb fills a call in
/// from a view, and a view's page runs one; a second reader of the same keys
/// is two answers to what a view means, and the page and the verb would come
/// to disagree the first time either grew a key.
///
/// **Nothing is validated here.** Whether `selects` names a kind this instance
/// holds is the caller's question and the answers differ: the verb refuses the
/// call, the page says the view cannot be run.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Asked {
    /// The kind token the view selects over, exactly as it is written.
    pub selects: Option<String>,
    /// Each thing's records.
    pub facts: bool,
    /// Each thing's prose.
    pub prose: bool,
    /// Each bot's charter, which is its prose under the name a reader asked
    /// for it by.
    pub charter: bool,
    /// **Only what has fallen due.** Arithmetic over dates rather than a value
    /// to filter on, which is why it is a named ask rather than a key filter.
    pub overdue: bool,
    /// **Only what can be started**, with what blocks the rest — see
    /// [`super::readiness`]. A named ask for the same reason `overdue` is: it
    /// asks about the things a reference list names, which a key filter on the
    /// thing cannot.
    pub ready: bool,
    /// **A key filter for every record on the view that carries one** — see
    /// [`asked_by_view`] for the shape a filter record takes. Several
    /// filters are several records; nothing here caps it at one per key.
    pub filters: Vec<FieldFilter>,
    /// **A whole question in one expression**, from the view's `where`. See
    /// [`Asked::filters`], which reads it against the asker's day.
    pub expression: Option<String>,
    /// **What the expression is asked of**, `thing` or `record`, from the view's
    /// `scope`. Absent is the thing.
    pub scope: Option<String>,
    /// **How the objects are ordered**, from the view's `order`: a key, and
    /// `ascending` or `descending` after it. See [`Asked::ordering`].
    pub order: Option<String>,
    /// **A type name to select structurally, by the key `answers_type`** —
    /// unresolved: the caller of [`asked_by_view`] still has to look the
    /// name up, the same way it already does for the argument of the same
    /// name.
    pub answers_type: Option<String>,
    /// **A relation walk, unresolved the same way `answers_type` is.**
    /// `None` when the view names no walk at all — distinct from a walk
    /// that names no shape or relation, which is "any edge".
    pub follow: Option<AskedFollow>,
    /// **The keys of each object's fields the view shows**, from its
    /// `shows_keys`. `None` when it names none, which means every key — a
    /// different question from an empty list.
    pub keys: Option<Vec<String>>,
}

impl Asked {
    /// **The view's key filters as a selection takes them**: its filter records,
    /// then its expression, which is read against the asker's day and asked in
    /// the view's scope. Both callers of [`asked_by_view`] run the view's question
    /// through this, so the page and the verb cannot read `where` two ways.
    pub fn filters(&self, today: jiff::civil::Date) -> Result<Vec<FieldFilter>, MemoryError> {
        let mut all = self.filters.clone();
        if let Some(text) = &self.expression {
            let scope = match self.scope.as_deref().map(str::trim) {
                None | Some("") => Scope::Thing,
                Some(token) => Scope::of_token(token).ok_or_else(|| {
                    MemoryError::InvalidQuery(format!(
                        "a view's scope is thing or record, and '{token}' is neither"
                    ))
                })?,
            };
            all.push(FieldFilter::expression(
                super::filter::expression::parse(text, today)?,
                scope,
            ));
        }
        Ok(all)
    }

    /// The view's ordering, when it holds one.
    pub fn ordering(&self) -> Result<Option<Order>, MemoryError> {
        self.order.as_deref().map(Order::in_words).transpose()
    }
}

/// **A view's own relation walk, read off its fields — the pieces
/// [`Follow`] is built from, still as plain strings.** A
/// relation's or a shape's name is not resolved here for the same reason a
/// filter's `fits_type` is not: resolving a declared type needs a read this
/// pure function cannot make, so the caller finishes what this starts,
/// exactly as it already does for the argument of the same shape.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AskedFollow {
    /// One edge shape, by its token — `along:location`, `along:membership`,
    /// and so on for the five shapes.
    pub shape: Option<String>,
    /// A declared relation's key, instead of a shape.
    pub relation: Option<String>,
    /// `in` or `out`. `None` is outbound, the same default the argument of
    /// the same name has.
    pub direction: Option<String>,
    /// How many hops. `None` defaults to 1, the same as the argument.
    pub depth: Option<u32>,
    /// **Keep only what fits this type**, by name, unresolved.
    pub fits_type: Option<String>,
}

/// Read a view's question off the keys it holds and the records it
/// carries. See [`Asked`].
///
/// **`held` answers `selects`/`shows`/`asks` — bare flags, one value each,
/// where the view's folded fields are exactly the right shape to hold
/// them.** A filter is not: it is several fields naming one question (a
/// key, a value, how they compare, what the comparison is asked of), and a
/// view may carry more than one. Folding them onto the view's own fields
/// would need a delimiter for the list and a parse for the fields inside
/// each entry — a syntax this store does not otherwise have. **A filter is
/// a record instead**, the same shape every other claim on this store
/// already is: its own fields carry `key`, `value`, an optional `compare`
/// and an optional `scope`, exactly the axes [`FieldFilter`] has. Several
/// filters are several records, and two filters on the same key are two
/// records — nothing here caps it at one.
pub fn asked_by_view(held: &BTreeMap<String, String>, facts: &[Fact]) -> Asked {
    let shows = |what: &str| {
        held.get("shows")
            .is_some_and(|s| s.split(',').any(|part| part.trim() == what))
    };
    let filters = facts
        .iter()
        .filter(|f| f.status == FactStatus::Active)
        .filter_map(|f| {
            let key = f.fields.get("key").cloned();
            let value = f.fields.get("value").cloned();
            if key.is_none() && value.is_none() {
                return None;
            }
            let compare = f
                .fields
                .get("compare")
                .and_then(|token| types::Compare::of_token(token))
                .unwrap_or_default();
            let scope = match f.fields.get("scope").map(String::as_str) {
                Some("record") => Scope::Record,
                _ => Scope::Thing,
            };
            Some(FieldFilter {
                key,
                value,
                compare,
                scope,
                tree: None,
            })
        })
        .collect();
    // **`None` when the view names neither a shape nor a relation and asks
    // no depth, direction or fits_type either** — a walk that names nothing
    // is not "any edge", it is no walk, the same distinction `args.follow`
    // being absent already makes for a caller.
    let follow = {
        let shape = held.get("follow_shape").cloned();
        let relation = held.get("follow_relation").cloned();
        let direction = held.get("follow_direction").cloned();
        let depth = held.get("follow_depth").and_then(|d| d.trim().parse().ok());
        let fits_type = held.get("follow_fits_type").cloned();
        if shape.is_none()
            && relation.is_none()
            && direction.is_none()
            && depth.is_none()
            && fits_type.is_none()
        {
            None
        } else {
            Some(AskedFollow {
                shape,
                relation,
                direction,
                depth,
                fits_type,
            })
        }
    };
    Asked {
        selects: held.get("selects").cloned(),
        expression: held.get("where").cloned().filter(|w| !w.trim().is_empty()),
        scope: held.get("scope").cloned(),
        order: held.get("order").cloned().filter(|o| !o.trim().is_empty()),
        facts: shows("facts"),
        prose: shows("prose"),
        filters,
        charter: shows("charter"),
        overdue: held.get("asks").map(String::as_str) == Some("overdue"),
        ready: held.get("asks").map(String::as_str) == Some(super::readiness::ASK),
        answers_type: held.get("answers_type").cloned(),
        follow,
        // **A list of blanks names no key**, so it narrows nothing, the same
        // as a view that names none.
        keys: held
            .get("shows_keys")
            .map(|list| {
                list.split(',')
                    .map(str::trim)
                    .filter(|key| !key.is_empty())
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .filter(|keys| !keys.is_empty()),
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
/// here.** Scoping the reverse walk to one type, under a name like `type.key`,
/// excludes nothing: the walked key is by construction one of the type's keys,
/// and answering a type takes only ONE of them, so whatever the walk reaches
/// answers the type. What narrows a walk is [`Follow::fits_type`], asked beside
/// it and answered on the stricter question of whether the thing holds EVERY
/// key.
///
/// **A name is followed only when a declaration backs it.** A value that is a
/// handle under a key nobody declared is still a link, reached by an unscoped
/// walk as a field link; what it cannot be is walked by the key's name.
///
/// **The search asks each declaration the WHOLE question**, because two types
/// may name one key and mean their own thing by it. Stopping at the first
/// declaration owning the name and judging only that one would let a type
/// calling the key text bury another type's reference — and which came first is
/// the store's alphabetical ordering, not anything the caller said.
fn relation_key<'a>(name: &str, declarations: &'a [types::DeclaredType]) -> Option<&'a str> {
    relation_field(name, declarations).map(|f| f.key.as_str())
}

/// **The declared key itself**, for a walk that has to read the cell the way
/// the declaration says it is written.
///
/// A key declared to hold a LIST holds its items separated by commas, and only
/// the declaration says so. A walk holding the key's NAME alone reads a
/// two-item cell as one handle nobody has.
fn relation_field<'a>(
    name: &str,
    declarations: &'a [types::DeclaredType],
) -> Option<&'a types::Field> {
    let name = name.trim();
    declarations.iter().find_map(|d| {
        d.field(name)
            .filter(|f| f.holds == types::ValueType::Reference)
    })
}

/// **Every relation these declarations make followable**, in name order — what
/// a caller who named one that is not there gets offered instead, and what a
/// reader of who-points-here asks about.
pub fn reference_keys(declarations: &[types::DeclaredType]) -> Vec<String> {
    relation_names(declarations)
}

/// **Every relation these declarations make followable**, in name order.
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
    // **The kind a selection names is the kind whose declaration governs its
    // filters.** A walk's `keeping` reaches objects of any kind, so no one kind's
    // declaration is asked of it and any declaration licenses.
    let named_kind = query.select.kind.map(|kind| kind.as_token());
    let selected = query.select.fields.iter().map(|field| (field, named_kind));
    let kept = query
        .follow
        .iter()
        .flat_map(|f| &f.keeping)
        .map(|field| (field, None));
    for (field, kind) in selected.chain(kept) {
        // A comparison against no value asks nothing the tree can read: the
        // leaf of a key with no value is only "is it carried".
        if field.tree.is_none() && field.compare.licensed_by().is_some() && field.value.is_none() {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' compares against a value and none was given",
                field.compare.as_token(),
            )));
        }
        // **The declaration is what licenses the operator**, and the tree judges
        // it: a comparison on a key nobody declared is refused, naming the
        // declaration that would license it.
        field.as_filter().validate(declarations, kind)?;
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
    /// **Every claim drawing this link is archived** — taken back, or
    /// replaced by a later one; a walk marks either the same way, because a
    /// reader deciding whether to act on a link does not need to know which.
    ///
    /// An archived claim keeps its link rather than losing it, for the reason
    /// a fact read keeps the claim: dropping it would make an archived claim
    /// and a claim nobody ever made the same answer, and those are different
    /// things a reader acts on differently.
    ///
    /// **A live claim wins.** Two records can draw the same link between the
    /// same pair, and the link is only marked when none of them stands.
    pub retracted: bool,
}

/// **One object in the answer**, and the objects it reached.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Object {
    /// The entity itself.
    pub entity: Entity,
    /// How the walk got here; `None` on a root.
    pub via: Option<Via>,
    /// **What this thing IS: the newest write of each key on it.**
    ///
    /// One value per key — the dense row a caller wants when the question is
    /// what the thing is rather than what was said about it. **Always here**,
    /// whatever else the query asked for, because it is the answer rather than
    /// a part of it, and a caller that had to fold the records itself would be
    /// doing jojobot's job — and would get it wrong, since which write won is
    /// something the records no longer say.
    ///
    /// **Folded over ALL of the thing's records, never the kept ones.** A
    /// filter says which objects the caller is after; it does not change what
    /// each one is. A row that shrank because the query narrowed would be a
    /// different thing on every call.
    pub fields: BTreeMap<String, String>,
    /// Its facts — the ones that answered the record filters, or all of them
    /// when none were given. Empty when facts were not asked for.
    pub facts: Vec<Fact>,
    /// **How many records are behind the fields**, whether or not they came
    /// back. It is what lets an answer that left them out say so, and say how
    /// much it left out — an empty [`Object::facts`] otherwise means both
    /// "nobody asked" and "nothing is recorded here".
    pub facts_held: usize,
    /// **How many of [`Object::facts_held`] were left off [`Object::facts`]
    /// because a shape in the listing already stands for them.**
    ///
    /// Zero whenever nothing was elided — nothing marked as a shape is in the
    /// listing, [`Include::stood_for`] asked for the sources back, or facts
    /// were not asked for at all, exactly as [`Object::facts`] is empty then
    /// too. Computed from the marks on every read, never stored, so it cannot
    /// drift from what the marks actually name.
    pub facts_folded: usize,
    /// **How many times each of [`Object::facts`] has been written**, keyed by
    /// the fact's local id. Present, one entry per fact, whenever facts were
    /// asked for — empty otherwise, exactly as `facts` is.
    ///
    /// **This is the signal, not the history.** A claim rewritten twice comes
    /// back field-for-field identical to one written once unless something
    /// says otherwise — this is that something, so a reader can tell "this is
    /// the only account" from "there was another wording here" without
    /// already holding the record's address. Reaching the earlier wording
    /// itself stays the separate, opt-in door it already was: `history`
    /// traces one record a caller names.
    pub fact_revisions: std::collections::HashMap<FactId, usize>,
    /// Its prose, whole. `None` when prose was not asked for, so a caller can
    /// tell "not asked for" from "the page is blank".
    pub prose: Option<String>,
    /// **How this THING answers the type the query named**, when it named one:
    /// the keys it holds, the keys it lacks by name, and any whose value is not
    /// what the type said it holds.
    ///
    /// Asked of the thing rather than of one record. A thing's fields are
    /// every write on it, folded, the newest write of each key winning, so a
    /// thing described over two sittings answers a type that neither sitting
    /// answers alone — which is how most things get written down.
    ///
    /// `None` when the query named no type.
    pub answers: Option<types::Match>,
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
    /// **The writes behind the key the query named**, on this object, oldest
    /// first. `None` when the query named no key, so a caller can tell "not
    /// asked for" from "nobody ever wrote it".
    pub history: Option<KeyHistory>,
    /// **The writes behind the RECORD the query named**, oldest first — what
    /// the claim used to say before somebody corrected it.
    ///
    /// **On the object the record is filed under and on no other.** A record's
    /// address names one claim on one thing, so an answer that hung it on every
    /// object in the walk would be repeating one thing's chain under handles
    /// that never carried it.
    ///
    /// `None` when the query named no record, and `None` on the other objects
    /// of a walk that did.
    pub record_history: Option<ClaimHistory>,
    /// **When the store took in what this object holds** — the moment of its
    /// newest write of any kind, and for each key the moment of the write
    /// holding its value. Filled by the store, not by this pure walk, for every
    /// object in the answer; `None` where the store could not say (a thing the
    /// memory store does not hold, such as a session). See [`FieldAges`].
    pub ages: Option<super::FieldAges>,
    /// **The thread this object is a standing decision of**, when the read of
    /// that thread put it ahead of the thread. `None` on every other object. It
    /// is what tells a renderer to serve the decision's words whether or not the
    /// caller asked for records: the words ARE the decision.
    pub standing_for: Option<EntityId>,
}

/// Every write of one key on one object, capped, and how many there are.
///
/// **The total is stored beside the writes because they are not the same
/// number.** A history that fits comes back whole and the two agree; a history
/// longer than the window comes back cut, and the total is what makes the cut
/// honest — a caller learns how much exists without being handed it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyHistory {
    /// The key asked for, exactly as the caller spelled it.
    pub key: String,
    /// The writes that came back, oldest first: **the newest ones**, when there
    /// were more than the window. Empty when nobody has written the key on this
    /// object — which is an answer, and a different one from `None`.
    ///
    /// The newest end is kept for the reason a session's chronology keeps its
    /// newest entries: what a thing has been doing lately is what a reader is
    /// nearly always after, and the far end of a long history is reachable by
    /// asking for a bigger window.
    pub writes: Vec<super::FieldWrite>,
    /// **How many writes exist behind the key**, whatever came back.
    pub total: usize,
}

impl KeyHistory {
    /// How many writes the window left out. Zero when the history fits, which
    /// is what a renderer branches on to keep an ordinary answer free of
    /// elision noise.
    pub fn elided(&self) -> usize {
        self.total.saturating_sub(self.writes.len())
    }
}

/// **Every write of one claim, capped, and how many there are.**
///
/// The same shape [`KeyHistory`] has and the same reason for the total beside
/// the writes: a chain longer than the window comes back cut, and the total is
/// what makes the cut honest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimHistory {
    /// The record asked for, exactly as the caller addressed it.
    pub record: FactAddress,
    /// The writes that came back, oldest first: **the newest ones**, when there
    /// were more than the window.
    ///
    /// A claim that stands has at least one write, so this is never empty — a
    /// record with nothing behind it is a miss rather than an answer.
    pub writes: Vec<super::ClaimWrite>,
    /// **How many writes exist behind the claim**, whatever came back.
    pub total: usize,
}

impl ClaimHistory {
    /// How many writes the window left out. Zero when the chain fits.
    pub fn elided(&self) -> usize {
        self.total.saturating_sub(self.writes.len())
    }
}

/// **A value one key already holds, and how many things hold it.**
///
/// The count is what makes the list usable for picking: a bare list cannot tell
/// a spelling forty things share from a typo one thing carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueInUse {
    /// The value as it is recorded, exactly.
    pub value: String,
    /// How many of the selected things hold it.
    pub things: usize,
}

/// **The values already recorded under one key**, over the objects a query
/// selected, most used first and then alphabetically.
///
/// **A read, never a rule.** Nothing here constrains a later write: a caller
/// picks a spelling that is already in use, or writes one that is not, and the
/// second is as ordinary as the first. What it removes is the guess — without
/// it, `dark green`, `Dark Green` and `darkgreen` become three values because
/// nobody could see the first one.
///
/// It is asked of each thing's FOLDED fields — what the thing holds now, one
/// value per key — so a value that was written and later replaced is not in
/// use, and a thing counts once however many times it was written.
pub fn values_in_use(objects: &[Object], key: &str) -> Vec<ValueInUse> {
    let key = key.trim();
    let mut counted: BTreeMap<&str, usize> = BTreeMap::new();
    for object in objects {
        if let Some(value) = object.fields.get(key) {
            *counted.entry(value.as_str()).or_default() += 1;
        }
    }
    let mut in_use: Vec<ValueInUse> = counted
        .into_iter()
        .map(|(value, things)| ValueInUse {
            value: value.to_string(),
            things,
        })
        .collect();
    // Most used first, because that is the order a caller picking one reads in.
    // Ties keep the alphabetical order the map already put them in, so the
    // answer is the same twice running.
    in_use.sort_by_key(|in_use| std::cmp::Reverse(in_use.things));
    in_use
}

/// **Whose writes to bring back, and how many of them.**
///
/// One value rather than two loose arguments, because a window with nothing to
/// window is not a question anybody can ask.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct History {
    /// What is being traced — a key, or one record.
    pub of: Trace,
    /// The most writes to return. Written more times than this and it comes
    /// back cut, saying so.
    pub most: usize,
}

/// **The two things that have writes behind them.**
///
/// One axis rather than two questions: both ask what the ordinary read projects
/// away, and they differ in what the writes are addressed by. A key's writes
/// are counted across every record that touched it; a claim's belong to one
/// claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trace {
    /// Every write of this key, on each object the query answered with.
    Key(String),
    /// Every write of this one claim, on the object it is filed under.
    Record(FactAddress),
}

/// **How many of a key's writes come back when the caller does not say.**
///
/// The same twenty `list_sent` shows of a bot's own mail, for the same reason:
/// it is enough to see the shape of what is there, and small enough that an
/// answer cannot bury the caller who asked one narrow question. A caller that
/// needs the far end asks for a bigger window.
pub const WRITES_SHOWN: usize = 20;

impl History {
    /// This key, with the default window.
    pub fn of(key: &str) -> Self {
        History {
            of: Trace::Key(key.to_string()),
            most: WRITES_SHOWN,
        }
    }
}

/// **The walk, over a store's own documents.** Pure: no I/O, no store, so the
/// graph semantics are one function that every adapter answers for identically.
///
/// The selection chooses where the walk STARTS. What the walk reaches is not
/// filtered again — a neighbour is in the answer because something pointed at
/// it, and re-applying the caller's filters at every hop would return a set
/// that is neither the neighbourhood nor the selection.
/// **What a selection answered, and what it kept back.**
///
/// The count is here rather than left to the caller because only the query
/// knows it: by the time a list of objects reaches anybody, what was filtered
/// out is indistinguishable from what was never there.
#[derive(Debug, Clone)]
pub struct Selected {
    /// The objects the caller may read, in handle order, or in the order the
    /// selection asked for.
    pub objects: Vec<Object>,
    /// **What the ordering did**, when the selection asked for one. It says
    /// whether a key read as text although its values would have parsed as
    /// numbers or dates, which the answer owes a caller who did not declare it.
    pub ordered: Option<Ordered>,
    /// **How many this selection matched and withheld as another identity's.**
    /// A total, never a breakdown: how many is work status, and which identity
    /// holds them is a directory of who is busy.
    pub withheld: usize,
    /// **How many records the chosen clock could not place.**
    ///
    /// A neighbourhood read on the taken-in stamp cannot place a record written
    /// before that stamp existed. Those records are not near the day and not
    /// far from it: **they are unreadable on this clock**, and a smaller answer
    /// that says nothing about them is indistinguishable from a day with
    /// nothing around it.
    ///
    /// **Zero is the ordinary answer** and it is a claim of its own: every
    /// record was placed, so an empty result means nothing was near.
    pub unplaced: usize,
    /// **How many this browse matched and archival took out**, the same
    /// shape `list_entities`'s own count uses: zero when nothing was
    /// archived, and zero for a selection naming a handle directly, since
    /// naming one never browses and archival never took it out of anything.
    pub archived_excluded: usize,
    /// **How many this browse matched and left out because they were folded
    /// into another thing**, counted apart from the archived ones: the two
    /// reasons are different, and a reader who sees one number cannot tell
    /// which it is looking at. Zero for a selection naming a handle.
    pub merged_excluded: usize,
    /// **What a call naming no kind left out because the object's own kind does
    /// not license the comparison asked of its key**, by kind. Only objects that
    /// would have matched had their kind licensed it are counted, so the list is
    /// what the caller lost and not everything the call did not match.
    pub unlicensed: Vec<Unlicensed>,
    /// **The decisions of a thread this read left out because they were
    /// archived**, named by handle. Empty unless the read named a thread. Each
    /// still answers by its own handle, which is how the one that replaced it
    /// is told apart from it.
    pub decisions_archived: Vec<EntityId>,
    /// **How many selected objects stand before the first one in `objects`**
    /// and were not read, because the caller's window began after them. Zero
    /// unless [`walk_in_window`] read a window.
    pub skipped: usize,
    /// **How many selected objects stand after the last one in `objects`** and
    /// were not read, because the window's room was spent before them. Zero
    /// unless [`walk_in_window`] read a window.
    pub left_over: usize,
}

/// **The objects of one kind a comparison did not reach**, and why: the kind
/// does not declare the key as the comparison needs, and `because` says which key
/// and what to declare.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unlicensed {
    pub kind: String,
    pub count: usize,
    pub because: String,
}

pub fn resolve(
    scanned: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<Selected, MemoryError> {
    query.validate()?;
    check_declared(query, declarations)?;
    let ctx = Ctx::of(scanned, declarations);
    ctx.check_expression_keys(query)?;
    let mut roots = ctx.roots(&query.select)?;
    let ordered = query.select.order.as_ref().map(|order| {
        let mut keyed: Vec<_> = roots
            .drain(..)
            .map(|id| {
                let held = ctx.held(&id);
                (id, held)
            })
            .collect();
        let ordered = order.sort(
            &mut keyed,
            |(_, held)| *held,
            declarations,
            query.select.kind.map(|kind| kind.as_token()),
        );
        roots.extend(keyed.into_iter().map(|(id, _)| id));
        ordered
    });
    let mut objects: Vec<Object> = roots
        .into_iter()
        .map(|id| {
            let mut seen = HashSet::from([id.clone()]);
            ctx.expand(&id, None, query, query.follow.as_ref(), &mut seen)
        })
        .collect();
    let decisions_archived = ctx.put_standing_decisions_first(&mut objects, query);
    Ok(Selected {
        objects,
        ordered,
        withheld: ctx.withheld(&query.select),
        unplaced: ctx.unplaced(&query.select),
        unlicensed: ctx.unlicensed_with_walk(&query.select),
        archived_excluded: ctx.archived_excluded(&query.select),
        merged_excluded: ctx.merged_excluded(&query.select),
        decisions_archived,
        skipped: 0,
        left_over: 0,
    })
}

impl Ctx<'_> {
    /// **A term of an expression on a key nothing holds is refused**, naming the
    /// keys that are held. An empty answer to it would read as *there is none*,
    /// and the likelier truth is a misspelt key.
    ///
    /// The population is the things the call could select (its kind, and a
    /// parent when it names one), asked in the scope the expression is asked in:
    /// the folded fields of a thing, or the fields of a record. **A key some type
    /// declares is a real question** even while nothing holds it. A key filter
    /// that is not an expression is not asked: it has always answered empty.
    fn check_expression_keys(&self, query: &GraphQuery) -> Result<(), MemoryError> {
        self.check_walk_keys(query)?;
        let select = &query.select;
        if select.subject.is_some() {
            return Ok(());
        }
        let population: Vec<&Entity> = self
            .entities
            .values()
            .copied()
            .filter(|e| self.matches_kind_and_parent(e, select))
            .collect();
        for field in select.fields.iter().filter(|f| f.tree.is_some()) {
            let tree = field.as_filter();
            let held_on_things: BTreeSet<&str> = population
                .iter()
                .filter_map(|e| self.held(&e.id))
                .flat_map(|fields| fields.keys().map(String::as_str))
                .collect();
            let held_on_records: BTreeSet<&str> = population
                .iter()
                .filter_map(|e| self.facts.get(&e.id))
                .flatten()
                .flat_map(|fact| fact.fields.keys().map(String::as_str))
                .collect();
            let (held_keys, other_keys, other_scope) = match field.scope {
                Scope::Thing => (&held_on_things, &held_on_records, Scope::Record),
                Scope::Record => (&held_on_records, &held_on_things, Scope::Thing),
            };
            for key in tree.keys() {
                let declared = self.declarations.iter().any(|d| d.field(key).is_some());
                if declared || held_keys.contains(key) {
                    continue;
                }
                // **A key held at the other scope is a scope mistake, not a
                // spelling mistake**, and the refusal names the scope that asks
                // it. A key some scope holds is never told to be respelt.
                if other_keys.contains(key) {
                    let advice = match other_scope {
                        Scope::Record => {
                            "the records of these things hold it, so ask the \
                            question of the records with scope: \"record\", or put the key on \
                            the thing with sets"
                        }
                        Scope::Thing => {
                            "the things hold it, so ask the question of what each \
                            thing holds, which is the default scope: leave scope: \"record\" off"
                        }
                    };
                    return Err(MemoryError::InvalidQuery(format!(
                        "no {} among the {} this call selected holds a key named '{key}' in \
                         the scope the question is asked in, but {advice}",
                        match field.scope {
                            Scope::Thing => "thing",
                            Scope::Record => "record",
                        },
                        population.len(),
                    )));
                }
                let in_use = if held_keys.is_empty() {
                    "none of the things this call selected holds any key".to_string()
                } else {
                    format!(
                        "the keys held among them are: {}",
                        held_keys.iter().copied().collect::<Vec<_>>().join(", ")
                    )
                };
                return Err(MemoryError::InvalidQuery(format!(
                    "no thing among the {} this call selected holds a key named '{key}', and no \
                     type declares one, so asking about it could only answer empty. Check the \
                     spelling: {in_use}",
                    population.len(),
                )));
            }
        }
        self.check_order_key(query, &population)
    }

    /// **A walk's `keeping` expression is checked for its keys as a selection's
    /// is.** What a walk reaches is not known before it runs, so the population is
    /// every thing the world holds: a key held by none of them, and declared by no
    /// type, could only answer empty. Asked of the folded fields, which is what a
    /// walk's expression is asked of.
    fn check_walk_keys(&self, query: &GraphQuery) -> Result<(), MemoryError> {
        let Some(follow) = query.follow.as_ref() else {
            return Ok(());
        };
        let held_keys: BTreeSet<&str> = self
            .entities
            .keys()
            .filter_map(|id| self.held(id))
            .flat_map(|fields| fields.keys().map(String::as_str))
            .collect();
        for field in follow.keeping.iter().filter(|f| f.tree.is_some()) {
            for key in field.as_filter().keys() {
                if held_keys.contains(key)
                    || self.declarations.iter().any(|d| d.field(key).is_some())
                {
                    continue;
                }
                return Err(MemoryError::InvalidQuery(format!(
                    "no thing this walk could reach holds a key named '{key}', and no type \
                     declares one, so keeping by it could only drop everything. Check the \
                     spelling: the keys held among them are: {}",
                    held_keys.iter().copied().collect::<Vec<_>>().join(", ")
                )));
            }
        }
        Ok(())
    }

    /// **An order on a key nothing holds is refused**, as a term on it is: every
    /// thing would tie and come back in handle order, which reads as an order.
    /// The population and the declarations are the ones a term is judged by, and
    /// the keys are the folded fields of the things, since an ordering reads what
    /// a thing holds now.
    fn check_order_key(
        &self,
        query: &GraphQuery,
        population: &[&Entity],
    ) -> Result<(), MemoryError> {
        let Some(order) = query.select.order.as_ref() else {
            return Ok(());
        };
        let key = order.key.trim();
        let held_keys: BTreeSet<&str> = population
            .iter()
            .filter_map(|e| self.held(&e.id))
            .flat_map(|fields| fields.keys().map(String::as_str))
            .collect();
        let declared = self.declarations.iter().any(|d| d.field(key).is_some());
        if declared || held_keys.contains(key) {
            return Ok(());
        }
        let in_use = if held_keys.is_empty() {
            "none of the things this call selected holds any key".to_string()
        } else {
            format!(
                "the keys held among them are: {}",
                held_keys.iter().copied().collect::<Vec<_>>().join(", ")
            )
        };
        Err(MemoryError::InvalidQuery(format!(
            "no thing among the {} this call selected holds a key named '{key}', and no type \
             declares one, so ordering by it would leave them in handle order. Check the \
             spelling: {in_use}",
            population.len(),
        )))
    }

    /// **Put the standing decisions of a thread ahead of it.** A read that names
    /// a thread gets its active `decision` children first, in handle order, each
    /// with its records whatever the caller asked of the thread: what was decided
    /// is held apart from what was gathered around it, and a reader that meets
    /// the research first comes away well informed and confidently wrong.
    ///
    /// **Only a read that names the thread.** A browse, a candidate set and a
    /// read of any other kind answer as they did. A decision that was archived is
    /// not in front, and the handles of those are returned so the answer can name
    /// what it left out.
    fn put_standing_decisions_first(
        &self,
        objects: &mut Vec<Object>,
        query: &GraphQuery,
    ) -> Vec<EntityId> {
        let Some(thread) = query.select.subject.as_ref() else {
            return Vec::new();
        };
        if query.select.candidates.is_some()
            || self.entities.get(thread).map(|e| e.kind) != Some(super::EntityKind::THREAD)
            || !objects.iter().any(|o| &o.entity.id == thread)
        {
            return Vec::new();
        }
        let children: Vec<&Entity> = self
            .entities
            .values()
            .copied()
            .filter(|e| {
                e.kind == super::EntityKind::DECISION
                    && e.parent.as_ref() == Some(thread)
                    && self.readable_by(&e.id, &query.select)
            })
            .collect();
        // Only the caller's identity carries over: the decisions are asked for
        // whole, and the thread's own filters say nothing about them.
        let asked = GraphQuery {
            select: Selection {
                asked_by: query.select.asked_by.clone(),
                ..Selection::default()
            },
            include: Include {
                facts: true,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        };
        let mut standing: Vec<Object> = children
            .iter()
            .filter(|e| e.browsable())
            .map(|e| {
                let mut seen = HashSet::from([e.id.clone()]);
                let mut decision = self.expand(&e.id, None, &asked, None, &mut seen);
                decision.standing_for = Some(thread.clone());
                decision
            })
            .collect();
        standing.sort_by(|a, b| a.entity.id.as_str().cmp(b.entity.id.as_str()));
        standing.append(objects);
        *objects = standing;
        let mut archived: Vec<EntityId> = children
            .iter()
            .filter(|e| e.archived.is_some())
            .map(|e| e.id.clone())
            .collect();
        archived.sort();
        archived
    }

    /// **Records the neighbourhood clock could not place**, across everything
    /// this selection could otherwise have reached.
    ///
    /// Counted over the store's records rather than over the answer, because a
    /// record that could not be placed never reaches the answer — which is the
    /// whole reason it has to be counted here. **But over the records this
    /// selection reaches, never over every record scanned:** a near-day read
    /// makes the walk fetch every entity, so a count taken over the scan is a
    /// fact about the store rather than about the question, and a caller who
    /// named a handle would be told their own records were unreadable because
    /// somebody else's were.
    ///
    /// Narrowed by [`Ctx::admits`] — the object's own properties, which is
    /// every narrowing that still holds once the clock is what is in doubt.
    /// The record filters are deliberately not applied: the near read is the
    /// filter being measured, and a record it cannot place cannot answer it.
    fn unplaced(&self, select: &Selection) -> usize {
        let Some(near) = select.near else {
            return 0;
        };
        self.all
            .iter()
            .filter(|fact| fact.status == crate::memory::FactStatus::Active)
            .filter(|fact| near.unplaceable(fact))
            // A record belongs to the page it is homed on as much as to the
            // thing it is about — the rule [`Ctx::facts`] is built by — so
            // either end being selected is this selection reaching it.
            .filter(|fact| self.admits(&fact.subject, select) || self.admits(&fact.home, select))
            .count()
    }
}

/// The store's documents, indexed the three ways a walk reads them.
struct Ctx<'a> {
    /// Every entity, by handle.
    entities: BTreeMap<&'a EntityId, &'a Entity>,
    /// **Who each owned object belongs to.** Absent for everything the whole
    /// instance can read, which is every stored row.
    owners: BTreeMap<&'a EntityId, &'a EntityId>,
    /// **The owned objects the walk reached and did not follow into**, once each
    /// however many links pointed at them. Filled while the answer is built and
    /// read after, so the answer can count what it left out at a hop.
    hops_withheld: std::cell::RefCell<BTreeSet<EntityId>>,
    /// **What a walk dropped because the dropped object's kind did not license a
    /// comparison its `keeping` asked**, by kind: the objects counted (each once,
    /// however many roots reach it) and the refusal that kind earns.
    walk_unlicensed: std::cell::RefCell<BTreeMap<String, (BTreeSet<EntityId>, String)>>,
    /// Every entity, in handle order — what the near-miss screen reads.
    index: Vec<Entity>,
    /// Each entity's prose.
    prose: BTreeMap<&'a EntityId, &'a str>,
    /// **What each thing IS**, as the store folded it from its writes. Read
    /// rather than derived: the records below cannot be folded back into this,
    /// because a record has already projected away which of its writes was the
    /// newest on the thing and which key it took off. See
    /// [`DocScan::fields`](super::search::DocScan::fields).
    fields: BTreeMap<&'a EntityId, &'a BTreeMap<String, String>>,
    /// The facts belonging to each entity: those ABOUT it, and those homed in
    /// its document whatever their subject says. The same rule
    /// [`recall`](super::Memory::recall) answers by, so one record does not
    /// belong to an entity through one verb and not another.
    facts: BTreeMap<&'a EntityId, Vec<&'a Fact>>,
    /// For each entity, the edges drawn AT it: the shape, and the entity whose
    /// record draws it. The reverse of the edge cell, built once.
    inbound: BTreeMap<EntityId, Vec<(EdgeShape, EntityId, bool)>>,
    /// For each entity, who mentioned it: the subject of every claim whose
    /// own words name it. The reverse of a mention, built in the same pass
    /// as `inbound`.
    ///
    /// **Needs facts already RENDERED** — a stored mention is a badge, not a
    /// handle, until something turns it back into one. `Ctx::of` is handed
    /// whatever `DocScan`s its caller fetched; over a bare `Memory` port
    /// (no `mention::Mentioning` decorator in front of it) a fact's content
    /// still carries the badge, `mention::named` finds nothing to resolve,
    /// and this map answers empty for a mention that is genuinely there.
    /// Production wires `Mentioning` in front of every store (`jojobot/src/
    /// wiring.rs`) for exactly this reason.
    mentioned_by: BTreeMap<EntityId, Vec<(EntityId, bool)>>,
    /// For each entity, who ref'd it: the subject of every claim whose
    /// `refs` names it. The reverse of `Fact::refs`, built in the same pass
    /// as `inbound`. Needs no rendering — `refs` is already handle form by
    /// the time a scan reaches here.
    ref_by: BTreeMap<EntityId, Vec<(EntityId, bool)>>,
    /// For each entity, who holds its handle as a whole field value, under
    /// any key. The reverse of [`Fact::linked`]'s field half, built in the
    /// same pass as `inbound`.
    field_by: BTreeMap<EntityId, Vec<(EntityId, bool)>>,
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
        let mut owners: BTreeMap<&EntityId, &EntityId> = BTreeMap::new();
        let mut prose = BTreeMap::new();
        let mut fields = BTreeMap::new();
        let mut facts: BTreeMap<&EntityId, Vec<&Fact>> = BTreeMap::new();
        let mut inbound: BTreeMap<EntityId, Vec<(EdgeShape, EntityId, bool)>> = BTreeMap::new();
        let mut mentioned_by: BTreeMap<EntityId, Vec<(EntityId, bool)>> = BTreeMap::new();
        let mut ref_by: BTreeMap<EntityId, Vec<(EntityId, bool)>> = BTreeMap::new();
        let mut field_by: BTreeMap<EntityId, Vec<(EntityId, bool)>> = BTreeMap::new();

        for doc in scanned {
            if let Some(entity) = doc.entity.as_ref() {
                if let Some(owner) = doc.owner.as_ref() {
                    owners.insert(&entity.id, owner);
                }
                entities.insert(&entity.id, entity);
                prose.insert(&entity.id, doc.prose.as_str());
                fields.insert(&entity.id, &doc.fields);
            }
            for fact in &doc.facts {
                facts.entry(&fact.subject).or_default().push(fact);
                if fact.home != fact.subject {
                    facts.entry(&fact.home).or_default().push(fact);
                }
                if let Some(Edge { shape, object }) = fact.edge.as_ref() {
                    inbound.entry(object.clone()).or_default().push((
                        *shape,
                        fact.subject.clone(),
                        fact.status == FactStatus::Archived,
                    ));
                }
                let retracted = fact.status == FactStatus::Archived;
                let mentioned = mention::named(&fact.content).into_iter().chain(
                    fact.details
                        .as_deref()
                        .map(mention::named)
                        .into_iter()
                        .flatten(),
                );
                for target in mentioned {
                    mentioned_by
                        .entry(target)
                        .or_default()
                        .push((fact.subject.clone(), retracted));
                }
                for target in &fact.refs {
                    ref_by
                        .entry(target.clone())
                        .or_default()
                        .push((fact.subject.clone(), retracted));
                }
                for target in fact.field_handles() {
                    field_by
                        .entry(target)
                        .or_default()
                        .push((fact.subject.clone(), retracted));
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
        // **A fact can name the same target twice** — `@X` written twice in
        // one sentence, or in both `content` and `details` — and a `refs`
        // list is free-form, so the caller could repeat an entry. One claim
        // draws one link to a reader, whichever route it used.
        for bucket in mentioned_by.values_mut() {
            let mut seen = HashSet::new();
            bucket.retain(|link| seen.insert(link.clone()));
        }
        for bucket in ref_by.values_mut() {
            let mut seen = HashSet::new();
            bucket.retain(|link| seen.insert(link.clone()));
        }
        for bucket in field_by.values_mut() {
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
            owners,
            hops_withheld: Default::default(),
            walk_unlicensed: Default::default(),
            index,
            prose,
            fields,
            facts,
            inbound,
            mentioned_by,
            ref_by,
            field_by,
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
                // **The screen sees only what the caller may read.** An owned
                // thing is named for its own words (a run is named for its
                // focus), so offering it as a near miss, or confirming it as a
                // same-name match, would hand its words to a caller that owns
                // none of it.
                let readable: Vec<Entity> = self
                    .index
                    .iter()
                    .filter(|e| self.readable_by(&e.id, select))
                    .cloned()
                    .collect();
                return Err(MemoryError::UnknownEntity {
                    attempted: subject.to_string(),
                    nearest: guard::screen(subject, &[], &readable),
                });
            }
            // **Naming a handle skips every filter below, which is why the
            // owner is checked HERE rather than only there.** The selection
            // path filters; the naming path returns the object it was given.
            if !self.readable_by(subject, select) {
                return Err(MemoryError::NotYours {
                    attempted: subject.to_string(),
                });
            }
            return Ok(vec![subject.clone()]);
        }
        let mut found: Vec<EntityId> = self
            .entities
            .values()
            .filter(|e| self.admits(&e.id, select))
            .filter(|e| {
                !select.filters_records()
                    || self.kept_facts(&e.id, select).next().is_some()
                    || self.run_started_near(e, select)
            })
            .map(|e| e.id.clone())
            .collect();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(found)
    }

    /// **Is this a run that started inside the window of a neighbourhood read.**
    /// A run holds no claim for a clock to place, so it is placed on the day it
    /// started, whichever clock is asked. A read that also asks a question of a
    /// record has nothing to put to a run, which holds none.
    fn run_started_near(&self, entity: &Entity, select: &Selection) -> bool {
        entity.kind == super::EntityKind::SESSION
            && select.fields.iter().all(|f| f.scope != Scope::Record)
            && select.near.is_some_and(|near| {
                self.held(&entity.id)
                    .and_then(|fields| fields.get(crate::session::STARTED_ON))
                    .and_then(|day| day.parse::<jiff::civil::Date>().ok())
                    .is_some_and(|day| near.holds_day(day))
            })
    }

    /// **Does this selection choose this object**, on the object's own
    /// properties — everything [`Ctx::roots`] narrows on before it asks
    /// anything of a record.
    ///
    /// One definition, because two readers ask it: which objects the answer is
    /// built from, and which records a count of what the clock could not place
    /// is taken over. A second copy could come to disagree, and the count would
    /// then be about a different question than the answer beside it.
    fn admits(&self, id: &EntityId, select: &Selection) -> bool {
        // **Naming a handle skips every filter below**, the way `roots` returns
        // the object it was given rather than filtering for it.
        if let Some(subject) = &select.subject {
            return subject == id;
        }
        let Some(entity) = self.entities.get(id) else {
            return false;
        };
        // **Archived is out of a browse, same as `list_entities`.** Naming a
        // handle already skipped this function above — this only ever runs
        // for a selection that chose the object rather than being given it,
        // and an archived thing is not something a browse chooses.
        entity.browsable() && self.admits_ignoring_archived(entity, select)
    }

    /// **Every filter `admits` asks except archival** — one definition, so
    /// [`Ctx::admits`] and [`Ctx::archived_excluded`] cannot drift apart
    /// about what "everything else matched" means.
    fn admits_ignoring_archived(&self, entity: &Entity, select: &Selection) -> bool {
        self.admits_if_licensed(entity, select) && self.licensed(entity, select).is_ok()
    }

    /// **Does this entity's own kind license every comparison the selection asks
    /// of its keys.** A call naming a kind was judged against that kind's
    /// declaration before any read, so this only has work to do when it names
    /// none. Otherwise `Err` carries the refusal that kind earns.
    fn licensed(&self, entity: &Entity, select: &Selection) -> Result<(), MemoryError> {
        if select.kind.is_some() {
            return Ok(());
        }
        select.fields.iter().try_for_each(|field| {
            field
                .as_filter()
                .validate(self.declarations, Some(entity.kind.as_token()))
        })
    }

    /// **Every filter `admits_ignoring_archived` asks except the kind's licence**,
    /// so the objects a licence left out can be counted over the same question.
    fn admits_if_licensed(&self, entity: &Entity, select: &Selection) -> bool {
        select.kind.is_none_or(|k| entity.kind == k)
            && select
                .parent
                .as_ref()
                .is_none_or(|p| entity.parent.as_ref() == Some(p))
            // **An owned object is its owner's alone.** Objects declaring no
            // owner are the whole store as it stands, and they answer everyone.
            && self.readable_by(&entity.id, select)
            // **The type is asked of the thing and the keys of its records.**
            // Two units, because they are two questions: whether this thing
            // carries a type's keys across everything said about it, and
            // whether one record describes what the caller is looking for.
            && (select.answers_type.is_none() || self.answers(&entity.id, select).is_some())
            // **A key filter is asked of the thing's folded fields by default,
            // which is the same map the type question is asked of.** Asked of
            // one record instead, "which of these have eaten three" misses the
            // thing that ate three one at a time and returns the thing that
            // recorded three at once — an answer that looks like an answer.
            && holds_all(self.held(&entity.id), select.fields.iter())
    }

    /// **What this selection matched and kept back as another identity's.**
    ///
    /// Counted over the same properties the selection filters on, so it answers
    /// "how many of the things you asked for are not yours" rather than "how
    /// many owned things exist".
    fn withheld(&self, select: &Selection) -> usize {
        self.entities
            .values()
            .filter(|e| self.matches_kind_and_parent(e, select))
            .filter(|e| !self.readable_by(&e.id, select))
            .count()
            + self.hops_withheld_beyond(select)
    }

    /// **The selection's licence drops and the walk's, one list by kind.** The walk
    /// fills its set while the answer is built, so this is read after it.
    fn unlicensed_with_walk(&self, select: &Selection) -> Vec<Unlicensed> {
        let mut listed = self.unlicensed(select);
        for (kind, (reached, because)) in self.walk_unlicensed.borrow().iter() {
            match listed.iter_mut().find(|u| &u.kind == kind) {
                Some(existing) => existing.count += reached.len(),
                None => listed.push(Unlicensed {
                    kind: kind.clone(),
                    count: reached.len(),
                    because: because.clone(),
                }),
            }
        }
        listed
    }

    /// Does this entity answer the selection's kind and parent, the two
    /// narrowings the withheld count is taken over.
    fn matches_kind_and_parent(&self, entity: &Entity, select: &Selection) -> bool {
        select.kind.is_none_or(|k| entity.kind == k)
            && select
                .parent
                .as_ref()
                .is_none_or(|p| entity.parent.as_ref() == Some(p))
    }

    /// **The objects the walk left out at a hop that the count above did not
    /// already take.** That count is over the objects the selection matched;
    /// an owned object reached by a link is not one of them unless it also
    /// answers the selection's kind and parent. Counted once each, and only
    /// after the answer is built, because the walk is what fills the set.
    fn hops_withheld_beyond(&self, select: &Selection) -> usize {
        self.hops_withheld
            .borrow()
            .iter()
            .filter_map(|id| self.entities.get(id))
            .filter(|e| !self.matches_kind_and_parent(e, select))
            .count()
    }

    /// **What this selection matched and archival took out.** Naming a
    /// handle directly never browses — [`Ctx::admits`] returns on the
    /// handle alone — so nothing here was excluded and this reports zero
    /// rather than guessing at a browse that never ran.
    fn archived_excluded(&self, select: &Selection) -> usize {
        if select.subject.is_some() {
            return 0;
        }
        self.entities
            .values()
            .filter(|e| e.archived.is_some())
            .filter(|e| self.admits_ignoring_archived(e, select))
            .count()
    }

    /// **What this selection would have matched and left out because the
    /// object's own kind does not license the comparison.** Grouped by kind, in
    /// kind order, each with the refusal that kind earns.
    fn unlicensed(&self, select: &Selection) -> Vec<Unlicensed> {
        if select.subject.is_some() || select.kind.is_some() {
            return Vec::new();
        }
        let mut by_kind: BTreeMap<&str, (usize, String)> = BTreeMap::new();
        for entity in self.entities.values().filter(|e| e.browsable()) {
            if !self.admits_if_licensed(entity, select)
                || (select.filters_records()
                    && self.kept_facts(&entity.id, select).next().is_none())
            {
                continue;
            }
            if let Err(MemoryError::InvalidQuery(because)) = self.licensed(entity, select) {
                let kind = entity.kind.as_token();
                by_kind.entry(kind).or_insert((0, because)).0 += 1;
            }
        }
        by_kind
            .into_iter()
            .map(|(kind, (count, because))| Unlicensed {
                kind: kind.to_string(),
                count,
                because,
            })
            .collect()
    }

    /// **What this selection matched and left out because the thing was folded
    /// into another.** The same rule as [`Ctx::archived_excluded`], for the
    /// other reason a browse omits a thing.
    fn merged_excluded(&self, select: &Selection) -> usize {
        if select.subject.is_some() {
            return 0;
        }
        self.entities
            .values()
            .filter(|e| e.left_out_as_folded())
            .filter(|e| self.admits_ignoring_archived(e, select))
            .count()
    }

    /// **May the caller read this object?**
    ///
    /// An object declaring no owner answers everyone — that is the whole store
    /// as it stands, and a rule that hid unowned things would empty every
    /// instance. An owned one answers its owner alone, and a caller carrying no
    /// identity owns nothing.
    fn readable_by(&self, id: &EntityId, select: &Selection) -> bool {
        match self.owners.get(id) {
            None => true,
            Some(owner) => select.asked_by.as_ref() == Some(*owner),
        }
    }

    /// **How this thing answers the selection's type**, over its fields folded
    /// together — `None` when no type was named, and when the thing carries
    /// none of its keys.
    fn answers(&self, id: &EntityId, select: &Selection) -> Option<types::Match> {
        let declared = select.answers_type.as_ref()?;
        declared.matched_by(&self.folded(id))
    }

    /// This thing's fields: what each key on it holds, as the store folded
    /// them.
    fn folded(&self, id: &EntityId) -> BTreeMap<String, String> {
        self.held(id).cloned().unwrap_or_default()
    }

    /// The same row, borrowed — `None` for a thing the scan has no fields for.
    ///
    /// **The filter path asks this once per candidate object**, so it does not
    /// take the copy [`Ctx::folded`] takes: a clone per candidate is a copy of
    /// the store's rows for a question that only reads them.
    fn held(&self, id: &EntityId) -> Option<&BTreeMap<String, String>> {
        self.fields.get(id).copied()
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
                for (via, reached) in reachable {
                    // **The owner is checked at every hop**, as it is where the
                    // walk starts. An owned object answers its owner alone, and a
                    // link to it is no reason to hand it to somebody else: it is
                    // left out, the object that pointed at it says an edge went
                    // unfollowed, and the answer counts it. Nothing of it is
                    // read first, so nothing of it can leak.
                    if !self.readable_by(&reached, &query.select) {
                        self.hops_withheld.borrow_mut().insert(reached);
                        unwalked = true;
                        continue;
                    }
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
                    //
                    // **Asked the same two ways a selection's are** — the
                    // thing's folded fields, and one record of its — because a
                    // filter that meant the record here and the thing there
                    // would carry the defect this scope exists to remove.
                    let matches = holds_all(self.held(&reached), follow.keeping.iter());
                    // **An object whose kind does not license a comparison is
                    // dropped, and counted**: only one that would have matched had
                    // its kind licensed it, as a selection counts its own.
                    let refused_licence = self.entities.get(&reached).and_then(|entity| {
                        follow.keeping.iter().find_map(|field| {
                            match field
                                .as_filter()
                                .validate(self.declarations, Some(entity.kind.as_token()))
                            {
                                Err(MemoryError::InvalidQuery(because)) => {
                                    Some((entity.kind.as_token().to_string(), because))
                                }
                                _ => None,
                            }
                        })
                    });
                    if let (true, Some((kind, because))) = (matches, &refused_licence) {
                        self.walk_unlicensed
                            .borrow_mut()
                            .entry(kind.clone())
                            .or_insert_with(|| (BTreeSet::new(), because.clone()))
                            .0
                            .insert(reached.clone());
                    }
                    if !matches
                        || refused_licence.is_some()
                        || (follow.filters_records()
                            && self.reached_facts(&reached, Some(follow)).is_empty())
                    {
                        unwalked = true;
                        continue;
                    }
                    // **And the type it has to fit**, which is a question about
                    // the object rather than about any one of its records: every
                    // key the type names, held across everything recorded about
                    // it. An object that answers the type in part is one this
                    // walk was not asking for.
                    if let Some(declared) = &follow.fits_type
                        && !declared
                            .matched_by(&self.folded(&reached))
                            .is_some_and(|found| found.complete())
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
                    connected.push(self.expand(&reached, Some(via), query, Some(&next), seen));
                }
            }
        }

        let answered = via
            .is_none()
            .then(|| self.answers(id, &query.select))
            .flatten();
        // **A shape's sources are folded out of the listing that also carries
        // the shape** — the shape already speaks for them in its own words,
        // so serving both would say the same thing twice. Computed from the
        // mark on every read, never stored: a cached count could overstate or
        // understate what a mark actually names. Off when the query asked for
        // the sources back, and moot when facts were not asked for at all.
        let folded: HashSet<(&EntityId, &FactId)> =
            if query.include.facts && !query.include.stood_for {
                kept.iter()
                    .flat_map(|f| f.stands_for.iter())
                    .map(|address| (&address.home, &address.local))
                    .collect()
            } else {
                HashSet::new()
            };
        let facts_folded = kept
            .iter()
            .filter(|f| folded.contains(&(&f.home, &f.id)))
            .count();
        Object {
            entity,
            via,
            fields: self.folded(id),
            // **The true total, never narrowed by a record filter.** `kept`
            // already answers the record filters — a caller's own scope — so
            // counting it here would report a scoped filter's narrowing as
            // the whole of what the thing holds. `facts_held` promises the
            // opposite: "unaffected by elision", the same claim it already
            // makes for a shape folding its sources out of the listing.
            facts_held: self.facts.get(id).map(Vec::len).unwrap_or(0),
            facts: if query.include.facts {
                kept.iter()
                    .filter(|f| !folded.contains(&(&f.home, &f.id)))
                    .map(|f| (*f).clone())
                    .collect()
            } else {
                Vec::new()
            },
            facts_folded,
            prose: query
                .include
                .prose
                .then(|| self.prose.get(id).copied().unwrap_or_default().to_string()),
            // **Only on a root.** The selection is what named a type, and it
            // describes where the walk starts; an object the walk reached was
            // reached because something pointed at it, not because it answers
            // anything.
            answers: answered,
            connected,
            unwalked,
            // **Filled by the store, not here.** The writes behind a key are
            // not on the records this pure walk reads: a record carries the
            // projection, and what it replaced is in the substrate. See
            // [`walk`].
            history: None,
            record_history: None,
            fact_revisions: std::collections::HashMap::new(),
            ages: None,
            standing_for: None,
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
        if !follow.is_some_and(Follow::filters_records) {
            return mine.collect();
        }
        let keeping = follow.map(|f| f.keeping.as_slice()).unwrap_or_default();
        let kept = all_of_scope(keeping.iter(), Scope::Record);
        mine.filter(|f| kept.holds(&f.fields)).collect()
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
    fn neighbours(&self, id: &EntityId, from: &[&Fact], follow: &Follow) -> Vec<(Via, EntityId)> {
        let direction = follow.direction();
        let mut found: Vec<(Via, EntityId)> = match &follow.along {
            Along::Relation(name) => self.along_relation(id, from, name, direction),
            Along::Edge(shape) => self.along_edge(id, from, Some(*shape), direction),
            // **A mention, a ref and a field value have no shape, so they
            // answer only an unscoped walk** — the same walk that already
            // meant "whatever it is connected to" before any of them
            // existed. A walk scoped to one edge shape or a relation is
            // unchanged: naming a shape asks for that shape, never for the
            // weaker admissions beside it.
            Along::AnyEdge => {
                let mut found = self.along_edge(id, from, None, direction);
                found.extend(self.along_mention(id, from, direction));
                found.extend(self.along_ref(id, from, direction));
                found.extend(self.along_field(id, from, direction));
                found
            }
        };
        found.retain(|(_, reached)| self.entities.contains_key(reached));
        // **A live claim sorts ahead of a taken-back one drawing the same
        // link**, and the dedup below keeps the first. That is what makes the
        // marker mean *nothing stands behind this* rather than *the newest
        // record happened to be retracted*.
        found.sort_by(|a, b| {
            a.1.as_str()
                .cmp(b.1.as_str())
                .then_with(|| a.0.link.token().cmp(b.0.link.token()))
                .then_with(|| a.0.retracted.cmp(&b.0.retracted))
        });
        found.dedup_by(|a, b| a.1 == b.1 && a.0.link == b.0.link);
        found
    }

    /// The entities an edge reaches from `id`, in the direction asked for —
    /// `shape` narrows to one, `None` is any.
    fn along_edge(
        &self,
        id: &EntityId,
        from: &[&Fact],
        shape: Option<EdgeShape>,
        direction: Direction,
    ) -> Vec<(Via, EntityId)> {
        match direction {
            Direction::Out => from
                .iter()
                .filter_map(|f| f.edge.as_ref().map(|e| (*f, e)))
                .filter(|(_, e)| shape.is_none_or(|s| e.shape == s))
                .map(|(f, e)| {
                    (
                        Via {
                            link: Link::Edge(e.shape),
                            direction,
                            retracted: f.status == FactStatus::Archived,
                        },
                        e.object.clone(),
                    )
                })
                .collect(),
            Direction::In => self
                .inbound
                .get(id)
                .into_iter()
                .flatten()
                .filter(|(shape_at, _, _)| shape.is_none_or(|s| *shape_at == s))
                .map(|(shape_at, drawn_by, retracted)| {
                    (
                        Via {
                            link: Link::Edge(*shape_at),
                            direction,
                            retracted: *retracted,
                        },
                        drawn_by.clone(),
                    )
                })
                .collect(),
        }
    }

    /// The entities a mention reaches from `id`, in the direction asked for.
    /// **Out** reads `from`'s own content and details directly — the same
    /// pass `mention::named` runs over one fact rather than a scan of
    /// everything. **In** reads [`Ctx::mentioned_by`], the reverse built once
    /// when this walk began.
    fn along_mention(
        &self,
        id: &EntityId,
        from: &[&Fact],
        direction: Direction,
    ) -> Vec<(Via, EntityId)> {
        match direction {
            Direction::Out => from
                .iter()
                .flat_map(|f| {
                    let retracted = f.status == FactStatus::Archived;
                    mention::named(&f.content)
                        .into_iter()
                        .chain(
                            f.details
                                .as_deref()
                                .map(mention::named)
                                .into_iter()
                                .flatten(),
                        )
                        .map(move |target| {
                            (
                                Via {
                                    link: Link::Mention,
                                    direction,
                                    retracted,
                                },
                                target,
                            )
                        })
                })
                .collect(),
            Direction::In => self
                .mentioned_by
                .get(id)
                .into_iter()
                .flatten()
                .map(|(mentioned_by, retracted)| {
                    (
                        Via {
                            link: Link::Mention,
                            direction,
                            retracted: *retracted,
                        },
                        mentioned_by.clone(),
                    )
                })
                .collect(),
        }
    }

    /// The entities a `refs` entry reaches from `id`, in the direction asked
    /// for — the mirror of [`Ctx::along_mention`], over a structured list
    /// rather than text.
    fn along_ref(
        &self,
        id: &EntityId,
        from: &[&Fact],
        direction: Direction,
    ) -> Vec<(Via, EntityId)> {
        match direction {
            Direction::Out => from
                .iter()
                .flat_map(|f| {
                    let retracted = f.status == FactStatus::Archived;
                    f.refs.iter().map(move |target| {
                        (
                            Via {
                                link: Link::Ref,
                                direction,
                                retracted,
                            },
                            target.clone(),
                        )
                    })
                })
                .collect(),
            Direction::In => self
                .ref_by
                .get(id)
                .into_iter()
                .flatten()
                .map(|(referrer, retracted)| {
                    (
                        Via {
                            link: Link::Ref,
                            direction,
                            retracted: *retracted,
                        },
                        referrer.clone(),
                    )
                })
                .collect(),
        }
    }

    /// The entities a field value reaches from `id`, in the direction asked
    /// for — the mirror of [`Ctx::along_ref`], over a record's fields rather
    /// than its `refs`. **A value that is a handle is a link whatever its key**,
    /// declared or not; the existence retain in [`Ctx::neighbours`] drops one
    /// naming nothing.
    fn along_field(
        &self,
        id: &EntityId,
        from: &[&Fact],
        direction: Direction,
    ) -> Vec<(Via, EntityId)> {
        match direction {
            Direction::Out => from
                .iter()
                .flat_map(|f| {
                    let retracted = f.status == FactStatus::Archived;
                    f.field_handles().into_iter().map(move |target| {
                        (
                            Via {
                                link: Link::Field,
                                direction,
                                retracted,
                            },
                            target,
                        )
                    })
                })
                .collect(),
            Direction::In => self
                .field_by
                .get(id)
                .into_iter()
                .flatten()
                .map(|(holder, retracted)| {
                    (
                        Via {
                            link: Link::Field,
                            direction,
                            retracted: *retracted,
                        },
                        holder.clone(),
                    )
                })
                .collect(),
        }
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
    ) -> Vec<(Via, EntityId)> {
        // ⚠️ **UNREACHABLE, AND DELIBERATELY KEPT.** No test reaches this
        // branch, and the comment exists so the next reader knows the coverage
        // is absent rather than assuming it: `check_declared` refuses a
        // relation no declaration backs before any walk starts, so a name that
        // gets this far has already resolved once.
        //
        // It stays because the refusal and the lookup are in different
        // functions, and an empty walk is the right answer if they ever
        // disagree. Nothing downstream depends on it.
        let Some(field) = relation_field(name, self.declarations) else {
            return Vec::new();
        };
        let key = field.key.as_str();
        let link = || Link::Relation(key.to_string());
        // **Each item, the way the declaration says the cell is written.** An
        // ordinary key carries one handle and a list carries several; asking
        // the field is what tells the two apart, and it is the same accessor
        // the write guard screens each handle through (rule 217).
        match direction {
            Direction::Out => from
                .iter()
                .filter_map(|f| f.fields.get(key).map(|cell| (*f, cell)))
                .flat_map(|(f, cell)| {
                    let retracted = f.status == FactStatus::Archived;
                    field
                        .items(cell)
                        .into_iter()
                        .map(|handle| {
                            (
                                Via {
                                    link: link(),
                                    direction,
                                    retracted,
                                },
                                EntityId(handle.to_string()),
                            )
                        })
                        .collect::<Vec<_>>()
                })
                .collect(),
            Direction::In => self
                .all
                .iter()
                .filter(|f| {
                    f.fields
                        .get(key)
                        .is_some_and(|cell| field.items(cell).contains(&id.as_str()))
                })
                .map(|f| {
                    (
                        Via {
                            link: link(),
                            direction,
                            retracted: f.status == FactStatus::Archived,
                        },
                        f.subject.clone(),
                    )
                })
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
///
/// **`sessions` is pre-projected, never read here.** A session carries no row
/// the store's own scan would find, so a caller that wants one reachable
/// fetches it through the `Sessions` port and turns it into a [`DocScan`] with
/// [`super::super::session::projected`] before calling this — the same shape
/// [`list_entities`](super::Memory::list_entities) hands every ordinary
/// entity, so the rest of this walk (subject resolution, kind filtering,
/// owner scoping) does not have to know sessions exist at all. Empty for
/// every caller that is not asking about sessions.
pub async fn walk<M>(
    store: &M,
    sessions: &[DocScan],
    query: &GraphQuery,
) -> Result<Selected, MemoryError>
where
    M: super::Memory + ?Sized,
{
    walk_in_window(store, sessions, query, None).await
}

/// **The part of a browse a caller will actually serve.**
///
/// `offset` objects of the selection stand before it and are not read. The
/// objects after them are read one at a time, in answer order, and each adds
/// what `size_of` says it will cost to `budget`; the read stops after the first
/// object that takes the total past `budget`, and the objects behind it are not
/// read. `size_of` has to be a LOWER bound of what an object costs when served:
/// a read that stopped early on an overestimate would leave out an object that
/// fits, and the answer says what it left out as a count, never as a guess.
pub struct Window<'a> {
    /// How many objects of the selection stand before the window.
    pub offset: usize,
    /// The room the served objects have.
    pub budget: usize,
    /// What one object costs when served, at least.
    pub size_of: &'a (dyn Fn(&Object) -> usize + Sync),
}

/// [`walk`], reading only the objects a window will serve when the question
/// lets the selection be decided without reading them.
///
/// **A browse by kind or parent that filters on nothing a thing holds** is
/// decided by the entities alone, so the objects are selected from the index and
/// read afterwards, in answer order, until the window's room is spent. Any other
/// question reads what it always read and answers in full: `skipped` and
/// `left_over` are then zero, and the caller finds the objects it expects.
pub async fn walk_in_window<M>(
    store: &M,
    sessions: &[DocScan],
    query: &GraphQuery,
    window: Option<Window<'_>>,
) -> Result<Selected, MemoryError>
where
    M: super::Memory + ?Sized,
{
    query.validate()?;
    // The entity index: what exists, what a kind selects, and what a handle
    // that names nothing is screened against.
    //
    // **A browse by one kind lists that kind alone.** With no subject and no walk
    // nothing is resolved, screened or reached across kinds: every object the
    // answer holds, and every count it gives (archived, folded, another
    // identity's), is taken over entities of the kind selected. A subject is
    // resolved and screened against everything that exists, and a walk reaches
    // any kind, so those two ask for the whole listing. Sessions are joined
    // below whatever was listed, and a kind filter drops the ones that are not
    // wanted.
    let listed = match (&query.select.subject, &query.follow) {
        (None, None) => query.select.kind,
        _ => None,
    };
    let mut entities = store.list_entities(listed).await?;
    // **Sessions join the same index they are selected and resolved from.**
    // A subject naming one has to find it here to resolve at all, and a kind
    // filter has to see it to select it — both read `entities`, below.
    entities.extend(sessions.iter().filter_map(|doc| doc.entity.clone()));
    let select = &query.select;
    // **Three tiers (decision log 272): a current handle answers directly, a
    // handle the subject used to wear resolves to what it is called now, and
    // only a handle nothing has ever answered to is a near-miss screen.** A
    // rename rewrites nothing (rule 243), so a subject reached by an old name
    // is not absent — `Memory::recall` on the trait already resolves this;
    // this walk gates on the current index alone and refuses it, which is
    // the bug this rule closes.
    let resolved_subject = match &select.subject {
        Some(subject) => {
            let former = store.former_handles().await?;
            match super::resolve_handle(subject, &entities, &former) {
                Some(current) => Some(current.id.clone()),
                None => {
                    // **The screen sees only what the caller may read.** A run
                    // is named for its focus and owned by a bot, so offering
                    // it as a near miss, or confirming it as a same-name
                    // match, would hand its words to a caller that owns none
                    // of it. The same rule `Ctx::readable_by` states for an
                    // answer, applied before there is a `Ctx`.
                    let hidden: HashSet<&EntityId> = sessions
                        .iter()
                        .filter(|doc| {
                            doc.owner
                                .as_ref()
                                .is_some_and(|owner| select.asked_by.as_ref() != Some(owner))
                        })
                        .filter_map(|doc| doc.entity.as_ref().map(|e| &e.id))
                        .collect();
                    let readable: Vec<Entity> = entities
                        .iter()
                        .filter(|e| !hidden.contains(&e.id))
                        .cloned()
                        .collect();
                    return Err(MemoryError::UnknownEntity {
                        attempted: subject.to_string(),
                        nearest: guard::screen(subject, &[], &readable),
                    });
                }
            }
        }
        None => None,
    };
    // **Every read below, and `resolve` itself, has to see the CURRENT
    // handle** — `scanned` is built from `entities`, which never carries a
    // stale one, so a query still holding the handle the caller sent would
    // find no root to start from. Owned rather than a second borrow, since
    // the original `query` is still read for `follow`/`include`/`history`.
    let mut query = query.clone();
    query.select.subject = resolved_subject.clone();
    let query = &query;
    let select = &query.select;

    // **A browse that asks nothing of what a thing holds is decided by the
    // entities alone**, so its objects are selected from the index and read
    // afterwards, in answer order, until the window's room is spent. A question
    // that reads fields or records to choose, that follows links, that names a
    // handle or a candidate set, or that joins runs reads what it always read.
    if let Some(window) = &window
        && sessions.is_empty()
        && resolved_subject.is_none()
        && query.follow.is_none()
        && query.history.is_none()
        && query.select.candidates.is_none()
        && query.select.kind != Some(super::EntityKind::SESSION)
        && !query.select.filters_facts()
        // An ordering reads the values of a key, which the shells below do not
        // hold: ordered over them, every object ties and handle order comes back.
        && query.select.order.is_none()
    {
        let shells: Vec<DocScan> = entities
            .iter()
            .map(|entity| DocScan {
                doc_id: entity.id.to_string(),
                title: entity.name.clone(),
                prose: String::new(),
                facts: Vec::new(),
                fields: BTreeMap::new(),
                entity: Some(entity.clone()),
                owner: None,
            })
            .collect();
        let declarations = if query.needs_declarations() {
            store.declared_types().await?
        } else {
            Vec::new()
        };
        let selected = resolve(&shells, &declarations, query)?;
        let total = selected.objects.len();
        let skipped = window.offset.min(total);
        let mut objects: Vec<Object> = Vec::new();
        let mut spent = 0usize;
        let mut passed = skipped;
        for shell in &selected.objects[skipped..] {
            passed += 1;
            let doc = read_doc(store, &shell.entity, query.include.prose).await?;
            let Some(mut object) = resolve(std::slice::from_ref(&doc), &declarations, query)?
                .objects
                .pop()
            else {
                continue;
            };
            fill_revision_counts(store, &mut object).await?;
            fill_ages(store, &mut object).await;
            spent += (window.size_of)(&object);
            objects.push(object);
            // The object that takes the total past the room is read and kept: it
            // is the first the caller will find does not fit, and the ones behind
            // it cannot.
            if spent > window.budget {
                break;
            }
        }
        return Ok(Selected {
            objects,
            skipped,
            left_over: total - passed,
            ..selected
        });
    }

    // **Whose records are needed.** A walk cannot know where it will go, and a
    // filter over records cannot choose objects without reading theirs — both
    // need all of them. Everything else needs only what it named.
    //
    // **`candidates`, when the caller already resolved one, is the whole of
    // this answer** — it does not merely add to `everyones`, it replaces the
    // question. A caller handing in a candidate set already paid the cost
    // `everyones` exists to avoid asking again; reopening the full list here
    // would spend it a second time.
    let everyones = query.follow.is_some() || select.filters_facts();
    let wanted: Vec<&Entity> = entities
        .iter()
        .filter(|e| match &select.candidates {
            Some(candidates) => candidates.contains(&e.id),
            None => {
                everyones
                    || resolved_subject.as_ref() == Some(&e.id)
                    || (e.kind == super::EntityKind::DECISION
                        && resolved_subject.is_some()
                        && e.parent == resolved_subject)
                    || (resolved_subject.is_none()
                        && select.kind.is_none_or(|k| e.kind == k)
                        && select
                            .parent
                            .as_ref()
                            .is_none_or(|p| e.parent.as_ref() == Some(p)))
            }
        })
        .collect();

    // A document per entity, so a kind selection and a near-miss screen see
    // everything that exists — with records and prose filled in only for the
    // entities this query actually reads.
    let mut scanned: Vec<DocScan> = Vec::with_capacity(entities.len());
    for entity in &entities {
        // **A session is projected whole, never scanned.** It carries no row
        // any of the calls below could read; the caller already built its
        // document, and this only has to find the one that matches.
        if entity.kind == super::EntityKind::SESSION {
            if let Some(doc) = sessions
                .iter()
                .find(|doc| doc.entity.as_ref().map(|e| &e.id) == Some(&entity.id))
            {
                scanned.push(doc.clone());
            }
            continue;
        }
        if wanted.iter().any(|w| w.id == entity.id) {
            scanned.push(read_doc(store, entity, query.include.prose).await?);
        } else {
            scanned.push(DocScan {
                doc_id: entity.id.to_string(),
                title: entity.name.clone(),
                prose: String::new(),
                facts: Vec::new(),
                fields: BTreeMap::new(),
                entity: Some(entity.clone()),
                owner: None,
            });
        }
    }
    // **Read only for the questions that need it.** A declaration is what makes
    // a key a relation and what licenses an ordering, and a query asking for
    // neither is answered without it.
    let declarations = if query.needs_declarations() {
        store.declared_types().await?
    } else {
        Vec::new()
    };
    let mut found = resolve(&scanned, &declarations, query)?;
    // **The history is read after the shape is decided**, once per object in
    // the answer: the walk cannot know which objects it will return, and a read
    // of every entity's writes would pay for the ones nobody asked about.
    if let Some(wanted) = &query.history {
        for object in &mut found.objects {
            fill_history(store, object, wanted).await?;
        }
    }
    // **Read the same way, and for the same reason — but never opt-in.** A
    // walk that did not ask for facts has none on any object, so this counts
    // nothing on it either: the signal follows `include.facts` on its own
    // rather than needing a flag of its own. See [`Object::fact_revisions`].
    for object in &mut found.objects {
        fill_revision_counts(store, object).await?;
    }
    // **How old what each object holds is**, read for every object in the
    // answer. A store that cannot say, or does not hold the object, leaves it
    // without an age; the answer is never refused over one.
    for object in &mut found.objects {
        fill_ages(store, object).await;
    }
    Ok(found)
}

/// **One entity's document, read from the store**: its records, its prose when
/// asked for, and its fields.
async fn read_doc<M>(store: &M, entity: &Entity, prose: bool) -> Result<DocScan, MemoryError>
where
    M: super::Memory + ?Sized,
{
    let facts = store.recall(&entity.id).await?;
    let prose = if prose {
        store
            .scan_entity(&entity.id)
            .await?
            .map(|doc| doc.prose)
            .unwrap_or_default()
    } else {
        String::new()
    };
    // **Asked of the store, never folded from the records above.** What a thing
    // holds is decided in the order its keys were written, which the records have
    // already projected away — so a walk that folded them for itself would answer
    // with a value the store does not hold.
    let fields = store.fields(&entity.id).await?;
    Ok(DocScan {
        doc_id: entity.id.to_string(),
        title: entity.name.clone(),
        prose,
        facts,
        fields,
        entity: Some(entity.clone()),
        owner: None,
    })
}

/// Attach, to an object and everything it reached, when the store took in what
/// it holds. See [`Object::ages`].
fn fill_ages<'a, M>(
    store: &'a M,
    object: &'a mut Object,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>>
where
    M: super::Memory + ?Sized,
{
    Box::pin(async move {
        object.ages = store.field_ages(&object.entity.id).await.ok();
        for reached in &mut object.connected {
            fill_ages(store, reached).await;
        }
    })
}

/// Attach, to every fact on an object and everything it reached, how many
/// times that claim has been written. See [`Object::fact_revisions`].
fn fill_revision_counts<'a, M>(
    store: &'a M,
    object: &'a mut Object,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), MemoryError>> + Send + 'a>>
where
    M: super::Memory + ?Sized,
{
    Box::pin(async move {
        for fact in &object.facts {
            let total = store.claim_history(&fact.address()).await?.len();
            object.fact_revisions.insert(fact.id.clone(), total);
        }
        for reached in &mut object.connected {
            fill_revision_counts(store, reached).await?;
        }
        Ok(())
    })
}

/// Attach the writes the query asked for to an object and to everything it
/// reached.
///
/// **A key is asked of every object in the answer, roots and reached alike.** A
/// caller that named a key asked it of the answer, and a walked object arriving
/// without the half its siblings carry is the silent elision this project does
/// not do.
///
/// **A record is asked of the one object it is filed under.** Its address names
/// one claim on one thing; hanging that chain on every object of a walk would
/// report one thing's writes under handles that never carried them.
fn fill_history<'a, M>(
    store: &'a M,
    object: &'a mut Object,
    wanted: &'a History,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), MemoryError>> + Send + 'a>>
where
    M: super::Memory + ?Sized,
{
    Box::pin(async move {
        match &wanted.of {
            Trace::Key(key) => {
                let mut writes = store.history(&object.entity.id, key).await?;
                let total = writes.len();
                // **The window is cut from the old end, and the answer keeps
                // its order.** A caller reading a capped history reads the
                // newest writes oldest-first, which is the same shape a short
                // history has — so nothing has to be read differently because
                // it was cut.
                if total > wanted.most {
                    writes.drain(..total - wanted.most);
                }
                object.history = Some(KeyHistory {
                    key: key.clone(),
                    writes,
                    total,
                });
            }
            Trace::Record(address) if address.home == object.entity.id => {
                let mut writes = store.claim_history(address).await?;
                let total = writes.len();
                if total > wanted.most {
                    writes.drain(..total - wanted.most);
                }
                object.record_history = Some(ClaimHistory {
                    record: address.clone(),
                    writes,
                    total,
                });
            }
            Trace::Record(_) => {}
        }
        for reached in &mut object.connected {
            fill_history(store, reached, wanted).await?;
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;
