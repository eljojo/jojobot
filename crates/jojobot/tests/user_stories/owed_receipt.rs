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
            json!({"address": address, "sets": {"decide_by": "2026-11-01"}}),
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

/// **A thing made with its day says it falls due on it, and a thing made
/// without one says nothing.** A creation carries no line of its own when the
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
    let line = the_line(&made);
    assert!(
        line.contains("work:sigma") && line.contains("2026-11-01"),
        "the line names the thing and the day: {line}"
    );
    assert!(
        made.json().get("owed").is_none(),
        "the day rides in the line, not in a field of its own: {}",
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
    let line = the_line(&set);
    assert!(
        line.contains("work:sigma") && line.contains("2026-11-01"),
        "the line names the thing and the day: {line}"
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

/// The thing's folded `due_on`, the way a caller reads it back.
async fn due_on_of(s: &Session, thing: &str) -> Option<String> {
    s.recall(thing).await.json()["objects"][0]["fields"]["due_on"]
        .as_str()
        .map(str::to_string)
}

/// **Finishing a thing through `update_entity` takes its stored due day off,
/// as finishing it through a claim does.** The update worked out that the day
/// should go and then did nothing about it, so the stored day stayed on a
/// finished thing. The pair beside it: a status that is not done keeps the day,
/// so a clear that fired on every status change would pass the finish alone.
#[tokio::test]
async fn finishing_a_thing_through_update_entity_takes_its_due_day_off() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_piece_of_work(&s).await;
    s.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"decide_by": "2026-11-01"}}),
    )
    .await;
    assert_eq!(
        due_on_of(&s, "work:sigma").await.as_deref(),
        Some("2026-11-01"),
        "the day is stored on the thing before it is finished"
    );

    // A status that is not done leaves the day where it was.
    s.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "now"}}),
    )
    .await;
    assert_eq!(
        due_on_of(&s, "work:sigma").await.as_deref(),
        Some("2026-11-01"),
        "a thing moved to now still falls due"
    );

    s.call(
        "update_entity",
        json!({"handle": "work:sigma", "sets": {"status": "done"}}),
    )
    .await;
    assert_eq!(
        due_on_of(&s, "work:sigma").await,
        None,
        "a finished thing holds no due day"
    );
    story.finish().await;
}

/// **A thing made already finished holds no due day**, so a creation has no
/// stored day to take off. The positive beside it: the same creation without
/// the finish holds the day.
#[tokio::test]
async fn a_thing_made_already_finished_holds_no_due_day() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    for (handle, name, status, held) in [
        ("tau", "The Mast", "next", true),
        ("upsilon", "The Boom", "done", false),
    ] {
        s.call(
            "add_entity",
            json!({
                "kind": "work", "handle": handle, "name": name,
                "source": "user-named", "parent": "project:atlas",
                "sets": {"decide_by": "2026-11-01", "status": status},
            }),
        )
        .await;
        assert_eq!(
            due_on_of(&s, &format!("work:{handle}")).await.is_some(),
            held,
            "{handle} made at {status}"
        );
    }
    story.finish().await;
}
