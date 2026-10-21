//! "I booted again the next day and my run from yesterday was still there."
//!
//! A run that states its own day is judged quiet on stated days, not on the
//! clock: a run that journals on the 10th is still live on the 11th and quiet
//! from the 12th. The served sentences about the sweep say so; this story holds
//! the behaviour they describe.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_run_stating_its_days_is_swept_two_stated_days_after_its_last_beat() {
    let story = Story::begin("bot:otto").await;
    let first = story.session_on("2026-03-10", None).await;
    first
        .call(
            "journal",
            json!({"entry": "started the shelves", "focus": "the shelves"}),
        )
        .await;

    // ── the next stated day: nothing is swept, and the live run is offered ───
    let (next_day, handle) = story
        .call(
            "start_here",
            json!({"bot": "bot:otto", "brief": true, "today": "2026-03-11"}),
        )
        .await;
    assert!(handle.is_none(), "the offer hands back no handle");
    let body = next_day.json();
    assert_eq!(body["session"]["swept"], json!([]), "{body}");
    assert_eq!(
        body["session"]["choices"][0]["working_on"], "the shelves",
        "{body}"
    );

    // ── two stated days on: the run is swept, and still offered back ─────────
    let (two_days, handle) = story
        .call(
            "start_here",
            json!({"bot": "bot:otto", "brief": true, "today": "2026-03-12"}),
        )
        .await;
    assert!(handle.is_none(), "the offer hands back no handle");
    let body = two_days.json();
    assert_eq!(
        body["session"]["swept"].as_array().map(Vec::len),
        Some(1),
        "{body}"
    );
    assert_eq!(
        body["session"]["choices"][0]["working_on"], "the shelves",
        "{body}"
    );
    story.finish().await;
}
