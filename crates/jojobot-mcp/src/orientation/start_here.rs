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
    pub bot: Option<String>,
    /// Skip the orientation essay and return only what changes between calls —
    /// the snapshot, your identity, your session.
    #[serde(default)]
    pub brief: Option<bool>,
    /// Your answer to the resume-or-new choice a boot hands back when this bot
    /// has a session worth picking up: the `sid` of the one you are resuming,
    /// exactly as the offer spelled it, or `new` for a fresh session. Leave it
    /// off on a first boot — there is nothing to answer yet.
    #[serde(default)]
    pub resume: Option<String>,
}

/// **The one orienting door**, with or without an identity: the world-model
/// in prose, a live snapshot of what exists, and — when a bot is named —
/// that identity and its session.
///
/// There is deliberately no second verb for the identified case. The two
/// used to be separate doors over this same function, which is one text and
/// one snapshot by construction but two surfaces to keep true, and the
/// second one drifted.
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
                       BOT NAMED so you can see which identities you could boot as, and every \
                       mailbox by name — with counts for the ones you drain), so you start \
                       oriented instead of guessing. CALLED THIS \
                       BEFORE? Pass brief: true and you get the snapshot without the essay — the \
                       essay is the only part that does not change between calls, and calling \
                       again without brief reads it in full. NAME A BOT and the same answer also \
                       carries that identity: its charter (the orienting text — what this \
                       identity is, its hard lines, where its work lives), its rules as dated \
                       claims each carrying its own provenance (testimony is settled, inference \
                       is a hypothesis — read them that way), and the per-state counts of the \
                       mailbox it owns. THIS DOOR CREATES NO IDENTITY: a name that is no bot \
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
                       session. Pass the `sid` you were handed on EVERY call, reads included — it \
                       is how jojobot knows which bot is asking."
    )]
    pub(crate) async fn start_here(
        &self,
        Parameters(args): Parameters<OrientArgs>,
    ) -> Result<CallToolResult, McpError> {
        let bot = named_bot(args.bot.as_deref())?;
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
        self.orient(bot.as_ref(), args.brief.unwrap_or(false), resume)
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
                create_new: None,
                sid: None,
            }))
            .await
            .expect("entity ok");
        make_box(&jojobot, "inbox").await;
        send(&jojobot, "inbox", "epsilon", "the shipment landed").await;

        let out = jojobot
            .start_here(Parameters(OrientArgs {
                bot: None,
                brief: None,
                resume: None,
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
            // supersede rather than overwrite; ambiguity goes to the operator.
            "drain",
            "superseded",
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
        let boxes = body["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("mailboxes listed");
        // **Anonymous orientation drains nothing, so it sees no queue.** There
        // is no longer a second case: every box has a drainer, so a box is
        // either yours or somebody's, and this caller is nobody.
        assert_eq!(boxes[0]["name"], "inbox");
        assert_eq!(
            boxes[0]["yours"], false,
            "an anonymous caller drains nothing"
        );
        assert!(
            boxes[0]["counts"].is_null(),
            "…and somebody else's queue is not its to weigh: {:?}",
            boxes[0]
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
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(full["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(full["orientation_elided"], false);

        let brief = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: Some(true),
                    resume: None,
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

        // **How to get it back is on the surface a caller reads**, since the
        // payload no longer carries a nudge of its own — an elision nobody can
        // undo is an elision that costs the reader the thing it saved.
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
        assert!(brief["snapshot"]["mailboxes"].is_object());
    }

    /// **The orientation stamp is gone, whole.** It was a version on the essay
    /// so a returning session could tell whether the copy it held was current —
    /// and it was rejected outright, along with every proposed way of keeping
    /// the check honest (a prose hash, a derived version, a hand-maintained
    /// one). A number a human has to remember to bump is a number that lies,
    /// and what it bought did not pay for that.
    ///
    /// **Asserted over the whole payload and the whole surface**, not over the
    /// two keys that used to carry it: the failure this guards against is the
    /// idea coming back somewhere adjacent, and a key-by-key check would miss
    /// it in a note or an arg doc. `brief` survives, as a plain caller-chosen
    /// option with nothing to compare.
    #[tokio::test]
    async fn nothing_on_the_surface_stamps_the_orientation_with_a_version() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store);
        make_bot(&jojobot, "gamma").await;

        let answers = [
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        bot: None,
                        brief: None,
                        resume: None,
                    }))
                    .await
                    .expect("start_here ok"),
            ),
            json_of(
                &jojobot
                    .start_here(Parameters(OrientArgs {
                        bot: None,
                        brief: Some(true),
                        resume: None,
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
                    bot: Some("gamma".into()),
                    brief: Some(true),
                    resume: None,
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
        let out = handler_with_mailboxes_down(Arc::new(InMemoryMemory::new()))
            .start_here(Parameters(OrientArgs {
                bot: None,
                brief: None,
                resume: None,
            }))
            .await
            .expect("orientation still lands");
        let body: serde_json::Value = serde_json::from_str(&text_of(&out)).expect("json");
        assert!(body["orientation"].as_str().is_some_and(|o| !o.is_empty()));
        assert_eq!(body["snapshot"]["mailboxes"]["available"], false);
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
                bot: None,
                brief: None,
                resume: Some("new".into()),
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
                bot: Some("person:milhouse".into()),
                brief: None,
                resume: None,
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
                    bot: Some("gamm".into()),
                    brief: None,
                    resume: None,
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
                    bot: Some("nobody".into()),
                    brief: None,
                    resume: None,
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
                    sid: None,
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
                    bot: Some("gamma".into()),
                    brief: None,
                    resume: None,
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
}
