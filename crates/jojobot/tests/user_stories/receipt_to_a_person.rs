//! "I wrote to the operator. Did it go, and can another bot see that I did?"
//!
//! The operator's box is read by nobody but the operator. The bot that wrote a
//! message there reads where its own mail got to, as a receipt with no words in
//! it. A bot beside it is not told the message exists.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_writer_of_a_message_to_the_operator_reads_its_receipt_and_another_bot_does_not() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    s.add("bot:sigma", "Sigma").await;
    s.add("bot:marge", "Marge").await;
    let sigma = story.as_bot("bot:sigma").await;
    let marge = story.as_bot("bot:marge").await;

    let id = sigma
        .post(
            "person:lisa",
            "the quarterly figure",
            "the oven is preheated",
        )
        .await;

    // ── the writer sees it in its outbox, with no words of it ───────────────
    let mine = sigma.call("list_sent", json!({})).await.json();
    let receipt = mine["messages"]
        .as_array()
        .expect("messages")
        .iter()
        .find(|message| message["id"] == id.as_str())
        .unwrap_or_else(|| panic!("the writer does not see its own message: {mine}"));
    assert!(receipt.get("body").is_none(), "{receipt}");
    assert!(receipt.get("body_head").is_none(), "{receipt}");

    // ── a bot beside it asks for that sender's mail and is told nothing of it
    let beside = marge
        .call("list_sent", json!({"sender": "bot:sigma"}))
        .await
        .json();
    assert!(
        beside["messages"]
            .as_array()
            .expect("messages")
            .iter()
            .all(|message| message["id"] != id.as_str()),
        "the message to the operator is not listed to another bot: {beside}"
    );
    assert!(
        beside["withheld"].as_u64().is_some_and(|count| count >= 1),
        "it is counted as withheld: {beside}"
    );

    marge.wrap("asked for a sibling's mail").await;
    sigma.wrap("wrote to the operator").await;
    s.wrap("stood two bots up").await;
    story.finish().await;
}
