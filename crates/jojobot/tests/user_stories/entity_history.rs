//! "The mailbox on the porch used to be called something else. What was it, and
//! which run changed it?"
//!
//! **A question about the thing itself, not about anything said of it.** The
//! name a thing wears now is on every read; what it wore before is not, and
//! neither is who changed it. A reader who needs the earlier name — to
//! recognise it in an old note, or to ask the run that renamed it — has nowhere
//! to look unless the thing's own writes are kept and can be asked for.
//!
//! ⭐ **The handle alone is the door.** `history_record` takes a claim's address
//! and, with no `#`, a thing's handle: the place an agent tries first.
//!
//! ⚠️ **Each version names a run, never a key.** The key a caller sends is a
//! secret; the run's public name is what is safe to read back.
//!
//! ⛔️ **Nothing is rendered unasked.** The ordinary read carries no versions.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_reader_can_tell_what_a_thing_used_to_be_called_and_which_run_changed_it() {
    let story = Story::begin("bot:otto").await;

    // ── Spring · the thing is made, then called something else ────────────────
    let spring = story.session_on("2026-04-14", None).await;
    spring.add("thing:the-mailbox", "The Mailbox").await;
    spring
        .call(
            "update_entity",
            json!({"handle": "thing:the-mailbox", "name": "The Porch Mailbox",
                   "aliases": ["the post box"]}),
        )
        .await;
    let spring_run = spring.run().to_string();
    spring.wrap("named the mailbox").await;

    // ── Autumn · another run archives it and brings it back ───────────────────
    let autumn = story.session_on("2026-09-15", Some("new")).await;
    autumn
        .call(
            "archive_entity",
            json!({"handle": "thing:the-mailbox", "reason": "listed twice"}),
        )
        .await;
    autumn
        .call(
            "archive_entity",
            json!({"handle": "thing:the-mailbox", "reason": "it was real", "restore": true}),
        )
        .await;
    let autumn_run = autumn.run().to_string();
    assert_ne!(
        spring_run, autumn_run,
        "two runs, or the story proves nothing"
    );

    // ── Winter · the question ─────────────────────────────────────────────────
    let winter = story.session_on("2026-12-20", Some("new")).await;

    // The ordinary read first: what it is now, and no versions with it.
    let now = winter.recall("thing:the-mailbox").await;
    assert!(
        now.json()["objects"][0].get("entity_history").is_none(),
        "the versions arrived without being asked for: {}",
        now.raw()
    );

    let asked = winter
        .shape(
            "what was it called before, and who changed it?",
            json!({"history_record": "thing:the-mailbox"}),
        )
        .await;
    asked
        .number("/objects/0/entity_history/count", 4)
        .number("/objects/0/entity_history/versions/0/nth", 1);
    let versions = asked.json()["objects"][0]["entity_history"]["versions"].clone();
    let name = |at: usize| {
        versions[at]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };
    assert_eq!(
        (name(0), name(1), name(3)),
        (
            "The Mailbox".to_string(),
            "The Porch Mailbox".to_string(),
            "The Porch Mailbox".to_string()
        ),
        "what it used to be called is not in its own history: {}",
        asked.raw()
    );
    assert_eq!(versions[1]["aliases"], json!(["the post box"]));
    assert_eq!(versions[2]["archived_reason"], "listed twice");
    assert!(
        versions[3].get("archived_reason").is_none(),
        "the restore leaves a version with no archive state: {}",
        asked.raw()
    );
    // ⭐ **And who changed it**: the run's public name, one per write.
    let runs: Vec<String> = (0..4)
        .map(|at| versions[at]["run"].as_str().unwrap_or_default().to_string())
        .collect();
    assert_eq!(
        runs,
        vec![
            spring_run.clone(),
            spring_run,
            autumn_run.clone(),
            autumn_run
        ],
        "each version names the run that made it: {}",
        asked.raw()
    );
    assert!(
        !asked.raw().contains(winter.sid()),
        "a key is in the answer: {}",
        asked.raw()
    );
}
