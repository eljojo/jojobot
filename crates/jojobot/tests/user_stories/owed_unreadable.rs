//! "Which of my things are owed? Including the one whose schedule is broken."
//!
//! A thing that should fall due and cannot say when is late and loud, and the
//! read that asks what is owed has to bring it back, first, with the reason it
//! cannot say. **The read is taught as one selection**: `fields: [{"key":
//! "due_on"}]` and `overdue: {}`. A key filter is an AND, so a thing that holds
//! no stored due day never reaches the part of the read that would sort it
//! first. Every negative here sits beside the positive it depends on, in the same
//! read: a read that brought back nothing passes "the broken one is not
//! hidden" on a build that finds nothing owed.

use serde_json::{Value, json};

use super::dsl::{Answer, Session, Story};

/// A thing with a loop's keys, one of which holds no number.
const BROKEN_SCHEDULE: &str =
    r#"{"cadence_days": "often", "advances_from": "due_date", "counts_from": "2026-06-01"}"#;

/// The owed read, exactly as the rhythms skill teaches it.
async fn what_is_owed(s: &Session) -> Answer {
    s.shape(
        "what is owed",
        json!({"fields": [{"key": "due_on"}], "overdue": {}}),
    )
    .await
}

fn handles(owed: &Answer) -> Vec<String> {
    owed.json()["objects"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|o| o["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn the_object<'a>(owed: &'a Value, handle: &str) -> &'a Value {
    owed["objects"]
        .as_array()
        .and_then(|all| all.iter().find(|o| o["id"] == handle))
        .unwrap_or_else(|| panic!("{handle} is in the answer: {owed}"))
}

/// **A thing whose schedule cannot be read is owed, first, and says why.**
/// Beside it a thing with a plain day, so the read is shown to find what has a
/// day and to put the broken one ahead of it.
#[tokio::test]
async fn a_thing_whose_schedule_cannot_be_read_is_owed_first_with_its_reason() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:fern", "The Fern").await;
    s.add("thing:wrench", "The Wrench").await;
    s.event_with(
        "thing:fern",
        "watered on a schedule nobody wrote down properly",
        serde_json::from_str(BROKEN_SCHEDULE).expect("fields"),
        &[],
    )
    .await;
    s.event_with(
        "thing:wrench",
        "has to go back",
        json!({"runs_out": "2026-08-01"}),
        &[],
    )
    .await;

    let owed = what_is_owed(&s).await;
    assert_eq!(
        handles(&owed),
        vec!["thing:fern", "thing:wrench"],
        "both are owed, the one that cannot say when first: {}",
        owed.json()
    );
    let body = owed.json();
    let fern = the_object(&body, "thing:fern");
    assert!(
        fern["unreadable_because"]
            .to_string()
            .contains("cadence_days"),
        "the broken one says which key it cannot be read from: {fern}"
    );
    let wrench = the_object(&body, "thing:wrench");
    assert!(
        wrench.get("unreadable_because").is_none(),
        "a dated one carries no reason: {wrench}"
    );
    story.finish().await;
}

/// **The day a schedule became unreadable is kept, so the thing's lateness
/// keeps counting.** A later write on another day, to a thing whose schedule is
/// still broken, leaves the stored day where it was. Beside it a thing whose
/// schedule broke on that later day, so the read is shown to carry a day it
/// stamped itself.
#[tokio::test]
async fn a_later_write_leaves_the_day_a_schedule_became_unreadable() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session_on("2026-09-01", None).await;
    first.add("thing:fern", "The Fern").await;
    first
        .event_with(
            "thing:fern",
            "watered on a schedule nobody wrote down properly",
            serde_json::from_str(BROKEN_SCHEDULE).expect("fields"),
            &[],
        )
        .await;

    let later = story.session_on("2026-09-20", Some("new")).await;
    later.add("thing:omega", "The Omega").await;
    later
        .event_with(
            "thing:fern",
            "still not written down properly",
            json!({"cadence_days": "frequently"}),
            &[],
        )
        .await;
    later
        .event_with(
            "thing:omega",
            "also not written down properly",
            serde_json::from_str(BROKEN_SCHEDULE).expect("fields"),
            &[],
        )
        .await;

    let owed = what_is_owed(&later).await;
    let body = owed.json();
    assert_eq!(
        the_object(&body, "thing:omega")["fields"]["due_on"],
        "2026-09-20",
        "a schedule that broke today is stamped today: {body}"
    );
    assert_eq!(
        the_object(&body, "thing:fern")["fields"]["due_on"],
        "2026-09-01",
        "a schedule that broke earlier keeps the day it broke: {body}"
    );
    story.finish().await;
}
