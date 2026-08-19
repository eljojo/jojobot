//! `recall` — The graph query: say the shape you want, and get that shape back.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.
//!
//! **It answers with objects, always** — one when a handle was named, several
//! when a filter chose them. A verb whose answer changed shape with its
//! arguments would make every caller branch on which question they had asked.

use jojobot_domain::attention;

use super::*;
use jojobot_domain::memory::graph;

/// One key filter of a `recall` — a key, and optionally the value it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyFilterArgs {
    /// The key, exactly as it is spelled where it was written. Matching is
    /// structural, so nothing has to have declared it.
    pub key: String,
    /// The value it must hold. **Omit it to ask only that the key is there** —
    /// a different question, and the one to ask when you want everything that
    /// records a thing rather than everything that records it one way.
    #[serde(default)]
    pub value: Option<String>,
    /// **How the value is compared**, and what a key was DECLARED to hold is
    /// what licenses it. `equals` is the default and needs no declaration.
    /// `before` and `after` need a type declaring the key a `date`; `less` and
    /// `greater` need one declaring it a `number`. Asking for an ordering a
    /// declaration does not license comes back blocked, rather than quietly
    /// answering the equality question instead.
    #[serde(default)]
    pub compare: Option<String>,
    /// **What this filter is asked OF**, and it is two different questions.
    ///
    /// `thing` is the default: what the object HOLDS — the newest write of the
    /// key, or the total when the key was declared a counter. *Which friends
    /// have eaten three or more donuts* is this one, and it finds the friend
    /// with three separate records of one as readily as the friend with one
    /// record of three.
    ///
    /// `record` asks about the occasions instead: an object comes back when a
    /// single record of its answers every `record` filter, and it arrives
    /// carrying the records that answered. *Which visits cost more than fifty*
    /// is this one — it is about what happened, not about what anybody holds
    /// now, and no folding can answer it.
    ///
    /// ⚠️ **The two scopes combine in one list, and every `record` filter must
    /// hold on the SAME record** — they describe a single record rather than a
    /// list of separate questions.
    #[serde(default)]
    pub scope: Option<String>,
}

/// The `follow` argument of a `recall` — which edges to walk, and how far.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FollowArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about`
    /// · `connection`). Omit for **any** edge — "whatever it is connected to".
    #[serde(default)]
    pub shape: Option<String>,
    /// **A declared relation to walk instead of an edge.** A relation is a KEY
    /// that some type declared to hold a `reference`: the declaration says the
    /// value is another entity rather than a string that looks like one, and
    /// that is what makes it walkable.
    ///
    /// **Name the key, and use `direction` to say which way.** `out` follows
    /// the key off this object's own records — from a pet, `owner` reaches the
    /// person. `in` reaches every record pointing here through that key — from
    /// that person, `owner` reaches the pets.
    ///
    /// ⚠️ **Inbound is scoped by the KEY and by nothing else.** It reaches
    /// every record using that key, whatever the record otherwise is: a repair
    /// record carrying `owner` comes back beside the pets. If you want one kind
    /// of thing, select it — `kind` plus a `fields` filter on the key — rather
    /// than walking.
    ///
    /// A key may be spelled like an edge shape. Pass one or the other, never
    /// both.
    #[serde(default)]
    pub relation: Option<String>,
    /// **Which end of the edge to leave by**, and it is two different
    /// questions. `out` (the default) follows the edges this object's own
    /// records draw — from a guest, the party they are attending. `in` follows
    /// the edges other objects draw AT this one — from the party, its guests.
    #[serde(default)]
    pub direction: Option<String>,
    /// How many hops: 1 is the neighbours, 2 is the neighbours' neighbours.
    /// Defaults to 1.
    #[serde(default)]
    pub depth: Option<u32>,
    /// **What the walk keeps of what it reaches.** The same key filters the
    /// selection takes, applied at every hop rather than to the roots — so
    /// "this person's pets" narrows to "this person's pets born before a date".
    /// Omit to keep everything. An object kept this way arrives carrying the
    /// records that answered, and one that was reached and not kept leaves the
    /// object that points at it marked as having edges nobody followed.
    #[serde(default)]
    pub keeping: Option<Vec<KeyFilterArgs>>,
    /// **Keep only what FITS this type**, by name — the objects carrying EVERY
    /// key the type names, counted across everything recorded about each one.
    ///
    /// ⚠️ **This is a different question from `answers_type` on the call
    /// itself, and the difference is the whole point of having two words.**
    /// `answers_type` selects objects carrying SOME of a type's keys and
    /// reports which each one lacks — it is for finding things worth looking
    /// at, gaps included. `fits_type` keeps only the objects with no gaps. Use
    /// `answers_type` to ask *which of these are described like a pet, and what
    /// is missing*; use `fits_type` to ask *which of these ARE pets*.
    ///
    /// It narrows a walk for a reason a partial match could not: a walk
    /// travelling one of a type's own keys reaches things that answer the type
    /// by construction, so admitting partials would exclude nothing.
    #[serde(default)]
    pub fits_type: Option<String>,
}

