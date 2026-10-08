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
use crate::session::session_declined;
use crate::teaching::{CLAIMS_DOMAIN, CLAIMS_TEACHING};
use jojobot_domain::memory::{entitlement, graph};
use jojobot_domain::text;

/// One key filter of a `recall` — a key, and optionally the value it holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct KeyFilterArgs {
    /// The key, exactly as it is spelled where it was written. Matching is
    /// structural, so nothing has to have declared it.
    ///
    /// ⚠️ **Omit it to ask about the VALUE under any key at all** — *is this
    /// held as a value anywhere*, as opposed to sitting in a claim's prose.
    /// That is the question when you know what was recorded and not where it
    /// went. A filter naming neither a key nor a value asks nothing and comes
    /// back blocked.
    #[serde(default)]
    pub(crate) key: Option<String>,
    /// The value it must hold. **Omit it to ask only that the key is there** —
    /// a different question, and the one to ask when you want everything that
    /// records a thing rather than everything that records it one way.
    #[serde(default)]
    pub(crate) value: Option<String>,
    /// **How the value is compared**, and what a key was DECLARED to hold is
    /// what licenses it. `equals` is the default and needs no declaration.
    /// `before` and `after` need a type declaring the key a `date`; `less` and
    /// `greater` need one declaring it a `number`. Asking for an ordering a
    /// declaration does not license comes back blocked, rather than quietly
    /// answering the equality question instead.
    #[serde(default)]
    pub(crate) compare: Option<String>,
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
    pub(crate) scope: Option<String>,
}

/// The `follow` argument of a `recall` — which edges to walk, and how far.
/// Leave both `shape` and `relation` unset and a walk also reaches a
/// mention (an entity named in a claim's own words), a ref (an entity a
/// claim touches with no claim about how) and a field link (an entity whose
/// handle a record holds as a whole field value, or as every item of a comma
/// list, under any key) — each
/// labelled mention, ref or field, never as an edge. Naming a shape or a
/// relation narrows to that alone: none has one.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct FollowArgs {
    /// Narrow to one shape (`location` · `membership` · `attendance` · `about`
    /// · `connection`). Omit for **any** edge — "whatever it is connected to"
    /// — which is also what reaches a mention, a ref or a field link; naming a shape here
    /// reaches edges of that shape alone.
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// **A declared relation to walk instead of an edge.** A relation is a KEY
    /// that some type declared to hold a `reference`: the declaration says the
    /// value must be an entity of a named kind, and that is what makes the key
    /// walkable by its name. A value that is a handle is a link under any key;
    /// an unscoped walk reaches it either way.
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
    pub(crate) relation: Option<String>,
    /// **Which end of the edge to leave by**, and it is two different
    /// questions. `out` (the default) follows the edges this object's own
    /// records draw — from a guest, the party they are attending. `in` follows
    /// the edges other objects draw AT this one — from the party, its guests.
    #[serde(default)]
    pub(crate) direction: Option<String>,
    /// How many hops: 1 is the neighbours, 2 is the neighbours' neighbours.
    /// Defaults to 1.
    #[serde(default)]
    pub(crate) depth: Option<u32>,
    /// **What the walk keeps of what it reaches.** The same key filters the
    /// selection takes, `scope` and all, applied at every hop rather than to
    /// the roots — so "this person's pets" narrows to "this person's pets born
    /// before a date". Omit to keep everything. An object kept by a `record`
    /// filter arrives carrying the records that answered; one kept by a filter
    /// asked of the thing arrives whole, because the fold answered and no
    /// record had to. An object reached and not kept leaves the object that
    /// points at it marked as having edges nobody followed.
    #[serde(default)]
    pub(crate) keeping: Option<Vec<KeyFilterArgs>>,
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
    pub(crate) fits_type: Option<String>,
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
    pub(crate) subject: Option<String>,
    /// Every entity of one kind. `session` reads your own past runs.
    #[serde(default)]
    pub(crate) kind: Option<String>,
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
    pub(crate) answers_type: Option<String>,
    /// **Which objects**, by a key and the value it holds. Omit a filter's
    /// value to ask only that the key is there.
    ///
    /// ⚠️ **Asked of the THING unless a filter says otherwise**, which is
    /// `scope` on the filter itself. Asked of the thing, a filter is answered
    /// by the FOLD — the newest write of the key — so a value some record of an
    /// object still carries, and the object no longer holds, selects nothing.
    /// `scope: record` asks about the occasions instead, and **every `record`
    /// filter must hold on ONE record**: those describe a single record rather
    /// than separate questions.
    ///
    /// **Several filters asked of the thing must ALL hold on one thing.** The
    /// list is an AND, never an OR: an object comes back only when it holds every
    /// key the filters name, each at its own value. To ask for either of two
    /// values, make two calls.
    #[serde(default)]
    pub(crate) fields: Option<Vec<KeyFilterArgs>>,
    /// Whether each object's records come back — the claims its fields were
    /// written in, each with its own wording, provenance and the address that
    /// edits it.
    ///
    /// **Off by default.** The fields are what a thing HOLDS and they come
    /// back always; the records say the same thing at length, and shipping every one
    /// of them unasked is the cost a caller cannot decline. Ask for them when
    /// you need a claim's own words, where it came from, or its address —
    /// and an answer that left them out says how many there were.
    ///
    /// **A claim written more than once says so.** `revised`/`revision_count`
    /// come back on any record that was rewritten — absent on one that was
    /// not, exactly as `stale` is absent on a fresh claim — because two
    /// claims that were rewritten a different number of times otherwise read
    /// back identical. `history_record` on this same call is the door that
    /// opens onto the earlier wording; the count only says the door is worth
    /// opening.
    #[serde(default)]
    pub(crate) facts: Option<bool>,
    /// **Serve a shape's sources alongside it**, on the same listing of
    /// `facts`. Off by default: a record marked with `stands_for` already
    /// carries its sources' content in its own words, so the listing that
    /// also carries the shape leaves the named sources out and says how many
    /// and how to reach them. Ask for this when you need the sources
    /// themselves — their own wording, provenance or address — rather than
    /// the shape that already speaks for them.
    ///
    /// **Elision applies to the listing, never to a record asked for by
    /// name.** `history_record` on a source's own address still serves it
    /// whole, whatever this argument says.
    #[serde(default)]
    pub(crate) stood_for: Option<bool>,
    /// **Which records `facts` lists: `active` or `archived`.** Left out, every
    /// record comes back whatever its status — going straight to a known thing
    /// is the direct door, and a retired record read that way is correct.
    /// `active` is the records that still stand; the answer says how many
    /// retired ones it left out and which call returns them, so a narrowed read
    /// never passes for the whole of what a thing holds. `archived` is the
    /// other half. It narrows the records of the objects the call selected.
    #[serde(default)]
    pub(crate) status: Option<String>,
    /// Whether each object's **prose** comes back — the human half of its page,
    /// whole. Off by default, because a page is bigger than a claim and shipping
    /// every one of them unasked is a cost the caller cannot decline.
    ///
    /// **For a bot this is the instance's own layer**, which is what somebody
    /// wrote and what `set_charter` replaces. Ask for `charter` instead to read
    /// what that identity actually answers with.
    #[serde(default)]
    pub(crate) prose: Option<bool>,
    /// **A bot's charter, whole** — what that identity answers with.
    ///
    /// **This is how you read a colleague.** Booting as another bot would make
    /// you it, and that is the one act the rules refuse, so the whole charter
    /// is readable here — no session is created and no handle comes back.
    ///
    /// ⚠️ **Do not send a charter you read here back to `set_charter`.** Some
    /// of it may be text the software supplies rather than text anybody wrote,
    /// and a write repeating it comes back blocked. Send what you are writing.
    ///
    /// Objects that are not bots carry no charter at all.
    #[serde(default)]
    pub(crate) charter: Option<bool>,
    /// **Which keys of each object's fields come back, and only those.** Name
    /// the keys you want, and every object in the answer — the ones a walk
    /// reaches included — carries just those of its fields.
    ///
    /// **Eliding is never silent.** An object that holds other keys says so in
    /// `fields_left_out`, naming them, and asking again without `keys` reads
    /// them all. When a view supplied the keys, ask again without the view,
    /// sending its kind, or with `keys` naming the ones left out. A key an
    /// object does not hold is simply not there.
    ///
    /// Omit it and every key comes back, which is the normal read. This
    /// narrows what each object says, never which objects come back, and it
    /// leaves facts, prose and the charter as the arguments for those say.
    /// **A view can carry it** as its `shows_keys`, and what you send beside
    /// the view's name wins.
    #[serde(default)]
    pub(crate) keys: Option<Vec<String>>,
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
    pub(crate) history: Option<String>,
    /// **The writes behind one RECORD, oldest first** — name a record's address
    /// `kind:slug#local-id`, and the object it is filed under comes back
    /// carrying every write of that claim: what it said, what it said
    /// underneath, who backed it and whether it stood, each time it was
    /// written.
    ///
    /// The other half of the same axis. `history` names a KEY and traces a
    /// value across every record that touched it; this names one CLAIM and
    /// traces the claim itself, which is how you read what a record said before
    /// somebody corrected it. Name one or the other, never both.
    ///
    /// Each write says **when it happened** and where it sits in the order.
    /// That moment is not the claim's own first-recorded one, which answers a
    /// different question and is on the claim. A write kept before jojobot
    /// recorded one carries none.
    ///
    /// **The address is selection enough.** Send it on its own and the read
    /// answers about the thing the address names — you do not have to repeat
    /// the subject beside it. Name a `subject` as well and it has to be that
    /// same thing, or the call is refused: two arguments pointing at two
    /// things is a mistake worth hearing about rather than one jojobot picks
    /// between.
    ///
    /// Omit it and no chain comes back, which is the normal read.
    #[serde(default)]
    pub(crate) history_record: Option<String>,
    /// **How many of those writes come back**, newest kept. Twenty when
    /// you do not say.
    ///
    /// A key written a thousand times would otherwise be a thousand entries in
    /// an answer somebody asked one narrow question of. The answer always says
    /// how many writes exist and how many it left out, so raising this is how
    /// you reach the far end — deliberately, rather than by surprise.
    #[serde(default)]
    pub(crate) history_most: Option<u32>,
    /// **Where each folded value came from**, and who backs it: the claim whose
    /// write won the key, its provenance and its standing.
    ///
    /// **Off by default because it is a second read of each object**, and most
    /// reads want the value rather than its pedigree. Ask for it when you are
    /// about to act on a value, or when you need to know whether the user said
    /// it. A key whose writes are summed has no single winning claim and is not
    /// here.
    #[serde(default)]
    pub(crate) backing: Option<bool>,
    /// **The claims worked out from one claim**, by its address
    /// `kind:slug#local-id` — lineage walked from the source's end.
    ///
    /// A claim names what it rests on; this asks the other way, which is the
    /// question somebody has the moment a claim is taken back. **Records of
    /// every status come back**, because a claim that was itself withdrawn is
    /// part of the answer to *what did we build on this*.
    ///
    /// **The address is selection enough.** Send it on its own and the call
    /// answers about the thing the address names — you do not have to repeat
    /// the subject beside it. Name a `subject` as well and it has to be that
    /// same thing, or the call is refused: two arguments pointing at two
    /// things is a mistake worth hearing about rather than one jojobot picks
    /// between.
    #[serde(default)]
    pub(crate) built_on: Option<String>,
    /// **The values already recorded under one key**, across the objects this
    /// call selected, each with how many of them hold it — most used first.
    ///
    /// **A read, never a rule.** It says what is there so a caller can pick a
    /// spelling that is already in use instead of inventing a third one; a
    /// value nobody has used is still written, and nothing here refuses one.
    ///
    /// The values come from what each thing HOLDS — its folded fields — so a
    /// value that was written and later replaced is not in use, and a thing
    /// counts once however many times it was written. **The scope is the
    /// selection**: name a `kind` and you get that kind's values, and a
    /// `fields` filter naming the key alone is how you ask across everything
    /// carrying it.
    ///
    /// A key nobody has written comes back with nothing in use rather than as
    /// a refusal.
    #[serde(default)]
    pub(crate) values: Option<String>,
    /// **How many values come back**, most used kept. Twenty when you do not
    /// say. The answer always says how many exist and how many it left out.
    #[serde(default)]
    pub(crate) values_most: Option<u32>,
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
    ///
    /// **Absence here is not proof a write landed.** This asks whether
    /// something is owed as of the date named — never whether an edit you
    /// just made took effect, and a write can succeed while leaving the very
    /// thing it was meant to change untouched. To confirm an edit worked,
    /// recall the thing by its own handle and read the field back directly;
    /// leaving a filtered list is not the same claim as a field changing.
    ///
    /// **The answer orders what it found, oldest due first, and each object
    /// carries `overdue_by_days`** — how many days past the date named it
    /// fell due, so the order is one you can check rather than one you have
    /// to trust. `null` on the one thing this cannot measure: a due moment
    /// that cannot be read sorts first, loudly, rather than behind a number
    /// that would have to be guessed. Absent entirely when this call asked no
    /// overdue question.
    #[serde(default)]
    pub(crate) overdue: Option<OverdueArgs>,
    /// **What else was recorded around a day.** See [`NearArgs`].
    #[serde(default)]
    pub(crate) near: Option<NearArgs>,
    /// Which edges to walk. Omit to walk none, and the answer is flat.
    #[serde(default)]
    pub(crate) follow: Option<FollowArgs>,
    /// **A question asked by name** — a view's handle, or its bare slug.
    ///
    /// A view is a record that holds a query, so asking for one is naming it.
    /// **Some ship with the software and you can declare your own**, with
    /// `add_entity` of kind `view` and the keys `selects`, `shows` and `asks`.
    /// Both are records of the same shape, so nothing about the answer depends
    /// on where the view came from.
    ///
    /// What the view says is what this call asks; anything you send beside it
    /// is yours and wins, so a view is a starting point rather than a cage.
    ///
    /// A name that is no view comes back blocked, with the views that exist.
    #[serde(default)]
    pub(crate) view: Option<String>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
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
    pub(crate) as_of: Option<String>,
}

