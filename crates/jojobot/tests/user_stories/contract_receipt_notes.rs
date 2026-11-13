//! **This file is a contract, not a story.**
//!
//! A receipt teaches its explanations once, using the session ledger.
use super::dsl::Story;
use serde_json::json;

#[tokio::test]
async fn lean_receipts_teach_read_and_revision_notes_once_per_session() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:alpha", "Alpha").await;
    let first = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess"}),
        )
        .await
        .json();
    let how = first["how_to_read"]
        .as_str()
        .expect("first read instruction")
        .to_string();
    let second = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays go"}),
        )
        .await
        .json();
    assert!(second.get("how_to_read").is_none(), "{second}");
    let address = first["address"].as_str().expect("claim address");
    let edited = s
        .call(
            "update_fact",
            json!({"address":address, "content":"plays cards"}),
        )
        .await
        .json();
    assert!(edited.get("how_to_read").is_none(), "{edited}");
    assert!(edited["revision_note"].is_string(), "{edited}");
    let again = s
        .call(
            "update_fact",
            json!({"address":address, "content":"plays darts"}),
        )
        .await
        .json();
    assert!(again.get("revision_note").is_none(), "{again}");
    assert!(
        again["revision"].is_number(),
        "the new revision remains: {again}"
    );
    s.wrap("Finished the first receipt session.").await;
    let fresh = story.session().await;
    let other = fresh
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays billiards"}),
        )
        .await
        .json();
    assert_eq!(other["how_to_read"], how, "{other}");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_replay_teaching_after_a_lost_answer() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:alpha", "Alpha").await;
    let args =
        json!({"subject":"person:alpha", "content":"plays chess", "request_id":"alpha-claim"});
    let first = s.call("capture", args.clone()).await.json();
    assert!(first["how_to_read"].is_string(), "{first}");
    assert!(first["teaching"].is_array(), "{first}");
    let again = s.call("capture", args).await.json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["address"], first["address"], "{again}");
    assert_eq!(again["how_to_read"], first["how_to_read"], "{again}");
    assert_eq!(again["teaching"], first["teaching"], "{again}");
    let next = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays go"}),
        )
        .await
        .json();
    assert!(next.get("how_to_read").is_none(), "{next}");
    assert!(next.get("teaching").is_none(), "{next}");
    story.finish().await;
}

#[tokio::test]
async fn lean_receipts_keep_each_calls_warning_while_teaching_the_static_note_once() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-23").await;
    let s = story.session_on("2026-10-23", None).await;
    s.add("person:alpha", "Alpha").await;
    let first = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays chess", "recorded_at":"2026-10-20"}),
        )
        .await
        .json();
    assert!(first["recorded_at_note"].is_string(), "{first}");
    assert!(first["how_to_read"].is_string(), "{first}");
    let second = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays go", "recorded_at":"2026-10-20"}),
        )
        .await
        .json();
    assert_eq!(
        second["recorded_at_note"], first["recorded_at_note"],
        "{second}"
    );
    assert!(second.get("how_to_read").is_none(), "{second}");
    assert!(second["address"].is_string(), "{second}");
    let matching = s
        .call(
            "capture",
            json!({"subject":"person:alpha", "content":"plays cards", "recorded_at":"2026-10-23"}),
        )
        .await
        .json();
    assert!(matching.get("recorded_at_note").is_none(), "{matching}");
    assert!(matching["address"].is_string(), "{matching}");
    story.finish().await;
}
