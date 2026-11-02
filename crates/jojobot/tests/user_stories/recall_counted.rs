//! "How much sits under each of these, and when was each last written?"
//!
//! The assistant asks recall to count a selection. Each object then says how many
//! live things sit directly under it, how many were archived, and how many stand
//! anywhere below it, and the instant of its newest write. A call that does not ask
//! gets neither.

use serde_json::{Value, json};

use super::dsl::Story;

const BOARD: &str = "project:atlas";
const CARD: &str = "work:sigma";
const CHILD_CARD: &str = "work:tau";
const GONE_CARD: &str = "work:omega";

fn object<'a>(answer: &'a Value, handle: &str) -> &'a Value {
    answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists objects: {answer}"))
        .iter()
        .find(|object| object["id"] == handle)
        .unwrap_or_else(|| panic!("{handle} is in the answer: {answer}"))
}

/// The instant an object says its newest write happened.
fn instant(object: &Value) -> jiff::Timestamp {
    object["written_at"]
        .as_str()
        .and_then(|at| at.parse().ok())
        .unwrap_or_else(|| panic!("written_at is an instant: {object}"))
}

#[tokio::test]
async fn a_counted_selection_says_what_sits_under_each_object_and_when_it_was_written() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(BOARD, "Counted story board").await;
    for (handle, parent) in [(CARD, BOARD), (CHILD_CARD, CARD), (GONE_CARD, BOARD)] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": handle, "source": "user-named",
                   "parent": parent}),
        )
        .await;
    }
    s.call(
        "archive_entity",
        json!({"handle": GONE_CARD, "reason": "story: one child goes"}),
    )
    .await;
    let before_the_capture = jiff::Timestamp::now();
    s.call(
        "capture",
        json!({"subject": CHILD_CARD, "content": "The card was written to.",
               "provenance": "observation",
               "fields": {"read_from": "the story", "read_ref": "a capture"}}),
    )
    .await;
    let after_the_capture = jiff::Timestamp::now();

    let plain = s.call("recall", json!({"roots": true})).await.json();
    let board = object(&plain, BOARD);
    assert!(
        board.get("children").is_none() && board.get("written_at").is_none(),
        "an uncounted call carries neither: {board}"
    );

    let counted = s
        .call("recall", json!({"roots": true, "counted": true}))
        .await
        .json();
    let board = object(&counted, BOARD);
    assert_eq!(board["children"]["count"], 1, "{board}");
    assert_eq!(board["children"]["archived"], 1, "{board}");
    assert_eq!(
        board["children"]["subtree"], 2,
        "the card and the card under it stand; the archived one does not: {board}"
    );
    let board_written = instant(board);
    assert!(
        board_written < before_the_capture,
        "the board was last written before the capture: {board}"
    );

    // A browse of one kind still counts children of other kinds: the board is a
    // project and its cards are work.
    let projects = s
        .call("recall", json!({"kind": "project", "counted": true}))
        .await
        .json();
    assert_eq!(
        object(&projects, BOARD)["children"]["subtree"],
        2,
        "{projects}"
    );

    let by_kind = s
        .call("recall", json!({"kind": "work", "counted": true}))
        .await
        .json();
    let card = object(&by_kind, CARD);
    assert_eq!(card["children"]["count"], 1, "{card}");
    assert_eq!(card["children"]["subtree"], 1, "{card}");
    let leaf = object(&by_kind, CHILD_CARD);
    assert!(leaf.get("children").is_none(), "a leaf has none: {leaf}");
    let leaf_written = instant(leaf);
    assert!(
        before_the_capture <= leaf_written && leaf_written <= after_the_capture,
        "the leaf's instant is its capture's, not its creation: {leaf} (taken between \
         {before_the_capture} and {after_the_capture})"
    );
    assert!(
        board_written < leaf_written,
        "a later write to the leaf does not move the board's instant: {board} against {leaf}"
    );
    story.finish().await;
}
