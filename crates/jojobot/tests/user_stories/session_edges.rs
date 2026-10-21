//! "A few edges of the run's own record: a blank focus, an empty page, and the
//! closing story of a run that was reopened."
//!
//! Each story puts the refusal beside the call that lands, so a build that
//! refuses everything cannot pass.

use serde_json::json;

use super::dsl::{Session, Story};

/// The reopened run, as a wrap code hands it back.
async fn reopened(story: &Story, first: &Session, closing: &str) -> Session {
    let wrapped = first
        .call("wrap_session", json!({ "story": closing }))
        .await
        .json();
    let code = wrapped["wrap_code"]
        .as_str()
        .unwrap_or_else(|| panic!("a wrap hands back a code: {wrapped}"))
        .to_string();
    let (_, again) = story
        .call(
            "start_here",
            json!({"bot": "otto", "brief": true, "resume": code}),
        )
        .await;
    again.expect("the code hands back a key for the wrapped run")
}

/// The chronology prose of the bot's runs, read by a later run, as a bot reads its past.
async fn chronology(story: &Story) -> String {
    let reader = story.session().await;
    let history = reader
        .call("recall", json!({"kind": "session", "prose": true}))
        .await
        .json();
    history["objects"][0]["prose"]
        .as_str()
        .unwrap_or_default()
        .to_string()
}

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

#[tokio::test]
async fn a_second_wrap_with_the_same_closing_story_does_not_tell_it_twice() {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first
        .call(
            "journal",
            json!({"entry": "built the first part", "focus": "the damper"}),
        )
        .await;
    let again = reopened(&story, &first, "the closing story, once only").await;
    again
        .call("journal", json!({"entry": "a last change, after the wrap"}))
        .await;
    again
        .call(
            "wrap_session",
            json!({"story": "the closing story, once only"}),
        )
        .await;
    let prose = chronology(&story).await;
    assert_eq!(
        prose.matches("the closing story, once only").count(),
        1,
        "the same story is told once: {prose}"
    );
    assert!(prose.contains("a last change"), "{prose}");
    story.finish().await;
}
