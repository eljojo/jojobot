//! **A key that comes back to an earlier person is the shape of turns taken, and
//! the receipt says where turns live** (rule 456: confident or silent). One
//! change of person is ordinary, a status word cycles the same way, so the line
//! fires only when a person handle is written that the key held before and does
//! not hold now.

use super::testing::*;
use super::*;
use crate::harness::*;

/// Write `key` on `thing:kettle` once per value, in order, and return each receipt.
async fn written_in_turn(key: &str, values: &[&str]) -> Vec<serde_json::Value> {
    let jojobot = handler();
    let mut receipts = Vec::new();
    for value in values {
        if value.starts_with("person:") {
            ensure(&jojobot, value).await;
        }
        receipts.push(
            capture_ok(
                &jojobot,
                CaptureArgs {
                    sets: Some([(key.to_string(), value.to_string())].into()),
                    ..capture_args("thing:kettle", "whose go it was")
                },
            )
            .await,
        );
    }
    receipts
}

/// The receipt's hint, or none.
fn hint_of(receipt: &serde_json::Value) -> Option<&str> {
    receipt.get("hint").and_then(|hint| hint.as_str())
}

/// **A, then B, then A on one key names the list on the third write only.** The
/// first two writes are an ordinary change of person.
#[tokio::test]
async fn a_person_who_comes_back_to_a_key_is_told_where_turns_live() {
    let receipts = written_in_turn(
        "last_put_out_by",
        &["person:homer", "person:marge", "person:homer"],
    )
    .await;

    assert_eq!(hint_of(&receipts[0]), None, "{}", receipts[0]);
    assert_eq!(hint_of(&receipts[1]), None, "{}", receipts[1]);
    let line = hint_of(&receipts[2]).unwrap_or_else(|| panic!("no hint: {}", receipts[2]));
    // The served identifiers of a turn-taking list, not the sentence around them.
    assert!(line.contains("`tick`") && line.contains("`back`"), "{line}");
    assert!(!line.contains("`after`"), "{line}");
}

/// **The same person twice running is no change and says nothing.**
#[tokio::test]
async fn the_same_person_twice_running_says_nothing() {
    let receipts = written_in_turn(
        "last_put_out_by",
        &["person:homer", "person:homer", "person:homer"],
    )
    .await;

    for receipt in &receipts {
        assert_eq!(hint_of(receipt), None, "{receipt}");
    }
}

/// **A, then B, then C is a change of person each time and says nothing.** Paired
/// with the case above: nobody came back.
#[tokio::test]
async fn three_different_people_on_a_key_say_nothing() {
    let receipts = written_in_turn(
        "last_put_out_by",
        &["person:homer", "person:marge", "person:bart"],
    )
    .await;

    for receipt in &receipts {
        assert_eq!(hint_of(receipt), None, "{receipt}");
    }
}

/// **A status word that cycles open, closed, open says nothing**: only a person
/// handle is a signal.
#[tokio::test]
async fn a_status_word_that_cycles_says_nothing() {
    let receipts = written_in_turn("shift", &["open", "closed", "open"]).await;

    for receipt in &receipts {
        assert_eq!(hint_of(receipt), None, "{receipt}");
    }
}
