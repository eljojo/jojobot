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
                       NOTHING IS REMOVED: the duplicate's handle keeps answering and sends a \
                       reader to the survivor, so a handle written down anywhere still resolves \
                       — it simply stops being a thing of its own. USE IT WHEN TWO HANDLES ARE \
                       ONE THING. It does not ask whether they really are: that is your \
                       judgement, so putting two unrelated things together is allowed and is \
                       recorded exactly as legibly. There is no way back, so if you are unsure, \
                       read both with recall first. ⚠️ THE CLAIMS THAT MOVE GET NEW ADDRESSES, \
                       because an address is local to the thing that holds it — any address you \
                       were holding for the duplicate's claims is stale afterwards, and the \
                       answer says how many moved. Naming one handle as both sides comes back \
                       status: blocked. So does naming a handle that was already merged away, on \
                       either side, and that refusal names the handle to use instead."
    )]
    pub(crate) async fn merge_entities(
        &self,
        Parameters(args): Parameters<MergeArgs>,
    ) -> Result<CallToolResult, McpError> {
        let caller = match self.identified(args.sid.as_deref()) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        let duplicate = EntityId(args.duplicate.trim().to_string());
        let survivor = EntityId(args.survivor.trim().to_string());
        let date = self.dated(args.recorded_at.as_deref(), args.sid.as_deref())?;

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
