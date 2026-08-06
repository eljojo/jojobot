//! **Who this session is** — the bot's record, its charter, its rules, and the
//! live state of the box it owns. Plus the door's own refusal for a name that
//! is no bot.
//!
//! This is also the ONE place jojobot heals a missing box: a bot's box is part
//! of what a bot is, so its absence is damage rather than a setup step, and
//! boot is the only moment where repairing it needs no judgement.

use super::*;

/// The boot door's own refusal: the roster, and an offer.
///
/// This must not reuse the generic absence gate ("nothing resembles it, call
/// add_entity first"): its candidate list is near misses only, so a name that
/// resembles nothing would come back with an empty list — reading as a broken
/// server rather than "you are not one of these" — and its advice would send
/// a caller with no identity to a verb that needs a session it does not have.
///
/// So this says what a caller in that position actually needs: here is who
/// exists, boot as one of them, and create the identity you wanted from inside
/// that session. **The door itself mints nothing** — creation is an intentional
/// act, and it happens through the verb that is for it, from a session that can
/// answer for it.
pub(crate) fn booting_unknown(
    attempted: &EntityId,
    candidates: &[EntityMatch],
    index: &[Entity],
) -> CallToolResult {
    let roster: Vec<&str> = index
        .iter()
        .filter(|e| e.id.as_str().starts_with("bot:"))
        .map(|e| e.id.as_str())
        .collect();
    let how_to_proceed = if roster.is_empty() {
        format!(
            "Nothing was written and no session was started. '{attempted}' is not a bot jojobot \
             knows, and there are no bots on this server at all yet. Call start_here with no bot \
             for the world and the snapshot, then add_entity with kind `bot` to create the first \
             identity — this door mints nothing."
        )
    } else {
        format!(
            "Nothing was written and no session was started. '{attempted}' is not a bot jojobot \
             knows. The identities that exist are: {}. Boot as one of these and create \
             '{attempted}' from inside that session — this door mints nothing.",
            roster.join(", "),
        )
    };
    let body = serde_json::json!({
        "status": "blocked",
        "attempted": attempted.as_str(),
        "wrote": false,
        // **The roster, not only the near misses.** `candidates` answers "did
        // you mean one of these"; it is empty whenever nothing resembles the
        // name, and that is exactly the caller who most needs to be told who
        // does exist.
        "bots": roster,
        "candidates": candidates.iter().map(candidate_json).collect::<Vec<_>>(),
        "how_to_proceed": how_to_proceed,
    });
    CallToolResult::success(vec![ContentBlock::text(body.to_string())])
}

impl Jojobot {
    /// Who this session is: the bot's record, the charter its prose carries,
    /// the rules its facts carry, and the live state of the box it owns.
    /// `Err(candidates)` is the guards' answer for a name that is no bot.
    ///
    /// **A caller answering the resume-or-new offer gets no charter.** That
    /// offer is only ever handed back by a boot that carried the charter, so
    /// the one reader this block is re-sent to is the reader that has it — and
    /// it is the largest fixed block in the answer, crowding out the session
    /// record a resume exists to deliver. The rules are not treated the same
    /// way: they are dated claims that change one at a time.
    pub(crate) async fn identity(
        &self,
        index: &[Entity],
        bot: &EntityId,
        answering_an_offer: bool,
    ) -> Result<Result<serde_json::Value, Vec<EntityMatch>>, McpError> {
        let Some(entity) = index.iter().find(|e| &e.id == bot) else {
            return Ok(Err(guard::screen(bot, &[], index)));
        };

        // The charter is the doc's prose; a bot nobody has written one for has
        // none, and null says so rather than an empty string pretending to be
        // an answer. It is not read at all when it is not being shipped.
        let charter = match answering_an_offer {
            true => None,
            false => self
                .memory
                .scan_entity(bot)
                .await
                .map_err(memory_error)?
                .map(|doc| doc.prose)
                .filter(|p| !p.trim().is_empty()),
        };
        let rules = self.memory.recall(bot).await.map_err(memory_error)?;

        let mut body = serde_json::json!({
            "bot": entity_json(entity),
            "charter": charter,
            // **Marked, because `null` here already means something else**: a
            // bot nobody has written a charter for. A reader left to tell
            // withheld from absent would report the absence.
            "charter_elided": answering_an_offer,
            "rules": rules.iter().map(fact_json).collect::<Vec<_>>(),
            "owned_mailbox": self.owned_mailbox(&entity.id).await?,
        });
        if answering_an_offer && let Some(obj) = body.as_object_mut() {
            obj.insert(
                "note".into(),
                "your charter is not in this answer. It is unchanged, and you are answering an \
                 offer only a boot that carried it can make, so it is text you already hold. To \
                 read it again, call start_here with your bot name and no `resume`: that boot \
                 writes nothing and starts nothing."
                    .into(),
            );
        }
        Ok(Ok(body))
    }