/// **How far either side counts as near when a caller says nothing.** A week:
/// long enough that a day's neighbours are what a person would call the same
/// stretch, short enough that the answer is still a neighbourhood.
const NEAR_WINDOW: u32 = 7;

/// **How many candidates the search-routed path asks for.** A ranked search
/// caps its answer at a page a person reads; this is not a ranked answer, it
/// is an existence question over every match a structural filter finds, so it
/// asks for far more than [`DEFAULT_LIMIT`] — large enough that a personal
/// graph's carrier hits never silently truncate, while still a real bound
/// rather than none.
const CARRIER_CANDIDATES_LIMIT: usize = 10_000;

/// **Whether the type-only search path's own ceiling was reached.** A count
/// at the ceiling cannot be told apart from a store that holds exactly that
/// many things and no more — but treating it as capped is the caller-safe
/// read: the wrong guess costs nothing, and the other one lets a truncated
/// answer read as complete.
fn hit_candidate_ceiling(hit_count: usize, limit: usize) -> bool {
    hit_count >= limit
}

/// Which clock a neighbourhood read compares.
/// **What an object owes, asked of its kind too**: a finished work item or
/// project owes nothing — see [`attention::owed_as`].
fn owed_by(carriers: &[&dyn attention::Carrier], object: &graph::Object) -> attention::Due {
    attention::owed_as(object.entity.kind.as_token(), carriers, &object.fields)
}

fn parse_clock(raw: Option<&str>) -> Result<graph::Clock, McpError> {
    match raw.map(str::trim) {
        None | Some("") | Some("recorded_on") => Ok(graph::Clock::RecordedOn),
        Some("taken_in") => Ok(graph::Clock::TakenIn),
        Some("happened_at") => Ok(graph::Clock::HappenedAt),
        Some(other) => Err(McpError::invalid_params(
            format!("clock must be recorded_on, taken_in or happened_at, got '{other}'"),
            None,
        )),
    }
}

/// **The neighbourhood question of a `recall`** — what else was recorded
/// around a day.
///
/// A sub-object rather than flat arguments, for the same reason `overdue` is
/// one: a window and a clock mean nothing unless a day is named, and three
/// flat arguments would admit a call that names a clock and filters nothing.
///
/// ⛔️ **Not a query language and not inference.** A window over dates the
/// store already holds.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct NearArgs {
    /// **The day to read around**, `YYYY-MM-DD`. Omit it and jojobot uses
    /// today, and the answer says which day it used.
    #[serde(default)]
    pub(crate) day: Option<String>,
    /// **How many days either side count as near.** Symmetric, because what
    /// came just before a day and just after it are equally what surrounded
    /// it. Omit it for a week.
    #[serde(default)]
    pub(crate) within_days: Option<u32>,
    /// **Which clock to compare**: `recorded_on` — the day the claim was made,
    /// the default — `taken_in`, the day jojobot took the record in, or
    /// `happened_at`, the day the thing itself happened.
    ///
    /// ⚠️ **They answer different questions.** *What did Milhouse say back in
    /// August* asks the first. *What was filed that week* asks the second.
    /// *What was going on around then* asks the third — the day on the claim,
    /// not the day it was said or filed.
    ///
    /// 🚨 **A record with nothing on the chosen clock carries none**, so a
    /// read on that clock cannot place it. Those are counted in
    /// `near_unplaced` rather than dropped: an answer that shrank silently
    /// would be indistinguishable from a day with nothing around it. Every
    /// record carries a recorded day, so only `taken_in` and `happened_at`
    /// can be unplaceable.
    #[serde(default)]
    pub(crate) clock: Option<String>,
}

/// The key filters of a call, wherever they sit: a selection describes the
/// roots and a walk's describe what it reaches, and they are the same filter.
fn key_filters(args: &[KeyFilterArgs]) -> Result<Vec<graph::FieldFilter>, McpError> {
    args.iter()
        .map(|f| {
            Ok(graph::FieldFilter {
                key: f.key.as_ref().map(|k| k.trim().to_string()),
                value: f.value.as_ref().map(|v| v.trim().to_string()),
                compare: parse_compare(f.compare.as_deref())?,
                scope: parse_scope(f.scope.as_deref())?,
            })
        })
        .collect()
}

