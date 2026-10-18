//! "I rewrote what I just wrote. Did jojobot's own line stay as it was?"
//!
//! **A run rewrites the newest entry it wrote itself, and never a line jojobot
//! wrote about its calls.** A write of a new class adds one of those lines after
//! the run's own entry, so the newest entry of the chronology is not always the
//! run's. A run whose only entries are jojobot's own has none to rewrite, and the
//! refusal says what can be rewritten.

use serde_json::json;

use super::dsl::{Session, Story};

/// The chronology of a run, as `recall` serves it with prose: its entries in the
/// order they were written, joined.
async fn chronology(s: &Session) -> String {
    let read = s
        .call(
            "recall",
            json!({"subject": format!("session:{}", s.run()), "prose": true}),
        )
        .await
        .json();
    read["objects"][0]["prose"]
        .as_str()
        .unwrap_or_else(|| panic!("a run answers with its chronology: {read}"))
        .to_string()
}

async fn captures(s: &Session, content: &str) {
    s.call(
        "capture",
        json!({"subject": "person:lisa", "content": content, "provenance": "testimony"}),
    )
    .await;
}

#[tokio::test]
async fn a_rewrite_changes_the_runs_own_entry_and_leaves_the_line_jojobot_wrote() {
    let story = Story::begin("bot:gamma").await;
    let setup = story.session().await;
    setup.add("person:lisa", "Lisa").await;
    let s = story.as_bot("bot:gamma").await;

    s.journal("plan A").await;
    captures(&s, "orders the pie").await;
    let before = chronology(&s).await;
    let entries: Vec<&str> = before.split("\n\n").collect();
    assert_eq!(
        entries.len(),
        2,
        "the run's entry and jojobot's line: {before}"
    );
    assert_eq!(entries[0], "plan A", "{before}");
    let beat = entries[1].to_string();

    s.call("amend_journal", json!({"entry": "plan B"})).await;
    let after = chronology(&s).await;
    let entries: Vec<&str> = after.split("\n\n").collect();
    assert_eq!(
        entries,
        ["plan B", beat.as_str()],
        "the run's own entry is rewritten and jojobot's line is as it was: {after}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_run_with_only_jojobots_lines_has_nothing_to_rewrite_and_is_told_what_can_be() {
    let story = Story::begin("bot:gamma").await;
    let setup = story.session().await;
    setup.add("person:lisa", "Lisa").await;
    let s = story.as_bot("bot:gamma").await;

    // The first write of the run is a capture, so the only entry is jojobot's.
    captures(&s, "orders the pie").await;
    let before = chronology(&s).await;
    assert!(!before.contains("\n\n"), "one entry, jojobot's: {before}");

    let refused = s.refused("amend_journal", json!({"entry": "plan B"})).await;
    refused.says("\"wrote\":false").says("journal");
    assert_eq!(
        chronology(&s).await,
        before,
        "the refusal left jojobot's line as it was"
    );
    story.finish().await;
}
