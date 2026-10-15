//! "Bart" and "Bartholomew" are one person written two ways, usually.
//!
//! A new name that is a known short form of an existing name of the same kind
//! is refused with the existing thing named and the reason `nickname`. The
//! caller may insist, and insisting means saying why: the token alone is
//! refused again asking for `override_reason`, and the reason is kept on the
//! new thing. The same words under another kind are not a nickname of anything.

use super::dsl::Story;
use serde_json::json;

fn creation(kind: &str, handle: &str, name: &str) -> serde_json::Value {
    json!({"kind": kind, "handle": handle, "name": name, "source": "user-named"})
}

/// The handles of every thing of `kind`, as a listing names them.
async fn handles_of_kind(s: &super::dsl::Session, kind: &str) -> Vec<String> {
    let listed = s
        .shape(&format!("every {kind}"), json!({ "kind": kind }))
        .await;
    listed.json()["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("a recall answers with objects: {}", listed.raw()))
        .iter()
        .filter_map(|o| o["id"].as_str().map(str::to_string))
        .collect()
}

fn with(mut call: serde_json::Value, extra: serde_json::Value) -> serde_json::Value {
    let (call, extra) = (call.as_object_mut().unwrap(), extra.as_object().unwrap());
    for (key, value) in extra {
        call.insert(key.clone(), value.clone());
    }
    serde_json::Value::Object(call.clone())
}

#[tokio::test]
async fn a_short_form_beside_its_full_name_lands_only_with_a_reason_and_keeps_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:bartholomew", "Bartholomew").await;

    let refusal = s
        .refused("add_entity", creation("person", "bart", "Bart"))
        .await;
    refusal.says("person:bartholomew").says("nickname");
    let token = refusal.advised("override_token");

    // The token alone is refused again, and the refusal asks for a reason by the
    // argument's name and says what the name resembled.
    s.refused(
        "add_entity",
        with(
            creation("person", "bart", "Bart"),
            json!({"override_token": &token}),
        ),
    )
    .await
    .says("override_reason")
    .says("person:bartholomew");
    // Nothing was made: the handle names nothing, and no person holds it. The
    // long form is in the same listing, so the listing is the one that would
    // show it.
    s.refused("recall", json!({"subject": "person:bart"})).await;
    let people = handles_of_kind(&s, "person").await;
    assert!(
        people.iter().any(|h| h == "person:bartholomew"),
        "{people:?}"
    );
    assert!(!people.iter().any(|h| h == "person:bart"), "{people:?}");

    // Token and reason: it lands, and the reason reads back on the thing.
    s.call(
        "add_entity",
        with(
            creation("person", "bart", "Bart"),
            json!({
                "override_token": &token,
                "override_reason": "the cousin who goes by the short name",
            }),
        ),
    )
    .await
    .says("person:bart");
    assert!(
        handles_of_kind(&s, "person")
            .await
            .iter()
            .any(|h| h == "person:bart"),
        "the short form landed with the reason"
    );
    s.recall("person:bart")
        .await
        .says("told_apart_by")
        .says("cousin");

    s.wrap("made the short name beside the long one, with the why")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn the_same_words_under_another_kind_or_beside_another_surname_are_not_refused() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:bartholomew-simpson", "Bartholomew Simpson")
        .await;

    // The positive that the negatives below are the negatives of.
    s.refused(
        "add_entity",
        creation("person", "bart-simpson", "Bart Simpson"),
    )
    .await
    .says("nickname");

    // Another kind: a short form is a nickname of a person, not of a place.
    s.call(
        "add_entity",
        creation("place", "contract-bart-simpson", "Bart Simpson"),
    )
    .await
    .says("place:contract-bart-simpson");
    // Another surname: not the same person written short.
    s.call(
        "add_entity",
        creation("person", "contract-bart-flanders", "Bart Flanders"),
    )
    .await
    .says("person:contract-bart-flanders");

    s.wrap("only the short form of the same name was asked about")
        .await;
    story.finish().await;
}
