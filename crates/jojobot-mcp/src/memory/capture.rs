//! `capture` — Remember one fact about an entity, with its provenance, at
//! most one edge, and — when it was derived from another claim rather than
//! from an entity — the claim it traces to.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use std::collections::BTreeMap;

use jojobot_domain::attention;
use jojobot_domain::memory::graph;

use super::clauses::{happened_at_clause, stale_after_clause};
use super::*;
use crate::session::session_declined;
use crate::teaching::{
    CHECK_IN_DATE_TEACHING, CLAIM_DIRECTION_DOMAIN, CLAIM_DIRECTION_TEACHING, CLAIM_SUBJECT_DOMAIN,
    CLAIM_SUBJECT_TEACHING, CLAIMS_DOMAIN, CLAIMS_TEACHING, FIELD_SHADOWS_ARGUMENT_DOMAIN,
    RHYTHM_HISTORY_DOMAIN,
};

/// **A tick on a list's entry**: which entry, and what was done to it.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct TickArg {
    /// The address of the entry, `list:packing#f2`, exactly as `recall` serves it
    /// under `entries`. It must be an active entry of the list the capture is on.
    pub(crate) entry: String,
    /// One of `done`, `undone`, `rotated` or `dropped`. `done` does what the
    /// list's `tick` setting says: a `sink` list keeps the entry in view at the
    /// bottom, a `leave` list takes it out of the open view, and a `back` list
    /// turns it to the end, still open. `undone` brings a done or dropped entry
    /// back, but never to undo a turn on a `back` list. `rotated` is a turn, for a
    /// `back` list only.
    /// `dropped` says the entry no longer applies, and it never reads as done.
    /// **A tick is refused when it changes nothing, or when the list cannot take
    /// it**: `undone` on an open entry, `done` on an entry that is already done,
    /// `rotated` on a list that is not `back`, and any act but `undone` on a
    /// dropped entry. The refusal names the act that lands instead.
    pub(crate) act: String,
}

/// **One entry of a batch**: a bare string is the entry's words, and an object
/// adds what a single entry can carry beside its words.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
#[serde(untagged)]
pub(crate) enum EntryItem {
    /// The entry's words.
    Text(String),
    /// The entry's words, with its own details or its own edge, so a checklist
    /// step can link to a piece of work.
    Full {
        /// The entry's words, one line, as `content` is for a single entry.
        content: String,
        /// The description under the entry, as `details` is for a single entry.
        #[serde(default)]
        details: Option<String>,
        /// This entry's edge shape, as `shape` is for a single entry.
        #[serde(default)]
        shape: Option<String>,
        /// The entity this entry's edge points at, as `object` is for a single entry.
        #[serde(default)]
        object: Option<String>,
    },
}

/// The most items one batch carries. Fifty entries or ticks answer with a
/// receipt of about four kilobytes, far inside the answer ceiling.
pub(crate) const BATCH_CAP: usize = 50;

