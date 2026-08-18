//! `set_charter` — Write a bot's charter: the prose layer of its own page.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `set_charter`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCharterArgs {
    /// The bot whose charter this is: its bare slug, or its full handle.
    pub bot: String,
    /// The charter itself. Prose: paragraphs are fine. It **replaces** whatever
    /// charter the bot had, so send the whole thing, not an addition.
    pub prose: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub sid: Option<String>,
}

/// Write a bot's charter — the prose layer of its own page.
#[tool_router(router = set_charter_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Write a bot's charter: the orienting text start_here hands a session that \
                       boots as this bot — what this identity is, its hard lines, where its work \
                       lives. Replaces the whole charter rather than adding to it. \
                       IT ANSWERS WITH A RECEIPT, NOT THE CHARTER: the bot it landed on, how \
                       many bytes were stored — compare it with what you sent and you learn \
                       the store trimmed it — and the opening line, so you can tell which \
                       charter this was. You wrote the prose; start_here with this bot returns \
                       it in full. A bot that \
                       does not exist comes back status: blocked with the nearest handles — \
                       add_entity first; nothing is created here. Rules are not written here \
                       either: a rule is a fact about the bot, so capture it."
    )]
    pub(crate) async fn set_charter(
        &self,
        Parameters(args): Parameters<SetCharterArgs>,
    ) -> Result<CallToolResult, McpError> {
        // Refused here, before anything is written — see
        // [`Jojobot::attributable`].
        if let Err(refused) = self.identified(args.sid.as_deref()) {
            return Ok(refused);
        }
        let bot = bot_id(&args.bot)?;
        let stored = match self.memory.set_prose(&bot, &args.prose).await {
            Ok(stored) => stored,
            Err(e) => return memory_declined("set_charter", e),
        };
        self.beat("set_charter", bot.as_str(), args.sid.as_deref())
            .await;
        // **A receipt, not the charter.** The caller wrote this prose in the
        // call it is reading the answer to, and a charter is the largest block
        // this surface carries — the boot that ships one calls it that itself.
        // What comes back is what the caller cannot know: which bot it landed
        // on, how many bytes the store kept, and enough of the opening to tell
        // one charter from another.
        let mut body = serde_json::json!({ "bot": bot.as_str() });
        crate::answer::elide_prose(
            &mut body,
            "charter",
            &stored,
            "you wrote this charter. start_here with this bot returns it in full.",
        );
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

    /// `set_charter` writes the orienting prose and reads it back — and it is
    /// the same text a boot hands over, so what an operator writes is what a
    /// session is told.
    #[tokio::test]
    async fn set_charter_writes_the_prose_that_a_boot_reads_back() {
        let jojobot = handler();
        make_bot(&jojobot, "gamma").await;

        let written = json_of(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "gamma".into(),
                    prose: "  Holds the plan. Does not implement.  ".into(),
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("set_charter ok"),
        );
        assert_eq!(written["bot"], "bot:gamma");
        // **The charter is not shipped back to the session that wrote it.** A
        // charter is the largest block on this surface, and its author is the
        // one reader it teaches nothing.
        assert_eq!(
            written["charter"],
            serde_json::Value::Null,
            "the prose comes back to nobody who just sent it: {written}"
        );
        assert_eq!(written["charter_elided"], true, "{written}");
        assert_eq!(
            written["charter_bytes"],
            "Holds the plan. Does not implement.".len(),
            "the count is of what was STORED, so the trim is legible from it: {written}"
        );
        assert!(
            written["charter_head"]
                .as_str()
                .is_some_and(|head| head.starts_with("Holds the plan")),
            "…and enough to recognize which charter landed: {written}"
        );
        assert!(
            written["how_to_read"]
                .as_str()
                .is_some_and(|how| how.contains("start_here")),
            "eliding is never silent — the answer names the call that returns it: {written}"
        );
        // **The write still landed whole**, which is the half the receipt must
        // not cost: a boot reads back every byte, trimmed as the store trims.
        assert_eq!(
            boot(&jojobot, "gamma").await["identity"]["charter"],
            "Holds the plan. Does not implement."
        );

        // A charter for a bot that does not exist misses — it never creates one,
        // and the miss wears the same blocked shape every other absence does.
        let missed = blocked(
            &jojobot
                .set_charter(Parameters(SetCharterArgs {
                    bot: "nobody".into(),
                    prose: "a charter for nobody".into(),
                    sid: Some(crate::harness::TEST_SID.into()),
                }))
                .await
                .expect("an unknown bot is an answer, not a protocol failure"),
        );
        assert_eq!(missed["attempted"], "bot:nobody");
        assert!(
            missed["how_to_proceed"]
                .as_str()
                .is_some_and(|a| a.contains("add_entity")),
            "the way out names the verb that opens it: {missed}"
        );
    }
}