    /// The live state of the box a bot owns — **and the one place jojobot heals
    /// one that is missing.**
    ///
    /// A box opens with its bot, so a bot whose box is absent is damage: an
    /// `add_entity` that wrote the identity and then failed to open the box, or
    /// a record predating the rule. The operator's ruling is that jojobot fixes
    /// it rather than filing it — *"the system should auto heal next time when
    /// it notices it's not there but it should. and notify the agent that the
    /// message/box wasn't created."*
    ///
    /// **This is not rule 18 being bent.** The intentional act already happened:
    /// somebody stood up the bot, and a bot's box is part of what a bot IS
    /// rather than a second thing they forgot to ask for. Healing completes an
    /// interrupted act; it mints nothing nobody asked for. The repair is only
    /// legitimate because it needs no judgement — the owner is in hand and the
    /// name is derived from it, so there is exactly one correct box.
    ///
    /// **Boot is the only place that heals**, and that is a deliberate limit.
    /// Counting a box and scoping a listing are pure reads of the board, and
    /// healing there would make every read a potential write. `post_message`
    /// names somebody *else's* box: writing another identity's infrastructure is
    /// not this caller's act, and the owner heals it the moment it boots — which
    /// is the next time anyone would drain it anyway. A message is not more
    /// delivered for a box existing that nobody has booted to read.
    pub(crate) async fn owned_mailbox(
        &self,
        bot: &EntityId,
    ) -> Result<serde_json::Value, McpError> {
        // The mailbox half degrades on its own, exactly as the snapshot's does.
        // Hard-erroring here made every box-owning identity unbootable over an
        // outage in the *other* world — while its charter and its rules, the
        // things a session most needs, were sitting right there in Memory.
        let boxes = match self.mailboxes.list_mailboxes().await {
            Ok(boxes) => boxes,
            Err(_) => {
                // Not `null`, which is the answer for a bot that owns no box:
                // jojobot does not know whether it owns one, and saying it does
                // not would be a guess a session would act on.
                // **What is unknowable is EXISTENCE, not the name.** A box is
                // named for the bot that owns it, by construction — `add_entity`
                // opens it that way and the heal repairs it that way — so a
                // caller left with "jojobot cannot say which box is yours" was
                // being told a mystery about the one half that is derivable
                // from what it already holds. What jojobot genuinely cannot say
                // is whether that box is there and what is in it, and the
                // difference matters to a caller deciding whether to wait or to
                // report damage.
                //
                // `name` stays null deliberately. Filling it in would be
                // asserting a box exists that jojobot cannot see, and the store
                // predates the name-is-the-handle rule — a box owned under some
                // other name is a thing history can hold. The note says what is
                // derivable; the field goes on saying only what is read.
                return Ok(serde_json::json!({
                    "available": false,
                    "note": format!(
                        "the mailbox world is not reachable right now, so jojobot cannot say \
                         whether your box exists or what is waiting in it — its tools will say \
                         why. What is not in doubt is its NAME: a box is named for the bot that \
                         owns it, so yours is '{}'. Treat that as the name to expect, not as \
                         confirmation it is there.",
                        bot.slug()
                    ),
                }));
            }
        };

        // **A lookup by owner, not a claim read.** A box is created for
        // somebody, so ownership is stated once on the box itself — there is no
        // second field on the bot's record to keep in step with it.
        //
        // A box this bot owns is a box that exists — ownership is stated on
        // the box, so there is no separate "claimed but not yet created" state
        // to handle, and no `exists` field either: `available` is the only
        // question a reader still has to branch on.
        let Some(mailbox) = boxes.into_iter().find(|b| &b.owner == bot) else {
            return Ok(self.heal_missing_box(bot).await);
        };
        let mut body = mailbox_json(&mailbox);
        if let Some(obj) = body.as_object_mut() {
            obj.insert("available".into(), true.into());
        }
        Ok(body)
    }

