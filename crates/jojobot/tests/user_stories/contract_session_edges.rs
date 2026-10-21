//! **This file is a contract, not a story.** It pins two edges of the surface
//! the run's own record sits behind: a beat with a blank focus is refused, and a
//! page of no runs is refused. Each case puts the refusal beside the call that
//! lands, so a build that refuses everything cannot pass. The use of the run's
//! record, a reopened run's closing story told once, is in `session_edges.rs`.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_beat_with_a_blank_focus_is_refused_and_one_with_words_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for blank in ["", "   "] {
        s.refused(
            "journal",
            json!({"entry": "a beat with no focus to speak of", "focus": blank}),
        )
        .await
        .holds_at("/fix_by", "change")
        .says("\"wrote\":false");
    }
    // Nothing was written, so the run still has no record to list.
    let none = s.call("list_runs", json!({})).await.json();
    assert_eq!(none["runs"].as_array().map(Vec::len), Some(0), "{none}");

    s.call(
        "journal",
        json!({"entry": "a beat with a focus", "focus": "the damper"}),
    )
    .await;
    let listed = s.call("list_runs", json!({})).await.json();
    assert_eq!(listed["runs"][0]["working_on"], "the damper", "{listed}");
    story.finish().await;
}

#[tokio::test]
async fn a_page_of_no_runs_is_refused_and_a_page_of_one_is_served() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call(
        "journal",
        json!({"entry": "first beat", "focus": "the flue"}),
    )
    .await;
    s.refused("list_runs", json!({"limit": 0}))
        .await
        .says("\"wrote\":false");
    let one = s.call("list_runs", json!({"limit": 1})).await.json();
    assert_eq!(one["runs"].as_array().map(Vec::len), Some(1), "{one}");
    story.finish().await;
}