/// Arguments to `capture`.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — any `kind:slug` id (a bare handle is read
    /// as a person), or `<collection>/<N>` for the member of a numbered
    /// collection holding the number N. **It must already exist**: a subject jojobot doesn't know
    /// comes back with candidates and nothing is written. Create it with
    /// `add_entity` first if it is genuinely new.
    pub(crate) subject: String,
    /// The crisp claim to remember — single line, no line breaks. Put the rest in
    /// `details`.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as the
    /// handle that thing wears today, even after a rename.
    /// Text inside backticks or a fenced block is literal: a handle there is
    /// kept as written and never becomes a link.
    ///
    /// A claim too big to be read back whole in one answer is refused with nothing
    /// written: shorten it, split it into claims that each say one thing, or keep
    /// the long text on a document (`put_document`) and write a pointer claim.
    ///
    /// Required for one claim. Leave it off when you send `entries` or `ticks`.
    #[serde(default)]
    pub(crate) content: String,
    /// Nuance, the why, merge notes — the description under the claim. It may
    /// hold paragraph breaks and reads back with every one; the claim above
    /// it stays one line. Carries `@kind:slug` mentions exactly as `content`
    /// does.
    #[serde(default)]
    pub(crate) details: Option<String>,
    /// `testimony` (the operator said it), `observation` (you read it in a system
    /// of record) or `inference` (you derived it). Defaults to `inference`:
    /// anything not tied to the operator's words is a hypothesis.
    ///
    /// **`observation` must say where it was read** — the field `read_from`
    /// naming the system, and `read_ref` for what was read there if you have
    /// one. Without it the call is refused, because a claim that reads back
    /// settled and cannot be gone back to is worse than one filed as a guess.
    ///
    /// **What another bot instructed or decided within its own remit is an
    /// `observation` read from it** — `read_from` its handle, `read_ref` the
    /// message id, read back settled and never naming the operator — **but a
    /// world fact a bot only reported is not covered** and stays `inference`
    /// until you read it at its own source.
    #[serde(default)]
    pub(crate) provenance: Option<String>,
    /// `settled` or `open` — **how sure anyone is**, which is a different
    /// question from `provenance`'s *who backs it*.
    ///
    /// Leave it off and it follows the provenance: the operator's word is
    /// `settled`, a claim you worked out is `open`. Set it only when the two
    /// come apart — and the case that matters is **the operator saying
    /// something first-hand and hedging it**: that is
    /// `provenance: testimony` with `standing: open`.
    ///
    /// **Declared on honour, exactly as `provenance` is.** Nothing here asks
    /// you to confirm a standing. `update_fact` is where the gate sits, on two
    /// moves of an EXISTING claim: anything to `testimony`, and an open
    /// standing to `settled`, which takes a `receipt`.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// **The day the claim was MADE**, `YYYY-MM-DD` — the day it was said,
    /// decided or worked out. Defaults to the day your run is in.
    ///
    /// Not the day the thing happened: that is `happened_at`, and it is
    /// separate because one field answering both is how a vague answer becomes
    /// an invented date.
    ///
    /// **Naming a day that differs from your run's stated day is never
    /// refused** — the receipt carries `recorded_at_note`, one sentence
    /// naming both days, so a sitting acting out a different period does not
    /// silently record under the wrong one by mistake.
    #[serde(default)]
    pub(crate) recorded_at: Option<String>,
    // Described by attribute, not a doc comment: the defining clause is shared
    // with `update_fact` (see `memory::clauses`) and a doc comment cannot take it.
    #[schemars(description = concat!(
        happened_at_clause!(),
        "\n\n",
        "⛔️ **Leave it off unless you were told a day.** *Over the summer* is not\n",
        "a day: a claim that says nothing about when the thing happened is a\n",
        "complete claim, and approximating one puts a date nobody stated on a\n",
        "record that may carry the operator's own authority. **jojobot never\n",
        "fills this in.**"
    ))]
    #[serde(default)]
    pub(crate) happened_at: Option<String>,
    /// **The far end, when the thing happened is a stretch of days rather
    /// than one** — a festival, a course — `YYYY-MM-DD`. Send it alongside
    /// `happened_at`, which is the start; an end with no start names a span
    /// nobody can read.
    #[serde(default)]
    pub(crate) happened_through: Option<String>,
    /// The shape of the edge this fact draws: `location` (object is a place) ·
    /// `membership` (an org) · `attendance` (an event) · `about` (any kind) ·
    /// `connection` (any kind — a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone. Every read and write
    /// receipt serves the same shape word that a write takes.
    ///
    /// **Draw `connection` with the subject as YOUR OWN bot handle and the
    /// claim becomes a thought of yours, not an ordinary claim about an
    /// entity.** A connection claim another bot writes on your handle is an
    /// ordinary claim: it takes no seat and a full room does not refuse it.
    /// A thing carrying `thought_capacity` holds only that many
    /// thoughts at once. A write that would go over is refused, naming how
    /// many are held and asking you to name one to drop and why, or to archive
    /// when the room is already over — see `drop`, below. One nobody touches for long enough goes quiet on its
    /// own: still there, just no longer counted. How long is counted in the
    /// bot's runs: its own `thought_ages_after_runs` setting, twenty when it
    /// has none. `update_fact`'s `keep: true` is the designed way to say one
    /// still matters, on purpose, without changing a word of it.
    ///
    /// **`thought_capacity` is a ceiling, and the thing it binds cannot set
    /// it.** Send this key about your own handle and the write is refused,
    /// whatever else the call carries — only a different identity may raise
    /// or lower it. The same holds for `thought_ages_after_runs`.
    ///
    /// A thought's content is capped at the bot's own `thought_body_cap`; with none,
    /// at 200 characters if the bot carries `thought_capacity`, and a bot with
    /// neither is not capped. A longer one is refused. Put the substance on the
    /// thing it points at and keep the thought short.
    #[serde(default)]
    pub(crate) shape: Option<String>,
    /// The entity the edge points at, as `kind:slug`. **It must already exist**,
    /// exactly as `subject` must — an edge into a node nobody else references is
    /// how a cross-entity question quietly starts coming back empty.
    #[serde(default)]
    pub(crate) object: Option<String>,
    /// **The claim this one was derived from**, as its address
    /// (`kind:slug#local-id`), when it was derived from another claim rather
    /// than from an entity. An edge's object is an entity; this is not an
    /// edge, because a claim has no entity to point at when what it came from
    /// is itself a claim.
    #[serde(default)]
    pub(crate) derived_from: Option<String>,
    /// **The record's fields**, as a flat bag of key/value pairs — anything
    /// worth recording about this claim beyond the sentence.
    ///
    /// Flat and free-form on purpose: jojobot stores what you put here, acts
    /// only on the keys the build names, and keeps a key it has never seen
    /// exactly as you wrote it. **A value that is a handle, or an item of a
    /// comma list that is a handle, is a link and must name something that
    /// exists**; the loose items beside it stay text, and a handle inside a
    /// longer sentence is prose. **Nothing has to be declared first**, and no
    /// name for the class of thing is asked for: the fields ARE what the record
    /// says, and a type is something the keys answer rather than something you
    /// announce.
    ///
    /// **`"starred": "true"` competes for one of the few seats a bot's own
    /// boot spends on its rules, and it stays on the claim: it never folds onto
    /// the thing.** An unstarred rule is fetched with `facts: true` when it is
    /// needed rather than shown by default. Seats are few on purpose — starring
    /// everything is the same as starring nothing — and **a star into full
    /// seats is refused, naming the seated rules: unstar one to make room.**
    /// **How many seats
    /// a bot has is `rule_seats`, written about it by a different identity** —
    /// a bot cannot raise its own — and a star or a count that would leave
    /// the boot over its size ceiling is refused, naming the overage.
    ///
    /// **Six keys describe their own claim and never fold onto the thing:**
    /// `read_from`, `read_ref`, `starred`, `subject`, `purpose` and `recorded_by`.
    /// Read them back with `facts: true`; find by them with a `fields` filter and
    /// `scope: record`. A field you send that equals what jojobot ships as its
    /// default today is stored exactly as you sent it either way, and the receipt's
    /// echoes_defaults names which key that was.
    ///
    /// **A pass, ticket, membership or registration somebody holds** is a claim on
    /// the HOLDER, with `admits` (the handle of the event, place or
    /// organisation it opens), `valid_from` and `valid_until` (days; no window
    /// reads as live) and optionally `tier`, all in `sets`. That an event offers a tier is a claim
    /// on the event instead. Holding is not attendance, not wanting to go and not a
    /// calendar entry: each is its own claim. Recall the holder with facts to read
    /// what they hold, or the event for its `held` block, which ranks who holds
    /// what, live first, on the day it names in `as_of`.
    #[serde(default)]
    pub(crate) fields: Option<std::collections::BTreeMap<String, String>>,
    /// **What this claim sets on its thing**, as key/value pairs: the keys that
    /// are properties of the thing the claim is about, apart from `fields`, which
    /// are the claim's own. One call records the claim and sets the thing. A key
    /// belongs to one of the two for as long as this record holds it, so a key
    /// named here and in `fields` is refused, and a key the record already holds
    /// in the other is refused with the way to move it. A key that only labels
    /// the claim (`read_from`, `read_ref`, `starred`, `subject`, `purpose`,
    /// `recorded_by`) is never a property of the thing and is refused here by name.
    /// The keys that make a thing fall due act on it only when sent here. A list
    /// key (`depends_on`) changes by its items with `sets_add` and `sets_remove`:
    /// its whole value sent here replaces what another session added. A key
    /// comes off the thing again with `update_fact`'s `clear_fields` on the
    /// record that carries it. Setting `status` to `now` on work your bot owns
    /// also points your bot's `working_on` at it.
    /// A write whose status words could not be read from `topic:instance` still lands, and its receipt carries `instance_unread`, naming under `not_worked_out` what it left undone. A note the receipt would have carried and a failed read kept from it is named the same way, under `reads_left`.
    ///
    /// **A container numbers what it holds** when its `sets` carries `numbers_by`
    /// naming a key, `{numbers_by: "number"}`. Every thing under it, at any depth,
    /// that sets that key holds a whole number no other holds, and keeps it after
    /// it is archived. Number a member in its own `sets` (`{number: "32"}`) and
    /// recall it by the container's handle, a slash and the number
    /// (`topic:all-rules/32`). The container's own recall says its `next_number`,
    /// and a number already held is refused naming its holder and the free number.
    #[serde(default)]
    pub(crate) sets: Option<std::collections::BTreeMap<String, String>>,
    /// **Items to add to a list key on the thing**, as key/value pairs where the
    /// value is the items, separated by commas: `{depends_on: "work:gamma"}`.
    /// Accepted only for a key declared a list. The thing's current value is read
    /// inside this call and the items are appended to the end, in the order sent;
    /// an item it already holds is not added twice, and every other item stays
    /// where it was, so nothing is resent. A handle that names nothing is refused
    /// with the nearest handles, as under `sets`. The answer is stored as an
    /// ordinary whole value. Naming the same key in `sets` is refused.
    #[serde(default)]
    pub(crate) sets_add: Option<std::collections::BTreeMap<String, String>>,
    /// **Items to take off a list key on the thing**, in the form `sets_add`
    /// takes. The rest of the list stays where it was. An item the list does not
    /// hold is no error, and neither is a handle that no longer names anything,
    /// so a reference to a thing that is gone can come off. With `sets_add` in one
    /// call, the removals are applied first.
    #[serde(default)]
    pub(crate) sets_remove: Option<std::collections::BTreeMap<String, String>>,
    /// The entities this record touches, as `kind:slug` — **each must already
    /// exist**, exactly as `subject` must.
    ///
    /// These are links whose MEANING is deliberately not recorded: the pointer
    /// is real and searchable, and what the connection was is left unsaid
    /// rather than guessed. That is why they are not `about` edges — `about`
    /// asserts the record is about that entity, and this only admits that it
    /// touches it.
    #[serde(default)]
    pub(crate) refs: Option<Vec<String>>,
    /// **Record this as a check-in on a rhythm** — `ran`, `skipped` or
    /// `snoozed`. The subject must be a `rhythm`; on anything else this is
    /// refused.
    ///
    /// **The difference between the three is whether the cycle is consumed.**
    /// `ran` happened. `skipped` did not happen and the cycle advances anyway,
    /// with the record saying plainly that it did not — which is what lets a
    /// skipped cycle be told from a completed one later. `snoozed` does NOT
    /// consume the cycle: it lasts until the day it names in `snoozed_until`,
    /// and the rhythm comes back then and is acted on again.
    ///
    /// **jojobot does the arithmetic.** It writes `outcome`, `last_check_in`
    /// (the day of this check-in, `recorded_at`, today when you send none) and,
    /// when the cycle is consumed, the new
    /// `counts_from` — worked out from `advances_from`, or its default. Do not
    /// compute those yourself and do not send them in `fields`.
    ///
    /// **A snooze names the day it lasts until**, as `snoozed_until` in
    /// `sets` (`YYYY-MM-DD`), a day after this check-in's own, which is
    /// `recorded_at` (today when you send none). The rhythm is
    /// not owed again before that day, and on it the rhythm comes back and is
    /// acted on again; it falls due on the later of that day and its own. A
    /// snooze with no day, or a day that is not after that day, comes back
    /// blocked and nothing is written. The next `ran` or `skipped` check-in
    /// ends the snooze, so send `snoozed_until` with `snoozed` only.
    ///
    /// **What the check MEASURED is yours to send** in `fields`: a reading, a
    /// distance, a count. A cadence is always time, so a measurement is a field
    /// on the check-in and never a unit of the schedule.
    ///
    /// A rhythm short of a cadence (`cadence_days` or `rrule`), stored or sent in
    /// this call's `sets`, comes back blocked naming both, and nothing is written. A rhythm
    /// short only of `counts_from` still takes a `ran` or `skipped` check-in —
    /// that outcome OPENS the rhythm, and the basis becomes this check-in's own
    /// date. `snoozed` cannot open a rhythm, so it still refuses on a
    /// `counts_from` the rhythm does not hold.
    #[serde(default)]
    pub(crate) check_in: Option<String>,
    // Described by attribute for the reason `happened_at` is.
    #[schemars(description = concat!(
        stale_after_clause!(),
        "\n\n",
        "Optional, and most claims never carry one. **The claim does not stop being\n",
        "true, it stops being trusted.** **A claim carrying no day made no\n",
        "promise**, which is neither fresh nor stale."
    ))]
    #[serde(default)]
    pub(crate) stale_after: Option<String>,
    /// **Why this claim is settled**, when you declare `settled` on an
    /// inference. Exactly one of two shapes: `check`, what was checked, written
    /// so another agent can run it again; or `confirmed`, the operator said so,
    /// as `{on: YYYY-MM-DD, at: a message id or the session}`. Never required
    /// here: a capture is not a promotion. Provenance stays what you declared.
    #[serde(default)]
    pub(crate) receipt: Option<super::receipt::ReceiptArg>,
    /// **The address of a thought to drop, when this capture is what fills
    /// the last free slot in a thing's own room** — the capped set of live
    /// thoughts `thought_capacity` bounds; see `shape`, above, for what
    /// makes a claim one. `kind:slug#local-id`,
    /// exactly as `recall` serves one. Archived in the SAME act as this
    /// fact is written — never a separate call — because a drop with
    /// nothing yet written in its place is a state the room must never
    /// reach. Ignored when the room this write would join is not full:
    /// naming one costs nothing until it is actually spent. Requires
    /// `drop_because`.
    #[serde(default)]
    pub(crate) drop: Option<String>,
    /// **The one-line reason the dropped thought no longer earns its
    /// slot** — your own testimony, at the moment of the act. Archived onto
    /// the dropped claim as its own `details`. Required whenever `drop` is.
    #[serde(default)]
    pub(crate) drop_because: Option<String>,
    /// **Where a new entry goes on a list**, when `subject` is a thing of kind
    /// `list`: the address of an entry the new one follows
    /// (`list:packing#f2`, exactly as `recall` serves it), or the word `first`
    /// for the front or `last` for the end. Absent means the end. The server mints each entry's
    /// order key, so there is no number to pick.
    /// Refused on any subject that is not a list, and refused when the address is
    /// not an open entry of this list; the refusal says what to send instead.
    #[serde(default)]
    pub(crate) after: Option<String>,
    /// **Tick an entry of a list**, instead of adding one: `{entry, act}`. The
    /// subject is the list. One record lands on the list naming the entry, the
    /// act, the session and the day; the entry itself is never edited, so its
    /// words, author and provenance stay as they were. The list's own `tick`
    /// setting says what `done` does, and `recall` of the list serves the order
    /// and each entry's `state` computed from its ticks. `content` is the note
    /// that goes with the tick. The receipt's `ticked` names the entry and the
    /// act, and for an entry that points at a piece of work, `linked` says
    /// whether that work is still open: a tick says the list item is done, never
    /// that the work is, and the work's status is left as it was. A tick that
    /// changes nothing is refused, with the act that lands named in its place (see
    /// `act`). When that work could not be read, the tick still landed and the
    /// receipt carries `linked_unread` in place of `linked`: an absent `linked` is
    /// not work with no state. A retried tick
    /// under one `request_id` lands once, so a rotation never turns twice. `recall` with `ticks` true reads the
    /// ticks. Refused on any subject that is not a list, beside `after`, and for
    /// an entry that is not an active entry of the list.
    #[serde(default)]
    pub(crate) tick: Option<TickArg>,
    /// **Add many entries to a list in one call**, instead of `content`: each
    /// item is the entry's words, or an object with `content` and, if the entry
    /// needs them, `details`, `shape` and `object`. The subject is the list.
    /// Every entry is a claim of its own with its own address and shares this
    /// call's provenance and standing, and they land in the order sent: at the
    /// end, or as one run behind the entry `after` names. The receipt lists the
    /// address of each, by position.
    ///
    /// **All or nothing for anything you can fix.** Every item is checked first,
    /// by the same rules a single entry meets; one that fails refuses the whole
    /// call, names the item by its position and writes nothing. At most 50 items
    /// a call. Send `request_id` on the first call: if a store failure stops the
    /// batch partway, the receipt says how many landed, and the same call again
    /// with the same `request_id` writes the rest and none twice. Not beside
    /// `content`, `tick` or `ticks`.
    #[serde(default)]
    pub(crate) entries: Option<Vec<EntryItem>>,
    /// **Tick many entries of a list in one call**, instead of `tick`: each item
    /// is `{entry, act}` exactly as `tick` takes one. The subject is the list.
    /// Each item is one tick record, as a single tick is, and each is judged
    /// against the list as the ticks before it in the batch leave it. One item that fails refuses
    /// the whole call, names it by its position and writes nothing: that includes
    /// a tick that changes nothing, such as `done` twice on one entry with no
    /// `undone` between, which is refused whole, while `done`, `undone`, `done` in
    /// that order lands as three ticks. At most 50
    /// items a call. A retried batch under one `request_id` lands once, so a
    /// rotation never turns twice. Not beside `entries`, `tick` or `after`.
    #[serde(default)]
    pub(crate) ticks: Option<Vec<TickArg>>,
    /// **The emergency reserve — usable once, and only once.** Set this
    /// instead of `drop` and a write that would take a full room over its
    /// ceiling lands anyway, over capacity, rather than being refused. A
    /// room already over capacity — a borrow already outstanding — refuses
    /// this exactly as it would with `borrow` unset: the priority once
    /// you're over is steering back to a clean state, never borrowing
    /// again on top of it. Bring the room back at or under capacity first
    /// — archive a live thought with `update_fact`, not a `drop` here,
    /// which swaps one in for one out and leaves the debt exactly where it
    /// was. Ignored wherever `drop` already lands the write, or the room is
    /// not at capacity to begin with.
    ///
    /// **A thread holds three live records unless a bot above its writers
    /// sets `record_capacity` on it**, and a capture past that is refused
    /// with nothing written. The refusal names four routes: `rewrite` a record in place with
    /// `update_fact`, `archive` one, `split` into a child thread, or move long
    /// text to a `document`. `borrow` lands one record over the ceiling, once.
    #[serde(default)]
    pub(crate) borrow: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    /// Receipt: `success`, `id`, address, `derived` and warnings, read back from the store; the calling bot's setting_token_saving=on (off by default) selects only id and derived values, deferring teaching and session notices; set it with capture sets on that bot and read claims with recall facts:true.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **A name of your choosing for this write**, up to 128 bytes.
    /// **Send it on the first call of the write**, because a call sent without
    /// one cannot be repeated safely. If an answer is lost, send the
    /// same call again with the same `request_id`: if
    /// the first call wrote, you get its receipt back marked `repeat` and nothing
    /// is written a second time. It needs a `sid`, and a call that wrote nothing
    /// spends no id.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    // **Not published**: no refusal of this verb carries a token while a type's
    // rule is off (rule 636), so the arguments would do nothing. The override
    // path below stays for the rule's return, and a call that sends them is
    // refused as one sending an argument the verb does not take.
    #[serde(skip)]
    pub(crate) override_token: Option<String>,
    #[serde(skip)]
    pub(crate) override_reason: Option<String>,
}