/// Arguments to `recall`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct RecallArgs {
    /// One entity, as `kind:slug` (a bare handle is read as a person).
    ///
    /// **A named subject always comes back**, even when the filters below keep
    /// none of its records: naming a handle asks for that object, and a filter
    /// asks which objects. A handle that names nothing comes back blocked with
    /// the nearest handles, never as an empty answer.
    #[serde(default)]
    pub subject: Option<String>,
    /// Every entity of one kind.
    #[serde(default)]
    pub kind: Option<String>,
    /// **Objects that answer this type**, by name. Matching is STRUCTURAL — an
    /// object carrying the type's keys answers it whether or not anybody
    /// declared it one — and it is asked of the OBJECT: every write on it
    /// counts, so a thing described over two sittings answers a type that
    /// neither sitting answers alone.
    ///
    /// ⚠️ **Some of the keys is enough, and that is what separates this from
    /// `follow.fits_type`.** An object holding a few of them comes back saying
    /// which it lacks, because a filter that kept only whole ones would hide
    /// exactly the objects worth finding. `fits_type`, on a walk, keeps only
    /// the objects that hold EVERY key. Ask this one for *which of these are
    /// described like a service, and what is missing*; ask that one for *which
    /// of these ARE services*.
    ///
    /// A name no type answers to comes back blocked, naming the types that do
    /// exist.
    #[serde(default)]
    pub answers_type: Option<String>,
    /// Objects holding a record that carries these keys, and the values named.
    /// **Every filter must hold on ONE record**: two filters describe a single
    /// record, not two separate questions.
    #[serde(default)]
    pub fields: Option<Vec<KeyFilterArgs>>,
    /// Whether each object's records come back — the claims its fields were
    /// written in, each with its own wording, provenance and the address that
    /// edits it.
    ///
    /// **Off by default.** The fields are what a thing HOLDS and they come
    /// back always; the records say the same thing at length, and shipping every one
    /// of them unasked is the cost a caller cannot decline. Ask for them when
    /// you need a claim's own words, where it came from, or its address —
    /// and an answer that left them out says how many there were.
    #[serde(default)]
    pub facts: Option<bool>,
    /// Whether each object's **prose** comes back — the human half of its page,
    /// whole. Off by default, because a page is bigger than a claim and shipping
    /// every one of them unasked is a cost the caller cannot decline.
    #[serde(default)]
    pub prose: Option<bool>,
    /// **The writes behind one key, oldest first** — name the key, and each
    /// object comes back carrying every write of it, with the record each one
    /// arrived in and that record's date.
    ///
    /// A read is current truth: one value per key, the newest write. This is
    /// the other question the same data answers — every time the key was
    /// written — and **the count of the writes is the answer to "how many
    /// times"**. Ask it of `weight` and you get what a thing has weighed over
    /// the years; ask it of a key a sitting records once each time it happens
    /// and you get how often.
    ///
    /// Omit it and no history comes back at all, which is the normal read: a
    /// caller who wants the value wants one value. A key nobody has written
    /// comes back as no writes rather than as a refusal — the thing is there
    /// and nothing was recorded under that key.
    #[serde(default)]
    pub history: Option<String>,
    /// **How many of that key's writes come back**, newest kept. Twenty when
    /// you do not say.
    ///
    /// A key written a thousand times would otherwise be a thousand entries in
    /// an answer somebody asked one narrow question of. The answer always says
    /// how many writes exist and how many it left out, so raising this is how
    /// you reach the far end — deliberately, rather than by surprise.
    #[serde(default)]
    pub history_most: Option<u32>,
    /// **Keep only what is OWED as of a date** — what has a due moment that
    /// day has reached.
    ///
    /// It OFFERS and never acts: arithmetic on what a thing holds and the day
    /// you name, never a judgement that something should happen. **It reads a
    /// clock never** — the day is yours to give, so *what is owed as of next
    /// Friday* is the same question asked about a different day.
    ///
    /// **What falls due, and when, is the KIND's own answer.** A loop is next
    /// due a cadence after the date its cycle counts from; another sort of owed
    /// thing computes its moment its own way, and this filter compares the
    /// moment rather than computing it.
    ///
    /// **A kind that owes nothing is absent**, whatever it holds — a person is
    /// never late, because nothing computes a due moment for a person.
    ///
    /// **A thing that carries none of what its kind falls due on is absent
    /// too**: a loop nobody has put a schedule on made no promise to be late
    /// against.
    ///
    /// **But a thing whose due moment cannot be READ comes back owed**,
    /// carrying its fields so you can see which key it is short of. That is
    /// deliberate: a half-built loop that surfaced at no read ever would never
    /// be heard from again.
    #[serde(default)]
    pub overdue: Option<OverdueArgs>,
    /// Which edges to walk. Omit to walk none, and the answer is flat.
    #[serde(default)]
    pub follow: Option<FollowArgs>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// **The overdue question of a `recall`** — which rhythms have gone quiet, as
/// of a date.
///
/// It is a sub-object rather than a flag beside a date because the date only
/// means anything when the question is asked. Two flat arguments would admit a
/// call that names a day and filters nothing, which reads as an answer to a
/// question nobody asked.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OverdueArgs {
    /// **The date the question is asked about**, `YYYY-MM-DD`. Omit it and
    /// jojobot uses today — the one clock read, taken here at the edge, and the
    /// answer says which day it used.
    ///
    /// Naming a date is what makes *what has gone quiet by next Friday* a
    /// question this can be asked.
    #[serde(default)]
    pub as_of: Option<String>,
}

/// The key filters of a call, wherever they sit: a selection describes the
/// roots and a walk's describe what it reaches, and they are the same filter.
fn key_filters(args: &[KeyFilterArgs]) -> Result<Vec<graph::FieldFilter>, McpError> {
    args.iter()
        .map(|f| {
            Ok(graph::FieldFilter {
                key: f.key.trim().to_string(),
                value: f.value.as_ref().map(|v| v.trim().to_string()),
                compare: parse_compare(f.compare.as_deref())?,
                scope: parse_scope(f.scope.as_deref())?,
            })
        })
        .collect()
}

/// **One key's writes on the wire, oldest first, with how many there are.**
///
/// The count is rendered beside them because counting is the question the
/// substrate exists to answer, and a caller that has to length the list to
/// answer it is a caller doing arithmetic jojobot already did.
///
/// A write that took the key off carries `cleared` instead of a value. A null
/// `value` would say the same thing to a reader who already knew the
/// convention, and nothing to one who did not.
fn history_json(history: &graph::KeyHistory) -> serde_json::Value {
    let mut body = serde_json::json!({
        "key": history.key,
        "count": history.total,
        "shown": history.writes.len(),
        "writes": history.writes.iter().map(|write| {
            let mut rendered = serde_json::json!({
                "record": write.fact.to_string(),
                "date": write.date.to_string(),
                "status": write.status.as_token(),
            });
            if let Some(fields) = rendered.as_object_mut() {
                match &write.value {
                    Some(value) => fields.insert("value".into(), value.as_str().into()),
                    None => fields.insert("cleared".into(), true.into()),
                };
            }
            rendered
        }).collect::<Vec<_>>(),
    });
    // **A cut says so, and says the way past itself.** A window that quietly
    // dropped the older writes would read as a complete history — the one
    // reading a caller cannot check and would have no reason to doubt.
    if let Some(fields) = body.as_object_mut()
        && history.elided() > 0
    {
        fields.insert(
            "older".into(),
            format!(
                "{} older writes are not here — ask again with a bigger history_most to reach \
                 them",
                history.elided()
            )
            .into(),
        );
    }
    body
}

