//! **A batch that ends what is owed and stops partway still takes the stored due
//! moment off.** An item that ends a thing defers the clear to the batch, which
//! makes it once after its last item. A batch cut by a store error returns before
//! that line, and with no `request_id` no resend comes to make it, so the thing
//! keeps a due moment for what it no longer owes.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use std::sync::Arc;

#[tokio::test]
async fn a_batch_cut_by_a_store_error_after_an_ending_item_takes_the_due_moment_off() {
    let inner = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    let healthy = handler_over(inner.clone());
    ensure(&healthy, "list:homer-chores").await;
    capture_ok(
        &healthy,
        CaptureArgs {
            sets: Some([("runs_out".to_string(), "2026-11-01".to_string())].into()),
            ..capture_args("list:homer-chores", "the chores run out in november")
        },
    )
    .await;
    let list = EntityId("list:homer-chores".into());
    let due = |memory: Arc<jojobot_domain::memory::testing::InMemoryMemory>| {
        let list = list.clone();
        async move {
            memory
                .fields(&list)
                .await
                .expect("the fields read")
                .get("due_on")
                .cloned()
        }
    };
    assert!(
        due(inner.clone()).await.is_some(),
        "the list holds a due moment before the batch"
    );

    let failing = Jojobot::new(
        Arc::new(FailsOnCapture {
            inner: inner.clone(),
            fails_from: 2,
            seen: Default::default(),
            edits_fail_from: usize::MAX,
            edits_seen: Default::default(),
        }),
        Arc::new(SpySearch::default()),
        Arc::new(jojobot_domain::mailbox::testing::InMemoryMailboxes::knowing_any_owner()),
        Arc::new(jojobot_domain::session::testing::InMemorySessions::new()),
        Arc::new(jojobot_domain::teaching::testing::InMemoryTeachings::new()),
        seeded_registry(),
    );
    let cut = json_of(
        &failing
            .capture(Parameters(CaptureArgs {
                entries: Some(
                    ["mow the lawn", "wash the car", "feed the cat"]
                        .iter()
                        .map(|text| crate::memory::capture::EntryItem::Text(text.to_string()))
                        .collect(),
                ),
                sets: Some([("ended".to_string(), "2026-10-20".to_string())].into()),
                ..capture_args("list:homer-chores", "")
            }))
            .await
            .expect("a cut batch answers"),
    );
    assert_eq!(cut["wrote"], true, "the first item landed: {cut}");
    assert_eq!(cut["landed"].as_array().map(Vec::len), Some(1), "{cut}");
    assert_eq!(
        due(inner).await,
        None,
        "the item that landed ended what was owed, so the due moment is off"
    );
}
