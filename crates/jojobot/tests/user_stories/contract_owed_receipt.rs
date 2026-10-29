//! **This file is a contract, not a story.**
//!
//! A write reports a derived due day once, beside the thing's identity.
//! Ordinary writes carry no due day. All four write doors use the same shape.

use serde_json::json;

use super::dsl::Story;
use super::owed_receipt::a_piece_of_work;

/// **A capture that sets the day says the thing now falls due on it.**
#[tokio::test]
async fn a_capture_that_sets_a_day_says_the_thing_falls_due_on_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_piece_of_work(&s).await;

    let set = s
        .call(
            "capture",
            json!({
                "subject": "work:sigma", "content": "the rigging has to be decided by November",
                "provenance": "testimony",
                "sets": {"decide_by": "2026-11-01"},
            }),
        )
        .await;
    let body = set.json();
    assert_eq!(body["falls_due"], true, "{body}");
    assert!(
        body["id"] == "work:sigma" && body["derived"]["due_on"] == "2026-11-01",
        "the receipt names the thing and its derived day: {body}"
    );
    assert!(
        set.json().get("owed").is_none(),
        "the receipt has one place for its derived day: {}",
        set.json()
    );

    // An ordinary write moves nothing, so its receipt does not repeat the day.
    let ordinary = s
        .call(
            "capture",
            json!({
                "subject": "work:sigma", "content": "the crew liked the rigging",
                "provenance": "testimony",
            }),
        )
        .await;
    assert!(
        ordinary.json()["derived"].get("due_on").is_none(),
        "{}",
        ordinary.json()
    );

    // Finishing the work takes the day off, and the receipt does not claim
    // anything about what is owed.
    let done = s
        .call(
            "capture",
            json!({
                "subject": "work:sigma", "content": "the rigging is done",
                "provenance": "testimony",
                "sets": {"status": "done"},
            }),
        )
        .await;
    let body = done.json();
    assert!(body["derived"]["due_on"].is_null(), "{body}");
    story.finish().await;
}

/// **An edit that sets the day says the thing now falls due on it**, and an
/// edit that finishes the work says nothing about the due moment.
#[tokio::test]
async fn an_edit_that_sets_a_day_says_the_thing_falls_due_on_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_piece_of_work(&s).await;
    let claim = s
        .call(
            "capture",
            json!({
                "subject": "work:sigma", "content": "the rigging needs a decision",
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    let address = claim["address"].as_str().expect("an address").to_string();

    let set = s
        .call(
            "update_fact",
            json!({"address": address, "sets": {"decide_by": "2026-11-01"}}),
        )
        .await;
    let body = set.json();
    assert_eq!(body["falls_due"], true, "{body}");
    assert!(
        body["id"] == "work:sigma" && body["derived"]["due_on"] == "2026-11-01",
        "the receipt names the thing and its derived day: {body}"
    );
    assert!(
        set.json().get("owed").is_none(),
        "the receipt has one place for its derived day: {}",
        set.json()
    );

    let ordinary = s
        .call(
            "update_fact",
            json!({"address": address, "details": "decided with the crew"}),
        )
        .await;
    assert!(
        ordinary.json()["derived"].get("due_on").is_none(),
        "{}",
        ordinary.json()
    );

    let done = s
        .call(
            "update_fact",
            json!({"address": address, "fields": {"status": "done"}}),
        )
        .await;
    let body = done.json();
    assert!(body["derived"]["due_on"].is_null(), "{body}");
    story.finish().await;
}

/// **A thing made with its day says it falls due on it, and a thing made
/// without one says nothing.** A creation carries no due value when the
/// day did not move.
#[tokio::test]
async fn a_creation_that_sets_a_day_says_the_thing_falls_due_on_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;

    let made = s
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "sigma", "name": "The Rigging",
                "source": "user-named", "parent": "project:atlas",
                "sets": {"decide_by": "2026-11-01"},
            }),
        )
        .await;
    let body = made.json();
    assert_eq!(body["falls_due"], true, "{body}");
    assert!(
        body["id"] == "work:sigma" && body["derived"]["due_on"] == "2026-11-01",
        "the receipt names the thing and its derived day: {body}"
    );
    assert!(
        made.json().get("owed").is_none(),
        "the receipt has one place for its derived day: {}",
        made.json()
    );

    let plain = s
        .call(
            "add_entity",
            json!({
                "kind": "work", "handle": "tau", "name": "The Mast",
                "source": "user-named", "parent": "project:atlas",
            }),
        )
        .await;
    assert!(
        plain.json().get("postcondition").is_none(),
        "a creation that moved no day has no line: {}",
        plain.json()
    );
    story.finish().await;
}

/// **Setting the day on a thing that exists says it falls due on it, and
/// setting something else says nothing.**
#[tokio::test]
async fn setting_a_day_on_a_thing_says_it_falls_due_on_it() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_piece_of_work(&s).await;

    let set = s
        .call(
            "update_entity",
            json!({"handle": "work:sigma", "sets": {"decide_by": "2026-11-01"}}),
        )
        .await;
    let body = set.json();
    assert_eq!(body["falls_due"], true, "{body}");
    assert!(
        body["id"] == "work:sigma" && body["derived"]["due_on"] == "2026-11-01",
        "the receipt names the thing and its derived day: {body}"
    );

    let ordinary = s
        .call(
            "update_entity",
            json!({"handle": "work:sigma", "sets": {"crew": "four"}}),
        )
        .await;
    assert!(
        ordinary.json().get("postcondition").is_none(),
        "an update that moved no day has no line: {}",
        ordinary.json()
    );
    story.finish().await;
}