impl super::declined::AcceptsOverride for CaptureArgs {
    fn override_token(&self) -> Option<&str> {
        self.override_token.as_deref()
    }
    fn override_reason(&self) -> Option<&str> {
        self.override_reason.as_deref()
    }
}

/// **How one pass of the capture path is used.** The default is a single
/// capture: it writes, and it takes the placement lock itself.
#[derive(Debug, Default, Clone)]
pub(super) struct Mode {
    /// Run every guard and stop before the write.
    pub(super) check_only: bool,
    /// The batch already holds the list's placement lock.
    pub(super) placing_held: bool,
    /// The part of a write under a `request_id` this claim is.
    pub(super) part: Option<String>,
    /// **The state a tick's entry is judged in**, when a batch has already
    /// worked out what the ticks before this one leave it. `None` judges the
    /// entry as the list reads it now, which is every single tick.
    pub(super) tick_state: Option<&'static str>,
    /// **Set by an item of a batch that ends what is owed**, in place of that item
    /// ending it: the batch ends it once, after its last item.
    pub(super) ends_later: Option<std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// **The caller a batch has already identified**, so each item reuses that
    /// read of the run. `None` identifies the caller, as every single capture does.
    pub(super) caller: Option<crate::caller::Caller>,
}

/// A plain capture still carries any due moment it computes in that same
/// record, so the receipt explains why that mixed record is a derivation.
const WHY_A_MOVED_DUE_MOMENT_DERIVES: &str = "this capture moved the stored due moment on its own, without a check-in — the same \
     arithmetic a check-in does, run because a field it reads just changed — and a record \
     carrying both jojobot's schedule and your sentence is a derivation";

/// Without a check-in the basis is exactly what the caller typed, rather
/// than a date derived from a completed turn of the rhythm.
pub(super) const WHY_THIS_BASIS_IS_HAND_TYPED: &str = "counts_from on this record is exactly what you sent — hand-typed, not derived, because this \
    capture asked for no check_in. A check-in is what stores the schedule jojobot works out \
    beside your sentence; capture one on this rhythm and jojobot computes it instead.";

/// **The mirror of [`WHY_THIS_BASIS_IS_HAND_TYPED`].** That sentence tells a
/// caller jojobot did not do the arithmetic; this one tells them it did, on
/// the one check-in where the basis came from nowhere but the check-in itself.
/// It names the day so the caller can see WHICH date was taken, which is the
/// only part they could not have predicted.
pub(super) const WHY_THIS_BASIS_WAS_DERIVED: &str = "This check-in opened the loop: it held a cadence and no basis, so counts_from was derived \
    from this check-in's own date,";

struct Declared {
    subject: String,
    provenance: Option<String>,
    standing: Option<String>,
    recorded_at: Option<String>,
}

impl Declared {
    /// Taken before the record is assembled, because assembling it consumes
    /// what the caller sent.
    fn of(args: &CaptureArgs) -> Self {
        Self {
            subject: args.subject.clone(),
            provenance: args.provenance.clone(),
            standing: args.standing.clone(),
            recorded_at: args.recorded_at.clone(),
        }
    }

