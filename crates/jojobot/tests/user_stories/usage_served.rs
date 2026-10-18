//! "How much was each bot served each day this week?"
//!
//! **The answer is exact.** Every answer a run is handed is counted on the day the
//! run was in when it was served, in the same step that adds it to the run's
//! total. A usage week carries one number per day that had any, so a run that is
//! served across a week boundary puts each day in its own week, and the weeks add
//! up to what the run was handed over its life.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

fn field(record: &Value, key: &str) -> Option<String> {
    record["fields"][key].as_str().map(str::to_string)
}

fn number(record: &Value, key: &str) -> u64 {
    field(record, key)
        .unwrap_or_else(|| panic!("{key} on {record}"))
        .parse()
        .unwrap_or_else(|_| panic!("{key} is a number on {record}"))
}

/// The run writes once, which gives it a record to be served on, and then reads:
/// every answer it is handed counts towards its total.
async fn is_served(who: &Session, reads: usize) {
    who.journal("working").await;
    for _ in 0..reads {
        who.call("recall", json!({"subject": "person:lisa", "facts": true}))
            .await;
    }
}

/// What the run has been served over its whole life, as it lists its own
/// runs, asked with nothing else asked of the run in between.
async fn held_by(who: &Session) -> u64 {
    let listed = who.call("list_runs", json!({})).await.json();
    listed["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["run"] == who.run()))
        .and_then(|run| run["served_lifetime"]["characters"].as_u64())
        .unwrap_or_else(|| panic!("the run lists what it was served: {listed}"))
}

async fn week(reader: &Session, handle: &str) -> Value {
    let read = reader
        .call("recall", json!({"subject": handle}))
        .await
        .json();
    read["objects"][0].clone()
}

#[tokio::test]
async fn a_run_served_across_a_week_boundary_is_counted_exactly_on_each_day() {
    // ── Sunday 2026-10-11, the last day of ISO week 41 ──────────────────────
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-11").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:milhouse", "Milhouse").await;
    let bart = story.as_bot("bot:bart").await;
    is_served(&bart, 3).await;
    let marge = story.as_bot("bot:marge").await;
    is_served(&marge, 2).await;

    // ── Monday 2026-10-12, the first day of week 42: the same run goes on ───
    let later = story.restarted_pretending_it_is("2026-10-12").await;
    let bart = later.session_carried_from(&bart).await;
    is_served(&bart, 2).await;
    let milhouse = later.as_bot("bot:milhouse").await;
    is_served(&milhouse, 1).await;

    // ── the head of the chart reads the weeks, then Bart says what it holds ─
    let assistant = later.as_bot("bot:assistant").await;
    let sunday = week(&assistant, "usage:bart-2026-w41").await;
    let monday = week(&assistant, "usage:bart-2026-w42").await;
    let lifetime = held_by(&bart).await;

    let on_sunday = number(&sunday, "served_2026-10-11");
    let on_monday = number(&monday, "served_2026-10-12");
    assert!(on_sunday > 0 && on_monday > 0, "{sunday} {monday}");
    assert_eq!(
        on_sunday + on_monday,
        lifetime,
        "the two days are everything the run was handed"
    );
    // Each week holds its own day and the total of its days, and no other day.
    assert_eq!(number(&sunday, "served_chars"), on_sunday, "{sunday}");
    assert_eq!(number(&monday, "served_chars"), on_monday, "{monday}");
    assert_eq!(field(&sunday, "served_2026-10-12"), None, "{sunday}");
    assert_eq!(field(&monday, "served_2026-10-11"), None, "{monday}");
    assert_eq!(field(&sunday, "served_basis").as_deref(), Some("day"));
    assert_eq!(field(&monday, "served_basis").as_deref(), Some("day"));
    // The count is exact from the first day any answer was counted for the bot.
    assert_eq!(
        field(&monday, "served_counted_from").as_deref(),
        Some("2026-10-11"),
        "{monday}"
    );

    // ── another bot's served stays in its own weeks ─────────────────────────
    let marge_week = week(&assistant, "usage:marge-2026-w41").await;
    assert!(number(&marge_week, "served_2026-10-11") > 0, "{marge_week}");
    assert_eq!(
        number(&marge_week, "served_chars"),
        number(&marge_week, "served_2026-10-11")
    );
    assert_eq!(
        field(&marge_week, "served_counted_from").as_deref(),
        Some("2026-10-11"),
        "{marge_week}"
    );
    // A bot first served a day later starts its own count a day later.
    let milhouse_week = week(&assistant, "usage:milhouse-2026-w42").await;
    assert_eq!(
        field(&milhouse_week, "served_counted_from").as_deref(),
        Some("2026-10-12"),
        "{milhouse_week}"
    );
    let missing = assistant
        .refused("recall", json!({"subject": "usage:marge-2026-w42"}))
        .await;
    missing.says("\"fix_by\":\"change\"");

    // ── the reading is the same after the store changed underneath ──────────
    later.the_store_changed_underneath().await;
    let again = week(&assistant, "usage:bart-2026-w41").await;
    assert_eq!(number(&again, "served_chars"), on_sunday, "{again}");
    later.finish().await;
}