/// **What is held that points at one thing, ranked and inside a budget.**
///
/// The read is targeted — the store answers who points at a handle without
/// reading every entity — and the ranking is the domain's, so what a reader is
/// told first is a claim with its own cases rather than an accident of how this
/// query was written.
fn held_json(
    held: &[entitlement::Held<'_>],
    as_of: jiff::civil::Date,
    widen: entitlement::Widen,
) -> serde_json::Value {
    let rendered: Vec<serde_json::Value> = held.iter().map(held_item).collect();
    let kept = text::HELD_CONTEXT.head(&rendered, |item| item.to_string().chars().count());
    let mut body = serde_json::json!({
        "as_of": as_of.to_string(),
        "count": held.len(),
        "left_out": kept.omitted(),
        "items": kept.kept(),
    });
    // **The two silences are different answers.** Nothing points here at all,
    // and nothing gets anybody in but other things point here, are what a
    // reader acts on differently — and the failure this whole read exists to
    // end was a silence nobody could tell apart from an absence.
    let note = if held.is_empty() && widen == entitlement::Widen::Never {
        // **Empty here is a choice this call made**, and saying "nothing points
        // at this" would be a claim nobody checked.
        Some(
            "this answer lists no record that mentions this thing, because this call follows \
             links of its own: recall the handle on its own to see the records that mention it",
        )
    } else if held.is_empty() {
        Some(
            "nothing points at this, so there is nothing anybody holds for it and nothing else \
             naming it",
        )
    } else if held
        .iter()
        .all(|h| h.standing == entitlement::Standing::Related)
    {
        Some(
            "these records mention this thing through a key other than `admits`: they link to it \
             and say nothing about who may use it",
        )
    } else if kept.omitted() > 0 {
        Some(
            "the rest did not fit this answer's budget: recall this handle with follow \
             {relation: \"admits\", direction: \"in\"} for all of them",
        )
    } else {
        None
    };
    if let Some(note) = note {
        body["note"] = note.into();
    }
    body
}

/// One record that points here, with what a reader acts on: who holds it, how
/// it stands, and **what backs it** — a pass somebody said they hold and one an
/// assistant inferred are not the same claim, and the consequence of reading
/// the second as the first is somebody turned away at a door.
fn held_item(held: &entitlement::Held<'_>) -> serde_json::Value {
    let field = |key: &str| held.fact.fields.get(key).cloned();
    let mut item = serde_json::json!({
        "holder": held.fact.subject.to_string(),
        // **Whether this record is in force on the day asked about**, which is
        // a different question from how sure anybody was — and it is spelled
        // differently for that reason. The two shared the word `standing` and
        // arrived in one body, so a reader had no way to tell which question an
        // answer was answering.
        //
        // `related` is not a weaker `live`: it is the answer that nothing
        // admits anybody at all, and this record points at the thing through
        // some other declared key. Said here because inferring that from the
        // token is how a reader gets it wrong.
        "liveness": match held.standing {
            entitlement::Standing::Live => "live",
            entitlement::Standing::Lapsed => "lapsed",
            entitlement::Standing::Related => "related",
        },
        "claim": held.fact.content,
        "provenance": held.fact.provenance.as_token(),
        // **How sure the operator was.** A pass somebody THINKS they hold and
        // one they confirmed are the same sentence otherwise — and this block is what a session reads to tell
        // them they are covered, so a lost hedge becomes a fact at the moment
        // somebody acts on it.
        "standing": held.fact.standing.as_token(),
        "address": held.fact.address().to_string(),
    });
    for key in [
        entitlement::ADMITS,
        entitlement::VALID_FROM,
        entitlement::VALID_UNTIL,
        "tier",
    ] {
        if let Some(value) = field(key) {
            item[key] = value.into();
        }
    }
    item
}

/// **The values one key already holds, on the wire.**
///
/// `distinct` is how many there are and `left_out` how many this answer does
/// not carry, so a caller who has to reach the far end knows there is one and
/// which argument gets there (rule 106).
fn values_json(objects: &[graph::Object], key: &str, most: usize) -> serde_json::Value {
    let in_use = graph::values_in_use(objects, key);
    let shown = in_use.len().min(most);
    let left_out = in_use.len() - shown;
    let mut body = serde_json::json!({
        "key": key.trim(),
        "distinct": in_use.len(),
        "left_out": left_out,
        "in_use": in_use
            .iter()
            .take(shown)
            .map(|v| serde_json::json!({ "value": v.value, "things": v.things }))
            .collect::<Vec<_>>(),
    });
    let note = if in_use.is_empty() {
        "nothing here holds that key yet, which is an answer rather than a refusal: write the \
         value you meant and it becomes the first one in use"
    } else if left_out > 0 {
        "the most used are here and the rest are not: raise values_most to reach them"
    } else {
        return body;
    };
    body["note"] = note.into();
    body
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
                "recorded_at": write.recorded_at.to_string(),
                "status": write.status.as_token(),
                // **Who backed the claim this write arrived in, and how sure
                // anyone was.** A value the user stated and one an assistant
                // worked out are the same string, and a reader that cannot see
                // the difference has to treat both alike.
                "provenance": write.provenance.as_token(),
                "standing": write.standing.as_token(),
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

/// **One claim's writes on the wire, oldest first, with how many there are.**
///
/// Each write carries the whole of what the claim said then, its place in the
/// order, and **when it happened** — which is not when the claim first entered
/// the store. A claim taken in last April and corrected in September has one
/// first-recorded moment and two writes months apart.
///
/// A write kept before the substrate recorded a moment carries none, and it is
/// absent rather than filled from the claim: a copied moment would report that
/// year of corrections as one instant.
fn record_history_json(history: &graph::ClaimHistory) -> serde_json::Value {
    let mut body = serde_json::json!({
        "record": history.record.to_string(),
        "count": history.total,
        "shown": history.writes.len(),
        "writes": history.writes.iter().map(|write| {
            let mut rendered = serde_json::json!({
                "nth": write.ordinal,
                // **When this write happened**, and absent for one kept before
                // the substrate recorded it — never the claim's own moment
                // standing in for it.
                "written_at": write.written_at.map(|at| at.to_string()),
                "content": write.content,
                "recorded_at": write.recorded_at.to_string(),
                // **What this write said about when the thing happened.**
                // Versioned like everything else the write carries, so a claim
                // that gained a day in a later edit reads apart from one that
                // always had it — and a guessed day taken back leaves a trace.
                "happened_at": write.happened_at.map(|day| day.to_string()),
                "status": write.status.as_token(),
                "provenance": write.provenance.as_token(),
                "standing": write.standing.as_token(),
            });
            if let Some(fields) = rendered.as_object_mut() {
                if let Some(details) = &write.details {
                    fields.insert("details".into(), details.as_str().into());
                }
                if let Some(edge) = &write.edge {
                    fields.insert(
                        "edge".into(),
                        serde_json::json!({
                            "type": edge.shape.as_name(),
                            "object": edge.object.to_string(),
                        }),
                    );
                }
                if let Some(source) = &write.derived_from {
                    fields.insert("derived_from".into(), source.to_string().into());
                }
                if let Some(day) = &write.stale_after {
                    fields.insert("stale_after".into(), day.to_string().into());
                }
            }
            rendered
        }).collect::<Vec<_>>(),
    });
    // **A cut says so, and says the way past itself** — the same rule the key
    // history keeps.
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

impl Jojobot {
    /// **Read a view and fill the call it was named in.**
    ///
    /// A view is a record, so this is an ordinary read of its keys — and it is
    /// the ONE path, whether the record came from the store or from what the
    /// build supplies. That is what keeps a shipped view and the operator's own
    /// from needing two mechanisms (rule 106).
    ///
    /// **The caller's own arguments win.** A view says what to ask; a caller
    /// that also named a kind meant that kind.
    async fn asked_by_name(
        &self,
        named: &str,
        args: RecallArgs,
    ) -> Result<(RecallArgs, Vec<graph::FieldFilter>), Box<CallToolResult>> {
        let handle = match named.trim().split_once(':') {
            Some((kind, _)) if kind == EntityKind::VIEW.as_token() => {
                EntityId(named.trim().to_string())
            }
            _ => EntityId::new(EntityKind::VIEW, named.trim()),
        };
        let held = self.memory.fields(&handle).await;
        let held = match held {
            Ok(held) if !held.is_empty() => held,
            // **No view under that name.** The candidates are the views that
            // exist, which is what a caller who guessed a name needs — and it
            // costs one read they were about to make anyway.
            _ => {
                let known = self
                    .memory
                    .list_entities(Some(EntityKind::VIEW))
                    .await
                    .unwrap_or_default();
                // **Every view, named in the sentence rather than as
                // candidates.** A candidate carries a reason the guard flagged
                // it, and none of these was flagged: they are simply what there
                // is. Putting them in the list would mean stating a match that
                // was never made.
                //
                // A similarity screen would be worse still — it answers a wild
                // guess with nothing, which reads as "there are no views".
                let names = known
                    .iter()
                    .map(|e| e.id.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(Box::new(blocked_body(
                    &handle,
                    &[],
                    match names.is_empty() {
                        true => format!(
                            "Nothing was read: no view is named '{}', and there are no views \
                             here yet. Declare one with add_entity of kind view.",
                            handle.as_str(),
                        ),
                        false => format!(
                            "Nothing was read: no view is named '{}'. The views here are: \
                             {names}. Ask for one of those, or declare your own with \
                             add_entity of kind view.",
                            handle.as_str(),
                        ),
                    },
                )));
            }
        };
        // **The view's own keys and its own records, read where the page
        // reads them** (rule 51). A filter is a record on the view, so
        // reading the view's question needs its facts beside its fields —
        // degrading to none on a read failure, the same way the candidate
        // list just above does, rather than failing a view whose fields
        // already answered.
        let facts = self.memory.recall(&handle).await.unwrap_or_default();
        let asked = graph::asked_by_view(&held, &facts);
        // **The caller's own `fields` wins whole, never merged item by
        // item.** A caller that named a filter meant that filter; the
        // view's own is a starting point, not something to add to.
        let view_filters = if args.fields.is_none() {
            asked.filters.clone()
        } else {
            Vec::new()
        };
        Ok((
            RecallArgs {
                kind: args.kind.or(asked.selects),
                facts: args.facts.or(asked.facts.then_some(true)),
                prose: args.prose.or(asked.prose.then_some(true)),
                charter: args.charter.or(asked.charter.then_some(true)),
                overdue: args
                    .overdue
                    .or_else(|| asked.overdue.then_some(OverdueArgs { as_of: None })),
                answers_type: args.answers_type.or(asked.answers_type),
                keys: args.keys.or(asked.keys),
                follow: args.follow.or_else(|| {
                    asked.follow.map(|f| FollowArgs {
                        shape: f.shape,
                        relation: f.relation,
                        direction: f.direction,
                        depth: f.depth,
                        keeping: None,
                        fits_type: f.fits_type,
                    })
                }),
                ..args
            },
            view_filters,
        ))
    }
}

/// **The charter a bot answers with** — its prose, read.
///
/// What the build supplies is already in the prose by the time it gets here,
/// resolved underneath this verb, so this only decides which key the caller
/// asked for it under. A caller that asked for the charter and not the page gets
/// the page taken back out: it is inside what they were given, and shipping it
/// twice is a cost they did not ask for.
fn charter_json(
    rendered: &mut serde_json::Value,
    object: &graph::Object,
    want_charter: bool,
    asked_prose: bool,
    elided_bot_has_charter: bool,
) {
    if object.entity.id.kind() != Some(EntityKind::BOT) {
        return;
    }
    let Some(fields) = rendered.as_object_mut() else {
        return;
    };
    // **Eliding is never silent, and charter was the exception.** A charter
    // can run to thousands of characters, so a reader who did not ask for one
    // needs to be told there was something left out, and how to reach it —
    // the same house style as `records`, `older` and `fields_backing_note`.
    //
    // **But only when there is something behind the note.** A bot with no
    // charter set has nothing to elide, and telling it to "ask again" would
    // send a reader back for an answer that is still empty. `object.prose`
    // cannot answer this here: it is only populated when the caller asked for
    // it, which is exactly the branch this is not — the caller's answer for
    // this comes in as `elided_bot_has_charter`, looked up separately.
    if !want_charter {
        fields.insert("charter_elided".into(), elided_bot_has_charter.into());
        if elided_bot_has_charter {
            fields.insert(
                "charter_note".into(),
                "the charter is not here — recall again naming this handle with charter: true \
                 to read it"
                    .into(),
            );
        }
        return;
    }
    fields.insert("charter_elided".into(), false.into());
    fields.insert(
        "charter".into(),
        match object
            .prose
            .as_deref()
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            Some(charter) => charter.into(),
            None => serde_json::Value::Null,
        },
    );
    if !asked_prose {
        fields.remove("prose");
    }
}

/// **The keys an answer is narrowed to, and where they came from.** The source
/// decides what an object's note tells a reader to do: keys the caller named are
/// undone by asking again without them, and keys a view supplied are not,
/// because the view supplies them again.
#[derive(Clone, Copy)]
struct KeyNarrowing<'a> {
    keys: &'a [String],
    by_view: bool,
}

/// One object on the wire, and everything it reached.
///
/// **Absence means "not asked for"** on both halves that can be turned off:
/// `facts` and `prose` are missing when the caller declined them, rather than
/// rendered empty, so nothing has to tell an empty page from a page nobody
/// wanted.
fn object_json(
    object: &graph::Object,
    include: graph::Include,
    as_of: jiff::civil::Date,
    only: Option<KeyNarrowing<'_>>,
) -> serde_json::Value {
    // **A folded handle is not a thing, and it is never served as an empty
    // one.** It goes on answering because a handle somebody wrote down must
    // still resolve, so the answer is a status naming the survivor and carries
    // none of the keys that would read as a thing nobody wrote about. It does
    // NOT answer as the survivor: whether a read should resolve silently is
    // not decided here, and this answer is correct either way (rule 261).
    if let Some(survivor) = &object.entity.merged_into {
        let mut body = entity_json(&object.entity);
        if let Some(map) = body.as_object_mut() {
            map.insert("status".into(), "merged".into());
            map.insert("merged_into".into(), survivor.as_str().into());
            map.insert(
                "how_to_proceed".into(),
                format!(
                    "{} was folded into {survivor} and holds nothing of its own. Recall \
                     {survivor} to read everything that was recorded about it, including the \
                     account of the merge.",
                    object.entity.id.as_str(),
                    survivor = survivor.as_str(),
                )
                .into(),
            );
        }
        return body;
    }
    let mut body = entity_json(&object.entity);
    let Some(fields) = body.as_object_mut() else {
        return body;
    };
    // **What the thing IS, in one row.** Always here: every write on it folded,
    // one value per key, the newest write of that key winning. It is the answer
    // to the question a caller usually has, and it is a fraction of the size of
    // the records those writes arrived in.
    //
    // **Narrowed to the keys the call named, when it named any**, and the same
    // narrowing reaches every object a walk brought, so a caller reading an org
    // chart is not handed the operational keys of each node. **What it left out
    // is named**, never dropped silently: a reader who had to infer
    // narrowed-from-empty would infer wrong.
    let wanted = |key: &str| only.is_none_or(|narrowed| narrowed.keys.iter().any(|k| k == key));
    fields.insert(
        "fields".into(),
        object
            .fields
            .iter()
            .filter(|(key, _)| wanted(key))
            .map(|(key, value)| (key.clone(), serde_json::Value::from(value.as_str())))
            .collect::<serde_json::Map<_, _>>()
            .into(),
    );
    let left_out: Vec<&str> = object
        .fields
        .keys()
        .map(String::as_str)
        .filter(|key| !wanted(key))
        .collect();
    if !left_out.is_empty() {
        // **The way back depends on who narrowed.** Keys the caller named come
        // back by asking again without them. Keys a view supplied do not: the
        // view supplies them again, so the advice would loop. The note names
        // the two calls that do return them.
        let way_back = if only.is_some_and(|narrowed| narrowed.by_view) {
            "the view named the ones it shows; ask again without view, sending its kind \
             yourself, or with keys naming these"
        } else {
            "ask again without keys to read them all"
        };
        fields.insert(
            "fields_left_out".into(),
            format!(
                "{} of this object's keys are not here: {} — {way_back}",
                left_out.len(),
                left_out.join(", ")
            )
            .into(),
        );
    }
    // **Eliding is never silent.** Without the note an object carrying no
    // `facts` key says both "you did not ask for them" and "there is nothing
    // recorded here", and a reader who has to infer which will infer wrong.
    if include.facts {
        fields.insert(
            "facts".into(),
            object
                .facts
                .iter()
                .map(|fact| fact_json(fact, as_of, object.fact_revisions.get(&fact.id).copied()))
                .collect(),
        );
        // **Eliding is never silent, here either.** A shape's sources stay out
        // of the listing that also carries the shape — the shape already
        // speaks for them — and the note says how many and how to reach them,
        // the same idiom `records` above uses for the whole listing.
        if object.facts_folded > 0 {
            fields.insert(
                "stood_for".into(),
                format!(
                    "{} records stand behind a shape above and are not here — ask again with \
                     stood_for: true to read them",
                    object.facts_folded
                )
                .into(),
            );
        }
        // **A `record`-scoped filter is a caller's own choice about which
        // records answer, never a claim about how many exist.** `facts_held`
        // is the true total; anything past what `facts` and `stood_for`
        // already account for is a record this scope left out, and that is
        // named here rather than read as "this thing only ever held the
        // ones shown".
        let scoped_out = object
            .facts_held
            .saturating_sub(object.facts.len() + object.facts_folded);
        if scoped_out > 0 {
            fields.insert(
                "scoped_out".into(),
                format!(
                    "{scoped_out} more records exist on this thing and do not answer the \
                     record filter — ask again with a broader one, or none, to read them"
                )
                .into(),
            );
        }
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
    // **The writes behind the record the call named**, on the one object it is
    // filed under. Under its own key rather than beside the key history: they
    // are different questions and a reader should not have to look at the value
    // to find out which was asked.
    if let Some(history) = object.record_history.as_ref() {
        fields.insert("record_history".into(), record_history_json(history));
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
            // **Never rendered as an edge.** `type` is the edge's own key —
            // a schema.org predicate — so a mention or a ref gets a key of
            // its own rather than borrowing one a reader would read as a
            // deliberate link somebody drew on purpose.
            graph::Link::Mention => serde_json::json!({ "mention": true }),
            graph::Link::Ref => serde_json::json!({ "ref": true }),
            graph::Link::Field => serde_json::json!({ "field": true }),
        };
        let mut link = link;
        if let Some(fields) = link.as_object_mut() {
            fields.insert("direction".into(), via.direction.as_token().into());
            // **A link nobody stands behind is marked, never dropped.** Hiding
            // it would make a claim somebody took back and a claim nobody ever
            // made the same answer, which is the one thing a reader here has to
            // be able to tell apart. Present only when it says something, for
            // the reason `unwalked` is: a marker on every ordinary link is a
            // key a reader learns to skip.
            if via.retracted {
                fields.insert(
                    "retracted".into(),
                    "the claim drawing this link is archived now — taken back outright, or \
                     replaced by one that changed what it said; the record's own note says \
                     which. The link is here so it can be told from one nobody ever drew. A \
                     replacement, if there is one, is an ordinary fact naming this one as \
                     derived_from."
                        .into(),
                );
            }
        }
        fields.insert("via".into(), link);
    }
    // **Bounded, and honest about it, the same as every other list on this
    // verb.** A hop with no cap at all is the exact overload a walk exists to
    // prevent: a subject with a wide inbound fan-in would otherwise answer
    // with every one of them, however many that is. Rendered first, then
    // capped by the rendered size — the same order `held_json` already
    // reads in, and the only order that costs the object it drops nothing
    // beyond leaving it out. **The kept order is the walk's own order, not a
    // ranking** — nothing here decides which neighbours matter more, only
    // how many fit.
    let rendered: Vec<serde_json::Value> = object
        .connected
        .iter()
        .map(|o| object_json(o, include, as_of, only))
        .collect();
    let kept = text::CONNECTED_CONTEXT.head(&rendered, |item| item.to_string().chars().count());
    fields.insert("connected".into(), kept.kept().to_vec().into());
    if kept.omitted() > 0 {
        fields.insert(
            "connected_left_out".into(),
            format!(
                "{} more objects are connected here and are not in this answer, in the order \
                 the walk reached them rather than any ranking — recall this handle again, or \
                 narrow the walk with keeping, to reach the rest",
                kept.omitted()
            )
            .into(),
        );
    }
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

/// The future [`Jojobot::fill_session_prose`] returns — named rather than
/// spelled out inline, per `clippy::type_complexity` on this shape.
type FillSessionProseFuture<'a> = std::pin::Pin<
    Box<dyn std::future::Future<Output = Option<Result<CallToolResult, McpError>>> + Send + 'a>,
>;

/// The graph query — objects, what is on them, and what they reach.
#[tool_router(router = recall_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "The graph query: say the shape you want and get that shape back. Use it \
                       when you can describe what you are after — a handle, a kind, a type, a key \
                       and its value — and use search when you are looking for something and only \
                       have words for it. \
                       YOUR OWN PAST RUNS ARE A KIND: kind `session` reads this identity's own \
                       past runs and no other's, and prose: true brings back a run's chronology \
                       — list_runs gives each run's state and focus line, and this is the read \
                       for what it did. THE FOCUS A RUN CLOSED ON IS NOT IN THAT PROSE: \
                       wrap_session's closing entry carries it as `closing_focus`. A selection \
                       that reaches another identity's runs counts \
                       them as `withheld` and shows none of them, so an empty list with \
                       withheld above zero means runs exist that are not yours, and an empty \
                       list with withheld at zero means none exist. A call with no sid owns \
                       no run: boot with start_here and pass the sid it returns to read your \
                       own. \
                       START WITH A VIEW IF ONE FITS: a view is a question somebody already \
                       worked out, asked for by NAME — `view: \"colleagues\"` for the identities \
                       here and what each is for, `view: \"loops\"` for the recurring things and \
                       what each last recorded. SOME SHIP WITH THE SOFTWARE AND YOU CAN DECLARE \
                       YOUR OWN, with add_entity of kind view and the keys selects, shows and \
                       asks; a name that is no view comes back blocked naming the ones that are, \
                       which is how you find out what is here. Anything you send beside the name \
                       wins, so a view is a starting point rather than a cage — and you never \
                       have to work the shape out first to get an answer. \
                       Three axes, and they COMBINE into one question rather \
                       than three. WHICH OBJECTS: subject (one handle), kind, answers_type \
                       (structural — an OBJECT answers a type by the keys every write on it \
                       leaves standing, whether or not anybody declared it one, and carrying \
                       SOME of them is enough: the answer says which it lacks), and fields (a \
                       key, and the value it holds; omit the value to ask only that the key is \
                       there). WHAT OF EACH: every object always comes back as its FIELDS — \
                       every write on it folded, one value per key, the newest write of that key \
                       winning, and a write that takes a key off takes it off the thing. A LOOP \
                       HOLDING A SNOOZE DAY carries `snooze` {until, in_force} beside its \
                       fields: in_force is true only while the loop's last check-in is that \
                       snooze, and after a ran or skipped check-in the day stays in the \
                       fields as history. Those \
                       fields are what the thing HOLDS — its KIND is what it \
                       IS — and they answer most questions; the records behind them are bigger and say the same thing at \
                       length. To see only some of the keys, name them in keys: every object, \
                       the ones a walk reaches included, then carries just those, an object \
                       that held others names them in fields_left_out, and asking again \
                       without keys reads them all, unless a view supplied the keys: then ask \
                       again without the view, sending its kind, or with keys naming them. A \
                       view carries it as shows_keys, which is \
                       how view colleagues shows each bot's one_liner and reports_to. Ask for \
                       facts when you need a claim's own wording, its \
                       provenance, or the address that edits it, and the answer says how many \
                       records it left out when you did not. A RECORD MARKED stands_for ANOTHER \
                       (a synthesis, over update_fact) is served, and the records it stands for \
                       are left out of that same facts listing — the answer says how many and \
                       names stood_for as the way to read them back; ask for stood_for: true when \
                       you need those sources themselves rather than the shape that already \
                       speaks for them. Then prose, off by default, which is the human half of the \
                       object's page, whole; charter, which is how you read a COLLEAGUE — a \
                       bot's charter whole, and no session is made and no handle comes back, \
                       because booting as another bot to read it is the one act the rules \
                       refuse; and history, which names ONE KEY and brings back \
                       every write of it, oldest first. A read is current truth — one value per \
                       key, the newest write — and history is the other question the same data \
                       answers: every time that key was written, with the record each write \
                       arrived in and its date. THE COUNT OF THE WRITES IS THE ANSWER TO HOW \
                       MANY TIMES, so a key a sitting records once each time something happens \
                       is how you count occurrences. A key nobody wrote comes back as no \
                       writes, never as a refusal. HISTORY_RECORD is the other half of that \
                       axis: it names ONE RECORD by its address and brings back every write of \
                       THAT CLAIM, oldest first — what it said, who backed it and whether it \
                       stood, each time somebody wrote it. That is how you read what a claim \
                       used to say before it was corrected, and how you tell a claim nobody ever \
                       made from one somebody made and got wrong. Each write says WHEN IT \
                       HAPPENED and where it sits in the order — and that moment is not the \
                       claim's own first-recorded one, which answers a different question. A \
                       write kept before jojobot recorded one carries none. Name a key or a \
                       record, never both. AN ADDRESS IS SELECTION ENOUGH: history_record on \
                       its own answers about the thing its address names, so you do not repeat \
                       the subject beside it — and naming a subject that is something else is \
                       refused rather than guessed at. A long history comes back CUT to its newest \
                       twenty, saying how many exist and how many it left out; history_most \
                       raises the window when you really want the far end, for either half. VALUES names a key \
                       and answers with the values the selected objects already hold under it, \
                       most used first, each with how many hold it — what to write in a key \
                       like colour or status when you want the spelling everything else \
                       uses. IT IS A READ AND NEVER A RULE: a value nobody has used is written \
                       exactly as any other, and this is how you see the list before adding to \
                       it. The scope is whatever the call selected, so a `fields` filter naming \
                       the key alone asks it of everything carrying the key. A key nobody has \
                       written comes back with nothing in use rather than as a refusal, and a \
                       long list is cut to its most used twenty with values_most to reach the \
                       rest. BACKING answers who stands behind each value: name it and every \
                       object also carries, per key, the claim whose write won that key and \
                       that claim's own provenance and standing — because a value the user \
                       stated and one an assistant worked out are the same string otherwise. It \
                       is off by default because it is a second read; ask for it when you are \
                       about to act on a value. A key whose writes are summed has no single \
                       winning claim and is not there. WHAT SOMEBODY HOLDS COMES BACK UNASKED: every object carries a held \
                       block naming the records that point at it, best first — what admits \
                       somebody to it and is good on the day, then what admits them and has \
                       lapsed, and, only when nothing admits anybody at all, whatever else \
                       points here through a declared key. Each one says who holds it, whether \
                       it is in force on the day (liveness: live, lapsed, or related when \
                       nothing admits anybody at all), what backs it, and HOW SURE THE OPERATOR \
                       WAS (standing) — because a claim somebody made and a claim an assistant \
                       worked out are not the same claim to act on, and neither are a pass \
                       somebody confirmed and one they only think they hold. It says nothing \
                       about whether anybody may go: that is a judgement and jojobot makes \
                       none. Nothing pointing here at all and nothing gating it are different \
                       answers and the block says which. It is bounded, and when it does not \
                       fit it names the walk that returns the rest. A BOT'S OWN THOUGHTS AGE \
                       QUIETLY, AND FACTS SAYS SO UNASKED: asking facts of a bot carrying \
                       thought_capacity (capture's connection edges drawn on the bot's own \
                       handle — see capture's shape) adds a `room` block naming the capacity, how \
                       many count against it now, and how many have gone quiet from being untouched \
                       too long — still there, marked aged_out on the fact itself, excluded from \
                       the count rather than hidden. THE BLOCK NAMES THE THRESHOLD: \
                       ages_after_runs is how many of the bot's runs a thought may go untouched, \
                       and ages_after_from says whether the bot's own thought_ages_after_runs \
                       setting gave it or the default did. THE BLOCK IS SENT ONLY WHEN AT LEAST ONE \
                       THOUGHT HAS GONE QUIET: no block means none has, and the room is \
                       exactly as full as its live thoughts. A write that would go over the \
                       capacity is refused the same way; this is the same room, read rather \
                       than written. \
                       WHICH EDGES: follow {shape, direction, depth}, and \
                       THE ANSWER NESTS — a walked object carries the objects it reached, each \
                       carrying its own. EACH HOP IS BOUNDED: a wide fan-in comes back cut to \
                       what fits, in the order the walk reached it rather than any ranking, and \
                       connected_left_out on the object above says how many did not fit — recall \
                       that handle again, or narrow the walk with keeping, to reach the rest. \
                       NARROW A WALK WITH follow.fits_type, which is the \
                       stricter half of the pair: answers_type selects objects carrying SOME of \
                       a type's keys and reports the gaps, fits_type keeps only the ones \
                       carrying EVERY key. Which of these are described like a pet, versus which \
                       of these ARE pets. Direction is two different questions: `out` follows the \
                       edges this object's records draw, `in` follows the edges drawn AT it, so \
                       from a party `in` reaches its guests and from a guest `out` reaches the \
                       party. A RELATION is the other kind of link, and a DECLARATION is what \
                       makes one: a KEY some type declared to hold a `reference` points at \
                       another entity, so it is walkable. LEAVE shape AND relation BOTH UNSET \
                       and a walk also reaches a MENTION (an entity named in a claim's own \
                       words), a REF (an entity a claim touches with no claim about how) and a \
                       FIELD LINK (an entity whose handle a record holds as a whole field value, \
                       or as every item of a comma list, under any key, declared or not), each labelled mention, ref or field \
                       rather than as an edge — name a shape or a relation and none answers, \
                       because none has one. Name the key and \
                       use `direction` — \
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
                       per object. Unlike search this returns claims of EVERY status, archived \
                       included, unless you pass `status`. A named subject always comes back, even when the filters keep \
                       none of its records — naming a handle asks for that object, a filter asks \
                       which objects — while a handle that names nothing comes back blocked with \
                       the nearest handles, never as an empty answer. An object that draws edges \
                       the walk did not follow says so; an empty `connected` with no such note is \
                       an object with nothing beyond it. A query that narrows nothing is refused: \
                       name at least one of subject, kind, answers_type or fields. AN \
                       answers_type THAT SELECTS NOTHING STILL SAYS WHAT THE TYPE IS: the \
                       answer carries type_keys, the type's keys and what each holds, so the \
                       spelling to write under it is in the empty answer."
    )]
    pub(crate) async fn recall(
        &self,
        Parameters(args): Parameters<RecallArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Resolved before the read runs — see [`Jojobot::attributable`]. This
        // verb publishes a `sid` and says it is what tells jojobot who is
        // asking, so a handle that addresses nothing is refused rather than
        // dropped.
        // **And the identity it resolves is KEPT**, because it is what says
        // which owned objects this read may reach. Deriving it here rather than
        // taking it as an argument is the whole of that rule: a caller cannot
        // ask for somebody else's, because there is nowhere to say so.
        let caller = match self.caller(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let asked_by = caller.as_ref().map(|caller| caller.bot.clone());
        // **A view fills the call in before anything reads it.** What the view
        // says is what this call asks; what the caller sent beside it wins, so
        // a view is a starting point rather than a cage. **Done before
        // anything below resolves a name to a declaration or a shape to a
        // link**, because those read `args` as the caller left it — a view
        // filling in `answers_type` or `follow` after that point would fill
        // in fields nothing downstream ever looks at again.
        //
        // **The view's own key filters ride separately from `args`**, because
        // `RecallArgs.fields` is the caller-facing shape (`KeyFilterArgs`) and
        // a view's are already the domain shape — carrying them through as
        // `RecallArgs` would mean converting them twice for no reason. Used
        // only when the caller named no `fields` of its own; a caller that did
        // meant that filter, exactly as every other view-filled argument works.
        // **Whether the caller named `keys` itself**, taken before a view fills
        // the call in: the note an object leaves when it is narrowed tells a
        // different way back for keys a view supplied.
        let caller_named_keys = args.keys.is_some();
        let (args, view_filters) = match args.view.clone() {
            None => (args, Vec::new()),
            Some(named) => match self.asked_by_name(&named, args).await {
                Ok(filled) => filled,
                Err(refused) => return Ok(*refused),
            },
        };
        // **The name is resolved to its declaration here, once**, exactly as
        // `search` resolves it: everything below takes the keys rather than
        // the name, and a name nobody declared is answered where the roster to
        // offer instead is in reach.
        let mut type_displaced = Vec::new();
        let answers_type = match &args.answers_type {
            None => None,
            Some(wanted) => match self.declared(wanted, "recalled").await? {
                Ok((declared, displaced)) => {
                    type_displaced.push((wanted.clone(), displaced));
                    Some(declared)
                }
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
                Ok((declared, displaced)) => {
                    type_displaced.push((wanted.clone(), displaced));
                    Some(declared)
                }
                Err(refused) => return Ok(refused),
            },
        };
        // **Kept for the answer.** Both resolved declarations move into the
        // query below, and an answer that selects nothing under them has to be
        // able to say what they are.
        let asked_answers = answers_type.clone();
        let asked_fits = fits_type.clone();
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
            Some(overdue) => Some(
                self.dated(overdue.as_of.as_deref(), args.sid.as_deref())
                    .await?,
            ),
        };
        // **The one clock read, taken here whether or not a question named a
        // day.** What is held is asked as of a day like everything else, so a
        // call that named one for `overdue` asks about the same day here, and
        // the answer says which day it used either way.
        let today = self.dated(None, args.sid.as_deref()).await?;
        // Every key some declaration made a reference — what makes a value a
        // link. Read once, here, so the ranking stays a function of what it is
        // handed.
        let keys = match self.memory.declared_types().await {
            Ok(declared) => graph::reference_keys(&declared),
            Err(e) => return memory_declined("recall", e),
        };
        // The same window the writes behind a key get, for the same reason: a
        // key with a thousand spellings would otherwise bury the caller who
        // asked one narrow question.
        let most_values = args
            .values_most
            .map_or(graph::WRITES_SHOWN, |most| most as usize);
        // **A charter IS the page**, so asking for one asks the walk for prose
        // whether or not the caller wanted it under that name. What the caller
        // asked for is what is SHIPPED: the charter replaces the page rather
        // than arriving beside it, because a reader who did not ask for the
        // page would otherwise be sent the same text twice.
        let history_most = args
            .history_most
            .map_or(graph::WRITES_SHOWN, |most| most as usize);
        // **Two words and nothing between them**, so a typo is refused rather
        // than read as "every record".
        let wanted_status = match args.status.as_deref().map(str::trim) {
            None | Some("") => None,
            Some("active") => Some(jojobot_domain::memory::FactStatus::Active),
            Some("archived") => Some(jojobot_domain::memory::FactStatus::Archived),
            Some(other) => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(format!(
                        "status is active or archived, and '{other}' is neither — leave it out \
                         to read every record"
                    )),
                );
            }
        };
        let want_charter = args.charter.unwrap_or(false);
        let asked_prose = args.prose.unwrap_or(false);
        // **Which keys of each object's fields come back**, when the call or
        // the view it named said. Kept as sent: matching is by key name.
        let only_keys = args.keys.clone();
        let keys_from_view = args.view.is_some() && !caller_named_keys && only_keys.is_some();
        let include = graph::Include {
            facts: args.facts.unwrap_or(false),
            prose: asked_prose || want_charter,
            stood_for: args.stood_for.unwrap_or(false),
        };
        // **The same one clock read as `overdue`, and only when asked.** The
        // domain is handed the day and reads none itself.
        let near = match &args.near {
            None => None,
            Some(asked) => Some(graph::Nearness {
                day: self
                    .dated(asked.day.as_deref(), args.sid.as_deref())
                    .await?,
                within_days: asked.within_days.unwrap_or(NEAR_WINDOW),
                clock: parse_clock(asked.clock.as_deref())?,
            }),
        };
        // **One axis, two things it can trace.** Naming both asks two questions
        // of one answer, so it is refused rather than answered with whichever
        // the code happened to check first.
        let trace = match (args.history.as_deref(), args.history_record.as_deref()) {
            (Some(_), Some(_)) => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "history names a KEY and history_record names a RECORD, and they are the \
                         same axis: a call can trace one or the other. Name whichever you meant \
                         and drop the other"
                            .into(),
                    ),
                );
            }
            // Trimmed like every other key a caller names, so `donuts_eaten `
            // asks the question `donuts_eaten` answers.
            (Some(key), None) => Some(graph::History {
                of: graph::Trace::Key(key.trim().to_string()),
                most: history_most,
            }),
            (None, Some(address)) => Some(graph::History {
                of: graph::Trace::Record(match FactAddress::parse(address) {
                    Ok(parsed) => parsed,
                    Err(refused) => return memory_declined("recall", refused),
                }),
                most: history_most,
            }),
            (None, None) => None,
        };
        // **Parsed here, once, so the fill below and the walk further down
        // share the same parse** — never a second `FactAddress::parse` of
        // the same string.
        let built_on_address = match args.built_on.as_deref() {
            None => None,
            Some(address) => match FactAddress::parse(address) {
                Ok(parsed) => Some(parsed),
                Err(refused) => return memory_declined("recall", refused),
            },
        };
        let query = graph::GraphQuery {
            select: graph::Selection {
                subject: args.subject.as_deref().map(EntityId::person),
                kind: args.kind.as_deref().map(parse_kind).transpose()?,
                answers_type,
                candidates: None,
                fields: match &args.fields {
                    Some(named) => key_filters(named)?,
                    None => view_filters,
                },
                near,
                asked_by,
            },
            include,
            follow,
            history: trace,
        };
        // 🚨 **An address is a selection.** A record's address contains its
        // subject, and a call that named only a record — to trace, or to walk
        // lineage from — was refused for naming nothing to recall, which is
        // what the first callers of these arguments met on their first use
        // (rule 236). The subject is filled in from the address when the
        // call chose nothing else, and the condition is the refusal's own,
        // so the two cannot come to disagree. `history_record` and
        // `built_on` share this one path rather than each carrying a copy
        // of it.
        let mut query = query;
        // **Whether `subject` is the caller's own word, or a fill this loop
        // is about to do itself.** The two read the same on `query.select`
        // once filled, and the refusal below has to tell them apart: a
        // caller-supplied subject disagreeing with an address is a caller
        // mistake worth naming as "asked for"; a subject THIS LOOP filled
        // from an earlier address disagreeing with a later one is a
        // different mistake — two addresses on two different things — and
        // saying the caller "asked for" the first address's entity would be
        // inventing a subject nobody sent.
        let subject_was_explicit = query.select.subject.is_some();
        let mut filled_by: Option<(&str, FactAddress)> = None;
        let selecting_addresses = [
            query.history.as_ref().and_then(|h| match &h.of {
                graph::Trace::Record(address) => Some(("history_record", address.clone())),
                graph::Trace::Key(_) => None,
            }),
            built_on_address
                .clone()
                .map(|address| ("built_on", address)),
        ];
        for (arg_name, address) in selecting_addresses.into_iter().flatten() {
            match &query.select.subject {
                // **The two DISAGREEING is the case worth a refusal.** A
                // caller whose arguments point at two different things has made
                // a mistake, and answering one of them quietly picks for them.
                Some(named) if named != &address.home && subject_was_explicit => {
                    let named = named.clone();
                    return memory_declined(
                        "recall",
                        MemoryError::InvalidQuery(format!(
                            "{arg_name} names {address}, which is a record on {}, and asks for \
                             {named}. Drop the subject — the address names its own — or name the \
                             record you meant on {named}",
                            address.home,
                        )),
                    );
                }
                // **The disagreement is between two ADDRESSES, not between
                // an address and anything the caller sent.** `filled_by` is
                // always `Some` here: `subject_was_explicit` is false, so the
                // only way `query.select.subject` holds a value that
                // disagrees is that an earlier iteration of this same loop
                // set it.
                Some(named) if named != &address.home => {
                    let (earlier_arg, earlier_address) = filled_by
                        .clone()
                        .expect("a subject that disagrees with no caller input was filled here");
                    return memory_declined(
                        "recall",
                        MemoryError::InvalidQuery(format!(
                            "{earlier_arg} names {earlier_address}, a record on {}, and \
                             {arg_name} names {address}, a record on {} — two different things. \
                             Narrow to whichever record you meant, or send a subject naming the \
                             one you want",
                            earlier_address.home, address.home,
                        )),
                    );
                }
                None if query.select.narrows_nothing() => {
                    query.select.subject = Some(address.home.clone());
                    filled_by = Some((arg_name, address.clone()));
                }
                _ => {}
            }
        }

        // **Route through the search index when nothing else already makes
        // the walk cheap.** A type-only selection — no handle, no kind — is
        // exactly the shape `graph::walk` cannot answer without reading every
        // entity's records to find the ones that carry the type's keys. The
        // same structural match already answers cheaply through the index
        // ingest already builds (search.rs's `type_clause`, a real query over
        // the `meta_key` postings a document carries — not a scan). This asks
        // that question first and hands the narrowed set in as `candidates`,
        // which `graph::walk` then reads instead of reopening the full list.
        //
        // **Every other selection shape is untouched.** A kind or a subject
        // already makes `graph::walk` itself cheap — existing callers,
        // rhythms included, keep the path they have.
        //
        // **`None` when this call took no path that could ever hit the
        // ceiling** — the same shape `overdue_excluded` uses, because a
        // question never asked and a question answered zero are different
        // claims.
        let mut candidates_capped: Option<bool> = None;
        if let (Some(declared), None, None) = (
            &query.select.answers_type,
            &query.select.subject,
            query.select.kind,
        ) {
            let candidates = match self
                .search
                .search(&SearchQuery {
                    answers_type: Some(declared.clone()),
                    limit: CARRIER_CANDIDATES_LIMIT,
                    ..SearchQuery::default()
                })
                .await
            {
                Ok(hits) => hits,
                Err(e) => return memory_declined("recall", e),
            };
            candidates_capped = Some(hit_candidate_ceiling(
                candidates.len(),
                CARRIER_CANDIDATES_LIMIT,
            ));
            query.select.candidates = Some(
                candidates
                    .into_iter()
                    .filter_map(|hit| match hit {
                        Hit::Entity { entity, .. } => Some(entity.id),
                        _ => None,
                    })
                    .collect(),
            );
        }

        // **A session joins the walk only when the query could reach one** —
        // kept off the ordinary path so a lookup that never mentions a
        // session does not pay for a `Sessions` read it did not ask for.
        // **Every run on the board joins the entity index, cheaply**:
        // `all_summaries` never asks the store for a beat's own text, so a
        // browse or a named lookup costs the same aggregate whether the run
        // is the caller's own or not. `graph::walk`'s owner check (an owned
        // object answers its owner alone — proven on this exact shape at
        // `19fca07`) is what keeps another bot's runs out of the answer; it
        // is the ONLY guard now, not a redundant second one, because nothing
        // upstream of it scopes by bot any more.
        let wants_sessions = query.select.kind == Some(EntityKind::SESSION)
            || query
                .select
                .subject
                .as_ref()
                .is_some_and(|s| s.kind() == Some(EntityKind::SESSION));
        // **How many runs a caller with no identity is told exist.** It owns
        // none, so it is shown none and told only the count.
        let mut runs_counted_for_nobody = 0usize;
        let session_docs: Vec<jojobot_domain::memory::search::DocScan> = if wants_sessions {
            match self.sessions.all_summaries().await {
                Ok(runs) if caller.is_some() => runs
                    .iter()
                    .map(jojobot_domain::session::projected_summary)
                    .collect(),
                // 🚨 **Counted, never fed to the walk.** A run's entity is named
                // for its focus text and owned by a bot, and the walk's near-miss
                // screen and its not-yours answer both say so. A caller with no
                // identity that reached them could read another bot's focus by
                // naming a near miss of a run's id, and its owner by naming the
                // id. Skipping the read altogether made an anonymous "nothing
                // here" read exactly like an empty index, so the runs are
                // counted and nothing else of them is kept.
                Ok(runs) => {
                    runs_counted_for_nobody = runs.len();
                    Vec::new()
                }
                Err(e) => {
                    return session_declined(
                        e,
                        caller.as_ref().map_or("", |caller| caller.sid.as_str()),
                    );
                }
            }
        } else {
            Vec::new()
        };

        let graph::Selected {
            objects: mut found,
            withheld,
            unplaced,
            archived_excluded,
        } = match graph::walk(self.memory.as_ref(), &session_docs, &query).await {
            Ok(answer) => answer,
            Err(e) => return memory_declined("recall", e),
        };
        // The count above, only for a browse of every run: a named subject
        // that is a run never reaches here for a caller with no identity.
        let withheld = if query.select.kind == Some(EntityKind::SESSION) {
            withheld + runs_counted_for_nobody
        } else {
            withheld
        };
        // **The chronology, read only for a run the walk actually kept.**
        // `session_docs` above is cheap on purpose — no beat's text left the
        // store to build it — so a run that made it into `found` (selectable
        // AND readable; the owner check already ran) still carries no
        // chronology until this reads it, one targeted `read_session` per
        // run, never the whole board. This is the same split ordinary
        // entities already have: `graph::walk` fetches a wanted entity's own
        // records after deciding it is wanted, never before — sessions
        // could not do that inside `walk` itself, since that module holds no
        // `Sessions` port on purpose (`session::projected`'s own doc: "the
        // caller already built its document"), so the same split happens
        // here instead, right after the same decision is made.
        if query.include.prose {
            let caller_sid = caller.as_ref().map(|c| c.sid.as_str()).unwrap_or_default();
            if let Some(refused) = self.fill_session_prose(&mut found, caller_sid).await {
                return refused;
            }
        }
        // **The arithmetic is the domain's and the selection is here.** It is
        // asked of the object's FOLDED fields — every write on it, one value
        // per key — because a rhythm is described a record at a time: the
        // record that set it up carries the cadence, and each check-in since
        // carries what it found.
        // 🚨 **How many the overdue filter dropped.** `None` when the call
        // asked no overdue question, exactly as `overdue_as_of` does — a
        // count of zero and an absent count are different claims. A total,
        // never a breakdown, the same shape as `near_unplaced` and
        // `withheld`: "not in this list" is a disjunction — not due yet,
        // never opened, unreadable, or a kind no carrier speaks for — and
        // this says how many, not which.
        let mut overdue_excluded: Option<usize> = None;
        // **How many days past the day asked about each object fell due**,
        // aligned with `found` in its final, already-sorted order. `None`
        // when the call asked no overdue question at all, exactly as
        // `overdue_as_of` and `overdue_excluded` already read — a caller that
        // asked nothing is told nothing, rather than a distance for a
        // question it never asked.
        let mut overdue_by_days: Option<Vec<Option<i64>>> = None;
        if let Some(as_of) = as_of {
            // **The read compares a moment to a day and computes none of them.**
            // Which moment a thing falls due at is its carrier's answer, so a
            // second sort of owed thing lands by answering here rather than by
            // this line growing a branch — and a kind no carrier speaks for
            // owes nothing, which is what keeps a person out of an answer about
            // what is late.
            let asked = self.carriers();
            let before = found.len();
            found.retain(|object| owed_by(&asked, object).owed_on(as_of));
            overdue_excluded = Some(before - found.len());
            // **Oldest due first — which has gone quiet longest, answered by
            // the order rather than left for a caller to re-derive from dates
            // it was never given.** Ordering an already-found, already-filtered
            // set costs nothing extra to find.
            found.sort_by_key(|object| owed_by(&asked, object).staleness());
            // **Read off the same `Due` the order already came from**, so the
            // number on the wire cannot disagree with the position it is in.
            overdue_by_days = Some(
                found
                    .iter()
                    .map(|object| owed_by(&asked, object).days_overdue(as_of))
                    .collect(),
            );
        }
        // **The context a reader did not ask for, and the whole point of the
        // card.** A session that pulls a thing is told who holds what that
        // gets them in, ranked, without knowing there was anything to ask.
        let mut backing = Vec::with_capacity(found.len());
        if args.backing.unwrap_or(false) {
            for object in &found {
                match self.memory.backing(&object.entity.id).await {
                    Ok(held) => backing.push(Some(held)),
                    Err(e) => return memory_declined("recall", e),
                }
            }
        }
        let mut held = Vec::with_capacity(found.len());
        for object in &found {
            let pointing = match self.memory.referring_to(&object.entity.id).await {
                Ok(pointing) => pointing,
                Err(e) => return memory_declined("recall", e),
            };
            let day = as_of.unwrap_or(today);
            let widen = if query.follow.is_some() {
                entitlement::Widen::Never
            } else {
                entitlement::Widen::WhenEmpty
            };
            held.push(held_json(
                &entitlement::ranked(&pointing, &object.entity.id, day, &keys, widen),
                day,
                widen,
            ));
        }
        // **Lineage, when the call asked for it.** It is a claim-level question
        // rather than an object-level one, so it rides beside the objects
        // rather than inside them.
        let standing_on = match &built_on_address {
            None => None,
            Some(source) => {
                match self.memory.built_on(source).await {
                    Ok(claims) => Some(serde_json::json!({
                        "source": source.to_string(),
                        "count": claims.len(),
                        // No revision count: lineage is read straight off the
                        // store rather than through a walk, so there is no
                        // `fact_revisions` map beside it to look one up in.
                        "claims": claims
                            .iter()
                            .map(|fact| fact_json(fact, today, None))
                            .collect::<Vec<_>>(),
                    })),
                    Err(e) => return memory_declined("recall", e),
                }
            }
        };
        // 🚨 **A record whose home this call did not select carries no chain,
        // and the answer says so.** The trace hangs on the one object the
        // record is filed under, so a caller that selected somebody else gets
        // an answer with no `record_history` anywhere — which reads as *that
        // claim has no trace* and is a different statement from *you did not
        // ask about the thing it is on*.
        let unreached = match &query.history {
            Some(graph::History {
                of: graph::Trace::Record(address),
                ..
            }) if !found.iter().any(|object| object.record_history.is_some()) => Some(format!(
                "no writes of {address} are here: this call selected no object it is filed \
                     under. Ask again with subject: {} to read them",
                address.home
            )),
            _ => None,
        };
        // **Whether an elided bot actually has a charter, looked up
        // separately.** The walk above never read prose for this call —
        // `include.prose` is false whenever nobody asked for it — so
        // `object.prose` carries nothing here to answer from, on any object.
        // A bot's own charter is small enough (unlike the store as a whole)
        // that reading it once per bot in this read is worth the honesty: the
        // alternative is a note that cannot tell "there is more" from "there
        // is nothing".
        let mut elided_bot_charters: std::collections::HashMap<EntityId, bool> =
            std::collections::HashMap::new();
        if !want_charter {
            for object in &found {
                if object.entity.id.kind() != Some(EntityKind::BOT) {
                    continue;
                }
                let has_charter = matches!(
                    self.memory.scan_entity(&object.entity.id).await,
                    Ok(Some(doc)) if !doc.prose.trim().is_empty()
                );
                elided_bot_charters.insert(object.entity.id.clone(), has_charter);
            }
        }
        // **A bot's own room, read without writing.** A capacity refusal
        // already says how many of a bot's own thoughts aged out
        // (`MemoryError::RoomFull`); a caller that only reads its own
        // room — the common case — got told nothing until now, and an aged
        // thought never stops appearing here (ageing never archives), so
        // silence would read as "the room has no opinion" rather than
        // "nobody asked". Computed only for a bot carrying
        // `THOUGHT_CAPACITY` whose facts were asked for and hold at least
        // one thought: the touch-moment read this needs, once per
        // candidate, is the same shape `built_on`/`record_history` already
        // pay. Any trouble computing it — `Sessions` or `claim_history`
        // erroring — leaves the object unenriched rather than failing the
        // read: this is an addition to an ordinary recall, never a
        // condition of one.
        struct RoomView {
            capacity: usize,
            live: usize,
            aged: std::collections::HashSet<jojobot_domain::memory::FactId>,
            /// The runs a thought may go untouched, and whether the bot's own
            /// setting said so (else the default did).
            ages_after: usize,
            from_setting: bool,
        }
        let mut room_views: std::collections::HashMap<EntityId, RoomView> =
            std::collections::HashMap::new();
        if include.facts {
            for object in &found {
                if object.entity.id.kind() != Some(EntityKind::BOT) {
                    continue;
                }
                let Some(capacity) = object
                    .fields
                    .get(jojobot_domain::memory::THOUGHT_CAPACITY)
                    .and_then(|v| v.trim().parse::<usize>().ok())
                else {
                    continue;
                };
                let nominal_room = jojobot_domain::memory::thought_room(&object.facts);
                if nominal_room.is_empty() {
                    continue;
                }
                let mut touched = std::collections::HashMap::new();
                for fact in &nominal_room {
                    if let Ok(chain) = self.memory.claim_history(&fact.address()).await
                        && let Some(at) = chain.last().and_then(|w| w.written_at)
                    {
                        touched.insert(fact.id.clone(), at);
                    }
                }
                let Ok(runs) = self.sessions.summaries_of(&object.entity.id).await else {
                    continue;
                };
                let setting = jojobot_domain::memory::ages_after_runs_setting(&object.fields);
                let ages_after = setting.unwrap_or(jojobot_domain::memory::AGES_AFTER_RUNS);
                let cutoff = jojobot_domain::memory::aging_cutoff(
                    &runs.iter().map(|r| r.started_at).collect::<Vec<_>>(),
                    ages_after,
                );
                let split = jojobot_domain::memory::split_by_age(nominal_room, &touched, cutoff);
                if !split.aged_out.is_empty() {
                    room_views.insert(
                        object.entity.id.clone(),
                        RoomView {
                            capacity,
                            live: split.live.len(),
                            aged: split.aged_out.iter().map(|f| f.id.clone()).collect(),
                            ages_after,
                            from_setting: setting.is_some(),
                        },
                    );
                }
            }
        }
        // **A claim reached this session** — computed before `found` is
        // consumed below, the same trigger `search` uses: a read that asked
        // for facts and got none never touched the domain.
        // **A narrowed listing says what it left out.** Applied after every
        // step above that could reorder or drop an object, and keyed by the
        // object's handle, so the counts cannot drift from what is rendered.
        let mut left_out_by_status = std::collections::HashMap::<EntityId, usize>::new();
        if let Some(want) = wanted_status {
            for object in found.iter_mut() {
                let before = object.facts.len();
                object.facts.retain(|fact| fact.status == want);
                let removed = before - object.facts.len();
                object.facts_held = object.facts_held.saturating_sub(removed);
                left_out_by_status.insert(object.entity.id.clone(), removed);
            }
        }
        let claims_reached = found.iter().any(|object| !object.facts.is_empty());
        // **A mark asked for by `keys` is not on the thing, and the answer says
        // where it is.** The six describing keys stay on their claim, so
        // narrowing a thing's fields to one of them hands back an object with
        // nothing under it, which reads as "it holds no such key" and sends the
        // caller away from the one read that has it.
        let marks_asked: Vec<String> = {
            let describing = crate::seed::describing_keys();
            only_keys
                .iter()
                .flatten()
                .filter(|key| describing.contains(key))
                .cloned()
                .collect()
        };
        let mut body = serde_json::json!({
            "count": found.len(),
            "built_on": standing_on,
            "record_history_unreached": unreached,
            // **What the selected things already hold under one key**, when the
            // call named one. It is a read of the store and never a rule: a
            // caller picks a value that is in use, or writes one that is not,
            // and nothing here refuses the second.
            "values": args.values.as_deref().map(|key| values_json(&found, key, most_values)),
            // **The day the question was asked about**, and null when it asked
            // about none. A caller that let jojobot supply today has no other
            // way to learn which day that was, and an answer about an unnamed
            // day is one nobody can check.
            "overdue_as_of": as_of.map(|d| d.to_string()),
            "overdue_excluded": overdue_excluded,
            // **The day, the window and the clock this read used**, and null
            // when it asked about no day. A caller that let jojobot supply
            // today has no other way to learn which day that was.
            "near_day": near.map(|n| n.day.to_string()),
            "near_within_days": near.map(|n| n.within_days),
            "near_clock": near.map(|n| match n.clock {
                graph::Clock::RecordedOn => "recorded_on",
                graph::Clock::TakenIn => "taken_in",
                graph::Clock::HappenedAt => "happened_at",
            }),
            // 🚨 **How many records this clock could not place.**
            //
            // Empty and could-not-look are the same empty answer without this.
            // A record written before the taken-in stamp existed carries none,
            // so a read on that clock reaches nothing and would otherwise
            // report a day with nothing around it.
            "near_unplaced": near.map(|_| unplaced),
            // **What this selection matched and did not hand over, because it
            // belongs to another identity.**
            //
            // 🚨 **Withheld and absent are the same empty list without this.** A
            // session searching its own past and getting nothing back reads it
            // as "there is nothing" rather than "you were not allowed", and
            // acts on the first.
            //
            // **A total, never a breakdown.** How many is work status, which a
            // colleague may see; which identity holds them is a directory of
            // who is busy, and the caller named no handle to earn it.
            "withheld": withheld,
            // 🚨 **How many this browse matched and archival took out** — the
            // same field, the same meaning, as `list_entities`'s own count.
            // A browse naming no handle drops archived entities exactly as
            // `list_entities` does; without this a caller cannot tell "only
            // two exist" from "one was hidden".
            "archived_excluded": archived_excluded,
            // 🚨 **The type-only search path has its own ceiling, and this is
            // the only place a caller can learn it was reached.** A structural
            // query naming no handle and no kind routes through the search
            // index with a bound far above what a ranked answer would ever
            // need — see `CARRIER_CANDIDATES_LIMIT` — and a store holding more
            // than that many things answering the type still only ever gets
            // the first slice. `true` when the search sat at its own ceiling,
            // `false` when it did not, `null` when this call took no path
            // that could ever reach it.
            "candidates_capped": candidates_capped,
            "objects": found
                .iter()
                .enumerate()
                .zip(held)
                .zip(backing.into_iter().chain(std::iter::repeat(None)))
                .map(|(((idx, o), held), backing)| {
                    let mut rendered = object_json(
                        o,
                        include,
                        today,
                        only_keys.as_deref().map(|keys| KeyNarrowing {
                            keys,
                            by_view: keys_from_view,
                        }),
                    );
                    if let (Some(want), Some(removed)) =
                        (wanted_status, left_out_by_status.get(&o.entity.id))
                    {
                        rendered["facts_status"] = want.as_token().into();
                        if *removed > 0 {
                            let (key, other) = match want {
                                jojobot_domain::memory::FactStatus::Active => {
                                    ("archived_left_out", "archived")
                                }
                                jojobot_domain::memory::FactStatus::Archived => {
                                    ("active_left_out", "active")
                                }
                            };
                            rendered[key] = serde_json::json!({
                                "count": removed,
                                "how_to_read": format!(
                                    "recall again with status: \"{other}\" to read them, or \
                                     with no status to read every record"
                                ),
                            });
                        }
                    }
                    // **Present, with a null value, on the one object this
                    // cannot measure — never absent.** Absent would read as
                    // "you did not ask", which is a different claim from
                    // "nothing here can say".
                    if let Some(distances) = &overdue_by_days {
                        rendered["overdue_by_days"] = match distances[idx] {
                            Some(days) => days.into(),
                            None => serde_json::Value::Null,
                        };
                    }
                    let elided_bot_has_charter = elided_bot_charters
                        .get(&o.entity.id)
                        .copied()
                        .unwrap_or(false);
                    charter_json(
                        &mut rendered,
                        o,
                        want_charter,
                        asked_prose,
                        elided_bot_has_charter,
                    );
                    // **The room's own count, and the pointer that needs no
                    // second call.** Nothing here is a new verb: the facts
                    // above already carry the aged thought, this only
                    // names which one, alongside the same total a capacity
                    // refusal already reports.
                    if let Some(view) = room_views.get(&o.entity.id) {
                        rendered["room"] = serde_json::json!({
                            "capacity": view.capacity,
                            "live": view.live,
                            "aged_out": view.aged.len(),
                            "ages_after_runs": view.ages_after,
                            "ages_after_from": if view.from_setting {
                                jojobot_domain::memory::THOUGHT_AGES_AFTER_RUNS
                            } else {
                                "default"
                            },
                        });
                        if let Some(facts) = rendered["facts"].as_array_mut() {
                            for (fact, raw) in facts.iter_mut().zip(&o.facts) {
                                if view.aged.contains(&raw.id) {
                                    fact["aged_out"] = serde_json::Value::Bool(true);
                                }
                            }
                        }
                    }
                    // **A held snooze day says whether it is in force.** The
                    // key stays in the fields after a ran or skipped check-in,
                    // and a reader of the fields alone would take the old day
                    // for a live snooze.
                    if let Some(standing) = attention::snooze_standing(&o.fields) {
                        rendered["snooze"] = serde_json::json!({
                            "until": standing.until,
                            "in_force": standing.in_force,
                        });
                    }
                    rendered["held"] = held;
                    match backing {
                        Some(backing) => {
                            rendered["fields_backing"] = backing
                                .iter()
                                .map(|(key, from)| {
                                    let mut backing = serde_json::json!({
                                        "claim": from.fact.as_str(),
                                        "provenance": from.provenance.as_token(),
                                        "standing": from.standing.as_token(),
                                    });
                                    // **The caveat rides the value it is about.**
                                    // A folded value reads as flat fact, and the
                                    // sentence saying it was estimated sits on a
                                    // record one hop away with nothing pointing at
                                    // it — so the page a person reads is the one
                                    // place it was missing.
                                    //
                                    // **Absent when the record carries none**,
                                    // never an empty string: a key that was always
                                    // there and always blank reads as a note
                                    // somebody wrote saying nothing.
                                    if let Some(note) = &from.note {
                                        backing["note"] = serde_json::json!(note);
                                    }
                                    (key.clone(), backing)
                                })
                                .collect::<serde_json::Map<_, _>>()
                                .into();
                        }
                        // **A value arriving bare must not read as one nobody
                        // stands behind.** The backing is a second read and is
                        // left out unless asked for, so the answer says it
                        // exists and names the call that returns it — an
                        // omission a reader cannot see is the one it will get
                        // wrong. Only where there are values to stand behind.
                        None if !o.fields.is_empty() => {
                            rendered["fields_backing_note"] = serde_json::json!(
                                "who backs each of these values is not in this answer: recall \
                                 again with backing: true and every key names the claim its \
                                 value came from, with that claim's provenance and standing"
                            );
                        }
                        None => {}
                    }
                    rendered
                })
                .collect::<Vec<_>>(),
        });
        if claims_reached && self.first_contact(CLAIMS_DOMAIN, caller.as_ref()).await {
            crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
        }
        for (wanted, displaced) in &type_displaced {
            crate::answer::note_type_displaced(&mut body, wanted, displaced.as_ref());
        }
        // 🚨 **An answer that selected nothing under a type says what the type
        // is.** The boot names the shipped types and nothing about their keys,
        // so a session that asks for one it has never written finds out what
        // to write from the empty answer or from nowhere: a zero count that
        // named no key sent a run away without the spelling of a single one,
        // and what it wrote instead was unreachable by the keys the store asks
        // by. The same declaration `declare_type` answers with, so the
        // spelling read here is the spelling to write.
        //
        // `answers_type` selects the objects, so it is asked when none came
        // back. `fits_type` narrows what a walk reaches, so it is asked when
        // the walk reached nothing.
        let nothing_selected = found.is_empty();
        let nothing_reached = found.iter().all(|object| object.connected.is_empty());
        let described: Vec<&DeclaredType> = asked_answers
            .iter()
            .filter(|_| nothing_selected)
            .chain(asked_fits.iter().filter(|_| nothing_reached))
            .collect();
        if !described.is_empty() {
            body["type_keys"] = serde_json::Value::Array(
                described
                    .into_iter()
                    .map(crate::memory::wire::declared_type_json)
                    .collect(),
            );
        }
        // **A caller with no identity owns no run, and the count above is all
        // it can see of them.** Say how to read its own, because "withheld" on
        // its own does not.
        if wants_sessions && caller.is_none() {
            body["withheld_note"] = serde_json::json!(
                "no sid was passed, so no run here is yours: withheld counts the runs that \
                 exist and shows none of them. Boot with start_here as your bot and pass the \
                 sid it returns on this call to read your own runs"
            );
        }
        if !marks_asked.is_empty() {
            body["keys_note"] = serde_json::json!(format!(
                "{} describe their own claim and never fold onto the thing, so no object carries \
                 them under fields. Their values live on the claim: recall again with facts: \
                 true to read them there, or filter with a fields entry whose scope is record.",
                marks_asked
                    .iter()
                    .map(|key| format!("`{key}`"))
                    .collect::<Vec<_>>()
                    .join(", "),
            ));
        }
        if let Some(sid) = args.sid.as_deref() {
            self.registry.note_shown(sid, &body);
        }
        json_result(&body)
    }

    /// **The one targeted read per wanted run.** `objects` already answered
    /// `resolve`'s owner check — every session object left in it is one the
    /// caller may read — so what is left is reading it whole. Recurses into
    /// `connected` exactly as `fill_history`/`fill_revision_counts` do, on
    /// the same reasoning: a walk can reach a session by more than one hop.
    ///
    /// `Some(refused)` on the first read that fails; the caller returns it
    /// unchanged. `None` once every session object in the tree carries its
    /// real prose. `caller_sid` is this CALL's own handle, never a
    /// session's — `session_declined`'s own doc names why.
    ///
    /// The return type is named ([`FillSessionProseFuture`]) rather than
    /// spelled out, the same fix clippy's own `type_complexity` lint asks
    /// for on this exact recursive-boxed-future shape.
    fn fill_session_prose<'a>(
        &'a self,
        objects: &'a mut [graph::Object],
        caller_sid: &'a str,
    ) -> FillSessionProseFuture<'a> {
        Box::pin(async move {
            for object in objects {
                if object.entity.kind == EntityKind::SESSION {
                    if let Some(prose) = &object.prose {
                        // **Only ever empty here.** `session::projected_summary`
                        // never sets anything else, and nothing else can have
                        // set `Some` on a session object — a non-empty value
                        // would mean this ran twice, which the loop shape
                        // cannot do.
                        debug_assert!(prose.is_empty());
                        let id =
                            jojobot_domain::session::SessionId(object.entity.id.slug().to_string());
                        match self.sessions.read_session(&id).await {
                            Ok(full) => {
                                object.prose =
                                    Some(jojobot_domain::session::projected(&full).prose);
                            }
                            Err(e) => return Some(session_declined(e, caller_sid)),
                        }
                    }
                }
                if let Some(refused) = self
                    .fill_session_prose(&mut object.connected, caller_sid)
                    .await
                {
                    return Some(refused);
                }
            }
            None
        })
    }
}

#[cfg(test)]
mod tests;
