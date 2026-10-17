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
    other.holds_at("/fix_by", "change");
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

/// Whether the board holds a box of this name.
async fn has_a_box(story: &Story, name: &str) -> bool {
    use jojobot_domain::mailbox::Mailboxes;
    story
        .mail_store()
        .list_mailboxes()
        .await
        .expect("the board reads")
        .iter()
        .any(|held| held.name.as_str() == name)
}

#[tokio::test]
async fn a_bot_whose_box_failed_to_open_is_finished_by_sending_it_again() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    let args = json!({
        "kind": "bot", "handle": "delta", "name": "Delta", "source": "user-named",
        "request_id": "bot-1",
    });

    // ── the bot is made, and its box does not open ──────────────────────────
    story.fails_the_next_commit_of("box");
    let first = gamma.refused("add_entity", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    // The positive the negative rests on: the first part is there.
    gamma.recall("bot:delta").await.says("Delta");
    assert!(!has_a_box(&story, "delta").await, "the box did not open");

    // ── the same call again opens the box, and makes the bot once ───────────
    let finished = gamma.call("add_entity", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["entity"]),
        "{finished}"
    );
    assert_eq!(finished["mailbox"], "delta", "{finished}");
    assert!(has_a_box(&story, "delta").await);
    assert_eq!(
        row_versions(&gamma, "bot:delta").await,
        1,
        "the bot was made once"
    );

    // ── a third send answers from the finished write ────────────────────────
    let third = gamma.call("add_entity", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_post_whose_delivery_failed_is_finished_by_sending_it_again_with_the_same_mail() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:delta", "Delta").await;
    let delta = story.as_bot("bot:delta").await;
    delta
        .post("gamma", "a note for gamma", "mail that waits for gamma")
        .await;
    let args = json!({
        "to": "delta", "subject": "an answer", "body": "gamma answers delta",
        "request_id": "post-1",
    });

    // ── the message lands, and the collection of gamma's own mail does not ──
    story.fails_the_next_commit_of("delivery");
    let first = gamma.refused("post_message", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    // The positive the negative rests on: the first part is there.
    assert_eq!(story.mail_held_by("bot:delta").await, (1, 0, 0));
    assert_eq!(
        story.mail_held_by("bot:gamma").await,
        (1, 0, 0),
        "gamma's mail was not taken"
    );

    // ── the same call again takes the same mail, and posts nothing twice ────
    let finished = gamma.call("post_message", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["post"]),
        "{finished}"
    );
    assert!(
        finished["your_mail"]
            .to_string()
            .contains("mail that waits for gamma"),
        "the mail comes back whole: {finished}"
    );
    assert_eq!(story.mail_held_by("bot:delta").await, (1, 0, 0));
    assert_eq!(story.mail_held_by("bot:gamma").await, (0, 1, 0));

    // ── a third send answers from the finished write ────────────────────────
    let third = gamma.call("post_message", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    assert_eq!(story.mail_held_by("bot:delta").await, (1, 0, 0));
    story.finish().await;
}

#[tokio::test]
async fn a_first_post_to_the_operator_whose_box_failed_to_open_is_sent_again_and_opens_it_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    let args = json!({
        "to": "person:lisa", "subject": "the figure", "body": "a figure for the operator",
        "request_id": "post-2",
    });

    // ── the box does not open, and nothing is posted ────────────────────────
    story.fails_the_next_commit_of("box");
    s.refused("post_message", args.clone()).await;
    assert!(!has_a_box(&story, "person-lisa").await);
    assert_eq!(story.mail_held_by("person:lisa").await, (0, 0, 0));

    // ── the same call again opens it and posts, once ────────────────────────
    let posted = s.call("post_message", args.clone()).await.json();
    assert_eq!(posted["mailbox"], "person-lisa", "{posted}");
    assert!(has_a_box(&story, "person-lisa").await);
    assert_eq!(story.mail_held_by("person:lisa").await, (1, 0, 0));

    // ── and a third send is the first receipt ───────────────────────────────
    let third = s.call("post_message", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert_eq!(story.mail_held_by("person:lisa").await, (1, 0, 0));
    story.finish().await;
}

#[tokio::test]
async fn a_capture_that_ends_what_is_owed_is_finished_by_sending_it_again() {
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
    let owed = || {
        s.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-12-06"}}),
        )
    };
    owed().await.says("thing:standing-desk");
    let args = json!({
        "subject": "thing:standing-desk", "content": "The operator keeps the desk.",
        "provenance": "testimony", "recorded_at": "2026-03-15",
        "sets": {"ended": "kept"}, "request_id": "capture-1",
    });

    // ── the claim lands, and the stored due moment stays on the thing ───────
    story.fails_the_next_commit_of("clear_due");
    let first = s.refused("capture", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    s.recall("thing:standing-desk")
        .await
        .says("The operator keeps the desk.");

    // ── the same call again takes it off, and writes the claim once ─────────
    let finished = s.call("capture", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["claim"]),
        "{finished}"
    );
    owed().await.never_says("thing:standing-desk");
    let held = s
        .shape(
            "what the desk holds",
            json!({ "subject": "thing:standing-desk", "facts": true }),
        )
        .await;
    assert_eq!(
        held.json()["objects"][0]["facts"]
            .as_array()
            .map_or(0, Vec::len),
        2,
        "the claim was written once: {}",
        held.raw()
    );

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("capture", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_capture_that_ends_what_is_owed_and_landed_whole_is_a_repeat_when_sent_again() {
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
        "sets": {"ended": "kept"}, "request_id": "capture-2",
    });

    let first = s.call("capture", args.clone()).await.json();
    assert!(first.get("repeat").is_none(), "{first}");
    let again = s.call("capture", args).await.json();
    assert_eq!(again["repeat"], true, "{again}");
    assert!(again.get("parts_already_written").is_none(), "{again}");
    let held = s
        .shape(
            "what the desk holds",
            json!({ "subject": "thing:standing-desk", "facts": true }),
        )
        .await;
    assert_eq!(
        held.json()["objects"][0]["facts"]
            .as_array()
            .map_or(0, Vec::len),
        2,
        "the claim was written once: {}",
        held.raw()
    );
    story.finish().await;
}

