//! `merge_entities` — Merge a duplicate into the thing it duplicates.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

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
    /// **The day the two were put together**, `YYYY-MM-DD`. Defaults to today
    /// in your session's zone.
    #[serde(default)]
    pub date: Option<String>,
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
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let duplicate = EntityId(args.duplicate.trim().to_string());
        let survivor = EntityId(args.survivor.trim().to_string());
        let date = self.dated(args.date.as_deref(), args.sid.as_deref())?;

        let done = match self
            .memory
            .merge(&duplicate, &survivor, args.reason.as_deref(), date)
            .await
        {
            Ok(done) => done,
            Err(e) => return memory_declined("merge_entities", e),
        };
        self.beat(
            "merge_entities",
            &duplicate.to_string(),
            args.sid.as_deref(),
        )
        .await;
        json_result(&serde_json::json!({
            "survivor": entity_json(&done.survivor),
            // **The handle that went, said back with where it now sends a
            // reader.** A caller holding it needs to know it still answers and
            // no longer names a thing of its own.
            "merged": done.folded.as_str(),
            "now_resolves_to": done.survivor.id.as_str(),
            // **The account, because it is what makes the act readable later.**
            "record": fact_json(&done.record, date),
            // **How many claims changed address.** Zero is an ordinary answer —
            // putting an empty duplicate away is exactly the repair this is
            // for — and any address a caller held for those claims is stale.
            "claims_moved": done.rehomed,
            "addresses_changed": done.rehomed > 0,
        }))
    }
}
