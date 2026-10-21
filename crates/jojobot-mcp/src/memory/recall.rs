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
use crate::memory::{left_out, view_run};
use crate::session::session_declined;
use jojobot_domain::memory::search::{Coverage, HeldField};
use jojobot_domain::memory::{entitlement, filter, graph, readiness};
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
    /// ⚠️ **A key a claim holds as one of its own `fields` is held by no thing.**
    /// It stays on that claim, so the default `thing` scope never finds it: ask for
    /// it with `scope: record`. A key sent in `sets` is held by the thing and is
    /// found by either scope.
    ///
    /// ⚠️ **The two scopes combine in one list, and every `record` filter must
    /// hold on the SAME record** — they describe a single record rather than a
    /// list of separate questions.
    #[serde(default)]
    pub(crate) scope: Option<String>,
}

/// **What a walk keeps of what it reaches**: key filters, or one expression.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub enum Keeping {
    /// One expression over keys, asked of what each reached thing holds.
    Expression(String),
    /// Key filters, each with its own scope.
    Filters(Vec<KeyFilterArgs>),
}

/// **The order the answer comes back in**: one key and a direction.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OrderArgs {
    /// The key the things are ordered by, as it is spelled where it was
    /// written. A key declared a date or a number orders as one, so 2 comes
    /// before 10; any other key orders as text, so 10 comes before 2, and the
    /// answer says so when the values would have parsed.
    pub(crate) key: String,
    /// `ascending` (the default; smallest first) or `descending`. Things that
    /// hold no such key come last in either direction.
    #[serde(default)]
    pub(crate) direction: Option<String>,
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
    /// · `connection`), or the word a read served for it (`memberOf`,
    /// `attendee`, `relatedTo`): either is understood. Omit for **any** edge — "whatever it is connected to"
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
    /// Defaults to 1; 1 to 5 are accepted, and 0 or more than 5 is refused.
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
    ///
    /// **It also takes one expression**, the same line a call's `where` takes
    /// (see there): `status != done && owner = bot:gamma`. The expression is
    /// asked of what each reached thing holds now, and of nothing else: a
    /// `scope: record` sent beside it is refused. To keep what a walk reaches by
    /// one record, send `keeping` as a list of key filters, each with
    /// `scope: record`.
    #[serde(default)]
    pub(crate) keeping: Option<Keeping>,
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
    /// **A member of a numbered collection answers to `<collection>/<N>`**, as
    /// `topic:contract-ordinances/32`: the descendant holding the number N under the key the
    /// collection numbers by. A number nobody holds comes back blocked with the
    /// numbers that exist. A collection says `next_number`, the number its next
    /// member takes.
    ///
    /// **A list answers `entries`**: its entries in order, each with its
    /// `address`, its `text`, its `link` when the entry draws an edge, and its
    /// `state`. An entry whose edge points at a piece of work also carries
    /// `linked`: that work's `status`, `owner` and `ready`, with `blocked_by`
    /// beside it when it is not ready, read now from the work (the same verdict
    /// the `ready` view gives). The list never skips a blocked entry or promotes a
    /// ready one. **It also answers `ticked`, how far along it is by its own
    /// ticks**: `done` out of `of` on a checklist, `done` and `open` on a queue and
    /// `turns` on a rotation, with nothing for a list with no entry that is not dropped. **A list whose
    /// entries link work also answers `progress`**: `done` out of `of` pieces of linked work, with `reads` saying it; work archived before it finished counts in neither. **A list that sets `covers` (a day, or an ISO week) is a plan**
    /// and also answers `plan`: its `from` and `to` days, what falls `due` in
    /// them, what is `overdue` (fell due before `from` and is still owed, with
    /// `overdue_by_days` at `from`) and which of that work is `ready`, worked out
    /// at the read by the due and ready views' own passes and stored nowhere. An
    /// entry carried from another plan (capture it with `derived_from` naming the
    /// old entry) reads `carried_from`, and the old entry reads `carried_to`; it
    /// stays as it was. It is the list's own read, so no `facts` are needed for it.
    ///
    /// **A named subject comes back** even when the filters below keep none of
    /// its records: naming a handle asks for that object, and a filter asks
    /// which objects. **The two questions that keep only some objects,
    /// `overdue` and `unreviewed`, apply to the named subject too**. A handle
    /// that names nothing comes back blocked with the nearest handles, never as
    /// an empty answer.
    #[serde(default)]
    pub(crate) subject: Option<String>,
    /// Every entity of one kind. `session` reads your own past runs.
    ///
    /// **YOUR OWN PAST RUNS ARE A KIND:** `session` reads this identity's own past
    /// runs and no other's, and `prose: true` brings back a run's chronology
    /// (list_runs gives each run's state and focus line; this is the read for what
    /// it did). The focus a run closed on is not in that prose: wrap_session's
    /// closing entry carries it as `closing_focus`. A selection that reaches
    /// another identity's runs counts them as `withheld` and shows none of them; a
    /// filter that leaves out runs of YOUR OWN says how many as
    /// `own_runs_excluded`. A run carries its name as the `sid` field, which you
    /// can filter on and which is never its key, and `near` places a run on the day
    /// it started. start_here resumes an unwrapped run by that name; only the
    /// `wrap_code` a wrap handed back reopens a wrapped one. A call with no sid owns no run: boot with start_here and pass
    /// the sid it returns to read your own. A run carries `served_since_compaction`,
    /// `served_lifetime` and `compactions` (how many it reported), and a read of
    /// one run adds a `served_<YYYY-MM-DD>` key for each day it was served on.
    ///
    /// **WEEKS OF USAGE ARE A KIND:** `usage` reads what each bot's runs add up
    /// to, one record per bot per ISO week (`week`, `from` and `to`; `partial`
    /// is true while the week is still open): runs begun, compactions reported,
    /// characters served, and the calls the bot made. `compactions` counts the
    /// compactions reported in the week, on the day the run was in when it
    /// reported (a run that states no day, on the day it was recorded), and
    /// `served_at_compaction` sums the totals they set back. `counted_from` is the earliest dated change of the
    /// bot's runs, which bounds the compactions: one before it left no dated
    /// record, so an earlier week can read low.
    ///
    /// `served_chars` is the week's total of what the bot was served, counted on
    /// the day each answer was served (`served_basis` is `day`), with one
    /// `served_<YYYY-MM-DD>` key for each day that had any. `served_counted_from`
    /// is the first day anything was counted for the bot. A week before it holds
    /// `served_chars` 0 and is not exact: read `served_by_run_start` for it, what
    /// the runs that began in the week were served over their whole lives.
    ///
    /// `calls` and `refused` are the week's totals; `calls_<verb>` counts every
    /// call to a verb, `refused_<verb>` the ones refused, and
    /// `refused_<verb>_<retry|change|person>` the refusals by who fixes them. A
    /// verb never called, and a word that never fixed a refusal, has no key, so a
    /// bare key (`where: "refused_capture"`) asks whether it happened, but only
    /// from `calls_counted_from` on, the first day a call of the bot was counted.
    /// A week before it holds no call keys whatever the bot did, so the answer
    /// there is unknown, never no. A call is counted once, after it is answered,
    /// on the day the run was in; a resend of a request id that landed is not
    /// counted again. A call is counted under the bot of its session, and a boot
    /// that opens or resumes a run is counted under that run. A call with no
    /// session, an anonymous boot and a refused boot count for no bot; a resume
    /// offer, for the bot named. A call no verb serves is counted as the verb
    /// `unserved`. They are computed when read and nobody writes one. You read
    /// your own bot's weeks and those of every bot below it on the chart; the
    /// assistant reads all; the rest are counted as `withheld`.
    #[serde(default)]
    pub(crate) kind: Option<String>,
    /// **Only the direct children of this entity**, as `kind:slug` (a bare
    /// handle is read as a person). One level, never a subtree. It combines with
    /// `kind`: `kind: "role"` with a bot as `parent` is that bot's roles, each
    /// with its holder, the moment of its last claim or renewal and its agent, in
    /// one read.
    #[serde(default)]
    pub(crate) parent: Option<String>,
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
    ///
    /// An answer that selects nothing still says what the type is: it carries
    /// `type_keys`, the type's keys and what each holds, so the spelling to write
    /// under it is in the empty answer. The first such answer in a session, under a
    /// type the software ships and with nothing else narrowing the read, also
    /// carries a teaching: reading again finds nothing, and what you worked out is
    /// kept by capturing under those keys. In `type_keys`, `required: true` marks a
    /// key the type counts as its floor and never means required to write: a record
    /// that holds some of the keys is kept and found here, with what it lacks named.
    /// `fits_type` keeps only the things that hold every key, optional ones too.
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
    /// **Which things, in one short expression over keys**, for
    /// example `status != done && owner = bot:gamma`,
    /// `due <= today+7d`, `tags contains kitchen`. The words are `=`, `!=`, `<`,
    /// `<=`, `>`, `>=`, `in (a, b)` (any of), `contains` (a list holds an item),
    /// `&&`, `||`, `!` and parentheses; a bare key asks that it is there. There
    /// are no wildcards. Quote a value that holds spaces or operator characters.
    ///
    /// **It reaches keys only.** `subject`, `kind`, `parent`, `answers_type` and
    /// `follow` stay arguments beside it. It combines with `fields`: both must
    /// hold.
    ///
    /// **One `scope` covers the whole expression**: what each thing holds now
    /// (the default), or what one record says. A negation keeps only things that
    /// hold the key: `status != done` is the things holding some other status,
    /// and a thing with no status is not among them; `!(status = done)` is the
    /// complement and does keep it. A comparison needs a declaration: `<`, `<=`,
    /// `>`, `>=` need a key declared a date or a number, and `contains` a key
    /// declared a list. A type named like a kind belongs to that kind, and its
    /// word on a key wins for things of that kind; a type under any other name
    /// licenses any thing carrying its key. Each thing is asked its own kind,
    /// named or not. A thing the declarations do not license does not match, and
    /// the answer counts what it left out by kind in `left_out_unlicensed`,
    /// saying which key and what to declare.
    /// `today`, `today+3d` and `today-2w` are dates relative to your day.
    ///
    /// A mistake is refused with the token, its position and the words accepted
    /// there; a key that nothing holds is refused with the keys that are held,
    /// rather than answering empty.
    #[serde(default, rename = "where")]
    pub(crate) expression: Option<String>,
    /// **What `where` is asked of**: `thing` (the default) is what each thing
    /// holds now, and `record` is what one record says, so every term must hold
    /// on the same record and the object arrives with the records that answered.
    /// Names neither scope for `fields`, which carry their own.
    #[serde(default)]
    pub(crate) scope: Option<String>,
    /// **The order things come back in**, applied to the whole selection before
    /// the answer is cut to size, so each part you read on with `offset` is a
    /// part of that order. Things that hold no such key come last.
    #[serde(default)]
    pub(crate) order: Option<OrderArgs>,
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
    ///
    /// **Who holds what comes back unasked** on a read that names no `keys`: every
    /// object carries a `held` block listing the records that point at it, best
    /// first, each marked by its `liveness`: what admits somebody and is good on the
    /// day (`live`), what admits them and has lapsed (`lapsed`), and, only when
    /// nothing admits anybody, whatever else points here through a declared key
    /// (`related`). Each says who holds it (`holder`), what backs it (`provenance`)
    /// and how sure the operator was (`standing`). It says nothing about whether
    /// anybody may go, which is a judgement jojobot does not make; it is bounded,
    /// counts what it left out (`left_out`) and, for admitting records, names the
    /// walk that returns the rest. **A bot's own thoughts age
    /// quietly:** asking facts of a bot that carries thought_capacity adds a `room`
    /// block naming the capacity, how many thoughts count against it now and how
    /// many have gone quiet from being untouched too long (still there, marked
    /// aged_out on the fact, excluded from the count rather than hidden). The block
    /// names its threshold (`ages_after_runs`, and `ages_after_from` saying whether
    /// the bot's own thought_ages_after_runs gave it or the default did) and is
    /// sent when at least one thought has gone quiet or the room is over its
    /// capacity (`over`). When one has gone quiet it names the call that brings
    /// it back (`bring_back`). A write that would go over the capacity is
    /// refused the same way.
    #[serde(default)]
    pub(crate) facts: Option<bool>,
    /// **Whether each record comes back with its details.** Off by default: a
    /// listing leads with each record's one-line claim, its address, provenance
    /// and standing, and the details (the reasons under a claim) are a paragraph
    /// the reader has not asked for. An answer that left details out says once,
    /// in `details_left_out`, how many it left out, the addresses of the first twenty, and how to ask. Send `true` to
    /// read them. Applies to every record this call returns, `records` included.
    #[serde(default)]
    pub(crate) details: Option<bool>,
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
    /// **Whether `facts` lists a thing's review mark.** Off by default: the mark
    /// is the one record defrag leaves on a thing it has reviewed, which is
    /// bookkeeping to every other reader. An object that has one says under
    /// `review_marks_left_out` how many the listing left out and how to read
    /// them. Send `true` to list it. `search` takes the same argument. A record
    /// asked for by `history_record` or `records` is served whole either way.
    ///
    /// **It also lists a bot's record of the release notes it was told**, which
    /// jojobot keeps and a listing leaves out the same way, counted under
    /// `release_notes_record_left_out`.
    #[serde(default)]
    pub(crate) review_marks: Option<bool>,
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
    /// every one of them unasked is a cost the caller cannot decline. A run's
    /// chronology is its prose, and an answer that left some out says so once,
    /// under `prose_left_out`.
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
    /// reaches included — carries its handle and name, just those of its fields
    /// and what the read computes. Its other identity fields (alternateName,
    /// archived, boot, crm, source, type, parent, as_of) are not there unless a key
    /// names them, and `connected` is left out where nothing was reached.
    ///
    /// **Eliding is never silent.** The answer says once, in `narrowed`, what
    /// the objects left out: the keys they hold that were not brought
    /// (`left_out_keys`) and, where it is true, that their records, their `held`
    /// block and who backs each value are not here, each with the argument that
    /// returns it; asking again without `keys` reads them all. When a view
    /// supplied the keys, each object says so in `fields_left_out`, naming them:
    /// ask again without the view, sending its kind, or with `keys` naming the
    /// ones left out. A key an object does not hold is simply not there.
    ///
    /// **A bot's `working_on`** is the thing its session said it is doing, and
    /// it is a field only while that session is live. After the wrap or the
    /// sweep the key is not there and the answer carries `working_on_note`.
    ///
    /// Omit it and every key comes back, which is the normal read. This
    /// narrows what each object says, never which objects come back, and it
    /// leaves facts, prose and the charter as the arguments for those say.
    /// **A list's blocks are keys too**: `entries`, `plan`, `progress`, `ticked`,
    /// `last_ticked` and `ticks` come back only when `keys` names them, so a list
    /// read that sends `keys` and wants its entries sends `entries`.
    /// **A view can carry it** as its `shows_keys`, and what you send beside
    /// the view's name wins.
    ///
    /// **How old what an object holds is:** each object carries `as_of`, the day
    /// the store last took in a write about it, and `fields_as_of` for any key
    /// whose own day is older; an object with no `as_of` has no write the store
    /// could date. **A read that names `keys` leaves `as_of` out unless `as_of` is
    /// one of them**, while `fields_as_of` still dates the keys it returns that
    /// are older than the object. That is a day the store saw, while
    /// `overdue`'s `as_of` is the day an overdue question is asked about.
    #[serde(default)]
    pub(crate) keys: Option<Vec<String>>,
    /// **Bring the `held` block back on a read that names `keys`.** A read that
    /// names keys carries, on each object, its handle, name, the keys asked and what
    /// the read computes, and says once, for the answer, what the objects left out.
    /// Who holds what points at an object (`held`) is one of those: send `held: true`
    /// to have it on each object, or `facts: true`, which reads the records the block
    /// is made of. A read that names no keys carries `held` as it always did.
    #[serde(default)]
    pub(crate) held: Option<bool>,
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
    ///
    /// The count of the writes is the answer to how many times: a key a sitting
    /// records once each time something happens is how you count occurrences. A key
    /// nobody wrote comes back as no writes, never as a refusal. A long history is
    /// cut to its newest twenty, saying how many exist and how many it left out;
    /// `history_most` raises the window.
    ///
    /// On a session it traces the run's own fields, named focus, timezone,
    /// stated_day, wrap_window, state, served_chars, charter_told or key, and
    /// `journal` returns the wordings of each entry written more than once. A
    /// run's history comes back in parts under the answer ceiling, newest
    /// first: `history_most` counts the changes or wordings of a part, and
    /// `offset` reads on. Each change of a field carries its `position`, its
    /// place in the order the run's changes were made.
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
    /// recorded one carries none. **Each write also names the `run` that made
    /// it**, by the run's public name, which opens nothing: a caller reads its
    /// own runs with `recall` kind session. A write no run made, and one kept
    /// before runs were recorded, carries none.
    ///
    /// **The address is selection enough.** Send it on its own and the read
    /// answers about the thing the address names — you do not have to repeat
    /// the subject beside it. Name a `subject` as well and it has to be that
    /// same thing, or the call is refused: two arguments pointing at two
    /// things is a mistake worth hearing about rather than one jojobot picks
    /// between.
    ///
    /// **Or a thing's own handle**, `kind:slug`, with no `#`: then the object
    /// comes back carrying every write of the THING itself — the name, aliases,
    /// prose and archive state it held before somebody changed them, and the run
    /// that changed them. Each version holds what the thing held after that write;
    /// a key it held nothing for is left out. A write kept before jojobot kept
    /// versions carries only its place in the order and its moment, and the
    /// answer says the earlier values were not kept. **A thing others were
    /// folded into carries their versions too**, in the order they were written,
    /// each marked with the `thing` that wrote it; a child a fold moved under it
    /// has a version of its own for the move.
    ///
    /// Omit it and no chain comes back, which is the normal read.
    #[serde(default)]
    pub(crate) history_record: Option<String>,
    /// **Record addresses to read as they stand now**, `kind:slug#fN` each, on
    /// any subjects, in one call. Each comes back with its current wording, its
    /// address and subject, provenance, standing, the day it was recorded and how
    /// many times it has been written, and none of the writes behind it:
    /// `history_record` is the door for those. An address that names nothing is
    /// named as missing and the others are still served. The call is its own
    /// question, so name no subject, kind or other selection beside it. A long
    /// list stops at the answer ceiling, and `not_shown` names the `offset` that
    /// reads on.
    #[serde(default)]
    pub(crate) records: Option<Vec<String>>,
    /// **How many of those writes come back**, newest kept. Twenty when
    /// you do not say.
    ///
    /// A key written a thousand times would otherwise be a thousand entries in
    /// an answer somebody asked one narrow question of. The answer always says
    /// how many writes exist and how many it left out, so raising this is how
    /// you reach the far end — deliberately, rather than by surprise.
    #[serde(default)]
    pub(crate) history_most: Option<u32>,
    /// **Read a list's ticks**, newest first: each says the entry ticked, the act,
    /// when, and which session. A list's ordinary answer never carries them, and
    /// says in `entries_left_out` how many done or dropped entries it left out.
    /// `history_most` counts the ticks of a part (twenty when you do not say) and
    /// `ticks_offset` reads on; the answer says how many are left.
    #[serde(default)]
    pub(crate) ticks: Option<bool>,
    /// **How many ticks of a list were already read**, newest first, when `ticks`
    /// reads on. `offset` pages the answer's objects and is not this.
    #[serde(default)]
    pub(crate) ticks_offset: Option<u32>,
    /// **How many you have already read, when an answer stopped at the answer
    /// ceiling.** The answer names the `offset` that returns the next part;
    /// repeat the same call with it. A call that selects ONE object counts its
    /// facts (`facts: true` on a subject with more than fit), or a list's
    /// entries; a call that selects several, a kind or a walk, counts the
    /// objects. Defaults to 0.
    ///
    /// One subject with more facts than fit is read the same way, `offset` counting
    /// its facts, and a list with more entries than fit pages its entries the same
    /// way, and a list read with `facts: true` counts its entries and then its
    /// records; an object that cannot fit even first in its part carries
    /// `facts_not_shown`, or for a list `entries_not_shown`, naming that subject to
    /// recall alone, which reads them in parts, and a single fact larger than the
    /// ceiling still comes back whole.
    ///
    /// **On a session named as the subject with `history`, `offset` counts the
    /// changes or wordings of its history already read, newest first.** A run
    /// holds no facts, so that call has one thing to read on through.
    #[serde(default)]
    pub(crate) offset: Option<u32>,
    /// **Where each folded value came from**, and who backs it: the claim whose
    /// write won the key, its provenance and its standing.
    ///
    /// **Off by default because it is a second read of each object**, and most
    /// reads want the value rather than its pedigree. Ask for it when you are
    /// about to act on a value, or when you need to know whether the operator said
    /// it. A key whose writes are summed has no single winning claim and is not
    /// here.
    ///
    /// **It follows `keys`**, and names the ones it left out. A claim that
    /// backs several keys is named by each of them, and its note is shipped
    /// once, in `fields_backing_notes`, under that claim.
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
    /// you name, never a judgement that something should happen. **Name the
    /// day in `as_of` or it reads the run's day** — the day it stated, else
    /// today in its zone — so *what is owed as of next Friday* is the same
    /// question asked about a different day.
    ///
    /// **What falls due, and when, is the answer of the keys a thing carries,
    /// whatever its kind.** A thing on a schedule (`cadence_days`/`rrule`,
    /// `anchored_on`, `counts_from`, `snoozed_until`) is next due a cadence
    /// after the day its cycle counts from; a thing carrying `runs_out`,
    /// `decide_by` or `promised_by` is due on that day. This filter compares the
    /// moment rather than computing it. A `work` item or `project` at `done`
    /// owes nothing, and neither does a thing whose `ended` is set beside
    /// `runs_out`, `decide_by` or `promised_by`.
    ///
    /// **A thing that carries none of those keys is absent**, whatever its
    /// kind: a rhythm nobody has put a schedule on made no promise to be late
    /// against, and a person who holds none of them is never late.
    ///
    /// **But a thing whose due moment cannot be READ comes back owed**,
    /// carrying its fields so you can see which key it is short of. That is
    /// deliberate: a half-built rhythm that surfaced at no read ever would never
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
    ///
    /// **It filters what another way of selecting chose** (a kind, a parent, a
    /// view): a call that names only `overdue` is refused as one that narrows
    /// nothing.
    #[serde(default)]
    pub(crate) overdue: Option<OverdueArgs>,
    /// **Only the things defrag has not reviewed**, oldest change first: a thing
    /// whose newest write is later than the stamp of defrag's add_entity, capture,
    /// update_fact or update_entity, or that has none. It names what to recall.
    /// Send `true`; the shipped view `unreviewed` asks the same.
    #[serde(default)]
    pub(crate) unreviewed: Option<bool>,
    /// **What else was recorded around a day.** See [`NearArgs`].
    #[serde(default)]
    pub(crate) near: Option<NearArgs>,
    /// Which edges to walk. Omit to walk none, and the answer is flat.
    ///
    /// **Walking.** The answer nests: a walked object carries the objects it
    /// reached, each carrying its own, so one call answers 'these objects, and what
    /// each is connected to'. Each hop is bounded: a wide fan-in comes back cut to
    /// what fits, in the order the walk reached it, and `connected_left_out` on the
    /// object above says how many did not fit (recall that handle again, or narrow
    /// with `keeping`). An object that draws edges the walk did not follow says so;
    /// an empty `connected` with no such note is an object with nothing beyond it.
    /// With `shape` and `relation` both unset a walk also reaches a MENTION (an
    /// entity named in a claim's own words), a REF (an entity a claim touches with
    /// no claim about how) and a FIELD LINK (an entity whose handle a record holds
    /// as a whole field value, or as every item of a comma list, under any key),
    /// each labelled mention, ref or field rather than as an edge; naming a shape
    /// or a relation answers only that. For one kind of thing, SELECT it (`kind`
    /// plus a `fields` filter on the key) rather than walking: traversal reaches,
    /// selection chooses. A key's declared value type licenses how you compare it
    /// (`before`/`after` on a date, `less`/`greater` on a number, `equals` on
    /// anything), and nothing is gated by a declaration: an undeclared record is
    /// still found by the keys it carries.
    #[serde(default)]
    pub(crate) follow: Option<FollowArgs>,
    /// **A question asked by name** — a view's handle, or its bare slug.
    ///
    /// A view is a record that holds a query, so asking for one is naming it.
    /// **Some ship with the software and you can declare your own**, with
    /// `add_entity` of kind `view` and the keys `selects`, `shows` and `asks`,
    /// with `where` (a line over keys, as the call's own `where`), `order` (a
    /// key, then `ascending` or `descending`) and `scope` (`thing` or `record`)
    /// for which of them and in what order.
    /// **A view that takes values declares them in `inputs`** (`project: kind
    /// project`) and uses each as `$name` in `parent` (`parent: "$project"`), in
    /// `where`, in `as_of` (the day an overdue ask is asked about) or in
    /// `answers_type` (the declared type whose things it finds). `shows_keys`
    /// names the keys each object comes back with.
    /// **A view needs something that narrows it**: `selects` (a kind), `parent`,
    /// `asks: overdue`, `asks: ready`, `asks: plan` or `asks: unreviewed`,
    /// `answers_type` or `where`.
    /// Asked as it stands without one, it is refused, naming the view and those
    /// keys; a kind, subject, parent, answers_type or fields sent beside it count.
    /// A write that leaves it with none still lands, with a note saying so.
    /// Both are records of the same shape, so nothing about the answer depends
    /// on where the view came from.
    ///
    /// What the view says is what this call asks; anything you send beside it
    /// is yours and wins, so a view is a starting point rather than a cage. The
    /// one exception: `ready` and `plan` answer for the work kind only, and a
    /// `kind` or `subject` of another kind beside them is refused.
    ///
    /// **A view may take inputs, and you fill them with `inputs`** rather than
    /// with a `where` of your own: `view: "ready", inputs: {project:
    /// "project:atlas"}` lists the ready work under that project. Recall a view's
    /// handle, or `kind: "view"` for all of them, to read what each takes.
    ///
    /// A name that is no view comes back blocked, with the views that exist.
    ///
    /// `view: "colleagues"` lists the identities here and what each is for; `view:
    /// "loops"` lists the recurring things (rhythms) and what each last recorded;
    /// `view: "ready"` lists the work that is not finished, whose dependencies are
    /// all finished and that waits on no one. It does not check `status`, so work
    /// parked at someday comes back too. The blocked work is counted as left out,
    /// and the answer names the call that says why: send the view with `subject`
    /// set to one piece of work and it comes back with `ready: false` and
    /// `blocked_by`. A view's answer can be narrowed with `where`: `view: "ready"`
    /// with `where` set to your board's not-started status, for example
    /// `where: "status = next"`, lists the ready work not yet started. On a
    /// view that holds a `where` of its own, yours replaces it. `view:
    /// "plan"` lists all the work that is not finished, ready or not, each piece
    /// with `ready` and, when it is not ready, `blocked_by`; send it with `parent`
    /// set to a project to read that project's whole plan in one call. A view
    /// carries its own `shows_keys`, and a `keys` you send wins over it. `view:
    /// "due"` lists what is coming up by the day `inputs: {by: ...}` names (a week
    /// ahead when you send none) and what is already late, whatever kinds owe
    /// something: an object past due carries `overdue_by_days`, one not
    /// yet due `due_in_days`. `view: "board"` takes the `project` it is about and
    /// lists that project's work ordered by status, each piece with its status,
    /// what it depends on and its owner. `view: "of-type"` takes a `type` (text)
    /// and lists the things that answer that declared type, each with the keys it
    /// lacks; it finds things only, so for a type another kind answers send
    /// `answers_type` to `recall` directly. `view: "unreviewed"` lists the things
    /// defrag has not reviewed, oldest change first, as `unreviewed: true` does.
    /// `view: "usage"` lists what each bot's runs add up to, newest week first;
    /// `inputs: {bot: ...}` narrows it to one bot. `view: "lists"` lists every
    /// list with its tick setting, how far along it is by its own ticks
    /// (`ticked`) and the day it was last ticked (`last_ticked`);
    /// `inputs: {under: ...}` narrows it to the lists hung on one thing, of any kind.
    #[serde(default)]
    pub(crate) view: Option<String>,
    /// **The values a view takes**, as `{name: value}`, beside `view`.
    ///
    /// A view is a saved question with typed inputs, and this is how you fill
    /// them: ask `view: "ready"` with `inputs: {project: "project:atlas"}` and it
    /// lists only the work under that project. **What a view takes is on the
    /// view**: recall it (`subject: "view:ready"`, or `kind: "view"` for all of
    /// them) and its `inputs` field reads `name: type`, where a type is `kind
    /// <kind>` (a handle of that kind), `handle` (a handle of any kind), `date`,
    /// `number` or `text`, and a `?` after it means the input may be left out. A
    /// value is checked as a write is: a handle must exist and be of the kind it
    /// names, a date takes the relative days `where` takes, a number must parse. An input left out takes its
    /// default when the view declares one (`by: date? = today+7d`) and otherwise
    /// drops the part of the question that uses it. **A call with a required input missing, an
    /// input the view does not declare, or a value of the wrong type is refused,
    /// and the refusal names every input the view takes**, so a view shows
    /// its shape.
    #[serde(default)]
    pub(crate) inputs: Option<std::collections::BTreeMap<String, serde_json::Value>>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// **The overdue question of a `recall`** — which things are owed, loops and
