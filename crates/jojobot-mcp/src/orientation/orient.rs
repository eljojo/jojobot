//! **The one orientation, anonymous or identified.**
//!
//! Naming a bot adds the identity half to an answer that is otherwise the same
//! text and the same snapshot; it does not open a second way in. The one call
//! site is the point, and `there_is_exactly_one_orientation_verb` counts it.

use super::*;

impl Jojobot {
    /// The one orientation, anonymous or identified — **the one call site is
    /// the point.** Naming a bot adds the identity half to an answer that is
    /// otherwise the same text and the same snapshot; it does not open a second
    /// way in.
    pub(crate) async fn orient(
        &self,
        bot: Option<&EntityId>,
        brief: bool,
        resume: Option<&str>,
    ) -> Result<CallToolResult, McpError> {
        // The entity index is read ONCE for the whole answer. Three parts of
        // a boot need it — the counts by kind, which boxes the caller drains,
        // and the identity itself — and reading it three times would mean
        // three remote round trips per boot, and three reads that can
        // disagree with one another inside a single payload.
        //
        // Best-effort per world: orientation must land even when one world is
        // down — a fresh agent on a half-configured server still gets the map.
        let index = self.memory.list_entities(None).await;
        let entities = match &index {
            Ok(entities) => {
                let mut by_kind = std::collections::BTreeMap::<&str, usize>::new();
                for e in entities {
                    let kind = e.id.as_str().split(':').next().unwrap_or("unknown");
                    *by_kind.entry(kind).or_default() += 1;
                }
                // **The one kind that is NAMED and not merely counted**, because
                // it is the only one this door asks you to pick from. An
                // anonymous boot could see that five identities exist and could
                // not learn one to boot as — while the refusal for an unknown
                // bot lists every real one, so the only route to a usable name
                // was to guess a wrong one and read it off the complaint. The
                // door's own suggested next step was unreachable from the door.
                //
                // **Names only, and the refusal's own spelling.** Counts and
                // charters belong to a caller weighing its own work; this one
                // owns nothing and is choosing an identity. `bots`, full
                // handles, so one record has one shape wherever it appears.
                let mut bots: Vec<&str> = entities
                    .iter()
                    .filter(|e| e.kind == EntityKind::Bot)
                    .map(|e| e.id.as_str())
                    .collect();
                bots.sort_unstable();
                serde_json::json!({
                    "available": true,
                    "count": entities.len(),
                    "by_kind": by_kind,
                    "bots": bots,
                })
            }
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the memory world is not reachable right now — its tools will say why",
            }),
        };
        // A memory world that is down cannot answer who anybody is; the
        // snapshot below already says so, and this stays null rather than
        // claiming the identity is missing.
        let identity = match (bot, &index) {
            (None, _) | (_, Err(_)) => serde_json::Value::Null,
            (Some(bot), Ok(index)) => match self.identity(index, bot).await? {
                Ok(identity) => identity,
                // A name that is no bot: the guards' own shape, so one
                // client-side branch handles every "jojobot declined" answer —
                // but with the door's own body, not the generic absence one.
                Err(candidates) => {
                    return Ok(booting_unknown(bot, &candidates, index));
                }
            },
        };
        // **The mailbox world is read AFTER the identity, and that ordering is
        // load-bearing.** Resolving the identity is what heals this bot's box
        // when it is missing. Read the board first and one payload says, in the
        // snapshot, that this bot has no box, and says in the identity beside it
        // that the box was missing and has just been opened — two halves of one
        // answer disagreeing about the world, with nothing to tell a session
        // which to believe. It is still exactly ONE read; it just happens once
        // the repair this boot performs has landed.
        //
        // **The snapshot is scoped the same way the listing is.** It was the
        // other place a boot met per-state counts for every box on the server,
        // and it posed the same question the own-box norm then has to answer in
        // prose: is that unread one mine? An anonymous `start_here` owns
        // nothing, which is exactly right for a caller that only posts.
        let listed = self.mailboxes.list_mailboxes().await;
        let mailboxes = match listed {
            Ok(boxes) => {
                let mine = self.ownership_of(&boxes, bot);
                serde_json::json!({
                "available": true,
                "counts_shown_for": mine.shown_for(&boxes),
                "note": mine.note(),
                "boxes": boxes
                    .iter()
                    .map(|b| {
                        if mine.drains(b.name.as_str()) {
                            let mut body = mailbox_json(b);
                            if let Some(obj) = body.as_object_mut() {
                                obj.insert("yours".into(), true.into());
                            }
                            body
                        } else {
                            serde_json::json!({
                                "name": b.name.as_str(),
                                "yours": false,
                                "counts": serde_json::Value::Null,
                                "counts_elided": true,
                                // **Quarantine is not a count, and it does not
                                // ride out with them.** It is the only place an
                                // unreadable card's existence shows, and the
                                // caller who most needs it is a SENDER — who by
                                // definition does not drain this box, and would
                                // otherwise conclude their message was never
                                // sent. What is scoped away is somebody's
                                // queue, never a fault on the board.
                                "quarantined": quarantined_json(b),
                            })
                        }
                    })
                    .collect::<Vec<_>>(),
                })
            }
            Err(_) => serde_json::json!({
                "available": false,
                "note": "the mailbox world is not reachable right now — its tools will say why",
            }),
        };
        let snapshot = serde_json::json!({ "entities": entities, "mailboxes": mailboxes });
        // **Only after the identity resolved.** A name that is no bot boots
        // nothing, so it starts no session and sweeps nothing either — binding
        // a connection to an identity jojobot just refused would be a session
        // belonging to nobody.
        let session = match bot {
            None => serde_json::Value::Null,
            Some(bot) => match self.attach(bot, resume).await {
                Ok(session) => session,
                // A handle that addresses nothing stops the whole answer.
                // Handing back orientation around it would bury the one thing
                // the caller has to act on.
                Err(refused) => return Ok(refused),
            },
        };
        json_result(&serde_json::json!({
            "orientation": if brief { serde_json::Value::Null } else { essay::ORIENTATION.into() },
            // **The elision is marked, and that is all it is.** The essay used
            // to arrive stamped with a version so a returning session could ask
            // whether the copy it held was current; the stamp is gone, and no
            // staleness check replaces it. What is left is the marker every
            // elision on this surface owes — less came back, and the caller is
            // told so rather than left to infer withheld from empty.
            "orientation_elided": brief,
            "snapshot": snapshot,
            "identity": identity,
            "session": session,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::mailboxes::testing::*;
    use crate::memory::testing::*;
    use crate::session::testing::*;

    /// The two worlds are apart, and this is the test that says so. Ownership
    /// is stated on the box, so an unreadable entity index takes the
    /// charter, the rules and the roster with it, and leaves the mail
    /// scoping standing. If this assertion's polarity ever flips back, that
    /// means ownership is being read off the entity record again — a
    /// regression.
    #[tokio::test]
    async fn an_unreadable_entity_index_no_longer_hides_who_drains_what() {
        let memory = Arc::new(InMemoryMemory::new());
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let seeded = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            boxes.clone(),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_box(&seeded, "dev").await;
        send(&seeded, "dev", "delta", "your hand-off").await;

        let blind = Jojobot::new(
            Arc::new(UnindexedMemory(memory)),
            Arc::new(SpySearch::default()),
            boxes,
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        // Read through the boot snapshot, which is where the scoping lives:
        // this is the door that applies the ownership rule.
        let listed = boot(&blind, "dev").await["snapshot"]["mailboxes"].clone();

        assert_eq!(listed["boxes"][0]["yours"], true, "{listed}");
        assert_eq!(
            listed["boxes"][0]["counts"]["new"], 1,
            "the mail world knows whose box this is without asking Memory: {listed}"
        );
        assert_eq!(
            listed["counts_shown_for"],
            serde_json::json!(["dev"]),
            "{listed}"
        );
    }

    /// **A fault on the board is not somebody's queue, and it is not scoped
    /// away with one.** What jojobot cannot read as a message is counted
    /// nowhere and delivered nowhere, so the only caller who can act on knowing
    /// it exists is often a SENDER — somebody who by definition does not drain
    /// that box, and who would otherwise read the silence as "my message never
    /// arrived". So the counts are withheld from a box that is not yours and
    /// the unreadable report is not.
    ///
    /// This is the only answer that renders a box the caller does not drain:
    /// `read_mailbox`'s counting mode is about your own box by construction,
    /// and `list_sent` only reaches boxes you have posted into.
    #[tokio::test]
    async fn a_boot_shows_what_cannot_be_read_even_on_a_box_it_will_not_count() {
        let boxes = Arc::new(InMemoryMailboxes::knowing_any_owner());
        let jojobot = with_mailboxes(boxes.clone());
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        boxes.quarantine(
            &MailboxName("delta".into()),
            &MessageId("4212".into()),
            "its row cannot be read — a state or a sender has been edited past parsing",
        );

        let booted = boot(&jojobot, "gamma").await;
        let theirs = booted["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .find(|b| b["name"] == "delta")
            .expect("delta's box")
            .clone();
        assert_eq!(theirs["yours"], false);
        assert!(
            theirs["counts"].is_null(),
            "somebody else's queue stays theirs: {theirs}"
        );
        assert_eq!(
            theirs["quarantined"]["count"], 1,
            "…and the fault on it does not: {booted}"
        );
        assert_eq!(theirs["quarantined"]["ids"][0], "4212");
    }

    /// A boot sees its own box's counts in the snapshot, and names only for the
    /// rest — the same rule, in the other place a session meets this listing.
    #[tokio::test]
    async fn a_boot_snapshot_counts_only_the_bot_s_own_box() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        send(&jojobot, "gamma", "delta", "your hand-off").await;
        send(&jojobot, "delta", "sigma", "not your business").await;

        let booted = boot(&jojobot, "gamma").await;
        let boxes = booted["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("boxes")
            .clone();
        let find = |name: &str| {
            boxes
                .iter()
                .find(|b| b["name"] == name)
                .expect("the box")
                .clone()
        };

        assert_eq!(
            find("gamma")["counts"]["new"],
            1,
            "my box, counted: {booted}"
        );
        assert_eq!(find("gamma")["yours"], true);
        assert!(
            find("delta")["counts"].is_null(),
            "somebody else's, name only: {booted}"
        );
        assert_eq!(find("delta")["yours"], false);
        // No `ownership_known` flag: it could only ever say `true` where it
        // appeared, since it would be rendered inside the `Ok` arm of the
        // very read whose `Err` arm is the only thing that would set it
        // false. A field that cannot vary is a question a reader branches on
        // and learns nothing from.
        assert!(
            booted["snapshot"]["mailboxes"]
                .get("ownership_known")
                .is_none(),
            "a flag that cannot be false is not an answer: {booted}"
        );

        // The bot's own box still comes back in full under `identity`, which is
        // the whole point of booting as somebody.
        assert_eq!(booted["identity"]["owned_mailbox"]["counts"]["new"], 1);
    }

    /// **The door's own next step was unreachable from the door.**
    ///
    /// An anonymous boot counted the bots and named none of them, so a caller
    /// with no identity — which is exactly the caller this answer is for — could
    /// see that five identities exist and could not pick one. Meanwhile the
    /// refusal for an unknown bot lists every real one. So the only way to learn
    /// a name you could boot with was to guess a name that does not exist and
    /// read it off the complaint, and a context-free runner did precisely that.
    ///
    /// The surface was inconsistent with itself rather than minimal. Naming them
    /// here adds no tool and no second call to the first call anybody makes.
    ///
    /// **Names only.** Not counts, not charters: an anonymous caller owns
    /// nothing and is weighing nothing, and the same spelling the refusal uses
    /// (`bots`, full handles) so one record has one shape across the surface.
    #[tokio::test]
    async fn the_snapshot_names_the_bots_a_caller_could_boot_as() {
        let jojobot = mailbox_handler();
        make_bot(&jojobot, "gamma").await;
        make_bot(&jojobot, "delta").await;
        ensure(&jojobot, "alpha").await;

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let entities = &anonymous["snapshot"]["entities"];
        assert_eq!(
            entities["bots"],
            serde_json::json!(["bot:delta", "bot:gamma"]),
            "the door has to name what it counts: {anonymous}"
        );
        // A person is not an identity you can boot as, so the roster is not
        // simply the index with a different key on it.
        assert_eq!(entities["by_kind"]["person"], 1, "{anonymous}");

        // **Named, never weighed.** The counts belong to whoever drains the
        // box; an anonymous caller drains none and is choosing an identity, not
        // grading one.
        let listed = entities["bots"].as_array().expect("a roster");
        assert!(
            listed.iter().all(serde_json::Value::is_string),
            "names, not records carrying counts or charters: {anonymous}"
        );

        // …and the name it hands over actually boots, which is the whole claim.
        let booted = boot(&jojobot, "delta").await;
        assert_eq!(booted["identity"]["bot"]["id"], "bot:delta", "{booted}");
    }

    /// **One response never contradicts itself about which boxes exist.**
    ///
    /// It could before: booting minted the declared box *between* taking the
    /// snapshot and reporting the identity, so a single payload said in one
    /// half that no such box was on the board and in the other that it was
    /// there with counts — and a session had no way to tell which half to
    /// believe. Both halves are reads of the same world now; this holds them to
    /// agreeing.
    ///
    /// There is no such thing as a bot whose box nobody has opened — a box
    /// opens with its owner — so the reachable disagreement is the opposite
    /// one: an identity naming a box the snapshot beside it does not list.
    #[tokio::test]
    async fn a_boot_never_disagrees_with_its_own_snapshot_about_a_box() {
        let jojobot = handler();
        make_bot(&jojobot, "sigma").await;
        make_bot(&jojobot, "delta").await;

        let booted = boot(&jojobot, "sigma").await;
        let named = booted["identity"]["owned_mailbox"]["name"]
            .as_str()
            .expect("the identity names its box")
            .to_string();
        let on_the_board: Vec<&str> = booted["snapshot"]["mailboxes"]["boxes"]
            .as_array()
            .expect("boxes")
            .iter()
            .map(|b| b["name"].as_str().expect("a name"))
            .collect();
        assert!(
            on_the_board.contains(&named.as_str()),
            "the identity named {named:?}, and the snapshot beside it does not list it: {booted}"
        );
        // …and the other bot's box is on the same board, so this is not passing
        // because the board holds exactly one thing.
        assert!(on_the_board.contains(&"delta"), "{booted}");
    }

    /// **One orientation, one door.** Naming a bot is `start_here` plus an
    /// identity — not a second world-model to drift out of step with the first.
    #[tokio::test]
    async fn a_named_boot_and_an_anonymous_one_hand_over_the_same_world() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        let identified = boot(&jojobot, "gamma").await;
        assert_eq!(
            anonymous["orientation"], identified["orientation"],
            "the world-model is one text, or the two doors teach different jojobots"
        );
        assert_eq!(
            anonymous["snapshot"]["entities"], identified["snapshot"]["entities"],
            "what exists is one answer, whoever asks"
        );
        // The mailbox half is deliberately NOT equal once a bot drains a
        // box — that is the whole point of scoping counts to the caller — so
        // the shared invariant is the set of boxes, not their contents. Make
        // sure the fixture actually exercises this (give gamma a mailbox), or
        // a full-equality assertion could pass for a reason that has nothing
        // to do with the invariant being claimed.
        let names = |body: &serde_json::Value| -> Vec<String> {
            body["snapshot"]["mailboxes"]["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .map(|b| b["name"].as_str().expect("a name").to_string())
                .collect()
        };
        assert_eq!(
            names(&anonymous),
            names(&identified),
            "both doors see the same board; they differ only in whose queue is theirs to read"
        );
        assert!(
            anonymous["identity"].is_null(),
            "an anonymous session claims no identity"
        );
    }

    /// …and the difference the previous test carves out, asserted directly: the
    /// booted door counts the box its identity drains, the anonymous one does
    /// not.
    #[tokio::test]
    async fn the_two_doors_differ_only_in_whose_queue_is_theirs_to_read() {
        let jojobot = handler();
        make_box(&jojobot, "dev").await;
        send(&jojobot, "dev", "delta", "your hand-off").await;

        let counts_for = |body: &serde_json::Value| -> serde_json::Value {
            body["snapshot"]["mailboxes"]["boxes"]
                .as_array()
                .expect("boxes")
                .iter()
                .find(|b| b["name"] == "dev")
                .expect("the box")
                .clone()
        };

        let anonymous = json_of(
            &jojobot
                .start_here(Parameters(OrientArgs {
                    bot: None,
                    brief: None,
                    resume: None,
                }))
                .await
                .expect("start_here ok"),
        );
        assert!(counts_for(&anonymous)["counts"].is_null(), "{anonymous}");
        assert_eq!(counts_for(&anonymous)["yours"], false);
        // Elided, never silently — the same rule the whole surface keeps: a
        // reader must not have to infer withheld from empty.
        assert_eq!(counts_for(&anonymous)["counts_elided"], true, "{anonymous}");
        assert_eq!(
            anonymous["snapshot"]["mailboxes"]["counts_shown_for"],
            serde_json::json!([]),
            "…and the answer names what it counted, which is nothing: {anonymous}"
        );

        let identified = boot(&jojobot, "dev").await;
        assert_eq!(counts_for(&identified)["counts"]["new"], 1, "{identified}");
        assert_eq!(counts_for(&identified)["yours"], true);
    }

    /// **Both halves of the door make the same promise, so both keep it.** `orient` says
    /// orientation lands even when a world is down — and `start_here` did,
    /// while the identified half hard-errored the moment a bot owned a box, which made
    /// every box-owning identity unbootable over an outage in the *other*
    /// world. The charter and the rules are in Memory and were right there.
    ///
    /// Now the mailbox half degrades on its own, the same way the snapshot's
    /// does: the boot lands, the identity is whole, and the one thing jojobot
    /// cannot answer says so instead of guessing.
    #[tokio::test]
    async fn a_boot_survives_a_world_that_is_down_exactly_as_an_anonymous_one_does() {
        // Stood up while both worlds are up — a claim that cannot be screened
        // is refused, so this bot could not have been created below.
        let memory = Arc::new(InMemoryMemory::new());
        let healthy = Jojobot::new(
            memory.clone(),
            Arc::new(SpySearch::default()),
            Arc::new(InMemoryMailboxes::knowing_any_owner()),
            Arc::new(InMemorySessions::new()),
            Arc::new(sid::SessionRegistry::new()),
        );
        make_bot(&healthy, "gamma").await;
        healthy
            .set_charter(Parameters(SetCharterArgs {
                bot: "gamma".into(),
                prose: "Holds the plan.".into(),
                sid: None,
            }))
            .await
            .expect("set_charter ok");

        let jojobot = handler_with_mailboxes_down(memory);
        let body = boot(&jojobot, "gamma").await;
        assert_ne!(body["status"], "blocked", "a boot must still land: {body}");

        let me = &body["identity"];
        assert_eq!(me["bot"]["id"], "bot:gamma");
        assert_eq!(
            me["charter"], "Holds the plan.",
            "the half that is up arrives whole"
        );

        // Ownership is stated on the box, not the bot's own record, so an
        // unreachable mailbox world means jojobot cannot say which box is
        // yours, or whether you have one — and it must say exactly that
        // instead of naming a box it cannot see.
        let owned = &me["owned_mailbox"];
        assert_eq!(owned["available"], false, "got {owned}");
        assert!(
            owned["name"].is_null(),
            "a box it cannot read is not a box it can name: {owned}"
        );
        // **The FIELD stays null and the NOTE stops pretending.** Existence is
        // what an outage hides; the name is derived from the handle the caller
        // is already holding, and reporting it as equally unknown made a
        // mystery of the half jojobot can work out from first principles. The
        // note says which is which — expect this name, do not conclude it is
        // there.
        let note = owned["note"].as_str().expect("a note");
        assert!(
            note.contains("'gamma'"),
            "the derivable half is named: {note}"
        );
        assert!(
            note.contains("not as confirmation"),
            "…and is fenced off from the half that is not: {note}"
        );

        // …and the snapshot degrades beside it, exactly as it does anonymously.
        assert_eq!(body["snapshot"]["mailboxes"]["available"], false);
    }
}
