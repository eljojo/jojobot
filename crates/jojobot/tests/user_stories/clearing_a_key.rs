//! "Take that key off Homer."
//!
//! **A caller finds the record to clear by following one sentence**, the one
//! `update_fact` serves on `clear_fields`. It has to be true for every key a
//! caller can ask about, and the keys fold two ways: the newest write wins, or
//! a counter adds its writes up.

use serde_json::json;

use super::dsl::Story;

/// A word the sentence names, not a substring of one.
fn names(text: &str, word: &str) -> bool {
    text.split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .any(|w| w == word)
}

/// **A key that keeps its newest write is cleared by the record `backing`
/// names.** The sentence sends the caller to `backing`, the caller follows it,
/// and the key comes off the thing.
#[tokio::test]
async fn a_key_that_keeps_its_newest_write_is_cleared_by_following_the_sentence() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;

    s.call(
        "declare_type",
        json!({
            "name": "snacking",
            "fields": [{ "key": "donuts", "folds": "sum" }, { "key": "mood" }],
        }),
    )
    .await;
    for mood in ["content", "queasy"] {
        s.event_with(
            "person:homer",
            &format!("felt {mood} after the plant"),
            json!({ "mood": mood }),
            &[],
        )
        .await;
    }

    let sentence = s.argument_description("update_fact", "clear_fields").await;
    assert!(
        names(&sentence, "backing"),
        "the sentence names backing: {sentence}"
    );

    // ── follow it: the thing, with backing, naming the key ──────────────────
    let backed = s
        .shape(
            "which record backs Homer's mood",
            json!({ "subject": "person:homer", "keys": ["mood"], "backing": true }),
        )
        .await;
    backed.says("\"mood\":\"queasy\"");
    let record = backed.json()["objects"][0]["fields_backing"]["mood"]["claim"]
        .as_str()
        .expect("backing names the record that holds the key")
        .to_string();

    s.call(
        "update_fact",
        json!({ "address": format!("person:homer#{record}"), "clear_fields": ["mood"] }),
    )
    .await;

    // The positive the negative rests on: the other key is still there.
    let after = s
        .shape("what Homer holds now", json!({ "subject": "person:homer" }))
        .await;
    after.never_says("\"mood\":\"queasy\"");

    story.finish().await;
}

/// **A counter has no single record behind it, and the sentence still reaches
/// one.** `backing` names no record for a counter key, so the route that works
/// for the key above ends there. The sentence has to send the caller to the
/// history of the key, which lists every record that wrote it.
#[tokio::test]
async fn a_counter_key_is_cleared_by_following_the_sentence() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;

    s.call(
        "declare_type",
        json!({
            "name": "snacking",
            "fields": [{ "key": "donuts", "folds": "sum" }, { "key": "mood" }],
        }),
    )
    .await;
    for eaten in ["at the plant", "on the drive home", "in front of the tv"] {
        s.event_with(
            "person:homer",
            &format!("a donut {eaten}"),
            json!({ "donuts": "1" }),
            &[],
        )
        .await;
    }

    let sentence = s.argument_description("update_fact", "clear_fields").await;
    assert!(
        names(&sentence, "history"),
        "the sentence names the history of a key for a counter: {sentence}"
    );

    // ── the dead end the old sentence led to ────────────────────────────────
    let backed = s
        .shape(
            "which record backs Homer's donuts",
            json!({ "subject": "person:homer", "keys": ["donuts"], "backing": true }),
        )
        .await;
    backed.says("\"donuts\":\"3\"");
    backed.never_says("person:homer#f");

    // ── the route the sentence offers ───────────────────────────────────────
    let occasions = s
        .shape(
            "every record that wrote Homer's donuts",
            json!({ "subject": "person:homer", "history": "donuts" }),
        )
        .await;
    occasions.number("/objects/0/history/count", 3);
    occasions.says("person:homer#f2");

    s.call(
        "update_fact",
        json!({ "address": "person:homer#f2", "clear_fields": ["donuts"] }),
    )
    .await;

    // Clearing the key on one record ends the total, and the thing holds no
    // count at all. The claims that wrote it are still there, with the clear
    // among them.
    let after = s
        .shape("what Homer holds now", json!({ "subject": "person:homer" }))
        .await;
    after.never_says("\"donuts\":");
    let writes = s
        .shape(
            "every write of Homer's donuts, the clear included",
            json!({ "subject": "person:homer", "history": "donuts" }),
        )
        .await;
    writes.number("/objects/0/history/count", 4);
    writes.says("\"cleared\":true");

    story.finish().await;
}