const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil and wipe the lid.\n";

#[tokio::test]
async fn a_document_put_that_stopped_partway_is_finished_in_the_same_places_by_sending_it_again() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    let args = json!({
        "document": "document:homer", "markdown": MANUAL, "request_id": "put-1",
    });

    // ── the first section lands, and the second does not ────────────────────
    story.fails_the_next_commit_of("chunk-1");
    let first = s.refused("put_document", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    // The positive the negative rests on: the first part is there.
    let half = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        half["markdown"], "# Fryer manual\n\nRead this before the first shift.\n\n",
        "{half}"
    );

    // ── the same call again writes the other two, after the first ───────────
    let finished = s.call("put_document", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["chunk-0"]),
        "{finished}"
    );
    assert_eq!(finished["sections"], 3, "{finished}");
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(read["markdown"], MANUAL, "{read}");
    let held = s
        .shape(
            "what the document holds",
            json!({ "subject": "document:homer", "facts": true }),
        )
        .await;
    assert_eq!(
        held.json()["objects"][0]["facts"]
            .as_array()
            .map_or(0, Vec::len),
        3,
        "each section was written once: {}",
        held.raw()
    );

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("put_document", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_put_after_a_section_that_stopped_partway_finishes_in_the_places_the_first_call_chose() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    s.call(
        "put_document",
        json!({
            "document": "document:homer",
            "markdown": "# Intro\n\nHello.\n\n# Outro\n\nBye.\n",
        }),
    )
    .await;
    let intro = s.recall("document:homer").await.json()["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("a section has an address")
        .to_string();
    let args = json!({
        "document": "document:homer", "after": intro, "request_id": "put-2",
        "markdown": "# Middle\n\nIn between.\n\n## Deeper\n\nDeep.\n\n## Deepest\n\nBottom.\n",
    });

    story.fails_the_next_commit_of("chunk-1");
    s.refused("put_document", args.clone()).await;
    s.call("put_document", args).await;

    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Intro\n\nHello.\n\n# Middle\n\nIn between.\n\n## Deeper\n\nDeep.\n\n## Deepest\n\nBottom.\n# Outro\n\nBye.\n",
        "{read}"
    );
    story.finish().await;
}

/// A paragraph of about a thousand characters, ending in a blank line.
fn paragraph() -> String {
    format!("{}\n\n", "word ".repeat(200).trim_end())
}

