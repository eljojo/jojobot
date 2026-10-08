//! "Why does every answer carry a dozen keys that say nothing?"
//!
//! **An answer says what is there, and a key with no value is not there.** A
//! claim recorded without a day, an entity with no parent, a boot with no
//! session yet: each used to arrive as `"key":null`, on every answer, for every
//! verb. An absent key says the same and costs nothing. The few nulls that mean
//! something stay, by name.
//!
//! The pin is over the ordinary answers of the ordinary verbs, because a
//! null-free recall proves nothing about a null-free capture.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn the_ordinary_answers_of_the_ordinary_verbs_carry_no_null_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // A write of each shape the verbs have: an entity, a claim, a correction.
    let made = s
        .call(
            "add_entity",
            json!({"kind": "person", "handle": "lisa", "name": "Lisa", "source": "user-named"}),
        )
        .await;
    let captured = s
        .call(
            "capture",
            json!({"subject": "person:lisa", "content": "likes the diner", "provenance": "testimony"}),
        )
        .await;
    let recalled = s
        .call("recall", json!({"subject": "person:lisa", "facts": true}))
        .await;
    let found = s.call("search", json!({"query": "diner"})).await;
    let listed = s.call("list_entities", json!({"kind": "person"})).await;

    for (verb, answer) in [
        ("add_entity", &made),
        ("capture", &captured),
        ("recall", &recalled),
        ("search", &found),
        ("list_entities", &listed),
    ] {
        answer.never_says(":null");
        // The positive each absence rests on: the answer is not empty.
        assert!(
            answer.size() > 40,
            "{verb} answered nothing: {}",
            answer.raw()
        );
    }
    s.wrap("looked at what a plain answer carries").await;

    // The claim is there and says what it holds; what it does not hold is not
    // spelled out.
    recalled.says("likes the diner");
    recalled.never_says("happened_at");

    story.finish().await;
}
