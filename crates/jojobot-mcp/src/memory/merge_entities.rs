//! `merge_entities` — Merge a duplicate into the thing it duplicates.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;
use crate::teaching::{CLAIMS_DOMAIN, CLAIMS_TEACHING};

/// Arguments to `merge_entities`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct MergeArgs {
    /// **The handle that stops being a thing of its own** — the duplicate.
    /// Everything it holds moves to the survivor, and the handle itself goes on
    /// resolving.
    pub(crate) duplicate: String,
    /// **The handle that survives.** Everything ends up here.
    pub(crate) survivor: String,
    /// Why the two are one thing, in one line. Optional — worth giving: this is
    /// the one call that destroys structure, and an account of why is what
    /// makes it readable a year later. Left out, the record says plainly that
    /// no reason was given rather than inventing one.
    #[serde(default)]
    pub(crate) reason: Option<String>,
    /// **The day this merge was made**, `YYYY-MM-DD`. Defaults to today in
    /// your session's zone.
    #[serde(default)]
    pub recorded_at: Option<String>,
    /// **Your session id**, exactly as the boot door returned it.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Merge one thing into another, and record that it happened.
#[tool_router(router = merge_entities_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Merge a duplicate into the thing it duplicates — the repair for one thing \
                       named twice. A deliberate act rather than a flag on an edit, because it is \
                       the one call here that destroys structure. Everything the duplicate held \
                       moves to the survivor: its claims, the edges drawn at it, the history \
                       under its keys, and its names, which become aliases of the survivor. \
                       Beside them lands a dated record of the merge itself, on the survivor, \
                       naming the handle that went and the reason if you give one. \
                       NOTHING IS REMOVED: the duplicate's handle keeps answering, so a handle \
                       written down anywhere still resolves — but a recall of it comes back with \
                       status merged, names the survivor in merged_into and holds nothing of \
                       its own. It does not answer as the survivor: recall the survivor to read \
                       everything. It simply stops being a thing of its own. USE IT WHEN TWO \
                       HANDLES ARE ONE THING. It does not ask whether they really are: that is your \
                       judgement, so putting two unrelated things together is allowed and is \
                       recorded exactly as legibly. There is no way back, so if you are unsure, \
                       read both with recall first. ⚠️ THE CLAIMS THAT MOVE GET NEW ADDRESSES, \
                       because an address is local to the thing that holds it — any address you \
                       were holding for the duplicate's claims is stale afterwards, and the \
                       answer says how many moved. A MERGE INTO YOUR OWN BOT IS REFUSED WHEN THE DUPLICATE CARRIES \
                       thought_capacity OR thought_body_cap, because the merge would raise your \
                       own ceiling: have a different identity merge it, or take the key off the \
                       duplicate first with update_fact and clear_fields. THE DUPLICATE'S \
                       THOUGHTS BECOME THE SURVIVOR'S, so a merge that would leave a room over \
                       its capacity (every active thought counts, none ages out), or bring a \
                       thought over its body cap, is refused with \
                       nothing moved: archive thoughts, or shorten the one named, with \
                       update_fact, then merge again. Naming one handle as \
                       both sides comes back status: blocked. So does naming a handle that was already merged away, on \
                       either side, and that refusal names the handle to use instead."
    )]
    pub(crate) async fn merge_entities(
        &self,
        Parameters(args): Parameters<MergeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.identified_for_write(args.sid.as_deref()).await {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let duplicate = EntityId(args.duplicate.trim().to_string());
        let survivor = EntityId(args.survivor.trim().to_string());
        // **A merge of a role object folds its name into another thing**, from
        // either side, so each side is judged as archiving it is.
        for side in [&duplicate, &survivor] {
            if let Some(refused) = self
                .refuse_a_stranger_the_role_object(&caller.bot, side, "merge it")
                .await
            {
                return Ok(refused);
            }
        }
        let date = self
            .dated(args.recorded_at.as_deref(), args.sid.as_deref())
            .await?;

        // **A merge into the caller's own bot carries the duplicate's ceiling
        // onto it, and the store decides that in the same act as the merge.**
        // A check made here, before the merge, left a gap a second session of
        // the same bot could write a ceiling into.
        // **A write that landed is never reported as failed** (rule 130): see
        // `capture`'s own note on the same shape.
        let (done, fold_behind) = match self
            .memory
            .merge(
                &duplicate,
                &survivor,
                args.reason.as_deref(),
                date,
                &caller.bot,
            )
            .await
        {
            Ok(done) => (done, None),
            Err(MemoryError::FoldBehind {
                landed: Landed::Merge(done),
                behind,
                ..
            }) => (*done, Some(behind)),
            Err(e) => return memory_declined("merge_entities", e),
        };
        self.registry.note_merged(done.folded.as_str());
        self.beat(
            "merge_entities",
            &duplicate.to_string(),
            args.sid.as_deref(),
        )
        .await;
        let mut body = serde_json::json!({
            "survivor": entity_json(&done.survivor),
            // **The handle that went, said back with where it now sends a
            // reader.** A recall of it answers that it was merged and names the
            // survivor in `merged_into`, and holds nothing of its own.
            "merged": done.folded.as_str(),
            "now_resolves_to": done.survivor.id.as_str(),
            // **The account, because it is what makes the act readable later.**
            "record": fact_json(&done.record, date, None),
            // **How many claims changed address.** Zero is an ordinary answer —
            // putting an empty duplicate away is exactly the repair this is
            // for — and any address a caller held for those claims is stale.
            "claims_moved": done.rehomed,
            "addresses_changed": done.rehomed > 0,
            // **What did not move, said back.** The survivor keeps its own page
            // and the duplicate's page is not carried over, so a caller who
            // wanted the two combined knows to do that by hand.
            "prose_stayed": format!(
                "the survivor keeps its own prose and the duplicate's prose is not carried over; \
                 a recall of {} answers status merged with merged_into naming {}, and serves no \
                 prose of its own",
                done.folded.as_str(),
                done.survivor.id.as_str(),
            ),
        });
        if let Some(behind) = fold_behind {
            crate::answer::note_fold_behind(&mut body, behind);
        }
        if self.first_contact(CLAIMS_DOMAIN, Some(&caller)).await {
            crate::answer::note_teaching(&mut body, CLAIMS_TEACHING);
        }
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::add_args;

    /// **Naming one handle as both sides is an answer, not a protocol
    /// failure** — the tool's own description promises `status: blocked`,
    /// and `MemoryError::NothingToMerge` fell straight past
    /// `memory_declined` into the client-error channel instead.
    #[tokio::test]
    async fn merging_a_handle_into_itself_is_blocked_rather_than_a_client_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args(
                "person",
                "person:milhouse",
                "Milhouse",
            )))
            .await
            .expect("add ok");

        let result = jojobot
            .merge_entities(Parameters(MergeArgs {
                duplicate: "person:milhouse".into(),
                survivor: "person:milhouse".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid),
            }))
            .await
            .expect("folding a handle into itself is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:milhouse");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap()
                .contains("survivor"),
            "the way forward names what to send instead: {body}",
        );
    }

    /// **Naming a handle that was already folded away is an answer too**,
    /// on either side — the description promises this comes back blocked
    /// and names the handle to use instead, and `MemoryError::AlreadyMerged`
    /// fell past `memory_declined` exactly as `NothingToMerge` did.
    #[tokio::test]
    async fn merging_an_already_merged_handle_is_blocked_rather_than_a_client_error() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        for handle in ["person:milhouse", "person:alpha", "person:bart"] {
            jojobot
                .add_entity(Parameters(add_args("person", handle, handle)))
                .await
                .expect("add ok");
        }
        jojobot
            .merge_entities(Parameters(MergeArgs {
                duplicate: "person:milhouse".into(),
                survivor: "person:alpha".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("the first fold lands")
            .content
            .first()
            .expect("a body came back");

        // The forwarding handle named again, as the duplicate.
        let result = jojobot
            .merge_entities(Parameters(MergeArgs {
                duplicate: "person:milhouse".into(),
                survivor: "person:bart".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid.clone()),
            }))
            .await
            .expect("a folded handle named again is an answer, not a protocol failure");
        let body = blocked(&result);
        assert_eq!(body["attempted"], "person:milhouse");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap()
                .contains("person:alpha"),
            "the way forward names where it forwards to: {body}",
        );

        // The forwarding handle named again, as the survivor.
        let as_survivor = jojobot
            .merge_entities(Parameters(MergeArgs {
                duplicate: "person:bart".into(),
                survivor: "person:milhouse".into(),
                reason: None,
                recorded_at: None,
                sid: Some(sid),
            }))
            .await
            .expect("a folded handle named as the survivor is an answer too");
        let body = blocked(&as_survivor);
        assert_eq!(body["attempted"], "person:milhouse");
        assert!(
            body["how_to_proceed"]
                .as_str()
                .unwrap()
                .contains("person:alpha"),
            "the way forward names where it forwards to, on either side: {body}",
        );
    }

    /// **A bot cannot raise its own ceiling by merging a thing that carries
    /// one into itself.** Writing a ceiling on ANOTHER thing is allowed, and a
    /// merge folds everything the duplicate held onto the survivor. Paired
    /// with merging a thing that carries no ceiling into the same bot, which
    /// lands, and with the ceiling-carrying thing merged into someone else,
    /// which lands too: the guard is about who ends up holding the ceiling.
    #[tokio::test]
    async fn a_bot_cannot_merge_a_thing_carrying_a_ceiling_into_itself() {
        use crate::memory::testing::{capture_args, capture_ok, ensure, fields_of};

        let jojobot = handler();
        let sid = writing_as(&jojobot);
        // The harness caller is bot:otto.
        for handle in ["bot:otto", "bot:milhouse", "bot:sigma", "bot:delta"] {
            ensure(&jojobot, handle).await;
        }
        let landed = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "5".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("bot:milhouse", "capacity is five")
            },
        )
        .await;
        assert_ne!(landed["status"], "blocked", "{landed}");
        let merge = |duplicate: &str, survivor: &str| MergeArgs {
            duplicate: duplicate.into(),
            survivor: survivor.into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid.clone()),
        };

        let refused = blocked(
            &jojobot
                .merge_entities(Parameters(merge("bot:milhouse", "bot:otto")))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        // **The refusal names the merge and the duplicate**, never a key the
        // caller sent, because the caller sent none. Identifiers only: the
        // duplicate, the key, and the verb and argument that take the key off.
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a refusal says how to proceed");
        for named in [
            "bot:milhouse",
            jojobot_domain::memory::THOUGHT_CAPACITY,
            "update_fact",
            "clear_fields",
        ] {
            assert!(
                how.contains(named),
                "the refusal does not name {named}: {how}"
            );
        }
        let own = fields_of(&jojobot, "bot:otto").await;
        assert!(
            own.get(jojobot_domain::memory::THOUGHT_CAPACITY).is_none(),
            "the refused merge must not carry the ceiling over: {own}"
        );
        let still_there = fields_of(&jojobot, "bot:milhouse").await;
        assert_eq!(
            still_there[jojobot_domain::memory::THOUGHT_CAPACITY],
            "5",
            "the refused merge left the duplicate as it was: {still_there}"
        );

        // **The second way forward the refusal names, taken exactly as it
        // says.** Another bot carries a ceiling; the caller takes the key off
        // the record that sets it, and the same merge then lands.
        ensure(&jojobot, "bot:epsilon").await;
        let carrier = capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "7".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("bot:epsilon", "capacity is seven")
            },
        )
        .await;
        let refused_again = blocked(
            &jojobot
                .merge_entities(Parameters(merge("bot:epsilon", "bot:otto")))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused_again["wrote"], false, "{refused_again}");
        let cleared = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    clear_fields: Some(vec![jojobot_domain::memory::THOUGHT_CAPACITY.to_string()]),
                    ..crate::memory::testing::update_args(&crate::memory::testing::address_of(
                        &carrier,
                    ))
                }))
                .await
                .expect("update_fact ok"),
        );
        assert_ne!(cleared["status"], "blocked", "{cleared}");
        let after_clearing = json_of(
            &jojobot
                .merge_entities(Parameters(merge("bot:epsilon", "bot:otto")))
                .await
                .expect("merge ok"),
        );
        assert_ne!(
            after_clearing["status"], "blocked",
            "taking the key off the duplicate, as the refusal says, must let the merge land: \
             {after_clearing}"
        );
        let own_after = fields_of(&jojobot, "bot:otto").await;
        assert!(
            own_after
                .get(jojobot_domain::memory::THOUGHT_CAPACITY)
                .is_none(),
            "the merge that landed must not have carried a ceiling: {own_after}"
        );

        // The positive halves.
        let ordinary = json_of(
            &jojobot
                .merge_entities(Parameters(merge("bot:sigma", "bot:otto")))
                .await
                .expect("merge ok"),
        );
        assert_ne!(ordinary["status"], "blocked", "{ordinary}");
        let elsewhere = json_of(
            &jojobot
                .merge_entities(Parameters(merge("bot:milhouse", "bot:delta")))
                .await
                .expect("merge ok"),
        );
        assert_ne!(elsewhere["status"], "blocked", "{elsewhere}");
        let carried = fields_of(&jojobot, "bot:delta").await;
        assert_eq!(
            carried[jojobot_domain::memory::THOUGHT_CAPACITY],
            "5",
            "a merge into somebody else carries the ceiling: {carried}"
        );
    }

    /// **A merge cannot put more thoughts in a room than the room holds.**
    /// The duplicate's thoughts become the survivor's when its claims move, so
    /// a merge is a way to write them that `capture` would have refused.
    /// Refused with the room beside it and the way forward, and nothing moves;
    /// archiving one thought, as the refusal says, lets the same merge land.
    /// Paired with a room that has space, which lands.
    #[tokio::test]
    async fn a_merge_that_would_overfill_a_bots_room_is_refused_and_moves_nothing() {
        use crate::memory::testing::{address_of, capture_args, capture_ok, ensure};

        let jojobot = handler();
        let sid = writing_as(&jojobot);
        let merge = |duplicate: &str, survivor: &str| MergeArgs {
            duplicate: duplicate.into(),
            survivor: survivor.into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid.clone()),
        };
        let thought = |subject: &str, content: &str, object: &str| CaptureArgs {
            shape: Some("connection".into()),
            object: Some(object.into()),
            ..capture_args(subject, content)
        };
        let capacity = |bot: &str, held: &str| CaptureArgs {
            fields: Some(
                [(
                    jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                    held.to_string(),
                )]
                .into_iter()
                .collect(),
            ),
            ..capture_args(bot, "capacity is set")
        };

        // The harness caller is bot:otto; the survivor is another bot, so the
        // self-ceiling guard has nothing to say here.
        ensure(&jojobot, "bot:otto").await;
        capture_ok(&jojobot, capacity("bot:milhouse", "1")).await;
        let held = capture_ok(
            &jojobot,
            thought(
                "bot:milhouse",
                "the gutter wants clearing",
                "thing:the-gutter",
            ),
        )
        .await;
        capture_ok(
            &jojobot,
            thought(
                "thing:jukebox",
                "the jukebox needs a needle",
                "thing:the-air-filter",
            ),
        )
        .await;

        let refused = blocked(
            &jojobot
                .merge_entities(Parameters(merge("thing:jukebox", "bot:milhouse")))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let room: Vec<&str> = refused["room"]
            .as_array()
            .expect("the refusal lists the room, as capture's does")
            .iter()
            .map(|thought| thought["address"].as_str().expect("an address"))
            .collect();
        assert_eq!(room, [address_of(&held)], "{refused}");
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a refusal says how to proceed");
        for named in ["update_fact", "archived", "merge_entities"] {
            assert!(
                how.contains(named),
                "the refusal does not name {named}: {how}"
            );
        }
        for argument in ["drop", "drop_because", "borrow"] {
            assert!(
                !how.contains(argument),
                "merge_entities has no `{argument}` argument, so the refusal must not name it: \
                 {how}"
            );
        }
        let survivor = serde_json::to_string(
            &json_of(
                &jojobot
                    .recall(Parameters(crate::memory::testing::recall_args(
                        "bot:milhouse",
                    )))
                    .await
                    .expect("recall ok"),
            )["objects"][0]["facts"],
        )
        .expect("facts serialize");
        assert!(
            !survivor.contains("jukebox"),
            "the refused merge moved a thought onto the survivor: {survivor}"
        );

        // **The way forward, taken exactly as the refusal says.**
        let archived = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    status: Some("archived".into()),
                    details: Some("no longer earns its slot".into()),
                    ..crate::memory::testing::update_args(&address_of(&held))
                }))
                .await
                .expect("update_fact ok"),
        );
        assert_ne!(archived["status"], "blocked", "{archived}");
        let landed = json_of(
            &jojobot
                .merge_entities(Parameters(merge("thing:jukebox", "bot:milhouse")))
                .await
                .expect("merge ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert_eq!(landed["merged"], "thing:jukebox", "{landed}");

        // **The positive half: a room with space takes the same merge.**
        capture_ok(&jojobot, capacity("bot:sigma", "2")).await;
        capture_ok(
            &jojobot,
            thought("bot:sigma", "the gutter wants clearing", "thing:the-gutter"),
        )
        .await;
        capture_ok(
            &jojobot,
            thought(
                "thing:kettle",
                "the kettle needs descaling",
                "thing:the-radiator",
            ),
        )
        .await;
        let fits = json_of(
            &jojobot
                .merge_entities(Parameters(merge("thing:kettle", "bot:sigma")))
                .await
                .expect("merge ok"),
        );
        assert_ne!(fits["status"], "blocked", "{fits}");
        assert_eq!(fits["merged"], "thing:kettle", "{fits}");
    }

    /// **A merge into a room with no capacity does not tell the caller to
    /// archive.** The room is empty, so there is nothing to archive; what is
    /// true is that a different identity has to raise `thought_capacity` first.
    /// The capacity-one case above is the paired positive: the same refusal
    /// there still says to archive.
    #[tokio::test]
    async fn a_merge_into_a_room_with_no_capacity_names_who_raises_the_ceiling() {
        use crate::memory::testing::{capture_args, capture_ok, ensure};

        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "bot:otto").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_CAPACITY.to_string(),
                        "0".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("bot:milhouse", "capacity is none")
            },
        )
        .await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("connection".into()),
                object: Some("thing:the-air-filter".into()),
                ..capture_args("thing:jukebox", "the jukebox needs a needle")
            },
        )
        .await;

        let refused = blocked(
            &jojobot
                .merge_entities(Parameters(MergeArgs {
                    duplicate: "thing:jukebox".into(),
                    survivor: "bot:milhouse".into(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(sid.clone()),
                }))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a refusal says how to proceed");
        for named in [
            jojobot_domain::memory::THOUGHT_CAPACITY,
            "different identity",
        ] {
            assert!(
                how.contains(named),
                "the refusal does not name {named}: {how}"
            );
        }
        assert!(
            !how.contains("archived"),
            "the refusal sends the caller to archive thoughts in a room that holds none: {how}"
        );
        assert!(
            refused["archive_needed"].is_null(),
            "the refusal counts thoughts to archive in a room that holds none: {refused}"
        );
    }

    /// **A merge cannot land a thought over the survivor's body cap.** The
    /// refusal names where the thought is now, on the duplicate, because the
    /// caller never wrote it on the survivor; shortening it there, as the
    /// refusal says, lets the same merge land. The boundary is exact: a
    /// thought at the cap lands.
    #[tokio::test]
    async fn a_merge_that_would_land_a_thought_over_the_body_cap_is_refused() {
        use crate::memory::testing::{address_of, capture_args, capture_ok, ensure};

        let jojobot = handler();
        let sid = writing_as(&jojobot);
        ensure(&jojobot, "bot:otto").await;
        capture_ok(
            &jojobot,
            CaptureArgs {
                fields: Some(
                    [(
                        jojobot_domain::memory::THOUGHT_BODY_CAP.to_string(),
                        "5".to_string(),
                    )]
                    .into_iter()
                    .collect(),
                ),
                ..capture_args("bot:milhouse", "cap is five")
            },
        )
        .await;
        let long = capture_ok(
            &jojobot,
            CaptureArgs {
                shape: Some("connection".into()),
                object: Some("thing:the-air-filter".into()),
                ..capture_args("thing:jukebox", "abcdef")
            },
        )
        .await;
        let merge = || MergeArgs {
            duplicate: "thing:jukebox".into(),
            survivor: "bot:milhouse".into(),
            reason: None,
            recorded_at: None,
            sid: Some(sid.clone()),
        };

        let refused = blocked(
            &jojobot
                .merge_entities(Parameters(merge()))
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let how = refused["how_to_proceed"]
            .as_str()
            .expect("a refusal says how to proceed");
        for named in [address_of(&long).as_str(), "update_fact", "merge_entities"] {
            assert!(
                how.contains(named),
                "the refusal does not name {named}: {how}"
            );
        }

        // **The way forward, taken exactly as the refusal says.**
        let shortened = json_of(
            &jojobot
                .update_fact(Parameters(UpdateFactArgs {
                    content: Some("abcde".into()),
                    provenance: Some("inference".into()),
                    ..crate::memory::testing::update_args(&address_of(&long))
                }))
                .await
                .expect("update_fact ok"),
        );
        assert_ne!(shortened["status"], "blocked", "{shortened}");
        let landed = json_of(
            &jojobot
                .merge_entities(Parameters(merge()))
                .await
                .expect("merge ok"),
        );
        assert_eq!(landed["merged"], "thing:jukebox", "{landed}");
    }

    /// **The write says it landed, never that it failed, when only the fold
    /// behind it could not confirm it** (rule 130) — `merge_entities`'s own
    /// catch of `MemoryError::FoldBehind`, the same shape `capture`'s own
    /// case proves.
    #[tokio::test]
    async fn a_merge_whose_fold_could_not_refresh_answers_landed_not_failed() {
        use crate::memory::testing::{FoldBehindMemory, InMemoryMemory, SpySearch};

        let jojobot = Jojobot::new(
            Arc::new(FoldBehindMemory(Arc::new(InMemoryMemory::booted()))),
            Arc::new(SpySearch::default()),
            Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
            Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
            Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
            seeded_registry(),
        );
        let sid = writing_as(&jojobot);
        jojobot
            .add_entity(Parameters(add_args(
                "person",
                "person:milhouse",
                "Milhouse",
            )))
            .await
            .expect("add ok");
        jojobot
            .add_entity(Parameters(add_args("person", "person:bart", "Bart")))
            .await
            .expect("add ok");

        let body = json_of(
            &jojobot
                .merge_entities(Parameters(MergeArgs {
                    duplicate: "person:bart".into(),
                    survivor: "person:milhouse".into(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(sid),
                }))
                .await
                .expect("merge ok"),
        );
        assert_eq!(body["fold"]["behind"], "stale", "{body}");
        // **The positive half.** The merge still landed, exactly as an
        // ordinary merge does.
        assert_eq!(body["merged"], "person:bart", "{body}");
    }

    /// **A ceiling that lands between the guard's read and the merge must not
    /// reach the caller's own bot.** The guard reads the duplicate, finds no
    /// ceiling, and the merge then folds everything the duplicate holds onto
    /// the survivor. A second session of the same bot can write a ceiling in
    /// that gap, and the merge would raise the ceiling its own bot is not
    /// allowed to raise. Paired with the same merge landing when nothing races,
    /// so the refusal cannot be a merge that never runs.
    #[tokio::test]
    async fn a_ceiling_written_between_the_guard_and_the_merge_does_not_reach_the_callers_bot() {
        use crate::memory::testing::{InMemoryMemory, RacingMemory, SpySearch, ensure, fields_of};

        let inner = Arc::new(InMemoryMemory::booted());
        let racing = |armed: bool| {
            Jojobot::new(
                Arc::new(RacingMemory {
                    inner: inner.clone(),
                    racer: EntityId("bot:milhouse".into()),
                    armed: std::sync::atomic::AtomicBool::new(armed),
                }),
                Arc::new(SpySearch::default()),
                Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
                Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
                Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
                seeded_registry(),
            )
        };
        let jojobot = racing(true);
        let sid = writing_as(&jojobot);
        // The harness caller is bot:otto.
        for handle in ["bot:otto", "bot:milhouse"] {
            ensure(&jojobot, handle).await;
        }
        async fn merge(
            jojobot: &Jojobot,
            sid: &str,
            duplicate: &str,
        ) -> Result<CallToolResult, McpError> {
            jojobot
                .merge_entities(Parameters(MergeArgs {
                    duplicate: duplicate.into(),
                    survivor: "bot:otto".into(),
                    reason: None,
                    recorded_at: None,
                    sid: Some(sid.to_string()),
                }))
                .await
        }

        let refused = blocked(
            &merge(&jojobot, &sid, "bot:milhouse")
                .await
                .expect("a refusal is an answer, not a failure"),
        );
        assert_eq!(refused["wrote"], false, "{refused}");
        let own = fields_of(&jojobot, "bot:otto").await;
        assert!(
            own.get(jojobot_domain::memory::THOUGHT_CAPACITY).is_none(),
            "a ceiling written after the check reached the caller's own bot: {own}"
        );

        // **The positive half**: a duplicate that gains nothing in the gap
        // merges, so the refusal above was the ceiling and not the setup.
        ensure(&jojobot, "bot:sigma").await;
        let landed = json_of(
            &merge(&racing(false), &sid, "bot:sigma")
                .await
                .expect("merge ok"),
        );
        assert_ne!(landed["status"], "blocked", "{landed}");
        assert_eq!(landed["merged"], "bot:sigma", "{landed}");
    }
}
