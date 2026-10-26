//! **A batch's receipt carries every note its items earned** (rules 130, 598).
//!
//! The items of a batch are written by the one path a single entry is written
//! by, so a note that path can put on a receipt is a note the batch carries.
//!
//! **No served-path case sends a note through the fold of unlisted keys today.**
//! The one that did, an unreadable instance record that left `due_on` unworked,
//! cannot happen on a list since the unread words decide only a work item or a
//! project at a word of the instance's own. The case below guards the other side:
//! the keys a plain entry's receipt holds are listed as the item's own, so a note
//! that a later change adds to a receipt reddens it and is either listed or
//! carried. `possible_subject_note` has its own batch cases and does not use this
//! fold.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// **What a batch does not carry is what only the item has.** A plain entry's
/// receipt holds nothing but the item's own keys, so a key a later change adds to
/// the receipt of a record reddens here and is listed as the item's own, or the
/// batch carries it as a note.
#[tokio::test]
async fn a_plain_entrys_receipt_holds_only_the_keys_a_batch_leaves_to_the_item() {
    let jojobot = handler();
    ensure(&jojobot, "list:homer-chores").await;
    let mut checked = 0;
    for _ in 0..2 {
        let receipt = capture_ok(
            &jojobot,
            CaptureArgs {
                provenance: Some("testimony".into()),
                ..capture_args("list:homer-chores", "mow the lawn")
            },
        )
        .await;
        if receipt["teaching"].is_null() {
            checked += 1;
            let strangers: Vec<&String> = receipt
                .as_object()
                .expect("a receipt is an object")
                .keys()
                .filter(|key| !ITEM_OWN.contains(&key.as_str()))
                .collect();
            assert!(strangers.is_empty(), "{strangers:?} in {receipt}");
        }
    }
    assert!(
        checked > 0,
        "every receipt carried a teaching, so none was read for strangers"
    );
}