    /// A reason is supplied only when this call computed a due moment on the
    /// caller's record; other differences do not borrow that explanation.
    fn not_stored(
        &self,
        fact: &Fact,
        converted: Option<&'static str>,
    ) -> Vec<crate::answer::Difference> {
        use crate::answer::Difference;
        [
            Difference::between("subject", Some(&self.subject), fact.subject.as_str()),
            Difference::converted(
                "provenance",
                self.provenance.as_deref(),
                fact.provenance.as_token(),
                converted,
            ),
            Difference::between(
                "standing",
                self.standing.as_deref(),
                fact.standing.as_token(),
            ),
            Difference::between(
                "recorded_at",
                self.recorded_at.as_deref(),
                &fact.recorded_at.to_string(),
            ),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

/// Sending one alongside `check_in` is a contradiction rather than an override,
/// so it is refused: the record would say two things about one schedule and
/// nothing could say which was meant.
const COMPUTED: [&str; 3] = [
    attention::OUTCOME,
    attention::LAST_CHECK_IN,
    attention::COUNTS_FROM,
];

impl Jojobot {
    /// The arithmetic itself belongs to the domain; what is here is the reach
    /// into the store the domain cannot make. It reads the rhythm's fields —
    /// every write on it folded to one value per key — because the schedule is
    /// described a record at a time: the record that set it up carries the
    /// cadence, and every check-in since carries what it found.
    ///
    /// **The read happens before the write and never instead of it.** A
    /// schedule this cannot read is a refusal, so a half-configured rhythm
    /// never takes a check-in that would look applied and move nothing.
    async fn check_in(
        &self,
        subject: &EntityId,
        outcome: &str,
        on: jiff::civil::Date,
        sent: &BTreeMap<String, String>,
    ) -> Result<Result<(BTreeMap<String, String>, bool), CallToolResult>, McpError> {
        // An unparseable token is an error rather than a refusal, exactly as an
        // unknown provenance or edge shape is: nothing about the store is
        // wrong, and the vocabulary is closed.
        let Some(outcome) = attention::Outcome::of_token(outcome) else {
            return Err(McpError::invalid_params(
                format!(
                    "check_in takes one of: {}",
                    attention::Outcome::ALL
                        .map(attention::Outcome::as_token)
                        .join(", ")
                ),
                None,
            ));
        };
        if !super::unlocks::takes_a_check_in(subject) {
            return Ok(Err(blocked_body(
                "CheckIn",
                subject,
                &[],
                format!(
                    "Nothing was written. A check-in records a turn of a recurring loop, so its \
                     subject must be a rhythm, and '{subject}' is not one. Capture this as an \
                     ordinary claim without check_in, or name the rhythm this was a turn of."
                ),
            )));
        }
        // **A contradiction, not an override.** A caller that sends one of
        // these alongside `check_in` has said two things about one schedule,
        // and picking either would make the other silently untrue.
        if let Some(key) = COMPUTED.iter().find(|key| sent.contains_key(**key)) {
            return Ok(Err(blocked_body(
                "CheckIn",
                subject,
                &[],
                format!(
                    "Nothing was written. This call sends '{key}' in fields AND asks for a \
                     check-in, and a check-in computes '{key}' itself. Check in without that key, \
                     and jojobot works it out."
                ),
            )));
        }

        // **The day a snooze lasts until is read from what this call SENT**,
        // never from what the loop already holds: a day left by an earlier
        // snooze must not stand in for a snooze that names none.
        if let Err(why) = attention::snooze_day_of(
            outcome,
            sent.get(attention::SNOOZED_UNTIL).map(String::as_str),
            on,
        ) {
            return Ok(Err(blocked_body(
                "CheckIn",
                subject,
                &[],
                format!("Nothing was written: {why}."),
            )));
        }

        let held = match graph::walk(
            self.memory.as_ref(),
            &[],
            &graph::GraphQuery {
                select: graph::Selection {
                    subject: Some(subject.clone()),
                    ..Default::default()
                },
                include: graph::Include {
                    facts: false,
                    prose: false,
                    stood_for: false,
                    status: None,
                },
                follow: None,
                history: None,
                masked: Default::default(),
            },
        )
        .await
        {
            Ok(found) => found
                .subject_object(subject)
                .map(|object| object.fields)
                .unwrap_or_default(),
            Err(e) => return Ok(Err(memory_declined("capture", e)?)),
        };

        // **The schedule this write leaves, not the one it found.** The same
        // call may carry the schedule's own keys (`cadence_days`,
        // `advances_from`) beside the check-in, and the arithmetic must run on
        // them: reading only the stored loop refused a capture for lacking a
        // cadence it had just sent. A key the call sends wins over the stored
        // one, exactly as it will in the fold once the write lands. The basis
        // is never among them (`COMPUTED` refuses it above), so whether this
        // check-in opens the loop is still read off what was stored.
        let mut after = held.clone();
        after.extend(sent.iter().map(|(key, value)| (key.clone(), value.clone())));

        match attention::check_in(&after, outcome, on) {
            // **Observed rather than restated.** Whether the loop was opened
            // is read off what changed — a basis where the thing held none —
            // so it stays true if the domain's rule for opening one moves.
            // The emptiness test is the domain's own: a key holding blank
            // space is a key nobody wrote.
            Ok(computed) => {
                let had_a_basis = held
                    .get(attention::COUNTS_FROM)
                    .is_some_and(|held| !held.trim().is_empty());
                let opened = !had_a_basis && computed.contains_key(attention::COUNTS_FROM);
                Ok(Ok((computed, opened)))
            }
            // **The way forward names the key and, when it is a vocabulary, its
            // values.** A rhythm short of a schedule is the caller's to
            // complete, and the repair is a capture of the missing key — which
            // is a different move from correcting a value that is already
            // there, so the domain's own sentence carries which of the two it
            // is.
            // **A refusal that carries its own way through keeps it.** The
            // advice below names a key to capture, which is right for a loop
            // short of a declaration no check-in can make. It is wrong for a
            // snooze on a loop with no basis: there the missing key is one a
            // check-in supplies, and sending the caller to type it is the
            // move this whole path exists to remove.
            Err(why) if why.names_its_own_way_through() => Ok(Err(blocked_body(
                "CheckIn",
                subject,
                &[],
                format!("Nothing was written: {why}."),
            ))),
            // **Name the key that is actually at fault, and only that one.**
            // `schedule_of` fails on the first key it cannot read, so `why`
            // is always about exactly one — never all three at once. Naming
            // the other two regardless used to tell a caller short of
            // `cadence_days` alone to also capture `counts_from`, which is
            // the value a working check-in already supplies and the exact
            // move rule 261 exists to stop.
            Err(why) => Ok(Err(blocked_body(
                "CheckIn",
                subject,
                &[],
                format!(
                    "Nothing was written: {why}. Capture the missing key on '{subject}', then \
                     send this check-in again.",
                ),
            ))),
        }
    }
}

/// Remember a fact about an entity. Returns the stored fact including the
/// address a later `update_fact` can edit it through.
#[tool_router(router = capture_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Remember one claim about an entity (a handle, or `<collection>/<N>`, \
                       the member numbered N): the words, when \
                       they became true, and who backs them. PROVENANCE: testimony (the operator said it), \
                       observation (you READ it in a system of record, so `read_from` in \
                       fields must name the system or the call is refused) or inference (you \
                       worked it out; the default, and a hypothesis, not a finding). STANDING \
                       (settled or open) is how sure anyone is, apart from provenance, \
                       which it follows unless you set it. A NEW THING THAT HAPPENED \
                       IS A NEW CLAIM: capture it again, never update_fact. Every entity the call names — the subject, an \
                       edge's object, refs — must ALREADY EXIST, or nothing is written and \
                       the answer is status: blocked with candidates: add_entity first, then \
                       capture. `fields` is a flat bag that stays \
                       on the claim (a handle, or one in a comma list, is a link and must \
                       exist; six keys describe their own claim); \
                       `sets` sets keys on the thing itself in the same call, the only bag \
                       that reaches it. A connection claim with your OWN bot handle as \
                       subject is a `thought`, capped in length (`thought_body_cap`, else 200 \
                       with `thought_capacity`) and in count (`thought_capacity`). In a full room \
                       a new thought needs `drop` with `drop_because`, or `borrow` once to land \
                       one over; a room over capacity takes none until you archive back under \
                       it. Another bot's such claim is ordinary and takes no seat. `record_capacity` caps any thing's live records; a \
                       THREAD holds 3 unless a bot above its writers sets it. On a `list`, `after` places an \
                       entry and `tick` marks one. `sets_add` and `sets_remove` change a list \
                       key by its items. A claim too big to read back \
                       whole is refused. The receipt's `changed` lists changes, or is `nothing` or `not_read`."
    )]
    pub(crate) async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        if args.entries.is_some() || args.ticks.is_some() {
            return self.capture_batch(args).await;
        }
        self.capture_one(args, Mode::default()).await
    }

    /// **The one write path for a claim**, for the single form and for each item
    /// of a batch alike (rule 498). `mode` says whether this is only the check of
    /// an item, whether the batch already holds the list's placement lock, and
    /// which part of a write under a `request_id` the claim is.
    pub(super) async fn capture_one(
        &self,
        args: CaptureArgs,
        mode: Mode,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match mode.caller.clone() {
            Some(caller) => caller,
            None => match self.identified_for_write(args.sid.as_deref()).await {
                Ok(caller) => caller,
                Err(refused) => return Ok(refused),
            },
        };
        // **What this write could not work out for want of the instance record**,
        // named on the receipt.
        let unread = super::instance_unread::InstanceUnread::default();
        let reads = super::reads_left::ReadsLeft::default();
        // **An earlier call under this id may have left its computed schedule
        // or the taking off of a due moment unfinished.** The checks and
        // arithmetic below judge the thing as
        // it stood before the claim, so they are not asked again.
        if let Some(recorded) = jojobot_domain::request::skip("claim") {
            return self.capture_finished(&args, &caller, recorded).await;
        }
        // **What the call sent to lift a `must` refusal**, taken before the
        // arguments are moved into the claim.
        let slot = super::declined::TokenSlot::from(&args);
        let (override_token, override_reason) =
            (args.override_token.clone(), args.override_reason.clone());
        let declared = Declared::of(&args);
        let checked_in = args.check_in.is_some();
        let subject = match self.row_handle(&EntityId::person(&args.subject)).await {
            Ok(subject) => subject,
            Err(e) => return memory_declined("capture", e),
        };
        // **The kind the store holds**, never the one the caller's spelling carries: a
        // handle a retyped thing used to wear still names it.
        let subject_kind = match self.kind_now(&subject).await {
            Ok(kind) => kind,
            Err(e) => return memory_declined("capture", e),
        };
        // **A voucher is issued, never written.** Judged on the subject's kind,
        // ahead of everything the call carries.
        if let Some(refused) = jojobot_domain::voucher::refuses_engine_only_write(&subject) {
            return memory_declined("capture", refused);
        }
        // **A claim on a list with no words is told the four ways to write to one**, where
        // the plain refusal for empty words would leave a caller guessing at them. A tick
        // carries no words of its own, so it is not this.
        if args.content.trim().is_empty()
            && args.tick.is_none()
            && subject_kind == Some(EntityKind::LIST)
        {
            return memory_declined(
                "capture",
                MemoryError::InvalidFact(
                    "content is empty. A claim on a list is an entry: send `content` for one \
                     entry, `entries` for several, `tick` to tick one or `ticks` to tick several"
                        .into(),
                ),
            );
        }
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let recorded_at = self
            .dated(args.recorded_at.as_deref(), args.sid.as_deref())
            .await?;
        let edge = match parse_edge(args.shape.as_deref(), args.object.as_deref())? {
            Ok(edge) => edge,
            Err(refused) => return Ok(refused),
        };

        let derived_from = args
            .derived_from
            .as_deref()
            .map(FactAddress::parse)
            .transpose()
            .map_err(memory_error)?;

        let drop = args
            .drop
            .as_deref()
            .map(FactAddress::parse)
            .transpose()
            .map_err(memory_error)?;
        // **The reason is not optional once a drop is named.** Splitting the
        // two would let a drop land with no testimony behind it, which is
        // exactly the gap the room's own refusal exists to prevent.
        if drop.is_some() && args.drop_because.as_deref().is_none_or(str::is_empty) {
            return memory_declined(
                "capture",
                MemoryError::InvalidFact(
                    "drop names a thought without drop_because: the reason is the bot's own \
                     testimony at the moment of the act, and a drop cannot land without one"
                        .into(),
                ),
            );
        }

        let mut fields = args.fields.unwrap_or_default();
        // **The setting bag joins `fields` for everything below**, so every guard
        // that reads a claim's keys reads these too, and the keys that are the bag
        // travel apart in `sets` so a store can tell them from the claim's own.
        let mut sets_sent = args.sets.unwrap_or_default();
        let dropped_keys = match self
            .drop_read_keys(
                &subject,
                None,
                [&mut fields, &mut sets_sent],
                &mut Vec::new(),
            )
            .await
        {
            Ok(dropped) => dropped,
            Err(e) => return memory_declined("capture", e),
        };
        // **What remains of the caller's own fields**, apart from the setting
        // bag for the receipt's hint; copied server keys are not caller fields.
        let own_sent: Vec<String> = fields.keys().cloned().collect();
        // **Held from the read of the thing's fold to the write**, on the one lock a
        // document's sections and a list's entries are placed under, so two callers
        // changing one list read one after the other. A list takes it for its
        // entries whether or not a delta is sent, and it is taken once.
        let _placing = if !mode.placing_held
            && (subject_kind == Some(EntityKind::LIST)
                || Self::changes_a_list_by_item(&args.sets_add, &args.sets_remove))
        {
            match self.hold_placing(&subject).await {
                Ok(guard) => Some(guard),
                Err(e) => return memory_declined("capture", e),
            }
        } else {
            None
        };
        if let Err(refused) = self
            .resolve_list_deltas(&subject, args.sets_add, args.sets_remove, &mut sets_sent)
            .await
        {
            return memory_declined("capture", refused);
        }
        if let Some(key) = sets_sent.keys().find(|key| fields.contains_key(*key)) {
            return memory_declined(
                "capture",
                MemoryError::InvalidFact(format!(
                    "'{key}' is named in both `fields` and `sets`. A key is one of the claim's \
                     own fields or a setting on its thing, never both: send it in one"
                )),
            );
        }
        let set_keys: std::collections::BTreeSet<String> = sets_sent.keys().cloned().collect();
        let sets_named: Vec<String> = set_keys.iter().cloned().collect();
        fields.extend(sets_sent);
        // **Captured before anything computed joins `fields`** — `check_in`
        // and a moved due moment both add keys below, and this is about what
        // the CALLER sent, never what jojobot added on its own.
        let sent_field_keys: Vec<String> = fields.keys().cloned().collect();
        // **A guarded key is written only by whom its declaration licenses.**
        // Checked before anything else about this write, on the caller's own
        // identity against the subject it is about to write.
        if let Some(refused) = self
            .refuses_an_unlicensed_key("capture", &subject, true, &caller.bot, &fields)
            .await?
        {
            return Ok(refused);
        }
        // **A role's own two fields are the boot door's, whoever is asking.**
        // This call reaches the Memory trait directly from the claim path
        // and the renewal path, never through this verb — so a write that
        // reaches here naming either field is, by construction, not one of
        // those two. See `refuses_role_fields`.
        if let Some(refused) = jojobot_domain::memory::refuses_role_fields(&subject, fields.keys())
        {
            return memory_declined("capture", refused);
        }
        // **The stored due moment is jojobot's, so a caller's own copy of it is
        // refused before anything is written** — see
        // `refuses_a_hand_written_due_moment`. Checked on what the caller sent,
        // ahead of the check-in and the mover, which add their own.
        if let Some(refused) = self
            .refuses_a_hand_written_due_moment(&subject, &fields, &[])
            .await
        {
            return Ok(refused);
        }
        // **The review stamp is jojobot's too**, set when the writer is defrag.
        if let Some(refused) = self.refuses_a_hand_written_review_stamp(&subject, &fields, &[]) {
            return Ok(refused);
        }
        // **The keys jojobot computes below reach the thing**: they are its own
        // arithmetic on the thing, not words a caller wrote on a record.
        let mut derived: std::collections::BTreeSet<String> = Default::default();
        let mut opened_the_loop = false;
        if let Some(outcome) = args.check_in.as_deref() {
            match self
                .check_in(&subject, outcome, recorded_at, &fields)
                .await?
            {
                Ok((computed, opened)) => {
                    opened_the_loop = opened;
                    derived.extend(computed.keys().cloned());
                    fields.extend(computed);
                }
                Err(refused) => return Ok(refused),
            }
        }
        // **Kept current here too, not only on a check-in.** A cadence edited
        // by a plain capture is a write like any other, and the due moment it
        // carries is jojobot's own arithmetic riding along in the same
        // record — exactly as a check-in's own computed keys already do.
        //
        // **Only what reaches the thing counts.** A key the caller sent as the
        // claim's own field stays on the claim, so it moves no due moment: the
        // arithmetic reads the settings and what jojobot computed, never the
        // merged map the guards above read.
        let reaching: std::collections::BTreeMap<String, String> = fields
            .iter()
            .filter(|(key, _)| set_keys.contains(*key) || derived.contains(*key))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect();
        let (due_on_computed, due_on_derived) = self
            .moved_due_moment(
                &subject,
                &reaching,
                &[],
                self.dated(None, args.sid.as_deref()).await?,
                &unread,
            )
            .await;
        let due_on_set = matches!(due_on_computed, attention::DueMove::Set(_)) && due_on_derived;
        if let attention::DueMove::Set(due_on) = due_on_computed {
            fields.insert(attention::DUE_ON.to_string(), due_on.to_string());
            derived.insert(attention::DUE_ON.to_string());
        }
        // **A write by defrag stamps the thing in the same act.** One computed
        // key on the thing, riding this record, so no record is added for it.
        if let Some((key, moment)) = self.review_stamp(&caller.bot) {
            fields.insert(key.clone(), moment);
            derived.insert(key);
        }
        // **A capture can only ADD fields to a record**: `NewFact` has no
        // clear_fields of its own. So when the write moves the thing out of owing
        // anything, the stored moment is taken off by the same `clear_fields` an
        // edit uses, on the record that carries it. See `clear_stored_due_moment`.
        let clears_due_on = matches!(due_on_computed, attention::DueMove::Cleared);
        // A check-in's schedule has its own derived record. A plain capture
        // still mixes any due moment it computes with the caller's sentence.
        let provenance = if due_on_set && !checked_in {
            Provenance::Inference
        } else {
            provenance
        };

        // **The cutoff is computed here, never inside `Memory`** — a
        // session's own clock is a different bounded context this record
        // never reaches into itself (rule: `Sessions` and `Memory` share no
        // relationship). Asked only when this write could possibly be a
        // thought: the kind check is a string parse, no store reached, so a
        // capture on anything else pays nothing for it.
        let aged_before = if subject_kind == Some(EntityKind::BOT)
            && jojobot_domain::memory::authorship::is_connection(edge.as_ref())
        {
            let ages_after = match self.ages_after_runs_of(&subject).await {
                Ok(runs) => runs,
                Err(e) => return memory_declined("capture", e),
            };
            match self.sessions.summaries_of(&subject).await {
                Ok(runs) => jojobot_domain::memory::aging_cutoff(
                    &jojobot_domain::session::history::started_ats(&runs),
                    ages_after,
                ),
                Err(e) => return session_declined(e, caller.sid.as_str()),
            }
        } else {
            None
        };

        // **A claim on a list is an entry, and the server places it.** Held from
        // the read of the neighbours to the write, on the one lock a document's
        // sections are placed under, so two callers cannot mint one key.
        // **What the call ticked**, for the receipt: the entry, the act, and the
        // work the entry points at if it points at any.
        let mut ticked: Option<(String, &'static str, Option<EntityId>)> = None;
        if let Some(tick) = &args.tick {
            if subject_kind != Some(EntityKind::LIST) {
                return memory_declined(
                    "capture",
                    MemoryError::InvalidFact(format!(
                        "`tick` ticks an entry of a list, and {subject} is not a thing of kind \
                         list. Capture the tick on the list the entry is on"
                    )),
                );
            }
            if args.after.is_some() {
                return memory_declined(
                    "capture",
                    MemoryError::InvalidFact(
                        "`tick` ticks an entry and `after` places a new one: they are two calls"
                            .into(),
                    ),
                );
            }
            let subject_now = match self.current_handle(&subject).await {
                Ok(now) => now,
                Err(e) => return memory_declined("capture", e),
            };
            let held = match self.memory.recall(&subject).await {
                Ok(held) => held,
                Err(e) => return memory_declined("capture", e),
            };
            let setting = match self.memory.fields(&subject).await {
                Ok(fields) => jojobot_domain::memory::entries::Setting::of(&fields),
                Err(e) => return memory_declined("capture", e),
            };
            let seen = jojobot_domain::memory::entries::view(&held, setting);
            let entries = jojobot_domain::memory::entries::open_entries(&held);
            let named = FactAddress::parse(tick.entry.trim())
                .ok()
                .filter(|address| {
                    address.home == subject_now
                        && entries.iter().any(|entry| entry.id == address.local)
                });
            let Some(named) = named else {
                return memory_declined(
                    "capture",
                    MemoryError::InvalidFact(format!(
                        "'{}' is not an active entry of {subject}: `tick` names the address of \
                         an entry, as `recall` serves it under `entries`. {}",
                        tick.entry,
                        if entries.is_empty() {
                            "It has no entries yet".to_string()
                        } else {
                            format!(
                                "Its entries are {}",
                                entries
                                    .iter()
                                    .map(|entry| entry.address().to_string())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        }
                    )),
                );
            };
            let Some(act) = jojobot_domain::memory::entries::Act::of(&tick.act) else {
                return memory_declined(
                    "capture",
                    MemoryError::InvalidFact(format!(
                        "'{}' is not a tick act: send one of {}",
                        tick.act,
                        jojobot_domain::memory::entries::Act::ALL
                            .map(jojobot_domain::memory::entries::Act::as_token)
                            .join(", ")
                    )),
                );
            };
            let state = mode.tick_state.unwrap_or_else(|| {
                seen.states
                    .get(named.local.0.as_str())
                    .copied()
                    .unwrap_or("open")
            });
            if let Some(why) = jojobot_domain::memory::entries::refuse_tick(setting, state, act) {
                return memory_declined("capture", MemoryError::InvalidFact(why));
            }
            ticked = Some((
                named.to_string(),
                act.as_token(),
                entries
                    .iter()
                    .find(|entry| entry.id == named.local)
                    .and_then(|entry| entry.edge.as_ref().map(|edge| edge.object.clone())),
            ));
            use jojobot_domain::memory::entries::{TICK_ACT, TICK_ENTRY, TICK_SESSION};
            fields.insert(TICK_ENTRY.to_string(), named.local.0.clone());
            fields.insert(TICK_ACT.to_string(), act.as_token().to_string());
            fields.insert(TICK_SESSION.to_string(), caller.sid.as_str().to_string());
        } else if subject_kind == Some(EntityKind::LIST) {
            let held = match self.memory.recall(&subject).await {
                Ok(held) => held,
                Err(e) => return memory_declined("capture", e),
            };
            let setting = match self.memory.fields(&subject).await {
                Ok(fields) => jojobot_domain::memory::entries::Setting::of(&fields),
                Err(e) => return memory_declined("capture", e),
            };
            // **An address from before a merge is followed to where its entry is
            // now**, as every address of a folded claim is.
            let placement =
                match jojobot_domain::memory::entries::parse_placement(args.after.as_deref()) {
                    Ok(jojobot_domain::memory::entries::Placement::After(address)) => {
                        match self.served_address(&address).await {
                            Ok(served) => {
                                Ok(jojobot_domain::memory::entries::Placement::After(served))
                            }
                            Err(e) => return memory_declined("capture", e),
                        }
                    }
                    other => other,
                };
            // **Placed as a move is**: against the order the list reads now, so an
            // entry behind a turned entry reads behind it.
            let placed = placement.and_then(|placement| {
                jojobot_domain::memory::entries::place_new(&held, setting, &subject, &placement)
            });
            match placed {
                Ok(placed) => {
                    use jojobot_domain::memory::entries::{HELD_SINCE, HELD_TURN};
                    // The server mints the whole place; a caller's own copy of the
                    // turn keys is not kept.
                    fields.remove(HELD_TURN);
                    fields.remove(HELD_SINCE);
                    fields.insert(
                        jojobot_domain::memory::document::ORDER.to_string(),
                        placed.order,
                    );
                    // An entry in the first bucket reads there by its key alone.
                    if placed.held_turn != 0 {
                        fields.insert(HELD_TURN.to_string(), placed.held_turn.to_string());
                        fields.insert(HELD_SINCE.to_string(), placed.held_since.to_string());
                    }
                }
                Err(jojobot_domain::memory::entries::PlaceError::NotAnEntry { named, entries }) => {
                    return memory_declined(
                        "capture",
                        MemoryError::InvalidFact(format!(
                            "'{named}' is not an open entry of {subject}: `after` names the \
                             address of an entry, `first` for the front or `last` for the end. {}",
                            if entries.is_empty() {
                                "It has no entries yet, so leave `after` off".to_string()
                            } else {
                                format!("Its entries are {}", entries.join(", "))
                            }
                        )),
                    );
                }
                Err(e) => {
                    return memory_declined("capture", super::list_move::refused(&subject, e));
                }
            }
        } else if let Some(after) = args.after.as_deref() {
            return memory_declined(
                "capture",
                MemoryError::InvalidFact(format!(
                    "`after` ({after}) places an entry on a list, and {subject} is not a thing \
                     of kind list. Make the list first (add_entity with kind list, under the \
                     thing it is about), then capture each entry on it"
                )),
            );
        }

        let opened_basis = fields
            .get(attention::COUNTS_FROM)
            .filter(|_| opened_the_loop)
            .cloned();
        let mut new = NewFact {
            app_stamp: None,
            subject,
            content: args.content,
            details: args.details,
            provenance,
            standing: args.standing.as_deref().map(parse_standing).transpose()?,
            status: Default::default(),
            recorded_at,
            // **Only what the caller sent.** No default and no derivation: a
            // claim that says nothing about when the thing happened says
            // nothing, which is the whole reason this field is separate.
            happened_at: parse_date(args.happened_at.as_deref())?,
            happened_through: parse_date(args.happened_through.as_deref())?,
            edge,
            fields,
            refs: args
                .refs
                .iter()
                .flatten()
                .map(|r| EntityId::person(r.trim()))
                .collect(),
            derived_from,
            stale_after: parse_date(args.stale_after.as_deref())?,
            receipt: args
                .receipt
                .as_ref()
                .map(super::receipt::parse_receipt)
                .transpose()?,
            drop,
            drop_because: args.drop_because,
            borrow: args.borrow.unwrap_or(false),
            aged_before,
            session: Some(caller.sid.as_str().to_string()),
            written_by_bot: Some(caller.bot.clone()),
            sets: set_keys,
            derived,
            also_written: Vec::new(),
            must_override: None,
            // **The run's day, not the day the claim is filed under**: a rule's
            // relative day counts from the caller's today.
            must_day: Some(self.dated(None, args.sid.as_deref()).await?),
        };
        let schedule = checked_in
            .then(|| super::capture_schedule::Schedule::take_from(&mut new, clears_due_on));
        // **A star or a seat count that would take the bot's boot over its
        // ceiling is refused here**, before anything lands — see
        // `refuses_a_boot_floor_over`.
        if let Some(refused) = self
            .refuses_a_boot_floor_for_capture(&new, &caller.bot, &reads)
            .await
        {
            return memory_declined("capture", refused);
        }
        // **A star into full seats is refused here**, before anything lands —
        // see `refuses_a_star_for_capture`.
        if let Some(refused) = self.refuses_a_star_for_capture(&new).await {
            return memory_declined("capture", refused);
        }
        // **A claim no reader could be handed whole is refused here**, before
        // anything lands — see `refuses_a_claim_too_big_to_read`.
        if let Some(refused) = self.refuses_a_new_claim_too_big_to_read(&new, None).await {
            return memory_declined("capture", refused);
        }
        // **Disagreement, not presence alone.** A check-in with no
        // `happened_at` has asked no question, and one where both fields
        // name the same day already got the schedule right, whichever the
        // caller believed was doing the work — see [`CHECK_IN_DATE_TEACHING`].
        let check_in_dates_disagree =
            checked_in && new.happened_at.is_some_and(|day| day != new.recorded_at);
        // **Checked before the write, on exactly what is about to be sent** —
        // never a reason to refuse or to drop a key, only a fact worth
        // naming on the receipt (see `Memory::echoed_defaults`'s own doc for
        // why nothing here can tell a deliberate match from an accidental
        // echo, and does not try to).
        let echoed_defaults = self.memory.echoed_defaults(&new.subject, &new.fields).await;
        // Routed through the declined path rather than straight to the mapper:
        // a fact the validators refuse is a caller mistake, and it comes back
        // as an answer with a way forward (rule 68).
        // **A write that landed is never reported as failed** (rule 130): the
        // fold's own refresh can fail after the inner write already
        // succeeded, and that failure must not read as this capture having
        // failed. `fold_behind` carries what to note on the receipt; the
        // written record itself is unpacked exactly as an ordinary success.
        // **A check ends here**: every guard above has judged this claim, and
        // nothing has been written.
        if mode.check_only {
            if let Err(refused) = jojobot_domain::memory::validate_new_fact(&new) {
                return memory_declined("capture", refused);
            }
            return json_result(&serde_json::json!({ "checked": true }));
        }
        let before = self.as_it_stands(&new.subject).await;
        let moves_to_now = self.moves_to_now(&new.fields, &new.sets).await;
        // **A capture that finishes a thing is two parts**: the claim, and the
        // taking off of the stored due moment from the record that carries it.
        // The claim keeps that plan in its note, so a resend finishes it.
        let write = self.write_the_claim(
            mode.part.as_deref(),
            clears_due_on && !checked_in,
            new,
            (override_token.as_deref(), override_reason.as_deref()),
        );
        let writing = match &schedule {
            Some(schedule) => {
                jojobot_domain::request::armed_part("claim", Some(schedule.frozen()), write).await
            }
            None => write.await,
        };
        let writing = match writing {
            super::capture_batch::ClaimWrite::Ran(written) => *written,
            super::capture_batch::ClaimWrite::Early(answer) => return answer,
        };
        let (captured, fold_behind) = match writing {
            Ok(captured) => (captured, None),
            Err(MemoryError::FoldBehind {
                landed: Landed::Fact(fact),
                behind,
                ..
            }) => (Guarded::Written(*fact), Some(behind)),
            Err(e) => return super::must_override::declined_or_lifted("capture", e, &slot),
        };
        match captured {
            Guarded::Written(fact) => {
                let schedule_behind = if let Some(schedule) = &schedule {
                    match self.write_check_in_schedule(&fact, schedule, &caller).await {
                        Ok(behind) => behind,
                        Err(error) => {
                            return self.check_in_schedule_not_written(
                                &fact,
                                self.dated(None, args.sid.as_deref()).await?,
                                error,
                            );
                        }
                    }
                } else {
                    None
                };
                let fold_behind = fold_behind.or(schedule_behind);
                // **Before the clear below, which can end the call for a resend to
                // finish.** The finishing path rebuilds the receipt from the claim
                // and does not point the bot at its work again.
                self.fill_working_on(&caller, &fact.subject, &moves_to_now, &unread)
                    .await;
                if clears_due_on
                    && let Some(unfinished) = self
                        .ends_what_is_owed_or_defers(&mode, &fact.subject, &caller)
                        .await
                {
                    return Ok(unfinished);
                }
                if checked_in {
                    jojobot_domain::request::finished();
                }
                self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
                    .await;
                // **As of the day the RUN is asking about, never the day the
                // claim is about** (rule 222). `date` above is what the claim
                // is true OF, which is a different question from whether its
                // reading still stands today — and the read that follows this
                // write answers as of today, so a receipt answering as of the
                // claim's day contradicts it inside one session.
                let mut body =
                    fact_receipt_json(&fact, self.dated(None, args.sid.as_deref()).await?);
                dropped_keys.note(&mut body);
                let changed = match fold_behind {
                    Some(_) => crate::changed::Changed::not_read(),
                    None => self.changed_on(&before, &fact.subject).await,
                };
                crate::changed::note_changed(&mut body, changed);
                if let Some((entry, act, link)) = &ticked {
                    // **A tick says the list item is done, never that the work is.**
                    // The receipt names whether the work it points at is still open.
                    let mut block = serde_json::json!({ "entry": entry, "act": act });
                    if let Some(link) = link {
                        // **The tick landed, so a read of the work that fails leaves
                        // `linked` off the receipt** and does not fail the write
                        // (rule 130). The receipt says the read failed, so an absent
                        // `linked` never reads as work with no state.
                        let pointed = std::collections::BTreeSet::from([link.clone()]);
                        match self.linked_state(&pointed).await {
                            Ok(known) => {
                                if let Some(work) = known.get(link) {
                                    block["linked"] = work.as_receipt();
                                }
                            }
                            Err(e) => {
                                tracing::warn!(error = %e, work = %link, "a tick receipt could not read its linked work");
                                block["linked_unread"] =
                                    "the tick landed; the work it links could not \
                                    be read. recall the list to read it."
                                        .into();
                            }
                        }
                    }
                    body["ticked"] = block;
                }
                if let Some(behind) = fold_behind {
                    crate::answer::note_fold_behind(&mut body, behind);
                }
                crate::answer::note_echoes_defaults(&mut body, &echoed_defaults);
                // **Only when the caller named a day AND their run has one of
                // its own AND the two disagree.** `recorded_at` above is
                // already the resolved day — equal to what the caller sent
                // whenever they sent one — so the comparison is this simple.
                if args.recorded_at.is_some()
                    && let Some(stated) = caller.day
                    && stated != recorded_at
                {
                    crate::answer::note_recorded_at_mismatch(&mut body, stated, recorded_at);
                }
                let why_demoted =
                    (due_on_set && !checked_in).then_some(WHY_A_MOVED_DUE_MOMENT_DERIVES);
                crate::answer::note_delta(&mut body, declared.not_stored(&fact, why_demoted));
                let mut line = self
                    .what_a_capture_left_standing(
                        &fact,
                        &caller.bot,
                        checked_in,
                        opened_basis.as_deref(),
                        &reads,
                        &mut body,
                    )
                    .await;
                if let Some(due) = crate::answer::falls_due_sentence(&fact.subject, due_on_computed)
                {
                    line.push(' ');
                    line.push_str(&due);
                }
                crate::answer::note_postcondition(&mut body, line);
                self.note_reach_and_hint(&mut body, &fact, &own_sent, &sets_named)
                    .await;
                self.note_view_narrows_nothing(&mut body, &fact.subject)
                    .await;
                self.note_status_word(&fact, &mut body, &unread).await;
                unread.note(&mut body);
                reads.note(&mut body);
                self.note_depends_on_read(&fact, &caller, &mut body).await;
                self.note_possible_subject(&fact, &caller, &mut body).await;
                self.note_unlocks(&fact.subject, &caller, &mut body).await;
                self.note_unstars_without_a_summary(&fact, false, caller.sid.as_str(), &mut body);
                if self.first_contact(CLAIMS_DOMAIN, Some(&caller)).await {
                    crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
                }
                if self
                    .first_contact(CLAIM_SUBJECT_DOMAIN, Some(&caller))
                    .await
                {
                    crate::answer::note_teaching(&mut body, CLAIM_SUBJECT_TEACHING);
                }
                // **Gated on the subject's kind, never on the capture.** A
                // write about a person or a thing has nothing to do with a
                // project's work, so it must not spend this domain's one line.
                if crate::teaching::is_project_work(fact.subject.kind()) {
                    self.offer_skills_for(
                        crate::orientation::skills::Act::ProjectWrite,
                        Some(&caller),
                        &mut body,
                    )
                    .await;
                }
                // **Gated on the check-in, never on the subject's kind**: a
                // rhythm written about without recording a turn of it is not
                // the act the skill covers.
                if checked_in {
                    self.offer_skills_for(
                        crate::orientation::skills::Act::CheckIn,
                        Some(&caller),
                        &mut body,
                    )
                    .await;
                }
                // **Gated on the edge, not the capture.** A claim naming no
                // other entity has no side to get wrong, so a plain capture
                // must not spend this domain's one teaching.
                if fact.edge.is_some()
                    && self
                        .first_contact(CLAIM_DIRECTION_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, CLAIM_DIRECTION_TEACHING);
                }
                // **Shares its domain with `add_entity`'s rhythm-history
                // teaching, deliberately.** Both state that a check-in's own
                // `recorded_at` is what the schedule counts from; sharing the
                // row means whichever site reaches a session first is the one
                // that tells it, never both — see `RHYTHM_HISTORY_DOMAIN`.
                if check_in_dates_disagree
                    && self
                        .first_contact(RHYTHM_HISTORY_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, CHECK_IN_DATE_TEACHING);
                }
                // **Checked before the gate, never after** — `first_contact`
                // has a side effect, and spending this domain's one teaching
                // on a call that did not shadow anything would silence the
                // call that actually does. The schema lookup is skipped
                // entirely when no fields were sent, which is most calls.
                //
                // **Against the UNION of both verbs' arguments, never
                // capture's alone.** The motivating case is exactly a
                // capture carrying `status`, an argument only update_fact
                // has — checking capture's own schema alone would miss it.
                //
                // **Only keys the subject's kind leaves undeclared** — a key
                // the kind declares is its own column, never a shadow.
                let undeclared_keys = crate::teaching::keys_the_kind_leaves_undeclared(
                    fact.subject.kind(),
                    &sent_field_keys,
                );
                if !undeclared_keys.is_empty()
                    && let Some(capture_properties) =
                        crate::teaching::published_arguments("capture")
                    && let Some(update_fact_properties) =
                        crate::teaching::published_arguments("update_fact")
                    && let Some((shadowed, owning_verb)) = crate::teaching::shadowed_argument_verb(
                        undeclared_keys.iter().copied(),
                        "capture",
                        &capture_properties,
                        "update_fact",
                        &update_fact_properties,
                    )
                    && self
                        .first_contact(FIELD_SHADOWS_ARGUMENT_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(
                        &mut body,
                        &crate::teaching::field_shadows_argument_teaching(shadowed, &owning_verb),
                    );
                }
                json_result(&body)
            }
            Guarded::Blocked {
                attempted,
                candidates,
            } => Ok(blocked_result(
                &attempted,
                &candidates,
                Blocked::MustExist("capture"),
            )),
        }
    }
}

