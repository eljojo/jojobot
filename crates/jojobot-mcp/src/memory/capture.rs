//! `capture` — Remember one fact about an entity, with its provenance, at
//! most one edge, and — when it was derived from another claim rather than
//! from an entity — the claim it traces to.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use std::collections::BTreeMap;

use jojobot_domain::attention;
use jojobot_domain::memory::graph;

use super::*;
use crate::session::session_declined;
use crate::teaching::{
    CHECK_IN_DATE_TEACHING, CLAIM_DIRECTION_DOMAIN, CLAIM_DIRECTION_TEACHING, CLAIM_SUBJECT_DOMAIN,
    CLAIM_SUBJECT_TEACHING, CLAIMS_DOMAIN, CLAIMS_TEACHING, FIELD_SHADOWS_ARGUMENT_DOMAIN,
    PROJECTS_SKILL_DOMAIN, PROJECTS_SKILL_TEACHING, RHYTHM_HISTORY_DOMAIN,
};

/// Arguments to `capture`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — any `kind:slug` id (a bare handle is read
    /// as a person). **It must already exist**: a subject jojobot doesn't know
    /// comes back with candidates and nothing is written. Create it with
    /// `add_entity` first if it is genuinely new.
    pub(crate) subject: String,
    /// The crisp claim to remember — single line, no line breaks. Put the rest in
    /// `details`.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as the
    /// handle that thing wears today, even after a rename.
    pub(crate) content: String,
    /// Nuance, the why, merge notes — the description under the claim. It may
    /// hold paragraph breaks and reads back with every one; the claim above
    /// it stays one line. Carries `@kind:slug` mentions exactly as `content`
    /// does.
    #[serde(default)]
    pub(crate) details: Option<String>,
    /// `testimony` (the user said it), `observation` (you read it in a system
    /// of record) or `inference` (you derived it). Defaults to `inference`:
    /// anything not tied to the user's words is a hypothesis.
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
    /// you to confirm a standing. `update_fact` is where the gate sits, and
    /// only on the one move that takes a claim the operator hedged and calls
    /// it settled.
    #[serde(default)]
    pub(crate) standing: Option<String>,
    /// **The day the claim was MADE**, `YYYY-MM-DD` — the day it was said,
    /// decided or worked out. Defaults to the day your run is in.
    ///
    /// Not the day the thing happened: that is `happened_at`, and it is
    /// separate because one field answering both is how a vague answer becomes
    /// an invented date.
    ///
    /// **Naming a day that differs from the day your run is in is never
    /// refused** — the receipt carries `recorded_at_note`, one sentence
    /// naming both days, so a sitting acting out a different period does not
    /// silently record under the wrong one by mistake.
    #[serde(default)]
    pub(crate) recorded_at: Option<String>,
    /// **The day the thing this claim is about HAPPENED**, `YYYY-MM-DD` —
    /// a different question from `date`, which is about the claim.
    ///
    /// ⛔️ **Leave it off unless you were told a day.** *Over the summer* is not
    /// a day: a claim that says nothing about when the thing happened is a
    /// complete claim, and approximating one puts a date nobody stated on a
    /// record that may carry the operator's own authority. **jojobot never
    /// fills this in.**
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
    /// recorded). Requires `object`; neither works alone.
    ///
    /// **Draw `connection` with the subject as YOUR OWN bot handle and the
    /// claim becomes a thought of yours, not an ordinary claim about an
    /// entity.** A thing carrying `thought_capacity` holds only that many
    /// thoughts at once. A write that would go over is refused, naming how
    /// many are held and asking you to name one to drop and why — see
    /// `drop`, below. One nobody touches for long enough goes quiet on its
    /// own: still there, just no longer counted. `update_fact`'s
    /// `keep: true` is the designed way to say one still matters, on
    /// purpose, without changing a word of it.
    ///
    /// **`thought_capacity` is a ceiling, and the thing it binds cannot set
    /// it.** Send this key about your own handle and the write is refused,
    /// whatever else the call carries — only a different identity may raise
    /// or lower it.
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
    /// exactly as you wrote it. **A value that is a handle, or a comma list of
    /// handles, is a link and must name something that exists**; a handle
    /// inside a sentence is prose. **Nothing has to be declared first**, and no
    /// name for the class of thing is asked for: the fields ARE what the record
    /// says, and a type is something the keys answer rather than something you
    /// announce.
    ///
    /// **`"starred": "true"` competes for one of the few seats a bot's own
    /// boot spends on its rules, and it stays on the claim: it never folds onto
    /// the thing.** Going home unmarked is normal, not a
    /// failure: an unstarred rule is fetched with `facts: true` when it is
    /// needed rather than shown by default. Seats are few on purpose —
    /// starring everything is the same as starring nothing. **How many seats
    /// a bot has is `rule_seats`, written about it by a different identity** —
    /// a bot cannot raise its own — and a star or a count that would leave
    /// the boot over its size ceiling is refused, naming the overage.
    #[serde(default)]
    pub(crate) fields: Option<std::collections::BTreeMap<String, String>>,
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
    /// and the loop comes back then and is acted on again.
    ///
    /// **jojobot does the arithmetic.** It writes `outcome`, `last_check_in`
    /// (the day of this check-in, `recorded_at`, today when you send none) and,
    /// when the cycle is consumed, the new
    /// `counts_from` — worked out from the rhythm's own `advances_from`. Do not
    /// compute those yourself and do not send them in `fields`.
    ///
    /// **A snooze names the day it lasts until**, as `snoozed_until` in
    /// `fields` (`YYYY-MM-DD`), a day after this check-in's own, which is
    /// `recorded_at` (today when you send none). The loop is
    /// not owed again before that day, and on it the loop comes back and is
    /// acted on again; it falls due on the later of that day and its own. A
    /// snooze with no day, or a day that is not after that day, comes back
    /// blocked and nothing is written. The next `ran` or `skipped` check-in
    /// ends the snooze, so send `snoozed_until` with `snoozed` only.
    ///
    /// **What the check MEASURED is yours to send** in `fields`: a reading, a
    /// distance, a count. A cadence is always time, so a measurement is a field
    /// on the check-in and never a unit of the schedule.
    ///
    /// A rhythm short of its `cadence_days` or its `advances_from` comes back
    /// blocked naming the key it is short of, and nothing is written. A rhythm
    /// short only of `counts_from` still takes a `ran` or `skipped` check-in —
    /// that outcome OPENS the loop, and the basis becomes this check-in's own
    /// date. `snoozed` cannot open a loop, so it still refuses on a
    /// `counts_from` the rhythm does not hold.
    #[serde(default)]
    pub(crate) check_in: Option<String>,
    /// **The day after which this reading stops being good**, `YYYY-MM-DD`.
    /// Optional, and most claims never carry one.
    ///
    /// It is a fact about jojobot's knowledge rather than about the world: a
    /// pass that runs out on a date is a claim about the world and belongs in
    /// the content. Past the day a read SAYS SO — **the claim does not stop
    /// being true, it stops being trusted** — and **nothing else happens**: no
    /// sweep, no reminder, nobody is coming to check it. **A claim carrying no
    /// day made no promise**, which is neither fresh nor stale.
    #[serde(default)]
    pub(crate) stale_after: Option<String>,
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
    #[serde(default)]
    pub(crate) borrow: Option<bool>,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// **The keys a check-in computes**, which a caller therefore does not send.
