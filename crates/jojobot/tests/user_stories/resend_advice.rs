//! "The write said it stopped halfway. Should I send it again?"
//!
//! **A write that stopped after its first part is told what to do from WHY the
//! next part stopped.** A store that was down is `retry`, and the same call under
//! the same `request_id` finishes it (see `request_ids_in_parts`). A store that
//! answered and refused is `change`: sending the call again meets the same
//! refusal, so the answer names the refusal and the call is not told to loop.
//! Each case here is the refusal half of one case there, with the part the same
//! and the first part standing.

use serde_json::{Value, json};

use super::dsl::{Answer, Story};

const RULE: &str = "the rule says no";

/// **A refused part is `change` and says why, on the first send and on the
/// resend under the same id.** The resend is refused the same way, so the advice
/// never promised a different outcome.
fn change_naming_the_rule(sent: &Answer) {
    sent.holds_at("/fix_by", "change");
    sent.says(RULE);
    assert!(
        !sent.raw().contains("it writes the"),
        "a refusal is not told that a resend finishes the step: {}",
        sent.raw()
    );
}

#[tokio::test]
async fn a_charter_whose_change_record_was_refused_is_told_to_change_and_not_to_resend() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    let args = json!({
        "bot": "bot:gamma", "prose": "Holds the plan and does not implement.",
        "request_id": "charter-9",
    });

    story.refuses_every_commit_of("charter_change", RULE);
    change_naming_the_rule(&delta.refused("set_charter", args.clone()).await);
    // The positive the negative rests on: the first part is there.
    let held = gamma
        .call("recall", json!({ "subject": "bot:gamma", "prose": true }))
        .await;
    assert_eq!(
        held.json()["objects"][0]["prose"],
        "Holds the plan and does not implement.",
        "{}",
        held.raw()
    );
    change_naming_the_rule(&delta.refused("set_charter", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_restore_whose_history_claim_was_refused_is_told_to_change() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.call(
        "archive_entity",
        json!({ "handle": "person:milhouse", "reason": "listed twice" }),
    )
    .await;
    let args = json!({
        "handle": "person:milhouse", "reason": "it was real", "restore": true,
        "request_id": "restore-9",
    });

    story.refuses_every_commit_of("history", RULE);
    change_naming_the_rule(&s.refused("archive_entity", args.clone()).await);
    s.list("person").await.says("person:milhouse");
    change_naming_the_rule(&s.refused("archive_entity", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_rename_whose_box_was_refused_is_told_to_change() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let args = json!({ "handle": "bot:delta", "to": "bot:epsilon", "request_id": "rename-9" });

    story.refuses_every_commit_of("box", RULE);
    change_naming_the_rule(&gamma.refused("rename_entity", args.clone()).await);
    gamma.recall("bot:epsilon").await.says("Delta");
    change_naming_the_rule(&gamma.refused("rename_entity", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_rename_whose_reason_was_refused_is_told_to_change() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("org:moes-tavern", "Moes Tavern").await;
    s.add("place:moes", "Moe's").await;
    s.add("place:gorge-spot", "The back room").await;
    let call = json!({"handle": "place:gorge-spot", "to": "place:moes-tavern"});
    let token = s
        .refused("rename_entity", call.clone())
        .await
        .advised("override_token");
    let args = json!({
        "handle": "place:gorge-spot", "to": "place:moes-tavern",
        "override_token": token,
        "override_reason": "the back room became its own premises",
        "request_id": "rename-8",
    });

    story.refuses_every_commit_of("reason", RULE);
    change_naming_the_rule(&s.refused("rename_entity", args.clone()).await);
    s.recall("place:moes-tavern").await.says("back room");
    change_naming_the_rule(&s.refused("rename_entity", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_post_whose_delivery_was_refused_is_told_to_change() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .post("gamma", "a note for gamma", "mail that waits for gamma")
        .await;
    let args = json!({
        "to": "delta", "subject": "an answer", "body": "gamma answers delta",
        "request_id": "post-9",
    });

    story.refuses_every_commit_of("delivery", RULE);
    change_naming_the_rule(&gamma.refused("post_message", args.clone()).await);
    assert_eq!(story.mail_held_by("bot:delta").await, (1, 0, 0));
    change_naming_the_rule(&gamma.refused("post_message", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_capture_whose_due_moment_could_not_come_off_is_told_to_change() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:standing-desk", "Standing desk").await;
    s.call(
        "capture",
        json!({"subject": "thing:standing-desk", "content": "Return window runs out in March.",
               "provenance": "inference", "recorded_at": "2026-02-08",
               "sets": {"runs_out": "2026-03-08"}}),
    )
    .await;
    let args = json!({
        "subject": "thing:standing-desk", "content": "The operator keeps the desk.",
        "provenance": "testimony", "recorded_at": "2026-03-15",
        "sets": {"ended": "kept"}, "request_id": "capture-9",
    });

    story.refuses_every_commit_of("clear_due", RULE);
    change_naming_the_rule(&s.refused("capture", args.clone()).await);
    s.recall("thing:standing-desk")
        .await
        .says("The operator keeps the desk.");
    change_naming_the_rule(&s.refused("capture", args).await);
    story.finish().await;
}

/// The manual `request_ids_in_parts` puts: a heading and two sections.
const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil and wipe the lid.\n";

#[tokio::test]
async fn a_document_put_whose_second_section_was_refused_is_told_to_change() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    let args = json!({
        "document": "document:homer", "markdown": MANUAL, "request_id": "put-9",
    });

    story.refuses_every_commit_of("chunk-1", RULE);
    change_naming_the_rule(&s.refused("put_document", args.clone()).await);
    let half = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        half["markdown"], "# Fryer manual\n\nRead this before the first shift.\n\n",
        "{half}"
    );
    change_naming_the_rule(&s.refused("put_document", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_beat_whose_focus_was_refused_is_told_to_change() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.journal("opened the run").await;
    let handle = format!("session:{}", s.run());
    let args = json!({
        "entry": "found the cause", "focus": "writing the fix", "request_id": "beat-9",
    });

    story.refuses_every_commit_of("focus", RULE);
    change_naming_the_rule(&s.refused("journal", args.clone()).await);
    let beats: Value = s.recall(&handle).await.json()["objects"][0]["fields"]["beats"].clone();
    assert_eq!(beats, "2", "the entry landed");
    change_naming_the_rule(&s.refused("journal", args).await);
    story.finish().await;
}

#[tokio::test]
async fn a_beat_whose_working_on_was_refused_is_told_to_change() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    let args = json!({
        "entry": "picked up sigma", "working_on": "work:sigma", "request_id": "beat-8",
    });

    story.refuses_every_commit_of("working_on", RULE);
    change_naming_the_rule(&otto.refused("journal", args.clone()).await);
    change_naming_the_rule(&otto.refused("journal", args).await);
    story.finish().await;
}

/// A paragraph long enough that a section of sixty of them is cut into pieces.
fn paragraph() -> String {
    format!("{}\n\n", "word ".repeat(200).trim_end())
}

#[tokio::test]
async fn a_section_move_whose_first_piece_was_refused_is_told_to_change() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    let mut text = String::from("# A\n\nalpha.\n\n# B\n\n");
    for _ in 0..60 {
        text.push_str(&paragraph());
    }
    text.push_str("# C\n\ngamma.\n");
    s.call(
        "put_document",
        json!({ "document": "document:homer", "markdown": text }),
    )
    .await;
    let held = s.recall("document:homer").await.json();
    let facts = held["objects"][0]["facts"]
        .as_array()
        .expect("sections")
        .clone();
    let address_of = |heading: &str| {
        facts
            .iter()
            .find(|fact| fact["content"] == heading)
            .and_then(|fact| fact["address"].as_str())
            .unwrap_or_else(|| panic!("no section {heading}: {held}"))
            .to_string()
    };
    assert!(facts.len() > 3, "B is cut into pieces: {held}");
    let args = json!({
        "address": address_of("# B"), "after": address_of("# C"), "request_id": "move-9",
    });

    story.refuses_every_commit_of("chunk-0", RULE);
    change_naming_the_rule(&s.refused("update_fact", args.clone()).await);
    change_naming_the_rule(&s.refused("update_fact", args).await);
    story.finish().await;
}
