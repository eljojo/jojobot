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

use std::collections::{BTreeMap, HashSet};

use super::{
    Edge, EdgeShape, Entity, EntityId, EntityKind, Fact, FactAddress, FactId, FactStatus,
    MemoryError, guard, search::DocScan, types, validate_subject,
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
}

impl FieldFilter {
    /// A filter asking only that the key is there.
    pub fn key(key: &str) -> Self {
        FieldFilter {
            key: Some(key.trim().to_string()),
            value: None,
            compare: types::Compare::Equals,
            scope: Scope::Thing,
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

    /// The same filter asked of one record rather than of the thing.
    pub fn on_a_record(self) -> Self {
        FieldFilter {
            scope: Scope::Record,
            ..self
        }
    }

    /// Does this bag of fields satisfy the filter. It is handed the thing's
    /// folded fields or one record's, and does not know which.
    fn satisfied_by(&self, fields: &BTreeMap<String, String>) -> bool {
        match (&self.key, &self.value) {
            (Some(key), None) => fields.contains_key(key),
            (Some(key), Some(wanted)) => fields
                .get(key)
                .is_some_and(|held| self.compare.holds_between(held, wanted)),
            // **Any key at all.** The bag is the thing's folded fields or one
            // record's, and never its prose — so this answers *held as a
            // value* rather than *written down somewhere*.
            (None, Some(wanted)) => fields
                .values()
                .any(|held| self.compare.holds_between(held, wanted)),
            // Refused before it reaches here; false rather than true so a
            // filter that asked nothing cannot select everything.
            (None, None) => false,
        }
    }
}

/// **A filter has to ask something.**
///
/// A key spelled empty is a caller mistake, and so is a filter naming neither a
/// key nor a value: it selects everything or nothing depending on which way the
/// predicate happens to fall, and neither is an answer anybody asked for.
fn validate_filter(field: &FieldFilter) -> Result<(), MemoryError> {
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
    filters
        .filter(|f| f.scope == Scope::Thing)
        .all(|f| held.is_some_and(|fields| f.satisfied_by(fields)))
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
        self.subject.is_none() && self.kind.is_none() && !self.filters_facts()
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
        self.fields
            .iter()
            .filter(|f| f.scope == Scope::Record)
            .all(|f| f.satisfied_by(&fact.fields))
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
    fn holds(&self, fact: &Fact) -> bool {
        let Some(at) = self.placed(fact) else {
            return false;
        };
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
        if select.narrows_nothing() {
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
    /// **A key filter for every record on the view that carries one** — see
    /// [`asked_by_view`] for the shape a filter record takes. Several
    /// filters are several records; nothing here caps it at one per key.
    pub filters: Vec<FieldFilter>,
    /// **A type name to select structurally, by the key `answers_type`** —
    /// unresolved: the caller of [`asked_by_view`] still has to look the
    /// name up, the same way it already does for the argument of the same
    /// name.
    pub answers_type: Option<String>,
    /// **A relation walk, unresolved the same way `answers_type` is.**
    /// `None` when the view names no walk at all — distinct from a walk
    /// that names no shape or relation, which is "any edge".
    pub follow: Option<AskedFollow>,
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
        facts: shows("facts"),
        prose: shows("prose"),
        filters,
        charter: shows("charter"),
        overdue: held.get("asks").map(String::as_str) == Some("overdue"),
        answers_type: held.get("answers_type").cloned(),
        follow,
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
/// **Nothing is inferred.** A value that looks like a handle under a key nobody
/// declared is a string that looks like a handle.
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
        // **An ordering needs a key to license it.** A filter asking about the
        // value under any key at all has no declaration to read, so there is
        // nothing that could permit a comparison other than equality.
        let Some(key) = field.key.as_deref() else {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' needs a key: an ordering is licensed by what that key was declared to \
                 hold, and a filter naming no key has no declaration to read. Name the key, or \
                 ask for the value itself",
                field.compare.as_token(),
            )));
        };
        if !declarations
            .iter()
            .any(|d| d.field(key).is_some_and(|f| f.holds == holds))
        {
            return Err(MemoryError::InvalidQuery(format!(
                "'{}' needs a type declaring '{}' to hold a {}. Declare one, or ask for the value \
                 itself",
                field.compare.as_token(),
                key,
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
    /// Asked of the thing rather than of one record. A thing's fields are its
    /// records' fields folded together, so a thing described over two sittings
    /// answers a type that neither sitting answers alone — which is how most
    /// things get written down.
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

    /// This record's own writes, with the default window.
    pub fn of_record(record: FactAddress) -> Self {
        History {
            of: Trace::Record(record),
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
    /// The objects the caller may read, in handle order.
    pub objects: Vec<Object>,
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
}

pub fn resolve(
    scanned: &[DocScan],
    declarations: &[types::DeclaredType],
    query: &GraphQuery,
) -> Result<Selected, MemoryError> {
    query.validate()?;
    check_declared(query, declarations)?;
    let ctx = Ctx::of(scanned, declarations);
    let roots = ctx.roots(&query.select)?;
    Ok(Selected {
        objects: roots
            .into_iter()
            .map(|id| {
                let mut seen = HashSet::from([id.clone()]);
                ctx.expand(&id, None, query, query.follow.as_ref(), &mut seen)
            })
            .collect(),
        withheld: ctx.withheld(&query.select),
        unplaced: ctx.unplaced(&query.select),
        archived_excluded: ctx.archived_excluded(&query.select),
    })
}

impl Ctx<'_> {
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
            owners,
            index,
            prose,
            fields,
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
            // **Naming a handle skips every filter below, which is why the
            // owner is checked HERE rather than only there.** The selection
            // path filters; the naming path returns the object it was given.
            if !self.readable_by(subject, select) {
                return Err(MemoryError::NotYours {
                    attempted: subject.to_string(),
                    owner: self
                        .owners
                        .get(subject)
                        .map(|owner| owner.to_string())
                        .unwrap_or_default(),
                });
            }
            return Ok(vec![subject.clone()]);
        }
        let mut found: Vec<EntityId> = self
            .entities
            .values()
            .filter(|e| self.admits(&e.id, select))
            .filter(|e| {
                !select.filters_records() || self.kept_facts(&e.id, select).next().is_some()
            })
            .map(|e| e.id.clone())
            .collect();
        found.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        Ok(found)
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
        entity.archived.is_none() && self.admits_ignoring_archived(entity, select)
    }

    /// **Every filter `admits` asks except archival** — one definition, so
    /// [`Ctx::admits`] and [`Ctx::archived_excluded`] cannot drift apart
    /// about what "everything else matched" means.
    fn admits_ignoring_archived(&self, entity: &Entity, select: &Selection) -> bool {
        select.kind.is_none_or(|k| entity.kind == k)
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
            .filter(|e| select.kind.is_none_or(|k| e.kind == k))
            .filter(|e| !self.readable_by(&e.id, select))
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
                    if !holds_all(self.held(&reached), follow.keeping.iter())
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
        mine.filter(|f| {
            keeping
                .iter()
                .filter(|k| k.scope == Scope::Record)
                .all(|k| k.satisfied_by(&f.fields))
        })
        .collect()
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
        let mut found: Vec<(Via, EntityId)> = match &follow.along {
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
pub async fn walk<M>(store: &M, query: &GraphQuery) -> Result<Selected, MemoryError>
where
    M: super::Memory + ?Sized,
{
    query.validate()?;
    // The entity index: what exists, what a kind selects, and what a handle
    // that names nothing is screened against.
    let entities = store.list_entities(None).await?;
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
                    return Err(MemoryError::UnknownEntity {
                        attempted: subject.to_string(),
                        nearest: guard::screen(subject, &[], &entities),
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
                    || (resolved_subject.is_none() && select.kind.is_none_or(|k| e.kind == k))
            }
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
        // **Asked of the store, never folded from the records above.** What a
        // thing holds is decided in the order its keys were written, which the
        // records have already projected away — so a walk that folded them for
        // itself would answer with a value the store does not hold.
        let fields = if read {
            store.fields(&entity.id).await?
        } else {
            BTreeMap::new()
        };
        scanned.push(DocScan {
            doc_id: entity.id.to_string(),
            title: entity.name.clone(),
            prose,
            facts,
            fields,
            entity: Some(entity.clone()),
            owner: None,
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
    Ok(found)
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
mod tests {
    use super::*;
    use crate::memory::{Boot, FactId, FactStatus, Provenance, Standing};

    /// 🚨 **A filter is a record on the view, its own fields carrying the
    /// key, the value, the comparison and the scope** — the same shape
    /// every other claim on this store already is, so a view needs no
    /// filter syntax of its own and no cap of one filter per key: several
    /// filters are several records.
    ///
    /// **Equals needs no `compare` field; `scope` defaults to `thing`.**
    /// Both are named explicitly on the second filter here, to prove the
    /// non-default path rather than only the common one.
    #[test]
    fn a_views_own_records_become_its_filters() {
        let held: std::collections::BTreeMap<String, String> =
            [("selects".to_string(), "rhythm".to_string())]
                .into_iter()
                .collect();
        let facts = vec![
            Fact {
                fields: [
                    ("key".to_string(), "status".to_string()),
                    ("value".to_string(), "active".to_string()),
                ]
                .into_iter()
                .collect(),
                ..fact("view:contract-asked-by-view", "f1", "status is active")
            },
            Fact {
                fields: [
                    ("key".to_string(), "donuts_eaten".to_string()),
                    ("value".to_string(), "3".to_string()),
                    ("compare".to_string(), "greater".to_string()),
                    ("scope".to_string(), "record".to_string()),
                ]
                .into_iter()
                .collect(),
                ..fact(
                    "view:contract-asked-by-view",
                    "f2",
                    "more than three donuts",
                )
            },
        ];
        let asked = asked_by_view(&held, &facts);
        assert_eq!(asked.selects.as_deref(), Some("rhythm"));
        assert_eq!(
            asked.filters.len(),
            2,
            "both records must become filters: {:?}",
            asked.filters
        );
        let status = asked
            .filters
            .iter()
            .find(|f| f.key.as_deref() == Some("status"))
            .expect("the status record must be a filter");
        assert_eq!(status.value.as_deref(), Some("active"));
        assert_eq!(status.compare, types::Compare::Equals);
        assert_eq!(status.scope, Scope::Thing);
        let donuts = asked
            .filters
            .iter()
            .find(|f| f.key.as_deref() == Some("donuts_eaten"))
            .expect("the donuts_eaten record must be a filter");
        assert_eq!(donuts.value.as_deref(), Some("3"));
        assert_eq!(donuts.compare, types::Compare::Greater);
        assert_eq!(donuts.scope, Scope::Record);
    }

    /// **A retracted filter record stops filtering.** The view's records
    /// are read the same way any object's records are: an archived one is
    /// history, not a live question.
    #[test]
    fn a_retracted_filter_record_is_not_asked() {
        let held = std::collections::BTreeMap::new();
        let facts = vec![Fact {
            status: FactStatus::Archived,
            fields: [
                ("key".to_string(), "status".to_string()),
                ("value".to_string(), "active".to_string()),
            ]
            .into_iter()
            .collect(),
            ..fact("view:contract-asked-by-view", "f1", "status is active")
        }];
        let asked = asked_by_view(&held, &facts);
        assert!(
            asked.filters.is_empty(),
            "a retracted filter record must not be asked: {:?}",
            asked.filters
        );
    }

    /// 🚨 **A view can name a type to select structurally, and a relation
    /// walk with its own direction, depth and fits_type** — both unresolved,
    /// because resolving a name to a declaration needs a read this pure
    /// function cannot make.
    ///
    /// **Paired with the absence**: a view naming neither must answer
    /// `None` for the walk, not a walk that names nothing — those are
    /// different questions, the same distinction `args.follow` already
    /// draws for a caller.
    #[test]
    fn a_views_own_fields_name_a_type_and_a_walk() {
        let held: std::collections::BTreeMap<String, String> = [
            ("selects".to_string(), "person".to_string()),
            ("answers_type".to_string(), "pet-owner".to_string()),
            ("follow_relation".to_string(), "owner".to_string()),
            ("follow_direction".to_string(), "in".to_string()),
            ("follow_depth".to_string(), "2".to_string()),
            ("follow_fits_type".to_string(), "pet".to_string()),
        ]
        .into_iter()
        .collect();
        let asked = asked_by_view(&held, &[]);
        assert_eq!(asked.answers_type.as_deref(), Some("pet-owner"));
        let follow = asked.follow.expect("a walk was named");
        assert_eq!(follow.relation.as_deref(), Some("owner"));
        assert_eq!(follow.shape, None);
        assert_eq!(follow.direction.as_deref(), Some("in"));
        assert_eq!(follow.depth, Some(2));
        assert_eq!(follow.fits_type.as_deref(), Some("pet"));

        let bare: std::collections::BTreeMap<String, String> =
            [("selects".to_string(), "person".to_string())]
                .into_iter()
                .collect();
        let unasked = asked_by_view(&bare, &[]);
        assert_eq!(unasked.answers_type, None);
        assert_eq!(
            unasked.follow, None,
            "a view naming no walk must answer no walk, not one that names nothing",
        );
    }

    fn entity(handle: &str, name: &str) -> Entity {
        // **The fixture stands a store up, because the set is setup here.**
        // Reading a handle asks the kinds this process loaded, and no case
        // behind this fixture asserts anything about the set — so the set
        // arrives the way a boot delivers it, from what a store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
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
            merged_into: None,
            badge: None,
            archived: None,
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
            recorded_at: "2026-08-10".parse().expect("a civil date"),
            happened_at: None,
            happened_through: None,
            edge: None,
            fields: Default::default(),
            refs: Vec::new(),
            derived_from: None,
            stands_for: Vec::new(),
            inserted_at: None,
            stale_after: None,
        }
    }

    /// A doc holding one entity, its prose and its rows.
    ///
    /// **The store is what folds a thing's fields, so the fixture stands in for
    /// it here.** Every doc below writes each of its keys once, which is the
    /// case where the order of the records and the order of the writes agree; a
    /// fixture that needs them to disagree says what the thing holds itself,
    /// with [`doc_holding`].
    fn doc(entity: Entity, prose: &str, facts: Vec<Fact>) -> DocScan {
        let fields = facts
            .iter()
            .filter(|f| f.status == FactStatus::Active)
            .flat_map(|f| f.fields.iter())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        doc_holding(entity, prose, facts, fields)
    }

    /// A doc whose thing holds what the store says it holds, records or no
    /// records — the shape a key written more than once really arrives in.
    fn doc_holding(
        entity: Entity,
        prose: &str,
        facts: Vec<Fact>,
        fields: BTreeMap<String, String>,
    ) -> DocScan {
        DocScan {
            doc_id: entity.id.to_string(),
            title: entity.name.clone(),
            prose: prose.to_string(),
            entity: Some(entity),
            facts,
            fields,
            owner: None,
        }
    }

    /// An owned document: readable by one identity and nobody else.
    fn owned_by(entity: Entity, owner: &str, prose: &str) -> DocScan {
        DocScan {
            owner: Some(EntityId(owner.to_string())),
            ..doc(entity, prose, Vec::new())
        }
    }

    /// **An owned object comes back to its owner and to nobody else.**
    ///
    /// Both halves in one read: the caller finds its own, and does not find
    /// another's. A third identity is asked the same question, so this is a
    /// filter rather than a list of one bot's things.
    ///
    /// ⚠️ **The withheld COUNT is not here yet, and until it is these two
    /// answers are the same empty list to a caller** — one that may not read a
    /// thing, and one for which there is nothing. Nothing declares an owner
    /// yet, so no caller meets that today.
    ///
    /// The unowned object is here because it is the whole rest of the store:
    /// nothing that exists today declares an owner, and a filter that hid
    /// unowned things would empty every instance.
    #[test]
    fn an_owned_object_answers_its_owner_and_nobody_else() {
        let scanned = vec![
            owned_by(entity("bot:gamma", "Gamma"), "bot:gamma", ""),
            owned_by(entity("bot:delta", "Delta"), "bot:delta", ""),
            doc(entity("bot:otto", "Otto"), "", Vec::new()),
        ];
        let asking = |who: Option<&str>| GraphQuery {
            select: Selection {
                kind: Some(EntityKind::BOT),
                asked_by: who.map(|w| EntityId(w.to_string())),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        };

        let mine = resolved(&scanned, &[], &asking(Some("bot:gamma"))).expect("a kind selects");
        assert_eq!(
            handles(&mine),
            vec!["bot:gamma", "bot:otto"],
            "the caller's own owned object, and everything owned by nobody",
        );

        let theirs = resolved(&scanned, &[], &asking(Some("bot:delta"))).expect("a kind selects");
        assert_eq!(
            handles(&theirs),
            vec!["bot:delta", "bot:otto"],
            "…and another identity sees its own instead, so this is a filter rather than a \
             list of one bot's things",
        );

        let anonymous = resolved(&scanned, &[], &asking(None)).expect("a kind selects");
        assert_eq!(
            handles(&anonymous),
            vec!["bot:otto"],
            "a caller with no identity reaches everything unowned and nothing owned",
        );
    }

    /// **What was withheld is counted, so a refusal cannot read as an empty
    /// store.**
    ///
    /// 🚨 **This is the whole reason the axis exists.** A bot that may not read
    /// something and a bot for which there is nothing both get an empty list,
    /// and the first reads as the second — so a session searching its own past
    /// concludes there is nothing rather than that it was not allowed.
    ///
    /// **A TOTAL and never a breakdown.** How many is work status; which
    /// colleague holds them is a directory of who is busy, and the caller named
    /// no handle to earn that.
    ///
    /// Both halves: the caller's own are returned AND the rest are counted.
    /// Without the count this passes on a build that filters silently.
    #[test]
    fn what_a_selection_withheld_is_counted_rather_than_dropped_in_silence() {
        let scanned = vec![
            owned_by(entity("bot:gamma", "Gamma"), "bot:gamma", ""),
            owned_by(entity("bot:delta", "Delta"), "bot:delta", ""),
            owned_by(entity("bot:epsilon", "Epsilon"), "bot:epsilon", ""),
            doc(entity("bot:otto", "Otto"), "", Vec::new()),
        ];
        let query = GraphQuery {
            select: Selection {
                kind: Some(EntityKind::BOT),
                asked_by: Some(EntityId("bot:gamma".into())),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: false,
                stood_for: false,
            },
            follow: None,
            history: None,
        };

        let answer = resolve(&scanned, &[], &query).expect("a kind selects");

        assert_eq!(
            handles(&answer.objects),
            vec!["bot:gamma", "bot:otto"],
            "the caller's own, and everything owned by nobody",
        );
        assert_eq!(
            answer.withheld, 2,
            "…and the two belonging to other identities are counted rather than vanishing",
        );
    }

    /// **Naming another identity's object is refused, and the refusal is not
    /// the one a typo gets.**
    ///
    /// ⚠️ **Selecting by kind filters; naming a handle does not.** A named
    /// subject returns that object before any filter runs, which is right for
    /// every reason it was written — and wrong the moment an object has an
    /// owner, because the filter is what keeps it private.
    ///
    /// ⛔️ **It must not answer "no such thing" either.** The caller is holding
    /// the handle, so denying the object exists hides nothing and only makes
    /// the answer untrustworthy — the defect closed on the write path at
    /// `c733f74`, arriving on the read path.
    ///
    /// Both halves: the owner still reads it by name.
    #[test]
    fn naming_another_identitys_object_is_refused_and_its_owner_still_reads_it() {
        let scanned = vec![owned_by(
            entity("bot:gamma", "Gamma"),
            "bot:gamma",
            "what gamma is for",
        )];
        let naming = |who: &str| GraphQuery {
            select: Selection {
                subject: Some(EntityId("bot:gamma".into())),
                asked_by: Some(EntityId(who.to_string())),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: true,
                stood_for: false,
            },
            follow: None,
            history: None,
        };

        let refused = resolve(&scanned, &[], &naming("bot:delta"));
        match refused {
            Err(MemoryError::NotYours { attempted, .. }) => {
                assert_eq!(attempted, "bot:gamma");
            }
            other => panic!(
                "another identity's object is refused as somebody else's, never as absent \
                 and never returned: {other:?}"
            ),
        }

        let mine = resolved(&scanned, &[], &naming("bot:gamma")).expect("its owner reads it");
        assert_eq!(handles(&mine), vec!["bot:gamma"]);
        assert_eq!(
            mine[0].prose.as_deref(),
            Some("what gamma is for"),
            "…and reads it whole, so this is a refusal of others rather than of everyone",
        );
    }

    /// The objects a selection answered with — what almost every case here
    /// asks for. The cases about what was WITHHELD call `resolve` directly,
    /// because the count is the thing they assert.
    fn resolved(
        scanned: &[DocScan],
        declarations: &[types::DeclaredType],
        query: &GraphQuery,
    ) -> Result<Vec<Object>, MemoryError> {
        resolve(scanned, declarations, query).map(|answer| answer.objects)
    }

    /// **What else was recorded around this day.**
    ///
    /// Every claim carries the day it is true of, and nothing could be asked
    /// about it associatively. Real questions arrive shaped as *when plus
    /// who* — *what did he say back in August* — and the store held the answer
    /// with no way to be asked for it.
    ///
    /// **Both halves in one case.** A window that kept everything would pass a
    /// check that only looked for the near record, and a dead build that kept
    /// nothing would pass one that only looked for the far one missing.
    #[test]
    fn a_selection_near_a_day_keeps_the_records_in_its_window_and_drops_the_rest() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let dated = |id: &str, day: &str, content: &str| Fact {
            recorded_at: day.parse().expect("a civil date"),
            ..fact("person:milhouse", id, content)
        };
        let scanned = vec![doc(
            entity("person:milhouse", "Milhouse"),
            "His page.",
            vec![
                dated("f1", "2026-08-16", "said the thing about the committee"),
                dated("f2", "2026-02-01", "said something in February"),
            ],
        )];
        let query = GraphQuery {
            select: Selection {
                near: Some(Nearness {
                    day: "2026-08-18".parse().expect("a civil date"),
                    within_days: 7,
                    clock: Clock::RecordedOn,
                }),
                ..Selection::default()
            },
            include: Include {
                facts: true,
                ..GraphQuery::default().include
            },
            ..GraphQuery::default()
        };
        let objects = resolved(&scanned, &[], &query).expect("a read, not an error");
        let content: Vec<&str> = objects
            .iter()
            .flat_map(|o| o.facts.iter())
            .map(|f| f.content.as_str())
            .collect();
        assert!(
            content.iter().any(|c| c.contains("committee")),
            "a record two days from the day asked about is not in the answer: {content:?}",
        );
        assert!(
            !content.iter().any(|c| c.contains("February")),
            "a record six months away came back, so the window keeps everything: {content:?}",
        );

        // 🚨 **A thing planned for next year does not sit in next year.** The
        // booking below is recorded today and carries 2027 as the day it
        // happens. Before the split there was one date field, so recording it
        // put 2027 on the claim — and the claim then answered a window around
        // 2027 and NOT one around the day it was written, which is where a
        // caller asking what was recorded this week looks for it.
        //
        // **Both halves**: it comes back for the day it was recorded on, and
        // it does not come back for the day it happens.
        let booking = Fact {
            happened_at: Some("2027-06-01".parse().expect("a civil date")),
            ..dated("f3", "2026-08-16", "booked the trip for next June")
        };
        let with_booking = vec![doc(
            entity("person:milhouse", "Milhouse"),
            "His page.",
            vec![booking],
        )];
        let recorded_week = resolve(&with_booking, &[], &query).expect("a read");
        assert_eq!(
            recorded_week
                .objects
                .iter()
                .flat_map(|o| o.facts.iter())
                .count(),
            1,
            "a claim recorded two days from the day asked about did not come back",
        );
        let next_year = resolve(
            &with_booking,
            &[],
            &GraphQuery {
                select: Selection {
                    near: Some(Nearness {
                        day: "2027-06-01".parse().expect("a civil date"),
                        within_days: 7,
                        clock: Clock::RecordedOn,
                    }),
                    ..Selection::default()
                },
                ..query.clone()
            },
        )
        .expect("a read");
        assert_eq!(
            next_year
                .objects
                .iter()
                .flat_map(|o| o.facts.iter())
                .count(),
            0,
            "the day the thing HAPPENS placed the claim, so a booking sits in next year",
        );

        // 🚨 **`Clock::HappenedAt` is the clock that DOES place a claim by the
        // day it happens.** Both halves, on the same booking: it comes back
        // for a window around the day it happens, and it does NOT come back
        // for a window around the day it was recorded — the mirror image of
        // `RecordedOn` above, proving this clock reads its own field rather
        // than falling back to another one.
        let asking_happened_at = |day: &str| GraphQuery {
            select: Selection {
                near: Some(Nearness {
                    day: day.parse().expect("a civil date"),
                    within_days: 7,
                    clock: Clock::HappenedAt,
                }),
                ..Selection::default()
            },
            include: Include {
                facts: true,
                ..GraphQuery::default().include
            },
            ..GraphQuery::default()
        };
        let near_the_event =
            resolve(&with_booking, &[], &asking_happened_at("2027-06-04")).expect("a read");
        assert_eq!(
            near_the_event
                .objects
                .iter()
                .flat_map(|o| o.facts.iter())
                .count(),
            1,
            "a window around the day the booking HAPPENS did not find it",
        );
        let near_the_recording =
            resolve(&with_booking, &[], &asking_happened_at("2026-08-18")).expect("a read");
        assert_eq!(
            near_the_recording
                .objects
                .iter()
                .flat_map(|o| o.facts.iter())
                .count(),
            0,
            "a window around the day the booking was RECORDED found it on the happened-at \
             clock, so this clock is reading the wrong field",
        );
    }

    /// 🚨 **A day with nothing around it, and a clock that could not look, must
    /// not read alike.**
    ///
    /// The taken-in stamp is absent on every record written before it existed,
    /// and it stays absent deliberately. So a read on that clock can come back
    /// empty for two entirely different reasons: nothing was recorded near that
    /// day, or nothing could be placed at all. **The count is what tells them
    /// apart**, and without it the second silently reads as the first.
    ///
    /// **Three states in one case**, because any two of them alone pass against
    /// a build that has the third wrong.
    #[test]
    fn a_clock_that_cannot_place_a_record_counts_it_rather_than_dropping_it() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let scanned = vec![doc(
            entity("person:milhouse", "Milhouse"),
            "His page.",
            vec![
                fact("person:milhouse", "f1", "written before the stamp existed"),
                fact("person:milhouse", "f2", "also written before it"),
            ],
        )];
        let asking = |clock| GraphQuery {
            select: Selection {
                near: Some(Nearness {
                    day: "2026-08-10".parse().expect("a civil date"),
                    within_days: 7,
                    clock,
                }),
                ..Selection::default()
            },
            include: Include {
                facts: true,
                ..GraphQuery::default().include
            },
            ..GraphQuery::default()
        };

        // The claim's own date places every record, so nothing is unreadable
        // and the records really are near the day.

        let by_day = resolve(&scanned, &[], &asking(Clock::RecordedOn)).expect("a read");
        assert_eq!(
            by_day.unplaced, 0,
            "the claim's own date is never absent, so nothing can be unplaceable on it",
        );
        assert_eq!(
            by_day.objects.iter().flat_map(|o| o.facts.iter()).count(),
            2,
            "the records are two days from the day asked about and did not come back",
        );

        // The same store on the other clock reaches nothing — and says so.
        let by_stamp = resolve(&scanned, &[], &asking(Clock::TakenIn)).expect("a read");
        assert_eq!(
            by_stamp.objects.iter().flat_map(|o| o.facts.iter()).count(),
            0,
            "a record with no stamp was placed on a clock that cannot place it",
        );
        assert_eq!(
            by_stamp.unplaced, 2,
            "the read came back empty and said nothing about what it could not look at, which \
             reads exactly like a day with nothing around it",
        );
    }

    /// 🚨 **The count answers the question that was asked, not the store.**
    ///
    /// A caller who named a subject or a kind asked about those records. A
    /// count taken over every document the walk happened to scan tells them
    /// their own records could not be placed when every one of them was — and
    /// because a walk with a near-day read scans every entity, one unrelated
    /// stampless record anywhere makes zero unreachable forever.
    ///
    /// **Both halves in one read, because the negative alone is satisfied by a
    /// count that is always zero**: a selection whose own records are all
    /// placeable reports none, and a selection whose own records cannot be
    /// placed still reports them.
    #[test]
    fn the_unplaceable_count_is_narrowed_the_way_the_selection_is() {
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let stamped = |home: &str, id: &str, content: &str| Fact {
            inserted_at: Some("2026-08-09T12:00:00Z".parse().expect("a timestamp")),
            ..fact(home, id, content)
        };
        let scanned = vec![
            // Every record placeable on the taken-in clock, and near the day.
            doc(
                entity("person:milhouse", "Milhouse"),
                "His page.",
                vec![
                    stamped("person:milhouse", "f1", "taken in the day before"),
                    stamped("person:milhouse", "f2", "and so was this one"),
                ],
            ),
            // Unrelated, and written before the stamp existed — the backfilled
            // corpus a real instance is mostly made of.
            doc(
                entity("place:moes", "Moe's Tavern"),
                "The tavern's page.",
                vec![
                    fact("place:moes", "f1", "written before the stamp existed"),
                    fact("place:moes", "f2", "also written before it"),
                ],
            ),
        ];
        let asking = |select: Selection| GraphQuery {
            select: Selection {
                near: Some(Nearness {
                    day: "2026-08-10".parse().expect("a civil date"),
                    within_days: 7,
                    clock: Clock::TakenIn,
                }),
                ..select
            },
            ..GraphQuery::default()
        };
        let read = |select: Selection| resolve(&scanned, &[], &asking(select)).expect("a read");
        let facts = |found: &Selected| found.objects.iter().flat_map(|o| o.facts.iter()).count();

        let his = read(Selection {
            subject: Some(EntityId("person:milhouse".to_string())),
            ..Selection::default()
        });
        assert_eq!(facts(&his), 2, "his records are placeable and near the day");
        assert_eq!(
            his.unplaced, 0,
            "every record he has was placed, so the read that named him has nothing to report \
             about records it could not look at",
        );

        let theirs = read(Selection {
            subject: Some(EntityId("place:moes".to_string())),
            ..Selection::default()
        });
        assert_eq!(facts(&theirs), 0, "no record of the tavern's can be placed");
        assert_eq!(
            theirs.unplaced, 2,
            "the tavern's own records are the ones this clock cannot look at, and a narrowed \
             count must still report them",
        );

        let people = read(Selection {
            kind: Some(EntityKind::PERSON),
            ..Selection::default()
        });
        assert_eq!(
            facts(&people),
            2,
            "the kind reaches the same placed records"
        );
        assert_eq!(
            people.unplaced, 0,
            "a kind narrows the count the way a handle does",
        );

        let places = read(Selection {
            kind: Some(EntityKind::PLACE),
            ..Selection::default()
        });
        assert_eq!(
            places.unplaced, 2,
            "the tavern drops out of the answer entirely because none of its records could be \
             placed, which is exactly why the count cannot be taken over the answer",
        );
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
        // **The fixture stands a store up, because the set is setup here.**
        // Reading a handle asks the kinds this process loaded, and no case
        // behind this fixture asserts anything about the set — so the set
        // arrives the way a boot delivers it, from what a store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let attending = |home: &str, id: &str, content: &str, rsvp: &str| Fact {
            fields: [("rsvp".to_string(), rsvp.to_string())]
                .into_iter()
                .collect(),
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
                kind: Some(EntityKind::PERSON),
                ..Selection::default()
            },
            include: Include {
                facts: false,
                prose: true,
                stood_for: false,
            },
            follow: None,
            history: None,
        };
        let found = resolved(&scanned, &[], &query).expect("a kind is a selection");
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

        let unasked = resolved(
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

    /// **A shape's sources are left off the listing that also carries the
    /// shape.** The shape already speaks for them in its own words, so
    /// serving both would say the same thing twice. Three cases in one,
    /// because each alone passes on a build that answers nothing: the count
    /// says how many were left out, clearing the mark serves everything
    /// again (the sabotage this case is built to catch), and asking for the
    /// sources back with `stood_for: true` serves them beside the shape.
    #[test]
    fn a_shapes_sources_are_folded_out_of_the_facts_listing() {
        let leaf = fact("person:contract-graph-fold", "f1", "the first claim");
        let shape = Fact {
            stands_for: vec![FactAddress::new(
                EntityId("person:contract-graph-fold".into()),
                FactId("f1".into()),
            )],
            ..fact(
                "person:contract-graph-fold",
                "f2",
                "the newest claim, standing for the first",
            )
        };
        let query = GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:contract-graph-fold".into())),
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
        fn ids(object: &Object) -> Vec<&str> {
            object.facts.iter().map(|f| f.id.as_str()).collect()
        }

        let marked = vec![doc(
            entity("person:contract-graph-fold", "Fold Case"),
            "The page.",
            vec![leaf.clone(), shape.clone()],
        )];
        let found = resolved(&marked, &[], &query).expect("a named subject always comes back");
        assert_eq!(
            ids(&found[0]),
            vec!["f2"],
            "the shape is served and its source is not: {:?}",
            found[0].facts,
        );
        assert_eq!(
            found[0].facts_folded, 1,
            "the count says how many were left out: {:?}",
            found[0],
        );
        assert_eq!(
            found[0].facts_held, 2,
            "the held count is the true total, unaffected by elision: {:?}",
            found[0],
        );

        // Sabotage: clear the mark and the same read must return everything —
        // a case that stayed green with the mark gone was never about the mark.
        let unmarked = vec![doc(
            entity("person:contract-graph-fold", "Fold Case"),
            "The page.",
            vec![
                leaf.clone(),
                Fact {
                    stands_for: Vec::new(),
                    ..shape.clone()
                },
            ],
        )];
        let found = resolved(&unmarked, &[], &query).expect("a named subject always comes back");
        assert_eq!(
            ids(&found[0]),
            vec!["f1", "f2"],
            "with the mark gone, the same read returns everything: {:?}",
            found[0].facts,
        );
        assert_eq!(found[0].facts_folded, 0);

        // The other argument: asking for the sources back serves both.
        let with_sources = GraphQuery {
            include: Include {
                stood_for: true,
                ..query.include
            },
            ..query.clone()
        };
        let found =
            resolved(&marked, &[], &with_sources).expect("a named subject always comes back");
        assert_eq!(
            ids(&found[0]),
            vec!["f1", "f2"],
            "stood_for: true serves the sources alongside the shape: {:?}",
            found[0].facts,
        );
        assert_eq!(
            found[0].facts_folded, 0,
            "nothing was left out when the sources were asked for: {:?}",
            found[0],
        );
    }

    /// **A record-scoped filter narrows which records come back — it must
    /// not narrow how many the answer claims are behind them.**
    ///
    /// `facts_held` is documented and already tested as "the true total,
    /// unaffected by elision" (see the shape-fold case above). A record
    /// filter is exactly as much an elision as shape-folding is: it decides
    /// which of the object's own records are served, never how many exist.
    /// An answer that let `facts_held` shrink to match a record filter would
    /// be a smaller number that reads as "this is everything", which is
    /// indistinguishable from an object that only ever held the one record.
    #[test]
    fn a_record_scoped_filter_narrows_what_comes_back_and_not_the_held_count() {
        let matching = Fact {
            fields: BTreeMap::from([("rsvp".to_string(), "yes".to_string())]),
            ..fact("person:milhouse", "f1", "said yes")
        };
        let other = Fact {
            fields: BTreeMap::from([("rsvp".to_string(), "no".to_string())]),
            ..fact("person:milhouse", "f2", "said no, a different sitting")
        };
        let scanned = vec![doc(
            entity("person:milhouse", "Milhouse"),
            "His page.",
            vec![matching, other],
        )];

        // The unscoped question: every record on the thing.
        let unscoped = GraphQuery {
            select: Selection {
                subject: Some(EntityId("person:milhouse".into())),
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
        let found = resolved(&scanned, &[], &unscoped).expect("a named subject always comes back");
        assert_eq!(
            found[0].facts.len(),
            2,
            "the unscoped question returns the full set: {:?}",
            found[0].facts,
        );
        assert_eq!(found[0].facts_held, 2);

        // The scoped question: only the record that answers the filter.
        let scoped = GraphQuery {
            select: Selection {
                fields: vec![FieldFilter::holding("rsvp", "yes").on_a_record()],
                ..unscoped.select.clone()
            },
            ..unscoped.clone()
        };
        let found = resolved(&scanned, &[], &scoped).expect("a named subject always comes back");
        assert_eq!(
            found[0]
                .facts
                .iter()
                .map(|f| f.id.as_str())
                .collect::<Vec<_>>(),
            vec!["f1"],
            "the scoped question narrows what comes back: {:?}",
            found[0].facts,
        );
        assert_eq!(
            found[0].facts_held, 2,
            "the held count must still say the true total — the caller's own filter is not the \
             store's whole picture of the thing: {:?}",
            found[0],
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
            resolved(&scanned, &[], &query).expect("a key filter is a selection")
        };
        assert_eq!(handles(&by_value("yes")), vec!["person:patana"]);
        assert_eq!(handles(&by_value("no")), vec!["person:barney-gumble"]);

        let any = resolved(
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
        // **A filter says which OBJECTS, and by default it says nothing about
        // which of an object's records come back.** Patana holds two, one of
        // which carries no key at all, and both are hers.
        assert_eq!(
            any[1].facts.len(),
            2,
            "a selection filter does not narrow the object's page: {:?}",
            any[1],
        );

        // **Asked of a record, the same filter also narrows the page** — the
        // other question, and the pair is what says the two are apart. Without
        // the assertion above, this one passes on a build where every filter is
        // still the record's.
        let on_a_record = resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    fields: vec![FieldFilter::key("rsvp").on_a_record()],
                    ..Selection::default()
                },
                ..GraphQuery::default()
            },
        )
        .expect("a key filter asked of a record is a selection too");
        assert_eq!(
            handles(&on_a_record),
            vec!["person:barney-gumble", "person:patana"],
            "the same objects: each holds a record carrying the key",
        );
        assert_eq!(
            on_a_record[1].facts.len(),
            1,
            "and it comes back with the record that answered, not its whole page: {:?}",
            on_a_record[1],
        );
    }

    /// 🚨 **The two scopes disagree about WHICH OBJECTS when the fold and a
    /// record disagree about the value.**
    ///
    /// The cases above ask both scopes of a store where every key was written
    /// once, so both select the same object and the only difference on show is
    /// which records ride along. **That leaves the load-bearing half untested:
    /// a key written twice, where the thing no longer holds what a record of it
    /// still says.**
    ///
    /// Asked of the THING, the old value selects nothing — the newest write won
    /// the fold. Asked of a RECORD, it selects, because a record did carry it.
    /// **A caller that reads one as the other writes a filter that goes empty
    /// the moment anybody rewrites the key.**
    ///
    /// The positive is the third read: the thing IS selected by what it holds
    /// now, so the negative above is a fact about the value rather than about a
    /// store nothing can be found in.
    #[test]
    fn the_scopes_part_company_when_the_fold_and_a_record_disagree() {
        let wrote = |id: &str, content: &str, rsvp: &str| Fact {
            fields: [("rsvp".to_string(), rsvp.to_string())]
                .into_iter()
                .collect(),
            ..fact("person:patana", id, content)
        };
        // Two writes of one key, and the thing holds the newer of them. The
        // fields are stated rather than folded from the rows, because that is
        // what the store does and a fixture that recomputed it could not put
        // the two out of step.
        let scanned = vec![doc_holding(
            entity("person:patana", "Patana"),
            "Patana's page.",
            vec![
                wrote("f1", "coming to the party", "yes"),
                wrote("f2", "cannot make it after all", "no"),
            ],
            [("rsvp".to_string(), "no".to_string())]
                .into_iter()
                .collect(),
        )];
        let by = |filter: FieldFilter| {
            resolved(
                &scanned,
                &[],
                &GraphQuery {
                    select: Selection {
                        fields: vec![filter],
                        ..Selection::default()
                    },
                    ..GraphQuery::default()
                },
            )
            .expect("a key filter is a selection")
        };

        assert!(
            handles(&by(FieldFilter::holding("rsvp", "yes"))).is_empty(),
            "the thing does not hold the value it used to, so asked of the thing it is not \
             selected",
        );
        assert_eq!(
            handles(&by(FieldFilter::holding("rsvp", "yes").on_a_record())),
            vec!["person:patana"],
            "asked of a record, the write that happened still answers",
        );
        assert_eq!(
            handles(&by(FieldFilter::holding("rsvp", "no"))),
            vec!["person:patana"],
            "and the thing is selected by what it holds now, so the empty answer above is about \
             the value rather than about an unreachable store",
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
            handles(&resolved(&scanned, &[], &query(EntityKind::PERSON)).expect("both filters")),
            vec!["person:barney-gumble", "person:patana"],
        );
        assert!(
            resolved(&scanned, &[], &query(EntityKind::PLACE))
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
            vec![types::Field::required("rsvp", types::ValueType::Text)],
        );
        let found = resolved(
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
            vec![types::Field::required("weight", types::ValueType::Number)],
        );
        assert!(
            resolved(
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

    /// **A THING answers a type, across everything recorded about it.**
    ///
    /// The question is not whether one record carries the type's keys. It is
    /// whether the thing does, and a thing's fields are its records' fields
    /// folded into one. Asking it of a record makes a thing that was described
    /// over two sittings answer nothing, which is how most things get written
    /// down.
    #[test]
    fn a_thing_answers_a_type_its_records_answer_only_together() {
        let declared = types::DeclaredType::new(
            "crate",
            vec![
                types::Field::required("weight", types::ValueType::Number),
                types::Field::required("arrives", types::ValueType::Date),
            ],
        );
        // One key each, and neither record answers the type on its own.
        let halves = |home: &str| {
            vec![
                Fact {
                    fields: [("weight".to_string(), "12".to_string())]
                        .into_iter()
                        .collect(),
                    ..fact(home, "f1", "somebody weighed it")
                },
                Fact {
                    fields: [("arrives".to_string(), "2026-08-10".to_string())]
                        .into_iter()
                        .collect(),
                    ..fact(home, "f2", "and somebody else was told when")
                },
            ]
        };
        let scanned = vec![
            doc(
                entity("thing:folding-chairs", "The Folding Chairs"),
                "",
                halves("thing:folding-chairs"),
            ),
            // The negative: a thing carrying one of the keys and no more. It is
            // what stops "the fold reached it" from meaning "everything comes
            // back".
            doc(
                entity("thing:torque-wrench", "The Torque Wrench"),
                "",
                vec![Fact {
                    fields: [("weight".to_string(), "3".to_string())]
                        .into_iter()
                        .collect(),
                    ..fact("thing:torque-wrench", "f1", "a lighter thing")
                }],
            ),
        ];

        let found = resolved(
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
            vec!["thing:folding-chairs", "thing:torque-wrench"],
            "both answer; the fold is what lets the first one answer at all"
        );
        // …and the object says how it answers, which is the thing the caller
        // asked for. Reported at the object, because that is the unit the
        // question was asked of.
        let whole = found
            .iter()
            .find(|o| o.entity.id.as_str() == "thing:folding-chairs")
            .expect("the whole one is in the answer");
        let answered = whole.answers.as_ref().expect("it answered a type");
        assert!(
            answered.complete(),
            "two records between them hold every key: {answered:?}"
        );
        let partial = found
            .iter()
            .find(|o| o.entity.id.as_str() == "thing:torque-wrench")
            .expect("the partial one is in the answer");
        assert_eq!(
            partial
                .answers
                .as_ref()
                .expect("it answered a type")
                .lacking,
            vec!["arrives".to_string()],
            "a partial answer names what it lacks rather than being dropped"
        );
    }

    /// **The type question is asked of what the THING holds, never of the
    /// records under it.**
    ///
    /// A key written twice is not a conflict to report — it is the key being
    /// written twice, and which write won is the store's answer, not a walk's
    /// to work out. So the fixture hands over a thing whose standing value is
    /// the newer one while both records are still there, and the walk must read
    /// the standing value: a walk that folded the records for itself would find
    /// the other one.
    ///
    /// Read through the declared value type, because that is where a folded
    /// VALUE surfaces: the value the thing holds is not a number and is
    /// reported as mistyped, where the one it replaced would have passed
    /// silently.
    #[test]
    fn the_type_question_reads_what_the_thing_holds() {
        let declared = types::DeclaredType::new(
            "crate",
            vec![types::Field::required("weight", types::ValueType::Number)],
        );
        let weighed = |id: &str, weight: &str| Fact {
            fields: [("weight".to_string(), weight.to_string())]
                .into_iter()
                .collect(),
            ..fact("thing:bike-chain", id, "somebody weighed it")
        };
        let scanned = vec![doc_holding(
            entity("thing:bike-chain", "The Chain"),
            "",
            // Out of order on purpose: the records say nothing about which
            // write won, and the walk must not read them as if they did.
            vec![
                weighed("f2", "heavier than the last one"),
                weighed("f1", "12"),
            ],
            [(
                "weight".to_string(),
                "heavier than the last one".to_string(),
            )]
            .into_iter()
            .collect(),
        )];

        let found = resolved(
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
        let answered = found
            .first()
            .expect("the thing answers the type either way")
            .answers
            .as_ref()
            .expect("it answered a type");
        assert_eq!(
            answered
                .mistyped
                .iter()
                .map(|m| m.value.as_str())
                .collect::<Vec<_>>(),
            vec!["heavier than the last one"],
            "the type was asked of the value the thing holds: {answered:?}"
        );
    }

    /// **The walk goes both ways, and the answer nests.**
    ///
    /// **A walk says when the claim behind a link was taken back.**
    ///
    /// A fact read can already tell a retracted claim from one that never
    /// existed: the claim comes back carrying its status, marked rather than
    /// hidden. A walk could not, so the retracted attendance and the live one
    /// arrived by the same edge, indistinguishable.
    ///
    /// **Marked, never filtered.** Dropping the link would make a claim
    /// somebody took back and a claim nobody ever made identical on a walk,
    /// which is the failure this exists to remove rather than a tidier version
    /// of it.
    ///
    /// **All three reads in one case.** The live link unmarked is what stops
    /// the marker being on everything; the retracted one marked is the
    /// capability; and the person nobody ever linked is absent from both, which
    /// is what stops the pair passing on a store where the walk reaches
    /// everybody.
    #[test]
    fn a_walk_marks_a_link_whose_claim_was_taken_back() {
        let mut scanned = store();
        let barney = scanned
            .iter_mut()
            .find(|d| d.doc_id == "person:barney-gumble")
            .expect("the store holds Barney");
        barney.facts[0].status = FactStatus::Archived;

        let guests = resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("event:birthday-party".into())),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    along: Along::Edge(EdgeShape::Attendance),
                    direction: Some(Direction::In),
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a subject with a walk");
        let reached = &guests[0].connected;

        assert_eq!(
            handles(reached),
            vec!["person:barney-gumble", "person:patana"],
            "both are still reached, because a retracted claim is marked and not hidden: \
             {reached:?}",
        );
        let via = |at: usize| {
            reached[at]
                .via
                .as_ref()
                .expect("a reached object says how the walk got to it")
        };
        assert!(
            via(0).retracted,
            "Barney's attendance was taken back, so the link says so: {reached:?}",
        );
        assert!(
            !via(1).retracted,
            "Patana's still stands, so nothing marks hers — without this the marker could be on \
             every link: {reached:?}",
        );
        assert_eq!(
            via(0).link,
            via(1).link,
            "and both arrived by the same edge, so the marker is the only difference: {reached:?}",
        );
        assert!(
            !handles(reached).contains(&"person:ned-flanders"),
            "somebody no claim ever linked is not reached at all, so the pair above is about the \
             claims rather than about a walk that returns everybody: {reached:?}",
        );

        // **The other end of the same edge**, because a walk outbound reads the
        // object's own records while inbound reads a map of who points here.
        // They are different code, so a marker on one says nothing about the
        // other — and the caller asking *what was this person at* is walking
        // outbound.
        let out_from = |who: &str| {
            resolved(
                &scanned,
                &[],
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId(who.into())),
                        ..Selection::default()
                    },
                    follow: Some(Follow {
                        along: Along::Edge(EdgeShape::Attendance),
                        direction: Some(Direction::Out),
                        ..Follow::hop()
                    }),
                    ..GraphQuery::default()
                },
            )
            .expect("a subject with a walk")[0]
                .connected
                .clone()
        };
        let withdrawn = out_from("person:barney-gumble");
        let stands = out_from("person:patana");
        assert_eq!(
            handles(&withdrawn),
            vec!["event:birthday-party"],
            "the party is still reached from the guest whose claim went: {withdrawn:?}",
        );
        assert!(
            withdrawn[0].via.as_ref().is_some_and(|v| v.retracted),
            "and walking out says the claim behind it was taken back: {withdrawn:?}",
        );
        assert!(
            stands[0].via.as_ref().is_some_and(|v| !v.retracted),
            "while the guest whose claim stands reaches it unmarked: {stands:?}",
        );
    }

    /// **Two claims can draw one link, and a claim that stands keeps it.**
    ///
    /// The marker says *nothing stands behind this*, so it must not fire
    /// because one of several records happened to be taken back. Barney is
    /// recorded twice, once withdrawn and once not, and arrives ONCE by an
    /// unmarked link — the same answer as if the withdrawn record had never
    /// been written, which is correct: it is not what the link rests on.
    #[test]
    fn a_claim_that_stands_keeps_a_link_another_claim_gave_up() {
        let mut scanned = store();
        let barney = scanned
            .iter_mut()
            .find(|d| d.doc_id == "person:barney-gumble")
            .expect("the store holds Barney");
        barney.facts[0].status = FactStatus::Archived;
        barney.facts.push(Fact {
            edge: Some(Edge {
                shape: EdgeShape::Attendance,
                object: EntityId("event:birthday-party".into()),
            }),
            ..fact("person:barney-gumble", "f2", "came after all")
        });

        let reached = resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("event:birthday-party".into())),
                    ..Selection::default()
                },
                follow: Some(Follow {
                    along: Along::Edge(EdgeShape::Attendance),
                    direction: Some(Direction::In),
                    ..Follow::hop()
                }),
                ..GraphQuery::default()
            },
        )
        .expect("a subject with a walk")[0]
            .connected
            .clone();

        assert_eq!(
            handles(&reached),
            vec!["person:barney-gumble", "person:patana"],
            "one link each, not one per claim: {reached:?}",
        );
        assert!(
            !reached[0]
                .via
                .as_ref()
                .expect("a reached object says how the walk got to it")
                .retracted,
            "a claim that stands draws the link, so the withdrawn one does not mark it: \
             {reached:?}",
        );
    }

    /// From the party inbound reaches its guests; from a guest outbound reaches
    /// the party. One edge, two questions, and a walk that could only go one
    /// way would answer one of them.
    #[test]
    fn a_walk_leaves_by_either_end_of_an_edge() {
        let scanned = store();
        let from_party = |direction: Direction| {
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Edge(EdgeShape::Attendance),
                        direction: Some(direction),
                        depth: 1,
                        keeping: Vec::new(),
                        fits_type: None,
                    }),
                    history: None,
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
                retracted: false,
            }),
            "a reached object says how the walk got to it",
        );
        assert_eq!(inbound[0].via, None, "a root was reached by nothing");

        assert!(
            from_party(Direction::Out)[0].connected.is_empty(),
            "the party's own record draws no edge, so outbound reaches nobody",
        );

        let from_guest = resolved(
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
    /// asking again per guest, which is two calls for one question.
    #[test]
    fn a_reached_object_brings_its_own_facts() {
        let found = resolved(
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
                    fits_type: None,
                }),
                history: None,
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
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::AnyEdge,
                        direction: Some(Direction::Out),
                        depth,
                        keeping: Vec::new(),
                        fits_type: None,
                    }),
                    history: None,
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
                retracted: false,
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
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::AnyEdge,
                        direction: Some(Direction::Out),
                        depth,
                        keeping: Vec::new(),
                        fits_type: None,
                    }),
                    history: None,
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
        let found = resolved(
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
                    stood_for: false,
                },
                follow: Some(Follow {
                    along: Along::AnyEdge,
                    direction: Some(Direction::Out),
                    depth: MAX_DEPTH,
                    keeping: Vec::new(),
                    fits_type: None,
                }),
                history: None,
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
        let found = resolved(
            &scanned,
            &[],
            &GraphQuery {
                select: Selection {
                    subject: Some(EntityId("person:ned-flanders".into())),
                    // Asked of a record, so the object is the one thing that
                    // brings it back and the filter decides only which of its
                    // records ride along.
                    fields: vec![FieldFilter::holding("rsvp", "yes").on_a_record()],
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

        let missed = resolved(
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
            resolved(&store(), &[], &query).expect_err("this query cannot be served");
        };
        refused(GraphQuery::default());
        refused(GraphQuery {
            select: Selection {
                kind: Some(EntityKind::PERSON),
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
                kind: Some(EntityKind::PERSON),
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
        resolved(
            &store(),
            &[],
            &GraphQuery {
                select: Selection {
                    kind: Some(EntityKind::PERSON),
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

    /// **A walk narrows to the things that FIT a type, which is not the same
    /// as the things that answer it.**
    ///
    /// The bike's repair record carries `owner`, and `owner` is one of `pet`'s
    /// keys — so the bike ANSWERS `pet`, partially, and a narrowing that
    /// admitted partial answers would keep it. Worse, it could never exclude
    /// anything: the walk travels `owner`, so everything it reaches answers the
    /// type by construction. Fitting — every key the type names — is what makes
    /// the narrowing mean something.
    #[test]
    fn a_walk_keeps_what_fits_a_type_and_not_what_merely_answers_it() {
        let reached = |fits: Option<types::DeclaredType>| {
            let found = resolved(
                &kennel(),
                &[pet()],
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:bart".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation("owner".into()),
                        direction: Some(Direction::In),
                        fits_type: fits,
                        ..Follow::hop()
                    }),
                    history: None,
                },
            )
            .expect("a declared relation is followable");
            let mut handles: Vec<String> = found[0]
                .connected
                .iter()
                .map(|o| o.entity.id.to_string())
                .collect();
            handles.sort();
            (handles, found[0].unwalked)
        };

        // Unnarrowed, the walk reaches the bike, and that is the right answer
        // to what it was asked.
        let (everything, _) = reached(None);
        assert_eq!(
            everything,
            vec![
                "pet:santas-little-helper".to_string(),
                "pet:snowball".to_string(),
                "thing:red-bike".to_string(),
            ],
        );

        // Narrowed to what fits, the bike drops out — and the pets, which hold
        // every key, stay. The pair is the point: a negative on its own would
        // pass on a walk that reached nothing at all.
        let (fitting, unwalked) = reached(Some(pet()));
        assert_eq!(
            fitting,
            vec![
                "pet:santas-little-helper".to_string(),
                "pet:snowball".to_string(),
            ],
        );
        // …and the bike is not silently gone: the walk reached it and did not
        // keep it, which is an edge nobody followed.
        assert!(
            unwalked,
            "a thing the narrowing dropped is an unfollowed edge, not an absence"
        );
    }

    /// The type the relation cases declare: a pet, whose `owner` is a
    /// reference and whose `born` is a date.
    fn pet() -> types::DeclaredType {
        types::DeclaredType::new(
            "pet",
            vec![
                types::Field::required("name", types::ValueType::Text),
                types::Field::required("born", types::ValueType::Date),
                types::Field::required("weight", types::ValueType::Number),
                types::Field::required("owner", types::ValueType::Reference),
            ],
        )
    }

    /// One owner and two pets, each pet's record pointing at the owner through
    /// a key the declaration calls a reference.
    fn kennel() -> Vec<DocScan> {
        let pet_record = |home: &str, id: &str, content: &str, born: &str, weight: &str| Fact {
            fields: [
                ("name".to_string(), home.to_string()),
                ("born".to_string(), born.to_string()),
                ("weight".to_string(), weight.to_string()),
                ("owner".to_string(), "person:bart".to_string()),
            ]
            .into_iter()
            .collect(),
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
                    fields: [
                        ("fitted".to_string(), "2026-02-01".to_string()),
                        ("owner".to_string(), "person:bart".to_string()),
                    ]
                    .into_iter()
                    .collect(),
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
    /// **A key declared to hold a LIST of references walks to every one of
    /// them, both ways.**
    ///
    /// A list cell holds its items separated by commas, and the declaration is
    /// what says the cell is a list — the same declaration the write guard
    /// screens each item through. Read as one handle, a two-item cell is a
    /// handle nobody has: the outbound walk reaches nothing and the inbound
    /// walk matches nothing, with no error and no flag on either.
    ///
    /// **A one-item list is indistinguishable from an ordinary reference**, so
    /// nothing shows until a second handle is recorded — which is the case a
    /// list exists for (rule 217).
    ///
    /// Both directions, because they read the cell in different ways and either
    /// can be right while the other is wrong.
    #[test]
    fn a_list_of_references_walks_to_every_handle_in_it() {
        let declarations = vec![types::DeclaredType::new(
            "outing",
            vec![types::Field::listing(
                "came_along",
                types::ValueType::Reference,
            )],
        )];
        let scanned = vec![
            doc(entity("person:bart", "Bart"), "Bart's page.", Vec::new()),
            doc(
                entity("person:milhouse", "Milhouse"),
                "The other one.",
                Vec::new(),
            ),
            doc(
                entity("event:winter-fest", "Winter Fest"),
                "The outing's page.",
                vec![Fact {
                    fields: [(
                        "came_along".to_string(),
                        "person:bart, person:milhouse".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                    ..fact("event:winter-fest", "f1", "who came along")
                }],
            ),
        ];
        let walk = |from: &str, direction: Direction| {
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation("came_along".into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                    history: None,
                },
            )
            .expect("a declared relation is followable")
        };

        let out = walk("event:winter-fest", Direction::Out);
        assert_eq!(
            handles(&out[0].connected),
            vec!["person:bart", "person:milhouse"],
            "a list cell read as one handle reaches nobody: {out:?}",
        );

        let back = walk("person:milhouse", Direction::In);
        assert_eq!(
            handles(&back[0].connected),
            vec!["event:winter-fest"],
            "the second name in the cell is not the whole cell, so an equality on the cell \
             finds nothing: {back:?}",
        );
    }

    /// declares an inverse, because there is nothing to declare — the same key
    /// walked the other way is the other question.
    #[test]
    fn a_declared_reference_key_is_a_walkable_link_both_ways() {
        let scanned = kennel();
        let declarations = vec![pet()];
        let walk = |from: &str, relation: &str, direction: Direction| {
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                    history: None,
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
                retracted: false,
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
                stood_for: false,
            },
            follow: Some(Follow {
                along: Along::Relation("owner".into()),
                ..Follow::hop()
            }),
            history: None,
        };
        resolved(&kennel(), &[], &query).expect_err(
            "with nothing declared, a key holding a handle is a string that looks like one",
        );

        // The same name, declared as ordinary text rather than a reference: the
        // value is identical and it is still not a link.
        let as_text = types::DeclaredType::new(
            "pet",
            vec![types::Field::required("owner", types::ValueType::Text)],
        );
        resolved(&kennel(), std::slice::from_ref(&as_text), &query)
            .expect_err("a key declared to hold text is not a relation");

        // …and the positive in the same case, so the two refusals are about the
        // declaration and not about relations.
        resolved(&kennel(), &[pet()], &query).expect("declared as a reference, it is one");
    }

    /// **One key name, two types, and the text one does not bury the
    /// reference.** Two types may name one key and mean their own thing by it,
    /// so a type calling `owner` text says nothing about the type calling it a
    /// reference — the relation stays walkable either way.
    ///
    /// **Both declaration orders, because the store hands them over sorted by
    /// type name.** A lookup that stopped at the first declaration owning the
    /// name would make walkability depend on alphabetical spelling, and a test
    /// fixing one order would pass on it half the time.
    #[test]
    fn a_text_key_of_another_type_does_not_hide_a_declared_relation() {
        let shelf = types::DeclaredType::new(
            "book",
            vec![types::Field::required("owner", types::ValueType::Text)],
        );
        let walk = |declarations: &[types::DeclaredType]| {
            resolved(
                &kennel(),
                declarations,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("pet:santas-little-helper".into())),
                        ..Selection::default()
                    },
                    include: Include {
                        facts: false,
                        prose: false,
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation("owner".into()),
                        direction: Some(Direction::Out),
                        ..Follow::hop()
                    }),
                    history: None,
                },
            )
            .expect("some type declares 'owner' a reference, so it is one")
        };

        for declarations in [vec![shelf.clone(), pet()], vec![pet(), shelf.clone()]] {
            let found = walk(&declarations);
            assert_eq!(
                handles(&found[0].connected),
                vec!["person:bart"],
                "the reference declaration is the one that counts, whichever came first: \
                 {found:?}",
            );
        }
    }

    /// **Two reference keys of one type stay apart, because they are two
    /// KEYS.** A trip's `from` and its `to` both point at places, and each name
    /// reaches its own end — inbound as well as outbound.
    #[test]
    fn two_reference_keys_of_one_type_do_not_collide() {
        let trip = types::DeclaredType::new(
            "trip",
            vec![
                types::Field::required("from", types::ValueType::Reference),
                types::Field::required("to", types::ValueType::Reference),
            ],
        );
        let travelling = Fact {
            fields: [
                ("from".to_string(), "place:springfield".to_string()),
                ("to".to_string(), "place:shelbyville".to_string()),
            ]
            .into_iter()
            .collect(),
            ..fact("person:bart", "f1", "went over for the day")
        };
        let scanned = vec![
            doc(entity("person:bart", "Bart"), "", vec![travelling]),
            doc(entity("place:springfield", "Springfield"), "", Vec::new()),
            doc(entity("place:shelbyville", "Shelbyville"), "", Vec::new()),
        ];

        let reached = |from: &str, relation: &str, direction: Direction| {
            let found = resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                    history: None,
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
    /// name claims nothing about pets.
    ///
    /// **A reverse walk that also required the record to answer the declared
    /// type, under a name like `pet.owner`, could not fail this case.** That
    /// check excludes nothing: the walked key is one of the type's keys, and
    /// holding one key is what answering a type means. So there is no such
    /// check, and no name advertising one.
    #[test]
    fn a_relation_is_scoped_by_its_key_and_the_direction_chooses_the_way() {
        let scanned = kennel();
        let declarations = vec![pet()];
        let walk = |from: &str, relation: &str, direction: Direction| {
            resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along: Along::Relation(relation.into()),
                        direction: Some(direction),
                        ..Follow::hop()
                    }),
                    history: None,
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
                retracted: false,
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

        // A qualified name is no relation, and the refusal offers the key
        // instead.
        let refused = walk("person:bart", "pet.owner", Direction::In)
            .expect_err("`type.key` is no relation name");
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
            vec![types::Field::required(
                "location",
                types::ValueType::Reference,
            )],
        );
        let by_key = Fact {
            fields: [("location".to_string(), "place:shelbyville".to_string())]
                .into_iter()
                .collect(),
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
            let found = resolved(
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
                        stood_for: false,
                    },
                    follow: Some(Follow {
                        along,
                        ..Follow::hop()
                    }),
                    history: None,
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
            resolved(
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
        let lighter = resolved(
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

        resolved(
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
        let found = resolved(
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
            resolved(
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
                    history: None,
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

    /// **A walk's filters are asked the same two ways a selection's are**, and
    /// which way decides both what the walk reaches and what each object brings.
    ///
    /// The greyhound is described over two records: one carries `born` and one
    /// does not. Asked of the THING, `born` is answered by the fold and the
    /// object arrives whole. Asked of a RECORD, it is answered by the one row
    /// carrying the key, and only that row rides along. Both in one case,
    /// because either alone passes on a build where every filter is the other
    /// scope.
    #[test]
    fn a_walks_filter_is_asked_of_the_thing_or_of_one_record() {
        let mut scanned = kennel();
        let greyhound = scanned
            .iter_mut()
            .find(|d| d.doc_id == "pet:santas-little-helper")
            .expect("the kennel holds the greyhound");
        greyhound
            .facts
            .push(fact("pet:santas-little-helper", "f2", "went to the vet"));
        let declarations = vec![pet()];
        let from_bart = |keeping: Vec<FieldFilter>| {
            resolved(
                &scanned,
                &declarations,
                &GraphQuery {
                    select: Selection {
                        subject: Some(EntityId("person:bart".into())),
                        ..Selection::default()
                    },
                    follow: Some(Follow {
                        along: Along::Relation("owner".into()),
                        direction: Some(Direction::In),
                        keeping,
                        ..Follow::hop()
                    }),
                    ..GraphQuery::default()
                },
            )
            .expect("a walk with filters on what it reaches")
        };

        let of_the_thing = from_bart(vec![FieldFilter::comparing(
            "born",
            types::Compare::Before,
            "2020-01-01",
        )]);
        assert_eq!(
            handles(&of_the_thing[0].connected),
            vec!["pet:santas-little-helper"],
            "the fold answers `born`, so the walk reaches the greyhound: {of_the_thing:?}",
        );
        assert_eq!(
            of_the_thing[0].connected[0].facts.len(),
            2,
            "and a filter about the thing does not narrow the thing's page: {:?}",
            of_the_thing[0].connected[0],
        );

        let of_a_record = from_bart(vec![
            FieldFilter::comparing("born", types::Compare::Before, "2020-01-01").on_a_record(),
        ]);
        assert_eq!(
            handles(&of_a_record[0].connected),
            vec!["pet:santas-little-helper"],
            "the same object, reached because one record of its answers: {of_a_record:?}",
        );
        assert_eq!(
            of_a_record[0].connected[0]
                .facts
                .iter()
                .map(|f| f.content.as_str())
                .collect::<Vec<_>>(),
            vec!["the greyhound"],
            "and it brings the record that answered, not the vet trip: {:?}",
            of_a_record[0].connected[0],
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
            resolved(&scanned, &[], &GraphQuery::subject(EntityId(handle.into())))
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
