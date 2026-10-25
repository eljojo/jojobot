//! **A batch is judged against the list's record ceiling as a whole** (rule 498).

use super::*;
use crate::harness::*;
use crate::memory::testing::*;

/// **Five entries on a list that holds three are refused together and none lands.**
/// The check pass of a batch must see the ceiling the write path enforces, or the
/// first three entries land and the call fails on the fourth.
#[tokio::test]
async fn a_batch_past_the_lists_record_ceiling_lands_nothing() {
    let jojobot = handler();
    ensure(&jojobot, "list:homer-chores").await;
    let ceiling = capture_ok(
        &jojobot,
        CaptureArgs {
            sets: Some(
                [("record_capacity".to_string(), "3".to_string())]
                    .into_iter()
                    .collect(),
            ),
            ..capture_args("list:homer-chores", "the list holds three")
        },
    )
    .await;
    assert_ne!(ceiling["status"], "blocked", "{ceiling}");

    let answer = json_of(
        &jojobot
            .capture(Parameters(CaptureArgs {
                content: String::new(),
                entries: Some(
                    ["mow", "rake", "weed", "prune", "water"]
                        .iter()
                        .map(|text| crate::memory::capture::EntryItem::Text(text.to_string()))
                        .collect(),
                ),
                ..capture_args("list:homer-chores", "")
            }))
            .await
            .expect("capture answers"),
    );
    assert_eq!(answer["status"], "blocked", "{answer}");
    assert_eq!(answer["wrote"], false, "{answer}");

    let read = json_of(
        &jojobot
            .recall(Parameters(RecallArgs {
                facts: Some(true),
                ..recall_args("list:homer-chores")
            }))
            .await
            .expect("recall ok"),
    );
    let live = read["objects"][0]["facts"]
        .as_array()
        .map_or(0, |facts| facts.len());
    assert_eq!(live, 1, "only the ceiling claim stands: {read}");
}