impl Jojobot {
    /// **The end of a capture whose claim an earlier call wrote**: its saved
    /// schedule or due clear is finished, and the receipt names the same claim.
    async fn capture_finished(
        &self,
        args: &CaptureArgs,
        caller: &crate::caller::Caller,
        recorded: jojobot_domain::request::Recorded,
    ) -> Result<CallToolResult, McpError> {
        let gone = |what: &str| {
            Ok(crate::caller::handle_declined(
                "capture",
                crate::answer::WayForward::person(format!(
                    "The claim was written under this request_id, but {what}, so the rest \
                     cannot be finished."
                )),
            ))
        };
        let Some(address) = serde_json::from_str::<serde_json::Value>(&recorded.landed)
            .ok()
            .and_then(|landed| landed.get("address")?.as_str().map(str::to_string))
            .and_then(|address| FactAddress::parse(&address).ok())
        else {
            return gone("what it wrote cannot be named");
        };
        let Some(fact) = self
            .memory
            .recall(&address.home)
            .await
            .ok()
            .and_then(|facts| facts.into_iter().find(|fact| fact.address() == address))
        else {
            return gone("it cannot be read back");
        };
        let schedule = match recorded.note.as_deref() {
            Some(note) if super::capture_schedule::Schedule::is_note(note) => {
                let Some(schedule) = super::capture_schedule::Schedule::thawed(note) else {
                    return self.check_in_schedule_not_written(
                        &fact,
                        self.dated(None, args.sid.as_deref()).await?,
                        MemoryError::StoredDamage {
                            what: "the saved check-in schedule plan cannot be read".to_string(),
                        },
                    );
                };
                Some(schedule)
            }
            _ => None,
        };
        let schedule_behind = if let Some(schedule) = &schedule {
            match self.write_check_in_schedule(&fact, schedule, caller).await {
                Ok(behind) => behind,
                Err(error) => {
                    return self.check_in_schedule_not_written(
                        &fact,
                        self.dated(None, args.sid.as_deref()).await?,
                        error,
                    );
                }
            }
        } else {
            None
        };
        if recorded.note.as_deref() == Some(super::due::CLEARS_DUE_ON)
            || schedule
                .as_ref()
                .is_some_and(|schedule| schedule.clears_due_on)
        {
            if let Some(unfinished) = self
                .end_what_is_owed("capture", &fact.subject, caller)
                .await
            {
                return Ok(unfinished);
            }
        } else {
            jojobot_domain::request::finished();
        }
        self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
            .await;
        let mut body = fact_receipt_json(&fact, self.dated(None, args.sid.as_deref()).await?);
        if let Some(behind) = schedule_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        crate::request::note_resumed(&mut body);
        crate::changed::note_changed(&mut body, crate::changed::Changed::not_read());
        json_result(&body)
    }

