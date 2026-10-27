//! "I sent my own `order` on a packing list entry, and the list took it."
//!
//! **The server places list entries, so a copied placement key is dropped**:
//! `order`, `held_turn` and `held_since`, in a claim's own fields or in its
//! settings. `capture` drops one the way `update_fact` does, from the same
//! gate, so the caller's value is named in the receipt and never
//! reaches the list itself as a field of its own.
//!
//! The roster names are the cast: a camping trip, a packing list.

use serde_json::json;

use super::dsl::Story;
use super::lists::{a_list_on_the_trip, add_entry, read_back};

/// The keys the server mints when it places an entry.
const PLACEMENT_KEYS: [&str; 3] = ["order", "held_turn", "held_since"];

/// **A capture naming a placement key in either bag lands and names the dropped
/// key; the server supplies the place.** The paired positives are the
/// plain capture, which lands with a place the server minted, and a capture with
/// an ordinary key in `sets`, which lands and sets it on the list.
#[tokio::test]
async fn a_capture_drops_a_copied_placement_key_and_the_list_keeps_no_such_field() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    add_entry(&s, "list:packing", "tent poles", None).await;
    let mut expected = vec!["tent poles".to_string()];

    for key in PLACEMENT_KEYS {
        for bag in ["fields", "sets"] {
            let content = format!("{key} copied through {bag}");
            let landed = s
                .call(
                    "capture",
                    json!({"subject": "list:packing", "content": content,
                           "provenance": "testimony", bag: {key: "n"}}),
                )
                .await
                .json();
            assert_eq!(
                landed["dropped_keys"],
                json!([key]),
                "{key} in {bag}: {landed}"
            );
            expected.push(content);
        }
    }

    // The words landed in fresh places; copied keys never became list settings.
    assert_eq!(read_back(&s, "list:packing").await, expected);
    let list = s
        .call(
            "recall",
            json!({"subject": "list:packing", "keys": PLACEMENT_KEYS}),
        )
        .await
        .json();
    for key in PLACEMENT_KEYS {
        assert!(
            list["objects"][0]["fields"].get(key).is_none(),
            "the list holds {key}: {list}"
        );
    }

    // ── the ordinary writes beside the copies ───────────────────────────────
    add_entry(&s, "list:packing", "stove", None).await;
    expected.push("stove".into());
    assert_eq!(
        read_back(&s, "list:packing").await,
        expected,
        "a plain capture lands and the server places it"
    );
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "heavy things first",
               "provenance": "testimony", "sets": {"weight": "heavy"}}),
    )
    .await;
    let list = s
        .call(
            "recall",
            json!({"subject": "list:packing", "keys": ["weight"]}),
        )
        .await
        .json();
    assert_eq!(list["objects"][0]["fields"]["weight"], "heavy", "{list}");
    story.finish().await;
}

/// **Both verbs answer from the one gate**: an edit naming a copied placement
/// key reports it dropped as a capture does, so the two cannot drift.
#[tokio::test]
async fn an_edit_and_a_capture_report_the_same_dropped_key() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent poles", None).await;

    let captured = s
        .call(
            "capture",
            json!({"subject": "list:packing", "content": "stove",
                   "provenance": "testimony", "fields": {"order": "n"}}),
        )
        .await
        .json();
    let edited = s
        .call(
            "update_fact",
            json!({"address": tent, "content": "tent repacked", "fields": {"order": "n"}}),
        )
        .await
        .json();
    assert_eq!(captured["dropped_keys"], json!(["order"]), "{captured}");
    assert_eq!(edited["dropped_keys"], captured["dropped_keys"], "{edited}");
    assert!(captured["dropped_keys_note"].is_string(), "{captured}");
    assert_eq!(
        edited["dropped_keys_note"], captured["dropped_keys_note"],
        "{edited}"
    );
    story.finish().await;
}