    /// Open the box this bot should have had, and **say that it was missing.**
    ///
    /// The notification is half the ruling, not a courtesy. A silent repair is
    /// the thing this codebase forbids everywhere else — eliding is never
    /// silent — and a session that cannot tell "your box is here" from "your box
    /// was gone and is here now" cannot report the damage to anybody who could
    /// ask why it happened.
    ///
    /// **One attempt, never a loop.** If the repair does not land, that is said
    /// too, and the boot still returns whole: the charter and the rules are in
    /// the other world entirely and are what a session most needs.
    pub(crate) async fn heal_missing_box(&self, bot: &EntityId) -> serde_json::Value {
        /// The repair did not land and the box is genuinely not there. Damage,
        /// and it takes a person — so it says so, and says what still works.
        fn still_missing(name: &MailboxName, said: Option<&str>) -> serde_json::Value {
            serde_json::json!({
                "available": true,
                "name": serde_json::Value::Null,
                "healed": false,
                "note": format!(
                    "YOUR BOX '{name}' is missing and jojobot could not open it. You have an \
                     identity with no way to receive mail, and this is damage rather than a \
                     setup step: a box opens with its bot. Nothing you post is affected — \
                     post_message needs no box of your own. Tell the operator.{}",
                    // **The failure's own words do not ride into a boot.** This
                    // is the first thing a session reads, and the mailbox
                    // world's account of what went wrong names pages and
                    // tables — logged instead, see [`crate::boundary`].
                    said.map(|s| {
                        format!(" {}", crate::boundary::store_failed("opening it", s))
                    })
                    .unwrap_or_default()
                ),
            })
        }

        let name = MailboxName(bot.slug().to_string());
        match self.mailboxes.create_mailbox(&name, bot, true).await {
            Ok(mailbox::Guarded::Written(opened)) => {
                let mut body = mailbox_json(&opened);
                if let Some(obj) = body.as_object_mut() {
                    obj.insert("available".into(), true.into());
                    obj.insert("healed".into(), true.into());
                    obj.insert(
                        "note".into(),
                        format!(
                            "YOUR BOX '{name}' was missing and jojobot has just opened it. A box \
                         opens with its bot, so its absence means that creation was \
                         interrupted — the identity existed with no way to receive mail. It \
                         is repaired and this boot is otherwise normal, but anything posted \
                         to you before now was refused as an unknown box and was never \
                         stored: tell the operator, since only they can say what was lost."
                        )
                        .into(),
                    );
                }
                body
            }
            // **Blocked on an exact name means somebody else just opened it.**
            // Two boots of one bot can both find no box and both come here; the
            // loser's create meets its own box and is refused, because an exact
            // name can never be forced. Reading that as "the repair failed"
            // told a session it had no way to receive mail about a box that was
            // working, and sent it to a person over nothing. The box is right
            // there — report it, with its counts, and say who opened it.
            Ok(mailbox::Guarded::Blocked { .. }) => match self.mailbox_named(&name).await {
                Some(mailbox) => {
                    let mut body = mailbox_json(&mailbox);
                    if let Some(obj) = body.as_object_mut() {
                        obj.insert("available".into(), true.into());
                        // Not `healed`, because this call did not open it — and
                        // not silent either, because the box WAS missing when
                        // this boot looked and a session that saw the gap is
                        // owed the end of the story.
                        obj.insert("healed".into(), false.into());
                        obj.insert(
                            "note".into(),
                            format!(
                                "YOUR BOX '{name}' was missing when this boot looked and is here \
                                 now: another run of this same bot opened it while this one was \
                                 starting. Nothing is wrong and there is nothing to report — \
                                 anything posted to you before it existed was refused as an \
                                 unknown box, and that window has closed."
                            )
                            .into(),
                        );
                    }
                    body
                }
                // Blocked, and still not there: a near miss on the name rather
                // than the box itself, which is a real failure and gets the
                // same answer every other one gets.
                None => still_missing(&name, None),
            },
            // **The world answered and the box still is not there**, which is a
            // different thing from the world being unreachable — so `available`
            // stays true and `name` is null rather than claiming a box.
            Err(err) => still_missing(&name, Some(&err.to_string())),
            _ => still_missing(&name, None),
        }
    }

