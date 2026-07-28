//! `amend_journal` — Rewrite the newest entry in place.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `amend_journal`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct AmendJournalArgs {
    /// What the most recent entry should say instead. It replaces that entry
    /// whole.
    pub entry: String,
    /// Your session id — the session whose newest entry to rewrite.
    pub sid: String,
}

/// Rewrite the newest entry in place.
#[tool_router(router = amend_journal_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Rewrite your session's MOST RECENT chronology entry, in place — for a \
                       beat you got wrong or want to finish saying. Only the most recent one: \
                       everything older is append-only, because a journal that can be rewritten \
                       further back is not evidence of anything. A session with no entries yet \
                       comes back status: blocked rather than quietly writing your text as a \
                       first entry — an amend that silently became an append leaves a chronology \
                       saying something you did not mean. A closed session comes back blocked \
                       too. Pass your `sid` on every call — it is the address, and it survives \
                       the fresh connection most clients open per tool call. This verb never \
                       STARTS a session: there is nothing to amend in one that does not exist \
                       yet."
    )]
    pub(crate) async fn amend_journal(
        &self,
        Parameters(args): Parameters<AmendJournalArgs>,
    ) -> Result<CallToolResult, McpError> {
        let gate = self.registry.gate(&self.gate_key(Some(&args.sid)));
        let _serialized = gate.lock().await;
        let caller = match self.identified(Some(&args.sid)) {
            Ok(caller) => caller,
            Err(refused) => return Ok(refused),
        };
        // **No lazy begin here, deliberately.** There is nothing to amend in a
        // session that has not been written yet, and minting a card to hold a
        // correction would be a card created by the one verb whose whole job is
        // to add nothing. A handle with no card behind it is told exactly that,
        // rather than "no such session" — the handle is real, the run simply has
        // not started writing.
        let Some(session) = caller.card else {
            return Ok(session_nothing_to_amend());
        };
        // The guard exists to be held across the amend, not merely taken.
        let _ = &_serialized;
        match self.sessions.amend_last(&session, &args.entry).await {
            Ok(entry) => json_result(&serde_json::json!({
                "session": session.as_str(),
                "entry": entry_json(&entry),
            })),
            Err(e) => session_declined(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::session::testing::*;

    /// **Amending a session that has not begun is refused, not turned into a
    /// first entry.** A correction that silently became an append leaves a
    /// chronology saying something nobody meant.
    #[tokio::test]
    async fn amending_before_the_first_entry_is_blocked_and_writes_nothing() {
        let store = Arc::new(InMemorySessions::new());
        let jojobot = with_sessions(store.clone());
        make_bot(&jojobot, "gamma").await;
        let sid = booted(&jojobot, "gamma").await;

        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "there is nothing to correct".into(),
                    sid,
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        assert!(
            store
                .sessions_of(&EntityId("bot:gamma".into()))
                .await
                .expect("list ok")
                .is_empty(),
            "…and it did not mint a session to hold the correction"
        );
    }

    /// **amend_journal triages the same way the other two do.** A caller with no
    /// identity is told to boot — not told there is nothing to amend, which is a
    /// different fact about a different thing.
    #[tokio::test]
    async fn amending_without_a_boot_says_to_boot_rather_than_no_entries() {
        let jojobot = with_sessions(Arc::new(InMemorySessions::new()));
        let body = json_of(
            &jojobot
                .amend_journal(Parameters(AmendJournalArgs {
                    entry: "actually, it was the other thing".into(),
                    // No boot, so no handle to carry. `sid` is a required
                    // parameter now, so "never booted" reaches the verb as an
                    // empty one rather than as an absent field.
                    sid: String::new(),
                }))
                .await
                .expect("call ok"),
        );
        assert_eq!(body["status"], "blocked");
        let how = body["how_to_proceed"].as_str().expect("advice");
        // **The remedy has to be one that works.** This is the message a
        // stateless caller sees, and identity survives nothing but the handle —
        // so the advice has to name the handle and the door that mints it,
        // rather than pointing back into the loop this refusal exists to close.
        assert!(
            how.contains("`sid`"),
            "the way out names the parameter: {how}"
        );
        assert!(
            how.contains("start_here"),
            "…and the door that hands one over: {how}"
        );
        assert!(
            !how.contains("no entries"),
            "…and it does not answer about a session nobody looked for: {how}"
        );
    }
}
