//! "Did an earlier run of mine already handle this?"
//!
//! start_here's handover hands back only the most recently wrapped run's
//! closing story — the one before it is invisible there, and always was: the
//! door remembers one story, not a history. `list_runs` is where a sitting
//! checks further back than that before it starts something a past run of
//! its own already finished.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_sitting_checks_its_own_history_before_starting_something_new() {
    let story = Story::begin("bot:otto").await;

    // ── ① a first piece of work, wrapped ─────────────────────────────────────
    let first = story.session().await;
    first
        .call(
            "journal",
            json!({
                "entry": "counted every donut left in the box",
                "focus": "counting the donut inventory",
            }),
        )
        .await;
    first
        .wrap("the box held eleven donuts, four of them jelly")
        .await;

    // ── ② a second piece of work, also wrapped — this is the one the door
    // remembers, and the only one, once ① is behind it ───────────────────────
    let second = story.session().await;
    second
        .call(
            "journal",
            json!({
                "entry": "started patching the register's tax bug",
                "focus": "patching the register's tax bug",
            }),
        )
        .await;
    second
        .wrap("the register now adds tax right; nobody has to double check the receipt")
        .await;

    // ── ③ a third sitting: the door hands over only ②, never ① ───────────────
    let (door, third) = story
        .call("start_here", json!({"bot": "otto", "brief": true}))
        .await;
    let third = third.expect("otto has nothing live, so the boot hands back a sid directly");
    let handed = door.json();
    let told = handed["session"]["handover"]["story"]
        .as_str()
        .unwrap_or_else(|| panic!("the door handed over no story: {handed}"));
    assert!(told.contains("tax right"), "{told}");
    assert!(
        !told.contains("donuts"),
        "the door is handing over more than its one most recent story: {told}",
    );

    // ③ checks its own history before starting anything, rather than
    // re-counting a box somebody already counted.
    third
        .call(
            "journal",
            json!({
                "entry": "checking whether either earlier run already covers today's ask",
                "focus": "checking its own past runs before starting",
            }),
        )
        .await;

    let runs = third.call("list_runs", json!({})).await.json();
    assert_eq!(runs["count"], 3, "①, ② and ③ itself: {runs}");
    let by_focus: std::collections::HashMap<String, String> = runs["runs"]
        .as_array()
        .expect("a list")
        .iter()
        .map(|r| {
            (
                r["working_on"].as_str().unwrap_or_default().to_string(),
                r["state"].as_str().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert_eq!(
        by_focus
            .get("counting the donut inventory")
            .map(String::as_str),
        Some("wrapped"),
        "① is invisible to the door but not to list_runs: {runs}",
    );
    assert_eq!(
        by_focus
            .get("patching the register's tax bug")
            .map(String::as_str),
        Some("wrapped"),
        "{runs}",
    );
    assert_eq!(
        by_focus
            .get("checking its own past runs before starting")
            .map(String::as_str),
        Some("active"),
        "③ itself, still open: {runs}",
    );

    story.finish().await;
}
