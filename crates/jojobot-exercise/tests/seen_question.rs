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

/// **The word a parent gives is not asked about, and any other word still is.**
///
/// Over the served surface, because the parent the question reads is the one a
/// read answer carries. Two creations under one project, beside a child the
/// session was shown: the one sharing only the project's word is asked nothing,
/// and the one sharing a word of its own is asked, so the first cannot pass by
/// the question having gone quiet.
#[tokio::test]
async fn a_creation_beside_a_sibling_is_asked_about_its_own_words_and_not_the_parents() {
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

    let created = |kind: &'static str, handle: &'static str, name: &'static str, parent: Value| {
        let surface = &surface;
        let sid = sid.clone();
        async move {
            let mut args = json!({"kind": kind, "handle": handle, "name": name,
                                  "source": "user-named", "sid": sid});
            if !parent.is_null() {
                args["parent"] = parent;
            }
            let receipt: Value =
                serde_json::from_str(&surface.call("add_entity", args).await).expect("json");
            receipt
        }
    };
    let asked_about = |receipt: &Value, handle: &str| {
        receipt["teaching"].as_array().is_some_and(|all| {
            all.iter()
                .filter_map(Value::as_str)
                .any(|t| t.contains(handle))
        })
    };

    created("project", "atlas", "Atlas", Value::Null).await;
    created(
        "work",
        "atlas-x-donut-stand",
        "X Donut Stand",
        json!("project:atlas"),
    )
    .await;
    surface
        .call("list_entities", json!({"kind": "work", "sid": sid}))
        .await;

    let quiet = created(
        "work",
        "atlas-kwik-e-mart",
        "Kwik E Mart",
        json!("project:atlas"),
    )
    .await;
    assert_eq!(quiet["id"], "work:atlas-kwik-e-mart", "it landed: {quiet}");
    assert!(
        !asked_about(&quiet, "work:atlas-x-donut-stand"),
        "the project's own word was asked about: {quiet}",
    );

    let asked = created(
        "work",
        "atlas-donut-stand-two",
        "Donut Stand Two",
        json!("project:atlas"),
    )
    .await;
    assert!(
        asked_about(&asked, "work:atlas-x-donut-stand"),
        "a word of the sibling's own was not asked about: {asked}",
    );
}
