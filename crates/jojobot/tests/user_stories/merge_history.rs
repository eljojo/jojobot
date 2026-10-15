//! "These two were one thing. Who moved what, and what did each of them say
//! before they were one?"
//!
//! **A merge says two things were one, and the history of the survivor has to
//! say so too.** A reader asking what a thing used to be called, or which run
//! changed it, gets the writes of the thing it absorbed along with its own, and
//! each write names the thing that made it. A child the fold moved under the
//! survivor shows the move as a version of its own, with the run that made it.

use serde_json::{Value, json};

use super::dsl::Story;

/// The versions an entity's history answers with.
fn versions_of(answer: &super::dsl::Answer) -> Vec<Value> {
    answer.json()["objects"][0]["entity_history"]["versions"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// **A child under a folded thing is served under the survivor, and its history
/// says which run moved it.**
#[tokio::test]
async fn a_merge_shows_in_the_history_of_a_child_it_moved() {
    let story = Story::begin("bot:otto").await;

    // ── Spring · a child under the thing that will be folded ──────────────────
    let spring = story.session_on("2026-04-14", None).await;
    spring.add("thing:commuter-bike", "Commuter bike").await;
    spring.add("thing:red-bike", "Red bike").await;
    spring
        .add_under("thing:commuter-bike", "thing:brass-bell", "Brass bell")
        .await;
    let spring_run = spring.run().to_string();
    spring.wrap("made the three").await;

    // ── Autumn · another run folds the parent into the other thing ────────────
    let autumn = story.session_on("2026-09-15", Some("new")).await;
    autumn
        .merge_entities(
            "thing:commuter-bike",
            "thing:red-bike",
            "the same bike entered twice",
            None,
        )
        .await;
    let autumn_run = autumn.run().to_string();
    assert_ne!(
        spring_run, autumn_run,
        "two runs, or the story proves nothing"
    );

    // ── Winter · the question ─────────────────────────────────────────────────
    let winter = story.session_on("2026-12-20", Some("new")).await;
    let now = winter.recall("thing:brass-bell").await;
    assert_eq!(
        now.json()["objects"][0]["parent"],
        "thing:red-bike",
        "{}",
        now.raw()
    );

    let asked = winter
        .shape(
            "who moved it under the survivor?",
            json!({"history_record": "thing:brass-bell"}),
        )
        .await;
    let versions = versions_of(&asked);
    assert_eq!(
        versions.len(),
        2,
        "the creation and the move are two versions: {}",
        asked.raw()
    );
    assert_eq!(versions[0]["parent"], "thing:commuter-bike");
    assert_eq!(versions[0]["run"], spring_run.as_str());
    assert_eq!(
        versions[1]["parent"],
        "thing:red-bike",
        "the move is not in the child's history: {}",
        asked.raw()
    );
    assert_eq!(
        versions[1]["run"],
        autumn_run.as_str(),
        "the move does not name the run that made it: {}",
        asked.raw()
    );
}

/// **The survivor's history holds the folded thing's versions**, each marked
/// with the handle of the thing that wrote it, in the order they happened.
/// A thing nothing was folded into shows only its own.
#[tokio::test]
async fn the_history_of_a_survivor_holds_the_versions_of_the_thing_folded_into_it() {
    let story = Story::begin("bot:otto").await;

    // ── Spring · two things, each written twice, their writes interleaved ──────
    let spring = story.session_on("2026-04-14", None).await;
    spring.add("thing:red-bike", "Red bike").await;
    spring.add("thing:commuter-bike", "Commuter bike").await;
    spring
        .call(
            "update_entity",
            json!({"handle": "thing:red-bike", "name": "Red bike with a basket"}),
        )
        .await;
    spring
        .call(
            "update_entity",
            json!({"handle": "thing:commuter-bike", "name": "Commuter bike with a basket"}),
        )
        .await;
    spring.add("thing:brass-bell", "Brass bell").await;
    spring.wrap("made them").await;

    // ── Autumn · they are one thing ───────────────────────────────────────────
    let autumn = story.session_on("2026-09-15", Some("new")).await;
    autumn
        .merge_entities(
            "thing:commuter-bike",
            "thing:red-bike",
            "the same bike entered twice",
            None,
        )
        .await;
    let autumn_run = autumn.run().to_string();

    // ── Winter · the question ─────────────────────────────────────────────────
    let winter = story.session_on("2026-12-20", Some("new")).await;
    let asked = winter
        .shape(
            "what did both of them say?",
            json!({"history_record": "thing:red-bike"}),
        )
        .await;
    let versions = versions_of(&asked);
    let seen: Vec<(String, String)> = versions
        .iter()
        .map(|v| {
            (
                v["thing"].as_str().unwrap_or("-").to_string(),
                v["name"].as_str().unwrap_or("-").to_string(),
            )
        })
        .collect();
    let expected: Vec<(String, String)> = [
        ("thing:red-bike", "Red bike"),
        ("thing:commuter-bike", "Commuter bike"),
        ("thing:red-bike", "Red bike with a basket"),
        ("thing:commuter-bike", "Commuter bike with a basket"),
        ("thing:commuter-bike", "Commuter bike with a basket"),
    ]
    .iter()
    .map(|(thing, name)| (thing.to_string(), name.to_string()))
    .collect();
    assert_eq!(
        seen,
        expected,
        "the survivor's history is not both things in time order: {}",
        asked.raw()
    );
    let nth: Vec<u64> = versions.iter().filter_map(|v| v["nth"].as_u64()).collect();
    assert_eq!(nth, vec![1, 2, 3, 4, 5], "{}", asked.raw());
    assert_eq!(
        versions[4]["run"],
        autumn_run.as_str(),
        "the fold's own version does not name its run: {}",
        asked.raw()
    );

    // A thing nothing was folded into shows only its own, and marks none.
    let alone = winter
        .shape(
            "and one never merged?",
            json!({"history_record": "thing:brass-bell"}),
        )
        .await;
    let alone = versions_of(&alone);
    assert_eq!(alone.len(), 1);
    assert!(
        alone[0].get("thing").is_none(),
        "a thing that was never merged marks its versions: {}",
        alone[0]
    );
}
