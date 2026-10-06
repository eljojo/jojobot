//! **The creation question, over the served surface.**
//!
//! A creation beside a thing this session was shown is asked about it once. The
//! unit cases hold the logic; this travels the path a caller uses, through the
//! real server, which is the only place the call count is kept: the count is
//! taken as each answer leaves, and a handler called directly never leaves one.

use jojobot_exercise::room::{Room, server_binary};
use serde_json::{Value, json};

#[tokio::test]
async fn a_creation_beside_a_thing_shown_three_calls_ago_says_so() {
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

    surface
        .call(
            "add_entity",
            json!({"kind": "place", "handle": "wonder-wharf", "name": "Wonder Wharf",
                   "source": "user-named", "sid": sid}),
        )
        .await;
    // The read that shows the place, then two calls that show nothing new.
    surface
        .call("list_entities", json!({"kind": "place", "sid": sid}))
        .await;
    for _ in 0..2 {
        surface
            .call("recall", json!({"kind": "person", "sid": sid}))
            .await;
    }

    let receipt: Value = serde_json::from_str(
        &surface
            .call(
                "add_entity",
                json!({"kind": "place", "handle": "wharf-road", "name": "Wharf Road",
                       "source": "user-named", "sid": sid}),
            )
            .await,
    )
    .expect("the receipt is json");
    let asked = receipt["teaching"]
        .as_array()
        .and_then(|all| {
            all.iter()
                .filter_map(Value::as_str)
                .find(|t| t.contains("place:wonder-wharf"))
        })
        .unwrap_or_else(|| {
            panic!("the creation was not asked about the place it was shown: {receipt}")
        });
    assert!(
        asked.contains("3 calls ago"),
        "the question says how long ago the place was shown: {asked}",
    );
    assert!(asked.contains("merge_entities"), "{asked}");
}
