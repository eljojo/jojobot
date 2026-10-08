//! "Record that the handcart was repainted, and set its colour, in one call."
//!
//! **A claim carries two bags.** Its own fields say things about the claim, and
//! what it sets says things about the thing the claim is about. One call writes
//! the claim and sets the thing, from capture and from an edit, and the thing
//! holds what was set. A key stays in the bag it was first written under, and
//! the refusal that says so names the way to move it.

use serde_json::json;

use super::dsl::Story;

/// **One capture records the claim and sets the thing.** The thing holds the key
/// that was set and the claim carries it beside its own.
#[tokio::test]
async fn one_capture_records_a_claim_and_sets_the_thing() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;

    let claim = s
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "repainted after the winter",
                "provenance": "testimony",
                "fields": {"painter": "a neighbour"},
                "sets": {"colour": "red"},
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    // The thing holds what the claim set on it.
    s.shape(
        "what the handcart holds",
        json!({"subject": "thing:handcart"}),
    )
    .await
    .says("\"colour\":\"red\"");
    // And the claim carries both, so it still says who painted it.
    s.recall("thing:handcart")
        .await
        .claim(&claim)
        .says("\"colour\":\"red\"")
        .says("\"painter\":\"a neighbour\"");
    story.finish().await;
}

/// **An edit sets the thing too, and a key stays in its bag.** The setting is
/// corrected through `sets` and the claim's own field through `fields`. Naming a
/// setting as an own field is refused by name with the way to move it, and the
/// way works: take it off, then write it under the other bag.
#[tokio::test]
async fn an_edit_sets_the_thing_and_a_key_stays_in_its_bag() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thing:handcart", "The Handcart").await;
    let claim = s
        .call(
            "capture",
            json!({
                "subject": "thing:handcart", "content": "repainted",
                "provenance": "testimony",
                "fields": {"painter": "a neighbour"}, "sets": {"colour": "red"},
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    s.call(
        "update_fact",
        json!({"address": claim, "sets": {"colour": "blue"}, "fields": {"painter": "the owner"}}),
    )
    .await;
    let held = s
        .shape(
            "what the handcart holds",
            json!({"subject": "thing:handcart"}),
        )
        .await;
    held.says("\"colour\":\"blue\"")
        .says("\"painter\":\"the owner\"");

    // A setting named as one of the claim's own fields.
    s.refused(
        "update_fact",
        json!({"address": claim, "fields": {"colour": "green"}}),
    )
    .await
    .says("blocked")
    .says("colour")
    .says("clear_fields");
    // A key named in both bags of one call.
    s.refused(
        "update_fact",
        json!({"address": claim, "fields": {"painter": "x"}, "sets": {"painter": "x"}}),
    )
    .await
    .says("blocked")
    .says("painter");
    // A key that only labels the claim is never set on the thing.
    s.refused(
        "capture",
        json!({
            "subject": "thing:handcart", "content": "read it off the plate",
            "provenance": "testimony", "sets": {"read_from": "the plate"},
        }),
    )
    .await
    .says("blocked")
    .says("read_from");

    // The way: take it off, then write it under the other bag.
    s.call(
        "update_fact",
        json!({"address": claim, "clear_fields": ["painter"]}),
    )
    .await;
    s.call(
        "update_fact",
        json!({"address": claim, "sets": {"painter": "now a property of the cart"}}),
    )
    .await;
    s.shape(
        "what the handcart holds",
        json!({"subject": "thing:handcart"}),
    )
    .await
    .says("\"painter\":\"now a property of the cart\"");
    story.finish().await;
}
