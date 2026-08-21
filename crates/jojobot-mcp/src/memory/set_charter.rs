//! `set_charter` — Write a bot's charter: the prose layer of its own page.
//!
//! One verb, one file: its arguments, the description a caller reads,
//! and an entrypoint that chains the systems below it.

use super::*;

/// Arguments to `set_charter`.
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub struct SetCharterArgs {
    /// The bot whose charter this is: its bare slug, or its full handle.
    pub(crate) bot: String,
    /// The charter itself. Prose: paragraphs are fine.
    ///
    /// **It replaces what this bot had**, so send the whole thing rather than
    /// an addition.
    ///
    /// ⚠️ **Send what YOU are writing, not a charter you just read back.** Some
    /// of what a read hands you may be text the software already supplies, and
    /// sending that back comes back blocked with nothing written.
    pub(crate) prose: String,
    /// **Your session id**, exactly as the boot door returned it. Pass it on
    /// every call — it is what tells jojobot which bot is asking. Reads are
    /// attributed, never journalled.
    #[serde(default)]
    pub(crate) sid: Option<String>,
}

/// Write a bot's charter — the prose layer of its own page.
#[tool_router(router = set_charter_router, vis = "pub(crate)")]
impl Jojobot {
    #[tool(
        description = "Write a bot's charter: the orienting text start_here hands a session that \
                       boots as this bot — what this identity is, its hard lines, where its work \
                       lives. IT REPLACES RATHER THAN ADDS, so send the whole charter rather \
                       than an addition. SEND WHAT YOU ARE WRITING, never a charter you just \
                       read back: some of what a read hands you may be text the software \
                       already supplies, and a charter repeating it comes back status: \
                       blocked with nothing written and says so. \
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
            // **The route a caller may actually take.** This named `start_here`
            // with the bot, which is how a session BOOTS as that bot — so the
            // receipt for a write proposed the one act the rules refuse,
            // reading another identity's data by becoming it. `recall` reads
            // the page and mints nothing.
            //
            // **It says what you wrote rather than the charter**, because on an
            // identity the software ships a charter is two layers and this call
            // writes one of them: `recall` returns the instance's own text, and
            // promising the whole would be wrong on exactly the identity a
            // fresh instance arrives holding.
            "you wrote this charter. recall this bot with prose: true returns what you wrote \
             here, in full.",
        );
        json_result(&body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;
    use crate::memory::testing::recall_args;

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
        let how = written["how_to_read"]
            .as_str()
            .unwrap_or_else(|| panic!("eliding is never silent: {written}"))
            .to_string();
        assert!(
            how.contains("recall"),
            "eliding is never silent — the answer names the call that returns it: {written}"
        );

        // **The route the answer names is WALKED, not trusted.** A receipt that
        // sends a caller somewhere is only as good as what it finds there, and
        // a string nobody followed is how the last one shipped naming a call
        // the rules forbid.
        let followed = json_of(
            &jojobot
                .recall(Parameters(RecallArgs {
                    prose: Some(true),
                    sid: Some(TEST_SID.to_string()),
                    ..recall_args("bot:gamma")
                }))
                .await
                .expect("the call the answer names is one a caller may make"),
        );
        assert_eq!(
            followed["objects"][0]["prose"], "Holds the plan. Does not implement.",
            "following what the receipt says returns the charter in full: {followed}",
        );
        // 🚨 **And it boots nobody.** This is the property rather than the
        // sentence: a route that hands back a session handle is a route that
        // made the caller somebody, and reading another bot's data by becoming
        // it is the one act the rules refuse. Asserted over the WHOLE answer,
        // because a handle anywhere in it is a handle a caller will use — and
        // this stays true against a build that names some other booting verb,
        // where grepping for the absence of one word would not.
        assert!(
            !followed.to_string().contains("\"sid\""),
            "the route the receipt names hands back a session handle, so following it makes the \
             caller somebody: {followed}"
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
