//! "Two runs read the same claim. The first corrected it. The second corrects it
//! from what it read."
//!
//! **An edit can say which revision it was made against.** A claim written once
//! is revision 1 and `recall` serves the count as `revision_count` from the
//! second write on. An edit naming a revision the claim has moved past is
//! refused with `retry`, names the revision now current and the read that
//! shows what changed, and writes nothing.
//!
//! **The negatives are what make the refusal mean something.** The edit naming
//! the revision now current lands, and so does an edit that names none.

use serde_json::json;

use super::dsl::Story;

/// **The second run's edit is refused, the claim keeps the first run's, and
/// the way through works.** Both runs hold the claim at revision 1. The first
/// lands against 1. The second, against 1, is refused naming revision 2. It
/// reads again and lands against 2.
#[tokio::test]
async fn the_second_of_two_runs_editing_from_the_same_read_is_refused() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("person:kenny", "Kenny").await;
    let claim = first
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "wears an orange parka",
                "provenance": "inference",
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    let second = story.session_in("UTC", Some("new")).await;

    first
        .call(
            "update_fact",
            json!({
                "address": claim, "content": "wears a brown parka",
                "provenance": "inference", "expected_revision": 1,
            }),
        )
        .await;

    let refused = second
        .refused(
            "update_fact",
            json!({
                "address": claim, "content": "wears a blue parka",
                "provenance": "inference", "expected_revision": 1,
            }),
        )
        .await;
    refused
        .says("\"fix_by\":\"retry\"")
        .says("expected_revision: 2")
        .says("history_record");

    // Nothing was written: the claim holds the first run's edit, at revision 2.
    let read = second.recall("person:kenny").await;
    read.claim(&claim)
        .says("wears a brown parka")
        .never_says("wears a blue parka")
        .says("\"revision_count\":2");

    // **The way through.** Read again, then edit against the revision read.
    second
        .call(
            "update_fact",
            json!({
                "address": claim, "content": "wears a blue parka",
                "provenance": "inference", "expected_revision": 2,
            }),
        )
        .await;
    second
        .recall("person:kenny")
        .await
        .claim(&claim)
        .says("wears a blue parka")
        .says("\"revision_count\":3");

    // **An edit naming no revision lands as it always did.**
    second
        .call(
            "update_fact",
            json!({
                "address": claim, "content": "wears a red parka",
                "provenance": "inference",
            }),
        )
        .await;
    second
        .recall("person:kenny")
        .await
        .claim(&claim)
        .says("wears a red parka")
        .says("\"revision_count\":4");
    story.finish().await;
}
