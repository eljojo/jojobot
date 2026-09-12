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
use crate::teaching::{
    CLAIM_SUBJECT_DOMAIN, CLAIM_SUBJECT_TEACHING, CLAIMS_DOMAIN, CLAIMS_TEACHING,
};

/// Arguments to `capture`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct CaptureArgs {
    /// The entity the fact is about — any `kind:slug` id (a bare handle is read
    /// as a person). **It must already exist**: a subject jojobot doesn't know
    /// comes back with candidates and nothing is written. Create it with
    /// `add_entity` first if it is genuinely new.
    pub(crate) subject: String,
    /// The crisp claim to remember — single line, no line breaks.
    ///
    /// Write `@kind:slug` to link to something that already exists, e.g.
    /// `@person:milhouse` — stored as the name that does not move, served as the
    /// handle that thing wears today, even after a rename.
    pub(crate) content: String,
    /// Nuance, the why, merge notes — the description under the claim.
    /// Carries `@kind:slug` mentions exactly as `content` does.
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
    /// The shape of the edge this fact draws: `location` (object is a place) ·
    /// `membership` (an org) · `attendance` (an event) · `about` (any kind) ·
    /// `connection` (any kind — a link is there and how it relates was not
    /// recorded). Requires `object`; neither works alone.
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
    /// Flat and free-form on purpose: jojobot stores what you put here and
    /// interprets none of it, and a key it has never seen is kept exactly as
    /// you wrote it. **Nothing has to be declared first**, and no name for the
    /// class of thing is asked for: the fields ARE what the record says, and a
    /// type is something the keys answer rather than something you announce.
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
    /// consume the cycle: the rhythm comes back at its own date and is acted on
    /// again.
    ///
    /// **jojobot does the arithmetic.** It writes `outcome`, `last_check_in`
    /// (the `date` of this record) and, when the cycle is consumed, the new
    /// `counts_from` — worked out from the rhythm's own `advances_from`. Do not
    /// compute those yourself and do not send them in `fields`.
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
        let standing = self
            .memory
            .recall(&fact.subject)
            .await
            .ok()
            .map(|facts| {
                facts
                    .iter()
                    .filter(|f| f.status == FactStatus::Active)
                    .count()
            })
            .map_or_else(String::new, |n| {
                format!(" {n} accounts now stand on {}.", fact.subject.as_str())
            });
        let keys = if fact.fields.is_empty() {
            String::new()
        } else {
            format!(
                " This record carries {}, so what those keys answer for {} has moved to it; the \
                 records that set them are untouched and still say what they said.",
                fact.fields.keys().cloned().collect::<Vec<_>>().join(", "),
                fact.subject.as_str(),
            )
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
             removed.{keys}{basis}{source_note}"
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

        let held = match graph::walk(
            self.memory.as_ref(),
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

        match attention::check_in(&held, outcome, on) {
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
                       a flat bag of key/value pairs jojobot stores and never interprets, and \
                       refs names the entities the record touches — those are links whose \
                       meaning is deliberately unrecorded, so they are searchable but assert \
                       nothing, which is what makes them not `about` edges. NOTHING HAS TO BE \
                       DECLARED FIRST and no name for the class of thing is asked for: a key you \
                       invent is kept as you wrote it, and a type is something the keys answer \
                       rather than something you announce. derived_from names the claim this one \
                       was worked out from, as its address — use it when the source is another \
                       claim, not an entity. IT ANSWERS WITH A RECEIPT, NOT THE RECORD: the \
                       address that edits it, the subject as it was qualified, the date, the \
                       provenance and the standing it was given — which is how a caller that \
                       named none of those learns what was recorded — and how many keys landed, \
                       with your claim elided and said to be. The write is still verified \
                       against the store before it is called a success; what stops is shipping \
                       you the words you just sent. recall the subject to read it back."
    )]
    pub(crate) async fn capture(
        &self,
        Parameters(args): Parameters<CaptureArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        let caller = match self.identified(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let declared = Declared::of(&args);
        let checked_in = args.check_in.is_some();
        let subject = EntityId::person(&args.subject);
        let provenance = parse_provenance(args.provenance.as_deref())?;
        let recorded_at = self.dated(args.recorded_at.as_deref(), args.sid.as_deref())?;
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

        let mut fields = args.fields.unwrap_or_default();
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
        // **jojobot's own arithmetic is jojobot's, whatever the caller said
        // about their claim.** A check-in computes the schedule keys, and
        // merging them into a record carrying `testimony` made a date nobody
        // uttered read back as the user's word — permanently, since a folded
        // value is read with the certainty of the claim that carried it. A
        // claim mixing the two is written as a derivation, and the caller's
        // words are not what is being demoted: the record is a caller's
        // sentence and a computed schedule together, and only one of those
        // has anybody's word behind it.
        let provenance = if args.check_in.is_some() {
            Provenance::Inference
        } else {
            provenance
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
        };
        // Routed through the declined path rather than straight to the mapper:
        // a fact the validators refuse is a caller mistake, and it comes back
        // as an answer with a way forward (rule 68).
        let captured = match self.memory.capture(new).await {
            Ok(captured) => captured,
            Err(e) => return memory_declined("capture", e),
        };
        match captured {
            Guarded::Written(fact) => {
                self.beat("capture", fact.subject.as_str(), args.sid.as_deref())
                    .await;
                // **As of the day the RUN is asking about, never the day the
                // claim is about** (rule 222). `date` above is what the claim
                // is true OF, which is a different question from whether its
                // reading still stands today — and the read that follows this
                // write answers as of today, so a receipt answering as of the
                // claim's day contradicts it inside one session.
                let mut body = fact_receipt_json(&fact, self.dated(None, args.sid.as_deref())?);
                crate::answer::note_delta(
                    &mut body,
                    declared.not_stored(&fact, checked_in.then_some(WHY_A_CHECK_IN_DERIVES)),
                );
                crate::answer::note_postcondition(
                    &mut body,
                    self.what_a_capture_left_standing(&fact, checked_in, opened_the_loop)
                        .await,
                );
                if self.first_contact(CLAIMS_DOMAIN, Some(&caller)).await {
                    crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
                }
                if self
                    .first_contact(CLAIM_SUBJECT_DOMAIN, Some(&caller))
                    .await
                {
                    crate::answer::note_teaching(&mut body, CLAIM_SUBJECT_TEACHING);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::*;

    /// A rhythm with a whole schedule on it, ready to take a check-in.
    async fn a_weekly_rhythm(jojobot: &Jojobot, handle: &str, counts_from: &str, advances: &str) {
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
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), advances.to_string()),
                        ("counts_from".to_string(), counts_from.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("rhythm:{handle}"), "every week")
            },
        )
        .await;
    }

    /// What a thing holds, folded, as `recall` renders it.
    async fn fields_of(jojobot: &Jojobot, subject: &str) -> serde_json::Value {
        let body = json_of(
            &jojobot
                .recall(Parameters(recall_args(subject)))
                .await
                .expect("recall ok"),
        );
        body["objects"][0]["fields"].clone()
    }

    /// **A schedule jojobot worked out is not something the user said.**
    ///
    /// A check-in computes the dates the next cycle counts from. Merged into
    /// the caller's own record, those keys make a record captured as testimony
    /// read a date nobody uttered back as the user's word. **A folded value is
    /// read with the certainty of the claim that carried it**, so that is
    /// permanent and invisible.
    ///
    /// **Paired with an ordinary capture in the same case**: testimony stays
    /// testimony when nothing was computed, so this cannot pass against a build
    /// that files everything as a derivation.
    #[tokio::test]
    async fn a_computed_schedule_does_not_inherit_the_callers_word() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

        let checked_in = capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                provenance: Some("testimony".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "did it this morning")
            },
        )
        .await;
        assert_eq!(
            checked_in["provenance"], "inference",
            "a record carrying dates jojobot computed reads as the user's own word: {checked_in}",
        );

        let said = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("rhythm:descale", "he says it is due fortnightly now")
            },
        )
        .await;
        assert_eq!(
            said["provenance"], "testimony",
            "a claim with nothing computed in it was demoted too: {said}",
        );
    }

    /// **A value the store did not keep as it was sent is named on the
    /// receipt.**
    ///
    /// A caller sends `provenance` and gets a record carrying a different one:
    /// the check-in path writes its own, because the record it builds mixes a
    /// caller's sentence with a schedule jojobot computed. The substitution is
    /// correct and the silence is not — a caller that cannot see its own value
    /// replaced has to price every other write at its worst case.
    ///
    /// **Paired with a capture where nothing differs**, which carries no delta
    /// at all: a line that prints on every write is noise a reader learns to
    /// skip, and the pair is what keeps this one meaningful.
    #[tokio::test]
    async fn a_stored_value_that_differs_from_the_sent_one_is_named() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

        let substituted = capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                provenance: Some("testimony".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "did it this morning")
            },
        )
        .await;

        let delta = &substituted["delta"];
        assert!(
            delta.is_array(),
            "the receipt of a write that replaced a caller's value says so: {substituted}",
        );
        let replaced = delta
            .as_array()
            .expect("checked above")
            .iter()
            .find(|d| d["field"] == "provenance")
            .unwrap_or_else(|| panic!("the replaced field is named: {substituted}"));
        assert_eq!(replaced["sent"], "testimony", "{substituted}");
        assert_eq!(replaced["stored"], "inference", "{substituted}");

        let kept = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("rhythm:descale", "he says it is due fortnightly now")
            },
        )
        .await;
        assert_eq!(
            kept["delta"],
            serde_json::Value::Null,
            "a write that kept every value it was sent carries no delta: {kept}",
        );
    }

    /// A check-in that sends a provenance the check-in path overrules — the
    /// one call on this surface known to store a value other than the one it
    /// was sent.
    fn a_late_check_in() -> CaptureArgs {
        CaptureArgs {
            check_in: Some("ran".into()),
            provenance: Some("testimony".into()),
            recorded_at: Some("2026-08-10".into()),
            ..capture_args("rhythm:descale", "did it this morning")
        }
    }

    /// **A write says what now stands and what it left alone.**
    ///
    /// The measured failure this answers: an agent declined to record a second
    /// account of an event because it believed the write would overwrite the
    /// first. It would not have. Nothing on the surface said so, and the
    /// information was lost permanently and silently.
    ///
    /// ⚠️ **The line is computed, never a constant.** A capture carrying fields
    /// moves what those keys answer for the thing, so the line names them; a
    /// capture carrying none moves nothing and names none. **That difference is
    /// the case**: a line that reads *nothing was changed* unconditionally is a
    /// false promise in the one place a caller has been taught to trust, which
    /// is worse than no line at all.
    #[tokio::test]
    async fn a_capture_says_what_now_stands_and_what_it_left_alone() {
        let jojobot = handler();

        let first = capture_ok(
            &jojobot,
            capture_args("person:alpha", "said the kiln was lit"),
        )
        .await;
        let opening = postcondition_of(&first);
        assert!(
            opening.contains('1'),
            "the line has to say how much now stands on this thing: {first}",
        );

        // The second account of the same thing, contradicting the first. This
        // is the write an agent talked itself out of.
        let second = capture_ok(
            &jojobot,
            capture_args("person:alpha", "said the kiln had never been lit"),
        )
        .await;
        let both = postcondition_of(&second);
        assert!(
            both.contains('2'),
            "two accounts now stand and the line has to say so: {second}",
        );
        assert!(
            !both.contains("mood") && !both.contains("kiln"),
            "a write that carried no keys names none: {second}",
        );

        // The same verb, this time displacing what a key answers.
        let with_keys = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("mood".to_string(), "delighted".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("person:alpha", "was pleased about it")
            },
        )
        .await;
        let moved = postcondition_of(&with_keys);
        assert!(
            moved.contains("mood"),
            "this write moved what 'mood' answers for the thing, and the line that says nothing \
             changed is a false promise unless it names it: {with_keys}",
        );
    }

    /// The postcondition line of a receipt, which every write carries.
    fn postcondition_of(body: &serde_json::Value) -> String {
        body["postcondition"]
            .as_str()
            .unwrap_or_else(|| panic!("a write states what now stands: {body}"))
            .to_string()
    }

    /// **A delta says why, where the verb that substituted has a reason.**
    ///
    /// ⚠️ **The line must not read as an apology.** This substitution is
    /// correct: the record mixes a caller's sentence with a schedule jojobot
    /// computed, and only one of those has anybody's word behind it. A bare
    /// *stored differs from sent* reads as a fault report, and a caller that
    /// reads it as one learns to distrust a verb that did the right thing.
    ///
    /// **The reason is a fact about the record, not about how the server went
    /// about the write** — it says what the stored value IS and why that is
    /// what the record can carry, which is where rule 158 draws the line.
    ///
    /// ⚠️ **What the pairing below watches, and what it cannot.** It catches a
    /// reason smeared onto every difference. It does NOT catch the reason
    /// escaping to a provenance substitution some other verb makes, and no
    /// case can: `check_in` is the only path on this surface that stores a
    /// provenance other than the one it was sent, so a build that attached
    /// this reason unconditionally is indistinguishable from this one through
    /// the served surface. The condition is held by construction. **The day a
    /// second substituting path lands, that is the case to write.**
    #[tokio::test]
    async fn a_substitution_with_a_reason_carries_it() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
        let receipt = capture_ok(&jojobot, a_late_check_in()).await;

        let because = receipt["delta"][0]["because"]
            .as_str()
            .unwrap_or_else(|| panic!("the substitution this verb makes has a reason: {receipt}"))
            .to_string();
        assert!(
            because.contains("check_in") || because.contains("schedule"),
            "the reason has to say what about this call replaced the value: {receipt}",
        );

        // The rendered line carries it too, since that is the half a reader
        // reads rather than branches on.
        let note = receipt["delta_note"].as_str().expect("a rendered line");
        assert!(note.contains(&because), "{receipt}");

        // ⭐ **Paired with a difference that has no reason to give.** A caller
        // that names a bare handle gets it qualified, and nothing about that
        // needs explaining — so `because` is absent rather than filled with a
        // sentence restating the comparison. This is the half that fails on a
        // build attaching the reason to every difference.
        let qualified = capture_ok(
            &jojobot,
            CaptureArgs {
                ..capture_args("alpha", "said the kiln was lit")
            },
        )
        .await;
        assert_eq!(qualified["delta"][0]["field"], "subject", "{qualified}");
        assert_eq!(
            qualified["delta"][0]["because"],
            serde_json::Value::Null,
            "a difference with nothing to explain carries no explanation: {qualified}",
        );
    }

    /// **A check-in records what was found, and jojobot does the arithmetic.**
    ///
    /// The caller says which of the three outcomes it was and on what day; the
    /// date the next cycle counts from is worked out here, because a caller
    /// doing that by hand is a caller who can get it wrong once and never find
    /// out.
    ///
    /// The three outcomes together, because the difference between them is the
    /// whole point of having three: a run and a skip move the schedule
    /// identically and only the record tells them apart, while a snooze moves
    /// nothing at all.
    #[tokio::test]
    async fn a_check_in_moves_the_schedule_and_a_snooze_does_not() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "descaled it, took ten minutes")
            },
        )
        .await;
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(held["outcome"], "ran");
        assert_eq!(held["last_check_in"], "2026-08-10");
        assert_eq!(
            held["counts_from"], "2026-08-10",
            "advancing from the check-in date moves the cycle to the day it happened: {held}",
        );

        // A snooze is still a check-in — it says when it happened — and the
        // schedule is exactly where it was.
        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("snoozed".into()),
                recorded_at: Some("2026-08-19".into()),
                ..capture_args("rhythm:descale", "not this week")
            },
        )
        .await;
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(held["outcome"], "snoozed");
        assert_eq!(held["last_check_in"], "2026-08-19");
        assert_eq!(
            held["counts_from"], "2026-08-10",
            "a snooze does not consume the cycle, so the schedule is untouched: {held}",
        );

        // And a skip advances it exactly as the run did, leaving a record that
        // says it did not happen — which is the only place the two differ.
        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("skipped".into()),
                recorded_at: Some("2026-08-20".into()),
                ..capture_args("rhythm:descale", "away, not doing it")
            },
        )
        .await;
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(held["outcome"], "skipped");
        assert_eq!(
            held["counts_from"], "2026-08-20",
            "a skipped cycle advances as if it had run: {held}",
        );
    }

    /// **A measurement rides on the check-in, never on the schedule.**
    ///
    /// A cadence is always time. What the check found — a reading, a distance,
    /// a count — is the caller's own key on the same record, kept as written
    /// beside the keys jojobot computed.
    #[tokio::test]
    async fn a_check_in_carries_the_callers_own_measurement() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "read-meter", "2026-08-01", "due_date").await;

        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-12".into()),
                fields: Some(
                    [("reading_kwh".to_string(), "4184".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("rhythm:read-meter", "read it off the dial")
            },
        )
        .await;

        let held = fields_of(&jojobot, "rhythm:read-meter").await;
        assert_eq!(
            held["reading_kwh"], "4184",
            "the caller's key is kept: {held}"
        );
        assert_eq!(
            held["counts_from"], "2026-08-08",
            "and this one advances from the date it fell due, not from the late check-in: {held}",
        );
    }

    /// **A rhythm that cannot say what it advances from takes no check-in.**
    ///
    /// The choice has no default, deliberately: the two answers diverge exactly
    /// when a check-in is late, and guessing wrong leaves a rhythm that re-arms
    /// itself for ever. So the refusal names the key and both values, and
    /// writes nothing.
    #[tokio::test]
    async fn a_rhythm_with_no_advances_from_refuses_the_check_in_and_writes_nothing() {
        let jojobot = handler();
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
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("rhythm:half-made", "every week, roughly")
            },
        )
        .await;

        let refused = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("ran".into()),
                    recorded_at: Some("2026-08-12".into()),
                    ..capture_args("rhythm:half-made", "did it")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked");
        assert_eq!(refused["wrote"], false);
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a blocked answer says how to proceed");
        assert!(
            how.contains("advances_from")
                && how.contains("due_date")
                && how.contains("check_in_date"),
            "the way forward names the key and both values it takes: {how}",
        );

        // Nothing was written — not the record, and not the outcome key that a
        // half-applied check-in would have left behind.
        let held = fields_of(&jojobot, "rhythm:half-made").await;
        assert_eq!(held["outcome"], serde_json::Value::Null);
        assert_eq!(held["last_check_in"], serde_json::Value::Null);
    }

    /// **A key the check-in computes is a contradiction when the caller sends
    /// it too**, not an override.
    ///
    /// The record would say two things about one schedule and nothing could say
    /// which was meant, so the call is refused and nothing is written. The
    /// refusal names the key, because which of the three it was is what the
    /// caller has to remove.
    #[tokio::test]
    async fn a_check_in_that_also_sends_a_computed_key_is_refused() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

        let refused = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("ran".into()),
                    recorded_at: Some("2026-08-10".into()),
                    fields: Some(
                        [("counts_from".to_string(), "2026-09-01".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args(
                        "rhythm:descale",
                        "descaled it, and moved the schedule myself",
                    )
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked");
        assert_eq!(refused["wrote"], false);
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .expect("a blocked answer says how to proceed")
                .contains("counts_from"),
            "the way forward names the key that is doubled: {refused}",
        );

        // Nothing moved: not the caller's date, and not the arithmetic either.
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(
            held["counts_from"], "2026-08-01",
            "the schedule is where it was: {held}",
        );
        assert_eq!(held["outcome"], serde_json::Value::Null);

        // The paired positive: the same check-in without that key lands, so the
        // refusal is about the contradiction and not about the check-in.
        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "descaled it")
            },
        )
        .await;
        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(held["counts_from"], "2026-08-10");
    }

    /// **A check-in is rhythm vocabulary**, and a subject of any other kind is
    /// refused rather than quietly written with keys that mean nothing on it.
    ///
    /// ⚠️ **The subject carries a whole schedule on purpose.** Matching is
    /// structural everywhere else here, so a `thing` holding the three keys
    /// answers every question the schedule reader asks — which leaves the KIND
    /// as the only thing that can refuse this call. Without those keys the case
    /// passed against a build with no kind check at all, refused by the
    /// half-configured gate instead and asserting nothing it claimed to.
    #[tokio::test]
    async fn a_check_in_on_something_that_is_not_a_rhythm_is_refused() {
        let jojobot = handler();
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), "check_in_date".to_string()),
                        ("counts_from".to_string(), "2026-08-01".to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("thing:kettle", "descaled weekly, in principle")
            },
        )
        .await;

        let refused = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("ran".into()),
                    recorded_at: Some("2026-08-10".into()),
                    ..capture_args("thing:kettle", "boiled it")
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["status"], "blocked");
        assert_eq!(refused["wrote"], false);
        assert_eq!(refused["attempted"], "thing:kettle");

        // The paired positive: nothing about the schedule was the problem, so
        // the same keys on a real rhythm take the same check-in.
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "boiled it")
            },
        )
        .await;

        // And the refused call left nothing on the thing.
        let held = fields_of(&jojobot, "thing:kettle").await;
        assert_eq!(held["outcome"], serde_json::Value::Null);
        assert_eq!(held["last_check_in"], serde_json::Value::Null);
    }

    /// **The path the teaching in `add_entity` cannot reach: a session that
    /// never saw it, or saw it and hand-typed the schedule anyway.** The
    /// receipt says so — the basis this cycle counts from is hand-typed, not
    /// derived — and names the call that would derive it instead.
    ///
    /// Three calls, because either negative alone passes on a build that
    /// always attaches the note or never does. A check-in derives
    /// `counts_from` itself — `WHY_A_CHECK_IN_DERIVES` already covers that
    /// path — and a capture naming no schedule key is not this case at all.
    #[tokio::test]
    async fn a_hand_typed_schedule_basis_says_so_in_the_receipt_and_only_then() {
        let jojobot = handler();
        a_weekly_rhythm(&jojobot, "descale", "2026-08-01", "check_in_date").await;

        // The failing path: `counts_from` sent by hand, no `check_in`.
        let hand_typed = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [("counts_from".to_string(), "2026-09-01".to_string())]
                        .into_iter()
                        .collect(),
                ),
                ..capture_args("rhythm:descale", "moved the schedule myself")
            },
        )
        .await;
        let note = hand_typed["postcondition"]
            .as_str()
            .expect("a postcondition string");
        assert!(
            note.contains("hand-typed") && note.contains("check_in"),
            "the receipt says the basis is hand-typed and names the call that would derive it: \
             {note}"
        );

        // The paired negative: an ordinary check-in derives `counts_from`
        // itself, so it is not this case.
        let checked_in = capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-08-10".into()),
                ..capture_args("rhythm:descale", "descaled it")
            },
        )
        .await;
        assert!(
            !checked_in["postcondition"]
                .as_str()
                .expect("a postcondition string")
                .contains("hand-typed"),
            "a check-in derives counts_from itself, so it is not hand-typed: {checked_in}"
        );

        // The second negative: a capture naming no schedule key at all is
        // not this case either.
        let untouched = capture_ok(
            &jojobot,
            capture_args("rhythm:descale", "just a note about it"),
        )
        .await;
        assert!(
            !untouched["postcondition"]
                .as_str()
                .expect("a postcondition string")
                .contains("hand-typed"),
            "a capture naming no schedule key is not this case: {untouched}"
        );
    }

    /// A loop that holds a cadence and a policy and has never been checked in
    /// — what `add_entity` plus one ordinary capture leaves behind, and the
    /// shape every observed opening actually starts from.
    async fn a_cadenced_rhythm_with_no_basis(jojobot: &Jojobot, handle: &str, advances: &str) {
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
                        ("cadence_days".to_string(), "7".to_string()),
                        ("advances_from".to_string(), advances.to_string()),
                    ]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args(&format!("rhythm:{handle}"), "every week")
            },
        )
        .await;
    }

    /// Which loops a `recall` says have gone quiet as of a day.
    async fn quiet_as_of(jojobot: &Jojobot, as_of: &str) -> Vec<String> {
        let read = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    kind: Some("rhythm".into()),
                    subject: None,
                    overdue: Some(super::recall::OverdueArgs {
                        as_of: Some(as_of.into()),
                    }),
                    ..recall_args("rhythm:descale")
                }))
                .await
                .expect("recall ok"),
        );
        read["objects"]
            .as_array()
            .map(|objects| {
                objects
                    .iter()
                    .filter_map(|one| one["id"].as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 🚨 **A back-dated check-in ALONE opens a loop, with no basis typed by
    /// hand.** This is the route the rhythms procedure teaches, and until the
    /// opening derive existed the engine refused it: a check-in was blocked
    /// until `counts_from` was already there, and `counts_from` cannot ride a
    /// check-in call, so the only way to open a loop was to type the one value
    /// a check-in exists to derive.
    ///
    /// **The whole journey, through the served surface**: capture writes it,
    /// the store folds it, and the loop's own carrier computes a due date the
    /// read selects on. Asserted from BOTH sides of the boundary, because a
    /// build that put the basis anywhere else still answers one side
    /// correctly.
    #[tokio::test]
    async fn a_back_dated_check_in_alone_opens_a_loop_that_holds_a_cadence() {
        let jojobot = handler();
        a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

        capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args(
                    "rhythm:descale",
                    "did it back before this session opened it",
                )
            },
        )
        .await;

        let held = fields_of(&jojobot, "rhythm:descale").await;
        assert_eq!(
            held["counts_from"], "2026-06-14",
            "the basis is the check-in's own date: {held}"
        );
        assert_eq!(
            held["last_check_in"], "2026-06-14",
            "and the turn is recorded on the day it happened: {held}"
        );

        // A cadence of seven from the fourteenth falls due on the twenty-first.
        assert!(
            !quiet_as_of(&jojobot, "2026-06-20")
                .await
                .contains(&"rhythm:descale".to_string()),
            "the day before it falls due, the loop is not owed",
        );
        assert!(
            quiet_as_of(&jojobot, "2026-06-21")
                .await
                .contains(&"rhythm:descale".to_string()),
            "and a cadence after the check-in's date, it is",
        );
    }

    /// **The receipt says the basis was derived, and says it only when it
    /// was.** The mirror of the hand-typed sentence beside it: one caller
    /// learns jojobot did the arithmetic, the other learns it did not.
    ///
    /// Three calls, because either negative alone passes on a build that
    /// always attaches the note or never does.
    #[tokio::test]
    async fn the_receipt_says_when_a_check_in_opened_the_loop_and_only_then() {
        let jojobot = handler();
        a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

        let opening = capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-14".into()),
                ..capture_args("rhythm:descale", "did it back then")
            },
        )
        .await;
        let note = opening["postcondition"]
            .as_str()
            .expect("a postcondition string");
        assert!(
            note.contains("derived") && note.contains("2026-06-14"),
            "the receipt says the basis was derived and from which day: {note}"
        );

        // The paired negative: the NEXT check-in on the same loop advances a
        // basis that was already there, so it opened nothing.
        let later = capture_ok(
            &jojobot,
            CaptureArgs {
                check_in: Some("ran".into()),
                recorded_at: Some("2026-06-28".into()),
                ..capture_args("rhythm:descale", "did it again")
            },
        )
        .await;
        assert!(
            !later["postcondition"]
                .as_str()
                .expect("a postcondition string")
                .contains("derived"),
            "a check-in on a loop that already had a basis opened nothing: {later}"
        );

        // The second negative: an ordinary capture is not this case at all.
        let plain = capture_ok(
            &jojobot,
            capture_args("rhythm:descale", "just a note about it"),
        )
        .await;
        assert!(
            !plain["postcondition"]
                .as_str()
                .expect("a postcondition string")
                .contains("derived"),
            "a capture that asked for no check-in opened nothing: {plain}"
        );
    }

    /// ⚠️ **A snooze cannot open a loop, and the refusal says what can.**
    ///
    /// A snooze is the outcome that leaves a schedule where it was, and a loop
    /// with no basis has no schedule to leave anywhere. The generic
    /// missing-key sentence is the WRONG advice here: it tells the caller to
    /// capture `counts_from`, which is the one move this whole path exists to
    /// remove. So the refusal names the outcomes that do open a loop instead.
    #[tokio::test]
    async fn a_snooze_cannot_open_a_loop_and_the_refusal_names_what_can() {
        let jojobot = handler();
        a_cadenced_rhythm_with_no_basis(&jojobot, "descale", "check_in_date").await;

        let refused = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("snoozed".into()),
                    recorded_at: Some("2026-06-14".into()),
                    ..capture_args("rhythm:descale", "not today")
                }))
                .await
                .expect("capture returns"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        let how = refused["how_to_proceed"].as_str().expect("a way forward");
        assert!(
            how.contains("ran") && how.contains("skipped"),
            "the refusal names the outcomes that DO open a loop: {how}"
        );
        assert!(
            !how.contains("Capture the missing key"),
            "and it does not send the caller to type the basis by hand: {how}"
        );

        // The positive this rests on: the same call with a consuming outcome
        // is not refused, so the refusal is about the outcome and not the loop.
        let opened = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("skipped".into()),
                    recorded_at: Some("2026-06-14".into()),
                    ..capture_args("rhythm:descale", "did not do it, cycle moves on")
                }))
                .await
                .expect("capture returns"),
        );
        assert_ne!(opened["status"], "blocked", "a skip opens it: {opened}");
    }

    /// **The refusal that is still right stays exactly as it was.** No
    /// check-in can say how long a cycle lasts, so a loop short of its cadence
    /// is refused and told which key to capture — which is the case the
    /// opening derive must not reach.
    #[tokio::test]
    async fn a_loop_short_of_its_cadence_still_names_the_key_to_capture() {
        let jojobot = handler();
        ensure(&jojobot, "thing:kettle").await;
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                parent: Some("thing:kettle".into()),
                ..add_args("rhythm", "half-made", "half-made")
            }))
            .await
            .expect("add ok");

        let refused = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    check_in: Some("ran".into()),
                    recorded_at: Some("2026-06-14".into()),
                    ..capture_args("rhythm:half-made", "did it back then")
                }))
                .await
                .expect("capture returns"),
        );
        assert_eq!(refused["status"], "blocked", "{refused}");
        let how = refused["how_to_proceed"].as_str().expect("a way forward");
        assert!(
            how.contains("cadence_days") && how.contains("Capture the missing key"),
            "a loop with no cadence is told which key to capture: {how}"
        );
        assert!(
            !how.contains("counts_from"),
            "counts_from is not this loop's problem, and a working check-in supplies it — \
             naming it here sends the caller to type the one value this whole path exists to \
             derive: {how}"
        );
    }

    /// **Fields ride on a fact, and no label is asked for.**
    ///
    /// A record's fields ARE the thing it describes, so the write that puts a
    /// key on a record cannot be gated on the writer also naming a class for
    /// it. Gated, "the fields of a thing" means "the fields somebody opted
    /// in", which is a biased sample — and every read that groups a thing's
    /// records computes over that sample.
    /// **The receipt reads the day the RUN is asking about, not the day the
    /// claim is about.**
    ///
    /// Which day it is belongs to the caller (rule 222). A claim carries the
    /// day it is true OF, and that is a different question: a reading taken in
    /// January and recorded now is about January, and whether it is still good
    /// is asked as of today.
    ///
    /// **Both halves in one run, because the disagreement is the defect.** A
    /// receipt that answers as of the claim's own date says a reading is fine,
    /// and the very next read of the same record in the same session says it is
    /// stale. Nothing else in the answer changes, so a caller has no way to see
    /// which of the two it should believe.
    #[tokio::test]
    async fn a_backdated_claim_is_receipted_as_of_today() {
        let jojobot = handler();
        ensure(&jojobot, "person:alpha").await;
        let receipt = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    recorded_at: Some("2026-01-10".into()),
                    stale_after: Some("2026-01-20".into()),
                    ..capture_args("person:alpha", "the rate the bank quoted")
                }))
                .await
                .expect("capture ok"),
        );
        assert_eq!(
            receipt["stale_after"], "2026-01-20",
            "the day the writer set is on the receipt: {receipt}",
        );
        assert_eq!(
            receipt["stale"],
            serde_json::json!(true),
            "the receipt read the claim's own day, so a reading past its day came back fine: \
             {receipt}",
        );

        // The same record, read back in the same run: a caller meeting two
        // answers has no way to tell which one this session believes.
        let read = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            read["objects"][0]["facts"][0]["stale"], receipt["stale"],
            "the receipt and the read disagree about the same record in one run: {read}",
        );
    }

    /// **A capture is answered with a receipt, not with the record.**
    ///
    /// The content, the details and the fields are what the caller sent in
    /// this call. **What the read-back proved is untouched**: the store is
    /// still read before the write is called a success, so a claim that did
    /// not survive storage is still an error rather than a success with
    /// mangled bytes. The proof does not require shipping the proof.
    ///
    /// **What must survive is everything the caller could not know**, and the
    /// defaulted values are the sharp part: a caller that omitted `provenance`,
    /// `standing` or `date` learns here what was recorded, and a defaulted
    /// value that never reaches the receipt vanishes silently. The address is
    /// the other half — without it the record cannot be edited, and a receipt
    /// that costs a caller the address has broken the verb.
    #[tokio::test]
    async fn a_capture_is_receipted_without_reading_the_record_back() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        let content = "said the kiln was finally lit, after three weeks of not being lit";

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    details: Some("and that the flue was the problem".into()),
                    fields: Some(
                        [("mood".to_string(), "delighted".to_string())]
                            .into_iter()
                            .collect(),
                    ),
                    ..capture_args("person:alpha", content)
                }))
                .await
                .expect("capture ok"),
        );

        assert_eq!(
            body["content"],
            serde_json::Value::Null,
            "the claim comes back to nobody who just wrote it: {body}"
        );
        assert_eq!(body["content_elided"], true, "{body}");
        assert_eq!(body["content_bytes"], content.len(), "{body}");
        assert_eq!(
            body["details"],
            serde_json::Value::Null,
            "…and the nuance beside it: {body}"
        );
        assert_eq!(
            body["fields_count"], 1,
            "how many keys landed, not which: {body}"
        );

        // The half a receipt may never cost: the address, and everything
        // jojobot decided for a caller that named none of it.
        assert_eq!(body["address"], "person:alpha#f1", "{body}");
        assert_eq!(body["subject"], "person:alpha", "{body}");
        assert_eq!(
            body["provenance"], "inference",
            "a caller that named no provenance learns what was recorded: {body}"
        );
        assert!(
            body["standing"].is_string() && body["status"].is_string(),
            "…and the standing this claim was given: {body}"
        );
        assert!(
            body["recorded_at"].is_string(),
            "…and the date it was stamped: {body}"
        );
        assert!(
            body["how_to_read"]
                .as_str()
                .is_some_and(|how| how.contains("recall")),
            "eliding is never silent — the answer names the call that returns it: {body}"
        );
    }

    #[tokio::test]
    async fn a_capture_carries_fields_with_no_label_and_reads_its_keys_back() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;
        ensure(&jojobot, "milhouse").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    fields: Some(
                        [
                            ("mood".to_string(), "delighted".to_string()),
                            ("weather".to_string(), "clear".to_string()),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                    refs: Some(vec!["person:milhouse".into()]),
                    ..capture_args("person:alpha", "the kiln was finally lit")
                }))
                .await
                .expect("capture ok"),
        );
        assert_ne!(body["status"], "blocked", "no label is asked for: {body}");
        assert_eq!(body["fields_count"], 2, "both keys landed: {body}");
        assert_eq!(body["refs"], serde_json::json!(["person:milhouse"]));

        // …and they are on the record a later reader takes, not only in the
        // answer to the write.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["fields"]["mood"], "delighted",
            "{recalled}"
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["refs"],
            serde_json::json!(["person:milhouse"]),
            "{recalled}"
        );
    }

    /// **A record with no fields carries an empty bag, not a missing key.**
    ///
    /// Carrying none is the ordinary case, so a reader must learn it from the
    /// answer rather than by branching on whether the key is there at all.
    #[tokio::test]
    async fn a_capture_with_no_fields_answers_with_an_empty_bag() {
        let jojobot = handler();
        let body = capture_ok(&jojobot, capture_args("person:alpha", "plays go")).await;
        assert_eq!(
            body["fields_count"], 0,
            "a reader learns there were none from the count, not by branching on a missing \
             key: {body}"
        );
        assert_eq!(body["refs"], serde_json::json!([]), "{body}");
    }

    /// **A ref names an entity, so it must already exist.** The rule is not
    /// about edges, it is about naming: nothing a write mentions is brought
    /// into being as a side effect of mentioning it. A ref that provisioned its
    /// own entity would make the open bag the one place on this surface where
    /// that stopped being true — and the bag takes any key precisely so that
    /// everything else about it stays strict.
    #[tokio::test]
    async fn a_ref_to_an_entity_nobody_created_is_blocked_and_writes_nothing() {
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        let body = json_of(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    refs: Some(vec!["person:ghost".into()]),
                    ..capture_args("person:alpha", "it happened")
                }))
                .await
                .expect("an answer"),
        );
        assert_eq!(body["status"], "blocked", "{body}");
        assert_eq!(body["attempted"], "person:ghost");
        assert_eq!(body["wrote"], false);

        // …and the fact did not land either: a record is one write, so a ref it
        // could not resolve takes the whole thing with it.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["objects"][0]["facts"]
                .as_array()
                .expect("a list")
                .is_empty(),
            "a blocked record wrote nothing: {recalled}"
        );
    }

    #[tokio::test]
    async fn a_fact_can_be_about_any_kind() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            capture_args("place:north-trail", "swimmable in August"),
        )
        .await;
        assert_eq!(captured["subject"], "place:north-trail");
    }

    /// Capture's subject must exist, near miss or complete stranger, and the
    /// way through is `add_entity` — never an override. The advice must say
    /// `add_entity`: `override_token` is not a parameter on this verb, and
    /// telling the caller to send one would offer a way out that does not
    /// exist.
    #[tokio::test]
    async fn a_blocked_capture_says_to_add_the_entity_first() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "zenith", "Zenith")))
            .await
            .expect("add ok");

        let near = jojobot
            .capture(Parameters(capture_args("zenit", "should not land")))
            .await
            .expect("call ok");
        let body = blocked(&near);
        assert_eq!(body["candidates"][0]["handle"], "person:zenith");
        // The near-miss branch has its own copy, and it has to earn its keep: the
        // candidate list is the whole reason this case differs from a stranger,
        // so the advice must point at it rather than repeat the stranger's text.
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("above"),
            "with candidates in hand, the advice must point at them: {advice}"
        );
        assert!(
            advice.contains("add_entity"),
            "…and still name the way through: {advice}"
        );
        assert!(
            !advice.contains("nothing resembles it"),
            "something does resemble it — that is what the candidates are: {advice}"
        );
        assert!(
            !advice.contains("override_token"),
            "capture has no override_token, near miss or not: {advice}"
        );

        // A handle nothing resembles blocks too, with nothing to suggest.
        let stranger = jojobot
            .capture(Parameters(capture_args("work:first-mix", "32 tracks")))
            .await
            .expect("call ok");
        let body = blocked(&stranger);
        assert_eq!(body["attempted"], "work:first-mix");
        assert!(
            body["candidates"].as_array().unwrap().is_empty(),
            "got {body}"
        );
        let advice = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            advice.contains("add_entity"),
            "must name the way through: {advice}"
        );
        assert!(
            !advice.contains("override_token"),
            "capture has no override_token; advising one offers a way out that does \
             not exist: {advice}"
        );
        assert!(
            !advice.contains("above"),
            "there are no candidates above to point at: {advice}"
        );

        // Two deliberate steps, and it lands.
        jojobot
            .add_entity(Parameters(add_args("work", "first-mix", "First Mix")))
            .await
            .expect("add ok");
        let landed = capture_ok(&jojobot, capture_args("work:first-mix", "32 tracks")).await;
        assert_eq!(landed["subject"], "work:first-mix");
    }

    /// `capture` draws a typed edge, and the edge comes back on every read of the
    /// fact — rendered with schema.org's word for the shape (`memberOf`), while
    /// the input token stays the lowercase `membership`.
    #[tokio::test]
    async fn capture_draws_an_edge_and_renders_its_schema_org_name() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("membership".into()),
                object: Some("org:north-trail-club".into()),
                ..capture_args("alpha", "rides with the club")
            },
        )
        .await;
        assert_eq!(captured["edge"]["type"], "memberOf");
        assert_eq!(captured["edge"]["object"], "org:north-trail-club");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["edge"]["type"],
            "memberOf"
        );
    }

    /// The shape set is closed, and the response spellings are not input tokens —
    /// the input grammar stays lowercase.
    #[tokio::test]
    async fn an_unknown_shape_is_a_client_error() {
        let jojobot = handler();
        for shape in ["knows", "memberOf", "Location", "attendee"] {
            let err = jojobot
                .capture(Parameters(CaptureArgs {
                    shape: Some(shape.into()),
                    object: Some("place:north-trail".into()),
                    ..capture_args("alpha", "an unknown shape")
                }))
                .await
                .expect_err("must reject shape {shape}");
            assert_eq!(err.code, ErrorCode::INVALID_PARAMS, "for {shape}");
            assert!(
                err.message.contains("location"),
                "the error must name the closed set: {}",
                err.message
            );
        }
    }

    /// A shape's object must be the kind it requires — a `location` pointing at a
    /// person is a mis-drawn edge, and the caller hears about it.
    #[tokio::test]
    async fn a_wrong_kind_edge_object_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("person:beta".into()),
                ..capture_args("alpha", "in the wrong kind of place")
            }))
            .await
            .expect_err("a wrong-kind object must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("place"),
            "must say what it wanted: {}",
            err.message
        );
    }

    /// A typo'd edge object comes back as the guard's candidates — the same
    /// error-flagged response a blocked subject gets, and nothing is written.
    #[tokio::test]
    async fn a_blocked_edge_object_returns_candidates() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("place", "riverbend", "Riverbend")))
            .await
            .expect("add ok");
        // The subject faces the gate too, and the guard reports the first handle
        // it stops — this spec is about the object.
        ensure(&jojobot, "alpha").await;

        let result = jojobot
            .capture(Parameters(CaptureArgs {
                shape: Some("location".into()),
                object: Some("place:riverbnd".into()),
                ..capture_args("alpha", "should not land")
            }))
            .await
            .expect("the call succeeds; the guard answers in the body");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "place:riverbnd");
        assert_eq!(body["candidates"][0]["handle"], "place:riverbend");
        assert_eq!(body["candidates"][0]["type"], "Place");

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert!(
            recalled["objects"][0]["facts"]
                .as_array()
                .unwrap()
                .is_empty(),
            "a blocked edge object must write no fact: {recalled}"
        );
    }

    /// The end-to-end MCP path: capture through the handler, then recall through
    /// the handler, and the fact comes back.
    #[tokio::test]
    async fn capture_then_recall_through_the_handler() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "drinks oat milk")).await;
        assert_eq!(captured["subject"], "person:alpha");

        let body = json_of(
            &jojobot
                .recall(Parameters(recall_args("alpha")))
                .await
                .expect("recall ok"),
        );
        assert_eq!(body["objects"][0]["id"], "person:alpha");
        let facts = body["objects"][0]["facts"]
            .as_array()
            .expect("recall returns a list");
        assert!(
            facts.iter().any(|f| {
                f["address"] == captured["address"] && f["content"] == "drinks oat milk"
            }),
            "recall must return the captured fact: {body}"
        );
    }

    /// Omitting `provenance` defaults to inference (a hypothesis until confirmed).
    #[tokio::test]
    async fn provenance_defaults_to_inference() {
        let jojobot = handler();
        let captured = capture_ok(&jojobot, capture_args("alpha", "maybe a morning person")).await;
        assert_eq!(captured["provenance"], "inference");
    }

    /// **A `derived_from` naming no claim is blocked, with the addresses that
    /// do exist.**
    ///
    /// The store refuses it; this is the half that says a caller sees a
    /// refusal it can act on rather than an error. The pair is here too,
    /// because a blocked answer proves nothing on a build where the accepted
    /// case never writes the link either.
    #[tokio::test]
    async fn a_derived_from_must_name_a_claim_that_exists() {
        let jojobot = handler();
        let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
        let address = source["address"].as_str().expect("an address").to_string();

        let linked = capture_ok(
            &jojobot,
            CaptureArgs {
                derived_from: Some(address.clone()),
                ..capture_args("alpha", "so the crossing is longer")
            },
        )
        .await;
        assert_eq!(
            linked["derived_from"], address,
            "a link to a claim that exists is written: {linked}"
        );

        let refused = blocked(
            &jojobot
                .capture(Parameters(CaptureArgs {
                    derived_from: Some("person:alpha#f99".into()),
                    ..capture_args("alpha", "and the fare went up")
                }))
                .await
                .expect("a miss is an answer, not a protocol failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        assert!(
            refused["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains(&address)),
            "the addresses that DO exist are what makes it repairable: {refused}"
        );

        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall answers"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"]
                .as_array()
                .expect("facts")
                .len(),
            2,
            "a refused capture wrote nothing: {recalled}"
        );
    }

    /// **Citing an archived claim as `derived_from` is permitted, and the
    /// caller is told rather than left to notice on a later read.**
    ///
    /// Archive is a visibility switch, not a validity gate: refusing the
    /// citation would make archiving decide what may be worked from, which
    /// is the thing this slice removed. The write must still go through, and
    /// the postcondition — the one place a caller reads what a write did —
    /// has to name the archived source, or the caller cannot tell without a
    /// second call.
    #[tokio::test]
    async fn citing_an_archived_source_is_allowed_and_named_on_the_receipt() {
        let jojobot = handler();
        let source = capture_ok(&jojobot, capture_args("alpha", "said the ferry moved")).await;
        let address = source["address"].as_str().expect("an address").to_string();

        jojobot
            .retract(Parameters(crate::memory::retract::RetractArgs {
                address: address.clone(),
                reason: Some("misread the notice".into()),
                recorded_at: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("retract answers");

        let linked = capture_ok(
            &jojobot,
            CaptureArgs {
                derived_from: Some(address.clone()),
                ..capture_args("alpha", "so the crossing is longer")
            },
        )
        .await;
        assert_eq!(
            linked["derived_from"], address,
            "a claim citing an archived source must still be written: {linked}"
        );
        assert!(
            linked["postcondition"]
                .as_str()
                .is_some_and(|note| note.contains(&address) && note.contains("archived")),
            "the receipt did not name the archived source the claim rests on: {linked}"
        );
    }

    /// **The `standing` argument reaches the store, and comes back.**
    ///
    /// Nothing tested this. The domain contract builds `NewFact { standing }`
    /// directly and never touches `parse_standing`; the argument builders only
    /// ever sent `None`; and the story that exercises the field asserts with a
    /// substring over a response holding two facts, so it cannot see the
    /// argument dropped. Setting this verb's `standing` to `None` — the MCP
    /// layer silently discarding what the caller asked for — left the entire
    /// suite green.
    ///
    /// The hedge is the case that matters: `testimony` with `open` is the one
    /// pairing a default cannot produce, so it is the only one that proves the
    /// argument travelled rather than being re-derived at the far end.
    #[tokio::test]
    async fn the_standing_argument_travels_and_reads_back() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                standing: Some("open".into()),
                ..capture_args("alpha", "thinks it shuts early")
            },
        )
        .await;
        // Paired: both halves, because `open` alone is what a default would
        // give an inference and `testimony` alone is what the provenance
        // argument already proves.
        assert_eq!(captured["provenance"], "testimony");
        assert_eq!(captured["standing"], "open");

        // …and it is on the page, not just in the answer.
        let recalled = json_of(
            &jojobot
                .recall(Parameters(recall_args("person:alpha")))
                .await
                .expect("recall answers"),
        );
        assert_eq!(
            recalled["objects"][0]["facts"][0]["standing"], "open",
            "{recalled}"
        );
    }

    /// An unknown `standing` is a client error, not a silent default — a
    /// caller who wrote something else meant something, and guessing which of
    /// two values they meant is how a hedge becomes a settled fact.
    #[tokio::test]
    async fn an_unknown_standing_is_a_client_error() {
        let jojobot = handler();
        let refused = jojobot
            .capture(Parameters(CaptureArgs {
                standing: Some("maybe".into()),
                ..capture_args("alpha", "something")
            }))
            .await;
        assert!(refused.is_err(), "an unknown standing must be refused");
    }

    /// **Two runs in two zones disagree about what today is, and both are
    /// right.**
    ///
    /// The frame belongs to the caller, so a claim captured with no date is
    /// stamped with the day it is in the run's own zone. The two zones here are
    /// the extremes on purpose: twenty-six hours apart, so their local dates
    /// differ at every instant and this case does not pass or fail by the hour
    /// it is run at.
    ///
    /// **Each date is pinned to its own zone, not merely to being different.**
    /// A case asserting only that two answers differ passes on a build that
    /// stamps them wrong in two directions.
    #[tokio::test]
    async fn two_runs_in_two_zones_stamp_a_claim_with_their_own_day() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;

        // Twenty-six hours apart: the widest the map goes, so the two local
        // dates can never coincide.
        // Both answer `new`: the fixture handle already has a run in flight,
        // and a bot may have several at once — which is what lets one case hold
        // two of them in two zones.
        let behind = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
        let ahead = booted_in(&jojobot, "otto", "Pacific/Kiritimati", Some("new")).await;

        let stamped = async |sid: &str| {
            let mut args = capture_args("milhouse", "no date on this one");
            args.sid = Some(sid.to_string());
            args.recorded_at = None;
            capture_ok(&jojobot, args).await["recorded_at"]
                .as_str()
                .expect("a capture is stamped with a day")
                .to_string()
        };
        let (behind, ahead) = (stamped(&behind).await, stamped(&ahead).await);

        let day_in = |zone: &str| {
            jiff::Timestamp::now()
                .to_zoned(jiff::tz::TimeZone::get(zone).expect("a zone"))
                .date()
                .to_string()
        };
        assert_eq!(behind, day_in("Etc/GMT+12"), "the run west of everything");
        assert_eq!(
            ahead,
            day_in("Pacific/Kiritimati"),
            "and the run east of it"
        );
        assert_ne!(
            behind, ahead,
            "…which are never the same day, whatever hour this runs at",
        );
    }

    /// 🚨 **A run that stated its day writes under that day, not under the
    /// server's.**
    ///
    /// The door takes the day a run is in, and the sweep and the beats already
    /// read it. A CLAIM did not: a session acting out March made every write
    /// under the day the run actually happened, and the prose it wrote was
    /// perfectly in period, so nothing in the store said the date was wrong.
    ///
    /// ⚠️ **Three halves, and each alone passes on a build nobody wants.** A
    /// run that stated no day must still get today, or the frame becomes a
    /// requirement rather than an option. And a write naming its own date must
    /// still win, or a run acting out a period can no longer record a claim
    /// about any other day — which is most of what such a run is for.
    #[tokio::test]
    async fn a_run_that_stated_its_day_writes_under_it() {
        let jojobot = handler();
        make_bot(&jojobot, "otto").await;
        ensure(&jojobot, "person:milhouse").await;
        // **Answering `new`**, because the fixture handle already has a run of
        // this bot in flight and a boot meeting one hands back a choice rather
        // than a handle.
        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("otto".into()),
                    today: Some("2026-03-15".into()),
                    resume: Some("new".into()),
                    brief: Some(true),
                    timezone: None,
                    skill: None,
                    sid: None,
                }))
                .await
                .expect("the boot call is ok"),
        );
        let acting =
            sid_of(&booted).unwrap_or_else(|| panic!("a boot that states a day: {booted}"));

        let mut args = capture_args("milhouse", "went to the fair");
        args.sid = Some(acting.clone());
        args.recorded_at = None;
        let stated = capture_ok(&jojobot, args).await;
        assert_eq!(
            stated["recorded_at"], "2026-03-15",
            "the claim was stamped with the server's day, not the run's: {stated}"
        );

        // **A write naming its own date still wins.** A run acting out a period
        // records claims about other days, and this is how.
        let mut args = capture_args("milhouse", "had been at the fair the day before");
        args.sid = Some(acting.clone());
        args.recorded_at = Some("2026-03-14".into());
        let named = capture_ok(&jojobot, args).await;
        assert_eq!(named["recorded_at"], "2026-03-14");

        // **A second run states another day, and the first one's frame does not
        // reach it.** Two runs of one bot are legitimately in two periods, and
        // the handle is what tells them apart.
        let elsewhere = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: Some("otto".into()),
                    today: Some("2026-07-04".into()),
                    resume: Some("new".into()),
                    brief: Some(true),
                    timezone: None,
                    skill: None,
                    sid: None,
                }))
                .await
                .expect("the boot call is ok"),
        );
        let mut args = capture_args("milhouse", "was at the parade");
        args.sid = sid_of(&elsewhere);
        args.recorded_at = None;
        let second = capture_ok(&jojobot, args).await;
        assert_eq!(
            second["recorded_at"], "2026-07-04",
            "the second run wrote under the first one's day: {second}"
        );

        // ⚠️ **A run that stated no day still gets today**, on the clock in its
        // own zone. Without this the frame stops being optional.
        let now = booted_in(&jojobot, "otto", "Etc/GMT+12", Some("new")).await;
        let mut args = capture_args("milhouse", "happening now");
        args.sid = Some(now);
        args.recorded_at = None;
        let clocked = capture_ok(&jojobot, args).await;
        assert_eq!(
            clocked["recorded_at"],
            jiff::Timestamp::now()
                .to_zoned(jiff::tz::TimeZone::get("Etc/GMT+12").expect("a zone"))
                .date()
                .to_string(),
            "a run that stated no day was answered with somebody else's frame: {clocked}"
        );
    }

    /// **A run that named no zone is answered in UTC**, which is the stated
    /// fallback rather than a server setting.
    ///
    /// The negative the case above rests on: without it, a build that always
    /// used the fallback and a build that reads the run's zone are told apart
    /// by nothing here.
    #[tokio::test]
    async fn a_run_with_no_zone_is_dated_in_the_fallback() {
        let jojobot = handler();
        let today = jiff::Timestamp::now()
            .to_zoned(jiff::tz::TimeZone::UTC)
            .date();
        let captured = capture_ok(&jojobot, capture_args("alpha", "dated today")).await;
        assert_eq!(captured["recorded_at"], today.to_string());
    }

    /// An explicit testimony provenance is honoured.
    #[tokio::test]
    async fn explicit_testimony_is_honoured() {
        let jojobot = handler();
        let captured = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                recorded_at: Some("2026-01-01".into()),
                ..capture_args("alpha", "speaks two languages")
            },
        )
        .await;
        assert_eq!(captured["provenance"], "testimony");
        assert_eq!(captured["recorded_at"], "2026-01-01");
    }

    #[tokio::test]
    async fn unknown_provenance_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                provenance: Some("maybe".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject unknown provenance");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn malformed_date_is_a_client_error() {
        let err = handler()
            .capture(Parameters(CaptureArgs {
                recorded_at: Some("not-a-date".into()),
                ..capture_args("alpha", "x")
            }))
            .await
            .expect_err("must reject a malformed date");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
    }

    /// Empty content is a caller mistake, so it comes back as a blocked
    /// answer with a way forward rather than as a protocol error (rule 68).
    #[tokio::test]
    async fn empty_content_is_a_blocked_answer() {
        let body = blocked(
            &handler()
                .capture(Parameters(capture_args("alpha", "   ")))
                .await
                .expect("a caller mistake is an answer, not a protocol failure"),
        );
        assert_eq!(body["wrote"], false, "{body}");
        let said = jojobot_domain::memory::validate_content("   ")
            .expect_err("empty content is refused")
            .to_string();
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| advice.contains(&said)),
            "the refusal names the fault: {body}"
        );
    }
}