/// **Every value this capture declared that the record does not carry.**
///
/// Only what the caller SENT is compared: a value it left off was defaulted,
/// not overruled, and the receipt already states each defaulted value on its
/// own key. The check-in path is the one that overrules today — it writes the
/// derivation its computed schedule makes the record into — and this is what
/// stops that being something a caller has to notice by comparing.
/// **Why a check-in stores a derivation whatever the caller declared.**
///
/// A fact about the record: it holds a schedule jojobot worked out beside the
/// caller's sentence, and a folded value is read with the certainty of the
/// claim that carried it. Stated so the substitution does not read as a fault
/// — it is correct, and a caller told only that its value was replaced learns
/// to distrust a verb that did the right thing.
const WHY_A_CHECK_IN_DERIVES: &str = "a check-in stores the schedule jojobot worked out beside your sentence, and a record \
     carrying both is a derivation";

/// **The other trigger for the same demotion.** The provenance downgrade
/// above fires on a check-in OR a plain capture that moves a stored due
/// moment on its own — see `due_on_set` — but until now the receipt's
/// explanation covered only the check-in case. A capture that never asked
/// for one got the identical substitution with no reason on the wire.
const WHY_A_MOVED_DUE_MOMENT_DERIVES: &str = "this capture moved the stored due moment on its own, without a check-in — the same \
     arithmetic a check-in does, run because a field it reads just changed — and a record \
     carrying both jojobot's schedule and your sentence is a derivation";