    /// The box with this name, or `None` — including when jojobot cannot read
    /// the board at all, since for this caller "not there" and "cannot tell"
    /// lead to the same next move.
    async fn mailbox_named(&self, name: &MailboxName) -> Option<Mailbox> {
        self.mailboxes
            .list_mailboxes()
            .await
            .ok()?
            .into_iter()
            .find(|b| &b.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;

    /// The boot that loses a race to heal must not tell its agent the box is
    /// missing — it is right there, opened a moment ago by the other one. The
    /// window is real: `owned_mailbox` finds no box and calls this, and
    /// between those two moments another boot of the SAME bot can open it.
    /// The loser's `create_mailbox` then meets an exact-name collision, which
    /// `guard::decide_create` blocks whatever `create_new` says — an exact
    /// name can never be forced. A non-`Written` answer must be checked for
    /// that collision before it is read as "the repair failed".
    ///
    /// Reproduced without a race, because the race is only how you arrive
    /// here: the state under test is "this call was going to open the box and
    /// the box now exists", and calling the heal against a box that is
    /// already there IS that state — deterministically, with no timing to
    /// get lucky with.
    #[tokio::test]
    async fn a_heal_that_lost_the_race_reports_the_box_rather_than_its_absence() {
        let jojobot = handler();
        // The other boot's work: the bot and its box are both already there.
        make_bot(&jojobot, "gamma").await;

        let healed = jojobot
            .heal_missing_box(&EntityId::new(EntityKind::Bot, "gamma"))
            .await;

        assert_eq!(healed["available"], true, "{healed}");
        assert_eq!(
            healed["name"], "gamma",
            "the box exists and must be named, not nulled: {healed}"
        );
        assert_eq!(
            healed["healed"], false,
            "this call did not open it — the other boot did: {healed}"
        );
        let note = healed["note"].as_str().expect("a note");
        assert!(
            !note.contains("no way to receive mail"),
            "the one thing that must never be said about a box that is working: {note}"
        );
        assert!(
            !note.contains("Tell the operator"),
            "…and nobody is sent to a person over a repair that succeeded: {note}"
        );
        // The counts come with it, exactly as they do on any other boot: this is
        // the box, not a placeholder standing in for one.
        assert_eq!(healed["counts"]["total"], 0, "{healed}");

        // And nothing was minted a second time.
        let boxes = jojobot.mailboxes.list_mailboxes().await.expect("list ok");
        assert_eq!(boxes.len(), 1, "{boxes:?}");
    }

    /// **jojobot heals the box it notices is missing, and says so out loud.**
    ///
    /// The operator's ruling: *"the system should auto heal next time when it
    /// notices it's not there but it should. and notify the agent that the
    /// message/box wasn't created."* Two halves, and the second is the one that
    /// is easy to drop — a silent repair is the class this codebase forbids
    /// everywhere else, because a caller who has to infer "this was fixed" from
    /// the absence of a complaint will eventually infer wrong.
    #[tokio::test]
    async fn booting_a_bot_whose_box_is_missing_opens_it_and_says_so() {
        let jojobot = handler();
        broken_bot(&jojobot, "gamma").await;

        let owned = boot(&jojobot, "gamma").await["identity"]["owned_mailbox"].clone();
        assert_eq!(owned["name"], "gamma", "the box is there now: {owned}");
        assert_eq!(owned["available"], true);
        assert_eq!(
            owned["healed"], true,
            "…and the repair is on the record, not silent: {owned}"
        );
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("was missing"),
            "the note says what was wrong, not just that something happened: {note}"
        );

        // It is a real box on the board, not a rendering.
        let boxes = jojobot.mailboxes.list_mailboxes().await.expect("list ok");
        assert_eq!(boxes.len(), 1, "{boxes:?}");
        assert_eq!(boxes[0].owner, EntityId::new(EntityKind::Bot, "gamma"));

        // …and the second boot is quiet, because there is nothing left to fix.
        let again = boot(&jojobot, "gamma").await["identity"]["owned_mailbox"].clone();
        assert!(
            again["healed"].is_null(),
            "a heal is news exactly once: {again}"
        );
    }

    /// **The heal never conjures a box for a bot that does not exist.** An
    /// unknown name is answered with the roster, and nothing is written — the
    /// heal must not turn a typo'd boot into a new identity's infrastructure.
    #[tokio::test]
    async fn healing_opens_nothing_for_a_bot_that_does_not_exist() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamm").await;
        assert_eq!(
            body["status"], "blocked",
            "an unknown bot is refused: {body}"
        );
        let names: Vec<String> = jojobot
            .mailboxes
            .list_mailboxes()
            .await
            .expect("list ok")
            .iter()
            .map(|b| b.name.as_str().to_string())
            .collect();
        assert_eq!(
            names,
            ["gamma"],
            "no box was conjured for a name nobody owns"
        );
    }

