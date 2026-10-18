//! "A fresh jojobot has its organize stage from its first day."
//!
//! `defrag` arrives with the software, as `assistant` does: nobody adds it, and
//! it answers with the charter and the one-liner the build ships and reports to
//! the assistant. These stories never call `add_entity` for it.

use serde_json::json;

use super::dsl::Story;

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_fresh_instance_boots_defrag_and_its_queue_is_the_unreviewed_read() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;
    dev.add("person:homer", "Homer").await;
    let defrag = story.as_bot("bot:defrag").await;

    let (booted, _) = story
        .call(
            "start_here",
            json!({"bot": "defrag", "brief": true, "resume": "new"}),
        )
        .await;
    let before = booted.json()["identity"]["review_queue"]["count"]
        .as_u64()
        .expect("the boot names how many wait");
    let waiting = listed(&defrag.call("recall", json!({"unreviewed": true})).await);
    assert_eq!(before, waiting.len() as u64, "{waiting:?}");
    assert!(waiting.contains(&"person:homer".to_string()), "{waiting:?}");

    defrag
        .call(
            "update_entity",
            json!({"handle": "person:homer", "reviewed": true}),
        )
        .await;
    let (booted, _) = story
        .call(
            "start_here",
            json!({"bot": "defrag", "brief": true, "resume": "new"}),
        )
        .await;
    let after = booted.json()["identity"]["review_queue"]["count"]
        .as_u64()
        .expect("the boot names how many wait");
    let waiting_after = listed(&defrag.call("recall", json!({"unreviewed": true})).await);
    assert_eq!(after, before - 1, "the boot's count fell by one");
    assert_eq!(waiting_after.len() as u64, before - 1, "{waiting_after:?}");
}

#[tokio::test]
async fn defrag_arrives_with_the_charter_and_the_one_liner_the_build_ships() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;

    let charter = dev
        .call("recall", json!({"subject": "bot:defrag", "charter": true}))
        .await
        .json();
    let text = charter.to_string();
    assert!(text.contains("YOU FOLD; YOU NEVER DISCARD."), "{text}");
    assert!(
        text.contains(
            "The organize stage: folds duplicates, connects and refits what jojobot holds"
        ),
        "{text}"
    );
}

#[tokio::test]
async fn defrag_reports_to_the_assistant_on_a_fresh_instance() {
    let story = Story::begin("bot:otto").await;
    let dev = story.session().await;

    let held = dev
        .call("recall", json!({"subject": "bot:defrag"}))
        .await
        .json();
    assert_eq!(
        held["objects"][0]["fields"]["reports_to"], "bot:assistant",
        "{held}"
    );
}