/// **The other direction of [`WHY_A_CHECK_IN_DERIVES`].** That sentence
/// explains why a check-in's computed schedule overrides a caller's own
/// value; it rides only when a check-in was asked for, so it says nothing on
/// the path every failing run actually takes — a rhythm's `counts_from` sent
/// by hand, with no `check_in` at all. Built from the same fact, stated the
/// other way round: a check-in is the only thing that derives `counts_from`,
/// so absent one, whatever the caller sent for it stands exactly as typed.
const WHY_THIS_BASIS_IS_HAND_TYPED: &str = "counts_from on this record is exactly what you sent — hand-typed, not derived, because this \
    capture asked for no check_in. A check-in is what stores the schedule jojobot works out \
    beside your sentence; capture one on this rhythm and jojobot computes it instead.";

/// **The mirror of [`WHY_THIS_BASIS_IS_HAND_TYPED`].** That sentence tells a
/// caller jojobot did not do the arithmetic; this one tells them it did, on
/// the one check-in where the basis came from nowhere but the check-in itself.
/// It names the day so the caller can see WHICH date was taken, which is the
/// only part they could not have predicted.
const WHY_THIS_BASIS_WAS_DERIVED: &str = "This check-in opened the loop: it held a cadence and no basis, so counts_from was derived \
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

    /// `converted` is the check-in's reason, given only when this call asked
    /// for one: the same substitution on a call that did not is not this
    /// verb's doing and must not borrow its explanation.
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
    /// **The keys a check-in on a rhythm writes**, or the refusal that says why
    /// it cannot.
    ///
    /// **What a caller's own capture did to the thing it named.**
    ///
    /// `capture` appends. It edits no record and it removes none, which is the
    /// property an agent has been observed not to believe — one declined to
    /// record a second account of an event because it expected the first to be
    /// overwritten, and the account was lost with nothing on the surface
    /// saying otherwise.
    ///
    /// **The keys are the half that keeps the line honest.** A record carrying
    /// fields folds into what the thing holds, so what those keys answer has
    /// moved even though no record was touched. Naming them is what stops
    /// *nothing was changed* being a promise this verb cannot keep.
    ///
    /// The count is read for this line and left out when the store cannot
    /// answer, because a number nobody can stand behind is worse than the
    /// sentence without one.
    ///
    /// **`checked_in` is what tells this apart from the case
    /// [`WHY_A_CHECK_IN_DERIVES`] already covers.** A rhythm's `counts_from`
    /// sent by hand with no check-in is not a difference — the stored value
    /// IS what the caller sent, so [`Difference::between`] has nothing to
    /// compare — and it is not caught anywhere else either, since a
    /// half-taught session or one that ignored the teaching still reaches
    /// `capture` directly. This is the receipt's own chance to say so.
    async fn what_a_capture_left_standing(
        &self,
        fact: &Fact,
        checked_in: bool,
        opened_the_loop: bool,
    ) -> String {
        let after = self.memory.recall(&fact.subject).await.ok();
        let standing = after
            .as_ref()
            .map(|facts| {
                facts
                    .iter()
                    .filter(|f| f.status == FactStatus::Active)
                    .count()
            })
            .map_or_else(String::new, |n| {
                format!(" {n} accounts now stand on {}.", fact.subject.as_str())
            });
        // **The emergency reserve is a VISIBLE debt, not a silent one.** A
        // thought landing while its own room already holds more than its
        // ceiling is the one moment this receipt can say so — a later read
        // of the room shows the overage structurally, but only this call
        // knows it JUST happened. Asked only when this fact could be a
        // room's own member at all, the same gate the write itself ran on.
        let debt = if fact
            .edge
            .as_ref()
            .is_some_and(|e| e.shape == EdgeShape::Connection)
        {
            let capacity = self
                .memory
                .fields(&fact.subject)
                .await
                .ok()
                .and_then(|f| f.get(jojobot_domain::memory::THOUGHT_CAPACITY).cloned())
                .and_then(|v| v.trim().parse::<usize>().ok());
            match (capacity, &after) {
                (Some(capacity), Some(facts)) => {
                    let live = jojobot_domain::memory::thought_room(facts).len();
                    if live > capacity {
                        format!(
                            " This used {}'s emergency reserve: its room now holds {live} of \
                             {capacity}, over its own ceiling. Bring it back by archiving a live \
                             thought with update_fact — the priority now is steering back to a \
                             clean state, not borrowing again.",
                            fact.subject.as_str()
                        )
                    } else {
                        String::new()
                    }
                }
                _ => String::new(),
            }
        } else {
            String::new()
        };
        // **Keys that reached the subject, told apart from keys that stayed.**
        // The six that describe their own claim never fold onto the thing, so a
        // receipt that said every key had moved to it told a bot that starred a
        // rule that the star was now the bot's. The set is the shipped
        // `record-labels` type, so a seventh key is placed with no edit here.
        let describing = crate::seed::describing_keys();
        let (stayed, reached): (Vec<&String>, Vec<&String>) = fact
            .fields
            .keys()
            .partition(|key| describing.contains(*key));
        let join = |keys: &[&String]| {
            keys.iter()
                .map(|key| key.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        };
        let keys = {
            let mut said = String::new();
            if !reached.is_empty() {
                said.push_str(&format!(
                    " This record carries {}, so what those keys answer for {} has moved to \
                     it; the records that set them are untouched and still say what they said.",
                    join(&reached),
                    fact.subject.as_str(),
                ));
            }
            if !stayed.is_empty() {
                said.push_str(&format!(
                    " {} stay on this record only: they describe it and do not fold onto {}. \
                     Read them with facts: true.",
                    join(&stayed),
                    fact.subject.as_str(),
                ));
            }
            said
        };
        let basis = if opened_the_loop {
            let derived = fact
                .fields
                .get(attention::COUNTS_FROM)
                .map(String::as_str)
                .unwrap_or_default();
            format!(" {WHY_THIS_BASIS_WAS_DERIVED} {derived}. Later cycles advance from it.")
        } else if !checked_in
            && fact.subject.kind() == Some(EntityKind::RHYTHM)
            && fact.fields.contains_key(attention::COUNTS_FROM)
        {
            format!(" {WHY_THIS_BASIS_IS_HAND_TYPED}")
        } else {
            String::new()
        };
        // **Archive is a visibility switch, not a validity gate** — citing an
        // archived claim as derived_from is permitted, so the caller has to
        // be told rather than left to notice on a later read. Silence here
        // would be the refusal this slice removed, arriving anyway as an
        // omission.
        let source_note = match &fact.derived_from {
            Some(source) => self
                .memory
                .recall(&source.home)
                .await
                .ok()
                .and_then(|facts| facts.into_iter().find(|f| f.id == source.local))
                .filter(|f| f.status == FactStatus::Archived)
                .map(|_| {
                    format!(
                        " The claim this was derived from, {source}, is archived — read it to \
                         judge whether the citation still holds."
                    )
                })
                .unwrap_or_default(),
            None => String::new(),
        };
        format!(
            "Recorded as an additional claim.{standing} No record was edited and none was \
             removed.{keys}{basis}{source_note}{debt}"
        )
    }

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
        if subject.kind() != Some(EntityKind::RHYTHM) {
            return Ok(Err(blocked_body(
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
                subject,
                &[],
                format!(
                    "Nothing was written. This call sends '{key}' in fields AND asks for a \
                     check-in, and a check-in computes '{key}' itself. Send the check-in without \
                     that key, or send the fields without check_in and keep the arithmetic."
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
                },
                follow: None,
                history: None,
            },
        )
        .await
        {
            Ok(found) => found
                .objects
                .first()
                .map(|object| object.fields.clone())
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
        description = "Remember one fact about an entity: the claim, when it became true, and \
                       whether it is testimony, observation or inference (default \
                       inference — a hypothesis, not a finding). A NEW THING THAT HAPPENED IS A \
                       NEW CLAIM, even about a thing that already carries one — capture it again \
                       rather than reaching for update_fact, because capture adds and never \
                       erases what stood before. OBSERVATION is a claim you READ \
                       out of a system of record, confidently — a statement, an app, a service — \
                       and it reads back settled, so it MUST say where: give fields a read_from \
                       naming the system, and a read_ref for what you read there if you have \
                       one. Without read_from the call is refused, because who confirmed \
                       something and on what is one question and half of it is no answer. Use it \
                       where nobody said it to you and you did not work it out. \
                       PROVENANCE AND STANDING ARE TWO QUESTIONS: provenance says \
                       WHO BACKS IT, standing says HOW SURE anyone is (settled or open). Leave \
                       standing off and it follows the provenance. Set it when the two come \
                       apart, and the case that matters is the operator stating something \
                       first-hand and hedging it — that is provenance testimony with \
                       standing open, and there is no other way to record it. It may also draw \
                       one typed edge at another entity. \
                       Returns the stored fact with the address you later edit it through. \
                       Every entity it names — the subject, and an edge's object — must \
                       ALREADY EXIST: one jojobot doesn't know comes back status: blocked with \
                       candidates and nothing is written. A genuinely new entity is two \
                       deliberate steps — add_entity, then capture. GIVE IT FIELDS: fields is \
                       a flat bag of key/value pairs jojobot stores as written; a handle or a \
                       comma list of handles is a link and must exist, and six keys stay on \
                       their claim. refs names the entities the record touches — those are links whose \
                       meaning is deliberately unrecorded, so they are searchable but assert \
                       nothing, which is what makes them not `about` edges. NOTHING HAS TO BE \
                       DECLARED FIRST and no name for the class of thing is asked for: a key you \
                       invent is kept as you wrote it, and a type is something the keys answer \
                       rather than something you announce. A field you send that equals what \
                       jojobot ships as its default today is stored exactly as you sent it either \
                       way, and the receipt's echoes_defaults names which key that was. \
                       derived_from names the claim this one \
                       was worked out from, as its address — use it when the source is another \
                       claim, not an entity. IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the \
                       address that edits it, the subject as it was qualified, the date, the \
                       provenance and the standing it was given — which is how a caller that \
                       named none of those learns what was recorded — and how many keys landed, \
                       with your claim elided and said to be. The write is still verified \
                       against the store before it is called a success; what stops is shipping \
                       you the words you just sent. recall the subject to read it back. A \
                       `thought` is a claim drawn as a connection edge with your OWN bot \
                       handle as the subject; `thought_capacity` caps how many are live at \
                       once. When the room is full, name one to `drop` and say why in \
                       `drop_because` (required with it), or pass `borrow` to land one over \
                       capacity — once, as the emergency reserve. A RULE FOR YOUR OWN BOOT: \
                       capture it on your own bot handle with `starred` set to \"true\" in fields \
                       and it competes for one of the few seats your boot spends on rules. An \
                       unstarred rule is kept all the same and reads back with recall's facts; \
                       when a new star pushes the oldest starred rule off its seat, the receipt \
                       names it."
    )]
    pub(crate) async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let declared = Declared::of(&args);
        let checked_in = args.check_in.is_some();
        let subject = EntityId::person(&args.subject);
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
        if let Some(refused) = jojobot_domain::memory::refuses_role_fields(fields.keys()) {
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
        let mut opened_the_loop = false;
        if let Some(outcome) = args.check_in.as_deref() {
            match self
                .check_in(&subject, outcome, recorded_at, &fields)
                .await?
            {
                Ok((computed, opened)) => {
                    opened_the_loop = opened;
                    fields.extend(computed);
                }
                Err(refused) => return Ok(refused),
            }
        }
        // **Kept current here too, not only on a check-in.** A cadence edited
        // by a plain capture is a write like any other, and the due moment it
        // carries is jojobot's own arithmetic riding along in the same
        // record — exactly as a check-in's own computed keys already do.
        let (due_on_computed, due_on_derived) = self.moved_due_moment(&subject, &fields, &[]).await;
        let due_on_set = matches!(due_on_computed, attention::DueMove::Set(_)) && due_on_derived;
        if let attention::DueMove::Set(due_on) = due_on_computed {
            fields.insert(attention::DUE_ON.to_string(), due_on.to_string());
        }
        // **A capture can only ADD fields to a record**: `NewFact` has no
        // clear_fields of its own. So when the write moves the thing out of owing
        // anything, the stored moment is taken off by the same `clear_fields` an
        // edit uses, on the record that carries it. See `clear_stored_due_moment`.
        let clears_due_on = matches!(due_on_computed, attention::DueMove::Cleared);
        //
        // **jojobot's own arithmetic is jojobot's, whatever the caller said
        // about their claim.** A check-in computes the schedule keys, and
        // merging them into a record carrying `testimony` made a date nobody
        // uttered read back as the user's word — permanently, since a folded
        // value is read with the certainty of the claim that carried it. A
        // claim mixing the two is written as a derivation, and the caller's
        // words are not what is being demoted: the record is a caller's
        // sentence and a computed schedule together, and only one of those
        // has anybody's word behind it. A moved due moment is the same
        // arithmetic on a plain capture that never asked for a check-in —
        // **but only where jojobot worked the day out.** A promise's day, a
        // run-out day and a decision's day are copied from a date the caller
        // wrote, which adds no date nobody said, so that record stays theirs.
        let provenance = if args.check_in.is_some() || due_on_set {
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
        let aged_before = if subject.kind() == Some(EntityKind::BOT)
            && edge
                .as_ref()
                .is_some_and(|e| e.shape == EdgeShape::Connection)
        {
            match self.sessions.summaries_of(&subject).await {
                Ok(runs) => jojobot_domain::memory::aging_cutoff(
                    &runs.iter().map(|r| r.started_at).collect::<Vec<_>>(),
                ),
                Err(e) => return session_declined(e, caller.sid.as_str()),
            }
        } else {
            None
        };

        let new = NewFact {
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
            drop,
            drop_because: args.drop_because,
            borrow: args.borrow.unwrap_or(false),
            aged_before,
            session: Some(caller.sid.as_str().to_string()),
        };
        // **A star or a seat count that would take the bot's boot over its
        // ceiling is refused here**, before anything lands — see
        // `refuses_a_boot_floor_over`.
        if let Some(refused) = self
            .refuses_a_boot_floor_for_capture(&new, &caller.bot)
            .await
        {
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
        let (captured, fold_behind) = match self.memory.capture(new).await {
            Ok(captured) => (captured, None),
            Err(MemoryError::FoldBehind {
                landed: Landed::Fact(fact),
                behind,
                ..
            }) => (Guarded::Written(*fact), Some(behind)),
            Err(e) => return memory_declined("capture", e),
        };
        match captured {
            Guarded::Written(fact) => {
                if clears_due_on {
                    self.clear_stored_due_moment(&fact.subject, &caller.bot)
                        .await;
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
                // **Either trigger for the provenance demotion gets its own
                // reason.** `checked_in` and `due_on_set` fire the same
                // substitution above; the explanation used to ride only with
                // the first.
                let why_demoted = if checked_in {
                    Some(WHY_A_CHECK_IN_DERIVES)
                } else if due_on_set {
                    Some(WHY_A_MOVED_DUE_MOMENT_DERIVES)
                } else {
                    None
                };
                crate::answer::note_delta(&mut body, declared.not_stored(&fact, why_demoted));
                crate::answer::note_postcondition(
                    &mut body,
                    self.what_a_capture_left_standing(&fact, checked_in, opened_the_loop)
                        .await,
                );
                self.note_seat_pushed_off(&fact, &mut body).await;
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
                if crate::teaching::is_project_work(fact.subject.kind())
                    && self
                        .first_contact(PROJECTS_SKILL_DOMAIN, Some(&caller))
                        .await
                {
                    crate::answer::note_teaching(&mut body, PROJECTS_SKILL_TEACHING);
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
        let ceiling_subject = if jojobot_domain::memory::names_a_guarded_key(fields) {
            match self.current_handle(subject).await {
                Ok(current) => current,
                Err(e) => return memory_declined(verb, e).map(Some),
            }
        } else {
            subject.clone()
        };
        let lineage = match jojobot_domain::memory::needs_the_chart(fields) {
            Some(named) => {
                let read = if exists {
                    self.chart_around(&ceiling_subject, named).await
                } else {
                    self.chart_around_new(named).await
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
            None => Ok(None),
        }
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
        Ok(jojobot_domain::memory::Lineage {
            above,
            named,
            above_named,
            has_reports,
        })
    }

    /// **The chart around a thing being created**: nothing above it, because it
    /// holds no manager yet, and the bots above the manager the write names.
    async fn chart_around_new(
        &self,
        named: Option<EntityId>,
    ) -> Result<jojobot_domain::memory::Lineage, MemoryError> {
        let above_named = match &named {
            Some(manager) => self.chain_above(manager).await?,
            None => Vec::new(),
        };
        Ok(jojobot_domain::memory::Lineage {
            above: Vec::new(),
            named,
            above_named,
            // A thing being created has nobody reporting to it yet.
            has_reports: false,
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
    async fn chain_above(&self, start: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
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
mod tests;
