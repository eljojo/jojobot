//! **A batch's receipt carries every note its items earned** (rules 130, 598).
//!
//! The items of a batch are written by the one path a single entry is written
//! by, so a note that path can put on a receipt is a note the batch carries.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// Two entries that share a due key. The key makes each entry's write work out a
/// due moment, which reads the instance's record.
async fn batch_sent(jojobot: &Jojobot) -> serde_json::Value {
    capture_ok(
        jojobot,
        CaptureArgs {
            provenance: Some("testimony".into()),
            entries: Some(vec![
                EntryItem::Text("mow the lawn".into()),
                EntryItem::Text("wash the car".into()),
            ]),
            sets: Some([("decide_by".to_string(), "2026-11-01".to_string())].into()),
            ..capture_args("list:homer-chores", "")
        },
    )
    .await
}

/// **A batch sent while the instance's record cannot be read says what it could
/// not work out, once.** The same batch with the record readable carries no such
/// block, and the blind batch's entries still land.
#[tokio::test]
async fn a_batch_over_an_unreadable_instance_record_says_what_it_could_not_work_out() {
    let (healthy, blind) = healthy_and_down(Down::FieldsOf("topic:instance"));
    ensure(&healthy, "list:homer-chores").await;

    let unread = batch_sent(&blind).await;
    assert_eq!(unread["count"], 2, "{unread}");
    assert_eq!(
        unread["instance_unread"]["not_worked_out"],
        serde_json::json!(["due_on"]),
        "{unread}"
    );
    assert_eq!(
        unread["instance_unread"]["record"], "topic:instance",
        "{unread}"
    );

    let read = batch_sent(&healthy).await;
    assert_eq!(read["count"], 2, "{read}");
    assert!(read["instance_unread"].is_null(), "{read}");
}

/// **What a batch does not carry is what only the item has.** A plain entry's
/// receipt holds nothing but the item's own keys, so a key a later change adds to
/// the receipt of a record reddens here and is listed as the item's own, or the
/// batch carries it as a note.
#[tokio::test]
async fn a_plain_entrys_receipt_holds_only_the_keys_a_batch_leaves_to_the_item() {
    let jojobot = handler();
    ensure(&jojobot, "list:homer-chores").await;
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
            let strangers: Vec<&String> = receipt
                .as_object()
                .expect("a receipt is an object")
                .keys()
                .filter(|key| !ITEM_OWN.contains(&key.as_str()))
                .collect();
            assert!(strangers.is_empty(), "{strangers:?} in {receipt}");
        }
    }
}
