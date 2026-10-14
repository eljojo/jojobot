//! Two names that share two rare words are probably one part of the world.
//!
//! `Springfield Nuclear Plant` and `Nuclear Plant Tours` are neither a typo
//! apart nor one inside the other, so the older screens let the second through
//! and the bot made a duplicate. The new reason refuses it with the existing
//! thing named, lifted with the token and a reason. One shared word is a family
//! name or a category and is not evidence, and a stop word is never a shared
//! word.

use super::dsl::Story;
use serde_json::json;

fn creation(kind: &str, handle: &str, name: &str) -> serde_json::Value {
    json!({"kind": kind, "handle": handle, "name": name, "source": "user-named"})
}

fn with(mut call: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let (call, extra) = (call.as_object_mut().unwrap(), extra.as_object().unwrap());
    for (key, value) in extra {
        call.insert(key.clone(), value.clone());
    }
    serde_json::Value::Object(call.clone())
}

#[tokio::test]
async fn two_shared_words_are_refused_and_land_with_a_reason_and_the_reason_is_kept() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(
        "place:springfield-nuclear-plant",
        "Springfield Nuclear Plant",
    )
    .await;

    let tours = creation(
        "place",
        "contract-nuclear-plant-tours",
        "Nuclear Plant Tours",
    );
    let refusal = s.refused("add_entity", tours.clone()).await;
    refusal
        .says("place:springfield-nuclear-plant")
        .says("shared-word");
    let token = refusal.advised("override_token");

    // The token alone is refused again, asking for the reason by name.
    s.refused(
        "add_entity",
        with(tours.clone(), json!({"override_token": &token})),
    )
    .await
    .says("override_reason")
    .says("place:springfield-nuclear-plant");
    s.recall("place:contract-nuclear-plant-tours")
        .await
        .never_says("Nuclear Plant Tours");

    s.call(
        "add_entity",
        with(
            tours,
            json!({
                "override_token": &token,
                "override_reason": "a company that sells tours of the plant, not the plant",
            }),
        ),
    )
    .await
    .says("place:contract-nuclear-plant-tours");
    s.recall("place:contract-nuclear-plant-tours")
        .await
        .says("told_apart_by")
        .says("company");

    s.wrap("made the tour company beside the plant, with the why")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn one_shared_word_another_kind_a_stop_word_and_a_typo_are_each_read_apart() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(
        "place:springfield-nuclear-plant",
        "Springfield Nuclear Plant",
    )
    .await;
    s.add("place:kwik-e-mart", "Kwik E Mart").await;
    s.add(
        "place:contract-pickles-with-mustard",
        "Pickles With Mustard",
    )
    .await;

    // The positive that the negatives below are the negatives of.
    s.refused(
        "add_entity",
        creation(
            "place",
            "contract-nuclear-plant-tours",
            "Nuclear Plant Tours",
        ),
    )
    .await
    .says("shared-word");

    // A typo-class collision is still refused by today's reason, not this one.
    s.refused(
        "add_entity",
        creation("place", "contract-kwik-e-mart-two", "Kwik-E-Mart"),
    )
    .await
    .says("place:kwik-e-mart");

    // One shared word is a category, not a duplicate.
    s.call(
        "add_entity",
        creation("place", "contract-nuclear-tours", "Plant Tours"),
    )
    .await
    .says("place:contract-nuclear-tours");
    // Another kind: a person named Plant is not the place.
    s.call(
        "add_entity",
        creation("person", "contract-plant", "Nuclear Plant Tours"),
    )
    .await
    .says("person:contract-plant");
    // A stop word is not a shared word, however rare it is in this store.
    s.call(
        "add_entity",
        creation("place", "contract-bread-with-mustard", "Bread With Mustard"),
    )
    .await
    .says("place:contract-bread-with-mustard");
    // Nothing near it lands with no refusal.
    s.call(
        "add_entity",
        creation("place", "moes-tavern", "Moe's Tavern"),
    )
    .await
    .says("place:moes-tavern");

    s.wrap("only two shared words were asked about").await;
    story.finish().await;
}
