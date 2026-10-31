//! **The harness holds the cursor of what changed in the room, and moves it on
//! every read.** `Surface::changes` sends the position it kept, keeps the position
//! the answer carries, and returns the notices. A read that kept no position would
//! start again each time and report nothing; one that never advanced would report
//! the same creation twice. Both let a lock about "what changed since the sitting"
//! pass over a room that changed nothing, or fail on one it had already judged.
//!
//! One room, three reads: the start, one creation, then a quiet read.

use jojobot_exercise::room::{Room, server_binary};
use serde_json::{Value, json};

fn created_handles(notices: &Value) -> Vec<&str> {
    notices
        .as_array()
        .into_iter()
        .flatten()
        .filter(|notice| notice["created"] == true)
        .filter_map(|notice| notice["handle"].as_str())
        .collect()
}

#[tokio::test]
async fn each_read_of_the_changes_reports_only_what_happened_since_the_one_before() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the shipped identity boots");
    let sid = booted["session"]["sid"]
        .as_str()
        .expect("a handle")
        .to_string();

    let start = surface.changes().await.expect("the first read");
    assert!(
        created_handles(&start).is_empty(),
        "the first read has nothing before it to be new against: {start}"
    );

    surface
        .must(
            "add_entity",
            json!({"kind": "thing", "handle": "floor-pump", "name": "The Floor Pump",
                   "source": "user-named", "sid": sid}),
        )
        .await
        .expect("the thing is made");
    let after_one = surface.changes().await.expect("the second read");
    assert_eq!(
        created_handles(&after_one),
        ["thing:floor-pump"],
        "the creation is reported once, and only that: {after_one}"
    );

    let quiet = surface.changes().await.expect("the third read");
    assert!(
        created_handles(&quiet).is_empty(),
        "a creation already reported was reported again: {quiet}"
    );
}
