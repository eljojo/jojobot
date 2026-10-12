//! "Which loops fall due this week?" — asked of every kind, by the day jojobot
//! itself works out.
//!
//! **A loop's next due day is computed by jojobot and stored on the loop**, with
//! the day of its last check-in and the day its next cycle counts from. They are
//! dates the software writes, so the software declares them, and a question that
//! names no kind can order them: `due_on <= today+7d` runs, and is not refused
//! for want of a declaration.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

fn handles(answer: &Value) -> Vec<String> {
    let mut found: Vec<String> = answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists no objects: {answer}"))
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect();
    found.sort();
    found
}

/// Two loops on one fern: one checked in a week ago on a ten-day cadence, so it
/// falls due the day after tomorrow; one on a ninety-day cadence, due in months.
async fn two_loops(s: &Session) {
    s.add("thing:the-fern", "The Fern").await;
    s.add_under("thing:the-fern", "rhythm:water-the-fern", "Water the fern")
        .await;
    s.add_under(
        "thing:the-fern",
        "rhythm:swap-the-air-filter",
        "Swap the air filter",
    )
    .await;
    s.event_with(
        "rhythm:water-the-fern",
        "watered it",
        json!({"name": "Water the fern", "last_check_in": "2026-10-01",
               "cadence_days": "10", "advances_from": "check_in_date", "counts_from": "2026-10-01"}),
        &[],
    )
    .await;
    s.event_with(
        "rhythm:swap-the-air-filter",
        "swapped it",
        json!({"name": "Swap the air filter", "last_check_in": "2026-10-01",
               "cadence_days": "90", "advances_from": "check_in_date", "counts_from": "2026-10-01"}),
        &[],
    )
    .await;
}

/// A thing of no loop kind that carries the same keys. A carrier is found by the
/// keys it holds, so it is owed and computed like a loop; the rhythm kind
/// declares two of these days for itself, and this thing is where only the
/// shipped type does.
async fn a_kiln_on_the_same_schedule(s: &Session) {
    s.add("thing:tau", "The Kiln").await;
    s.event_with(
        "thing:tau",
        "fired it",
        json!({"last_check_in": "2026-10-01", "cadence_days": "10",
               "advances_from": "check_in_date", "counts_from": "2026-10-01"}),
        &[],
    )
    .await;
}

async fn asked(s: &Session, where_: &str) -> Value {
    s.call("recall", json!({ "where": where_ })).await.json()
}

#[tokio::test]
async fn the_day_a_loop_falls_due_can_be_asked_of_every_kind() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-10-09", None).await;
    two_loops(&s).await;
    a_kiln_on_the_same_schedule(&s).await;

    // The positive: the loop that falls due within the week, and not the other.
    let soon = asked(&s, "due_on <= today+7d").await;
    assert_eq!(
        handles(&soon),
        ["rhythm:water-the-fern", "thing:tau"],
        "{soon}"
    );
    // The other side of the same key: the loop due in months is found later.
    let later = asked(&s, "due_on > today+7d").await;
    assert_eq!(handles(&later), ["rhythm:swap-the-air-filter"], "{later}");

    // The other two dates jojobot computes, asked the same way.
    let checked = asked(&s, "last_check_in <= today").await;
    assert_eq!(
        handles(&checked),
        [
            "rhythm:swap-the-air-filter",
            "rhythm:water-the-fern",
            "thing:tau"
        ],
        "{checked}"
    );

    let counted = asked(&s, "counts_from <= today").await;
    assert_eq!(
        handles(&counted),
        [
            "rhythm:swap-the-air-filter",
            "rhythm:water-the-fern",
            "thing:tau"
        ],
        "{counted}"
    );

    story.finish().await;
}