/// One object on the wire, and everything it reached.
///
/// **Absence means "not asked for"** on both halves that can be turned off:
/// `facts` and `prose` are missing when the caller declined them, rather than
/// rendered empty, so nothing has to tell an empty page from a page nobody
/// wanted.
fn object_json(object: &graph::Object, include: graph::Include) -> serde_json::Value {
    let mut body = entity_json(&object.entity);
    let Some(fields) = body.as_object_mut() else {
        return body;
    };
    // **What the thing IS, in one row.** Always here: every write on it folded,
    // one value per key, the newest write of that key winning. It is the answer
    // to the question a caller usually has, and it is a fraction of the size of
    // the records those writes arrived in.
    fields.insert(
        "fields".into(),
        object
            .fields
            .iter()
            .map(|(key, value)| (key.clone(), serde_json::Value::from(value.as_str())))
            .collect::<serde_json::Map<_, _>>()
            .into(),
    );
    // **Eliding is never silent.** Without the note an object carrying no
    // `facts` key says both "you did not ask for them" and "there is nothing
    // recorded here", and a reader who has to infer which will infer wrong.
    if include.facts {
        fields.insert("facts".into(), object.facts.iter().map(fact_json).collect());
    } else if object.facts_held > 0 {
        fields.insert(
            "records".into(),
            format!(
                "{} records are behind these fields and are not here — ask again with facts: \
                 true to read them, each with the address that edits it",
                object.facts_held
            )
            .into(),
        );
    }
    if let Some(prose) = object.prose.as_ref() {
        fields.insert("prose".into(), prose.as_str().into());
    }
    // **How this THING answers the type, when one was named.** Absent when
    // none was: a key rendered null on every object of every other query would
    // be a field a reader has to learn to ignore.
    if let Some(answers) = object.answers.as_ref() {
        fields.insert("answers".into(), answers_json(answers));
    }
    // **The writes behind the key the call named**, absent when it named none:
    // a `history` rendered null on every ordinary read is a key a reader has to
    // learn to ignore. An empty list is the other answer — nobody wrote it —
    // and it is not the same one.
    if let Some(history) = object.history.as_ref() {
        fields.insert("history".into(), history_json(history));
    }
    // How the walk got here. Absent on a root, which nothing reached.
    //
    // An edge names its shape and a relation names itself, under different
    // keys: they are different things, and one key holding either would leave
    // a reader to work out which by looking at the value.
    if let Some(via) = object.via.as_ref() {
        let link = match &via.link {
            graph::Link::Edge(shape) => serde_json::json!({ "type": shape.as_name() }),
            graph::Link::Relation(name) => serde_json::json!({ "relation": name }),
        };
        let mut link = link;
        if let Some(fields) = link.as_object_mut() {
            fields.insert("direction".into(), via.direction.as_token().into());
        }
        fields.insert("via".into(), link);
    }
    fields.insert(
        "connected".into(),
        object
            .connected
            .iter()
            .map(|o| object_json(o, include))
            .collect(),
    );
    // **Eliding is never silent.** An empty `connected` otherwise means both
    // "the walk stopped here" and "there is nothing there", and the note says
    // which as well as how to get the rest.
    if object.unwalked {
        fields.insert(
            "unwalked".into(),
            "this object draws edges the walk did not follow — recall it by handle, or ask again \
             with a deeper follow"
                .into(),
        );
    }
    body
}

