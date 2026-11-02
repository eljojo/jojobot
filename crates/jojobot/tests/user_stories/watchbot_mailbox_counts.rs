//! **An outside controller learns which bots have mail waiting, and since when.**
//! A colleague leaves mail in a bot's box; the controller, under a grant of the
//! one read, sees that bot's count and its oldest message's moment and nothing of
//! the mail itself. Reading the count moves nothing: the bot then takes delivery
//! through its own tools, and the controller's next read says the box is empty.

use jojobot_domain::app::operations::OperationSpec;
use serde_json::json;

use super::dsl::Story;

const APP: &str = "springfield-watch";

fn row_of<'a>(counted: &'a serde_json::Value, bot: &str) -> &'a serde_json::Value {
    counted["boxes"]
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["bot"] == bot))
        .unwrap_or_else(|| panic!("a row for {bot}: {counted}"))
}

#[tokio::test]
async fn a_controller_sees_a_bot_has_mail_waiting_then_sees_it_taken() {
    let (story, token) = Story::begin_serving_an_app(
        "bot:marge",
        APP,
        vec![OperationSpec {
            operation: "mailbox_counts".into(),
            kinds: vec![],
            keys: vec![],
        }],
    )
    .await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;

    // ── 1. nothing waits: the box is there, with no age ─────────────────────
    let (status, quiet) = story.app_calls(&token, "mailbox_counts", json!({})).await;
    assert_eq!(status, 200, "{quiet}");
    assert_eq!(row_of(&quiet, "bot:gamma")["new"], 0, "{quiet}");
    assert!(row_of(&quiet, "bot:gamma")["oldest_new_sent_at"].is_null());

    // ── 2. a colleague leaves mail; the controller sees a count and an age ──
    s.call(
        "post_message",
        json!({"to": "bot:gamma", "body": "Please read the card.", "subject": "card"}),
    )
    .await;
    let (status, waiting) = story.app_calls(&token, "mailbox_counts", json!({})).await;
    assert_eq!(status, 200, "{waiting}");
    let row = row_of(&waiting, "bot:gamma");
    assert_eq!(row["new"], 1, "{waiting}");
    assert!(row["oldest_new_sent_at"].is_string(), "{waiting}");
    assert_eq!(
        row.as_object().map(|keys| keys.len()),
        Some(4),
        "a row says whose box, how many wait, how many cards were quarantined and since when, and nothing of the mail: {row}"
    );

    // ── 3. reading the counts moved nothing: the bot still has the mail ─────
    let delivered = gamma.call("read_mailbox", json!({})).await.json();
    assert_eq!(
        delivered["messages"].as_array().map(Vec::len),
        Some(1),
        "the message is still there to be delivered: {delivered}"
    );

    // ── 4. once taken, the controller's next read says the box is empty ─────
    let message = delivered["messages"][0]["id"].clone();
    gamma
        .call("mark_processed", json!({"message_id": message}))
        .await;
    let (status, taken) = story.app_calls(&token, "mailbox_counts", json!({})).await;
    assert_eq!(status, 200, "{taken}");
    assert_eq!(row_of(&taken, "bot:gamma")["new"], 0, "{taken}");
    assert!(row_of(&taken, "bot:gamma")["oldest_new_sent_at"].is_null());
    story.finish().await;
}
