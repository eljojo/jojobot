//! "Did the deploy land, and who said so?"
//!
//! **An outside program records a deploy, and a colleague finds it the way any
//! colleague finds anything.** The app posts over its own route with a token of
//! its own; it holds no session. A bot then reads the event through the tool
//! surface, by recall under the thread the app's grant names and by search for
//! the commit, and sees that an app recorded it. The words are the server's, so a
//! bot cannot rewrite them in place; it archives the claim and captures the
//! correction. A retry of the same attempt answers repeat and writes nothing.

use serde_json::{Value, json};

use super::dsl::Story;

const THREAD: &str = "thread:deploys-alpha";
const APP: &str = "alpha-deployer";

/// The events recall finds under the thread, as the objects it answers.
fn events(read: &Value) -> Vec<Value> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer has objects: {read}"))
        .clone()
}

#[tokio::test]
async fn a_colleague_finds_a_deploy_event_an_app_recorded_and_cannot_rewrite_it() {
    let (story, token) =
        Story::begin_with_an_app("bot:marge", APP, THREAD, &["commit", "host"]).await;
    let s = story.session().await;
    s.add(THREAD, "Deploys of the alpha service").await;

    // ── the app posts, over its own route ───────────────────────────────────
    let body = json!({"attempt_id": "attempt-1",
                      "fields": {"commit": "0a1b2c3", "host": "springfield-ci"}});
    let (status, landed) = story.app_posts(&token, body.clone()).await;
    assert_eq!(status, 201, "the first post is created: {landed}");
    assert_eq!(landed["repeat"], false, "{landed}");
    let handle = landed["handle"]
        .as_str()
        .expect("the event's handle")
        .to_string();
    let address = landed["address"]
        .as_str()
        .expect("the claim's address")
        .to_string();

    // ── a colleague finds it by recall, under the thread ────────────────────
    let read = s
        .call("recall", json!({"parent": THREAD, "facts": true}))
        .await
        .json();
    let found = events(&read);
    assert_eq!(found.len(), 1, "one event under the thread: {read}");
    assert_eq!(found[0]["id"], handle.as_str(), "{read}");
    let claim = &found[0]["facts"][0];
    assert!(
        claim["content"]
            .as_str()
            .is_some_and(|words| words.contains("0a1b2c3")),
        "the claim carries the commit: {claim}"
    );
    // **Who recorded it is the server's mark**, not a word a bot could write.
    assert_eq!(claim["recorded_by"]["app"], APP, "{claim}");
    assert_eq!(
        claim["recorded_by"]["grant"], "record_deploy_event",
        "{claim}"
    );

    // ── and by search for the commit ────────────────────────────────────────
    let searched = s.call("search", json!({"query": "0a1b2c3"})).await.json();
    let hit = searched["results"]
        .as_array()
        .and_then(|hits| hits.iter().find(|hit| hit["address"] == address.as_str()))
        .unwrap_or_else(|| panic!("a search for the commit finds the event: {searched}"));
    assert_eq!(
        hit["recorded_by"]["app"], APP,
        "the hit carries the mark: {hit}"
    );

    // ── a bot cannot rewrite it in place; the refusal names the route ───────
    let refused = s
        .refused(
            "update_fact",
            json!({"address": address, "content": "Deploy event: commit ffffff0",
                   "provenance": "inference"}),
        )
        .await;
    refused.says("archive").says("derived_from");
    let after = s
        .call("recall", json!({"parent": THREAD, "facts": true}))
        .await
        .json();
    assert!(
        events(&after)[0]["facts"][0]["content"]
            .as_str()
            .is_some_and(|words| words.contains("0a1b2c3")),
        "nothing was rewritten: {after}"
    );

    // ── a retry of the same attempt answers repeat and writes nothing ───────
    let (status, again) = story.app_posts(&token, body).await;
    assert_eq!(status, 200, "the retry is answered, not created: {again}");
    assert_eq!(again["repeat"], true, "{again}");
    assert_eq!(again["handle"], handle.as_str(), "{again}");
    let still = s.call("recall", json!({"parent": THREAD})).await.json();
    assert_eq!(events(&still).len(), 1, "still exactly one event: {still}");
    story.finish().await;
}
