//! "I put down two errands in one call, and one of them named somebody I know.
//! Why did the answer say nothing about it?"
//!
//! **A batch carries every note its items earned, once.** Each entry of a batch
//! is its own capture, and a capture can say, beside its receipt, that its words
//! also name a thing the session knows, or teach the rule a first claim of a
//! kind needs. Those notes are spent once per session, so a batch that kept only
//! the first item's would use up the rest and deliver none of them. And a batch
//! that stops partway must spend nothing it did not deliver: the same call sent
//! again delivers what the items it already wrote earned.

use serde_json::json;

use super::dsl::Story;
use super::lists::a_list_on_the_trip;

/// Two errands that name a known person: the note is on the receipt, once.
#[tokio::test]
async fn a_batch_carries_the_possible_subject_note_its_items_earned_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.add("person:homer", "Homer").await;

    let batch = s
        .call(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "entries": ["call Homer", "email Homer"]}),
        )
        .await;
    let note = batch.json()["possible_subject_note"].clone();
    assert!(
        note.as_str()
            .is_some_and(|said| said.contains("person:homer")),
        "the receipt says the words also name person:homer: {}",
        batch.raw()
    );
    // Once: the second item named the same person, and the notes of the items are
    // kept apart by a blank line, so one note is one paragraph.
    assert_eq!(
        note.as_str().map(|said| said.split("\n\n").count()),
        Some(1),
        "the person is named by one note only: {}",
        batch.raw()
    );

    // The pair: a later call naming him is not told again, because the batch
    // delivered the note it spent.
    let later = s
        .call(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "content": "phone Homer again"}),
        )
        .await;
    assert!(
        later.json().get("possible_subject_note").is_none(),
        "{}",
        later.raw()
    );
    story.finish().await;
}

/// Items that each earn a different teaching: every teaching rides, once.
#[tokio::test]
async fn a_batch_carries_every_teaching_its_items_earned_once() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.add("thing:kettle", "The Kettle").await;

    // The first item earns the rules every first claim earns; the second draws an
    // edge, which earns the rule about which side of an edge a claim sits on.
    let batch = s
        .call(
            "capture",
            json!({"subject": "list:packing", "provenance": "testimony",
                   "entries": ["tent",
                               {"content": "sharpen the axe", "shape": "about",
                                "object": "thing:kettle"},
                               "stove"]}),
        )
        .await;
    let lines = batch.teachings();
    let alone = {
        let again = story.session_in("UTC", Some("new")).await;
        let one = again
            .call(
                "capture",
                json!({"subject": "list:packing", "provenance": "testimony",
                       "content": "lantern"}),
            )
            .await;
        one.teachings()
    };
    assert!(
        lines.len() > alone.len(),
        "the batch carries the edge's teaching beside the first item's {}: {}",
        alone.len(),
        batch.raw()
    );
    let mut distinct = lines.clone();
    distinct.sort();
    distinct.dedup();
    assert_eq!(
        distinct.len(),
        lines.len(),
        "no teaching twice: {}",
        batch.raw()
    );
    story.finish().await;
}

/// A batch cut at the second item spends nothing it did not deliver, and the same
/// call sent again delivers the note the first item earned.
#[tokio::test]
async fn a_cut_batch_resent_delivers_the_note_its_first_item_earned() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    s.add("person:homer", "Homer").await;
    let call = json!({"subject": "list:packing", "provenance": "testimony",
                      "request_id": "errands-1",
                      "entries": ["call Homer",
                                  {"content": "tell Homer about the axe", "shape": "about",
                                   "object": "thing:kettle"}]});

    // The second item points at a thing that does not exist yet, so it fails at
    // the write and the first has landed.
    s.refused("capture", call.clone()).await;
    s.add("thing:kettle", "The Kettle").await;

    let finished = s.call("capture", call).await;
    assert_eq!(
        finished.json()["finished_a_cut_batch"]["already_written"],
        1,
        "{}",
        finished.raw()
    );
    let note = finished.json()["possible_subject_note"].clone();
    assert!(
        note.as_str()
            .is_some_and(|said| said.contains("person:homer")),
        "the finishing receipt delivers the note the first item earned: {}",
        finished.raw()
    );
    story.finish().await;
}
