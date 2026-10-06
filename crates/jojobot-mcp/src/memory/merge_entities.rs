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
                       moves to the survivor: its claims, the edges drawn at it, and the history \
                       under its keys. Beside them lands a dated record of the merge itself, on \
                       the survivor, naming the handle that went and the reason if you give one. \
                       NOTHING IS REMOVED: the duplicate's handle keeps answering, so a handle \
                       written down anywhere still resolves — but a recall of it comes back with \
                       status merged, names the survivor in merged_into and holds nothing of \
                       its own. It does not answer as the survivor: recall the survivor to read \
                       everything. It simply stops being a thing of its own. USE IT WHEN TWO \
                       HANDLES ARE ONE THING. It does not ask whether they really are: that is your \
                       judgement, so putting two unrelated things together is allowed and is \
                       recorded exactly as legibly. WHAT DOES NOT MOVE: the duplicate's prose and \
                       its aliases stay with the handle that forwards, and the survivor does not \
                       gain them. There is no way back, so if you are unsure, \
                       read both with recall first. ⚠️ THE CLAIMS THAT MOVE GET NEW ADDRESSES, \
                       because an address is local to the thing that holds it — any address you \
                       were holding for the duplicate's claims is stale afterwards, and the \
                       answer says how many moved. A MERGE INTO YOUR OWN BOT IS REFUSED WHEN THE DUPLICATE CARRIES \
                       thought_capacity OR thought_body_cap, because the merge would raise your \
                       own ceiling: have a different identity merge it, or take the key off the \
                       duplicate first with update_fact and clear_fields. Naming one handle as \
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
        let date = self.dated(args.recorded_at.as_deref(), args.sid.as_deref())?;

        // **A merge into the caller's own bot carries the duplicate's ceiling
        // onto it.** Everything the duplicate held moves to the survivor and
        // folds there, so a bot that wrote a ceiling on another thing, which is
        // allowed, could merge that thing into itself and raise its own. The
        // trait's merge takes no caller, so the check is here. Both handles are
        // compared as they answer now, and a read that fails refuses the merge.
        let survivor_now = match self.current_handle(&survivor).await {
            Ok(current) => current,
            Err(e) => return memory_declined("merge_entities", e),
        };
        if survivor_now == caller.bot {
            let duplicate_now = match self.current_handle(&duplicate).await {
                Ok(current) => current,
                Err(e) => return memory_declined("merge_entities", e),
            };
            if duplicate_now != survivor_now {
                let carried = match self.memory.fields(&duplicate_now).await {
                    Ok(carried) => carried,
                    Err(e) => return memory_declined("merge_entities", e),
                };
                let keys = jojobot_domain::memory::ceiling_keys_in(&carried);
                if !keys.is_empty() {
                    // **The refusal says the MERGE was refused and why**, not
                    // that the caller tried to set a key: it sent no fields.
                    // Both ways forward exist: a different identity performs
                    // the merge, or the key comes off the duplicate first.
                    let keys = keys.join(", ");
                    return Ok(blocked_body(
                        &duplicate_now,
                        &[],
                        format!(
                            "Nothing was written. '{duplicate_now}' carries {keys}, and merging it \
                             into '{survivor_now}', your own bot, would raise your own ceiling, \
                             which only a different identity may do. Ask a different identity to \
                             make this merge, or take {keys} off '{duplicate_now}' first: \
                             update_fact the record that sets it with clear_fields, then merge \
                             again."
                        ),
                    ));
                }
            }
        }

        // **A write that landed is never reported as failed** (rule 130): see
        // `capture`'s own note on the same shape.
        let (done, fold_behind) = match self
            .memory
            .merge(&duplicate, &survivor, args.reason.as_deref(), date)
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
        self.beat(
            "merge_entities",
            &duplicate.to_string(),
            args.sid.as_deref(),
        )
        .await;
        let mut body = serde_json::json!({
            "survivor": entity_json(&done.survivor),
            // **The handle that went, said back with where it now sends a
            // reader.** A caller holding it needs to know it still answers and
            // no longer names a thing of its own.
            "merged": done.folded.as_str(),
            "now_resolves_to": done.survivor.id.as_str(),
            // **The account, because it is what makes the act readable later.**
            "record": fact_json(&done.record, date, None),
            // **How many claims changed address.** Zero is an ordinary answer —
            // putting an empty duplicate away is exactly the repair this is
            // for — and any address a caller held for those claims is stale.
            "claims_moved": done.rehomed,
            "addresses_changed": done.rehomed > 0,
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
}
