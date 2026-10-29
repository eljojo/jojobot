//! "Set Homer's hat to nothing."
//!
//! **A caller who writes an empty value means the key is unset**, as every line
//! that writes `line ""` does. A bare-key `where` that still found the thing
//! would offer it to every reader of that key.

use serde_json::json;

use super::dsl::Story;

/// **An empty value takes the key off the thing**, and the bare-key question that
/// found the thing before no longer finds it.
#[tokio::test]
async fn an_empty_value_takes_the_key_off_so_a_bare_key_where_no_longer_finds_the_thing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:homer", "Homer").await;
    s.event_with(
        "person:homer",
        "wore a bowler to the plant",
        json!({ "hat": "bowler" }),
        &[],
    )
    .await;

    // The positive the negative rests on: the bare key finds the thing while it
    // holds a value.
    let held = s
        .shape(
            "who holds a hat",
            json!({ "kind": "person", "where": "hat" }),
        )
        .await;
    held.says("person:homer");

    s.event_with(
        "person:homer",
        "took the hat off",
        json!({ "hat": "" }),
        &[],
    )
    .await;

    let after = s
        .shape(
            "who holds a hat now",
            json!({ "kind": "person", "where": "hat" }),
        )
        .await;
    after.never_says("person:homer");
    let homer = s
        .shape("what Homer holds now", json!({ "subject": "person:homer" }))
        .await;
    homer.never_says("\"hat\":");

    story.finish().await;
}
