//! "A bot's mail is finished only by that bot."
//!
//! Mail left for a bot is owed by that bot. Another bot, or a call that names
//! no session, cannot make it look handled: that would tell the sender, and
//! anyone who asks after it, that the work was done when its owner never
//! touched it.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// The per-state counts of the caller's own box, taking delivery of nothing.
async fn counts(s: &Session) -> Answer {
    s.call("read_mailbox", json!({"counts_only": true})).await
}

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
    counts(&delta)
        .await
        .number("/counts/new", 1)
        .number("/counts/processed", 0);

    // ── the owner's own retirement lands, and processed stays terminal ──────
    delta
        .call(
            "mark_processed",
            json!({"message_id": &message, "notes": "did it"}),
        )
        .await
        .says("\"state\":\"processed\"")
        .says("did it");
    counts(&delta)
        .await
        .number("/counts/new", 0)
        .number("/counts/read", 0)
        .number("/counts/processed", 1);

    // Terminal: a delivery hands over nothing, a read by id moves nothing, and
    // another bot's second attempt is refused again; the counts stand.
    delta
        .call("read_mailbox", json!({}))
        .await
        .number("/count", 0);
    let archived = delta
        .call("read_message", json!({"message_id": &message}))
        .await;
    assert_eq!(
        archived.json()["state"],
        "processed",
        "a processed message reads back as processed: {}",
        archived.raw()
    );
    gamma
        .refused(
            "mark_processed",
            json!({"message_id": &message, "notes": "me too"}),
        )
        .await
        .says("owner");
    counts(&delta)
        .await
        .number("/counts/new", 0)
        .number("/counts/read", 0)
        .number("/counts/processed", 1);

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

    // The door says why and what to do: it has no session to attribute the
    // call to, and a boot gives one.
    story
        .refused_at_the_door(
            "mark_processed",
            json!({"message_id": &message, "notes": "all done"}),
        )
        .await
        .says("\"wrote\":false")
        .holds_at("/how_to_proceed", "sid")
        .holds_at("/how_to_proceed", "start_here");

    // Nothing was written: the owner still owes it.
    counts(&delta)
        .await
        .number("/counts/new", 1)
        .number("/counts/processed", 0);

    gamma
        .wrap("tried to finish a colleague's mail unseen")
        .await;
    delta.wrap("kept my mail").await;
    story.finish().await;
}
