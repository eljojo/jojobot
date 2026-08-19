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
    /// The charter itself. Prose: paragraphs are fine.
    ///
    /// **For a bot somebody stood up, this replaces what that bot had** — send
    /// the whole thing, not an addition.
    ///
    /// **For the identity the software ships, it replaces that instance's own
    /// layer and nothing else.** That identity reads back as two layers: the
    /// core the build carries, which no call reaches, and the text written
    /// here, which narrows it. **So send your own half rather than the whole
    /// answer you just read** — a caller that sends the composed text back
    /// stores the build's own words as the instance's, and they stop moving
    /// when the software does.
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
                       lives. IT REPLACES RATHER THAN ADDS — but what it replaces depends on \
                       the bot. For one somebody stood up it is the whole charter, so send the \
                       whole thing. For the identity the software ships it is that instance's \
                       OWN layer: the core the build carries is composed in on the way out and \
                       no call writes it, so send your half rather than the composed text you \
                       read, or the build's words become the instance's and stop moving when \
                       the software does. \
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
        // **The core is not a caller's to store, and the round trip is how it
        // gets stored anyway.** A caller that reads the composed charter and
        // sends it back writes the build's own words into the instance's layer,
        // where they never move again — a shipped default quietly turned into a
        // frozen customisation, by a caller doing what this description used to
        // tell it to.
        //
        // **Refused rather than trimmed** (rule 68): silently cutting somebody's
        // prose down to the half we wanted would store something they did not
        // write, and they would never learn which half we kept.
        if let Some(core) = crate::orientation::charter::core_for(&bot)
            && carries(&args.prose, core)
        {
            return Ok(blocked_body(
                &bot,
                &[],
                "Nothing was written. This charter carries the core the build ships, which \
                 means it is the composed text a read hands back rather than this instance's \
                 own half. Storing it would freeze the build's words as this instance's. Send \
                 the part below the divider — what this instance has written for itself — or \
                 nothing at all if it has none."
                    .to_string(),
            ));
        }
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

/// **Does this prose carry that core?** Compared on the core's first paragraph
/// rather than the whole of it, so a caller sending back a charter it read
/// through a client that rewrapped the text is still caught.
///
/// A paragraph is long enough that nobody writes it by accident, which is what
/// keeps this from refusing a charter that merely agrees with the core.
fn carries(prose: &str, core: &str) -> bool {
    let Some(opening) = core.trim().split("\n\n").next() else {
        return false;
    };
    let opening = opening.trim();
    !opening.is_empty() && prose.contains(opening)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::harness::*;

    /// **The shipped identity's charter is two layers, and this call reaches
    /// one of them.**
    ///
    /// The core is carried by the build and composed on the way out; what a
    /// caller writes is the instance's own half. **The description used to
    /// promise this call replaced the whole thing**, and a caller believing it
    /// does the round trip below: read the charter, send it back.
    ///
    /// 🚨 **That is the silent one.** It stores the build's own words as the
    /// instance's, where they stop moving when the software does — a shipped
    /// default quietly converted into a frozen customisation, by a caller doing
    /// what the surface told it to.
    #[tokio::test]
    async fn sending_the_composed_charter_back_does_not_store_the_core() {
        let jojobot = handler();
        let sid = writing_as(&jojobot);
        make_bot(&jojobot, "assistant").await;

        // What a caller reads: the core, and nothing of its own yet.
        let composed = charter_of(&jojobot, &sid).await;
        // The needle is the core's own words, in the case the core writes
        // them: a lowercase reading of it matches nothing and passes on a build
        // that ships no core at all.
        assert!(
            composed.contains("GROUND TRUTH"),
            "the shipped identity reads back with the core it ships: {composed}",
        );

        // **The round trip a caller believing the old promise makes.**
        let sent = jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "assistant".into(),
                prose: composed.clone(),
                sid: Some(sid.clone()),
            }))
            .await
            .expect("set_charter answers");
        let body = json_of(&sent);
        // **Refused, and it has to be.** The instance layer cannot both hold
        // what was sent and keep the core out of the store — those are the same
        // bytes. So the only answer that does not freeze the build's words is
        // to refuse, and the refusal says which half to send instead (rule 68).
        assert_eq!(
            body["status"], "blocked",
            "the composed text was stored, so the core is now this instance's: {body}",
        );
        assert!(
            body["how_to_proceed"]
                .as_str()
                .is_some_and(|advice| !advice.is_empty()),
            "the refusal leaves the caller nowhere to go: {body}",
        );

        // **What the store kept.** The instance's own layer must not hold the
        // core: the core is composed, and a copy of it in the store is a copy
        // that stops moving when the build does.
        let kept = jojobot
            .memory
            .scan_entity(&EntityId("bot:assistant".into()))
            .await
            .expect("the store answers")
            .map(|doc| doc.prose)
            .unwrap_or_default();
        assert!(
            !kept.contains("GROUND TRUTH"),
            "the core rode into the store on a caller's round trip, where it is frozen: {kept}",
        );

        // **And the read is unchanged**: the core still arrives, once.
        let after = charter_of(&jojobot, &sid).await;
        assert_eq!(
            after, composed,
            "the answer moved on a call that wrote nothing: {after}",
        );

        // **The positive the refusal rests on: this instance's OWN half is
        // stored exactly as before.** Without it, the case passes against a
        // build that refuses every charter for the shipped identity.
        jojobot
            .set_charter(Parameters(SetCharterArgs {
                bot: "assistant".into(),
                prose: "This instance keeps the workshop rota.".into(),
                sid: Some(sid.clone()),
            }))
            .await
            .expect("set_charter answers");
        let kept = jojobot
            .memory
            .scan_entity(&EntityId("bot:assistant".into()))
            .await
            .expect("the store answers")
            .map(|doc| doc.prose)
            .unwrap_or_default();
        assert_eq!(
            kept.trim(),
            "This instance keeps the workshop rota.",
            "the instance's own half is not what the store kept",
        );
        assert!(
            charter_of(&jojobot, &sid).await.contains("GROUND TRUTH"),
            "…and the core is still composed on top of it",
        );
    }

    /// The charter the shipped identity reads back with, off the door.
    async fn charter_of(jojobot: &Jojobot, sid: &str) -> String {
        let booted = json_of(
            &jojobot
                .start_here(Parameters(crate::orientation::OrientArgs {
                    bot: Some("assistant".into()),
                    brief: Some(true),
                    skill: None,
                    resume: None,
                    timezone: None,
                    sid: Some(sid.to_string()),
                }))
                .await
                .expect("the door answers"),
        );
        booted["identity"]["charter"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    }

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
