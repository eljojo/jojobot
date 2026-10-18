//! "What did my last run cost, and what did it call and get refused?"
//!
//! **The answer comes from the run's own record.** A run lists two figures for
//! what it was served, each named for what it counts: the characters since its
//! last compaction, and the characters over its whole life. It also says how many
//! compactions it reported. Reading the one run adds the days it was served on,
//! and those days add up to the whole-life figure.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The run writes once, which gives it a record to be served on, and then reads:
/// every answer it is handed counts towards its total.
async fn is_served(who: &Session, reads: usize) {
    who.journal("working").await;
    for _ in 0..reads {
        who.call("recall", json!({"subject": "person:lisa", "facts": true}))
            .await;
    }
}

fn characters(figure: &Value) -> u64 {
    figure["characters"]
        .as_u64()
        .unwrap_or_else(|| panic!("a figure of what was served on {figure}"))
}

fn field_number(record: &Value, key: &str) -> u64 {
    record["fields"][key]
        .as_str()
        .unwrap_or_else(|| panic!("{key} on {record}"))
        .parse()
        .unwrap_or_else(|_| panic!("{key} is a number on {record}"))
}

#[tokio::test]
async fn a_run_reads_what_it_was_served_over_its_life_and_since_its_compaction() {
    // ── Sunday: Bart is served, then reports one compaction ─────────────────
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-11").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    is_served(&bart, 3).await;
    bart.call("start_here", json!({"compacted": true})).await;

    // ── Monday: the same run goes on and is served again ────────────────────
    let later = story.restarted_pretending_it_is("2026-10-12").await;
    let bart = later.session_carried_from(&bart).await;
    is_served(&bart, 2).await;

    // ── the run lists its own cost ───────────────────────────────────────────
    let listed = bart.call("list_runs", json!({})).await.json();
    let run = listed["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["run"] == bart.run()))
        .unwrap_or_else(|| panic!("the run lists itself: {listed}"))
        .clone();
    assert_eq!(run["compactions"], 1, "{run}");
    let since = characters(&run["served_since_compaction"]);
    let whole = characters(&run["served_lifetime"]);
    assert!(since > 0, "{run}");
    assert!(
        whole > since,
        "the whole life holds what the compaction set back: {run}"
    );
    assert!(
        run.get("served").is_none(),
        "a figure that does not say what it counts is gone: {run}"
    );

    // ── the one run, read, adds its days and they add up to its life ────────
    let one = bart
        .call(
            "recall",
            json!({"subject": format!("session:{}", bart.run())}),
        )
        .await
        .json();
    let record = &one["objects"][0];
    assert_eq!(field_number(record, "compactions"), 1, "{record}");
    let on_sunday = field_number(record, "served_2026-10-11");
    let on_monday = field_number(record, "served_2026-10-12");
    assert!(on_sunday > 0 && on_monday > 0, "{record}");
    assert_eq!(
        field_number(record, "served_lifetime"),
        on_sunday + on_monday,
        "the days are everything the run was handed: {record}"
    );
    assert!(
        field_number(record, "served_lifetime") > field_number(record, "served_since_compaction"),
        "{record}"
    );

    later.finish().await;
}
