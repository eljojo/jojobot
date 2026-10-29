//! "I added to the packing list, and the receipt did not blame me for what the
//! server wrote."
//!
//! A list write carries keys the server mints: the entry's place, the turn and
//! the count a move holds, and what a tick records. A caller never sends them,
//! so a receipt that says they were sent as the caller's own fields, and advises
//! `sets` for them, is wrong twice. A key the caller really did send as an own
//! field is still told apart.

use serde_json::{Value, json};

use super::dsl::{Answer, Session, Story};
use super::lists::{a_list_on_the_trip, add_entry, read_back};

/// The keys the server mints on a list write.
const MINTED: [&str; 6] = [
    "order",
    "held_turn",
    "held_since",
    "tick_entry",
    "tick_act",
    "tick_session",
];

/// **No minted key is named as stayed, and the receipt never says it was sent.**
/// The receipt always carries `stayed_on_the_record`, so it is read at its place
/// and must be there and empty: a receipt that dropped the list altogether is a
/// receipt that stopped saying what stayed. The words are read for the key names
/// and for the sentence that blames the caller.
fn names_no_minted_key(answer: &Answer) {
    let body = answer.json();
    let stayed = body["stayed_on_the_record"].as_array().unwrap_or_else(|| {
        panic!(
            "the receipt carries no stayed_on_the_record list: {}",
            answer.raw()
        )
    });
    assert!(
        stayed.is_empty(),
        "the server's own keys are named as stayed on the record: {}",
        answer.raw()
    );
    for key in MINTED {
        assert!(
            !answer.raw().contains(&format!("{key}, "))
                && !answer.raw().contains(&format!(" {key} stay")),
            "the receipt names {key} among keys that stayed: {}",
            answer.raw()
        );
    }
    assert!(
        !answer.raw().contains("sent as its own fields"),
        "the receipt says the caller sent an own field: {}",
        answer.raw()
    );
}

/// The open entries of a list as `text:state`, in the order `recall` serves them.
async fn states(s: &Session, list: &str) -> Vec<String> {
    let answer = s.call("recall", json!({"subject": list})).await;
    answer.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", answer.raw()))
        .iter()
        .map(|entry| {
            format!(
                "{}:{}",
                entry["text"].as_str().unwrap_or_default(),
                entry["state"].as_str().unwrap_or_default()
            )
        })
        .collect()
}

async fn the_packing_list(s: &Session) {
    a_list_on_the_trip(s, "packing", json!({"tick": "sink"})).await;
}

#[tokio::test]
async fn a_capture_on_a_list_does_not_say_the_caller_sent_the_order() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_packing_list(&s).await;

    let plain = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "tent", "provenance": "testimony"}),
        )
        .await;
    names_no_minted_key(&plain);
    assert_eq!(read_back(&s, "list:packing").await, ["tent"]);

    let first = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "stove", "provenance": "testimony",
                   "after": "first"}),
        )
        .await;
    names_no_minted_key(&first);
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["stove", "tent"],
        "the entry placed first reads first"
    );

    s.wrap("added two entries").await;
    story.finish().await;
}

#[tokio::test]
async fn a_tick_does_not_say_the_caller_sent_what_the_server_recorded() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_packing_list(&s).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;

    let ticked = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "done it", "provenance": "testimony",
                   "tick": {"entry": tent, "act": "done"}}),
        )
        .await;
    names_no_minted_key(&ticked);
    assert_eq!(
        states(&s, "list:packing").await,
        ["tent:done"],
        "the ticked entry reads done on a sink list"
    );

    s.wrap("ticked the tent").await;
    story.finish().await;
}

#[tokio::test]
async fn a_move_does_not_say_the_caller_sent_what_the_server_held() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_packing_list(&s).await;
    add_entry(&s, "list:packing", "tent", None).await;
    let stove = add_entry(&s, "list:packing", "stove", None).await;

    let moved = s
        .call("update_fact", json!({"address": stove, "after": "first"}))
        .await;
    names_no_minted_key(&moved);
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["stove", "tent"],
        "the moved entry reads first"
    );

    s.wrap("moved the stove").await;
    story.finish().await;
}

#[tokio::test]
async fn an_own_field_the_caller_sent_still_stays_on_the_record_and_is_told() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    the_packing_list(&s).await;

    let sunny = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "sunscreen", "provenance": "testimony",
                   "fields": {"mood": "sunny"}}),
        )
        .await;
    let body = sunny.json();
    let stayed: Vec<&str> = body["stayed_on_the_record"]
        .as_array()
        .expect("a list of keys")
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(stayed, ["mood"], "{}", sunny.raw());
    // Pins the receipt trim of `postcondition`, not a retired key.
    assert!(body.get("postcondition").is_none(), "{body}");
    assert!(body["a_key_stayed_off_the_thing"].is_string(), "{body}");

    s.wrap("added sunscreen with a mood").await;
    story.finish().await;
}

/// **A key named like one the server mints is the caller's own on a thing that
/// places nothing.** Only a list or a document mints them, so on an ordinary
/// thing a receipt reports the key as reached like any other.
#[tokio::test]
async fn an_order_key_on_an_ordinary_thing_is_told_as_reached() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;

    let set = s
        .call(
            "capture",
            json!({"subject": "thing:kettle", "content": "third in the queue",
                   "provenance": "testimony", "sets": {"order": "3"}}),
        )
        .await;
    assert_eq!(
        set.json()["reached_the_thing"],
        json!(["order"]),
        "{}",
        set.raw()
    );

    s.wrap("numbered the kettle").await;
    story.finish().await;
}