    /// **The licence check every write that sets fields makes, in one place.**
    /// A guarded key is written only by whom its declaration licenses — never a
    /// kind question, see `refuses_unlicensed_write`. A verb that writes fields
    /// calls this and no verb carries a copy, so a key guarded later is covered
    /// by construction on every door that sets fields.
    ///
    /// **Compared as the handle the subject answers to now.** A subject typed as
    /// a handle the caller's own bot used to wear still names the bot, and the
    /// session is bound to its current one. The resolving read is spent only on
    /// a write that names a guarded key, and a read that fails refuses the write
    /// rather than waving it through.
    ///
    /// **The chart is read here, before the write, and not inside it.** A fresh
    /// write carries no caller down to the store, so the chain this write is
    /// judged against can be one write old: a chart change that lands between
    /// this read and the store's write is judged against the chart before it.
    /// Chart changes are rare and made by superiors, and the window is accepted
    /// (decision log 381); the edit, the retraction and the merge read the chart
    /// inside their own transaction.
    ///
    /// `exists` is false on a creation: a subject that does not exist yet has no
    /// chart above it and no former handle, so it is judged on the manager the
    /// write names alone.
    pub(crate) async fn refuses_an_unlicensed_key(
        &self,
        verb: &'static str,
        subject: &EntityId,
        exists: bool,
        caller: &EntityId,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Result<Option<CallToolResult>, McpError> {
        // **A thing being created is judged as given.** It has no former handle,
        // and resolving its handle would find the thing that used to wear the
        // name, whose rename freed it for this new one.
        let ceiling_subject = if exists && jojobot_domain::memory::names_a_guarded_key(fields) {
            match self.current_handle(subject).await {
                Ok(current) => current,
                Err(e) => return memory_declined(verb, e).map(Some),
            }
        } else {
            subject.clone()
        };
        // **A resend of the value a key holds is not a change**, so it is not
        // asked of the key's licence: the edit of a claim is judged on what the
        // fold would become, and a new claim is judged the same way. Only the
        // guarded keys that would move are judged.
        let changing = if exists && jojobot_domain::memory::names_a_guarded_key(fields) {
            match self
                .fields_that_would_change(&ceiling_subject, fields)
                .await
            {
                Ok(changing) => changing,
                Err(e) => return memory_declined(verb, e).map(Some),
            }
        } else {
            fields.clone()
        };
        let sent = fields;
        let fields = &changing;
        let lineage = match jojobot_domain::memory::needs_the_chart(fields) {
            Some(named) => {
                let read = if exists {
                    self.chart_around(&ceiling_subject, named).await
                } else {
                    self.chart_around_new(&ceiling_subject, named).await
                };
                match read {
                    Ok(lineage) => Some(lineage),
                    Err(e) => return memory_declined(verb, e).map(Some),
                }
            }
            None => None,
        };
        match jojobot_domain::memory::refuses_unlicensed_write(
            &ceiling_subject,
            caller,
            fields,
            lineage.as_ref(),
        ) {
            Some(refused) => memory_declined(verb, refused).map(Some),
            // A thing being created has no records to have been filled.
            None if exists => {
                self.refuses_a_filler_widening(verb, &ceiling_subject, caller, sent, &[])
                    .await
            }
            None => Ok(None),
        }
    }

    /// **The fields a write names that would not leave the thing as it is.** A
    /// guarded key sent at the value the thing holds is dropped from the answer; a
    /// key it does not hold, and a key at another value, stay. The held value is
    /// read through the store's folded fields and compared as the handle it names,
    /// since a store keeps the permanent id of what a key points at.
    pub(crate) async fn fields_that_would_change(
        &self,
        subject: &EntityId,
        written: &std::collections::BTreeMap<String, String>,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let held = self.memory.fields(subject).await?;
        let mut changing = std::collections::BTreeMap::new();
        for (key, value) in written {
            let guarded = jojobot_domain::memory::GUARDED_KEYS
                .iter()
                .any(|rule| rule.key == key.trim());
            let same = match held.get(key.trim()) {
                Some(now) if guarded => {
                    let now = now.trim();
                    let served = match now.strip_prefix(jojobot_domain::memory::mention::MARK) {
                        Some(id) => self
                            .current_handle(&EntityId(id.to_string()))
                            .await?
                            .to_string(),
                        None => now.to_string(),
                    };
                    served == value.trim()
                }
                _ => false,
            };
            if !same {
                changing.insert(key.clone(), value.clone());
            }
        }
        Ok(changing)
    }

    /// **The chart around a write**: the bots above `subject`, the manager the
    /// write names and the bots above that one. Read through the store's own
    /// folded fields, one bot at a time, and a read that fails refuses the
    /// write rather than waving it through.
    pub(crate) async fn chart_around(
        &self,
        subject: &EntityId,
        named: Option<EntityId>,
    ) -> Result<jojobot_domain::memory::Lineage, MemoryError> {
        let above = self.chain_above(subject).await?;
        let above_named = match &named {
            Some(manager) => self.chain_above(manager).await?,
            None => Vec::new(),
        };
        // Asked only by the write that names a manager for a thing with none.
        let has_reports = if above.is_empty() && named.is_some() {
            self.has_reports(subject).await?
        } else {
            false
        };
        let holds = jojobot_domain::memory::held_guarded_keys(&self.memory.fields(subject).await?);
        let heads = if jojobot_domain::memory::heads_wanted(&holds) {
            self.chart_heads().await?
        } else {
            Vec::new()
        };
        // **Who writes a thing is the domain's one answer**, the thoughts of a
        // thread and the fills of any thing together (rule 598).
        let thoughts = if subject.kind() == Some(jojobot_domain::memory::EntityKind::THREAD) {
            self.memory.thought_writers(subject).await?
        } else {
            Vec::new()
        };
        let fills = self.record_writers(subject).await?;
        let writers = jojobot_domain::memory::thread_writers::thread_writers(&thoughts, &fills);
        let mut chains = Vec::with_capacity(writers.len());
        for writer in &writers {
            chains.push(self.chain_above(writer).await?);
        }
        let above_writers = jojobot_domain::memory::thread_writers::above_the_writers(chains);
        Ok(jojobot_domain::memory::Lineage {
            above,
            named,
            above_named,
            has_reports,
            holds,
            heads,
            writers,
            above_writers,
        })
    }

    /// **The bots that head the chart**: each manager something reports to that
    /// has no manager of its own. Read through the store's folded fields one
    /// thing at a time, as the chain is. **This is the stores' own definition**,
    /// archived bots included: a report held by an archived bot still makes its
    /// manager a head, as the fake and the real store read it, so the guard here
    /// and the guard inside the write cannot disagree. The reading that skips
    /// archived entities is [`Jojobot::live_chart_heads`], a different question.
    async fn chart_heads(&self) -> Result<Vec<EntityId>, MemoryError> {
        let mut managers = std::collections::BTreeSet::new();
        for other in self.memory.list_entities(None).await? {
            let held = self.memory.fields(&other.id).await?;
            if let Some(manager) = held
                .get(jojobot_domain::memory::REPORTS_TO)
                .and_then(|value| jojobot_domain::memory::manager_in(value))
            {
                managers.insert(manager);
            }
        }
        let mut heads = Vec::new();
        for manager in managers {
            if self.chain_above(&manager).await?.is_empty() {
                heads.push(manager);
            }
        }
        Ok(heads)
    }

    /// **The chart around a thing being created**: nothing above it, because it
    /// holds no manager yet, and the bots above the manager the write names.
    ///
    /// **Whether anything reports to it is still asked.** A manager written
    /// before the store checked it can sit as text naming a handle nothing wore
    /// yet, and the thing made under that handle already has a report.
    async fn chart_around_new(
        &self,
        subject: &EntityId,
        named: Option<EntityId>,
    ) -> Result<jojobot_domain::memory::Lineage, MemoryError> {
        let above_named = match &named {
            Some(manager) => self.chain_above(manager).await?,
            None => Vec::new(),
        };
        let has_reports = if named.is_some() {
            self.has_reports(subject).await?
        } else {
            false
        };
        Ok(jojobot_domain::memory::Lineage {
            above: Vec::new(),
            named,
            above_named,
            has_reports,
            // A thing being created holds nothing.
            holds: Default::default(),
            heads: Vec::new(),
            writers: Vec::new(),
            above_writers: Vec::new(),
        })
    }

    /// **Whether any other thing reports to `subject`**, read through the
    /// store's folded fields one thing at a time, as the chain is.
    async fn has_reports(&self, subject: &EntityId) -> Result<bool, MemoryError> {
        for other in self.memory.list_entities(None).await? {
            if &other.id == subject {
                continue;
            }
            let held = self.memory.fields(&other.id).await?;
            if jojobot_domain::memory::reports_to(&held, subject) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// The bots above `start` on its `reports_to` chain, nearest first.
    pub(crate) async fn chain_above(&self, start: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        let mut walk = jojobot_domain::memory::ChainWalk::from(start);
        while let Some(at) = walk.at().cloned() {
            let held = self.memory.fields(&at).await?;
            walk.step(
                held.get(jojobot_domain::memory::REPORTS_TO)
                    .map(String::as_str),
            );
        }
        Ok(walk.above())
    }
}

#[cfg(test)]
mod retyped_subject_tests;
#[cfg(test)]
mod tests;
