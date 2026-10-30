//! **What a batch says when one of its items is refused**: the reason the item
//! was refused, without the prefix the domain's error puts before it.

use super::*;
use crate::harness::*;
use crate::memory::testing::*;
use std::sync::Arc;

/// **An item the guard refuses as an invalid claim is told by its reason alone.**
/// The domain's refusal reads `bad claim: <reason>`; the batch names the item and
/// says the reason after its own words, so the prefix would stand inside them.
#[tokio::test]
async fn an_item_refused_as_a_bad_claim_is_told_by_its_reason_without_the_prefix() {
    let inner = Arc::new(jojobot_domain::memory::testing::InMemoryMemory::seeded());
    let jojobot = handler_over(inner);
    ensure(&jojobot, "list:homer-chores").await;

    let refused = blocked(
        &jojobot
            .capture(Parameters(CaptureArgs {
                entries: Some(vec![
                    crate::memory::capture::EntryItem::Text("mow the lawn".into()),
                    crate::memory::capture::EntryItem::Full {
                        content: "".into(),
                        details: None,
                        shape: None,
                        object: None,
                    },
                ]),
                provenance: Some("testimony".into()),
                ..capture_args("list:homer-chores", "")
            }))
            .await
            .expect("a refused batch answers"),
    );
    let said = refused["how_to_proceed"].as_str().unwrap_or_default();
    assert!(
        said.contains("item 2 of 2"),
        "the refusal names the item: {refused}"
    );
    assert!(
        !said.contains("nothing was written: bad claim"),
        "the item's reason is told without the domain's prefix: {refused}"
    );
}
