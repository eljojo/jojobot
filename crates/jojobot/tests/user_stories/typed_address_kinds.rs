//! "The second venue is Springfield; the twelfth rule governs the ledger."

use jojobot_domain::memory::EntityKind;
use jojobot_domain::memory::types::{Field, Origin};
use serde_json::json;

use super::dsl::{Session, Story};

pub(super) const RULES: &str = "topic:rules-springfield";

pub(super) async fn world(story: &Story) -> Session {
    // This instance's kind schema narrows the ledger's pointer to decisions.
    story
        .memory_store()
        .declare_kind(
            "thing",
            Origin::Shipped,
            vec![Field::pointing_at("keeps", EntityKind::DECISION)],
        )
        .await
        .expect("the instance's kind schema");
    story.the_store_changed_underneath().await;
    let s = story.session().await;
    s.add(RULES, "Springfield rules").await;
    s.call(
        "capture",
        json!({"subject": RULES, "content": "numbers its rules",
        "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "springfield-12",
        "name": "Leads", "source": "user-named", "parent": RULES,
        "sets": {"number": "12"}}),
    )
    .await;
    s.add("thing:omega", "Springfield ledger").await;
    s
}

#[tokio::test]
async fn b5_story_location_uses_the_members_kind() {
    let story = Story::begin("bot:gamma").await;
    let s = world(&story).await;
    for (collection, name) in [("topic:alpha", "Venues"), ("place:shelbyville", "Guests")] {
        s.add(collection, name).await;
        s.call(
            "capture",
            json!({"subject": collection, "content": "numbers its members",
            "sets": {"numbers_by": "number"}}),
        )
        .await;
    }
    for (kind, handle, name, parent) in [
        ("place", "springfield", "Springfield", "topic:alpha"),
        ("person", "milhouse", "Milhouse", "place:shelbyville"),
    ] {
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": handle, "name": name,
            "source": "user-named", "parent": parent, "sets": {"number": "2"}}),
        )
        .await;
    }
    let visit = |object: &str| {
        json!({"subject": "thing:omega", "content": "ledger is kept in Springfield",
        "shape": "location", "object": object, "sets": {"keeps": "decision:springfield-12"}})
    };
    let direct = s.call("capture", visit("place:springfield")).await.json();
    assert_eq!(direct["edge"]["object"], "place:springfield");
    let numbered = s.call("capture", visit("topic:alpha/2")).await.json();
    assert_eq!(numbered["edge"]["object"], "place:springfield");
    let address = numbered["address"].as_str().expect("a claim");
    let edited = s
        .call(
            "update_fact",
            json!({"address": address, "shape": "location", "object": "topic:alpha/2"}),
        )
        .await
        .json();
    assert_eq!(edited["edge"]["object"], "place:springfield");
    s.refused("capture", visit("place:shelbyville/2")).await;
    s.refused(
        "update_fact",
        json!({"address": address, "shape": "location", "object": "place:shelbyville/2"}),
    )
    .await;
    let read = s
        .call("recall", json!({"subject": "thing:omega", "facts": true}))
        .await
        .json();
    let held = read["objects"][0]["facts"].as_array().unwrap();
    let visits: Vec<_> = held
        .iter()
        .filter(|f| f["content"] == "ledger is kept in Springfield")
        .collect();
    assert_eq!(visits.len(), 2);
    assert!(
        visits
            .iter()
            .all(|f| f["edge"]["object"] == "place:springfield")
    );
    story.finish().await;
}

#[tokio::test]
async fn b5_story_narrowed_reference_uses_the_members_kind() {
    let story = Story::begin("bot:gamma").await;
    let s = world(&story).await;
    let direct = s
        .call(
            "capture",
            json!({"subject": "thing:omega", "content": "the leads rule governs the ledger",
        "sets": {"keeps": "decision:springfield-12"}}),
        )
        .await
        .json();
    let control = s
        .call("recall", json!({"subject": "thing:omega"}))
        .await
        .json();
    assert_eq!(
        control["objects"][0]["fields"]["keeps"],
        "decision:springfield-12"
    );
    let numbered = s
        .call(
            "capture",
            json!({"subject": "thing:omega", "content": "filed under rule twelve",
        "sets": {"keeps": format!("{RULES}/12")}}),
        )
        .await
        .json();
    assert!(direct["address"].is_string());
    s.call(
        "update_fact",
        json!({"address": numbered["address"], "sets": {"keeps": format!("{RULES}/12")}}),
    )
    .await;
    s.add("person:milhouse", "Milhouse").await;
    s.refused(
        "capture",
        json!({"subject": "thing:omega", "content": "a person is not a ruling",
        "sets": {"keeps": "person:milhouse"}}),
    )
    .await;
    s.refused(
        "update_fact",
        json!({"address": numbered["address"], "sets": {"keeps": "person:milhouse"}}),
    )
    .await;
    let read = s
        .call("recall", json!({"subject": "thing:omega", "facts": true}))
        .await
        .json();
    assert_eq!(
        read["objects"][0]["fields"]["keeps"],
        "decision:springfield-12"
    );
    let facts = read["objects"][0]["facts"].as_array().unwrap();
    assert_eq!(
        facts
            .iter()
            .filter(|f| f["address"] == direct["address"] || f["address"] == numbered["address"])
            .count(),
        2
    );
    story.finish().await;
}