/// promises among them, as of a date.
///
/// It is a sub-object rather than a flag beside a date because the date only
/// means anything when the question is asked. Two flat arguments would admit a
/// call that names a day and filters nothing, which reads as an answer to a
/// question nobody asked.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OverdueArgs {
    /// **The date the question is asked about**, `YYYY-MM-DD`. Omit it and
    /// the run's day answers: the day it stated, else today in its zone. The
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

/// **The most objects one page of an answer can hold.** An object served is at
/// least dozens of bytes and the answer ceiling is 28,000, so a page never holds
/// more than this. The front of an ordering is asked for this many past the
/// offset, which is the room a page can need.
const MOST_OBJECTS_A_PAGE_HOLDS: usize = 1_024;

/// **Whether the type-only search path's own ceiling was reached.** A count
/// at the ceiling cannot be told apart from a store that holds exactly that
/// many things and no more — but treating it as capped is the caller-safe
/// read: the wrong guess costs nothing, and the other one lets a truncated
/// answer read as complete.
fn hit_candidate_ceiling(hit_count: usize, limit: usize) -> bool {
    hit_count >= limit
}

/// **Whether an answer returns an active claim**: a `facts` list, on an object or
/// on one it walked to, holding a claim that still stands.
fn returns_an_active_claim(body: &serde_json::Value) -> bool {
    match body {
        serde_json::Value::Object(fields) => fields.iter().any(|(key, value)| {
            (key == "facts"
                && value
                    .as_array()
                    .is_some_and(|facts| facts.iter().any(|fact| fact["status"] == "active")))
                || returns_an_active_claim(value)
        }),
        serde_json::Value::Array(items) => items.iter().any(returns_an_active_claim),
        _ => false,
    }
}