#[tokio::test]
async fn a_long_section_put_in_cut_pieces_finishes_with_every_piece_linked_to_its_heading() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    let mut text = String::from("# Big\n\n");
    for _ in 0..60 {
        text.push_str(&paragraph());
    }
    let args = json!({ "document": "document:homer", "markdown": text, "request_id": "put-3" });

    // ── the heading's record lands, and the piece that follows it does not ──
    story.fails_the_next_commit_of("chunk-1");
    s.refused("put_document", args.clone()).await;
    let finished = s.call("put_document", args).await.json();
    assert!(
        finished["cut_for_size"].as_u64().is_some_and(|n| n >= 1),
        "the section is cut for size: {finished}"
    );
    assert_eq!(
        finished["parts_already_written"],
        json!(["chunk-0"]),
        "{finished}"
    );

    // ── read whole, every piece where it belongs ────────────────────────────
    let mut read = String::new();
    let mut offset = 0;
    loop {
        let part = s
            .call(
                "read_document",
                json!({"document": "document:homer", "offset": offset}),
            )
            .await
            .json();
        read.push_str(part["markdown"].as_str().expect("a part has markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    assert_eq!(read, text, "the document reads back byte for byte");
    story.finish().await;
}

#[tokio::test]
async fn a_section_moved_with_its_cut_pieces_is_finished_in_the_places_the_first_call_chose() {
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
        "address": address_of("# B"), "after": address_of("# C"), "request_id": "move-1",
    });

    // ── the section moves, and the first piece that follows it does not ─────
    story.fails_the_next_commit_of("chunk-0");
    let first = s.refused("update_fact", args.clone()).await;
    first.holds_at("/fix_by", "retry");

    // ── the same call again moves the rest, where the first call chose ──────
    let finished = s.call("update_fact", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["section"]),
        "{finished}"
    );
    let mut read = String::new();
    let mut offset = 0;
    loop {
        let part = s
            .call(
                "read_document",
                json!({"document": "document:homer", "offset": offset}),
            )
            .await
            .json();
        read.push_str(part["markdown"].as_str().expect("a part has markdown"));
        match part["next_offset"].as_u64() {
            Some(next) => offset = next,
            None => break,
        }
    }
    let b_at = read.find("# B").expect("B is there");
    let c_at = read.find("# C").expect("C is there");
    assert!(
        read.starts_with("# A\n\nalpha.\n\n# C"),
        "A then C: {read:.80}"
    );
    assert!(c_at < b_at, "B moved behind C");
    assert!(
        read[b_at..].contains(&"word ".repeat(200).trim_end().to_string()),
        "B was not cut off from its pieces"
    );
    assert_eq!(read.len(), text.len(), "nothing was lost or doubled");

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("update_fact", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_section_moved_whole_under_an_id_is_a_repeat_when_sent_again() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Fryer manual").await;
    s.call(
        "put_document",
        json!({
            "document": "document:homer",
            "markdown": "# A\n\nalpha.\n\n# B\n\nbravo.\n\n# C\n\ngamma.\n",
        }),
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
    let args = json!({
        "address": address_of("# A"), "after": address_of("# B"), "request_id": "move-2",
    });
    let first = s.call("update_fact", args.clone()).await.json();
    assert!(first.get("repeat").is_none(), "{first}");
    let again = s.call("update_fact", args).await.json();
    assert_eq!(again["repeat"], true, "{again}");
    assert!(
        again.get("parts_already_written").is_none(),
        "the first call finished it, so this is its receipt: {again}"
    );
    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"], "# B\n\nbravo.\n\n# A\n\nalpha.\n\n# C\n\ngamma.\n",
        "A moved behind B and C stayed last: {read}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_beat_whose_focus_failed_is_finished_by_sending_it_again_and_the_entry_lands_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.journal("opened the run").await;
    let handle = format!("session:{}", s.run());
    let args = json!({
        "entry": "found the cause", "focus": "writing the fix", "request_id": "beat-1",
    });
    let beats =
        || async { s.recall(&handle).await.json()["objects"][0]["fields"]["beats"].clone() };

    // ── the entry lands, and the focus does not move ────────────────────────
    story.fails_the_next_commit_of("focus");
    let first = s.refused("journal", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    // The positive the negative rests on: the first part is there.
    assert_eq!(beats().await, "2", "the entry landed");
    s.recall(&handle).await.never_says("writing the fix");

    // ── the same call again moves the focus, and writes no second entry ─────
    let finished = s.call("journal", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["entry"]),
        "{finished}"
    );
    assert_eq!(finished["focus"], "writing the fix", "{finished}");
    assert_eq!(beats().await, "2", "the entry was written once");
    s.recall(&handle).await.says("writing the fix");

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("journal", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn a_beat_whose_working_on_failed_is_finished_by_sending_it_again() {
    let story = Story::begin("bot:otto").await;
    let otto = story.session().await;
    otto.add("work:sigma", "Sigma").await;
    let reader = story.as_bot("bot:assistant").await;
    let working_on = || async {
        reader
            .call("recall", json!({"subject": "bot:otto"}))
            .await
            .json()["objects"][0]["fields"]["working_on"]
            .clone()
    };
    let args = json!({
        "entry": "picked up sigma", "working_on": "work:sigma", "request_id": "beat-2",
    });

    story.fails_the_next_commit_of("working_on");
    let first = otto.refused("journal", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    assert!(working_on().await.is_null(), "nothing set yet");

    let finished = otto.call("journal", args).await.json();
    assert_eq!(
        finished["parts_already_written"],
        json!(["entry"]),
        "{finished}"
    );
    assert_eq!(working_on().await, "work:sigma");
    story.finish().await;
}

#[tokio::test]
async fn a_compaction_report_whose_total_failed_is_sent_again_and_marks_the_run_once() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let run = story.as_bot("bot:gamma").await;
    run.journal("opened the run").await;
    let handle = format!("session:{}", run.run());
    let marks = || async {
        run.call("recall", json!({"subject": &handle, "prose": true}))
            .await
            .json()["objects"][0]["prose"]
            .as_str()
            .unwrap_or_default()
            .matches("reported that its context was compacted")
            .count()
    };
    let args = json!({ "compacted": true, "brief": true, "request_id": "compaction-1" });

    // ── the mark is written, and the total does not restart ─────────────────
    story.fails_the_next_commit_of("total");
    let first = run.refused("start_here", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    assert_eq!(marks().await, 1, "the mark landed");

    // ── the same call again restarts it, and writes no second mark ──────────
    let finished = run.call("start_here", args.clone()).await.json();
    assert!(
        finished["orientation"]
            .as_str()
            .is_some_and(|essay| !essay.is_empty()),
        "{finished}"
    );
    assert_eq!(marks().await, 1, "the mark was written once");

    // ── a third send answers from the finished report ───────────────────────
    let third = run.call("start_here", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert_eq!(marks().await, 1);
    story.finish().await;
}

#[tokio::test]
async fn an_edit_that_ends_what_is_owed_is_finished_by_sending_it_again() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:standing-desk", "Standing desk").await;
    let carrier = s
        .call(
            "capture",
            json!({"subject": "thing:standing-desk", "content": "Return window runs out in March.",
                   "provenance": "inference", "recorded_at": "2026-02-08",
                   "sets": {"runs_out": "2026-03-08"}}),
        )
        .await
        .field("address");
    // The stored due moment the thing holds: the read of what is owed leaves a
    // finished thing out whatever is stored, so the moment itself is read.
    let stored = || {
        s.call(
            "recall",
            json!({"subject": "thing:standing-desk", "keys": ["due_on"]}),
        )
    };
    stored().await.says("2026-03-08");
    let args = json!({
        "address": carrier, "sets": {"ended": "kept"}, "request_id": "edit-1",
    });

    // ── the edit lands, the stored due moment stays, and the call says so ───
    story.fails_the_next_commit_of("clear_due");
    let first = s.refused("update_fact", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    stored().await.says("2026-03-08");

    // ── the same call again takes it off, and writes the edit once ──────────
    let finished = s.call("update_fact", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["claim"]),
        "{finished}"
    );
    stored().await.never_says("2026-03-08");

    // ── a third send answers from the finished write ────────────────────────
    let third = s.call("update_fact", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}

#[tokio::test]
async fn an_entity_edit_that_ends_what_is_owed_is_finished_by_sending_it_again() {
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
    // The stored due moment the thing holds: the read of what is owed leaves a
    // finished thing out whatever is stored, so the moment itself is read.
    let stored = || {
        s.call(
            "recall",
            json!({"subject": "thing:standing-desk", "keys": ["due_on"]}),
        )
    };
    stored().await.says("2026-03-08");
    let args = json!({
        "handle": "thing:standing-desk", "sets": {"ended": "kept"}, "request_id": "edit-2",
    });

    story.fails_the_next_commit_of("clear_due");
    let first = s.refused("update_entity", args.clone()).await;
    first.holds_at("/fix_by", "retry");
    stored().await.says("2026-03-08");

    let finished = s.call("update_entity", args.clone()).await.json();
    assert_eq!(finished["repeat"], true, "{finished}");
    assert_eq!(
        finished["parts_already_written"],
        json!(["claim"]),
        "{finished}"
    );
    stored().await.never_says("2026-03-08");

    let third = s.call("update_entity", args).await.json();
    assert_eq!(third["repeat"], true, "{third}");
    assert!(third.get("parts_already_written").is_none(), "{third}");
    story.finish().await;
}
