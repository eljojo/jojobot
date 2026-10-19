//! `start_here` — The one orienting door, with or without an identity.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// **Why a fetch and a compaction report are two calls.** A fetch answers before
/// the boot and starts nothing, so a report sent beside it would be dropped
/// without a word, and the run would go on believing the server had forgotten
/// what it was taught and shown.
fn compaction_beside_a_fetch(what: &str) -> String {
    format!(
        "reporting a compaction and reading {what} are two calls: a fetch touches no run, so \
         honouring `compacted` here would drop it and forget nothing. Call start_here with \
         `compacted` and your `sid` to report the compaction, then again with the fetch alone — \
         or drop `compacted` to read now."
    )
}

/// Arguments to `start_here` — **the one door**, with or without an identity.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OrientArgs {
    /// Optional. The bot to boot as: its bare slug, or its full `bot:`-prefixed
    /// handle. A handle of any other kind is refused — this door boots bots.
    /// Omit it for an anonymous orientation: you get the world and the
    /// snapshot, and no sid.
    ///
    /// Booting sweeps that bot's sessions that have gone a day without a beat to
    /// `abandoned`; an abandoned run is offered back at a later boot for {{offer}}. It
    /// does REPAIR one thing: a bot whose box is missing gets it opened here,
    /// because a box is part of what a bot is and its absence is damage rather than
    /// a setup step, and the answer says so plainly. A boot that starts a new run
    /// ends any wrap window the bot has open.
    #[serde(default)]
    pub(crate) bot: Option<String>,
    /// Skip the orientation essay and return only what changes between calls —
    /// the snapshot, your identity, your session.
    ///
    /// The essay is the only part of the answer that does not change between calls.
    /// An anonymous boot whose essay did not fit whole names every section it left
    /// out; `section` fetches one.
    #[serde(default)]
    pub(crate) brief: Option<bool>,
    /// Your answer to the resume-or-new choice a boot hands back when this bot
    /// has a session worth picking up: the name of the run you are resuming (its
    /// `run`), exactly as the offer spelled it, or `new` for a fresh session. A
    /// name is not a key: never send a `sid` here. Leave it off on a first boot —
    /// there is nothing to answer yet.
    ///
    /// **Or the `wrap_code` a wrap handed back**, with `bot`: it hands the
    /// wrapped run a new sid so that run can take one last change. The run
    /// stays wrapped. No role is taken, nothing is swept and no lease renews.
    ///
    /// Answering the resume-or-new offer? Send the offer's `offer_code` beside
    /// it. With the code, the answer carries what is new (the run, its key, the
    /// role claim, the zone, and any rule written since the offer) and leaves out
    /// what the offer shipped: the essay, the snapshot, the charter, the carried
    /// rules and the map of those at home, and the handover story. `left_out`
    /// names each and the call that returns them, a boot with no `resume`.
    /// Without the code, or with a code that is another bot's or already used,
    /// the answer is the full boot less the charter. A boot given a
    /// `wrap_code` as `resume` opens a wrap window: the wrapped run gets a new sid,
    /// the run stays wrapped for one last change, and no role is taken,
    /// nothing is swept and no lease renews.
    #[serde(default)]
    pub(crate) resume: Option<String>,
    /// **A skill to read, by the name the index gave it.** A boot lists the
    /// skills meant for it by name and when-to-use and ships no bodies; this is
    /// how you get one, once the index has told you it is relevant. Any skill can
    /// be fetched by name, whether or not your boot lists it.
    ///
    /// It is the same door because this door is skill zero: it is where a
    /// session learns the model, so it is where a session learns the
    /// procedures. A name that is no skill comes back blocked, naming the ones
    /// that are. A fetch that sends your `sid` records that this session read
    /// the skill, so the act it covers is not offered to you again; it changes
    /// no run.
    #[serde(default)]
    pub(crate) skill: Option<String>,
    /// **One unit of the essay's remainder, left out of an anonymous boot's
    /// answer by the ceiling** — by its exact `##` heading, or the literal
    /// word `"opening"` for the remainder's un-headed opening paragraphs.
    /// The answer carries that one unit, whole, and nothing else from the
    /// essay — never the core, which always ships and needs no fetch.
    ///
    /// It takes no bot and starts no session, and it writes nothing.
    /// A name that matches no section comes back blocked, naming every
    /// valid one, `"opening"` included.
    #[serde(default)]
    pub(crate) section: Option<String>,
    /// The session id you are already carrying, if you have one — the same
    /// `sid` that rides every other call you make. Leave it off when you have
    /// none: this door is where one comes from, so a first boot has nothing to
    /// pass. **A handle is never turned away here**, whatever became of it: the
    /// answer says whether the one you carried still addresses your session, so
    /// a session that came back to a server it does not recognise learns that
    /// in the same call it re-orients with.
    #[serde(default)]
    pub(crate) sid: Option<String>,
    /// **Send `true` with your live `sid` when your context was compacted.** If
    /// you no longer hold the `sid`, send `bot` alone: it reports the bot's one
    /// live run, and is refused, naming the runs, when the bot has none or more
    /// than one. A compaction takes away what jojobot taught you and showed you,
    /// and jojobot cannot tell. This call says so: jojobot forgets what it believed this run
    /// was taught and shown, restarts the running total of what the run has been
    /// handed, and writes a dated mark in the run's chronology. The compaction is
    /// also counted on that week's usage record (recall with view: usage).
    ///
    /// **The run stays the run.** Same `sid`, same chronology, same identity: no
    /// new key, no voucher moves, and no other run of the bot is touched. The
    /// answer is the full orientation, charter included, whatever `brief` says,
    /// because that is what the compaction took. A `resume` or an `offer_code`
    /// beside it changes neither the answer nor the run. A `skill` or a `section`
    /// beside it is refused: a fetch touches no run. A `sid` that opens no
    /// run is refused. Naming `bot` is optional; when you do, it has to be the
    /// bot the `sid` belongs to.
    #[serde(default)]
    pub(crate) compacted: Option<bool>,
    /// **A name of your choosing for a compaction report**, up to 128 bytes, sent
    /// with `compacted: true` and your `sid`. The report is made of parts, the
    /// mark in the chronology and the restart of the running total: if an answer
    /// is lost or a part fails, send the same call again with the same
    /// `request_id` and the mark is not written twice. A boot takes none.
    // Read by the dispatch from the raw arguments, before the verb runs; the field
    // is here so the verb publishes the argument.
    #[allow(dead_code)]
    #[serde(default)]
    pub(crate) request_id: Option<String>,
    /// **The day this run is in**, as `YYYY-MM-DD`, when it is not the day the
    /// server is having.
    ///
    /// **The sweep decides whether your other runs went quiet, and it decides in
    /// this frame.** Send it when a run is not happening now: a session catching
    /// up on last week, an instance restored from a backup, a run acting out a
    /// stretch of time. Without it the sweep answers on the server's clock, so a
    /// run that covers months in minutes leaves every one of its own sittings
    /// looking like it is still working, and each boot after the first meets a
    /// resume-or-new choice for runs that are long over.
    ///
    /// **jojobot never derives it.** The zone beside it says how to name a day;
    /// this says which day you are in, and no zone can tell the server that.
    ///
    /// Send none and the sweep answers on the clock, which is what it always
    /// did.
    #[serde(default)]
    pub(crate) today: Option<String>,
    /// **Omit this on a boot.** The instance holds the operator's zone and
    /// answers every run that sends none in it; the boot says which zone it
    /// used and where it came from. Send an IANA name only when this run really
    /// works in another zone, and send it again when you resume from somewhere
    /// else. A zone you invent moves the day for everything this run writes.
    ///
    /// It is what *today* means for everything day-grained: the date a `capture`
    /// gets when you name none, and whether a rhythm has fallen due.
    /// A zone you send wins over the instance's, and the boot tells you when the
    /// one you sent differs from it.
    ///
    /// ⚠️ **Two sessions in different zones will disagree about what today is
    /// for the same stored claim, and that is correct.** A claim captured at
    /// nine in the evening is already the next day in a zone further east; both
    /// runs are reading the same claim and answering in their own frame. It is
    /// not a fault and there is nothing to work around.
    ///
    /// Send none and the instance's zone answers, or UTC when its operator set
    /// none. On a resume, sending none keeps the zone the run already had
    /// rather than moving it.
    #[serde(default)]
    pub(crate) timezone: Option<String>,
    /// **A role to claim, by a name you choose.** Boot names this when you
    /// want jojobot to refuse a second claimant while your claim is fresh —
    /// **`taken` means your claim succeeded and YOU hold the role; it never
    /// means somebody else has it, which is `refused`.** The answer is one of
    /// `taken`, `refused` naming who holds it and until
    /// when — or, when the role belongs to another bot, naming that `owner` and
    /// writing nothing, so boot as the owner to claim it — `conflict` when the claim collided with another write
    /// landing the same instant, or `unavailable` when the store could not
    /// decide it. **A conflict is the store working correctly, not a
    /// mistake in what you sent** — retry the same call. **An unavailable
    /// claim holds nothing**, and its answer says what to do next.
    /// **A boot that names no claim claims the role the bot carries in
    /// `claims_role`, if it carries one**, and is otherwise unchanged: two
    /// sessions working two separate slices never meet a lease neither of
    /// them claimed. `claims_role` is written about a bot by
    /// a different identity, never by the bot itself.
    ///
    /// **Every write you make while holding it renews the lease**, not
    /// only a journal beat — capture, add_entity, post_message and the
    /// rest all count — and the lease is {{lease}}. A write less than {{renewal}}
    /// after the last renewal renews nothing, so a lapse can come up to that
    /// much sooner than {{lease}} after your last write. **Wrapping releases
    /// whatever you held**, so a fresh session may claim the same role at
    /// once.
    #[serde(default)]
    pub(crate) claim: Option<String>,
    /// **Ask for a code to boot as the bot `bot` names.** A session the operator
    /// started by hand has no voucher, and this is how it gets one. jojobot
    /// writes a fresh code into the operator's own mailbox, which is where the
    /// operator reads their mail, and answers with a reference to the challenge and
    /// never the code. The call starts no session. One challenge stands per bot:
    /// while a code is waiting a new request is refused and names when it ends, a
    /// code is good for {{code}}, and three wrong codes void it sooner.
    ///
    /// The operator types the code into this session. Boot again with `bot`,
    /// `challenge` and `code` to present it.
    #[serde(default)]
    pub(crate) ask_for_code: Option<bool>,
    /// **The reference to a challenge**, as the answer to `ask_for_code` named
    /// it. Sent with `code` and `bot` on a boot.
    #[serde(default)]
    pub(crate) challenge: Option<String>,
    /// **The code the operator typed in.** A boot that presents the right code
    /// for its bot is given a voucher: the root when the bot is the assistant,
    /// that bot's ordinary voucher otherwise. The code is spent on first use,
    /// and the third wrong code voids the challenge.
    #[serde(default)]
    pub(crate) code: Option<String>,
    /// **The `offer_code` the resume-or-new offer carried**, sent beside `resume`.
    /// With it, and only with it, the answer to that offer carries what is new.
    /// Without it, or with a code for another bot or one already used, the answer
    /// is the full boot.
    #[serde(default)]
    pub(crate) offer_code: Option<String>,
}

