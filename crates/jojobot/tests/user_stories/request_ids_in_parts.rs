//! "The write said it failed halfway. Can I send it again and have it finish?"
//!
//! **A write that commits in several parts can be sent again under the same
//! `request_id`.** The parts that landed stay, the part that did not is written
//! by the second call, and the store holds each part once. Once every part is
//! written the id is spent, and a third send answers from the first full
//! receipt.

use serde_json::{Value, json};

use super::dsl::{Answer, Story};

const CHARTER: &str = "Holds the plan and does not implement.";

/// The entrant a session's status bar carries for a rewritten charter.
fn charter_notice(answer: &Answer) -> Value {
    answer.json()["status_bar"]["charter_changed"].clone()
}

/// How many versions the thing's own row has.
async fn row_versions(s: &super::dsl::Session, handle: &str) -> i64 {
    s.shape("the versions of a row", json!({ "history_record": handle }))
        .await
        .json()["objects"][0]["entity_history"]["count"]
        .as_i64()
        .expect("a count of versions")
}

#[tokio::test]
async fn a_charter_whose_change_record_failed_is_finished_by_sending_it_again() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    let args = json!({ "bot": "bot:gamma", "prose": CHARTER, "request_id": "charter-1" });
    let before = row_versions(&delta, "bot:gamma").await;

    // ── the charter lands, and the note of the change does not ──────────────
    story.fails_the_next_commit_of("charter_change");
    let first = delta.refused("set_charter", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    first.says("charter");
    let held = gamma
        .call("recall", json!({ "subject": "bot:gamma", "prose": true }))
        .await;
    // The positive the negative rests on: the first part is there.
    assert_eq!(
        held.json()["objects"][0]["prose"],
        CHARTER,
        "{}",
        held.raw()
    );
    assert_eq!(
        charter_notice(&held),
        Value::Null,
        "the second part did not land: {}",
        held.raw()
    );

    // ── the same call again finishes it, and writes the charter once ────────
    let finished = delta.call("set_charter", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["charter"]),
        "{finished}"
    );
    let told = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(charter_notice(&told)["by"], "bot:delta", "{}", told.raw());
    assert_eq!(
        row_versions(&delta, "bot:gamma").await,
        before + 1,
        "the charter was written once, though the call was sent twice"
    );

    // ── a third send answers from the finished write ────────────────────────
    let third = delta.call("set_charter", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_restart_between_the_two_calls_still_finishes_the_write() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    let args = json!({ "bot": "bot:gamma", "prose": CHARTER, "request_id": "charter-2" });

    story.fails_the_next_commit_of("charter_change");
    delta.refused("set_charter", args.clone()).await;

    let restarted = story.restarted().await;
    let delta = restarted.as_bot("bot:delta").await;
    let finished = delta.call("set_charter", args).await.json();
    assert_eq!(
        finished["parts_already_written"],
        json!(["charter"]),
        "{finished}"
    );
    let gamma = restarted.as_bot("bot:gamma").await;
    let told = gamma.call("list_entities", json!({ "kind": "bot" })).await;
    assert_eq!(charter_notice(&told)["by"], "bot:delta", "{}", told.raw());
    restarted.finish().await;
}

#[tokio::test]
async fn an_id_sent_with_other_arguments_before_the_write_finished_is_turned_back() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;

    story.fails_the_next_commit_of("charter_change");
    delta
        .refused(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": CHARTER, "request_id": "charter-3" }),
        )
        .await;
    let other = delta
        .refused(
            "set_charter",
            json!({ "bot": "bot:gamma", "prose": "Another text.", "request_id": "charter-3" }),
        )
        .await;
    other.says("did not finish");
    // The charter that stands is the first call's.
    let held = gamma
        .call("recall", json!({ "subject": "bot:gamma", "prose": true }))
        .await;
    assert_eq!(
        held.json()["objects"][0]["prose"],
        CHARTER,
        "{}",
        held.raw()
    );
    story.finish().await;
}

#[tokio::test]
async fn a_restore_whose_history_claim_failed_is_finished_by_sending_it_again() {
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
        "request_id": "restore-1",
    });

    // ── the restore lands, and the claim that records it does not ───────────
    story.fails_the_next_commit_of("history");
    let first = s.refused("archive_entity", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    let listed = s.list("person").await;
    // The positive the negative rests on: the first part is there.
    listed.says("person:milhouse");
    let claims = |answer: &Answer| answer.json()["objects"][0]["facts"].clone();
    let held = s
        .shape(
            "what Milhouse holds",
            json!({ "subject": "person:milhouse", "facts": true }),
        )
        .await;
    assert_eq!(
        claims(&held).as_array().map_or(0, Vec::len),
        0,
        "{}",
        held.raw()
    );

    // ── the same call again records it, once, with the reason it was archived
    let finished = s.call("archive_entity", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["restore"]),
        "{finished}"
    );
    assert!(
        finished["recorded_as"]
            .as_str()
            .is_some_and(|address| address.starts_with("person:milhouse#")),
        "{finished}"
    );
    let held = s
        .shape(
            "what Milhouse holds",
            json!({ "subject": "person:milhouse", "facts": true }),
        )
        .await;
    assert_eq!(
        claims(&held).as_array().map_or(0, Vec::len),
        1,
        "{}",
        held.raw()
    );
    held.says("listed twice").says("it was real");

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("archive_entity", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_rename_whose_box_repoint_failed_is_finished_by_sending_it_again() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    gamma
        .post("delta", "for the new name", "mail that has to follow")
        .await;
    let args = json!({ "handle": "bot:delta", "to": "bot:epsilon", "request_id": "rename-1" });

    // ── the handle moves, and the box does not ──────────────────────────────
    story.fails_the_next_commit_of("box");
    let first = gamma.refused("rename_entity", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    // The positive the negative rests on: the first part is there.
    gamma.recall("bot:epsilon").await.says("Delta");
    assert_eq!(
        story.mail_held_by("bot:epsilon").await,
        (0, 0, 0),
        "the box still belongs to the old handle"
    );

    // ── the same call again moves the box, and renames nothing twice ────────
    let finished = gamma.call("rename_entity", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["rename"]),
        "{finished}"
    );
    assert_eq!(finished["mailbox"], "epsilon", "{finished}");
    assert_eq!(story.mail_held_by("bot:epsilon").await, (1, 0, 0));
    assert_eq!(
        row_versions(&gamma, "bot:epsilon").await,
        2,
        "the creation and one rename"
    );

    // ── a third send answers from the finished write ────────────────────────
    let third = gamma.call("rename_entity", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_rename_whose_reason_failed_is_finished_by_sending_it_again() {
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
        "request_id": "rename-2",
    });

    // ── the handle moves, and the reason that let it through is not kept ────
    story.fails_the_next_commit_of("reason");
    let first = s.refused("rename_entity", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    s.recall("place:moes-tavern").await.says("back room");
    s.recall("place:moes-tavern").await.never_says("premises");

    // ── the same call again keeps it, and renames nothing twice ─────────────
    let finished = s.call("rename_entity", args).await.json();
    assert_eq!(
        finished["parts_already_written"],
        json!(["rename"]),
        "{finished}"
    );
    s.recall("place:moes-tavern").await.says("premises");
    assert_eq!(
        row_versions(&s, "place:moes-tavern").await,
        2,
        "the creation and one rename"
    );
    story.finish().await;
}
