//! "I sent my own `order` on a packing list entry, and the list took it."
//!
//! **The server places list entries, so a caller never sends a placement key**:
//! `order`, `held_turn` and `held_since`, in a claim's own fields or in its
//! settings. `capture` refuses one the way `update_fact` does, from the same
//! gate, so the caller's value is never dropped without a word and never
//! reaches the list itself as a field of its own.
//!
//! The roster names are the cast: a camping trip, a packing list.

use serde_json::json;

use super::dsl::Story;
use super::lists::{a_list_on_the_trip, add_entry, read_back};

/// The keys the server mints when it places an entry.
const PLACEMENT_KEYS: [&str; 3] = ["order", "held_turn", "held_since"];

/// **A capture naming a placement key in either bag is refused and writes
/// nothing; the same capture without it lands.** The paired positives are the
/// plain capture, which lands with a place the server minted, and a capture with
/// an ordinary key in `sets`, which lands and sets it on the list.
#[tokio::test]
async fn a_capture_naming_a_placement_key_is_refused_and_the_list_keeps_no_such_field() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    add_entry(&s, "list:packing", "tent poles", None).await;

    for key in PLACEMENT_KEYS {
        for bag in ["fields", "sets"] {
            let refused = s
                .refused(
                    "capture",
                    json!({"subject": "list:packing", "content": "stove",
                           "provenance": "testimony", bag: {key: "n"}}),
                )
                .await;
            assert_eq!(refused.json()["fix_by"], "change", "{key} in {bag}");
            refused.says(key);
        }
    }

    // **Nothing was written**: the list reads as it did, and holds none of the
    // keys as a field of its own.
    assert_eq!(read_back(&s, "list:packing").await, ["tent poles"]);
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

    // ── the positives the refusals rest on ───────────────────────────────────
    add_entry(&s, "list:packing", "stove", None).await;
    assert_eq!(
        read_back(&s, "list:packing").await,
        ["tent poles", "stove"],
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

/// **Both verbs answer from the one gate**: an edit of an entry naming a
/// placement key is refused as `capture` refuses one, with the same way
/// forward, so the two cannot drift.
#[tokio::test]
async fn an_edit_and_a_capture_are_refused_with_the_same_way_forward() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "packing", json!({"tick": "sink"})).await;
    let tent = add_entry(&s, "list:packing", "tent poles", None).await;

    let captured = s
        .refused(
            "capture",
            json!({"subject": "list:packing", "content": "stove",
                   "provenance": "testimony", "fields": {"order": "n"}}),
        )
        .await
        .json();
    let edited = s
        .refused(
            "update_fact",
            json!({"address": tent, "fields": {"order": "n"}}),
        )
        .await
        .json();
    assert_eq!(captured["fix_by"], "change", "{captured}");
    assert_eq!(edited["fix_by"], "change", "{edited}");
    // The refusal itself is one sentence; what follows it names the verb.
    let refusal = |body: &serde_json::Value| {
        body["how_to_proceed"]
            .as_str()
            .and_then(|how| how.split(". Nothing is missing").next())
            .map(str::to_string)
    };
    assert!(refusal(&captured).is_some(), "{captured}");
    assert_eq!(
        refusal(&captured),
        refusal(&edited),
        "the two verbs word the refusal differently"
    );
    story.finish().await;
}