/// **The one orienting door**, with or without an identity: the world-model
/// in prose, a live snapshot of what exists, and — when a bot is named —
/// that identity and its session.
///
/// There is deliberately no second verb for the identified case: one
/// function producing one text and one snapshot, kept true by construction.
/// Two surfaces would drift.
///
/// The prose below is ENGINE material: it explains the method, names only
/// roles ("the operator"), and every example identity is fictional.
#[tool_router(router = start_here_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "New here? Call this first — it is the ONE door, whether or not you \
                       have an identity. With no bot it is a read-only orientation: what \
                       jojobot is, a live snapshot (entities by kind, UP TO TWENTY BOTS NAMED \
                       so you can see which identities you could boot as, the rest counted) and the skills index (a \
                       name and what each is FOR, never the procedures: call again with \
                       skill: its name to get one). NAME A BOT and the answer also carries \
                       that identity: its charter, its rules as dated claims each with its \
                       own provenance (testimony and observation are settled, inference is a \
                       hypothesis), and the counts of the mailbox it owns. THIS DOOR CREATES \
                       NO IDENTITY: a name that is no bot comes back blocked, listing the \
                       bots that exist. Booting starts or resumes that bot's session — there \
                       is no separate start verb — and THIS DOOR IS WHERE A SESSION ID COMES \
                       FROM: that `sid` rides every verb after it, reads included, and \
                       passing `sid` here tells you whether it still addresses your session. \
                       A bot with a run to pick up gets a resume-or-new choice and no sid \
                       until it answers with `resume`: the name of the run (its `run`), `new`, \
                       or the wrap_code a wrap handed back. CALLED THIS BEFORE? Pass brief: true to skip the essay; \
                       calling without brief reads it in full. `claim` holds a named role for \
                       {{lease}} so two sessions never work one role at once (taken, \
                       refused, conflict or unavailable; a claim is refused while its role \
                       object is archived, and archive_entity restores it); every write, and \
                       every read of your own box, renews the lease unless the last renewal is less than {{renewal}} old, and wrapping releases \
                       it. A boot writes only the sweep of abandoned runs, the repair of a \
                       missing box, a note of the key it hands out and, with a wrap_code, \
                       the wrap window it opens. Each \
                       argument's own description holds the rest."
    )]
    pub(crate) async fn start_here(
        &self,
        Parameters(args): Parameters<OrientArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bot = named_bot(args.bot.as_deref())?;
        // **The handle a caller brought is answered, never declined** — this
        // door is the way back, so a caller whose handle stopped addressing
        // anything is the one who most needs it to open. See
        // [`Jojobot::standing`].
        let carried = self.standing(args.sid.as_deref()).await;
        let resume = args
            .resume
            .as_deref()
            .map(str::trim)
            .filter(|r| !r.is_empty());
        // **An answer with nobody to answer for.** `resume` responds to an
        // offer only a named boot makes, so carrying one without a bot is a
        // malformed call rather than an absence — there is no session it could
        // be about, and honouring it would mean guessing whose it was.
        if resume.is_some() && bot.is_none() {
            // **The prose is unchanged; the CHANNEL is the fix.** A thrown error
            // is not a value a caller can branch on, so advice naming two ways
            // forward arrived somewhere nothing reads structurally. Blocked is
            // the shape every other misuse here wears.
            return Ok(misused(
                "resume answers the choice a boot hands back, so it needs the bot you are booting \
                 as — pass `bot` too, or drop `resume` for an anonymous orientation"
                    .to_string(),
            ));
        }
        // **The fetch is answered before the boot**, and it does not touch a
        // session: reading a procedure is a read, and it must not sweep, start
        // or resume anything.
        if let Some(wanted) = args
            .skill
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            // **Two asks, refused rather than half-served.** Reading a
            // procedure starts no session and booting is what starts one, so
            // honouring both would hand back a body and no `sid` — a caller
            // that cannot tell it has not booted.
            //
            // `resume` is not tested here and does not need to be: a `resume`
            // with no bot is refused above as a malformed call, so any
            // `resume` reaching this line has a bot beside it.
            if bot.is_some() {
                return Ok(misused(
                    "reading a skill and booting an identity are two calls: a fetch starts no \
                     session, so honouring `bot` here would hand you a body and no handle. Call \
                     start_here with `skill` alone to read the procedure, then again with `bot` \
                     to boot — or drop `skill` to boot now."
                        .to_string(),
                ));
            }
            if args.compacted == Some(true) {
                return Ok(misused(compaction_beside_a_fetch("a skill")));
            }
            return match skills::named(self.skills, wanted) {
                Some(skill) => {
                    // **Reading is recorded against the session that sent its
                    // `sid`**, so the act this skill covers is not offered to a
                    // session that already has it. A fetch with no `sid`, or a
                    // key that opens nothing, records nothing and still answers.
                    let reader = self.caller(args.sid.as_deref()).ok().flatten();
                    self.first_contact(
                        &format!("{}{}", crate::teaching::SKILL_READ_PREFIX, skill.name),
                        reader.as_ref(),
                    )
                    .await;
                    json_result(&serde_json::json!({
                    "skill": {
                        "name": skill.name,
                        "when_to_use": skill.when_to_use,
                        "body": skill.body,
                    },
                    "carried_session": carried,
                    }))
                }
                None => Ok(handle_declined(
                    wanted,
                    format!(
                        "Nothing was read. '{wanted}' is not a skill this build ships. These \
                         are, by name: {}. Ask for one by its exact name; a boot lists the \
                         skills meant for it with what each is for, and any skill can be \
                         fetched by name. They are compiled into the server, so \
                         there is no verb that adds one.",
                        self.skills
                            .iter()
                            .map(|s| s.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )),
            };
        }
        // **The fetch is answered before the boot, exactly as `skill`'s
        // is.** Reading one unit of the essay's remainder is a read; it
        // must not sweep, start or resume anything.
        if let Some(wanted) = args
            .section
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if bot.is_some() {
                return Ok(misused(
                    "reading a section and booting an identity are two calls: a fetch starts no \
                     session, so honouring `bot` here would hand you a body and no handle. Call \
                     start_here with `section` alone to read it, then again with `bot` to boot \
                     — or drop `section` to boot now."
                        .to_string(),
                ));
            }
            if args.compacted == Some(true) {
                return Ok(misused(compaction_beside_a_fetch("a section")));
            }
            let (preamble, sections) = essay::remainder();
            let body = if wanted == "opening" {
                Some(preamble)
            } else {
                sections
                    .iter()
                    .find(|s| s.heading == wanted)
                    .map(|s| s.body)
            };
            return match body {
                Some(body) => json_result(&serde_json::json!({
                    "section": {
                        "name": wanted,
                        "body": body,
                    },
                    "carried_session": carried,
                })),
                None => Ok(handle_declined(
                    wanted,
                    format!(
                        "Nothing was read. '{wanted}' is not a section of this essay. These are, \
                         by name: \"opening\", {}. Ask for one by its exact name.",
                        sections
                            .iter()
                            .map(|s| format!("\"{}\"", s.heading))
                            .collect::<Vec<_>>()
                            .join(", "),
                    ),
                )),
            };
        }
        // **Validated at the door, where a caller can still fix it.** A name
        // that is no zone is a malformed argument rather than a near miss, and
        // it is refused before any session is minted or resumed.
        let timezone = parse_zone(args.timezone.as_deref())?
            .iana_name()
            .map(str::to_string)
            .filter(|_| {
                args.timezone
                    .as_deref()
                    .map(str::trim)
                    .is_some_and(|z| !z.is_empty())
            });
        // **Validated here too, and for the same reason.** A day that is no day
        // is a malformed argument a caller can still fix, and taking it as
        // "stated nothing" would answer the sweep on the clock while the caller
        // believes it stated a frame.
        let today = parse_day(args.today.as_deref())?;
        let claim = args
            .claim
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty());
        let instance_zone = self.instance_zone().await;
        // **The operator's code**: asked for on one call, which starts no
        // session, and presented on the boot that follows. See
        // [`crate::session::challenge`].
        let asking = args.ask_for_code.unwrap_or(false);
        let reference = args
            .challenge
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty());
        let presented = args
            .code
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty());
        if asking && (reference.is_some() || presented.is_some() || resume.is_some()) {
            return Ok(misused(
                "asking for a code and presenting one are two calls: ask_for_code starts no \
                 session, so it cannot also boot one. Call with bot and ask_for_code alone to \
                 get a challenge, then again with bot, challenge and code to boot."
                    .to_string(),
            ));
        }
        if (asking || reference.is_some() || presented.is_some()) && bot.is_none() {
            return Ok(misused(
                "a code is for a bot: pass `bot`, the one the code boots as, with ask_for_code \
                 to ask for it or with challenge and code to present it"
                    .to_string(),
            ));
        }
        if reference.is_some() != presented.is_some() {
            return Ok(misused(
                "a code is presented with both halves: `challenge`, the reference the ask \
                 answered with, and `code`, what the operator typed. Send both with `bot`."
                    .to_string(),
            ));
        }
        // **A wrap code reopens a wrapped run for one last change and takes no
        // code**: a run that is already closed must not be handed a root. Refused
        // before the code is checked, so nothing is spent.
        if presented.is_some() && resume.is_some_and(jojobot_domain::session::is_wrap_code) {
            return Ok(misused(
                "a wrap code reopens a wrapped run for one last change, and a run that is \
                 closed is given no voucher. Nothing was spent. Reopen with the wrap code alone, \
                 or boot a new run with bot, challenge and code."
                    .to_string(),
            ));
        }
        // **Held from the ask or the check of a code to the spending of it**, so
        // two asks cannot both stand and two boots presenting one code cannot
        // both be given a voucher.
        let _redeeming = match (asking, presented) {
            (false, None) => None,
            _ => Some(self.registry.redeem_lock().lock().await),
        };
        if asking && let Some(bot) = bot.as_ref() {
            let day = today.unwrap_or_else(|| self.clock().today_in(&jiff::tz::TimeZone::UTC));
            return self.ask_for_code(bot, day).await;
        }
        let verified = match (bot.as_ref(), reference, presented) {
            (Some(bot), Some(reference), Some(code)) => {
                match self.verify_code(bot, reference, code).await {
                    Ok(verified) => Some(verified),
                    Err(refused) => return Ok(refused),
                }
            }
            _ => None,
        };
        // **A compaction report is the live run, answered in full.** What it
        // forgets is decided first, and only a run that really opened gets
        // there.
        let compacted = args.compacted == Some(true);
        let reporter = if compacted {
            match self
                .report_compaction(args.sid.as_deref(), bot.as_ref())
                .await
            {
                Ok(reporter) => Some(reporter),
                Err(refused) => return Ok(refused),
            }
        } else {
            None
        };
        self.orient(orient::OrientRequest {
            redeem: verified.as_ref(),
            bot: reporter.as_ref().map(|run| &run.bot).or(bot.as_ref()),
            reported_run: reporter
                .as_ref()
                .filter(|_| args.sid.as_deref().map(str::trim).is_none_or(str::is_empty))
                .map(|run| run.sid.as_str()),
            brief: args.brief.unwrap_or(false) && !compacted,
            resume,
            timezone: timezone.as_deref(),
            instance_zone,
            today,
            carried,
            claim,
            sid: args.sid.as_deref(),
            compacted,
            offer_code: args
                .offer_code
                .as_deref()
                .map(str::trim)
                .filter(|code| !code.is_empty()),
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;

    /// **Which guard answers a `skill` call that also carries `resume`.**
    ///
    /// The skill guard reads `bot.is_some() || resume.is_some()`, and the
    /// `resume` half decides nothing: a `resume` with no bot is already
    /// refused above as a malformed call, so by the time the skill guard runs
    /// a `resume` always has a `bot` beside it and the first half is true.
    ///
    /// Both refusals are correct and they name different problems, which is
    /// why this pins which one answers rather than only that something did.
    #[tokio::test]
    async fn a_skill_fetch_with_resume_is_refused_by_the_guard_that_fits() {
        let jojobot = handler();

        // No bot: the malformed-resume refusal answers, before the skill guard
        // is reached at all.
        let out = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: Some("evidence".into()),
                section: None,
                resume: Some("jm7z".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert_eq!(body["status"], "blocked", "{body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap_or_default()
                .contains("resume answers the choice"),
            "the malformed-resume refusal must answer this one: {body}"
        );

        // With a bot, the skill guard answers, and it answers because of the
        // bot rather than the resume.
        let out = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("dev".into()),
                brief: None,
                skill: Some("evidence".into()),
                section: None,
                resume: Some("jm7z".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert_eq!(body["status"], "blocked", "{body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap_or_default()
                .contains("two calls"),
            "the skill guard must answer this one: {body}"
        );
    }

    /// **The instructed mid-session fetch, with the handle in hand.**
    ///
    /// This door's own description sends a booted session back for a procedure
    /// by name, and a booted session is carrying its `sid` — the essay tells it
    /// to, on writes and reads alike. So the fetch takes one, hands back the
    /// body, and says what the handle is worth on the way past.
    ///
    /// Paired with what the fetch must NOT become: reading a procedure is a
    /// read, so it still starts nothing, handle or no handle.
    #[tokio::test]
    async fn a_skill_is_fetched_with_the_handle_the_session_is_carrying() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let live = booted(&jojobot, "gamma").await;

        let fetched = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: Some("evidence".into()),
                    section: None,
                    resume: None,
                    sid: Some(live.clone()),
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            fetched["skill"]["body"]
                .as_str()
                .is_some_and(|body| !body.is_empty()),
            "the procedure comes back to the session that asked for it: {fetched}"
        );
        assert_eq!(fetched["carried_session"], "held", "{fetched}");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "a fetch is a read: it begins nothing, whoever asks"
        );
    }

    /// **The door never turns a handle away, and says what the one you carried
    /// is worth.**
    ///
    /// This is the door a session comes back to when the surface stops looking
    /// like the one it booted on — and every handle is dead after a restart, so
    /// refusing one here would shut the way back on exactly the caller that
    /// needs it, in the words of the call it just made.
    ///
    /// Four answers, each pairing with the others: a live handle is `held`, a
    /// well-formed handle nothing is holding is `gone` and the answer still
    /// lands whole, a string that is no handle at all is `malformed`, and a
    /// call carrying none says nothing about one. Without the first this passes
    /// against a door that says `gone` to everything; without the third,
    /// against a door that tells a caller whose handle arrived upcased or
    /// truncated that its session ended.
    #[tokio::test]
    async fn the_door_says_what_the_handle_you_carried_is_worth() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        make_bot(&jojobot, "gamma").await;
        let live = booted(&jojobot, "gamma").await;

        let orienting = async |sid: Option<String>| {
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        request_id: None,
                        compacted: None,
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: Some(true),
                        skill: None,
                        section: None,
                        resume: None,
                        sid,
                        today: None,
                        ask_for_code: None,
                        challenge: None,
                        code: None,
                        offer_code: None,
                    }))
                    .await
                    .expect("start_here ok"),
            )
        };

        let held = orienting(Some(live.clone())).await;
        assert_eq!(held["carried_session"], "held", "{held}");

        // Well-formed and never minted — the shape every handle takes once the
        // process holding it has gone.
        let never_minted = jojobot_domain::session::mint_key();
        let lost = orienting(Some(never_minted.clone())).await;
        assert_eq!(lost["carried_session"], "gone", "{lost}");
        assert_ne!(
            lost["status"], "blocked",
            "the door a lost session comes back to must not turn it away: {lost}"
        );
        assert_eq!(
            lost["snapshot"]["entities"]["available"], true,
            "…and must answer whole, which is what it was called for: {lost}"
        );

        // **The same four characters, upcased — and a different answer.** A
        // handle a client quoted, cased or truncated names nothing either, and
        // `gone` sends its holder to boot a second run; `malformed` sends it to
        // look at what it sent, which is the one that gets it back to the
        // session still sitting there. Read beside `lost` above: the two must
        // not collapse into one word in either direction.
        let mistyped = orienting(Some(never_minted.to_uppercase())).await;
        assert_eq!(mistyped["carried_session"], "malformed", "{mistyped}");
        assert_ne!(
            mistyped["status"], "blocked",
            "…and a mistyped handle is answered here too, never refused: {mistyped}"
        );

        let anonymous = orienting(None).await;
        assert!(
            anonymous["carried_session"].is_null(),
            "a call carrying no handle is told nothing about one: {anonymous}"
        );
    }

    #[tokio::test]
    async fn start_here_lands_a_fresh_agent_with_the_world_and_a_snapshot() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(AddEntityArgs {
                request_id: None,
                kind: "person".into(),
                handle: "milhouse".into(),
                name: "Milhouse".into(),
                aliases: None,
                source: "user-named".into(),
                crm: None,
                boot: None,
                parent: None,
                sets: None,
                override_token: None,
                override_reason: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("entity ok");
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;

        let out = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        let orientation = body["orientation"].as_str().expect("orientation prose");
        // The orientation must teach the load-bearing vocabulary, not assume it.
        for taught in [
            "entity",
            "fact",
            "testimony",
            "inference",
            "edge",
            "mailbox",
            "processed",
            "search",
            "blocked",
            // The norms the box-minting review added (2026-07-26): a mailbox is
            // a channel someone drains, never minted mid-errand; changed claims
            // archive rather than overwrite; ambiguity goes to the operator.
            "drain",
            "archived",
            "ask the operator",
            // M4: an identity is a thing a session can be, and the orientation
            // has to say what one is made of before the door hands one over.
            "bot",
            "charter",
        ] {
            assert!(
                orientation.contains(taught),
                "the orientation never teaches `{taught}`"
            );
        }
        // Two entities, and the second one is the point: the box below belongs
        // to a bot, because there is no other kind of box.
        assert_eq!(body["snapshot"]["entities"]["count"], 2);
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["person"], 1);
        assert_eq!(body["snapshot"]["entities"]["by_kind"]["bot"], 1);
        let bots = body["snapshot"]["entities"]["bots"]
            .as_array()
            .expect("the bots");
        // Mail hangs off the bot that owns it — there is no second population.
        // Anonymous orientation drains nothing, so it sees no queue: every box
        // has an owner, so mail is either yours or somebody's, and this caller
        // is nobody.
        assert_eq!(
            bots[0]["yours"], false,
            "an anonymous caller drains nothing"
        );
        assert!(
            bots[0]["mail"]["counts"].is_null(),
            "…and somebody else's queue is not its to weigh: {:?}",
            bots[0]
        );
    }

    /// **A returning session pays for the essay once.** The orientation prose
    /// is the only part of this answer that does not change between calls, and
    /// it rode every one of them — so a client running a boot-surface token
    /// budget skipped orientation entirely rather than paying for it again,
    /// which is the opposite of what it is for. `brief` returns everything that
    /// moves, and says plainly that the essay is what it left out.
    #[tokio::test]
    async fn a_brief_orientation_keeps_the_snapshot_and_drops_only_the_essay() {
        // **Bare on purpose, and not proof about a real instance.** It compares
        // the brief answer with a full one; a real fresh instance holds the
        // shipped kinds with their keys declared and its full anonymous boot
        // leaves `## Sessions` out because the whole essay does not fit.
        let jojobot = handler_bare();
        ensure(&jojobot, "alpha").await;

        let full = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(full["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(full["orientation_elided"], false);

        let brief = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: Some(true),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(
            brief["orientation"].is_null(),
            "the essay is what was dropped: {brief}"
        );
        assert_eq!(brief["orientation_elided"], true);
        assert_eq!(
            full["orientation_elided"], false,
            "…and the marker says which of the two answers this is: {full}"
        );

        // How to get it back must be on the surface a caller reads: an
        // elision nobody can undo is an elision that costs the reader the
        // thing it saved.
        let tools = Jojobot::tool_router().list_all();
        let door = tools
            .iter()
            .find(|t| t.name == "start_here")
            .expect("start_here is a tool");
        let description = door.description.as_deref().unwrap_or_default();
        assert!(
            description.contains("without brief"),
            "the way back to the essay must be stated where brief is: {description}"
        );

        // A boot given a wrap_code as its resume answer writes a window open, so
        // the description that lists what a boot writes names that route.
        assert!(
            description.contains("wrap_code"),
            "the door's description names the wrap_code route: {description}"
        );

        // Everything that changes between calls is still here.
        assert_eq!(brief["snapshot"], full["snapshot"]);
        assert_eq!(brief["snapshot"]["entities"]["available"], true);
        assert!(brief["snapshot"]["mail"].is_object());
    }

    /// There is no version stamp on the essay, deliberately: every way of
    /// keeping a freshness check honest (a prose hash, a derived version, a
    /// hand-maintained one) was rejected, because a number a human has to
    /// remember to bump is a number that lies.
    ///
    /// Asserted over the whole payload and the whole surface, not over one
    /// or two specific keys: a key-by-key check would miss the idea
    /// reappearing somewhere adjacent, in a note or an arg doc.
    #[tokio::test]
    async fn nothing_on_the_surface_stamps_the_orientation_with_a_version() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store);
        make_bot(&jojobot, "gamma").await;

        let answers = [
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        request_id: None,
                        compacted: None,
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: None,
                        skill: None,
                        section: None,
                        resume: None,
                        sid: None,
                        today: None,
                        ask_for_code: None,
                        challenge: None,
                        code: None,
                        offer_code: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        request_id: None,
                        compacted: None,
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: Some(true),
                        skill: None,
                        section: None,
                        resume: None,
                        sid: None,
                        today: None,
                        ask_for_code: None,
                        challenge: None,
                        code: None,
                        offer_code: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            boot(&jojobot, "gamma").await,
        ];
        for body in &answers {
            assert!(
                !body.to_string().contains("orientation_version"),
                "no answer carries a version stamp: {body}"
            );
            assert!(
                body.get("how_to_read_orientation").is_none(),
                "…nor the nudge that existed only to explain one: {body}"
            );
        }

        for tool in Jojobot::tool_router().list_all() {
            let description = tool.description.as_deref().unwrap_or_default();
            let schema = serde_json::to_string(&tool.input_schema).expect("a schema");
            for surface in [description, schema.as_str()] {
                assert!(
                    !surface.contains("orientation_version"),
                    "{} still teaches a version stamp: {surface}",
                    tool.name
                );
            }
        }
    }

    /// A boot is brief the same way, and never at the cost of the things a boot
    /// exists for: the identity, its box, and its session.
    #[tokio::test]
    async fn a_brief_boot_still_hands_over_the_identity_and_the_session() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(true),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("boot ok"),
        );
        assert!(booted["orientation"].is_null());
        assert_eq!(booted["orientation_elided"], true);
        assert_eq!(booted["identity"]["bot"]["id"], "bot:gamma");
        assert_eq!(booted["session"]["available"], true);
        assert_eq!(booted["session"]["resumed"], false);
    }

    /// One world being down must not take orientation with it: a fresh agent
    /// on a half-configured server still deserves the map.
    #[tokio::test]
    async fn start_here_survives_a_world_that_is_down() {
        let out = handler_with_mailboxes_down(Arc::new(InMemoryMemory::booted()))
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("orientation still lands");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert!(body["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(body["snapshot"]["mail"]["available"], false);
    }

    /// **A misuse is an ANSWER, not a thrown error.** `resume` responds to an
    /// offer only a named boot makes, so carrying one without a `bot` is a
    /// caller mistake — and every other caller mistake on this surface comes
    /// back as a blocked result a client can branch on. A thrown error is not a
    /// value: the model on the other end gets a failure where it should get a
    /// next move, and the prose telling it what to do instead is stranded in a
    /// channel nothing reads structurally.
    ///
    /// **The prose was already right and is kept verbatim** — it names both ways
    /// forward. Only the channel changes.
    #[tokio::test]
    async fn resume_without_a_bot_is_a_blocked_answer_rather_than_a_thrown_error() {
        let jojobot = handler();
        let out = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("a misuse is an answer, not a protocol failure");
        let body = blocked(&out);
        assert_eq!(body["wrote"], false, "nothing was started: {body}");

        // Both ways forward survive the move, because that is the whole value of
        // the answer over the error.
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("bot") && how.contains("resume"),
            "the advice names both moves: {how}"
        );

        // …and no session was minted behind the refusal.
        assert!(
            body["sid"].is_null(),
            "a refused boot hands back no handle: {body}"
        );
    }

    /// This door boots bots. A bare name is read as one, and a handle of another
    /// kind is the caller's mistake — booting a person as an identity would hand
    /// back somebody's page as a charter.
    #[tokio::test]
    async fn the_door_reads_a_bare_name_as_a_bot_and_refuses_another_kind() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        assert_eq!(
            boot(&jojobot, "bot:gamma").await["identity"]["bot"]["id"],
            "bot:gamma",
            "a fully qualified bot handle is the same door"
        );

        let err = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("person:milhouse".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect_err("another kind must be refused");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);
        assert!(
            err.message.contains("bot"),
            "the error says what this door takes: {}",
            err.message
        );
    }

    /// A name that is no bot comes back in the guards' own shape — nothing was
    /// written, here is what jojobot suspects you meant — rather than a fresh
    /// identity conjured out of a typo.
    ///
    /// **And with the roster, not only the near misses.** `candidates` answers
    /// "did you mean one of these", so it is EMPTY whenever the name resembles
    /// nothing — and an empty list reads as a broken server to the one caller
    /// who most needs telling who does exist. The way out is an offer: boot as
    /// somebody real and create the identity you wanted from in there.
    #[tokio::test]
    async fn booting_an_unknown_bot_answers_with_the_roster_and_an_offer() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        ensure(&jojobot, "alpha").await;

        // A near miss: the candidates are the guards' own answer, and they stay.
        let near = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamm".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(near["attempted"], "bot:gamm");
        assert_eq!(near["candidates"][0]["handle"], "bot:gamma");

        // A name resembling nothing: the candidate list is empty, and the
        // answer still has to be useful.
        let stranger = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("nobody".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert!(
            stranger["candidates"]
                .as_array()
                .expect("a list")
                .is_empty(),
            "nothing resembles this name, which is exactly the case: {stranger}"
        );

        for body in [&near, &stranger] {
            let roster: Vec<&str> = body["bots"]
                .as_array()
                .expect("the roster is a list")
                .iter()
                .map(|b| b.as_str().expect("a handle"))
                .collect();
            // Handle order, the order the boot's own roster is listed in.
            assert_eq!(roster, ["bot:delta", "bot:gamma"], "who exists: {body}");
            let how = body["how_to_proceed"].as_str().expect("advice");
            assert!(
                how.contains("bot:gamma"),
                "the roster is in the words too: {how}"
            );
            assert!(
                how.contains("Boot as one of these") && how.contains("from inside that session"),
                "the offer is the way out: {how}"
            );
            assert!(
                how.contains("mints nothing"),
                "…and the door says what it will not do: {how}"
            );
        }

        // **Nothing was written.** Not the identity, not a session, not a box.
        let listed = json_of(
            &jojobot
                .list_entities(Parameters(ListEntitiesArgs {
                    kind: Some("bot".into()),
                    parent: None,
                    offset: None,
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("list ok"),
        );
        assert_eq!(
            listed["count"], 2,
            "a refused boot mints no identity: {listed}"
        );
    }

    /// The empty board says something different, because "boot as one of these"
    /// is no offer when there is nobody to boot as.
    #[tokio::test]
    async fn booting_into_an_empty_roster_says_so_rather_than_offering_nobody() {
        let jojobot = handler();
        let body = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert!(body["bots"].as_array().expect("a list").is_empty());
        let how = body["how_to_proceed"].as_str().expect("advice");
        assert!(
            how.contains("no bots on this server") && how.contains("add_entity"),
            "with nobody to boot as, the way out is the verb that creates one: {how}"
        );
    }

    /// **A fresh claim on a named role is taken, through the real door.**
    #[tokio::test]
    async fn a_fresh_claim_on_a_named_role_is_taken() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let booted = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            booted["session"]["claim"]["role"], "dev-dispatch",
            "{booted}"
        );
        assert_eq!(booted["session"]["claim"]["status"], "taken", "{booted}");
    }

    /// **A running session that holds no lease claims its role with its own
    /// sid.** The boot that gave it the sid claimed nothing; naming the role on
    /// a later call, with the live sid, answers the claim for that session and
    /// offers no resume-or-new. Paired with the refusal: the same call from a
    /// second running session of the bot, while the first holds a fresh lease,
    /// is refused naming the holder.
    #[tokio::test]
    async fn a_running_session_claims_its_role_with_its_own_sid() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;
        let args = |sid: Option<String>, claim: Option<&str>, resume: Option<&str>| OrientArgs {
            request_id: None,
            compacted: None,
            claim: claim.map(str::to_string),
            timezone: None,
            bot: Some("gamma".into()),
            brief: Some(true),
            skill: None,
            section: None,
            resume: resume.map(str::to_string),
            sid,
            today: None,
            ask_for_code: None,
            challenge: None,
            code: None,
            offer_code: None,
        };

        // The boot claims nothing and hands the session its sid.
        let first = json_of(
            &jojobot
                .start_here(Parameters(args(None, None, None)))
                .await
                .expect("start_here ok"),
        );
        let first_sid = sid_of(&first).expect("the first boot hands a sid");
        assert!(first["session"].get("claim").is_none(), "{first}");

        // The same session names the role, carrying its sid.
        let claimed = json_of(
            &jojobot
                .start_here(Parameters(args(
                    Some(first_sid.clone()),
                    Some("dev-dispatch"),
                    None,
                )))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            sid_of(&claimed).as_deref(),
            Some(first_sid.as_str()),
            "the claim is answered for the session that carried the sid, with no new one: {claimed}"
        );
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
        assert!(
            claimed["session"].get("choices").is_none(),
            "no resume-or-new is offered to a session that is running: {claimed}"
        );

        // A second running session of the bot, naming the same role while the
        // lease is fresh, is refused naming the first.
        let second = json_of(
            &jojobot
                .start_here(Parameters(args(None, None, Some("new"))))
                .await
                .expect("start_here ok"),
        );
        let second_sid = sid_of(&second).expect("the second session's handle");
        assert_ne!(second_sid, first_sid);
        let refused = json_of(
            &jojobot
                .start_here(Parameters(args(
                    Some(second_sid.clone()),
                    Some("dev-dispatch"),
                    None,
                )))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            refused["session"]["claim"]["status"], "refused",
            "{refused}"
        );
        assert_eq!(
            refused["session"]["claim"]["holder"],
            run_name(&jojobot, &first_sid),
            "the refusal names the session that holds the role: {refused}"
        );
    }

    /// **A second claimant is refused while the first claim is fresh, and told
    /// who holds it and until when — through the real door.**
    #[tokio::test]
    async fn a_second_claimant_is_refused_by_the_door_while_the_lease_is_fresh() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let holder_sid = sid_of(&first).expect("the first claimant's handle");
        assert_eq!(first["session"]["claim"]["status"], "taken", "{first}");

        // A distinct session of the SAME bot, naming the SAME role.
        let second = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_ne!(
            sid_of(&second).as_deref(),
            Some(holder_sid.as_str()),
            "a distinct session of the same bot: {second}"
        );
        assert_eq!(second["session"]["claim"]["status"], "refused", "{second}");
        assert_eq!(
            second["session"]["claim"]["fix_by"], "retry",
            "a held role frees when its lease ends: {second}"
        );
        assert_eq!(
            second["session"]["claim"]["holder"],
            run_name(&jojobot, &holder_sid),
            "the refusal names who holds it: {second}"
        );
        assert!(
            second["session"]["claim"]["until"].is_string(),
            "…and until when: {second}"
        );
        // **And what to do about it**, naming the two things a caller acts on:
        // who holds it and until when.
        let how = second["session"]["claim"]["how_to_proceed"]
            .as_str()
            .unwrap_or_else(|| panic!("a refused claim says how to proceed: {second}"));
        let until = second["session"]["claim"]["until"]
            .as_str()
            .unwrap_or_default();
        assert!(
            how.contains(run_name(&jojobot, &holder_sid).as_str()) && how.contains(until),
            "the way forward names the holder and the day it lapses: {how}"
        );
    }

    /// **The same claimant renewing its own claim is never refused, and a
    /// claim on a DIFFERENT role never meets a lease it did not claim** —
    /// through the real door, and both read back correctly.
    #[tokio::test]
    async fn the_same_claimant_renews_and_an_unrelated_role_is_untouched() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );

        // The same claimant, naming the same role again: not refused.
        let renewed = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(run_of(&first).expect("a run name")),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            renewed["session"]["claim"]["status"], "taken",
            "the holder renewing its own claim: {renewed}"
        );

        // A DIFFERENT role, claimed by a distinct session: never meets the
        // first role's lease.
        let unrelated = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("reviewer-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            unrelated["session"]["claim"]["status"], "taken",
            "an unclaimed role, named by anyone: {unrelated}"
        );
    }

    /// **A write other than `journal` renews a role claim too**, and one
    /// landing after the OLD five-minute window still keeps it — through the
    /// served surface, with no real waiting.
    ///
    /// Three handlers share one store and registry but run different clocks,
    /// which is what stands in for time actually passing: `Clock::Stated`'s
    /// `now()` is `midnight(day) + (real elapsed since `began`)`, so choosing
    /// `began` in the past reads as if that much time had already gone by,
    /// deterministically and in milliseconds of real test time.
    ///
    /// The gap between the claim and the write (400s) is past the old
    /// five-minute lease on its own — if renewal only happened on `journal`,
    /// this write would find nothing to renew and the claim would still read as
    /// claimed at t0. The final check (3000s after t0) is stale against t0
    /// alone but fresh against a renewal at t0+400s under the lease that holds
    /// now (`LEASE_FRESHNESS`), so the two cases read oppositely on the second
    /// claimant.
    #[tokio::test]
    async fn a_write_other_than_journal_renews_a_role_claim_past_the_old_five_minute_window() {
        let memory = Arc::new(InMemoryMemory::booted());
        let search = Arc::new(SpySearch::default());
        let mailboxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let sessions = Arc::new(InMemorySessions::new());
        let teachings = Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new());
        let registry = Arc::new(crate::sid::SessionRegistry::new());
        let day: jiff::civil::Date = "2026-06-01".parse().expect("a date");

        let at = |elapsed_secs: i64| {
            Jojobot::new(
                memory.clone(),
                search.clone(),
                mailboxes.clone(),
                sessions.clone(),
                teachings.clone(),
                registry.clone(),
            )
            .on_clock(jojobot_domain::clock::Clock::Stated {
                day,
                began: jiff::Timestamp::now() - jiff::SignedDuration::from_secs(elapsed_secs),
            })
        };

        let t0 = at(0);
        make_bot(&t0, "gamma").await;
        let claimed = json_of(
            &t0.start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: Some("dev-dispatch".into()),
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok"),
        );
        let holder_sid = sid_of(&claimed).expect("a handle");
        assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");

        // A write other than journal, carrying the holder's own sid, 400
        // seconds later — past the old 300-second window.
        ensure_as(&at(400), &holder_sid, "alpha").await;

        // 3000 seconds after the ORIGINAL claim: stale against t0 alone
        // (3000 > 2700), fresh against a renewal at t0+400 (3000-400 = 2600
        // < 2700).
        let rival = json_of(
            &at(3000)
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            rival["session"]["claim"]["status"], "refused",
            "the intervening write must have renewed the claim, or a rival 3000s after the \
             original claim would find it stale: {rival}"
        );
        assert_eq!(
            rival["session"]["claim"]["holder"],
            run_name(&at(3000), &holder_sid),
            "{rival}"
        );
    }

    /// **A poll of one's OWN mailbox, made with one's own sid, renews that
    /// session's role claim — the same as a write does.** A session that only
    /// polls writes nothing, so without this its healthy claim ages out and
    /// every sweep probes it. Both ways of reading the box renew: the count and
    /// the delivery.
    ///
    /// Timeline on a stated clock: the claim is taken at t0, the poll is at
    /// 2000s, and a rival claims at 3000s. The lease is 2700s, so t0 alone is
    /// stale at 3000s and a renewal at 2000s is fresh. **Three controls ride
    /// beside the positive**, each its own world: nobody polls (the claim goes
    /// stale), ANOTHER bot polls its own box (nothing of the holder's
    /// renews), and the holder's session wraps before it polls (a wrapped
    /// session's poll must not lease the role again).
    #[tokio::test]
    async fn a_poll_of_ones_own_mailbox_renews_the_role_claim_and_no_other_read_does() {
        struct World {
            memory: Arc<InMemoryMemory>,
            search: Arc<SpySearch>,
            mailboxes: Arc<InMemoryMailboxes>,
            sessions: Arc<InMemorySessions>,
            teachings: Arc<jojobot_domain::teaching::testing::InMemoryTeachings>,
            registry: Arc<crate::sid::SessionRegistry>,
        }
        impl World {
            fn new() -> Self {
                Self {
                    memory: Arc::new(InMemoryMemory::booted()),
                    search: Arc::new(SpySearch::default()),
                    mailboxes: Arc::new(InMemoryMailboxes::knowing_any_owner()),
                    sessions: Arc::new(InMemorySessions::new()),
                    teachings: Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
                    registry: Arc::new(crate::sid::SessionRegistry::new()),
                }
            }
            /// The server `elapsed_secs` after the claim was taken.
            fn at(&self, elapsed_secs: i64) -> Jojobot {
                Jojobot::new(
                    self.memory.clone(),
                    self.search.clone(),
                    self.mailboxes.clone(),
                    self.sessions.clone(),
                    self.teachings.clone(),
                    self.registry.clone(),
                )
                .on_clock(jojobot_domain::clock::Clock::Stated {
                    day: "2026-06-01".parse().expect("a date"),
                    began: jiff::Timestamp::now() - jiff::SignedDuration::from_secs(elapsed_secs),
                })
            }
        }
        let boot = |bot: &str, claim: Option<&str>| OrientArgs {
            request_id: None,
            compacted: None,
            claim: claim.map(str::to_string),
            timezone: None,
            bot: Some(bot.into()),
            brief: None,
            skill: None,
            section: None,
            resume: Some("new".into()),
            sid: None,
            today: None,
            ask_for_code: None,
            challenge: None,
            code: None,
            offer_code: None,
        };
        let poll = |sid: &str, counts_only: bool| ReadMailboxArgs {
            counts_only: Some(counts_only),
            new_only: None,
            sid: Some(sid.to_string()),
        };
        // The claim is taken at t0 by a session of `gamma`.
        let claimed_in = |world: &World| {
            let t0 = world.at(0);
            let boot = &boot;
            async move {
                make_bot(&t0, "gamma").await;
                make_bot(&t0, "alpha").await;
                let claimed = json_of(
                    &t0.start_here(Parameters(boot("gamma", Some("dev-dispatch"))))
                        .await
                        .expect("start_here ok"),
                );
                assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
                sid_of(&claimed).expect("a handle")
            }
        };
        let rival_at = |world: &World, secs: i64| {
            let server = world.at(secs);
            let boot = &boot;
            async move {
                json_of(
                    &server
                        .start_here(Parameters(boot("gamma", Some("dev-dispatch"))))
                        .await
                        .expect("start_here ok"),
                )
            }
        };

        // The positive, in both ways of reading the box.
        for counts_only in [true, false] {
            let world = World::new();
            let holder = claimed_in(&world).await;
            world
                .at(2000)
                .read_mailbox(Parameters(poll(&holder, counts_only)))
                .await
                .expect("poll ok");
            let rival = rival_at(&world, 3000).await;
            assert_eq!(
                rival["session"]["claim"]["status"], "refused",
                "a poll (counts_only: {counts_only}) must renew the holder's claim: {rival}"
            );
            assert_eq!(
                rival["session"]["claim"]["holder"],
                run_name(&world.at(0), &holder),
                "{rival}"
            );
        }

        // Control 1: a holder that does nothing at all goes stale.
        let world = World::new();
        claimed_in(&world).await;
        let rival = rival_at(&world, 3000).await;
        assert_eq!(rival["session"]["claim"]["status"], "taken", "{rival}");

        // Control 2: another bot's poll of its own box renews nothing here.
        let world = World::new();
        claimed_in(&world).await;
        let other = sid_of(&json_of(
            &world
                .at(1000)
                .start_here(Parameters(boot("alpha", None)))
                .await
                .expect("start_here ok"),
        ))
        .expect("a handle");
        world
            .at(2000)
            .read_mailbox(Parameters(poll(&other, true)))
            .await
            .expect("poll ok");
        let rival = rival_at(&world, 3000).await;
        assert_eq!(rival["session"]["claim"]["status"], "taken", "{rival}");

        // Control 3: a wrapped session's poll does not lease the role again.
        let world = World::new();
        let holder = claimed_in(&world).await;
        world
            .at(100)
            .wrap_session(Parameters(WrapSessionArgs {
                story: "done".into(),
                sid: holder.clone(),
            }))
            .await
            .expect("wrap ok");
        world
            .at(200)
            .read_mailbox(Parameters(poll(&holder, true)))
            .await
            .expect("a poll from a wrapped session is an answer");
        let rival = rival_at(&world, 300).await;
        assert_eq!(
            rival["session"]["claim"]["status"], "taken",
            "a poll from a wrapped session leased the role again: {rival}"
        );
    }

    /// **A section fetched by its exact heading returns exactly that unit,
    /// whole — and nothing else from the essay.** The positive this whole
    /// mechanism rests on, paired against a neighbouring section's own
    /// distinguishing text to prove the answer is one unit and not the
    /// whole remainder.
    #[tokio::test]
    async fn a_section_by_exact_heading_returns_exactly_that_unit() {
        let jojobot = handler();
        let (_, sections) = crate::orientation::essay::remainder_units(
            crate::orientation::essay::ORIENTATION_REMAINDER.as_str(),
        );
        let bots = sections
            .iter()
            .find(|s| s.heading == "## Bots")
            .expect("the essay has a Bots section");

        let fetched = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("## Bots".into()),
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(fetched["section"]["name"], "## Bots", "{fetched}");
        assert_eq!(fetched["section"]["body"], bots.body, "{fetched}");
        assert!(
            !fetched["section"]["body"]
                .as_str()
                .unwrap_or_default()
                .contains("The two endings")
                || bots.body.contains("The two endings"),
            "a neighbouring section's own text leaked into this one: {fetched}"
        );
        assert!(
            !fetched.to_string().contains("## Sessions"),
            "only the one unit asked for comes back: {fetched}"
        );
    }

    /// **`section: "opening"` returns the remainder's un-headed preamble,**
    /// the one unit with no heading of its own — paired against a real
    /// heading's own text to prove the answer is the preamble alone.
    #[tokio::test]
    async fn section_opening_returns_the_un_headed_preamble() {
        let jojobot = handler();
        let (preamble, _) = crate::orientation::essay::remainder_units(
            crate::orientation::essay::ORIENTATION_REMAINDER.as_str(),
        );

        let fetched = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("opening".into()),
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(fetched["section"]["name"], "opening", "{fetched}");
        assert_eq!(fetched["section"]["body"], preamble, "{fetched}");
        assert!(
            !fetched.to_string().contains("## Bots"),
            "the opening paragraphs carry no section heading: {fetched}"
        );
    }

    /// **A section name that matches nothing is refused, and the refusal
    /// lists every valid name — `"opening"` included.**
    #[tokio::test]
    async fn an_unknown_section_is_refused_and_lists_every_valid_name() {
        let jojobot = handler();
        let (_, sections) = crate::orientation::essay::remainder_units(
            crate::orientation::essay::ORIENTATION_REMAINDER.as_str(),
        );

        let refused = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    request_id: None,
                    compacted: None,
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("## Nonexistent".into()),
                    resume: None,
                    sid: None,
                    today: None,
                    ask_for_code: None,
                    challenge: None,
                    code: None,
                    offer_code: None,
                }))
                .await
                .expect("an unknown section is an answer, not a protocol failure"),
        );
        assert_eq!(refused["attempted"], "## Nonexistent", "{refused}");
        let how = refused["how_to_proceed"].as_str().expect("advice");
        assert!(how.contains("opening"), "{how}");
        for section in &sections {
            assert!(how.contains(section.heading), "{}\n{how}", section.heading);
        }
    }

    /// **Fetching a section and booting an identity are two calls**, the
    /// same guard `skill` already wears — a fetch starts no session, so
    /// honouring `bot` alongside it would hand back a body and no handle.
    #[tokio::test]
    async fn a_section_fetch_with_a_bot_is_refused() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let out = jojobot
            .start_here(Parameters(OrientArgs {
                request_id: None,
                compacted: None,
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: Some("## Bots".into()),
                resume: None,
                sid: None,
                today: None,
                ask_for_code: None,
                challenge: None,
                code: None,
                offer_code: None,
            }))
            .await
            .expect("start_here ok");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert_eq!(body["status"], "blocked", "{body}");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap_or_default()
                .contains("two calls"),
            "{body}"
        );
    }

    /// **Naming no role is the ordinary boot: unchanged.** No `claim` key
    /// appears anywhere in the answer.
    #[tokio::test]
    async fn naming_no_role_is_the_ordinary_boot() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let booted = boot(&jojobot, "gamma").await;
        assert!(
            booted["session"].get("claim").is_none(),
            "no role was named, so nothing about a claim is in the answer: {booted}"
        );
    }
}