/// Which clock a neighbourhood read compares.
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
    /// **Which clock to compare**: `recorded_on` — the day in a claim's
    /// `recorded_at`, the day it was made, the default — `taken_in`, the day jojobot took the record in, or
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
                tree: None,
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
    let note: Option<String> = if held.is_empty() && widen == entitlement::Widen::Never {
        // **Empty here is a choice this call made**, and saying "nothing points
        // at this" would be a claim nobody checked.
        Some(
            "this answer lists no record that mentions this thing, because this call follows \
             links of its own: recall the handle on its own to see the records that mention it"
                .into(),
        )
    } else if held.is_empty() {
        Some(
            "nothing points at this, so there is nothing anybody holds for it and nothing else \
             naming it"
                .into(),
        )
    } else if held
        .iter()
        .all(|h| h.standing == entitlement::Standing::Related)
    {
        // **Both facts when both hold**: they link without admitting, and some did
        // not fit. The walk that reads them all follows every link in, with no
        // relation named, which reaches a link held in a field.
        let mut note = String::from(
            "these records mention this thing through a key other than `admits`: they link to it \
             and say nothing about who may use it",
        );
        if kept.omitted() > 0 {
            note.push_str(
                "; the rest did not fit this answer's budget: recall this handle with follow \
                 {direction: \"in\"}, naming no relation, for all of them",
            );
        }
        Some(note)
    } else if kept.omitted() > 0 {
        Some(
            "the rest did not fit this answer's budget: recall this handle with follow \
             {relation: \"admits\", direction: \"in\"} for all of them"
                .into(),
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
            if let Some(stamp) = &write.recorded_by {
                rendered["recorded_by"] = super::wire::recorded_by_json(stamp);
            }
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

/// **One run's history on the wire.** A field comes back as the changes it went
/// through, oldest first, each with its place in the run's order, what kind of
/// change it was, when the store made the record and what was kept of it. A
/// change that kept no value says what it was instead: a cleared field is
/// `cleared`, and a key that changed is `changed` and never carries the key. The
/// journal comes back as the wordings of each entry that was written more than
/// once, with how many wordings the entry has in all. `count` is how many records
/// there are and `shown` how many this part carries; what the part left out is
/// named by [`fit_run_histories`].
fn run_history_json(history: &graph::RunHistory) -> serde_json::Value {
    use jojobot_domain::session::history::ChangeKind;
    match history {
        graph::RunHistory::Field {
            field,
            changes,
            total,
            ..
        } => serde_json::json!({
            "key": field,
            "count": total,
            "shown": changes.len(),
            "writes": changes.iter().map(|change| {
                let mut rendered = serde_json::json!({
                    "position": change.ordinal,
                    "change": change.changed.map(ChangeKind::as_token),
                });
                if let Some(fields) = rendered.as_object_mut() {
                    if let Some(at) = change.written_at {
                        fields.insert("written_at".into(), at.to_string().into());
                    }
                    match (&change.value, change.changed) {
                        (Some(value), Some(ChangeKind::CharterTold)) => {
                            fields.insert(
                                "value".into(),
                                jojobot_domain::session::told_in_words(value).into(),
                            );
                        }
                        (Some(value), _) => {
                            fields.insert("value".into(), value.as_str().into());
                        }
                        (None, Some(ChangeKind::Key)) => {
                            fields.insert("changed".into(), true.into());
                        }
                        (None, _) => {
                            fields.insert("cleared".into(), true.into());
                        }
                    }
                }
                rendered
            }).collect::<Vec<_>>(),
        }),
        graph::RunHistory::Journal { entries, total, .. } => serde_json::json!({
            "key": jojobot_domain::session::history::JOURNAL,
            "count": total,
            "shown": history.shown(),
            "entries": entries.iter().map(|(entry, held, wordings)| serde_json::json!({
                "entry": entry.as_str(),
                "count": held,
                "wordings": wordings.iter().map(|wording| {
                    let mut rendered = serde_json::json!({
                        "revision": wording.revision,
                        "text": wording.text,
                        "written_at": wording.written_at.to_string(),
                    });
                    if let (Some(fields), Some(touched)) = (rendered.as_object_mut(), wording.touched) {
                        fields.insert("touched".into(), touched.to_string().into());
                    }
                    rendered
                }).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    }
}

/// **Cut each run's history under the answer ceiling, newest kept, and name
/// what it left out** (rule 466). A part keeps the newest records it carries
/// that fit with the rest of its object, never cutting a record, and always
/// keeps one, so reading on always moves. A single record larger than the
/// ceiling still comes back whole.
///
/// `paged` is the `offset` the caller sent when the call named a run alone: the
/// block that names what was left out then says to repeat the call with the
/// next offset. In a call that selected several objects `offset` counts objects,
/// so the block names the call that reads the run alone.
fn fit_run_histories(body: &mut serde_json::Value, traced: &str, paged: Option<usize>) {
    use jojobot_domain::text::ANSWER_CEILING;
    let Some(objects) = body.get_mut("objects").and_then(|o| o.as_array_mut()) else {
        return;
    };
    let mut objects = std::mem::take(objects);
    let rest = crate::answer::served_len(body)
        + crate::answer::STATUS_BAR_ROOM
        + crate::answer::widest_not_shown("objects");
    let skip = paged.unwrap_or(0);
    let what = if traced == jojobot_domain::session::history::JOURNAL {
        "wordings"
    } else {
        "changes"
    };
    for object in &mut objects {
        let Some(history) = object.get("history").cloned() else {
            continue;
        };
        let id = object["id"].as_str().unwrap_or_default().to_string();
        let count = history["count"].as_u64().unwrap_or(0) as usize;
        // Every record of the part, in the order it is served, with the entry it
        // belongs to for the journal.
        let mut records: Vec<(Option<usize>, serde_json::Value)> = Vec::new();
        let mut skeleton = history.clone();
        if let Some(entries) = skeleton.get_mut("entries").and_then(|e| e.as_array_mut()) {
            for (at, entry) in entries.iter_mut().enumerate() {
                let wordings = entry["wordings"].as_array_mut().map(std::mem::take);
                for wording in wordings.unwrap_or_default() {
                    records.push((Some(at), wording));
                }
            }
        } else if let Some(writes) = skeleton.get_mut("writes").and_then(|w| w.as_array_mut()) {
            for write in std::mem::take(writes) {
                records.push((None, write));
            }
        }
        let pointer = |left: usize, next: usize| match paged {
            Some(_) => crate::answer::not_shown(left, next, what),
            None => serde_json::json!({
                "count": left,
                "offset": next,
                "how_to_proceed": format!(
                    "these {what} are older than the ones above: recall subject: {id} with \
                     history: {traced} and offset: {next} reads them"
                ),
            }),
        };
        object["history"] = skeleton.clone();
        let reserve = crate::answer::under_key("not_shown", &pointer(usize::MAX, usize::MAX));
        let mut room =
            ANSWER_CEILING.saturating_sub(rest + crate::answer::served_len(object) + 1 + reserve);
        // Newest first: the newest write is the one written last, so fill in that
        // order and drop the oldest writes when the room runs out. The part is
        // served in the run's order, so the order of writing is read off each
        // record, with the run's order breaking a tie. **The moment is read as an
        // instant**: a moment prints its fraction only when it is not zero, so
        // as text `…:04Z` sorts after `…:04.5Z`.
        let mut newest_first: Vec<usize> = (0..records.len()).collect();
        newest_first.sort_by(|a, b| {
            let written = |at: &usize| {
                records[*at].1["written_at"]
                    .as_str()
                    .and_then(|moment| moment.parse::<jiff::Timestamp>().ok())
            };
            written(b).cmp(&written(a)).then(b.cmp(a))
        });
        let mut kept_at = std::collections::BTreeSet::new();
        for at in newest_first {
            let cost = crate::answer::served_len(&records[at].1) + 1;
            if !kept_at.is_empty() && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            kept_at.insert(at);
        }
        let kept = kept_at.len();
        let mut history = skeleton;
        for (at, (entry, record)) in records.into_iter().enumerate() {
            if !kept_at.contains(&at) {
                continue;
            }
            match entry {
                Some(at) => {
                    if let Some(wordings) = history["entries"][at]["wordings"].as_array_mut() {
                        wordings.push(record);
                    }
                }
                None => {
                    if let Some(writes) = history["writes"].as_array_mut() {
                        writes.push(record);
                    }
                }
            }
        }
        if let Some(entries) = history.get_mut("entries").and_then(|e| e.as_array_mut()) {
            entries.retain(|entry| entry["wordings"].as_array().is_some_and(|w| !w.is_empty()));
        }
        history["shown"] = kept.into();
        let left = count.saturating_sub(skip + kept);
        if left > 0 {
            history["not_shown"] = pointer(left, skip + kept);
        }
        object["history"] = history;
    }
    body["objects"] = objects.into();
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
fn record_history_json(history: &graph::ClaimHistory, details_asked: bool) -> serde_json::Value {
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
                // **The run that made this write**, by its public name and never a
                // key, as the versions of a thing's own row name it. Absent for a
                // write no run made.
                if let Some(run) = &write.session {
                    fields.insert("run".into(), run.as_str().into());
                }
                // **A write's details are left out of a history unless the call
                // asked**, the way a listing leaves them out; the answer counts them.
                if let (Some(details), true) = (&write.details, details_asked) {
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
                // **The receipt this write carried**, so the write that settled
                // a claim says on what, after a reopening took it off the claim.
                if let Some(receipt) = &write.receipt {
                    fields.insert("receipt".into(), super::wire::settling_receipt_json(receipt));
                }
                // **The app that wrote this write**, by the one renderer a fact
                // read uses. Absent for a write no app made.
                if let Some(stamp) = &write.app_stamp {
                    fields.insert("recorded_by".into(), super::wire::recorded_by_json(stamp));
                }
                // **What this write pointed at and stood for.** An empty list is
                // a write that recorded none, and an absent key is a write kept
                // before the store recorded them: the two read apart.
                if let Some(refs) = &write.refs {
                    fields.insert(
                        "refs".into(),
                        refs.iter().map(|named| named.as_str()).collect::<Vec<_>>().into(),
                    );
                }
                if let Some(marked) = &write.stands_for {
                    fields.insert(
                        "stands_for".into(),
                        marked.iter().map(|named| named.to_string()).collect::<Vec<_>>().into(),
                    );
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

/// **How many of a record's shown writes carry details this answer leaves out.**
fn writes_leaving_details_out(history: &graph::ClaimHistory, details_asked: bool) -> usize {
    if details_asked {
        return 0;
    }
    history
        .writes
        .iter()
        .filter(|write| write.details.is_some())
        .count()
}

/// **The versions of one thing's own row**, oldest first.
///
/// One version carries its place in the order, when it happened and the run that
/// made it (the run's public name, never a key), then each key the row held
/// after that write. A key the row held nothing for is left out, so a version
/// that never had a parent does not say `null`. A version kept before jojobot
/// kept them carries no keys, and the answer says what that means. When a thing
/// folded into this one wrote some of them, each version names that thing.
fn entity_history_json(history: &graph::EntityHistory) -> serde_json::Value {
    let versions: Vec<serde_json::Value> = history
        .versions
        .iter()
        .map(|version| {
            let mut rendered = serde_json::json!({
                "nth": version.ordinal,
                "written_at": version.written_at.map(|at| at.to_string()),
            });
            if let Some(fields) = rendered.as_object_mut() {
                if let Some(run) = &version.session {
                    fields.insert("run".into(), run.as_str().into());
                }
                if history.folded_in {
                    fields.insert("thing".into(), version.thing.to_string().into());
                }
                if let Some(held) = &version.snapshot {
                    fields.insert("kind".into(), held.kind.as_str().into());
                    fields.insert("name".into(), held.name.as_str().into());
                    fields.insert("source".into(), held.source.as_str().into());
                    fields.insert("boot".into(), held.boot.as_str().into());
                    fields.insert("aliases".into(), held.aliases.clone().into());
                    fields.insert("prose".into(), held.prose.as_str().into());
                    for (key, value) in [
                        ("crm", held.crm.clone()),
                        ("parent", held.parent.as_ref().map(ToString::to_string)),
                        ("archived_reason", held.archived_reason.clone()),
                        ("archived_at", held.archived_at.clone()),
                        (
                            "merged_into",
                            held.merged_into.as_ref().map(ToString::to_string),
                        ),
                    ] {
                        if let Some(value) = value {
                            fields.insert(key.into(), value.into());
                        }
                    }
                }
            }
            rendered
        })
        .collect();
    let unkept = history
        .versions
        .iter()
        .filter(|version| version.snapshot.is_none())
        .count();
    let mut body = serde_json::json!({
        "entity": history.entity.to_string(),
        "count": history.total,
        "shown": history.versions.len(),
        "versions": versions,
    });
    if let Some(fields) = body.as_object_mut() {
        if unkept > 0 {
            fields.insert(
                "values_not_kept".into(),
                format!(
                    "{unkept} versions were written before jojobot kept what the thing held, so \
                     they say that a write happened and when, and the earlier values are gone"
                )
                .into(),
            );
        }
        if history.elided() > 0 {
            fields.insert(
                "older".into(),
                format!(
                    "{} older versions are not here — ask again with a bigger history_most to \
                     reach them",
                    history.elided()
                )
                .into(),
            );
        }
    }
    body
}

/// **The refusal for a name that is no view.** The views that exist ride in the
/// sentence rather than as candidates: a candidate carries a reason the guard
/// flagged it, and none of these was flagged, they are simply what there is.
/// A similarity screen would be worse still, since it answers a wild guess with
/// nothing, which reads as "there are no views".
fn no_such_view(handle: &EntityId, known: &[Entity]) -> CallToolResult {
    let names = known
        .iter()
        .map(|e| e.id.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    blocked_body(
        handle,
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
    )
}

/// **The handle a view is named by**, whether the call sent the whole handle or
/// the bare slug.
fn view_handle(named: &str) -> EntityId {
    match named.trim().split_once(':') {
        Some((kind, _)) if kind == EntityKind::VIEW.as_token() => {
            EntityId(named.trim().to_string())
        }
        _ => EntityId::new(EntityKind::VIEW, named.trim()),
    }
}

/// **Why a view named in a call could not be filled.** A view the call
/// cannot use is an answer about the call; a store that could not be read is
/// an outage, and saying it is the call's fault sends the caller to change
/// something that was fine (rule 130).
enum ViewStop {
    /// The refusal to hand back as the answer.
    Said(Box<CallToolResult>),
    /// A read the store could not serve.
    Outage(MemoryError),
}

impl ViewStop {
    fn said(answer: CallToolResult) -> Self {
        Self::Said(Box::new(answer))
    }
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
        today: jiff::civil::Date,
    ) -> Result<ViewFilled, ViewStop> {
        let handle = view_handle(named);
        let held = self.memory.fields(&handle).await;
        let held = match held {
            Ok(held) if !held.is_empty() => held,
            // **A read the store could not serve is not "no such view".**
            Err(e @ (MemoryError::Store(_) | MemoryError::StoredDamage { .. })) => {
                return Err(ViewStop::Outage(e));
            }
            // **No view under that name.** The candidates are the views that
            // exist, which is what a caller who guessed a name needs — and it
            // costs one read they were about to make anyway.
            _ => {
                let known = self
                    .memory
                    .list_entities(Some(EntityKind::VIEW))
                    .await
                    .map_err(ViewStop::Outage)?;
                // **A view made with no keys is a view.** It holds no fields, which
                // is what "no view under that name" looks like from here, so the
                // list of views settles which it is. Such a view goes on to the
                // refusal that names it and the keys that would narrow it.
                if known.iter().any(|view| view.id == handle) {
                    Default::default()
                } else {
                    return Err(ViewStop::said(no_such_view(&handle, &known)));
                }
            }
        };
        // **The view's own keys and its own records, read where the page
        // reads them** (rule 51). A filter is a record on the view, so
        // reading the view's question needs its facts beside its fields. A
        // read of them that fails is the outage, as every other read this
        // view needs is: answering without them would run the question with no
        // filter and hand back every thing it selects as though that were the
        // answer (rule 473).
        let facts = self
            .memory
            .recall(&handle)
            .await
            .map_err(ViewStop::Outage)?;
        let asked = graph::asked_by_view(&held, &facts);
        // **The call's inputs are bound to what the view declares, and the view is
        // filled from them**, through the one function the page fills a view
        // through. A call that does not fit is refused naming what the view takes,
        // and nothing else runs. Handles are bound by the resolvers a write uses:
        // the current and former names of a thing, and the nearest things when a
        // handle names nothing.
        let given = match inputs_as_text(args.inputs.as_ref()) {
            Ok(given) => given,
            Err(words) => return Err(ViewStop::said(blocked_body(&handle, &[], words))),
        };
        let known = self
            .memory
            .list_entities(None)
            .await
            .map_err(ViewStop::Outage)?;
        let former = self
            .memory
            .former_handles()
            .await
            .map_err(ViewStop::Outage)?;
        let (asked, view_parent) =
            match asked.filled(handle.as_str(), &given, &known, &former, today) {
                Ok(filled) => filled,
                Err(refused) => {
                    return Err(ViewStop::said(blocked_body(
                        &handle,
                        &refused.nearest,
                        refused.words,
                    )));
                }
            };
        // **The view's own filters, handed on whole.** Whether they apply is
        // decided once, where the query is built: the caller's own `fields`
        // replace them there.
        let view_filters = asked.filters.clone();
        // **The view's own `where` has to read, unless the caller brought one.** A
        // `where` past a cap, or one that does not parse, was stored before the cap
        // or by an edit outside jojobot. The caller's call is not what is wrong, so
        // the answer is about the view: its name, what its line broke, and that
        // rewriting the line is the fix. Not asked when the caller's own `where`
        // wins, because the view's is not the one that runs.
        if args.expression.is_none()
            && let Some(line) = asked.expression.as_deref()
            && let Err(why) = filter::expression::reads_saved(line)
        {
            return Err(ViewStop::said(blocked_body(
                &handle,
                &[],
                format!(
                    "Nothing was read: the view '{}' cannot be run, because its stored `where` \
                     does not read: {why}. Rewrite the view's `where` (capture on the view with \
                     `sets`, or update_fact on the record that holds it), or ask with a `where` of \
                     your own, which runs in its place.",
                    handle.as_str()
                ),
            )));
        }
        // **The view's ordering reads as one line**, and a view whose ordering
        // does not read is a view that cannot be run: said here, with its name.
        let view_order = match asked.ordering() {
            Ok(order) => order.map(|order| OrderArgs {
                direction: Some(
                    match order.direction {
                        filter::Direction::Ascending => "ascending",
                        filter::Direction::Descending => "descending",
                    }
                    .to_string(),
                ),
                key: order.key,
            }),
            Err(why) => {
                return Err(ViewStop::said(blocked_body(
                    &handle,
                    &[],
                    format!(
                        "Nothing was read: the view '{}' cannot be run. {why}",
                        handle.as_str()
                    ),
                )));
            }
        };
        Ok((
            RecallArgs {
                // **The caller's own `where`, `scope` and `order` win whole**,
                // exactly as the arguments a view fills in always do.
                expression: args.expression.or(asked.expression),
                scope: args.scope.or(asked.scope),
                order: args.order.or(view_order),
                kind: args.kind.or(asked.selects),
                parent: args.parent.or(view_parent.map(|parent| parent.to_string())),
                facts: args.facts.or(asked.facts.then_some(true)),
                prose: args.prose.or(asked.prose.then_some(true)),
                charter: args.charter.or(asked.charter.then_some(true)),
                overdue: args.overdue.or_else(|| {
                    asked.overdue.then(|| OverdueArgs {
                        as_of: asked.as_of.clone(),
                    })
                }),
                unreviewed: args.unreviewed.or(asked.unreviewed.then_some(true)),
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
            asked.ready,
            asked.overdue,
        ))
    }
}

/// **A view read into the call it was named in**: the call, the view's own
/// filters, the readiness it asks and whether it asks what is overdue.
type ViewFilled = (
    RecallArgs,
    Vec<graph::FieldFilter>,
    Option<readiness::Ask>,
    bool,
);

/// **The values of a call's `inputs` as the text a view reads them as.** A string
/// is itself and a number or a boolean is what it prints as; an object, a list
/// or null is no value for an input.
fn inputs_as_text(
    sent: Option<&std::collections::BTreeMap<String, serde_json::Value>>,
) -> Result<std::collections::BTreeMap<String, String>, String> {
    let mut text = std::collections::BTreeMap::new();
    for (name, value) in sent.into_iter().flatten() {
        let word = match value {
            serde_json::Value::String(word) => word.clone(),
            serde_json::Value::Number(number) => number.to_string(),
            serde_json::Value::Bool(flag) => flag.to_string(),
            _ => {
                return Err(format!(
                    "Nothing was read: the input '{name}' was sent as something other than text \
                     or a number. Send each input as a single value: `inputs: {{\"{name}\": \
                     \"...\"}}`"
                ));
            }
        };
        text.insert(name.clone(), word);
    }
    Ok(text)
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

/// **Mark an object, and every object it reached, that is in `pages`**, with the
/// private `mark` the answer counts once. Walks what was rendered, so an object a
/// ceiling cut from `connected` is not counted.
fn mark_pages(
    rendered: &mut serde_json::Value,
    pages: &std::collections::HashSet<EntityId>,
    mark: &str,
) {
    if rendered["id"]
        .as_str()
        .is_some_and(|id| pages.contains(&EntityId(id.to_string())))
    {
        rendered[mark] = 1.into();
    }
    if let Some(reached) = rendered
        .get_mut("connected")
        .and_then(|connected| connected.as_array_mut())
    {
        for one in reached {
            mark_pages(one, pages, mark);
        }
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

/// **The identity fields an object carries beside its handle and name**, which a
/// read that names its keys leaves out unless a key names them. `connected` is
/// left out with them when nothing was reached.
const IDENTITY: [&str; 8] = [
    "alternateName",
    "archived",
    "boot",
    "crm",
    "source",
    "type",
    "parent",
    "as_of",
];

impl KeyNarrowing<'_> {
    /// Whether `key` is one of the keys the read kept.
    fn wants(&self, key: &str) -> bool {
        self.keys.iter().any(|kept| kept == key)
    }
}

/// **Whether `key` comes back**: every key does when nothing narrowed the read.
/// The one test every place that narrows by keys asks.
///
/// **The review stamp is the exception**: jojobot's own bookkeeping stays out of
/// an ordinary read and comes back when a read names it.
fn wants_key(only: Option<KeyNarrowing<'_>>, key: &str) -> bool {
    match only {
        None => !jojobot_domain::memory::review::is_bookkeeping_key(key),
        Some(narrowing) => narrowing.wants(key),
    }
}

/// **Whether a read names its keys itself**, as opposed to a view that supplied
/// them. The caller who names keys has said which of an object it wants, so what
/// else rows would carry is left out and said once for the answer.
fn names_keys(narrowing: Option<KeyNarrowing<'_>>) -> bool {
    narrowing.is_some_and(|narrowing| !narrowing.by_view)
}

/// **Whether a row of this read carries its `held` block.** The one place that
/// decides it: a read that names no keys carries it as it always did, and a read
/// that names keys carries it only when asked, by `held: true` or by `facts:
/// true`.
fn ships_held(asked_held: Option<bool>, facts: bool, names_keys: bool) -> bool {
    !names_keys || asked_held == Some(true) || facts
}

/// **What is true of the objects of an answer** that a note about a read naming
/// keys may say: the keys they hold that were not brought, and whether any holds
/// a record or a field, across the objects and everything walked from them.
#[derive(Default)]
struct Held {
    left_out_keys: std::collections::BTreeSet<String>,
    any_record: bool,
    any_field: bool,
}

/// Read [`Held`] off `objects` and what was walked from them.
fn held_by(objects: &[graph::Object], narrowing: KeyNarrowing<'_>, into: &mut Held) {
    for object in objects {
        into.left_out_keys.extend(
            object
                .fields
                .keys()
                .filter(|key| !narrowing.wants(key))
                .cloned(),
        );
        into.any_record |= object.facts_held > 0;
        into.any_field |= !object.fields.is_empty();
        held_by(&object.connected, narrowing, into);
    }
}

/// **What a read that names keys left out, said once for the answer** instead of
/// on every row. Each sentence is present only when it is true of the answer, and
/// names the argument that returns what it left out.
fn narrowed_note(
    keys: &[String],
    identity_left_out: &[&str],
    left_out_keys: &std::collections::BTreeSet<String>,
    records_left_out: bool,
    held_left_out: bool,
    backing_left_out: bool,
) -> serde_json::Value {
    let mut note = serde_json::json!({ "keys": keys });
    if !identity_left_out.is_empty() {
        note["envelope"] = format!(
            "not here: {}, and connected where nothing was reached — ask again without keys to \
             read them, or name them in keys",
            identity_left_out.join(", ")
        )
        .into();
    }
    if !left_out_keys.is_empty() {
        // **A list's blocks are named** when one was left out: "ask again without
        // keys" sent a caller back to the full read, and never said that `entries`
        // is a key it could name.
        let a_list_block = left_out_keys.iter().any(|key| {
            matches!(
                key.as_str(),
                "entries" | "plan" | "progress" | "ticked" | "last_ticked" | "ticks"
            )
        });
        note["fields"] = match a_list_block {
            true => "every object carries only these keys; its other keys are not here — \
                     ask again without keys to read them all, or name them in keys (a list's \
                     entries, plan, progress, ticked, last_ticked and ticks are keys)"
                .into(),
            false => "every object carries only these keys; its other keys are not here — \
                      ask again without keys to read them all"
                .into(),
        };
        note["left_out_keys"] = serde_json::json!(left_out_keys);
    }
    if records_left_out {
        note["records"] = "the records behind each object are not here — ask again with facts: \
                           true to read them, each with the address that edits it"
            .into();
    }
    if held_left_out {
        note["held"] =
            "who holds each object is not here — ask again with held: true to read it".into();
    }
    if backing_left_out {
        note["backing"] = "who backs each value is not in this answer: ask again with backing: \
                           true and every key names the claim its value came from, with that \
                           claim's provenance and standing"
            .into();
    }
    note
}

/// **The characters of record one part of a read of a single thing can carry**,
/// once the rest of the answer, the thing's own row and the pointer that names
/// what follows are counted. A fact is served whole or not at all, so this is
/// the most one fact may cost. The fill above and the write guard that refuses a
/// record over it ask this one function, so they cannot disagree.
pub(crate) fn record_room(rest: usize, skeleton: usize, widest_pointer: usize) -> usize {
    jojobot_domain::text::ANSWER_CEILING.saturating_sub(rest + skeleton + widest_pointer)
}

/// **What an answer says of the standing decisions it did not carry**, and the
/// browse that reads them.
fn decisions_not_shown(count: usize, of: &str) -> serde_json::Value {
    serde_json::json!({
        "count": count,
        "how_to_proceed": format!(
            "recall kind: decision with parent: {of} and facts: true reads every \
             standing decision, filling in parts"
        ),
    })
}

/// The key a standing decision carries in an answer, naming its thread.
const STANDING_DECISION_OF: &str = "standing_decision_of";

/// **Fit the records of one object under the ceiling**, from the `offset`th
/// record on, whole records only. `ahead` is what the answer already carries in
/// front of it, which the object's room is counted after. `keys` are the arrays
/// that are filled, in the order they are read: a thing's `facts`, a list's
/// `entries`, or a list's `entries` and then its `facts` when both were asked
/// for. `offset` counts through them as one sequence, so one `not_shown` reads
/// on through all of them.
fn fit_facts_of_one(
    body: &mut serde_json::Value,
    mut object: serde_json::Value,
    offset: usize,
    ahead: usize,
    keys: &[&str],
) {
    use jojobot_domain::text::Capped;
    let size = |json: &serde_json::Value| crate::answer::served_len(json) + 1;
    let rest = crate::answer::served_len(body) + crate::answer::STATUS_BAR_ROOM + ahead;
    let what = keys.join(" and ");
    let widest = crate::answer::widest_not_shown(&what);
    let mut sequence: Vec<(usize, serde_json::Value)> = Vec::new();
    for (at, key) in keys.iter().enumerate() {
        let records: Vec<serde_json::Value> = object[*key]
            .as_array_mut()
            .map(std::mem::take)
            .unwrap_or_default();
        sequence.extend(records.into_iter().map(|record| (at, record)));
        object[*key] = serde_json::json!([]);
    }
    let held = sequence.len();
    let skeleton = size(&object);
    let window: Vec<(usize, serde_json::Value, usize)> = sequence
        .into_iter()
        .skip(offset)
        .map(|(at, record)| {
            let cost = size(&record);
            (at, record, cost)
        })
        .collect();
    let room = Capped {
        budget: record_room(rest, skeleton, widest),
    };
    let kept = room.head(&window, |(_, _, cost)| *cost);
    for (at, record, _) in kept.kept() {
        if let Some(served) = object[keys[*at]].as_array_mut() {
            served.push(record.clone());
        }
    }
    body["objects"] = serde_json::json!([object]);
    if kept.elided() {
        body["not_shown"] =
            crate::answer::not_shown(kept.omitted(), offset + kept.kept().len(), &what);
    } else if offset > 0 && window.is_empty() {
        body["past_the_end"] = format!(
            "offset {offset} is past the last of this object's {held} {what}, so nothing is \
             left to read"
        )
        .into();
    }
}

/// **What the first object of a browse part says of the records it was cut
/// short of**: how many, and the call that reads them. `kept` is how many of its
/// `key` the part carries, which is where a list read alone starts on.
fn records_not_shown(id: &str, key: &str, count: usize, kept: usize) -> serde_json::Value {
    let how_to_proceed = if key == "entries" {
        format!("recall {id} alone with offset: {kept} reads its entries in parts that fit")
    } else {
        format!(
            "recall {id} alone with facts: true reads its facts in parts that fit, and a single \
             fact larger than the ceiling still comes back whole"
        )
    };
    serde_json::json!({"count": count, "how_to_proceed": how_to_proceed})
}

/// **The selected objects the walk did not read**, standing before and after the
/// ones the body holds. A walk that read a window of a browse leaves the rest
/// counted and not served, so the body holds a part of the selection and the
/// answer has to count the whole of it.
#[derive(Clone, Copy, Default)]
struct Outside {
    /// Objects before the body's first, already read by the caller's `offset`.
    before: usize,
    /// Objects after the body's last, never read.
    after: usize,
}

/// **Fit a recall under the answer ceiling, filling in the answer's own order.**
///
/// Silent when everything fits and no `offset` was sent: the answer is untouched
/// and carries no `not_shown`.
///
/// A call that selected ONE object fills that object's facts: `offset` is how
/// many of them were already read, whole facts only; a list read alone fills its
/// entries the same way. A call that selected
/// several fills the objects: `offset` is how many were already read, whole
/// objects only; an object that cannot fit even as the first of its part has its
/// facts cut where it stands, `facts_not_shown` says so, and recalling it alone
/// reads them in parts, except that a single fact larger than the ceiling is
/// never cut and still comes back whole. What was left out is counted under `not_shown` with the
/// `offset` that reads on.
fn fit_recall(body: &mut serde_json::Value, offset: usize, outside: Outside) {
    use jojobot_domain::text::{ANSWER_CEILING, Capped};
    let Some(objects) = body
        .get_mut("objects")
        .and_then(|o| o.as_array_mut())
        .map(std::mem::take)
    else {
        return;
    };
    let size = |json: &serde_json::Value| crate::answer::served_len(json) + 1;
    let rest = crate::answer::served_len(body) + crate::answer::STATUS_BAR_ROOM;
    let widest = crate::answer::widest_not_shown("objects");
    let total = objects.len();
    let windowed = outside.before + outside.after > 0;

    // **A list read alone fills its entries**, as a thing read alone fills its
    // facts: `offset` is how many were already read, and what is left out is
    // counted under `not_shown`. A list read with its records fills the entries
    // first and then the records, and `offset` counts through both. Its `ticked`
    // and `progress` were counted over every entry before this cut and ride
    // unchanged.
    let arrays: Vec<&str> = ["entries", "facts"]
        .into_iter()
        .filter(|key| total == 1 && objects[0][*key].is_array())
        .collect();
    if total == 1 && !windowed && !arrays.is_empty() {
        let object = objects.into_iter().next().expect("one object");
        fit_facts_of_one(body, object, offset, 0, &arrays);
        return;
    }

    // **A thread read with standing decisions in front of it.** The decisions
    // lead the first part, and every part after it is the thread's own records,
    // so `offset` counts records exactly as it does for a thread with no
    // decisions and the pointer "recall it alone" keeps meaning what it says.
    let lead = objects
        .iter()
        .take_while(|o| o.get(STANDING_DECISION_OF).is_some())
        .count();
    let thread_has_records = lead > 0 && objects.last().is_some_and(|o| o["facts"].is_array());
    if lead > 0 && !windowed && total == lead + 1 && (thread_has_records || offset == 0) {
        let mut objects = objects;
        let thread = objects.pop().expect("the thread follows its decisions");
        if offset > 0 {
            body["count"] = 1.into();
            fit_facts_of_one(body, thread, offset, 0, &["facts"]);
            return;
        }
        // The decisions that fit while the thread keeps room for itself.
        let mut skeleton = thread.clone();
        if thread_has_records {
            skeleton["facts"] = serde_json::json!([]);
        }
        skeleton["facts_not_shown"] = serde_json::json!({"count": 0, "how_to_proceed": ""});
        let of = objects[0][STANDING_DECISION_OF]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let widest_block =
            crate::answer::under_key("decisions_not_shown", &decisions_not_shown(usize::MAX, &of));
        let mut room = ANSWER_CEILING.saturating_sub(rest + widest_block + size(&skeleton));
        let mut kept: Vec<serde_json::Value> = Vec::new();
        let mut used = 0usize;
        for decision in &objects {
            let cost = size(decision);
            if !kept.is_empty() && cost > room {
                break;
            }
            room = room.saturating_sub(cost);
            used += cost;
            kept.push(decision.clone());
        }
        let left_out = objects.len() - kept.len();
        // The block that names the decisions left out rides beside the thread's
        // records, so the records are filled after it is counted.
        let block = (left_out > 0).then(|| decisions_not_shown(left_out, &of));
        let block_cost = block.as_ref().map_or(0, |block| {
            crate::answer::under_key("decisions_not_shown", block) + 1
        });
        if thread_has_records {
            fit_facts_of_one(body, thread, 0, used + block_cost, &["facts"]);
        } else {
            body["objects"] = serde_json::json!([thread]);
        }
        let mut served = kept;
        served.extend(
            body["objects"]
                .as_array_mut()
                .map(std::mem::take)
                .unwrap_or_default(),
        );
        body["count"] = served.len().into();
        body["objects"] = served.into();
        if let Some(block) = block {
            body["decisions_not_shown"] = block;
        }
        return;
    }

    let window: Vec<serde_json::Value> = objects.into_iter().skip(offset).collect();
    let all_fit = offset == 0
        && outside.after == 0
        && rest + window.iter().map(size).sum::<usize>() <= ANSWER_CEILING;
    if all_fit {
        body["objects"] = window.into();
        return;
    }
    let mut room = ANSWER_CEILING.saturating_sub(rest + widest);
    let mut shown: Vec<serde_json::Value> = Vec::new();
    for mut object in window.iter().cloned() {
        let cost = size(&object);
        if cost <= room {
            room -= cost;
            shown.push(object);
            continue;
        }
        let cuttable: Vec<&str> = ["entries", "facts"]
            .into_iter()
            .filter(|key| object[*key].is_array())
            .collect();
        if shown.is_empty() && cuttable.is_empty() {
            // Nothing of it can be cut: it is served whole, as the first of its
            // part always is.
            shown.push(object);
        } else if shown.is_empty() {
            // The first object of the part cannot fit whole: its entries and
            // facts are cut where they stand, and the rest of it is served as it
            // is. The facts of a list are filled only when every entry was kept,
            // so the part reads in the order a list read alone does.
            let id = object["id"].as_str().unwrap_or_default().to_string();
            let mut held: Vec<(&str, Vec<serde_json::Value>)> = Vec::new();
            for key in &cuttable {
                let records: Vec<serde_json::Value> = object[*key]
                    .as_array_mut()
                    .map(std::mem::take)
                    .unwrap_or_default();
                object[*key] = serde_json::json!([]);
                held.push((*key, records));
            }
            // The pointers ride in the object, so their room is counted before
            // anything is kept; each is measured at its widest.
            for (key, _) in &held {
                object[format!("{key}_not_shown")] =
                    records_not_shown(&id, key, usize::MAX, usize::MAX);
            }
            let mut left = room.saturating_sub(size(&object));
            let mut cut = false;
            for (key, records) in held {
                let total = records.len();
                let costs: Vec<(serde_json::Value, usize)> = if cut {
                    Vec::new()
                } else {
                    records
                        .iter()
                        .map(|record| (record.clone(), size(record)))
                        .collect()
                };
                let kept = Capped { budget: left }.head_or_none(&costs, |(_, cost)| *cost);
                left =
                    left.saturating_sub(kept.kept().iter().map(|(_, cost)| *cost).sum::<usize>());
                let kept_count = kept.kept().len();
                cut |= kept_count < total;
                object[key] = kept
                    .kept()
                    .iter()
                    .map(|(record, _)| record.clone())
                    .collect::<Vec<_>>()
                    .into();
                object[format!("{key}_not_shown")] =
                    records_not_shown(&id, key, total - kept_count, kept_count);
            }
            shown.push(object);
        }
        break;
    }
    body["count"] = shown.len().into();
    // Counted over the whole selection: the objects the body holds and the ones
    // the walk did not read on either side of them.
    let offset = outside.before + offset;
    let total = outside.before + total + outside.after;
    let after = offset + shown.len();
    let remaining = total.saturating_sub(after);
    body["objects"] = shown.into();
    if remaining > 0 {
        body["not_shown"] = crate::answer::not_shown(remaining, after, "objects");
    } else if offset > 0 && body["objects"].as_array().is_some_and(|o| o.is_empty()) {
        body["past_the_end"] = format!(
            "offset {offset} is past the last of the {total} objects this call selected, so \
             nothing is left to read"
        )
        .into();
    }
}

/// One object on the wire, and everything it reached.
///
/// **Absence means "not asked for"** on both halves that can be turned off:
/// `facts` and `prose` are missing when the caller declined them, rather than
/// rendered empty, so nothing has to tell an empty page from a page nobody
/// wanted.
/// **The frame a moment is read as a day in**: the server's clock and the zone
/// of the run asking, the same pair every other day-grained read uses.
#[derive(Clone, Copy)]
struct DayFrame<'a> {
    clock: jojobot_domain::clock::Clock,
    zone: &'a jiff::tz::TimeZone,
}

impl DayFrame<'_> {
    fn day(&self, at: jiff::Timestamp) -> jiff::civil::Date {
        self.clock.day_of(at, self.zone)
    }
}

/// **What a read narrowed by a view says an object left out**, with the way back.
/// The way back depends on who narrowed: keys the caller named come back by
/// asking again without them; keys a view supplied do not, because the view
/// supplies them again, so the advice would loop. The one sentence for an
/// object's fields and for a list's blocks, which are narrowed by the same keys.
fn fields_left_out_text(left_out: &[&str], only: Option<KeyNarrowing<'_>>) -> String {
    let way_back = if only.is_some_and(|narrowed| narrowed.by_view) {
        "the view named the ones it shows; ask again without view, sending its kind \
         yourself, or with keys naming these"
    } else {
        "ask again without keys to read them all"
    };
    format!(
        "{} of this object's keys are not here: {} — {way_back}",
        left_out.len(),
        left_out.join(", ")
    )
}

/// **The key a list block is narrowed by.** A block that rides beside another says
/// what the other left out, so it comes and goes with it: `entries_left_out` with
/// `entries`, and the ticks' count and pointer with `ticks`.
fn list_block_key(key: &str) -> &str {
    match key {
        "entries_left_out" => "entries",
        "ticks_total" | "ticks_left_out" => "ticks",
        other => other,
    }
}

/// **Whether a list's block comes back**: asked of the same keys the fields are,
/// so an overview of lists that names the keys it wants stays small. A `ticks:
/// true` names the ticks itself.
fn list_block_wanted(only: Option<KeyNarrowing<'_>>, key: &str, ticks_asked: bool) -> bool {
    let owner = list_block_key(key);
    wants_key(only, owner) || (owner == "ticks" && ticks_asked)
}

fn object_json(
    object: &graph::Object,
    include: graph::Include,
    as_of: jiff::civil::Date,
    only: Option<KeyNarrowing<'_>>,
    frame: DayFrame<'_>,
    details: bool,
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
            // **A claim the survivor named as one it cannot serve is read here**,
            // at the address under the folded handle it was named by: the
            // history asked for is the one thing a folded handle still answers.
            if let Some(history) = object.record_history.as_ref() {
                map.insert(
                    "record_history".into(),
                    record_history_json(history, details),
                );
                let left_out = writes_leaving_details_out(history, details);
                if left_out > 0 {
                    map.insert(wire::OBJECT_LEFT_OUT.into(), left_out.into());
                }
            }
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
    let wanted = |key: &str| wants_key(only, key);
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
        .filter(|key| !wanted(key) && !jojobot_domain::memory::review::is_bookkeeping_key(key))
        .collect();
    if !left_out.is_empty() && !names_keys(only) {
        fields.insert(
            "fields_left_out".into(),
            fields_left_out_text(&left_out, only).into(),
        );
    }
    // **The day the store last learned anything about this object, and a day of
    // a key's own only where it differs.** A value written in August reads
    // exactly like one written this morning, and a date printed on every value
    // becomes wallpaper nobody reads. One day says how fresh the thing is; a key
    // is dated only when it is older than the thing, so the day that is printed
    // is the one that means something. Absent where the store took in no moment
    // it could vouch for, never a made-up day.
    if let Some(ages) = &object.ages
        && let Some(newest) = ages.newest
    {
        let day = frame.day(newest);
        fields.insert("as_of".into(), day.to_string().into());
        let older: serde_json::Map<String, serde_json::Value> = ages
            .keys
            .iter()
            .filter(|(key, _)| wanted(key))
            .filter_map(|(key, at)| {
                let own = frame.day(*at);
                (own != day).then(|| (key.clone(), own.to_string().into()))
            })
            .collect();
        if !older.is_empty() {
            fields.insert("fields_as_of".into(), older.into());
        }
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
                .map(|fact| {
                    wire::listed_json(
                        fact,
                        as_of,
                        object.fact_revisions.get(&fact.id).copied(),
                        details,
                    )
                })
                .collect(),
        );
        // **What this object's records left out is counted here and said once by
        // the answer**, off the objects it kept.
        let left_out = wire::left_out_of(object.facts.iter(), details);
        if left_out > 0 {
            fields.insert(wire::OBJECT_LEFT_OUT.into(), left_out.into());
        }
        let at = wire::addresses_left_out(object.facts.iter(), details);
        if !at.is_empty() {
            fields.insert(wire::OBJECT_LEFT_OUT_AT.into(), at.into());
        }
        // **A claim the thing cannot serve is named, never dropped without a
        // word**: a merge left it under the folded thing's key and its event does
        // not list it. Each comes with the reason and the address that reads it.
        if !object.unmapped.is_empty() {
            fields.insert(
                "claims_left_out".into(),
                serde_json::json!({
                    "count": object.unmapped.len(),
                    "claims": object
                        .unmapped
                        .iter()
                        .map(|claim| {
                            serde_json::json!({
                                "address": claim.address.to_string(),
                                "reason": claim.reason(),
                            })
                        })
                        .collect::<Vec<_>>(),
                    "how_to_proceed": "recall with history_record set to an address here reads that claim where it is stored",
                }),
            );
        }
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
    } else if object.facts_held > 0 && !names_keys(only) {
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
        let mut body = history_json(history);
        // **A computed record says why it has no writes**, so the empty list is
        // not read as a key nobody wrote on a record somebody could have.
        if object.entity.kind == jojobot_domain::memory::EntityKind::USAGE
            && let Some(map) = body.as_object_mut()
        {
            map.insert(
                "note".into(),
                "a usage record is computed when it is read and nobody writes it, so it has no \
                 write history"
                    .into(),
            );
        }
        fields.insert("history".into(), body);
    }
    // **A run's own history**, under the same key a thing's is: the call asked one
    // question of an object and an object is a run or a thing, never both.
    if let Some(history) = object.run_history.as_ref() {
        fields.insert("history".into(), run_history_json(history));
    }
    // **The writes behind the record the call named**, on the one object it is
    // filed under. Under its own key rather than beside the key history: they
    // are different questions and a reader should not have to look at the value
    // to find out which was asked.
    if let Some(history) = object.record_history.as_ref() {
        fields.insert(
            "record_history".into(),
            record_history_json(history, details),
        );
        let left_out = writes_leaving_details_out(history, details);
        if left_out > 0 {
            fields.insert(wire::OBJECT_LEFT_OUT.into(), left_out.into());
        }
    }
    // **The writes behind the thing the call named**, on the one object it names.
    if let Some(history) = object.entity_history.as_ref() {
        fields.insert("entity_history".into(), entity_history_json(history));
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
        .map(|o| object_json(o, include, as_of, only, frame, details))
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
    // **A read that names its keys gets those keys and nothing else unless asked.**
    // The identity fields come off here, for the objects of the answer and the ones
    // a walk reached alike; a key the caller named stays, and the answer says once
    // which were left out.
    if names_keys(only)
        && let Some(narrowing) = only
    {
        for key in IDENTITY.iter().filter(|key| !narrowing.wants(key)) {
            fields.remove(*key);
        }
        if !narrowing.wants("connected")
            && fields["connected"].as_array().is_some_and(Vec::is_empty)
        {
            fields.remove("connected");
        }
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
        description = "The graph query: say the shape you want and get that shape back. Use \
                       it for a handle, a kind, a type or a key and its value; search is for \
                       words. THREE AXES COMBINE INTO ONE QUESTION. \
                       WHICH OBJECTS: subject (one handle or `<collection>/<N>`; a named \
                       handle that names nothing is blocked with the nearest), kind, \
                       parent, answers_type, fields (a key and the value it holds), where (one \
                       line over keys, status != done && owner = bot:gamma) or view (a question worked out, asked by name; \
                       start with one if it fits). A call that narrows nothing is refused: name one of subject, \
                       kind, parent, answers_type, fields, where, view, unreviewed or near, or a \
                       record's address (history_record, built_on). WHAT OF EACH: every object comes back as \
                       its FIELDS, what the thing HOLDS: every write on it folded, the newest \
                       write of each key winning (a snoozed rhythm also carries \
                       `snooze` {until, in_force}). facts, prose, charter, history, \
                       history_record, values, backing, stood_for and review_marks ask for more, each off \
                       by default, each saying what it left out; `keys` asks for less, list \
                       blocks too (`narrowed` names what it hid; \
                       `fields_left_out` does for a view's shows_keys). A merged thing carries \
                       `foldedNames`, the names folded into it. It can carry `claims_left_out`: \
                       claims under a thing merged into it that it cannot serve, each with a \
                       reason and an address; read one with history_record set to that address \
                       and no subject, several with records. An app's claim carries a server-set \
                       `recorded_by` {app, grant, received_at}, not the describing key; \
                       archive and re-capture it, never rewrite it. WHICH EDGES: follow {shape or relation, direction, depth, \
                       keeping, fits_type}, and the answer nests. Unlike search, recall \
                       returns claims of EVERY status unless you pass `status`. Your own past \
                       runs are kind session. AN ANSWER STOPS AT THE CEILING, never \
                       mid-record: not_shown names the offset that reads on. An empty answer \
                       names what it looked through, as searched. Each argument's \
                       description holds the rest."
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
        if let Some(addresses) = args.records.as_deref() {
            return self.recall_records(&args, addresses).await;
        }
        // **Whether this read takes a whole kind**, judged on the call as it was sent,
        // before a view fills anything in.
        let a_whole_read = crate::teaching::is_a_whole_read(&args);
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
        // **Whether the question's `where` is the view's own**: the caller named
        // none, and the view fills one in. A refusal of that question names the
        // view, because the caller never wrote the line it is about.
        let view_named = args.view.clone();
        let caller_named_scope = args.scope.is_some();
        let where_is_the_views = args.expression.is_none();
        let (args, view_filters, readiness_asked, view_overdue) = match args.view.clone() {
            None if args.inputs.is_some() => {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "inputs fill the inputs of a view, so send `view` beside them: recall \
                         a view's handle (`subject: \"view:ready\"`) to read what it takes"
                            .into(),
                    ),
                );
            }
            None => (args, Vec::new(), None, false),
            Some(named) => {
                let day = self.dated(None, args.sid.as_deref()).await?;
                match self.asked_by_name(&named, args, day).await {
                    Ok(filled) => filled,
                    Err(ViewStop::Said(refused)) => return Ok(*refused),
                    Err(ViewStop::Outage(e)) => return memory_declined("recall", e),
                }
            }
        };
        // **Readiness is a question about work, and only work answers it.** The
        // view selects the work kind; a caller that sent another kind beside it,
        // or named a subject of another kind, would be handed `ready` for things
        // that have no dependencies to wait on.
        if readiness_asked.is_some() {
            if args
                .kind
                .as_deref()
                .map(parse_kind)
                .transpose()?
                .is_some_and(|kind| kind != EntityKind::WORK)
            {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(
                        "the ready view answers for the work kind only: leave kind off, or send \
                         kind: work"
                            .into(),
                    ),
                );
            }
            if let Some(subject) = args.subject.as_deref()
                && EntityId::person(subject).kind_token() != EntityKind::WORK.as_token()
            {
                return memory_declined(
                    "recall",
                    MemoryError::InvalidQuery(format!(
                        "the ready view answers for the work kind only, and '{subject}' is not \
                         work: name a piece of work in subject, or leave subject off"
                    )),
                );
            }
        }
        // **A readiness ask with no kind means the work kind.** Only work is ever
        // ready or blocked, and a walk over every kind would count a person as
        // ready, so a view that asks it and selects nothing is asked of work. A
        // call that named a subject keeps the one it named.
        let args = if readiness_asked.is_some() && args.kind.is_none() && args.subject.is_none() {
            RecallArgs {
                kind: Some(EntityKind::WORK.as_token().to_string()),
                ..args
            }
        } else {
            args
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
        // **The one clock read, taken here whether or not a question named a
        // day.** What is held is asked as of a day like everything else, so a
        // call that named one for `overdue` asks about the same day here, and
        // the answer says which day it used either way.
        let today = self.dated(None, args.sid.as_deref()).await?;
        // **An expression is read here, once, against the asker's day**, so a
        // relative date means that day and a mistake is refused before any read.
        let keeping_expression = match args.follow.as_ref().and_then(|f| f.keeping.as_ref()) {
            Some(Keeping::Expression(text)) => match filter::expression::parse(text, today) {
                Ok(tree) => Some(tree),
                Err(refused) => return memory_declined("recall", refused),
            },
            _ => None,
        };
        let through_view = |refused: MemoryError| -> MemoryError {
            match (&view_named, refused) {
                (Some(name), MemoryError::InvalidQuery(words))
                    if where_is_the_views && args.expression.is_some() =>
                {
                    MemoryError::InvalidQuery(format!(
                        "the view '{name}' asks a question that cannot run: {words}. Send a \
                         where or a scope of your own beside the view, and it takes the \
                         place of the view's"
                    ))
                }
                (_, refused) => refused,
            }
        };
        let where_expression = match args.expression.as_deref() {
            None => None,
            Some(text) => match filter::expression::parse(text, today) {
                Ok(tree) => Some(tree),
                Err(refused) => return memory_declined("recall", refused),
            },
        };
        let where_scope = parse_scope(args.scope.as_deref())?;
        // **A `scope` the caller sent says how a `where` is asked**, so beside no
        // `where` (the view's own counts) it would be ignored without a word. A
        // walk's `keeping` expression has a refusal of its own just below.
        if caller_named_scope && where_expression.is_none() && keeping_expression.is_none() {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "scope says how a where is asked, and this call has no where: send one, \
                     or leave scope off. fields carry a scope of their own"
                        .into(),
                ),
            );
        }
        // **A walk's expression is asked of what each reached thing holds now.**
        // The call's `scope` would be silently dropped beside it, so it is
        // refused, naming the form that does take a record scope.
        if keeping_expression.is_some() && where_scope == graph::Scope::Record {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "a keeping expression is asked of what each reached thing holds now, and \
                     scope: record does not apply to it. To keep what a walk reaches by one \
                     record, send keeping as a list of key filters, each with scope: record"
                        .into(),
                ),
            );
        }
        let order = match args.order.as_ref() {
            None => None,
            Some(asked) => match filter::Order::asked(&asked.key, asked.direction.as_deref()) {
                Ok(order) => Some(order),
                Err(refused) => return memory_declined("recall", refused),
            },
        };
        let follow = args
            .follow
            .as_ref()
            .map(|f| -> Result<graph::Follow, McpError> {
                Ok(graph::Follow {
                    along: match (&f.relation, &f.shape) {
                        (Some(relation), _) => graph::Along::Relation(relation.trim().to_string()),
                        (None, Some(shape)) => graph::Along::Edge(parse_shape_filter(shape)?),
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
                    keeping: match (&f.keeping, &keeping_expression) {
                        (Some(Keeping::Filters(named)), _) => key_filters(named)?,
                        (_, Some(tree)) => vec![graph::FieldFilter::expression(
                            tree.clone(),
                            graph::Scope::Thing,
                        )],
                        _ => Vec::new(),
                    },
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
            status: wanted_status,
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
            // **A thing's own handle has no `#`**, and names the thing rather
            // than one of its records.
            (None, Some(handle)) if !handle.contains('#') => Some(graph::History {
                of: graph::Trace::Entity(EntityId::person(handle.trim())),
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
        // **A thing named for its history is the subject**, as an address is: a
        // `subject` beside it has to be the same thing.
        let entity_traced = match trace.as_ref().map(|history| &history.of) {
            Some(graph::Trace::Entity(named)) => Some(named.clone()),
            _ => None,
        };
        if let Some(named) = &entity_traced
            && let Some(asked) = args.subject.as_deref().map(EntityId::person)
            && &asked != named
        {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(format!(
                    "history_record names {named}, a thing, and asks for {asked}. Drop the \
                     subject — the handle names its own — or name the thing you meant"
                )),
            );
        }
        let query = view_run::Question::new(
            graph::Selection {
                subject: args
                    .subject
                    .as_deref()
                    .map(EntityId::person)
                    .or_else(|| entity_traced.clone()),
                kind: args.kind.as_deref().map(parse_kind).transpose()?,
                parent: args.parent.as_deref().map(EntityId::person),
                answers_type,
                candidates: None,
                candidates_exact: false,
                ordered_head: None,
                fields: {
                    // **The caller's own `fields` win whole, never merged item
                    // by item.** A caller that named a filter meant that
                    // filter; the view's own is a starting point, not
                    // something to add to. This is the one place that decides.
                    let mut asked = match &args.fields {
                        Some(named) => key_filters(named)?,
                        None => view_filters,
                    };
                    asked.extend(
                        where_expression
                            .map(|tree| graph::FieldFilter::expression(tree, where_scope)),
                    );
                    asked
                },
                order,
                near,
                asked_by,
                usage_reach: Default::default(),
                owed: view_overdue,
                ready: readiness_asked.is_some(),
                unreviewed: args.unreviewed == Some(true),
            },
            include,
            follow,
            trace,
        )
        .query;
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
        // **A bot's `working_on` is read through the one gate before the walk**, and
        // left out of what the walk reads for a bot whose session is gone, so a
        // filter, an order and the objects all see the same bot.
        let stale_working_on = self.stale_working_on_bots().await;
        query.masked = super::working_on::masked_working_on(&stale_working_on);
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
                graph::Trace::Key(_) | graph::Trace::Entity(_) => None,
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

        // **A view that narrows nothing is refused by name.** The call chose
        // nothing beside it either, so the whole store is what it asks for. The
        // generic refusal below lists `view` among the ways to choose, which sends
        // a caller who named a view round in a circle; this one names the view and
        // the keys that would narrow it. Checked after the address fill above, so
        // a view traced by a record's address is not refused.
        if let Some(named) = &view_named
            && query.select.narrows_nothing()
        {
            return memory_declined(
                "recall",
                MemoryError::ViewNarrowsNothing {
                    view: view_handle(named).to_string(),
                },
            );
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
        // **A key filter asked of the thing is a lookup, not a scan.** What a
        // thing holds is in the index as postings, so the things carrying the
        // key (and the value, when equality is asked) come from it, and the walk
        // reads those and no other. A walk (`follow`), a subject, a candidate
        // set already chosen, a filter asked of one record (the index holds what
        // a thing holds, not what its records say), a filter naming no key, and a
        // question about runs (which are not in the index as things) all keep the
        // read of everyone they always had.
        // **One function says what the index can answer for the filters**
        // (`Filter::routed`, rule 498): a union of conjunctions of the keys a
        // thing holds, and whether the carriers are exactly the things the
        // filters keep. A `where` leaves the same way a key filter does.
        let routed: Option<filter::Routed> = if query.follow.is_none()
            && query.select.subject.is_none()
            && query.select.candidates.is_none()
            && !query.select.kind.is_some_and(EntityKind::is_projected)
        {
            let asked: Vec<filter::Filter> = query
                .select
                .fields
                .iter()
                .filter(|filter| filter.scope == graph::Scope::Thing)
                .map(graph::FieldFilter::as_filter)
                .collect();
            (!asked.is_empty())
                .then(|| filter::Filter::All(asked).routed())
                .flatten()
        } else {
            None
        };
        let type_only = matches!(
            (
                &query.select.answers_type,
                &query.select.subject,
                query.select.kind
            ),
            (Some(_), None, None)
        );
        if type_only || routed.is_some() {
            let asking_a_type = if type_only {
                query.select.answers_type.clone()
            } else {
                None
            };
            // **One search for each conjunction, and the carriers are their
            // union.** A tree without an `||` is one search, as it was. **A term
            // the search index cannot hold is asked of the store**
            // (`Memory::keys_holding`), and a conjunction's carriers are the
            // things every one of its terms keeps.
            let conjunctions: Vec<Vec<filter::Term>> = match &routed {
                Some(routed) => routed.any_of.clone(),
                None => vec![Vec::new()],
            };
            let mut candidates: Vec<EntityId> = Vec::new();
            let mut ceiling_reached = false;
            let mut carriers_seen: std::collections::HashSet<EntityId> = Default::default();
            for terms in conjunctions {
                let (asked_of_the_store, indexed): (Vec<filter::Term>, Vec<filter::Term>) =
                    terms.into_iter().partition(|term| term.ask.is_some());
                let only_the_store = routed.is_some() && indexed.is_empty();
                // **The index is refreshed by a search, and a conjunction the
                // store answers alone asks none.** Belief in a candidate set is
                // asked of the index's coverage below, so a first read that
                // would have filled it asks the smallest search that does.
                let holding: Vec<HeldField> = if only_the_store {
                    let refreshing = (self.search.memory_coverage() != Coverage::Loaded)
                        .then(|| asked_of_the_store.first())
                        .flatten();
                    refreshing
                        .map(|term| {
                            vec![HeldField {
                                key: term.key.clone(),
                                value: None,
                            }]
                        })
                        .unwrap_or_default()
                } else {
                    indexed
                        .iter()
                        .map(|term| HeldField {
                            key: term.key.clone(),
                            value: term.value.clone(),
                        })
                        .collect()
                };
                let mut carriers: Option<std::collections::HashSet<EntityId>> = None;
                if !only_the_store || !holding.is_empty() {
                    let hits = match self
                        .search
                        .search(&SearchQuery {
                            answers_type: asking_a_type.clone(),
                            holding,
                            kind: query.select.kind,
                            limit: if only_the_store {
                                1
                            } else {
                                CARRIER_CANDIDATES_LIMIT
                            },
                            ..SearchQuery::default()
                        })
                        .await
                    {
                        Ok(hits) => hits,
                        Err(e) => return memory_declined("recall", e),
                    };
                    if !only_the_store {
                        ceiling_reached |=
                            hit_candidate_ceiling(hits.len(), CARRIER_CANDIDATES_LIMIT);
                        carriers = Some(
                            hits.into_iter()
                                .filter_map(|hit| match hit {
                                    Hit::Entity { entity, .. } => Some(entity.id),
                                    _ => None,
                                })
                                .collect(),
                        );
                    }
                }
                for term in &asked_of_the_store {
                    let Some(test) = term.ask.as_ref() else {
                        continue;
                    };
                    let kept = match self
                        .memory
                        .keys_holding(&term.key, test, query.select.kind)
                        .await
                    {
                        Ok(kept) => kept,
                        Err(e) => return memory_declined("recall", e),
                    };
                    carriers = Some(match carriers {
                        None => kept.into_iter().collect(),
                        Some(held) => kept.into_iter().filter(|id| held.contains(id)).collect(),
                    });
                }
                let mut carriers: Vec<EntityId> =
                    carriers.unwrap_or_default().into_iter().collect();
                carriers.sort();
                candidates.extend(
                    carriers
                        .into_iter()
                        .filter(|id| carriers_seen.insert(id.clone())),
                );
            }
            // **The candidates are believed only while the memory half reads
            // loaded.** An index that is behind the store (never read, or
            // holding an older version) may lack a carrier the store holds, and
            // the read of everyone cannot miss one. It is asked after the search,
            // which refreshes the half, so a first read that fills it is counted.
            let believed = self.search.memory_coverage() == Coverage::Loaded;
            let capped = ceiling_reached;
            if type_only && believed {
                candidates_capped = Some(capped);
            }
            // **A key filter's answer does not change with the index's depth**: a
            // candidate list cut at the ceiling would drop carriers the full read
            // finds, so a capped one is not used and the read of everyone stands.
            // **A `where` that finds no carrier is not answered from the index.**
            // An empty answer is where a caller needs to be told why: a key no
            // thing holds is refused, and a key only the records hold names the
            // scope that asks it. Both are decided by reading, so the read of
            // everyone stands.
            let asked_a_where = query
                .select
                .fields
                .iter()
                .any(|filter| filter.tree.is_some());
            let finds_nobody = candidates.is_empty() && asked_a_where;
            if believed && (type_only || !capped) && !finds_nobody {
                // **Exact when the index decided every filter and was believed
                // in full**: a type the index asked is the type's own keys, a
                // key filter is the keys the thing holds, and nothing is left
                // for a record or a type the index did not ask.
                // **A term a declaration licenses is exact where every thing is
                // licensed.** Under a kind named the walk has refused a key that
                // kind does not license. With none named each thing is licensed by
                // its own kind's declaration, which the store does not read: the
                // candidates are exact when every kind among them is licensed, and
                // otherwise the walk decides each one and counts what it leaves out.
                let licensed = match &routed {
                    Some(routed)
                        if query.select.kind.is_none()
                            && routed.any_of.iter().flatten().any(|term| {
                                matches!(
                                    term.ask,
                                    Some(filter::Test::Contains(_) | filter::Test::Order(..))
                                )
                            }) =>
                    {
                        match self.memory.declared_types_shared().await {
                            Ok(declared) => {
                                let kinds: std::collections::BTreeSet<&str> = candidates
                                    .iter()
                                    .filter_map(|id| id.kind())
                                    .map(|kind| kind.as_token())
                                    .collect();
                                routed.licensed_for_every(
                                    &declared,
                                    &kinds.into_iter().collect::<Vec<_>>(),
                                )
                            }
                            Err(e) => return memory_declined("recall", e),
                        }
                    }
                    _ => true,
                };
                query.select.candidates_exact = !capped
                    && licensed
                    && routed.as_ref().is_none_or(|routed| routed.exact)
                    && (type_only || query.select.answers_type.is_none());
                query.select.candidates = Some(candidates);
            }
        }

        // **The frame an intake moment is read as a day in**: the server's clock
        // and the asking run's zone, else the instance's, else UTC.
        let day_zone = match caller.as_ref().and_then(Caller::zone) {
            Some(zone) => zone,
            None => self.unzoned_frame().await,
        };
        let day_frame = DayFrame {
            clock: *self.clock(),
            zone: &day_zone,
        };
        // **A session joins the walk only when the query could reach one** —
        // kept off the ordinary path so a lookup that never mentions a
        // session does not pay for a `Sessions` read it did not ask for.
        // **What joins is what the answer can show, plus a count of the rest**,
        // and never the whole board: the caller's own runs, the one run a
        // subject names (which may be another bot's, and is then refused as
        // theirs), and the number of runs that belong to others. The walk's
        // owner check (an owned object answers its owner alone — proven on this
        // exact shape at `19fca07`) is still what keeps another bot's runs out
        // of the answer; it is the ONLY guard now, not a redundant second one,
        // because nothing upstream of it scopes by bot any more.
        // **The count is the walk's own `withheld`, taken without loading the
        // runs**: the runs of other bots that answer the selection's parent. It is
        // owed to a caller that BROWSES runs, who asked for all of them and was
        // shown its own. A subject names one thing, so naming a run says nothing of
        // how many other runs there are: the caller's own comes back with nothing
        // withheld, and another bot's is refused as theirs.
        let wants_sessions = query.select.kind == Some(EntityKind::SESSION)
            || query
                .select
                .subject
                .as_ref()
                .is_some_and(|s| s.kind() == Some(EntityKind::SESSION));
        let browses_runs =
            query.select.subject.is_none() && query.select.kind == Some(EntityKind::SESSION);
        // **How many runs the walk was not shown and must still be told exist.**
        // For a caller with no identity that is every run, and it owns none; for
        // one with an identity it is the other bots' runs. Read for a browse of
        // runs only.
        let mut runs_counted_not_shown = 0usize;
        let session_docs: Vec<jojobot_domain::memory::search::DocScan> = if wants_sessions {
            let read = async {
                let counts = if browses_runs {
                    self.sessions.run_counts().await?
                } else {
                    Default::default()
                };
                let Some(caller) = caller.as_ref() else {
                    return Ok((Vec::new(), counts.values().sum::<usize>()));
                };
                let mut runs = self.sessions.summaries_of(&caller.bot).await?;
                if let Some(subject) = query.select.subject.as_ref()
                    && subject.kind() == Some(EntityKind::SESSION)
                {
                    let named = jojobot_domain::session::SessionId(subject.slug().to_string());
                    if !runs.iter().any(|run| run.id == named)
                        && let Some(run) = self.sessions.summary_of(&named).await?
                    {
                        runs.push(run);
                    }
                }
                let answers_parent = |bot: &EntityId| {
                    query
                        .select
                        .parent
                        .as_ref()
                        .is_none_or(|parent| parent == bot)
                };
                let others: usize = counts
                    .iter()
                    .filter(|(bot, _)| **bot != caller.bot && answers_parent(bot))
                    .map(|(_, n)| *n)
                    .sum();
                // **The days a run was served on ride only on a read of ONE
                // run**, so a browse of every run does not grow with each day of
                // each of them.
                let served = if query.select.subject.is_some() {
                    self.sessions.served_days().await?
                } else {
                    Vec::new()
                };
                // **The calls of a run ride the same way**, with the verbs.
                let called = if query.select.subject.is_some() {
                    let names: Vec<String> =
                        runs.iter().map(|run| run.run_name().to_string()).collect();
                    self.sessions.run_calls(Some(&names)).await?
                } else {
                    Vec::new()
                };
                let docs = runs
                    .iter()
                    .map(|run| {
                        let mut doc = jojobot_domain::session::projected_summary(run);
                        doc.fields.extend(jojobot_domain::session::served_on_keys(
                            served
                                .iter()
                                .filter(|held| held.run == run.id)
                                .map(|held| (held.day, held.chars)),
                        ));
                        doc.fields.extend(jojobot_domain::usage::call_fields(
                            &jojobot_domain::usage::run_verbs(&called, run.run_name()),
                        ));
                        // **The day the run started**: what `near` places a run
                        // by, since a run holds no claim for any clock to read.
                        // **The day the run stated, when it stated one**, because
                        // a run played on a stated day began on that day whatever
                        // the clock said when it did. A run that stated none is
                        // placed by the instant it began, in the asking run's
                        // frame. Read here, where the zone is known, and not in
                        // the walk, which has none.
                        doc.fields.insert(
                            jojobot_domain::session::STARTED_ON.to_string(),
                            run.started_on
                                .unwrap_or_else(|| day_frame.day(run.started_at))
                                .to_string(),
                        );
                        doc
                    })
                    .collect();
                Ok::<_, jojobot_domain::session::SessionError>((docs, others))
            }
            .await;
            match read {
                // 🚨 **A caller with no identity gets nothing but the count.** A
                // run's entity is named for its focus text and owned by a bot,
                // and the walk's near-miss screen and its not-yours answer both
                // say so. A caller with no identity that reached them could read
                // another bot's focus by naming a near miss of a run's id, and
                // its owner by naming the id. Skipping the read altogether made
                // an anonymous "nothing here" read exactly like an empty index,
                // so the runs are counted and nothing else of them is kept.
                Ok((docs, not_shown)) => {
                    runs_counted_not_shown = not_shown;
                    docs
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

        // **The weeks of usage join the walk the way runs do**, only when the
        // query could reach one. Which of them the caller may read is the chart's
        // answer, handed to the walk as the selection's reach.
        let wants_usage = query.select.kind == Some(EntityKind::USAGE)
            || query
                .select
                .subject
                .as_ref()
                .is_some_and(|s| s.kind() == Some(EntityKind::USAGE));
        let mut projected_docs = session_docs.clone();
        if wants_usage {
            match self.usage_docs(caller.as_ref(), day_frame, today).await {
                Ok((docs, reach)) => {
                    projected_docs.extend(docs);
                    query.select.usage_reach = reach;
                }
                Err(usage::Unread::Runs(e)) => {
                    return session_declined(
                        e,
                        caller.as_ref().map_or("", |caller| caller.sid.as_str()),
                    );
                }
                Err(usage::Unread::Chart(e)) => return memory_declined("recall", e),
            }
        }
        // **Only the objects that will fit are read.** A browse by kind is decided
        // by the index alone, and what it costs is reading each object, so the walk
        // is told where the answer starts and how much room it has, and reads
        // objects in answer order until the room is spent. The cost it is told is
        // a floor: what an object renders to, with the smallest `held` block every
        // object carries. A floor can only read a few objects more than fit, never
        // fewer, and the fit below cuts exactly. A question that needs every object
        // to answer — an overdue day, the values in use, backing, a status, a
        // lineage — reads them all as it always did.
        let offset = args.offset.map_or(0, |o| o as usize);
        // **On a run named alone, `offset` reads on through its history.** The
        // history is cut newest first under the ceiling, and `offset` counts
        // what was already read of it. A run holds no records, so on such a call
        // there are no facts for `offset` to count, and one offset is enough.
        let pages_history = args.history.is_some()
            && query
                .select
                .subject
                .as_ref()
                .is_some_and(|subject| subject.kind() == Some(EntityKind::SESSION));
        let narrowing = only_keys.as_deref().map(|keys| KeyNarrowing {
            keys,
            by_view: keys_from_view,
        });
        // **Whether the records this answer renders carry their details**: only
        // when the call asked.
        let details_asked = args.details.unwrap_or(false);
        let ships_held = ships_held(args.held, include.facts, names_keys(narrowing));
        let held_floor = if ships_held {
            [entitlement::Widen::Never, entitlement::Widen::WhenEmpty]
                .into_iter()
                .map(|widen| crate::answer::served_len(&held_json(&[], today, widen)) + 1)
                .min()
                .unwrap_or_default()
        } else {
            0
        };
        let floor = |object: &graph::Object| -> usize {
            crate::answer::served_len(&object_json(
                object,
                include,
                today,
                narrowing,
                day_frame,
                details_asked,
            )) + 1
                + held_floor
        };
        let window = (as_of.is_none()
            && args.values.is_none()
            && !args.backing.unwrap_or(false)
            && wanted_status.is_none()
            && built_on_address.is_none())
        .then(|| graph::Window {
            offset,
            budget: jojobot_domain::text::ANSWER_CEILING
                .saturating_sub(crate::answer::STATUS_BAR_ROOM)
                .saturating_sub(crate::answer::widest_not_shown("objects")),
            size_of: &floor,
        });
        // **A readiness question is narrowed before the window cuts.** A window
        // spent during the walk is spent on finished work and on work that is
        // blocked, and the ready work behind them never reaches the answer; the
        // window is applied after the narrowing, over what is ready.
        let (walk_window, deferred_window) =
            if readiness_asked.is_some() || args.unreviewed == Some(true) {
                (None, window)
            } else {
                (window, None)
            };
        // **An ordering by a date or a number is served from the front of the
        // order when the store keeps its values by key.** The window reads the
        // objects in answer order until its room is spent, so the front of the
        // order is all it needs: a page and the most a page can hold, and the ties
        // of the last. The walk reads what it always read when the page runs past
        // a front that was cut.
        if walk_window.is_some()
            && query.follow.is_none()
            && query.history.is_none()
            && query.select.takes_an_ordered_head()
            && let Some(order) = query.select.order.clone()
            && let Ok(declared) = self.memory.declared_types_shared().await
            && let Some(holds) = order.reads_as(&declared, query.select.kind.map(|k| k.as_token()))
        {
            let among = if query.select.candidates_exact {
                query.select.candidates.clone()
            } else {
                None
            };
            match self
                .memory
                .ordered_first_k(
                    &order.key,
                    holds,
                    order.direction,
                    query.select.kind,
                    among.as_deref(),
                    offset.saturating_add(MOST_OBJECTS_A_PAGE_HOLDS),
                )
                .await
            {
                Ok(head) => query.select.ordered_head = Some(head),
                Err(e) => return memory_declined("recall", e),
            }
        }
        let graph::Selected {
            objects: mut found,
            ordered,
            unlicensed,
            withheld,
            unplaced,
            archived_excluded,
            merged_excluded,
            decisions_archived,
            decisions_replaced,
            skipped: walked_past,
            subject_children,
            left_over: walked_left,
        } = match view_run::select(self.memory.as_ref(), &projected_docs, &query, walk_window).await
        {
            Ok(answer) => answer,
            Err(e) => return memory_declined("recall", through_view(e)),
        };
        // The runs the walk was not shown, counted with the ones it withheld, on a
        // browse of runs and never when a subject is named.
        let withheld = if browses_runs {
            withheld + runs_counted_not_shown
        } else {
            withheld
        };
        // **An empty browse of runs says how many of the caller's OWN a filter
        // left out**, apart from those withheld as another identity's. A filter no
        // run carries the key of matches nothing, and a caller reading "owned by
        // another identity" beside an empty list takes its own run for one that
        // was taken from it. Counted over the runs this caller owns, which is
        // every run the walk would have shown it without the filter.
        let own_runs_excluded = match caller.as_ref() {
            Some(caller)
                if wants_sessions && query.select.subject.is_none() && found.is_empty() =>
            {
                session_docs
                    .iter()
                    .filter(|doc| doc.owner.as_ref() == Some(&caller.bot))
                    .count()
            }
            _ => 0,
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
        // **A run's own history**, when `history` traced a field of it or its
        // journal. The key is the same argument a thing's key is traced with.
        if let Some(traced) = args.history.as_deref().map(str::trim) {
            let caller_sid = caller.as_ref().map(|c| c.sid.as_str()).unwrap_or_default();
            let skip = if pages_history { offset } else { 0 };
            if let Some(refused) = self
                .fill_run_history(&mut found, traced, history_most, skip, caller_sid)
                .await
            {
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
        // never opened, finished, or a thing no carrier speaks for — and
        // this says how many. **`overdue_excluded_because` is the breakdown,
        // served beside it**, and counts none of them differently.
        let mut overdue_excluded: Option<usize> = None;
        let mut overdue_excluded_because: Option<
            std::collections::BTreeMap<
                view_run::LeftOut,
                std::collections::BTreeMap<String, usize>,
            >,
        > = None;
        // **How many days past the day asked about each object fell due**,
        // aligned with `found` in its final, already-sorted order. `None`
        // when the call asked no overdue question at all, exactly as
        // `overdue_as_of` and `overdue_excluded` already read — a caller that
        // asked nothing is told nothing, rather than a distance for a
        // question it never asked.
        let mut overdue_by_days: Option<Vec<Option<i64>>> = None;
        // **Why each object that cannot say when it falls due cannot**, aligned
        // with `found` the same way. Only the carrier that answers for the object
        // knows which keys of its schedule are missing or unreadable.
        let mut unreadable_because: Option<Vec<Option<attention::Unreadable>>> = None;
        // **The read compares a moment to a day and computes none of them.**
        // Which moment a thing falls due at is its carrier's answer, so a
        // second sort of owed thing lands by answering there rather than by
        // this line growing a branch — and a thing whose keys no carrier speaks
        // for owes nothing, which keeps a person who holds none of them out of
        // an answer about what is late. **The page of a view runs these same passes** (rule 444).
        //
        // **What can be started, when the view asked.** Asked of the work the
        // selection found, against the things its `depends_on` names, and of
        // nothing else: an ordinary read never reaches here. Work that is not
        // ready is left out and counted, **except the one thing a call named**,
        // which always comes back so a caller can read why. The count names the
        // call that does.
        let asks = view_run::Asks {
            overdue: as_of,
            // **A view measures from the run's own day**, so a thing not yet due
            // says how far off it is; a `recall` of its own measures from the day
            // it asked about.
            measure_from: (view_overdue && as_of.is_some()).then_some(today),
            ready: readiness_asked.map(|ask| view_run::Readiness {
                ask,
                named: query.select.subject.clone(),
            }),
            unreviewed: args.unreviewed == Some(true),
        };
        let carriers = self.carriers();
        // **What the operator mapped their status words to**, read once and
        // only for an overdue question: a word mapped to `done` finishes a work
        // item or project here too.
        let instance = match as_of {
            Some(_) => match self.instance_fields().await {
                Ok(instance) => instance,
                // **A record that cannot be read is the verb's store failure**, never
                // an instance that mapped no words (rule 130).
                Err(e) => return memory_declined("recall", e),
            },
            None => Default::default(),
        };
        let narrowed = match view_run::narrow(
            &*self.memory,
            &mut found,
            &asks,
            &view_run::Context {
                carriers: &carriers,
                instance: &instance,
            },
        )
        .await
        {
            Ok(narrowed) => narrowed,
            Err(e) => return memory_declined("recall", e),
        };
        if let Some(pass) = narrowed.owed {
            overdue_excluded = Some(pass.excluded);
            overdue_excluded_because = Some(pass.excluded_because);
            overdue_by_days = Some(pass.by_days);
            unreadable_because = Some(pass.unreadable_because);
        }
        let mut readiness: Option<Vec<crate::memory::ready::Verdict>> = None;
        let mut ready_left_out: Option<usize> = None;
        if let Some(pass) = narrowed.ready {
            readiness = Some(pass.verdicts);
            ready_left_out = Some(pass.left_out);
        }
        let (skipped, left_over) = match &deferred_window {
            Some(window) => {
                // **Restored only if the question had one**: a window deferred for
                // another narrowing (the review queue) has no verdicts to align,
                // and an empty list put back would be indexed per object.
                let asked_readiness = readiness.is_some();
                let mut verdicts = readiness.take().unwrap_or_default();
                let outside = view_run::cut_to_window(&mut found, &mut [&mut verdicts], window);
                if asked_readiness {
                    readiness = Some(verdicts);
                }
                outside
            }
            None => (walked_past, walked_left),
        };
        // **The context a reader did not ask for, and the whole point of the
        // card.** A session that pulls a thing is told who holds what that
        // gets them in, ranked, without knowing there was anything to ask.
        let hidden_working_on = super::working_on::stale_in(&found, &stale_working_on);
        let mut backing = Vec::with_capacity(found.len());
        if args.backing.unwrap_or(false) {
            for object in &found {
                match self.memory.backing(&object.entity.id).await {
                    Ok(mut held) => {
                        if hidden_working_on.contains(&object.entity.id) {
                            held.remove(jojobot_domain::memory::kinds::WORKING_ON);
                        }
                        backing.push(Some(held))
                    }
                    Err(e) => return memory_declined("recall", e),
                }
            }
        }
        let mut held = Vec::with_capacity(found.len());
        for object in &found {
            // A read that names keys and did not ask for `held` reads none of it.
            if !ships_held {
                held.push(None);
                continue;
            }
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
            held.push(Some(held_json(
                &entitlement::ranked(&pointing, &object.entity.id, day, &keys, widen),
                day,
                widen,
            )));
        }
        // **Lineage, when the call asked for it.** It is a claim-level question
        // rather than an object-level one, so it rides beside the objects
        // rather than inside them.
        let standing_on = match &built_on_address {
            None => None,
            Some(source) => {
                match self.memory.built_on(source).await {
                    Ok(claims) => {
                        let mut lineage = serde_json::json!({
                            "source": source.to_string(),
                            "count": claims.len(),
                            // No revision count: lineage is read straight off the
                            // store rather than through a walk, so there is no
                            // `fact_revisions` map beside it to look one up in.
                            "claims": claims
                                .iter()
                                .map(|fact| wire::listed_json(fact, today, None, details_asked))
                                .collect::<Vec<_>>(),
                        });
                        let left_out = wire::left_out_of(claims.iter(), details_asked);
                        if left_out > 0 {
                            lineage[wire::OBJECT_LEFT_OUT] = left_out.into();
                        }
                        let at = wire::addresses_left_out(claims.iter(), details_asked);
                        if !at.is_empty() {
                            lineage[wire::OBJECT_LEFT_OUT_AT] = at.into();
                        }
                        Some(lineage)
                    }
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
        // **Which pages this read leaves out hold prose**, the answer's objects
        // and the ones a walk reached alike, asked of the memory one bit per
        // object rather than by reading each page. A bot's page is its charter
        // and a run's is its chronology, and each says it was left out its own
        // way, above and below. A folded handle serves no page at all.
        let mut pages_left_out: std::collections::HashSet<EntityId> =
            std::collections::HashSet::new();
        // **A page the memory could not be asked about is neither counted nor
        // passed over** (rule 130): it is named on the answer.
        let mut pages_unread: std::collections::HashSet<EntityId> =
            std::collections::HashSet::new();
        if !asked_prose {
            let mut pending: Vec<&graph::Object> = found.iter().collect();
            while let Some(object) = pending.pop() {
                pending.extend(object.connected.iter());
                if object.entity.merged_into.is_some()
                    || matches!(
                        object.entity.id.kind(),
                        Some(EntityKind::BOT) | Some(EntityKind::SESSION)
                    )
                    || pages_left_out.contains(&object.entity.id)
                {
                    continue;
                }
                match self.memory.has_prose(&object.entity.id).await {
                    Ok(true) => {
                        pages_left_out.insert(object.entity.id.clone());
                    }
                    Ok(false) => {}
                    Err(error) => {
                        tracing::warn!(%error, page = %object.entity.id, "a page could not be asked for prose");
                        pages_unread.insert(object.entity.id.clone());
                    }
                }
            }
        }
        // **A numbered collection says the number its next member takes**, read
        // for each object that carries `numbers_by`. The largest number held in
        // the whole subtree, archived members included, plus one: a read, not
        // an allocation.
        let mut numbering_views: std::collections::HashMap<EntityId, u64> =
            std::collections::HashMap::new();
        for object in &found {
            if object
                .fields
                .contains_key(jojobot_domain::memory::numbered::NUMBERS_BY)
                && let Ok(Some(numbering)) = self.memory.numbering_of(&object.entity.id).await
            {
                numbering_views.insert(object.entity.id.clone(), numbering.next_number);
            }
        }
        // **A list says its open entries, in order.** Each is a claim on the
        // list; the order is the key the server minted when it was placed, and
        // the state is computed, never stored on the entry. Read for every list
        // the answer holds, whether or not facts were asked for, because the
        // entries are what a list is.
        let mut list_entries: std::collections::HashMap<
            EntityId,
            serde_json::Map<String, serde_json::Value>,
        > = std::collections::HashMap::new();
        for object in &found {
            if object.entity.id.kind() != Some(EntityKind::LIST) {
                continue;
            }
            let Ok(held) = self.memory.recall(&object.entity.id).await else {
                continue;
            };
            let setting = jojobot_domain::memory::entries::Setting::of(&object.fields);
            let seen = jojobot_domain::memory::entries::view(&held, setting);
            let mut read = serde_json::Map::new();
            // **A linked entry shows the work's own state beside it**, read now and
            // stored nowhere: position is not readiness.
            let counted = jojobot_domain::memory::entries::counted_links(&held, setting);
            // **Only a block the caller asked for is read**, so a read that names
            // other keys is not failed by a block it leaves out (rule 130). A
            // block left unread keeps its key, empty, so the note on what the
            // read left out still names it.
            let ticks_asked = args.ticks == Some(true);
            let wants = |key: &str| list_block_wanted(narrowing, key, ticks_asked);
            // **A read of the linked work that fails is the verb's store failure**,
            // never a list that counts only the work it could read (rule 130).
            let linked = if wants("entries") || wants("progress") {
                match self.linked_state(&counted).await {
                    Ok(linked) => linked,
                    Err(e) => return memory_declined("recall", e),
                }
            } else {
                Default::default()
            };
            // **A plan says where its entries went.** A list that covers a period
            // is a plan, and carrying an entry into another plan is a capture that
            // names it in `derived_from`; the source reads as carried by the
            // inbound read of that pointer, never by a flag stored on it.
            let span = object
                .fields
                .get("covers")
                .and_then(|covers| jojobot_domain::memory::types::period_span(covers.trim()));
            let mut carried_to: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            if span.is_some() && wants("entries") {
                for entry in &seen.entries {
                    let Ok(built) = self.memory.built_on(&entry.fact.address()).await else {
                        continue;
                    };
                    let to: Vec<String> = built
                        .iter()
                        .filter(|fact| {
                            fact.status == jojobot_domain::memory::FactStatus::Active
                                && jojobot_domain::memory::entries::is_entry(fact)
                        })
                        .map(|fact| fact.address().to_string())
                        .collect();
                    if !to.is_empty() {
                        carried_to.insert(entry.fact.address().to_string(), to);
                    }
                }
            }
            let shown: Vec<serde_json::Value> = seen
                .entries
                .iter()
                .map(|entry| {
                    let mut shown = serde_json::json!({
                        "address": entry.fact.address().to_string(),
                        "text": entry.fact.content,
                        "state": entry.state,
                    });
                    if let Some(edge) = &entry.fact.edge {
                        shown["link"] = super::wire::edge_json(edge);
                        if let Some(work) = linked.get(&edge.object) {
                            shown["linked"] = work.as_read();
                        }
                    }
                    if span.is_some() {
                        if let Some(source) = &entry.fact.derived_from {
                            shown["carried_from"] = source.to_string().into();
                        }
                        if let Some(to) = carried_to.get(&entry.fact.address().to_string()) {
                            shown["carried_to"] = to.clone().into();
                        }
                    }
                    shown
                })
                .collect();
            read.insert("entries".into(), shown.into());
            // **What the period holds**, worked out now by the passes the due and
            // ready views run, and stored nowhere.
            if let Some((from, to)) = span {
                if !wants("plan") {
                    read.insert("plan".into(), serde_json::Value::Null);
                } else {
                    match self
                        .plan_of_period(object.fields["covers"].trim(), from, to)
                        .await
                    {
                        Ok(plan) => {
                            read.insert("plan".into(), plan);
                        }
                        Err(e) => return memory_declined("recall", e),
                    }
                }
            }
            // **How far along the work its entries link is**, counted over every
            // entry that is not dropped, in view or not.
            if !wants("progress") {
                if !counted.is_empty() {
                    read.insert("progress".into(), serde_json::Value::Null);
                }
            } else if let Some(progress) = super::list_linked::Progress::of(&counted, &linked) {
                read.insert("progress".into(), progress.as_read());
            }
            // **How far along the list is by its own ticks**, counted from the same
            // states the entries above show, and not the same as `progress`.
            // **And the day of the newest tick that stands**, so a list nobody has
            // touched for a month reads as one. Both come from the one function the
            // page of a view that lists lists calls.
            super::list_ticks::TickRead::of(&held, setting).write_into(&mut read);
            // **What the open view left out is said, with the call that reads it.**
            if seen.done_left_out + seen.dropped_left_out > 0 {
                read.insert(
                    "entries_left_out".into(),
                    serde_json::json!({
                        "done": seen.done_left_out,
                        "dropped": seen.dropped_left_out,
                        "read_with": "recall this list again with ticks: true; every tick \
                                      names its entry and the act",
                    }),
                );
            }
            // **The ticks are their own read**, newest first and in parts, never
            // part of the default answer: a daily rotation writes a year of them.
            if args.ticks == Some(true) {
                let all: Vec<_> = jojobot_domain::memory::entries::active_ticks(&held)
                    .into_iter()
                    .rev()
                    .collect();
                let from = args
                    .ticks_offset
                    .map_or(0, |offset| offset as usize)
                    .min(all.len());
                let most = args.history_most.map_or(20, |most| most as usize);
                let page: Vec<serde_json::Value> = all
                    .iter()
                    .skip(from)
                    .take(most)
                    .map(|(tick, act)| {
                        let entry = &tick.fields[jojobot_domain::memory::entries::TICK_ENTRY];
                        serde_json::json!({
                            "address": tick.address().to_string(),
                            "entry": format!("{}#{}", object.entity.id, entry.trim()),
                            "act": act.as_token(),
                            "at": tick
                                .inserted_at
                                .map_or_else(|| tick.recorded_at.to_string(), |at| at.to_string()),
                            "session": tick
                                .fields
                                .get(jojobot_domain::memory::entries::TICK_SESSION)
                                .cloned()
                                .unwrap_or_default(),
                            "note": tick.content,
                        })
                    })
                    .collect();
                let after = from + page.len();
                read.insert("ticks".into(), page.into());
                read.insert("ticks_total".into(), all.len().into());
                if after < all.len() {
                    read.insert(
                        "ticks_left_out".into(),
                        serde_json::json!({
                            "count": all.len() - after,
                            "read_with": format!(
                                "recall this list again with ticks: true and ticks_offset: {after}"
                            ),
                        }),
                    );
                }
            }
            list_entries.insert(object.entity.id.clone(), read);
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
        let mut room_views: std::collections::HashMap<EntityId, super::room::RoomView> =
            std::collections::HashMap::new();
        if include.facts {
            for object in &found {
                if object.entity.id.kind() != Some(EntityKind::BOT) {
                    continue;
                }
                if let Some(view) = self
                    .count_room_of(&object.entity.id, &object.facts, &object.fields)
                    .await
                    && !view.ages_unread
                    && view.count.worth_a_block()
                {
                    room_views.insert(object.entity.id.clone(), view);
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
                object.facts.retain(|fact| graph::lists(Some(want), fact));
                let removed = before - object.facts.len();
                object.facts_held = object.facts_held.saturating_sub(removed);
                left_out_by_status.insert(object.entity.id.clone(), removed);
            }
        }
        // **A thing's review mark is defrag's bookkeeping**, left out of the
        // listing unless asked for, and the object says how many it left out.
        let mut review_marks_left_out = std::collections::HashMap::<EntityId, usize>::new();
        // **And so is the record of the release notes a bot was told**, counted
        // apart, because it is a different thing left out.
        let mut ledger_left_out = std::collections::HashMap::<EntityId, usize>::new();
        if !args.review_marks.unwrap_or(false) {
            for object in found.iter_mut() {
                let before = object.facts.len();
                object
                    .facts
                    .retain(|fact| !jojobot_domain::memory::review::is_review_mark(&fact.fields));
                let marks = before - object.facts.len();
                let before = object.facts.len();
                object.facts.retain(|fact| {
                    !jojobot_domain::memory::review::is_release_notes_record(&fact.fields)
                });
                let ledgers = before - object.facts.len();
                object.facts_held = object.facts_held.saturating_sub(marks + ledgers);
                if marks > 0 {
                    review_marks_left_out.insert(object.entity.id.clone(), marks);
                }
                if ledgers > 0 {
                    ledger_left_out.insert(object.entity.id.clone(), ledgers);
                }
            }
        }
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
            // **Why those were left out, by kind, and the call that lists them.**
            // Absent when nothing was left out. The groups add up to the count.
            "overdue_excluded_because": overdue_excluded_because
                .as_ref()
                .and_then(left_out::overdue_excluded_because),
            // **How many pieces of work the ready question left out, and the
            // call that says why.** Absent when no readiness was asked, and
            // when nothing was left out.
            // **One line beside the ready list**, on the ready view and nowhere
            // else: what ready means, and the argument that narrows it.
            "ready_note": readiness_asked.map(left_out::ready_note),
            "ready_left_out": ready_left_out
                .and_then(|left| left_out::ready_left_out(left, readiness_asked)),
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
            // **And how many it left out because they were folded into another
            // thing**, apart from the archived ones: the handle still answers,
            // naming where it went, but a browse does not offer it.
            "merged_excluded": merged_excluded,
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
                    // **A standing decision serves its words whether or not the
                    // caller asked for records**: the words are the decision.
                    let include = if o.standing_for.is_some() {
                        graph::Include {
                            facts: true,
                            ..include
                        }
                    } else {
                        include
                    };
                    let mut rendered = object_json(
                        o,
                        include,
                        today,
                        narrowing,
                        day_frame,
                        details_asked,
                    );
                    if let Some(next) = numbering_views.get(&o.entity.id) {
                        rendered["next_number"] = (*next).into();
                    }
                    if let Some(read) = list_entries.get(&o.entity.id) {
                        let ticks_asked = args.ticks == Some(true);
                        let mut blocks_left_out: Vec<&str> = Vec::new();
                        for (key, value) in read {
                            if list_block_wanted(narrowing, key, ticks_asked) {
                                rendered[key.as_str()] = value.clone();
                            } else if !blocks_left_out.contains(&list_block_key(key)) {
                                blocks_left_out.push(list_block_key(key));
                            }
                        }
                        // **A view that narrowed the blocks says so on the object**,
                        // in the sentence its fields already use.
                        if !blocks_left_out.is_empty()
                            && narrowing.is_some_and(|narrowed| narrowed.by_view)
                        {
                            let mut left_out: Vec<&str> = o
                                .fields
                                .keys()
                                .map(String::as_str)
                                .filter(|key| {
                                    !wants_key(narrowing, key)
                                        && !jojobot_domain::memory::review::is_bookkeeping_key(key)
                                })
                                .collect();
                            left_out.extend(blocks_left_out);
                            rendered["fields_left_out"] =
                                fields_left_out_text(&left_out, narrowing).into();
                        }
                    }
                    if let Some(thread) = &o.standing_for {
                        rendered[STANDING_DECISION_OF] = thread.as_str().into();
                    }
                    // **A decision another replaced says so, with the new words.**
                    if let Some(replaced) = &o.replaced_by {
                        rendered["replaced_by"] = serde_json::json!({
                            "handle": replaced.handle.as_str(),
                            "words": replaced.words,
                        });
                        // **When it was replaced in its turn, the one that stands
                        // now.** Absent when the direct replacer stands.
                        if let Some(standing) = &replaced.standing {
                            rendered["standing_replacement"] = serde_json::json!({
                                "handle": standing.handle.as_str(),
                                "words": standing.words,
                            });
                        }
                    }
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
                    if let Some(removed) = review_marks_left_out.get(&o.entity.id) {
                        rendered["review_marks_left_out"] = serde_json::json!({
                            "count": removed,
                            "how_to_read": "recall again with review_marks: true to list them",
                        });
                    }
                    if let Some(removed) = ledger_left_out.get(&o.entity.id) {
                        rendered["release_notes_record_left_out"] = serde_json::json!({
                            "count": removed,
                            "how_to_read": "the record of the release notes this bot was told; \
                                            recall again with review_marks: true to list it",
                        });
                    }
                    // **Present, with a null value, on the one object this
                    // cannot measure — never absent.** Absent would read as
                    // "you did not ask", which is a different claim from
                    // "nothing here can say".
                    //
                    // **A thing not yet due says how far off it is** (`due_in_days`),
                    // never a lateness below zero. Only a view that asks about a
                    // later day than the run's own can reach it.
                    if let Some(distances) = &overdue_by_days {
                        match distances[idx] {
                            Some(days) if days < 0 => rendered["due_in_days"] = (-days).into(),
                            Some(days) => rendered["overdue_by_days"] = days.into(),
                            None => rendered["overdue_by_days"] = serde_json::Value::Null,
                        }
                    }
                    if let Some(all) = &readiness {
                        crate::memory::ready::render(&mut rendered, &all[idx]);
                    }
                    // **Beside the distance it cannot give**: which keys of the
                    // schedule the thing lacks or holds unreadably. Absent on an
                    // object that has a day.
                    if let Some(reasons) = &unreadable_because
                        && let Some(why) = &reasons[idx]
                    {
                        rendered["unreadable_because"] = serde_json::json!({
                            "lacks": why.lacks,
                            "will_not_parse": why.will_not_parse,
                        });
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
                    // **A run's chronology is its prose**, and a run that has
                    // written a beat has some. Marked here, counted once for the
                    // answer by `say_details_left_out`. A bot's page is its
                    // charter, which says it was left out on the object itself.
                    if !asked_prose
                        && o.entity.id.kind() == Some(EntityKind::SESSION)
                        && o.fields
                            .get("beats")
                            .and_then(|beats| beats.parse::<u64>().ok())
                            .is_some_and(|beats| beats > 0)
                    {
                        rendered[wire::OBJECT_PROSE_LEFT_OUT] = 1.into();
                    }
                    // **Any other thing's page is its prose**, marked when it
                    // holds some and the caller did not ask for it.
                    mark_pages(&mut rendered, &pages_left_out, wire::OBJECT_PROSE_LEFT_OUT);
                    mark_pages(&mut rendered, &pages_unread, wire::OBJECT_PROSE_UNREAD);
                    // **A subject that others sit under says so.** The count is
                    // the walk's, taken off the inventory it had already read, and
                    // the call that lists them is named, because a caller cannot
                    // tell a leaf from a parent without it. No child contents and
                    // no deeper level. Absent when nothing sits under it.
                    if let Some(children) = subject_children
                        .as_ref()
                        .filter(|c| c.parent == o.entity.id && c.live + c.archived > 0)
                    {
                        rendered["children"] = serde_json::json!({
                            "count": children.live,
                            "archived": children.archived,
                            "how_to_list": format!(
                                "recall with parent: \"{}\" lists the {} that stand; the \
                                 archived ones are left out of that list and counted there \
                                 under archived_excluded",
                                o.entity.id, children.live
                            ),
                        });
                    }
                    // **The room's own count, and the pointer that needs no
                    // second call.** Nothing here is a new verb: the facts
                    // above already carry the aged thought, this only
                    // names which one, alongside the same total a capacity
                    // refusal already reports.
                    if let Some(view) = room_views.get(&o.entity.id) {
                        rendered["room"] = view.block();
                        if let Some(facts) = rendered["facts"].as_array_mut() {
                            for (fact, raw) in facts.iter_mut().zip(&o.facts) {
                                if view.count.aged_out.iter().any(|aged| aged.id == raw.id) {
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
                    if let Some(held) = held {
                        rendered["held"] = held;
                    }
                    // **A record that set a `working_on` whose session is gone says
                    // so**, beside the value it still carries as history.
                    if stale_working_on.contains(&o.entity.id)
                        && let Some(facts) = rendered["facts"].as_array_mut()
                    {
                        for (fact, raw) in facts.iter_mut().zip(&o.facts) {
                            if raw
                                .sets
                                .contains(jojobot_domain::memory::kinds::WORKING_ON)
                            {
                                fact["working_on_stale"] = serde_json::Value::Bool(true);
                            }
                        }
                    }
                    match backing {
                        Some(backing) => {
                            // **Backing obeys the keys the call kept, exactly
                            // as the fields do**, and names what it left out.
                            let wanted = |key: &str| wants_key(narrowing, key);
                            let mut kept = serde_json::Map::new();
                            // **A claim's note is shipped once, found by the
                            // claim that carries it.** A claim that backs
                            // several keys is one record with one note, and
                            // repeating it per key spends the answer on words
                            // the reader already has.
                            let mut notes = serde_json::Map::new();
                            for (key, from) in backing.iter().filter(|(key, _)| wanted(key)) {
                                let mut entry = serde_json::json!({
                                    "claim": from.fact.as_str(),
                                    "provenance": from.provenance.as_token(),
                                    "standing": from.standing.as_token(),
                                });
                                // **Absent unless an app wrote the claim**, so a
                                // reader tells "the app recorded this" from "a
                                // bot said so" in the value's own backing.
                                if let Some(stamp) = &from.recorded_by {
                                    entry["recorded_by"] = super::wire::recorded_by_json(stamp);
                                }
                                kept.insert(key.clone(), entry);
                                // **Absent when the record carries none**,
                                // never an empty string: a key that was always
                                // there and always blank reads as a note
                                // somebody wrote saying nothing.
                                if let Some(note) = &from.note {
                                    notes
                                        .entry(from.fact.as_str().to_string())
                                        .or_insert_with(|| serde_json::json!(note));
                                }
                            }
                            rendered["fields_backing"] = kept.into();
                            if !notes.is_empty() {
                                rendered["fields_backing_notes"] = notes.into();
                            }
                            let left_out: Vec<&str> = backing
                                .keys()
                                .map(String::as_str)
                                .filter(|key| !wanted(key))
                                .collect();
                            if !left_out.is_empty() && !names_keys(narrowing) {
                                let way_back = if narrowing.is_some_and(|narrowing| narrowing.by_view) {
                                    "the view named the ones it shows; ask again without view, \
                                     sending its kind yourself, or with keys naming these"
                                } else {
                                    "ask again without keys to read them all"
                                };
                                rendered["fields_backing_left_out"] = format!(
                                    "{} of this object's keys are not backed here: {} — {way_back}",
                                    left_out.len(),
                                    left_out.join(", ")
                                )
                                .into();
                            }
                        }
                        // **A value arriving bare must not read as one nobody
                        // stands behind.** The backing is a second read and is
                        // left out unless asked for, so the answer says it
                        // exists and names the call that returns it — an
                        // omission a reader cannot see is the one it will get
                        // wrong. Only where there are values to stand behind.
                        None if !o.fields.is_empty() && !names_keys(narrowing) => {
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
        if !hidden_working_on.is_empty() && wants_key(narrowing, "working_on") {
            body["working_on_note"] = super::working_on::hidden_note(&hidden_working_on).into();
        }
        // **What a read that names keys left out is said here, once**, not on
        // every row; the argument that returns each is named beside it.
        if names_keys(narrowing)
            && let Some(narrowing) = narrowing
        {
            let mut held = Held::default();
            held_by(&found, narrowing, &mut held);
            // **A list's blocks are narrowed by the same keys**, so the note names
            // the ones this read left out beside the fields it left out.
            let ticks_asked = args.ticks == Some(true);
            for read in list_entries.values() {
                for key in read.keys() {
                    if !list_block_wanted(Some(narrowing), key, ticks_asked) {
                        held.left_out_keys.insert(list_block_key(key).to_string());
                    }
                }
            }
            let identity_left_out: Vec<&str> = IDENTITY
                .into_iter()
                .filter(|key| !narrowing.wants(key))
                .collect();
            body["narrowed"] = narrowed_note(
                narrowing.keys,
                if found.is_empty() {
                    &[]
                } else {
                    &identity_left_out
                },
                &held.left_out_keys,
                !include.facts && held.any_record,
                !ships_held,
                !args.backing.unwrap_or(false) && held.any_field,
            );
        }
        // **An empty selection names what it looked through.** A filter that
        // matched nothing and a store with nothing look the same to a caller,
        // and a caller acts on "nothing among these" as if it were "nothing".
        // Added only to an answer that returned nothing; one that holds
        // something carries no extra bytes.
        if found.is_empty() {
            let mut looked: Vec<String> = Vec::new();
            if let Some(view) = args.view.as_deref() {
                looked.push(format!("the view {view}"));
            }
            if let Some(kind) = args.kind.as_deref() {
                looked.push(format!("kind {kind}"));
            }
            if let Some(declared) = args.answers_type.as_deref() {
                looked.push(format!("things answering type {declared}"));
            }
            // **The KEYS the call filtered on, never the values it sent**: the
            // caller knows what it asked for, and the line is the population.
            for filter in args.fields.iter().flatten() {
                looked.push(format!("key {}", filter.key.as_deref().unwrap_or("any")));
            }
            if let Some(day) = as_of {
                looked.push(format!("owed as of {day}"));
            }
            if let Some(near) = near {
                looked.push(format!("near {}", near.day));
            }
            if let Some(subject) = args.subject.as_deref() {
                looked.push(format!("subject {subject}"));
            }
            // **The arguments that narrow by name, never the values sent**, as the
            // key filters above: a `where`, a `parent` and a `follow`
            // each leave nothing, and a caller needs to know which one.
            if args.expression.is_some() {
                looked.push("a where expression".to_string());
            }
            if let Some(parent) = args.parent.as_deref() {
                looked.push(format!("parent {parent}"));
            }
            if args.follow.is_some() {
                looked.push("a follow walk".to_string());
            }
            // **How many entities were looked at**: those of the kind named, or
            // every one, archived ones aside. A store that cannot be read says
            // so rather than naming a count it did not take.
            let population = match self.memory.list_entities(query.select.kind).await {
                Ok(held) => format!("{} entities", held.iter().filter(|e| e.browsable()).count()),
                Err(_) => "an unreadable number of entities".to_string(),
            };
            let looked = if looked.is_empty() {
                format!("{population}, no filter")
            } else {
                format!("{population}, narrowed by {}", looked.join(", "))
            };
            let mut left_out = vec![format!("archived entities ({archived_excluded})")];
            if merged_excluded > 0 {
                left_out.push(format!("{merged_excluded} merged into another entity"));
            }
            if withheld > 0 {
                left_out.push(if caller.is_none() && wants_usage {
                    format!("{withheld} weeks, because no sid was passed")
                } else {
                    format!("{withheld} owned by another identity")
                });
            }
            if own_runs_excluded > 0 {
                left_out.push(format!(
                    "{own_runs_excluded} of your own runs, which the filters do not match"
                ));
            }
            body["searched"] = crate::answer::population_line(
                &looked,
                &left_out,
                "drop a filter, or call search with the words a record would use",
            )
            .into();
        }
        // **A filter on the thing that selected nothing says the other scope
        // exists.** A key a claim holds as one of its own `fields` is held by no
        // thing, so the default scope cannot find it, and the empty answer read as
        // the data being absent. Told only to an empty answer that carried a
        // key filter asked of the thing.
        if found.is_empty()
            && args
                .fields
                .iter()
                .flatten()
                .any(|filter| filter.scope.as_deref() != Some("record"))
        {
            body["scope_note"] =
                "a key filter asks what the THING holds, and a key a claim holds as one \
                 of its own `fields` is held by no thing: to find a claim by it, ask again with \
                 `scope: record`"
                    .into();
        }
        if own_runs_excluded > 0 {
            body["own_runs_excluded"] = own_runs_excluded.into();
        }
        // **An answer that holds something and left something out says how to
        // get it.** The counts say that archived or merged-away entities are
        // missing; only an empty answer carried the way to them above. Added
        // only when a count is above zero, so an answer that left nothing out
        // carries no extra bytes.
        if !found.is_empty() && (archived_excluded > 0 || merged_excluded > 0) {
            let mut routes: Vec<String> = Vec::new();
            if archived_excluded > 0 {
                routes.push(format!(
                    "{archived_excluded} archived left out of this browse: recall the handle of \
                     one to read it whole, an archived entity answers by its own handle"
                ));
            }
            if merged_excluded > 0 {
                routes.push(format!(
                    "{merged_excluded} merged into another thing left out: recall the handle of \
                     one and merged_into names where it went"
                ));
            }
            body["excluded_note"] = routes.join("; ").into();
        }
        // **A call naming no kind judges each object by its own kind's
        // declaration**, so an object whose kind does not license a comparison
        // is not matched. Never a silent shrink: the answer counts what it
        // lost by kind and says what to declare.
        if !unlicensed.is_empty() {
            body["left_out_unlicensed"] = unlicensed
                .iter()
                .map(|u| {
                    serde_json::json!({
                        "kind": u.kind,
                        "count": u.count,
                        "because": u.because,
                    })
                })
                .collect::<Vec<_>>()
                .into();
        }
        // **An ordering that read a key as text although its values parse as
        // numbers or dates owes the caller the line saying so**: ten sorts before
        // two as text, and nothing else in the answer would show the key was
        // never declared.
        if let (Some(asked), Some(ordered)) = (args.order.as_ref(), ordered)
            && ordered.read_as_text_though_it_looks_otherwise
        {
            body["order_note"] = format!(
                "'{}' was ordered as text, so 10 comes before 2: no type declares it a number \
                 or a date. Declare one with declare_type to order it as one",
                asked.key.trim()
            )
            .into();
        }
        // **A thread's archived decisions are named, not shown.** The decisions in
        // front are the standing ones; one that was replaced is archived with its
        // reason and still answers by its own handle.
        if !decisions_archived.is_empty() {
            body["decisions_archived"] = serde_json::json!({
                "handles": decisions_archived.iter().map(|id| id.as_str()).collect::<Vec<_>>(),
                "how_to_proceed": "recall one of these handles to read it, with the reason it \
                                   was archived",
            });
            // **A replaced decision is counted as replaced and leads to the one that
            // replaced it.**
            if !decisions_replaced.is_empty() {
                body["decisions_archived"]["replaced"] = serde_json::json!({
                    "count": decisions_replaced.len(),
                    "by": decisions_replaced
                        .iter()
                        .map(|(old, new)| (old.as_str().to_string(), new.as_str().into()))
                        .collect::<serde_json::Map<String, serde_json::Value>>(),
                });
            }
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
        // **The first empty read of a type the software ships says that reading
        // again finds nothing, and where to keep what the session worked out.**
        // Shared with `search`, so the ledger row is spent once across both.
        if nothing_selected && let Some(declared) = asked_answers.as_ref() {
            let select = &query.select;
            let only_narrowing = select.subject.is_none()
                && select.kind.is_none()
                && select.parent.is_none()
                && select.fields.is_empty()
                && select.near.is_none()
                && as_of.is_none();
            self.note_empty_answers_type(&mut body, declared, caller.as_ref(), only_narrowing)
                .await;
        }
        // **A caller with no identity owns no run, and the count above is all
        // it can see of them.** Say how to read its own, because "withheld" on
        // its own does not.
        if caller.is_none() && (wants_sessions || wants_usage) {
            body["withheld_note"] =
                serde_json::json!(no_sid_note(if wants_sessions { "run" } else { "week" }));
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
        // **Reading its own charter is what ends the notice that it changed.** A
        // session of the bot that was handed the charter in this answer has it,
        // whether it asked for `charter` or for the page, which carries the same
        // text.
        if let Some(asking) = &caller
            && body["objects"].as_array().is_some_and(|objects| {
                objects.iter().any(|object| {
                    object["id"] == asking.bot.as_str()
                        && (object["charter_elided"] == false || object["prose"].is_string())
                })
            })
        {
            let now = self.clock().now();
            self.registry.note_charter_told(asking.sid.as_str(), now);
            // Kept on the card too, so the notice stays ended across a restart.
            // A run with no card yet has it set when its card is made.
            if let Some(card) = &asking.card
                && let Err(e) = self.sessions.set_charter_told(card, now).await
            {
                tracing::warn!(error = %e, "the moment a charter was read could not be kept");
            }
        }
        // **The first answer to a session that returns a claim carries the claims
        // teaching**, before the session decides how to write one. Asked of the
        // answer as built, so a recall that returned no claim spends nothing.
        // Added ahead of the fit, so it counts against the ceiling.
        if returns_an_active_claim(&body)
            && self
                .first_contact(crate::teaching::CLAIMS_DOMAIN, caller.as_ref())
                .await
        {
            crate::answer::note_teaching(&mut body, crate::teaching::CLAIMS_TEACHING);
        }
        // **A read of a whole kind that returned enough to be worth narrowing is told,
        // once per session for each kind, that `where` would have asked directly.** The example is
        // built from the answer's own keys, so the domain is spent only when there is
        // one: a session is never told something the answer cannot back.
        if a_whole_read
            && let Some(kind) = args.kind.as_deref()
            && let Some(count) = body["count"]
                .as_u64()
                .map(|count| count as usize)
                .filter(|count| *count >= crate::teaching::WHERE_ON_A_BROAD_READ_MIN)
            && let Some((key, value)) = body["objects"]
                .as_array()
                .and_then(|objects| crate::teaching::where_example(objects))
            && self
                .first_contact(
                    &crate::teaching::where_on_a_broad_read_domain(kind),
                    caller.as_ref(),
                )
                .await
        {
            crate::answer::note_teaching(
                &mut body,
                &crate::teaching::where_on_a_broad_read_teaching(kind, count, &key, &value),
            );
        }
        // **Room for the line that says details were left out is made before the
        // ceiling is applied, and the count is made after it**, so the line counts
        // the objects the answer kept and not the ones the ceiling cut.
        wire::say_details_left_out(&mut body, wire::Marks::Keep);
        crate::memory::ready::fit_under_ceiling(&mut body);
        if let Some(traced) = args.history.as_deref().map(str::trim) {
            fit_run_histories(&mut body, traced, pages_history.then_some(offset));
        }
        fit_recall(
            &mut body,
            if pages_history { 0 } else { offset - skipped },
            Outside {
                before: skipped,
                after: left_over,
            },
        );
        wire::say_details_left_out(&mut body, wire::Marks::Take);
        if let Ok(Some(caller)) = self.caller(args.sid.as_deref()) {
            self.registry.note_shown(caller.sid.as_str(), &body);
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
                if object.entity.kind == EntityKind::SESSION
                    && let Some(prose) = &object.prose
                {
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
                            object.prose = Some(jojobot_domain::session::projected(&full).prose);
                        }
                        Err(e) => return Some(session_declined(e, caller_sid)),
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

    /// **The history of a run the call named a field or the journal of.** A run
    /// is not in the memory store, so the walk leaves it unfilled and the
    /// session port is asked here, once for each run in the answer. `field` is
    /// the name `history` took.
    fn fill_run_history<'a>(
        &'a self,
        objects: &'a mut [graph::Object],
        field: &'a str,
        most: usize,
        skip: usize,
        caller_sid: &'a str,
    ) -> FillSessionProseFuture<'a> {
        Box::pin(async move {
            use jojobot_domain::session::history::{JOURNAL, trace_field, trace_journal};
            for object in objects {
                if object.entity.kind == EntityKind::SESSION {
                    // **A part of no changes reads nothing**, and the pointer it
                    // would leave names the same offset, so a caller that followed it
                    // would be sent back to the call it made.
                    if most == 0 {
                        return Some(memory_declined(
                            "recall",
                            MemoryError::InvalidQuery(
                                "history_most is how many changes or wordings of a run's \
                                 history one part holds, and 0 holds none, so it reads \
                                 nothing and never moves on. Send 1 or more, or leave \
                                 history_most off for the newest part"
                                    .into(),
                            ),
                        ));
                    }
                    let id =
                        jojobot_domain::session::SessionId(object.entity.id.slug().to_string());
                    let traced = if field == JOURNAL {
                        let run = match self.sessions.read_session(&id).await {
                            Ok(run) => run,
                            Err(e) => return Some(session_declined(e, caller_sid)),
                        };
                        let mut entries = Vec::with_capacity(run.entries.len());
                        for entry in &run.entries {
                            match self.sessions.journal_history(&id, &entry.id).await {
                                Ok(wordings) => entries.push((entry.id.clone(), wordings)),
                                Err(e) => return Some(session_declined(e, caller_sid)),
                            }
                        }
                        let (entries, total) = trace_journal(entries, most, skip);
                        graph::RunHistory::Journal { entries, total }
                    } else {
                        let changes = match self.sessions.session_history(&id).await {
                            Ok(changes) => changes,
                            Err(e) => return Some(session_declined(e, caller_sid)),
                        };
                        let (changes, total) = trace_field(field, &changes, most, skip);
                        graph::RunHistory::Field {
                            field: field.to_string(),
                            changes,
                            total,
                        }
                    };
                    object.run_history = Some(traced);
                }
                if let Some(refused) = self
                    .fill_run_history(&mut object.connected, field, most, 0, caller_sid)
                    .await
                {
                    return Some(refused);
                }
            }
            None
        })
    }
}

impl Jojobot {
    /// **Read records by address, each as it stands now** — the `records`
    /// argument.
    ///
    /// **The lookup is `history_record`'s**: `claim_history` answers from the
    /// fold, and the record as it stands is the newest of its writes. Nothing
    /// here reads a second way, and none of the writes behind the newest ride
    /// along. An address that names nothing is named in its own place in the
    /// list and the others are served.
    async fn recall_records(
        &self,
        args: &RecallArgs,
        addresses: &[String],
    ) -> Result<CallToolResult, McpError> {
        let details_asked = args.details.unwrap_or(false);
        if let Some(beside) = selection_beside_records(args) {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(format!(
                    "records is its own question and takes no {beside} beside it: send the \
                     addresses alone (with sid, offset and details if you have them), or drop \
                     records to ask the other question"
                )),
            );
        }
        if addresses.is_empty() {
            return memory_declined(
                "recall",
                MemoryError::InvalidQuery(
                    "records names no address: send at least one, as kind:slug#fN, or leave \
                     records off"
                        .into(),
                ),
            );
        }
        let mut answered = Vec::with_capacity(addresses.len());
        for asked in addresses {
            let address = match FactAddress::parse(asked) {
                Ok(address) => address,
                Err(refused) => {
                    answered.push(missing_record(asked, &refused.to_string()));
                    continue;
                }
            };
            match self.memory.claim_history(&address).await {
                Ok(writes) => match writes.last() {
                    Some(newest) => {
                        answered.push(record_json(&address, newest, writes.len(), details_asked));
                    }
                    None => answered.push(missing_record(asked, "the record holds no write")),
                },
                Err(
                    miss @ (MemoryError::UnknownEntity { .. } | MemoryError::UnknownFact { .. }),
                ) => answered.push(missing_record(asked, &miss.to_string())),
                // **A thing merged away that maps nothing at this address** is
                // named where it was asked, with the survivor to read, like a
                // thing that is not there. A looped chain is stored damage and
                // still refuses the call.
                Err(MemoryError::AlreadyMerged { attempted, into }) => {
                    answered.push(missing_record(
                        asked,
                        &format!(
                            "'{attempted}' was merged into '{into}', and the merge maps no \
                             record to this address: read '{into}' for what it holds now"
                        ),
                    ));
                }
                Err(refused) => return memory_declined("recall", refused),
            }
        }
        let mut body = serde_json::json!({ "count": answered.len(), "records": answered });
        fit_records(&mut body, args.offset.map_or(0, |offset| offset as usize));
        // **Said once, off the records the answer kept.**
        wire::say_details_left_out(&mut body, wire::Marks::Take);
        json_result(&body)
    }
}

/// **The first argument sent beside `records` that is not `sid` or `offset`**,
/// by name. A selection or a shaping argument next to it would be dropped
/// without a word, so the call is refused naming it.
fn selection_beside_records(args: &RecallArgs) -> Option<&'static str> {
    // **Every field is named here, so a field added to `RecallArgs` does not
    // compile until it is placed**: sent beside `records` and refused, or one of
    // the three that belong with it. A list written apart from the struct would
    // let the new argument be dropped without a word.
    let RecallArgs {
        subject,
        kind,
        parent,
        answers_type,
        fields,
        expression,
        scope,
        order,
        facts,
        details: _,
        stood_for,
        review_marks,
        status,
        prose,
        charter,
        keys,
        held,
        history,
        history_record,
        records: _,
        history_most,
        offset: _,
        ticks: _,
        ticks_offset: _,
        backing,
        built_on,
        values,
        values_most,
        overdue,
        unreviewed,
        near,
        follow,
        view,
        inputs,
        sid: _,
    } = args;
    [
        ("subject", subject.is_some()),
        ("kind", kind.is_some()),
        ("parent", parent.is_some()),
        ("answers_type", answers_type.is_some()),
        ("fields", fields.is_some()),
        ("where", expression.is_some()),
        ("scope", scope.is_some()),
        ("order", order.is_some()),
        ("view", view.is_some()),
        ("inputs", inputs.is_some()),
        ("facts", facts.is_some()),
        ("stood_for", stood_for.is_some()),
        ("review_marks", review_marks.is_some()),
        ("status", status.is_some()),
        ("prose", prose.is_some()),
        ("charter", charter.is_some()),
        ("keys", keys.is_some()),
        ("follow", follow.is_some()),
        ("overdue", overdue.is_some()),
        ("unreviewed", unreviewed.is_some()),
        ("near", near.is_some()),
        ("history", history.is_some()),
        ("history_record", history_record.is_some()),
        ("history_most", history_most.is_some()),
        ("values", values.is_some()),
        ("values_most", values_most.is_some()),
        ("built_on", built_on.is_some()),
        ("backing", backing.is_some()),
        ("held", held.is_some()),
    ]
    .into_iter()
    .find_map(|(name, sent)| sent.then_some(name))
}

/// **One record as it stands now**: its newest write, with how many writes it
/// has. The writes behind the newest are not here; `history_record` reads them.
fn record_json(
    address: &FactAddress,
    newest: &jojobot_domain::memory::ClaimWrite,
    revisions: usize,
    details_asked: bool,
) -> serde_json::Value {
    let mut record = serde_json::json!({
        "address": address.to_string(),
        "subject": address.home.as_str(),
        "content": newest.content,
        "provenance": newest.provenance.as_token(),
        "standing": newest.standing.as_token(),
        "status": newest.status.as_token(),
        "recorded_at": newest.recorded_at.to_string(),
        "revisions": revisions,
    });
    // **The app that wrote the record**, by the one renderer a fact read uses.
    if let Some(stamp) = &newest.app_stamp {
        record["recorded_by"] = wire::recorded_by_json(stamp);
    }
    if let (Some(details), Some(fields)) = (&newest.details, record.as_object_mut()) {
        if details_asked {
            fields.insert("details".into(), details.as_str().into());
        } else {
            // Marked, so the answer counts what it kept and says it once.
            fields.insert(wire::OBJECT_LEFT_OUT.into(), 1.into());
            fields.insert(
                wire::OBJECT_LEFT_OUT_AT.into(),
                vec![address.to_string()].into(),
            );
        }
    }
    record
}

/// **An address that names nothing**, in the place it was asked, with the
/// reason the lookup gave.
fn missing_record(asked: &str, why: &str) -> serde_json::Value {
    serde_json::json!({ "address": asked, "missing": why })
}

/// **Fit the records under the answer ceiling, in the order asked.** `offset` is
/// how many were already read. The first of a part is always served, so a record
/// larger than the room still comes back whole; what was left out is counted
/// under `not_shown` with the `offset` that reads on.
fn fit_records(body: &mut serde_json::Value, offset: usize) {
    use jojobot_domain::text::ANSWER_CEILING;
    let Some(records) = body
        .get_mut("records")
        .and_then(|records| records.as_array_mut())
        .map(std::mem::take)
    else {
        return;
    };
    let size = |json: &serde_json::Value| crate::answer::served_len(json) + 1;
    let total = records.len();
    let window: Vec<serde_json::Value> = records.into_iter().skip(offset).collect();
    // **Room for the line that says details were left out**, which is added
    // after this fit and so counts against the ceiling here, whenever a record
    // that could be served marks that it left some.
    let details_room = if window
        .iter()
        .any(|record| record.get(wire::OBJECT_LEFT_OUT).is_some())
    {
        crate::answer::served_len(&wire::details_note(usize::MAX, &[])) + 32
    } else {
        0
    };
    let rest = crate::answer::served_len(body) + crate::answer::STATUS_BAR_ROOM + details_room;
    if offset == 0 && rest + window.iter().map(size).sum::<usize>() <= ANSWER_CEILING {
        body["records"] = window.into();
        return;
    }
    let mut room = ANSWER_CEILING.saturating_sub(rest + crate::answer::widest_not_shown("records"));
    let mut shown: Vec<serde_json::Value> = Vec::new();
    for record in window {
        let cost = size(&record);
        if !shown.is_empty() && cost > room {
            break;
        }
        room = room.saturating_sub(cost);
        shown.push(record);
    }
    let after = offset + shown.len();
    let none_shown = shown.is_empty();
    body["count"] = shown.len().into();
    body["records"] = shown.into();
    if total > after {
        body["not_shown"] = crate::answer::not_shown(total - after, after, "records");
    } else if offset > 0 && none_shown {
        body["past_the_end"] = format!(
            "offset {offset} is past the last of the {total} records this call asked for, so \
             nothing is left to read"
        )
        .into();
    }
}

#[cfg(test)]
mod prose_unread;
#[cfg(test)]
mod routed;
#[cfg(test)]
mod story_only;
#[cfg(test)]
mod tests;
mod usage;

/// **What a caller with no identity is told about the runs or weeks it was
/// shown none of**: it owns none, and the way to read its own is the door.
fn no_sid_note(thing: &str) -> String {
    format!(
        "no sid was passed, so no {thing} here is yours: withheld counts the {thing}s that \
         exist and shows none of them. Boot with start_here as your bot and pass the sid it \
         returns on this call to read your own {thing}s"
    )
}