/// The graph query — objects, what is on them, and what they reach.
#[tool_router(router = recall_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "The graph query: say the shape you want and get that shape back. Use it \
                       when you can describe what you are after — a handle, a kind, a type, a key \
                       and its value — and use search when you are looking for something and only \
                       have words for it. Three axes, and they COMBINE into one question rather \
                       than three. WHICH OBJECTS: subject (one handle), kind, answers_type \
                       (structural — an OBJECT answers a type by the keys every write on it \
                       leaves standing, whether or not anybody declared it one, and carrying \
                       SOME of them is enough: the answer says which it lacks), and fields (a \
                       key, and the value it holds; omit the value to ask only that the key is \
                       there). WHAT OF EACH: every object always comes back as its FIELDS — \
                       every write on it folded, one value per key, the newest write of that key \
                       winning, and a write that takes a key off takes it off the thing. Those \
                       fields are what the thing HOLDS — its KIND is what it \
                       IS — and they answer most questions; the records behind them are bigger and say the same thing at \
                       length. Ask for facts when you need a claim's own wording, its \
                       provenance, or the address that edits it, and the answer says how many \
                       records it left out when you did not. Then prose, off by default, which is the human half of the \
                       object's page, whole; and history, which names ONE KEY and brings back \
                       every write of it, oldest first. A read is current truth — one value per \
                       key, the newest write — and history is the other question the same data \
                       answers: every time that key was written, with the record each write \
                       arrived in and its date. THE COUNT OF THE WRITES IS THE ANSWER TO HOW \
                       MANY TIMES, so a key a sitting records once each time something happens \
                       is how you count occurrences. A key nobody wrote comes back as no \
                       writes, never as a refusal. A long history comes back CUT to its newest \
                       twenty, saying how many exist and how many it left out; history_most \
                       raises the window when you really want the far end. WHICH EDGES: follow {shape, direction, depth}, and \
                       THE ANSWER NESTS — a walked object carries the objects it reached, each \
                       carrying its own. NARROW A WALK WITH follow.fits_type, which is the \
                       stricter half of the pair: answers_type selects objects carrying SOME of \
                       a type's keys and reports the gaps, fits_type keeps only the ones \
                       carrying EVERY key. Which of these are described like a pet, versus which \
                       of these ARE pets. Direction is two different questions: `out` follows the \
                       edges this object's records draw, `in` follows the edges drawn AT it, so \
                       from a party `in` reaches its guests and from a guest `out` reaches the \
                       party. A RELATION is the other kind of link, and a DECLARATION is what \
                       makes one: a KEY some type declared to hold a `reference` points at \
                       another entity, so it is walkable. Name the key and use `direction` — \
                       `out` reaches what a record points at ('owner' from a pet reaches the \
                       person), `in` reaches every record pointing here through that key ('owner' \
                       from that person reaches the pets). Inbound is scoped by the KEY ALONE: it \
                       returns every record using that key, so a repair record carrying `owner` \
                       arrives beside the pets. For one kind of thing, SELECT it — `kind` plus a \
                       `fields` filter on the key — rather than walking. `keeping` narrows what \
                       the walk reaches, taking the same key \
                       filters, so 'this person's pets' becomes 'this person's pets born before a \
                       date'. A key's DECLARED value type also licenses how you compare it: \
                       `before` and `after` on a declared date, `less` and `greater` on a \
                       declared number, `equals` always and on anything. That is what declaring a \
                       type buys you — reach — and nothing is gated by it: an undeclared record \
                       is still found by the keys it carries. That nesting is what a flat list cannot express: one call answers \
                       'these objects, and what each of them is connected to' instead of one call \
                       per object. Unlike search this returns claims of EVERY status, superseded \
                       included. A named subject always comes back, even when the filters keep \
                       none of its records — naming a handle asks for that object, a filter asks \
                       which objects — while a handle that names nothing comes back blocked with \
                       the nearest handles, never as an empty answer. An object that draws edges \
                       the walk did not follow says so; an empty `connected` with no such note is \
                       an object with nothing beyond it. A query that narrows nothing is refused: \
                       name at least one of subject, kind, answers_type or fields."
    )]
    pub(crate) async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the read runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        if let Err(refused) = self.attributable(args.sid.as_deref()) {
            return Ok(refused);
        }
        // **The name is resolved to its declaration here, once**, exactly as
        // `search` resolves it: everything below takes the keys rather than
        // the name, and a name nobody declared is answered where the roster to
        // offer instead is in reach.
        let answers_type = match &args.answers_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "recalled").await? {
                Ok(declared) => Some(declared),
                Err(refused) => return Ok(refused),
            },
        };
        // Resolved here for the same reason `answers_type` is, and separately
        // from it: the two narrow different halves of one call, and a name that
        // names no type has to be answered where the roster to offer instead is
        // in reach.
        let fits_type = match args.follow.as_ref().and_then(|f| f.fits_type.as_ref()) {
            None => None,
            Some(wanted) => match self.declared(wanted, "walked to").await? {
                Ok(declared) => Some(declared),
                Err(refused) => return Ok(refused),
            },
        };
        // **Two link vocabularies, and a call names one of them.** An edge
        // shape and a relation describe different things — a link nobody typed,
        // and a link a declaration made — so a call carrying both is refused
        // rather than one of them being picked.
        if let Some(f) = args.follow.as_ref()
            && f.shape.is_some()
            && f.relation.is_some()
        {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "follow a shape or a relation, not both: a shape is one of the five names for \
                     a link nobody typed, and a relation is a key some type declared to hold a \
                     reference"
                        .into(),
                ),
            );
        }
        let follow = args
            .follow
            .as_ref()
            .map(|f| -> Result<graph::Follow, McpError> {
                Ok(graph::Follow {
                    along: match (&f.relation, &f.shape) {
                        (Some(relation), _) => graph::Along::Relation(relation.trim().to_string()),
                        (None, Some(shape)) => graph::Along::Edge(parse_shape(shape)?),
                        (None, None) => graph::Along::AnyEdge,
                    },
                    // Kept as an Option all the way down, because a relation
                    // already says which way it goes and the domain refuses the
                    // pair — which it can only do if "unset" and "out" are
                    // still telling apart by the time it looks.
                    direction: f
                        .direction
                        .as_deref()
                        .map(|d| parse_direction(Some(d)))
                        .transpose()?,
                    depth: f.depth.map_or(1, |d| d as usize),
                    keeping: key_filters(f.keeping.as_deref().unwrap_or_default())?,
                    fits_type: fits_type.clone(),
                })
            })
            .transpose()?;
        // **The one clock read, and only when the question needs it.** The
        // domain reads no clock at all: it is handed the date and stays a
        // function of its arguments, so *what is overdue as of next Friday* is
        // the same code path as *what is overdue now*.
        let as_of = match &args.overdue {
            None => None,
            Some(overdue) => Some(parse_date(overdue.as_of.as_deref())?),
        };
        let include = graph::Include {
            facts: args.facts.unwrap_or(false),
            prose: args.prose.unwrap_or(false),
        };
        let query = graph::GraphQuery {
            select: graph::Selection {
                subject: args.subject.as_deref().map(EntityId::person),
                kind: args.kind.as_deref().map(parse_kind).transpose()?,
                answers_type,
                fields: key_filters(args.fields.as_deref().unwrap_or_default())?,
            },
            include,
            follow,
            // Trimmed like every other key a caller names, so `donuts_eaten `
            // asks the question `donuts_eaten` answers.
            history: args.history.as_deref().map(|key| graph::History {
                key: key.trim().to_string(),
                most: args
                    .history_most
                    .map_or(graph::WRITES_SHOWN, |most| most as usize),
            }),
        };

        let mut found = match graph::walk(self.memory.as_ref(), &query).await {
            Ok(found) => found,
            Err(e) => return memory_declined("recall", e),
        };
        // **The arithmetic is the domain's and the selection is here.** It is
        // asked of the object's FOLDED fields — every write on it, one value
        // per key — because a rhythm is described a record at a time: the
        // record that set it up carries the cadence, and each check-in since
        // carries what it found.
        if let Some(as_of) = as_of {
            // **The read compares a moment to a day and computes none of them.**
            // Which moment a thing falls due at is its carrier's answer, so a
            // second sort of owed thing lands by answering here rather than by
            // this line growing a branch — and a kind no carrier speaks for
            // owes nothing, which is what keeps a person out of an answer about
            // what is late.
            let carriers = attention::shipped();
            let asked: Vec<&dyn attention::Carrier> =
                carriers.iter().map(std::convert::AsRef::as_ref).collect();
            found.retain(|object| {
                attention::owed(&asked, object.entity.id.kind_token(), &object.fields)
                    .owed_on(as_of)
            });
        }
        let body = serde_json::json!({
            "count": found.len(),
            // **The day the question was asked about**, and null when it asked
            // about none. A caller that let jojobot supply today has no other
            // way to learn which day that was, and an answer about an unnamed
            // day is one nobody can check.
            "overdue_as_of": as_of.map(|d| d.to_string()),
            "objects": found
                .iter()
                .map(|o| object_json(o, include))
                .collect::<Vec<_>>(),
        });
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// The whole-page query, spelled once: one handle, its records and nothing
    /// else. **It asks for the records**, which are off by default — a case
    /// asserting on a claim's own wording has to ask for the claim.
    fn of(subject: &str) -> RecallArgs {
        RecallArgs {
            subject: Some(subject.into()),
            kind: None,
            answers_type: None,
            fields: None,
            facts: Some(true),
            prose: None,
            follow: None,
            overdue: None,
            sid: None,
            history: None,
            history_most: None,
        }
    }

    /// A rhythm under something, holding a whole schedule.
    async fn a_rhythm(jojobot: &Jojobot, handle: &str, cadence: &str, counts_from: &str) {
        ensure(jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", handle, handle)
            }))
            .await
            .expect("add ok");
        capture_ok(
            jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), cadence.to_string()),
                        ("advances_from".to_string(), "due_date".to_string()),
                        ("counts_from".to_string(), counts_from.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("rhythm:{handle}"), "the loop, set up")
            },
        )
        .await;
    }

    /// Which handles an answer came back with, in order.
    fn handles(body: &serde_json::Value) -> Vec<String> {
        body["objects"]
            .as_array()
            .expect("an answer carries objects")
            .iter()
            .map(|o| {
                o["id"]
                    .as_str()
                    .expect("an object has a handle")
                    .to_string()
            })
            .collect()
    }

    /// **Which rhythms have gone quiet, as of a date the caller names.**
    ///
    /// Two loops on different cadences and one date: the answer is the
    /// difference between them. Asserted with the loop that is NOT overdue in
    /// the same store, because an answer that returns everything is
    /// indistinguishable from one that returns the right things.
    ///
    /// The date is an argument, so the same store gives a different answer for
    /// a later day — which is what makes *what is overdue as of next Friday* a
    /// question this can be asked, and what stops the case rotting when the
    /// calendar moves.
    #[tokio::test]
    async fn recall_says_which_rhythms_have_gone_quiet_as_of_a_date() {
        let jojobot = handler();
        a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
        a_rhythm(&jojobot, "deep-clean", "90", "2026-08-01").await;

        let quiet = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    overdue: Some(OverdueArgs {
                        as_of: Some("2026-08-10".into()),
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            handles(&quiet),
            vec!["rhythm:descale".to_string()],
            "the weekly loop fell due on the eighth and the quarterly one has not: {quiet}",
        );
        assert_eq!(
            quiet["overdue_as_of"], "2026-08-10",
            "the answer says which day it was asked about: {quiet}",
        );

        // The same store, a later day: the quarterly one has fallen due too.
        // Without this the case passes on a build that keeps whatever it likes.
        let later = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    overdue: Some(OverdueArgs {
                        as_of: Some("2026-11-01".into()),
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            handles(&later),
            vec![
                "rhythm:deep-clean".to_string(),
                "rhythm:descale".to_string()
            ],
            "both loops have gone quiet by November: {later}",
        );

        // And the positive the filter rests on: without it the read is the
        // ordinary one and both come back whatever the date.
        let all = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            handles(&all).len(),
            2,
            "an unfiltered read keeps both: {all}"
        );
        assert_eq!(
            all["overdue_as_of"],
            serde_json::Value::Null,
            "and it names no date, because it asked about none: {all}",
        );
    }

    /// **A rhythm that cannot say when it is due is overdue**, which is the
    /// loud answer rather than the tidy one: the alternative is a half-built
    /// loop that surfaces at no boot ever and is never heard from again.
    ///
    /// It comes back carrying its fields, so the caller can see which key it is
    /// short of.
    #[tokio::test]
    async fn a_rhythm_with_half_a_schedule_is_overdue_rather_than_invisible() {
        let jojobot = handler();
        a_rhythm(&jojobot, "descale", "7", "2026-08-01").await;
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "half-made", "Half Made")
            }))
            .await
            .expect("add ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("cadence_days".to_string(), "7".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("rhythm:half-made", "every week, roughly")
            },
        )
        .await;

        // A date before the whole loop is due: the only reason the half-made
        // one is here is that nothing can say when it falls due.
        let quiet = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    overdue: Some(OverdueArgs {
                        as_of: Some("2026-08-02".into()),
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            handles(&quiet),
            vec!["rhythm:half-made".to_string()],
            "the loop nobody finished is the one that surfaces: {quiet}",
        );
        assert_eq!(
            quiet["objects"][0]["fields"]["cadence_days"], "7",
            "and it arrives with what it does hold, so the gap is readable: {quiet}",
        );
    }

    /// **The writes behind a key come back on the read that already exists**,
    /// and only when the call asks for them.
    ///
    /// Both halves, because either alone is satisfied by the wrong build: a
    /// history that is always there costs every caller who never asked, and one
    /// that is never there is a capability nothing can reach. The count is
    /// asserted beside the values because counting is what the whole substrate
    /// is for, and the addresses because a history of one record's edits and a
    /// history of a key written by many records are the two answers this could
    /// have been.
    #[tokio::test]
    async fn recall_answers_with_the_writes_behind_a_key_when_asked() {
        let jojobot = handler();
        for nth in 1..=3 {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some(
                        [("donuts_eaten".to_string(), nth.to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("alpha", &format!("ate one, number {nth}"))
                },
            )
            .await;
        }

        let asked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    history: Some("donuts_eaten".into()),
                    ..of("alpha")
                }))
                .await
                .expect("recall ok"),
        );
        let history = &asked["objects"][0]["history"];
        assert_eq!(history["key"], "donuts_eaten");
        assert_eq!(
            history["count"], 3,
            "the count of the writes is the answer to how many times: {history}"
        );
        assert_eq!(
            history["writes"]
                .as_array()
                .expect("the writes come back as a list")
                .iter()
                .map(|w| w["value"].as_str().unwrap_or_default().to_string())
                .collect::<Vec<_>>(),
            vec!["1", "2", "3"],
            "oldest first: {history}"
        );
        assert_eq!(
            history["writes"][0]["record"], "person:alpha#f1",
            "each write names the record it arrived in, and they differ: {history}"
        );
        assert_eq!(history["writes"][2]["record"], "person:alpha#f3");

        let plain = json_of(
            &jojobot
                .recall(Parameters(of("alpha")))
                .await
                .expect("recall ok"),
        );
        assert!(
            plain["objects"][0].get("history").is_none(),
            "a call that asked for no key is not charged for one: {plain}"
        );
    }

    /// **A thing comes back as one dense row, and the records it was folded
    /// from say they are not here.**
    ///
    /// The whole point of the read: a caller asking what a thing HOLDS gets one
    /// value per key rather than every claim ever made about it. Both halves,
    /// because a row that is always there and records that are always there is
    /// the build this replaces.
    #[tokio::test]
    async fn recall_answers_with_the_folded_row_and_names_what_it_left_out() {
        let jojobot = handler();
        for (key, value, content) in [
            ("weight", "11", "weighed at the bench"),
            ("wheel", "700c", "measured the rim"),
            ("weight", "12", "weighed again after the rebuild"),
        ] {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some([(key.to_string(), value.to_string())].into_iter().collect()),
                    ..capture_args("thing:gravel-bike", content)
                },
            )
            .await;
        }

        let dense = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    facts: Some(false),
                    ..of("thing:gravel-bike")
                }))
                .await
                .expect("recall ok"),
        );
        let object = &dense["objects"][0];
        assert_eq!(
            object["fields"],
            serde_json::json!({"weight": "12", "wheel": "700c"}),
            "one row, the newest write winning a repeated key: {object}"
        );
        assert!(
            object.get("facts").is_none(),
            "the records were not asked for: {object}"
        );
        let note = object["records"]
            .as_str()
            .expect("an answer that left the records out says so");
        assert!(
            note.contains('3') && note.contains("facts"),
            "…how many there are, and the call that returns them: {note}"
        );

        // The other half: asked for, they come back — with the addresses that
        // make them editable, which is what a caller loses if the dense read
        // is the only one.
        let whole = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    facts: Some(true),
                    ..of("thing:gravel-bike")
                }))
                .await
                .expect("recall ok"),
        );
        let object = &whole["objects"][0];
        assert_eq!(
            object["facts"].as_array().map(Vec::len),
            Some(3),
            "every record is reachable: {object}"
        );
        assert_eq!(object["facts"][0]["address"], "thing:gravel-bike#f1");
        assert!(
            object.get("records").is_none(),
            "…and nothing was left out, so nothing says it was: {object}"
        );
        assert_eq!(
            object["fields"], dense["objects"][0]["fields"],
            "the row does not change with the records"
        );
    }

    /// **The records are off unless the call asks**, and the fields are not.
    ///
    /// The default itself, which every other case here now states explicitly —
    /// so a build that quietly went back to shipping every claim would pass
    /// all of them and fail this one. It reads a plain call, the one an agent
    /// makes when it knows nothing about arguments.
    #[tokio::test]
    async fn a_plain_recall_ships_the_fields_and_not_the_records() {
        let jojobot = handler();
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("weight".to_string(), "11".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("thing:gravel-bike", "weighed at the bench")
            },
        )
        .await;

        let plain = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("thing:gravel-bike".into()),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let object = &plain["objects"][0];
        assert_eq!(
            object["fields"],
            serde_json::json!({"weight": "11"}),
            "what the thing is comes back unasked: {object}"
        );
        assert!(
            object.get("facts").is_none(),
            "…and the claims behind it do not: {object}"
        );
        assert!(
            object["records"].as_str().is_some_and(|n| n.contains('1')),
            "…and the answer says they are there and how to read them: {object}"
        );
    }

    /// **A history longer than the window comes back cut, and the answer says
    /// how many exist and how to reach the rest.**
    ///
    /// A read whose job is the small answer must not be able to flood the
    /// caller it serves. The short case is asserted beside it because a cap
    /// that fires always and a cap that fires never look the same from one
    /// call.
    #[tokio::test]
    async fn a_long_history_is_capped_on_the_wire_and_says_what_it_left_out() {
        let jojobot = handler();
        let written = jojobot_domain::memory::graph::WRITES_SHOWN + 5;
        for nth in 1..=written {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some(
                        [("donuts_eaten".to_string(), nth.to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("alpha", &format!("ate one, number {nth}"))
                },
            )
            .await;
        }

        let capped = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    history: Some("donuts_eaten".into()),
                    facts: Some(false),
                    ..of("alpha")
                }))
                .await
                .expect("recall ok"),
        );
        let history = &capped["objects"][0]["history"];
        assert_eq!(
            history["count"], written,
            "the answer says how many writes exist: {history}"
        );
        assert_eq!(
            history["shown"],
            jojobot_domain::memory::graph::WRITES_SHOWN,
            "…and how many it handed over"
        );
        assert_eq!(
            history["writes"].as_array().map(Vec::len),
            Some(jojobot_domain::memory::graph::WRITES_SHOWN),
            "…which is what it actually handed over: {history}"
        );
        assert_eq!(
            history["writes"][0]["value"], "6",
            "the window is the newest writes, still oldest first: {history}"
        );
        let older = history["older"]
            .as_str()
            .expect("a cut answer says it was cut");
        assert!(
            older.contains('5') && older.contains("history_most"),
            "…how many are missing, and the way to them: {older}"
        );

        // Raising the window is that way, and it works — the whole history,
        // and no note, because nothing was left out.
        let whole = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    history: Some("donuts_eaten".into()),
                    history_most: Some(written as u32),
                    facts: Some(false),
                    ..of("alpha")
                }))
                .await
                .expect("recall ok"),
        );
        let history = &whole["objects"][0]["history"];
        assert_eq!(history["writes"].as_array().map(Vec::len), Some(written));
        assert_eq!(history["writes"][0]["value"], "1");
        assert!(
            history.get("older").is_none(),
            "a history that came back whole carries no elision noise: {history}"
        );
    }

    /// Every recalled fact carries its address, and that address is what
    /// `update_fact` takes — the pairing that makes editing possible.
    #[tokio::test]
    async fn recall_returns_addresses_that_update_fact_accepts() {
        let jojobot = handler();
        capture_ok(&jojobot, capture_args("alpha", "works at the old place")).await;

        let body = json_of(
            &jojobot
                .recall(Parameters(of("alpha")))
                .await
                .expect("recall ok"),
        );
        let address = body["objects"][0]["facts"][0]["address"]
            .as_str()
            .expect("every fact carries an address");
        assert_eq!(address, "person:alpha#f1");

        let updated = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("works at the new place".into()),
                    details: Some("changed jobs in July".into()),
                    ..update_args(address)
                }))
                .await
                .expect("update ok"),
        );
        // **The address recall handed over is the address the edit landed on**,
        // which is the whole claim of this case. The claim itself is read back
        // through the verb that reads, since the edit answers with a receipt.
        assert_eq!(updated["address"], "person:alpha#f1");
        assert_eq!(updated["content_elided"], true, "{updated}");
        let read = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        let claim = &read["objects"][0]["facts"][0];
        assert_eq!(claim["content"], "works at the new place", "{read}");
        assert_eq!(claim["details"], "changed jobs in July", "{read}");
    }

    /// **An unknown handle is a miss at the wire too.** The production smoke
    /// test asked for a nonexistent person and was told "reads fine, no facts"
    /// — the same answer an empty page gives, so a caller can never repair a
    /// bad handle. The miss now comes back as an error naming the handle and
    /// its near candidates, while an empty-but-real entity still reads fine.
    #[tokio::test]
    async fn recall_of_an_unknown_entity_is_a_miss_with_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let missed = blocked(
            &jojobot
                .recall(Parameters(of("person:zenit")))
                .await
                .expect("a handle that names nothing is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "person:zenit");
        assert_eq!(
            missed["candidates"][0]["handle"], "person:zenith",
            "the near candidate surfaces: {missed}"
        );

        let body = json_of(
            &jojobot
                .recall(Parameters(of("person:zenith")))
                .await
                .expect("an existing entity's empty page still reads"),
        );
        assert_eq!(
            body["objects"][0]["facts"]
                .as_array()
                .expect("a list")
                .len(),
            0
        );
    }

    /// **`recall` shows the edges too.** Search grew a neighborhood; a recall
    /// that answered with the same rows stripped of their edges would make the
    /// graph a thing you can only see by searching for it, and reading an
    /// entity's own page is the commonest way anyone looks.
    #[tokio::test]
    async fn recall_returns_the_edge_a_fact_draws() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("org", "guild", "The Guild")))
            .await
            .expect("add_entity ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:guild".into()),
                ..capture_args("alpha", "joined in the spring")
            },
        )
        .await;

        let body = json_of(
            &jojobot
                .recall(Parameters(of("alpha")))
                .await
                .expect("recall ok"),
        );
        let edged = body["objects"][0]["facts"]
            .as_array()
            .expect("recall returns a list")
            .iter()
            .find(|f| f["content"] == "joined in the spring")
            .unwrap_or_else(|| panic!("the captured fact must come back: {body}"));
        assert_eq!(edged["edge"]["type"], "memberOf", "got {edged}");
        assert_eq!(edged["edge"]["object"], "org:guild");
    }

    /// **Objects of a kind, with their pages** — through the wire, and with
    /// nothing in the call naming what the page is for.
    ///
    /// The negative it rests on is the same query without `prose`: the key is
    /// then absent rather than empty, so a caller can tell a page nobody asked
    /// for from a page with nothing on it.
    #[tokio::test]
    async fn a_kind_comes_back_with_each_objects_prose() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("bot", "gamma", "Gamma")))
            .await
            .expect("add_entity ok");
        let charter = "Keeps the roster.\n\nHard line: never writes to the ledger.";
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: charter.into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let asked = RecallArgs {
            kind: Some("bot".into()),
            prose: Some(true),
            facts: Some(false),
            ..of_nothing()
        };
        let body = json_of(&jojobot.recall(Parameters(asked)).await.expect("recall ok"));
        let gamma = body["objects"]
            .as_array()
            .expect("a list of objects")
            .iter()
            .find(|o| o["id"] == "bot:gamma")
            .unwrap_or_else(|| panic!("the bot must be in a query for its kind: {body}"));
        assert_eq!(
            gamma["prose"], charter,
            "the page comes back whole: {gamma}"
        );
        assert!(
            gamma.get("facts").is_none(),
            "facts were declined, so the key is absent: {gamma}"
        );

        let unasked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("bot".into()),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert!(
            unasked["objects"][0].get("prose").is_none(),
            "prose nobody asked for is absent rather than empty: {unasked}"
        );
    }

    /// **A key's value selects, and the walk nests** — the two halves this
    /// verb grew, through the surface a caller uses.
    #[tokio::test]
    async fn a_value_selects_and_a_walk_nests() {
        let jojobot = handler();
        for (kind, slug, name) in [
            ("event", "birthday-party", "Birthday Party"),
            ("person", "patana", "Patana"),
        ] {
            jojobot
                .add_entity(Parameters(add_args(kind, slug, name)))
                .await
                .expect("add_entity ok");
        }
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("attendance".into()),
                object: Some("event:birthday-party".into()),
                fields: Some(
                    [("answer".to_string(), "yes".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("patana", "coming to the party")
            },
        )
        .await;
        capture_ok(&jojobot, capture_args("patana", "vegetarian")).await;

        let selected = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    fields: Some(vec![KeyFilterArgs {
                        key: "answer".into(),
                        value: Some("yes".into()),
                        compare: None,
                        // The record that answered is what this half is about,
                        // so it asks the question that is about records.
                        scope: Some("record".into()),
                    }]),
                    // The claim that answered is what this case is about, so
                    // it asks for the records the fold is taken from.
                    facts: Some(true),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(selected["objects"][0]["id"], "person:patana", "{selected}");
        assert_eq!(
            selected["objects"][0]["facts"].as_array().map(Vec::len),
            Some(1),
            "the object carries the record that answered, not its whole page: {selected}"
        );

        let walked = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("event:birthday-party".into()),
                    follow: Some(FollowArgs {
                        shape: Some("attendance".into()),
                        relation: None,
                        direction: Some("in".into()),
                        depth: None,
                        keeping: None,
                        fits_type: None,
                    }),
                    // The claim a reached object carries is what the walk half
                    // asserts on, so this half asks for the records too.
                    facts: Some(true),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let guest = &walked["objects"][0]["connected"][0];
        assert_eq!(guest["id"], "person:patana", "{walked}");
        assert_eq!(guest["via"]["direction"], "in", "{walked}");
        assert!(
            guest["facts"]
                .as_array()
                .expect("the guest's own facts")
                .iter()
                .any(|f| f["content"] == "vegetarian"),
            "a reached object arrives carrying its own page: {walked}"
        );
    }

    /// **A query that narrows nothing is refused**, with a way forward rather
    /// than a protocol failure — and the same call with one filter is served,
    /// so the refusal is about the argument and not about the verb.
    #[tokio::test]
    async fn a_query_that_narrows_nothing_is_blocked() {
        let jojobot = handler();
        let refused = blocked(
            &jojobot
                .recall(Parameters(of_nothing()))
                .await
                .expect("a malformed query is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");

        jojobot
            .recall(Parameters(RecallArgs {
                kind: Some("person".into()),
                ..of_nothing()
            }))
            .await
            .expect("one filter is enough");
    }

    /// **A type query reaches a thing nobody labelled.**
    ///
    /// A thing answers a type by the keys its records carry, and they carried
    /// them whether or not their writer also named a class. While a key could
    /// only be written beside a label, the set a type query ran over was the
    /// set that opted in, so a type reported the writers who knew about it
    /// rather than the things that answer it.
    #[tokio::test]
    async fn a_type_query_reaches_a_record_written_with_no_label() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        jojobot
            .declare_type(Parameters(DeclareTypeArgs {
                name: "service".into(),
                fields: vec![FieldArgs {
                    key: "odometer".into(),
                    holds: Some("number".into()),
                    folds: None,
                    required: false,
                }],
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("declare_type ok");
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some([("odometer".to_string(), "18000".to_string())].into()),
                ..capture_args("person:alpha", "the chain was replaced")
            },
        )
        .await;

        let found = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    answers_type: Some("service".into()),
                    facts: Some(true),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            found["objects"][0]["id"], "person:alpha",
            "the record answers the type by its key: {found}"
        );
        assert_eq!(
            found["objects"][0]["facts"][0]["content"], "the chain was replaced",
            "{found}"
        );
    }

    /// **A declared reference key is walkable through the wire**, both ways,
    /// and the ordering a declaration licenses narrows what the walk reaches.
    ///
    /// The whole slice through the surface a caller holds: nothing here calls
    /// the resolver, so it fails on a build where the arguments never reach it.
    #[tokio::test]
    async fn a_relation_is_walkable_and_a_walk_can_filter_what_it_reaches() {
        let jojobot = handler();
        for (kind, slug, name) in [
            ("person", "bart", "Bart"),
            ("pet", "santas-little-helper", "Santa's Little Helper"),
            ("pet", "snowball", "Snowball"),
        ] {
            jojobot
                .add_entity(Parameters(add_args(kind, slug, name)))
                .await
                .expect("add_entity ok");
        }
        jojobot
            .declare_type(Parameters(DeclareTypeArgs {
                name: "pet".into(),
                fields: vec![
                    FieldArgs {
                        key: "born".into(),
                        holds: Some("date".into()),
                        folds: None,
                        required: false,
                    },
                    FieldArgs {
                        key: "owner".into(),
                        holds: Some("reference".into()),
                        folds: None,
                        required: false,
                    },
                ],
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("declare_type ok");
        for (slug, born) in [
            ("santas-little-helper", "2019-04-15"),
            ("snowball", "2024-11-02"),
        ] {
            capture_ok(
                &jojobot,
                CaptureArgs {
                    fields: Some(
                        [
                            ("born".to_string(), born.to_string()),
                            ("owner".to_string(), "person:bart".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    ..capture_args(&format!("pet:{slug}"), "one of the pets")
                },
            )
            .await;
        }

        let has_many = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    facts: Some(false),
                    follow: Some(FollowArgs {
                        relation: Some("owner".into()),
                        direction: Some("in".into()),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let reached: Vec<&str> = has_many["objects"][0]["connected"]
            .as_array()
            .expect("the pets hang off the owner")
            .iter()
            .filter_map(|o| o["id"].as_str())
            .collect();
        assert_eq!(
            reached,
            vec!["pet:santas-little-helper", "pet:snowball"],
            "the reverse of a declared reference key is the has-many: {has_many}"
        );
        assert_eq!(
            has_many["objects"][0]["connected"][0]["via"]["relation"], "owner",
            "and a reached object says which relation carried it: {has_many}"
        );

        let older = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    facts: Some(false),
                    follow: Some(FollowArgs {
                        relation: Some("owner".into()),
                        direction: Some("in".into()),
                        keeping: Some(vec![KeyFilterArgs {
                            key: "born".into(),
                            value: Some("2020-01-01".into()),
                            compare: Some("before".into()),
                            scope: None,
                        }]),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("recall ok"),
        );
        let kept: Vec<&str> = older["objects"][0]["connected"]
            .as_array()
            .expect("a filtered walk still answers with a list")
            .iter()
            .filter_map(|o| o["id"].as_str())
            .collect();
        assert_eq!(
            kept,
            vec!["pet:santas-little-helper"],
            "the ordering the declaration licenses narrows what the walk reaches: {older}"
        );
    }

    /// **An ordering no declaration licenses is refused**, and so is a name no
    /// declaration backs, and so is a call naming both link vocabularies at
    /// once. Each comes back blocked with a way forward, rather than as an
    /// answer to a question nobody asked.
    #[tokio::test]
    async fn an_unlicensed_ordering_and_an_unbacked_relation_are_blocked() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "bart", "Bart")))
            .await
            .expect("add_entity ok");

        let unlicensed = blocked(
            &jojobot
                .recall(Parameters(RecallArgs {
                    fields: Some(vec![KeyFilterArgs {
                        key: "born".into(),
                        value: Some("2020-01-01".into()),
                        compare: Some("before".into()),
                        scope: None,
                    }]),
                    ..of_nothing()
                }))
                .await
                .expect("an unlicensed ordering is an answer, not a protocol failure"),
        );
        assert_eq!(unlicensed["wrote"], false, "{unlicensed}");

        // A name no declaration backs. `pet.owner` was a relation name once
        // and is not one now, so this is also the case that pins the rename.
        let unbacked = blocked(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    follow: Some(FollowArgs {
                        relation: Some("pet.owner".into()),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("a name no declaration backs is an answer, not a protocol failure"),
        );
        assert_eq!(unbacked["wrote"], false, "{unbacked}");

        // **Both vocabularies at once, under ONE word.** A key may be spelled
        // like an edge shape, so the refusal has to be legible when the two
        // names are identical rather than merely adjacent.
        let both = blocked(
            &jojobot
                .recall(Parameters(RecallArgs {
                    subject: Some("person:bart".into()),
                    follow: Some(FollowArgs {
                        shape: Some("location".into()),
                        relation: Some("location".into()),
                        ..no_follow()
                    }),
                    ..of_nothing()
                }))
                .await
                .expect("naming both vocabularies is an answer, not a protocol failure"),
        );
        assert_eq!(both["wrote"], false, "{both}");
        let why = both.to_string();
        assert!(
            why.contains("shape") && why.contains("relation"),
            "the refusal says which vocabulary is which, or one word names two things: {both}"
        );

        // The positive the refusals rest on: the same shape of call, naming one
        // vocabulary, is served.
        jojobot
            .recall(Parameters(RecallArgs {
                subject: Some("person:bart".into()),
                follow: Some(FollowArgs {
                    shape: Some("connection".into()),
                    direction: Some("in".into()),
                    ..no_follow()
                }),
                ..of_nothing()
            }))
            .await
            .expect("an edge shape takes a direction");
    }

    /// A `follow` naming nothing — the base the walk cases vary.
    fn no_follow() -> FollowArgs {
        FollowArgs {
            shape: None,
            relation: None,
            direction: None,
            depth: None,
            keeping: None,
            fits_type: None,
        }
    }

    /// A call naming nothing at all — the base every case above varies.
    fn of_nothing() -> RecallArgs {
        RecallArgs {
            subject: None,
            kind: None,
            answers_type: None,
            fields: None,
            facts: None,
            prose: None,
            follow: None,
            overdue: None,
            sid: None,
            history: None,
            history_most: None,
        }
    }
}
