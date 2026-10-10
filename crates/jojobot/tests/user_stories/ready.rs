//! "What can I start next, and why can't I start the rest?"
//!
//! Four pieces of work, two of them waiting on something. The session asks one
//! named question and is told which can be started now, how many it left out,
//! and the call that says why. It makes that call for one piece of work and is
//! told what stops it. The read works because the question is shipped: nobody
//! declared a view first.
//!
//! **A dependency counts as finished by the same word the rest of the instance
//! uses**, and finishing the one it waited on is what moves the work onto the
//! list.

use serde_json::json;

use super::dsl::Story;

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn what_can_be_started_and_what_stops_the_rest() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    let work = |handle: &'static str, sets: serde_json::Value| {
        let slug = handle.strip_prefix("work:").expect("a work handle");
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": sets})
    };
    s.call("add_entity", work("work:sigma", json!({"status": "done"})))
        .await;
    s.call("add_entity", work("work:tau", json!({"status": "now"})))
        .await;
    s.call(
        "add_entity",
        work(
            "work:phi",
            json!({"status": "next", "depends_on": "work:sigma"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "work:first-mix",
            json!({"status": "next", "depends_on": "work:sigma, work:tau"}),
        ),
    )
    .await;

    // ── the question, by name ───────────────────────────────────────────────
    let ready = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(listed(&ready), ["work:phi", "work:tau"], "{}", ready.raw());
    // Two are left out: the finished one and the one that waits on work in hand.
    let left_out = &ready.json()["ready_left_out"];
    assert_eq!(left_out["count"], 2, "{}", ready.raw());

    // ── the call the count names, for one piece of work ─────────────────────
    let why = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:first-mix"}),
        )
        .await;
    let object = &why.json()["objects"][0];
    assert_eq!(object["ready"], false, "{}", why.raw());
    assert_eq!(
        object["blocked_by"],
        json!([{"blocker": "dependency", "handle": "work:tau", "why": "unfinished"}]),
        "{}",
        why.raw()
    );

    // ── finishing what it waited on moves it onto the list ──────────────────
    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "done", "provenance": "testimony",
               "sets": {"status": "done"}}),
    )
    .await;
    let after = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&after),
        ["work:first-mix", "work:phi"],
        "{}",
        after.raw()
    );

    story.finish().await;
}
