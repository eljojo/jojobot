//! `start_here` — The one orienting door, with or without an identity.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `start_here` — **the one door**, with or without an identity.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct OrientArgs {
    /// Optional. The bot to boot as: its bare slug, or its full `bot:`-prefixed
    /// handle. A handle of any other kind is refused — this door boots bots.
    /// Omit it for an anonymous orientation: you get the world and the
    /// snapshot, and no sid.
    #[serde(default)]
    pub(crate) bot: Option<String>,
    /// Skip the orientation essay and return only what changes between calls —
    /// the snapshot, your identity, your session.
    #[serde(default)]
    pub(crate) brief: Option<bool>,
    /// Your answer to the resume-or-new choice a boot hands back when this bot
    /// has a session worth picking up: the `sid` of the one you are resuming,
    /// exactly as the offer spelled it, or `new` for a fresh session. Leave it
    /// off on a first boot — there is nothing to answer yet.
    #[serde(default)]
    pub(crate) resume: Option<String>,
    /// **A skill to read, by the name the index gave it.** The boot lists every
    /// skill by name and when-to-use and ships no bodies; this is how you get
    /// one, once the index has told you it is relevant.
    ///
    /// It is the same door because this door is skill zero: it is where a
    /// session learns the model, so it is where a session learns the
    /// procedures. A name that is no skill comes back blocked, naming the ones
    /// that are.
    #[serde(default)]
    pub(crate) skill: Option<String>,
    /// **One unit of the essay's remainder, left out of an anonymous boot's
    /// answer by the ceiling** — by its exact `##` heading, or the literal
    /// word `"opening"` for the remainder's un-headed opening paragraphs.
    /// The answer carries that one unit, whole, and nothing else from the
    /// essay — never the core, which always ships and needs no fetch.
    ///
    /// It reads the same way `skill` does: no bot, no session, a pure fetch.
    /// A name that matches no section comes back blocked, naming every
    /// valid one, `"opening"` included.
    #[serde(default)]
    pub(crate) section: Option<String>,
    /// The session handle you are already carrying, if you have one — the same
    /// `sid` that rides every other call you make. Leave it off when you have
    /// none: this door is where one comes from, so a first boot has nothing to
    /// pass. **A handle is never turned away here**, whatever became of it: the
    /// answer says whether the one you carried still addresses your session, so
    /// a session that came back to a server it does not recognise learns that
    /// in the same call it re-orients with.
    #[serde(default)]
    pub(crate) sid: Option<String>,
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
    /// **The timezone this session works in** — an IANA name like
    /// `America/New_York` or `Europe/Madrid`. Send it when you boot, and send
    /// it again when you resume from somewhere else.
    ///
    /// It is what *today* means for everything day-grained: the date a `capture`
    /// gets when you name none, and whether a recurring loop has fallen due.
    /// A zone you send wins over the instance's.
    ///
    /// ⚠️ **Two sessions in different zones will disagree about what today is
    /// for the same stored claim, and that is correct.** A claim captured at
    /// nine in the evening in New York is the 18th there and the 19th in
    /// Madrid; both runs are reading the same claim and answering in their own
    /// frame. It is not a fault and there is nothing to work around.
    ///
    /// Send none and the instance's zone answers, or UTC when its operator set
    /// none; the boot says which. On a resume, sending none keeps the zone the
    /// run already had rather than moving it.
    #[serde(default)]
    pub(crate) timezone: Option<String>,
    /// **A role to claim, by a name you choose.** Boot names this when you
    /// want jojobot to refuse a second claimant while your claim is fresh —
    /// **`taken` means your claim succeeded and YOU hold the role; it never
    /// means somebody else has it, which is `refused`.** The answer is one of
    /// `taken`, `refused` naming who holds it and until
    /// when, `conflict` when the claim collided with another write
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
    /// rest all count — and the lease is 45 minutes. **Wrapping releases
    /// whatever you held**, so a fresh session may claim the same role at
    /// once.
    #[serde(default)]
    pub(crate) claim: Option<String>,
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
        description = "New here? Call this first — it is the ONE door, whether or not you have an \
                       identity. Explains what jojobot is and how its world fits together — \
                       entities, facts, provenance, edges, mailboxes — with worked examples, and \
                       returns a live snapshot of what exists right now (entities by kind, EVERY \
                       BOT NAMED so you can see which identities you could boot as, each with \
                       its own mail beside it — counts on the one box you drain, elided on \
                       everybody else's), so you start oriented instead of guessing. IT ALSO LISTS THE SKILLS this build \
                       ships — a name and what each is FOR, never the procedures themselves. \
                       When one of them matches the job in front of you, call this again with \
                       skill: its name and you get that body. Nothing here decides when a \
                       skill applies; the index says what each is for and you choose. AN \
                       ANONYMOUS BOOT WHOSE ESSAY DID NOT FIT WHOLE names every section it left \
                       out — call this again with section: one of those exact headings, or \
                       \"opening\" for the remainder's un-headed opening paragraphs, and you get \
                       that one unit whole, no bot and no session either. CALLED THIS \
                       BEFORE? Pass brief: true and you get the snapshot without the essay — the \
                       essay is the only part that does not change between calls, and calling \
                       again without brief reads it in full. NAME A BOT and the same answer also \
                       carries that identity: its charter (the orienting text — what this \
                       identity is, its hard lines, where its work lives), its rules as dated \
                       claims each carrying its own provenance (testimony and observation are \
                       settled, inference \
                       is a hypothesis — read them that way), and the per-state counts of the \
                       mailbox it owns. ANSWERING THE RESUME-OR-NEW OFFER? That answer carries \
                       no charter: only a boot that shipped one hands the offer back, so you \
                       are holding it already. It says so rather than reading as a bot with \
                       none, and booting again with no resume reads it in full. THIS DOOR \
                       CREATES NO IDENTITY: a name that is no bot \
                       comes back status: blocked, listing the bots that do exist and offering to \
                       boot as one of them. It does REPAIR one thing: a bot whose box is missing \
                       gets it opened here, because a box is part of what a bot is and its \
                       absence is damage rather than a setup step — and the answer says so \
                       plainly rather than reading as normal. BOOTING STARTS OR RESUMES THAT \
                       BOT'S SESSION — there is no separate start verb. It first sweeps that \
                       bot's sessions that have gone a day without a beat to `abandoned`. That \
                       sweep and that repair are the only things a boot writes. Name no bot at \
                       all and this is an orientation \
                       preview: read-only, the world and the snapshot, no identity and no \
                       session. THIS DOOR IS WHERE A SESSION HANDLE COMES FROM, and it takes one \
                       too: that handle rides every verb after it, reads included, because it is \
                       how jojobot knows which bot is asking — and this call is one of them. Pass \
                       `sid` if you are already carrying one and the answer says whether it still \
                       addresses your session; leave it off on a first boot, when you have none. \
                       The `sid` you carry is never turned away here, whatever became of it: this \
                       is the door you come back to. PASS `claim` WITH A ROLE NAME TO HOLD THAT \
                       ROLE for 45 minutes, so two sessions never work one role at once: the \
                       answer says `taken` (your claim succeeded and you hold the role), \
                       `refused` (somebody else holds it, and the answer names who and until when), \
                       `conflict` (the same call, retried, is the answer) or `unavailable` \
                       (the store could not decide the claim, nothing is held, and the answer \
                       says what to do next). Any write renews the lease and wrapping \
                       releases it."
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
            return match skills::named(wanted) {
                Some(skill) => json_result(&serde_json::json!({
                    "skill": {
                        "name": skill.name,
                        "when_to_use": skill.when_to_use,
                        "body": skill.body,
                    },
                    "carried_session": carried,
                })),
                None => Ok(handle_declined(
                    wanted,
                    format!(
                        "Nothing was read. '{wanted}' is not a skill this build ships. These \
                         are, by name: {}. Ask for one by its exact name; any boot lists them \
                         again with what each is for. They are compiled into the server, so \
                         there is no verb that adds one.",
                        skills::SKILLS
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
        self.orient(orient::OrientRequest {
            bot: bot.as_ref(),
            brief: args.brief.unwrap_or(false),
            resume,
            timezone: timezone.as_deref(),
            instance_zone,
            today,
            carried,
            claim,
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
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: Some("evidence".into()),
                section: None,
                resume: Some("jm7z".into()),
                sid: None,
                today: None,
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
                claim: None,
                timezone: None,
                bot: Some("dev".into()),
                brief: None,
                skill: Some("evidence".into()),
                section: None,
                resume: Some("jm7z".into()),
                sid: None,
                today: None,
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
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: Some("evidence".into()),
                    section: None,
                    resume: None,
                    sid: Some(live.clone()),
                    today: None,
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
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: Some(true),
                        skill: None,
                        section: None,
                        resume: None,
                        sid,
                        today: None,
                    }))
                    .await
                    .expect("start_here ok"),
            )
        };

        let held = orienting(Some(live.clone())).await;
        assert_eq!(held["carried_session"], "held", "{held}");

        // Well-formed and never minted — the shape every handle takes once the
        // process holding it has gone.
        let lost = orienting(Some("2gf7".into())).await;
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
        let mistyped = orienting(Some("2GF7".into())).await;
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
                kind: "person".into(),
                handle: "milhouse".into(),
                name: "Milhouse".into(),
                aliases: None,
                source: "user-named".into(),
                crm: None,
                boot: None,
                parent: None,
                override_token: None,
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("entity ok");
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;

        let out = jojobot
            .start_here(Parameters(OrientArgs {
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
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
        let jojobot = handler();
        ensure(&jojobot, "alpha").await;

        let full = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(full["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(full["orientation_elided"], false);

        let brief = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: Some(true),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: None,
                        skill: None,
                        section: None,
                        resume: None,
                        sid: None,
                        today: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        claim: None,
                        timezone: None,
                        bot: None,
                        brief: Some(true),
                        skill: None,
                        section: None,
                        resume: None,
                        sid: None,
                        today: None,
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
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: Some(true),
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
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
                claim: None,
                timezone: None,
                bot: None,
                brief: None,
                skill: None,
                section: None,
                resume: Some("new".into()),
                sid: None,
                today: None,
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
                claim: None,
                timezone: None,
                bot: Some("person:milhouse".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
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
                    claim: None,
                    timezone: None,
                    bot: Some("gamm".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
                    claim: None,
                    timezone: None,
                    bot: Some("nobody".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
            assert_eq!(roster, ["bot:gamma", "bot:delta"], "who exists: {body}");
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
                    claim: None,
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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

    /// **A second claimant is refused while the first claim is fresh, and told
    /// who holds it and until when — through the real door.**
    #[tokio::test]
    async fn a_second_claimant_is_refused_by_the_door_while_the_lease_is_fresh() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let first = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
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
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
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
            second["session"]["claim"]["holder"], holder_sid,
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
            how.contains(holder_sid.as_str()) && how.contains(until),
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
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: None,
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let holder_sid = sid_of(&first).expect("a handle");

        // The same claimant, naming the same role again: not refused.
        let renewed = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some(holder_sid.clone()),
                    sid: None,
                    today: None,
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
                    claim: Some("reviewer-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
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
    /// The gap between the claim and the write (400s) is past the OLD
    /// `LEASE_FRESHNESS` (300s) on its own — if renewal only happened on
    /// `journal`, this write would find nothing to renew and the claim would
    /// still read as claimed at t0. The final check (3000s after t0) is
    /// stale against t0 alone but fresh against a renewal at t0+400s under
    /// the new 45-minute threshold, so the two cases read oppositely on the
    /// second claimant.
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
                claim: Some("dev-dispatch".into()),
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: None,
                resume: None,
                sid: None,
                today: None,
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
                    claim: Some("dev-dispatch".into()),
                    timezone: None,
                    bot: Some("gamma".into()),
                    brief: None,
                    skill: None,
                    section: None,
                    resume: Some("new".into()),
                    sid: None,
                    today: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert_eq!(
            rival["session"]["claim"]["status"], "refused",
            "the intervening write must have renewed the claim, or a rival 3000s after the \
             original claim would find it stale: {rival}"
        );
        assert_eq!(rival["session"]["claim"]["holder"], holder_sid, "{rival}");
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
            claim: claim.map(str::to_string),
            timezone: None,
            bot: Some(bot.into()),
            brief: None,
            skill: None,
            section: None,
            resume: Some("new".into()),
            sid: None,
            today: None,
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
            assert_eq!(rival["session"]["claim"]["holder"], holder, "{rival}");
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
            crate::orientation::essay::ORIENTATION_REMAINDER,
        );
        let bots = sections
            .iter()
            .find(|s| s.heading == "## Bots")
            .expect("the essay has a Bots section");

        let fetched = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("## Bots".into()),
                    resume: None,
                    sid: None,
                    today: None,
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
            crate::orientation::essay::ORIENTATION_REMAINDER,
        );

        let fetched = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("opening".into()),
                    resume: None,
                    sid: None,
                    today: None,
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
            crate::orientation::essay::ORIENTATION_REMAINDER,
        );

        let refused = blocked(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    claim: None,
                    timezone: None,
                    bot: None,
                    brief: None,
                    skill: None,
                    section: Some("## Nonexistent".into()),
                    resume: None,
                    sid: None,
                    today: None,
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
                claim: None,
                timezone: None,
                bot: Some("gamma".into()),
                brief: None,
                skill: None,
                section: Some("## Bots".into()),
                resume: None,
                sid: None,
                today: None,
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
