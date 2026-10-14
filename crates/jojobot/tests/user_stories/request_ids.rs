//! "My connection dropped before the answer came back. Did that write land?"
//!
//! **A write that carries a `request_id` can be sent again.** If the first call
//! landed, the second one returns the first one's receipt, marked as a repeat,
//! and writes nothing. If the first one wrote nothing, the second one runs.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// How many claims `person:milhouse` holds.
async fn claims(s: &Session) -> usize {
    s.shape(
        "what Milhouse holds",
        json!({ "subject": "person:milhouse", "facts": true }),
    )
    .await
    .json()["objects"][0]["facts"]
        .as_array()
        .map_or(0, Vec::len)
}

fn capture(content: &str, request_id: &str) -> Value {
    json!({
        "subject": "person:milhouse", "content": content,
        "provenance": "testimony", "request_id": request_id,
    })
}

/// The receipt without what a repeat adds to it.
fn without_repeat_marks(mut receipt: Value) -> Value {
    if let Some(body) = receipt.as_object_mut() {
        for key in ["repeat", "repeat_note", "arguments_differed", "first_verb"] {
            body.remove(key);
        }
    }
    receipt
}

#[tokio::test]
async fn a_write_sent_again_returns_the_first_receipt_and_writes_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    // The positive the negative rests on: the first call wrote.
    assert_eq!(claims(&s).await, 1);
    assert!(
        first.get("repeat").is_none(),
        "a first call is not a repeat: {first}"
    );

    let again = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "the answer is marked: {again}");
    assert_eq!(
        without_repeat_marks(again.clone()),
        without_repeat_marks(first.clone()),
        "the answer is the first receipt"
    );
    assert!(
        again.get("arguments_differed").is_none(),
        "the same arguments did not differ: {again}"
    );
    assert_eq!(claims(&s).await, 1, "nothing was written the second time");

    story.finish().await;
}

#[tokio::test]
async fn the_same_id_from_another_bot_writes_normally() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.call("capture", capture("wears glasses", "r1")).await;
    assert_eq!(claims(&s).await, 1);

    let other = story.as_bot("bot:assistant").await;
    let theirs = other
        .call("capture", capture("owns a radio", "r1"))
        .await
        .json();
    assert!(
        theirs.get("repeat").is_none(),
        "another bot's id is its own: {theirs}"
    );
    assert_eq!(claims(&s).await, 2);

    story.finish().await;
}

#[tokio::test]
async fn an_id_sent_with_other_arguments_returns_the_first_receipt_and_says_so() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();

    let again = s
        .call("capture", capture("owns a radio", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["arguments_differed"], true, "{again}");
    assert_eq!(
        without_repeat_marks(again),
        without_repeat_marks(first),
        "the first receipt comes back"
    );
    assert_eq!(claims(&s).await, 1, "the second claim was not written");

    story.finish().await;
}

#[tokio::test]
async fn a_restart_between_the_two_calls_still_recognises_the_repeat() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    let first = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();

    let restarted = story.restarted().await;
    // The same bot, in a run of the new process.
    let s = restarted.as_bot("bot:otto").await;
    let again = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(
        without_repeat_marks(again),
        without_repeat_marks(first),
        "the first receipt survives the restart"
    );
    assert_eq!(claims(&s).await, 1);

    restarted.finish().await;
}

#[tokio::test]
async fn a_write_that_stored_nothing_spends_no_id() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // Milhouse is not known yet: the write is blocked and stores nothing.
    let blocked = s
        .refused("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert!(blocked.get("repeat").is_none(), "{blocked}");

    // The same id, once the subject exists, is a first call.
    s.add("person:milhouse", "Milhouse").await;
    let landed = s
        .call("capture", capture("wears glasses", "r1"))
        .await
        .json();
    assert!(
        landed.get("repeat").is_none(),
        "the blocked call spent nothing: {landed}"
    );
    assert_eq!(claims(&s).await, 1);

    story.finish().await;
}

#[tokio::test]
async fn an_id_needs_a_session_and_a_good_shape() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    // No session: refused, and nothing written.
    let (anonymous, _) = story
        .call_either(
            "capture",
            json!({
                "subject": "person:milhouse", "content": "wears glasses",
                "provenance": "testimony", "request_id": "r1",
            }),
        )
        .await;
    anonymous.says("request_id");
    assert_eq!(claims(&s).await, 0);

    // A shape no id has.
    for bad in ["", " r1", &"x".repeat(129)] {
        s.refused("capture", capture("wears glasses", bad)).await;
    }
    assert_eq!(claims(&s).await, 0);

    // The positive: a good id on the same session lands.
    s.call("capture", capture("wears glasses", "r1")).await;
    assert_eq!(claims(&s).await, 1);

    story.finish().await;
}