    /// **The heal never conjures a box for something that is not a bot.** A
    /// person is not an addressee, and the door that heals only ever holds a
    /// bot — this pins that rather than trusting it.
    #[tokio::test]
    async fn healing_opens_nothing_for_an_entity_that_is_not_a_bot() {
        let jojobot = handler();
        jojobot
            .add_entity(Parameters(add_args("person", "milhouse", "Milhouse")))
            .await
            .expect("add_entity call ok");

        // The one door refuses a non-bot handle outright, so the heal is never
        // reached with one…
        let err = jojobot
            .start_here(Parameters(OrientArgs {
                bot: Some("person:milhouse".into()),
                brief: None,
                skill: None,
                resume: None,
            }))
            .await
            .expect_err("this door boots bots");
        assert_eq!(err.code, ErrorCode::INVALID_PARAMS);

        // …and nothing was opened on the way to finding that out.
        assert!(
            jojobot
                .mailboxes
                .list_mailboxes()
                .await
                .expect("list ok")
                .is_empty(),
            "a person is not an addressee and never gets a box"
        );
    }

    /// **A resume does not re-ship the charter.** The caller answering the
    /// resume-or-new offer read it on the call that made the offer, so sending
    /// it again spends a large, unchanging block of the answer on the one reader
    /// that demonstrably has it — while the parts a resume exists for are what
    /// the payload then has no room for.
    ///
    /// **And it is marked, because `charter: null` already means something
    /// else**: a bot nobody has written one for. A reader that cannot tell
    /// "withheld" from "there is none" would report the second.
    #[tokio::test]
    async fn a_resumed_boot_does_not_re_ship_the_charter() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");
        store
            .begin(NewSession {
                bot: EntityId("bot:gamma".into()),
                sid: fixture_sid(line!()),
                focus: "reading the hand-off".into(),
                started_at: jiff::Timestamp::now(),
            })
            .await
            .expect("begin ok");

        // The boot that makes the offer carries the charter whole — the
        // positive the elision below depends on.
        let offering = boot(&jojobot, "gamma").await;
        assert_eq!(offering["identity"]["charter"], "Holds the plan.");
        assert_eq!(
            offering["identity"]["charter_elided"], false,
            "an unanswered boot ships it, and says it did: {offering}"
        );
        let offer = offering["session"]["choices"][0]["sid"]
            .as_str()
            .expect("one option")
            .to_string();

        let resumed = boot_answering(&jojobot, "gamma", &offer).await;
        assert_eq!(resumed["session"]["resumed"], true, "{resumed}");
        assert!(
            resumed["identity"]["charter"].is_null(),
            "the charter is what a resume drops: {resumed}"
        );
        assert_eq!(
            resumed["identity"]["charter_elided"], true,
            "…and it says so, because null already means a bot with no charter: {resumed}"
        );
        let note = resumed["identity"]["note"]
            .as_str()
            .expect("an elision says how to undo itself");
        assert!(
            note.contains("resume"),
            "the way back is the boot that does not answer an offer: {note}"
        );

        // Everything a resume is for survives it.
        assert_eq!(resumed["identity"]["bot"]["id"], "bot:gamma");
        assert!(
            resumed["identity"]["rules"].is_array(),
            "the rules are dated claims and each can change: {resumed}"
        );
        assert_eq!(resumed["identity"]["owned_mailbox"]["name"], "gamma");
    }

    /// **`new` is an answer to the same offer, so it drops the charter too.**
    /// A `resume` of either shape can only be a reply to a boot that carried
    /// the charter — there is nothing else that hands out the sid it names, or
    /// the word `new`.
    #[tokio::test]
    async fn answering_the_offer_with_new_drops_the_charter_the_same_way() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: Some(crate::harness::TEST_SID.into()),
            }))
            .await
            .expect("set_charter ok");

        let fresh = boot_answering(&jojobot, "gamma", sid::NEW).await;
        assert!(fresh["identity"]["charter"].is_null(), "{fresh}");
        assert_eq!(fresh["identity"]["charter_elided"], true, "{fresh}");
    }

    /// **A heal that cannot land is reported honestly, and is not retried into a
    /// loop.** The boot still lands: the charter and the rules are the things a
    /// session most needs and they are in the other world entirely.
    #[tokio::test]
    async fn a_heal_that_fails_is_reported_rather_than_spun_on() {
        let memory = Arc::new(InMemoryMemory::new());
        let jojobot = Jojobot::new(
            memory,
            Arc::new(SpySearch::default()),
            Arc::new(UnopenableMailboxes(InMemoryMailboxes::knowing_any_owner())),
            Arc::new(InMemorySessions::new()),
            crate::harness::seeded_registry(),
        );
        broken_bot(&jojobot, "gamma").await;

        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "the boot still lands: {body}");
        assert_eq!(body["identity"]["bot"]["id"], "bot:gamma");

        let owned = &body["identity"]["owned_mailbox"];
        assert_eq!(
            owned["healed"], false,
            "the repair was attempted and did not land, and says so: {owned}"
        );
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("could not"),
            "an honest failure, not a silent absence: {note}"
        );
    }
}
