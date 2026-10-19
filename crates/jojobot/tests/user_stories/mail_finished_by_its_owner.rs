//! "A bot's mail is finished only by that bot."
//!
//! Mail left for a bot is owed by that bot. Another bot, or a call that names
//! no session, cannot make it look handled: that would tell the sender, and
//! anyone who asks after it, that the work was done when its owner never
//! touched it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn only_the_bot_a_message_was_left_for_can_finish_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.wrap("stood up the two bots").await;

    let gamma = story.as_bot("bot:gamma").await;
    let delta = story.as_bot("bot:delta").await;
    let message = gamma.post("delta", "A job", "Please do the job.").await;

    // ── another bot's retirement is refused, and says whose box it is ───────
    gamma
        .refused(
            "mark_processed",
            json!({"message_id": &message, "notes": "all done"}),
        )
        .await
        .says("delta")
        .says("owner");

    // ── nothing was written: the owner still owes it ────────────────────────
    delta
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"new\":1")
        .says("\"processed\":0");

    // ── the owner's own retirement lands, and processed stays terminal ──────
    delta
        .call(
            "mark_processed",
            json!({"message_id": &message, "notes": "did it"}),
        )
        .await
        .says("\"state\":\"processed\"")
        .says("did it");
    delta
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"processed\":1");

    gamma.wrap("tried to finish a colleague's mail").await;
    delta.wrap("finished my own mail").await;
    story.finish().await;
}

#[tokio::test]
async fn a_call_naming_no_session_cannot_finish_a_bots_mail() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.wrap("stood up the two bots").await;

    let gamma = story.as_bot("bot:gamma").await;
    let delta = story.as_bot("bot:delta").await;
    let message = gamma.post("delta", "A job", "Please do the job.").await;

    story
        .refused_at_the_door(
            "mark_processed",
            json!({"message_id": &message, "notes": "all done"}),
        )
        .await;

    // Nothing was written: the owner still owes it.
    delta
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .says("\"new\":1")
        .says("\"processed\":0");

    gamma
        .wrap("tried to finish a colleague's mail unseen")
        .await;
    delta.wrap("kept my mail").await;
    story.finish().await;
}
