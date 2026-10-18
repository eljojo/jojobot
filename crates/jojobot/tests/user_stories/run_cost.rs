//! "What did my last run cost, and what did it call and get refused?"
//!
//! **The answer comes from the run's own record.** A run lists two figures for
//! what it was served, each named for what it counts: the characters since its
//! last compaction, and the characters over its whole life. It also says how many
//! compactions it reported. Reading the one run adds the days it was served on,
//! and those days add up to the whole-life figure.

use jojobot_domain::session::SessionId;
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

/// Each total a compaction set back, as the run reads them from its own history
/// of what it was served.
async fn resets_of(who: &Session) -> Vec<u64> {
    let history = who
        .call(
            "recall",
            json!({"subject": format!("session:{}", who.run()), "history": "served_chars"}),
        )
        .await
        .json();
    history["objects"][0]["history"]["writes"]
        .as_array()
        .unwrap_or_else(|| panic!("the run's served history: {history}"))
        .iter()
        .filter(|write| write["change"] == "served_reset")
        .map(|write| {
            write["value"]
                .as_str()
                .and_then(|v| v.parse().ok())
                .unwrap_or_else(|| panic!("a total a compaction set back: {write}"))
        })
        .collect()
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
    // ── Sunday: Bart is served, then reports a compaction ───────────────────
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-11").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    is_served(&bart, 3).await;
    bart.call("start_here", json!({"compacted": true})).await;
    // One call that is refused because the call itself must change.
    bart.refused(
        "capture",
        json!({
            "subject": "person:milhouse", "content": "plays the tuba",
            "provenance": "testimony",
        }),
    )
    .await
    .says("\"fix_by\":\"change\"");

    // ── Monday: the same run goes on, is served, reports a second compaction
    // and is served again ────────────────────────────────────────────────────
    let later = story.restarted_pretending_it_is("2026-10-12").await;
    let bart = later.session_carried_from(&bart).await;
    is_served(&bart, 2).await;
    bart.call("start_here", json!({"compacted": true})).await;
    is_served(&bart, 1).await;
    let set_back = resets_of(&bart).await;
    assert_eq!(
        set_back.len(),
        2,
        "two compactions were reported: {set_back:?}"
    );
    assert!(set_back.iter().all(|total| *total > 0), "{set_back:?}");

    // ── the run lists its own cost ───────────────────────────────────────────
    let listed = bart.call("list_runs", json!({})).await.json();
    let run = listed["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["run"] == bart.run()))
        .unwrap_or_else(|| panic!("the run lists itself: {listed}"))
        .clone();
    assert_eq!(run["compactions"], 2, "{run}");
    // Three boots (the first and the two compaction reports), three beats written,
    // seven reads (the run's own history among them) and the refused capture:
    // fourteen calls, one refused. The listing is not yet counted in its own
    // answer.
    assert_eq!(run["calls"], 14, "{run}");
    assert_eq!(run["refused"], 1, "{run}");
    let since = characters(&run["served_since_compaction"]);
    let whole = characters(&run["served_lifetime"]);
    assert!(since > 0, "{run}");
    // The whole life is exactly what the run holds since its last compaction and
    // what each of its compactions set back, as the run reads them itself.
    assert_eq!(
        whole,
        since + set_back.iter().sum::<u64>(),
        "the whole life holds what the compactions set back: {run}"
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
    assert_eq!(field_number(record, "compactions"), 2, "{record}");
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
    // The same calls, by verb, now with the listing counted.
    for (key, expected) in [
        ("calls", 15),
        ("calls_start_here", 3),
        ("calls_journal", 3),
        ("calls_recall", 7),
        ("calls_capture", 1),
        ("calls_list_runs", 1),
        ("refused", 1),
        ("refused_capture", 1),
        ("refused_capture_change", 1),
    ] {
        assert_eq!(field_number(record, key), expected, "{key} on {record}");
    }
    assert!(
        record["fields"].get("refused_recall").is_none(),
        "a verb that was never refused has no key: {record}"
    );

    later.finish().await;
}

#[tokio::test]
async fn a_stored_compaction_that_is_no_number_is_no_compaction_in_any_read() {
    let story = Story::begin_pretending_it_is("bot:otto", "2026-10-11").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:bart", "Bart").await;
    let bart = story.as_bot("bot:bart").await;
    is_served(&bart, 2).await;
    bart.call("start_here", json!({"compacted": true})).await;
    // A second change of the same kind, kept by a hand or an older build, whose
    // value no whole number reads.
    story
        .sessions_store()
        .plant_unreadable_reset(&SessionId(bart.run().to_string()), "not-a-number");
    story.the_store_changed_underneath().await;

    // list_runs, the run read by its handle and the usage week agree on one.
    let listed = bart.call("list_runs", json!({})).await.json();
    let run = listed["runs"]
        .as_array()
        .and_then(|runs| runs.iter().find(|run| run["run"] == bart.run()))
        .unwrap_or_else(|| panic!("the run lists itself: {listed}"));
    assert_eq!(run["compactions"], 1, "{run}");
    let one = bart
        .call(
            "recall",
            json!({"subject": format!("session:{}", bart.run())}),
        )
        .await
        .json();
    assert_eq!(field_number(&one["objects"][0], "compactions"), 1, "{one}");
    let weeks = bart
        .call(
            "recall",
            json!({"view": "usage", "inputs": {"bot": "bot:bart"}}),
        )
        .await
        .json();
    let week = &weeks["objects"][0];
    assert_eq!(field_number(week, "compactions"), 1, "{weeks}");

    story.finish().await;
}
