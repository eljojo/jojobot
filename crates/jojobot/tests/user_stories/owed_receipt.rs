//! "Write down the day, and be told the thing now falls due on it."
//!
//! A write that gives a thing a day to fall due on changes what is owed, and
//! the caller who wrote it should hear that in the answer to the write, not
//! learn it by asking later. A day written onto the wrong thing is the mistake
//! this makes visible at the moment it can be seen. **The line a write ends with
//! names the thing and the day when the write sets a due moment, and says
//! nothing about the due moment when the write moved none or took one off.**
//! Taking one off is not reported: the same move covers a schedule that cannot
//! be read, and a thing with an unreadable schedule is owed.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

/// A project with one piece of work under it, owing nothing yet.
async fn a_piece_of_work(s: &Session) {
    s.add("project:atlas", "The Campaign").await;
    s.add_under("project:atlas", "work:sigma", "The Rigging")
        .await;
}

/// The line a write ends with, the way a caller reads it.
fn the_line(receipt: &Answer) -> String {
    receipt.json()["postcondition"]
        .as_str()
        .unwrap_or_else(|| panic!("a write ends with a line: {}", receipt.json()))
        .to_string()
}

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
    let line = the_line(&set);
    assert!(
        line.contains("work:sigma") && line.contains("2026-11-01"),
        "the line names the thing and the day: {line}"
    );
    assert!(
        set.json().get("owed").is_none(),
        "the day rides in the line, not in a field of its own: {}",
        set.json()
    );

    // An ordinary write moves nothing, so its line does not mention a day.
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
        !the_line(&ordinary).contains("2026-11-01"),
        "{}",
        ordinary.json()
    );

    // Finishing the work takes the day off, and the line does not claim
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
    let line = the_line(&done);
    assert!(!line.contains("2026-11-01"), "{line}");
    assert!(!line.contains("owed"), "{line}");
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
            json!({"address": address, "fields": {"decide_by": "2026-11-01"}}),
        )
        .await;
    let line = the_line(&set);
    assert!(
        line.contains("work:sigma") && line.contains("2026-11-01"),
        "the line names the thing and the day: {line}"
    );
    assert!(
        set.json().get("owed").is_none(),
        "the day rides in the line, not in a field of its own: {}",
        set.json()
    );

    let ordinary = s
        .call(
            "update_fact",
            json!({"address": address, "details": "decided with the crew"}),
        )
        .await;
    assert!(
        !the_line(&ordinary).contains("2026-11-01"),
        "{}",
        ordinary.json()
    );

    let done = s
        .call(
            "update_fact",
            json!({"address": address, "fields": {"status": "done"}}),
        )
        .await;
    let line = the_line(&done);
    assert!(!line.contains("2026-11-01"), "{line}");
    assert!(!line.contains("owed"), "{line}");
    story.finish().await;
}
