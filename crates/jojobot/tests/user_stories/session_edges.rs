//! "The closing story of a run that was reopened is told once." The edges of
//! the surface the run's record sits behind, a blank focus and an empty page,
//! are a contract and live in `contract_session_edges.rs`.

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
