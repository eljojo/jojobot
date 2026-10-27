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
async fn b5_story_list_plain_entry_uses_the_member() {
    let story = Story::begin("bot:gamma").await;
    let s = list_world(&story).await;
    super::lists::add_entry(&s, "list:packing", "tent", None).await;
    super::lists::add_entry(&s, &format!("{RULES}/20"), "stove", None).await;
    assert_eq!(
        super::lists::read_back(&s, "list:packing").await,
        ["tent", "stove"]
    );
    story.finish().await;
}

async fn list_world(story: &Story) -> Session {
    let s = world(story).await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "the packing rule governs the ledger",
        "sets": {"keeps": "decision:springfield-12"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "list", "handle": "packing", "name": "Packing",
        "source": "user-named", "parent": RULES,
        "sets": {"number": "20", "tick": "sink", "numbers_by": "number"}}),
    )
    .await;
    s
}

#[tokio::test]
async fn b5_story_list_single_tick_uses_the_member() {
    let story = Story::begin("bot:gamma").await;
    let s = list_world(&story).await;
    let tent = super::lists::add_entry(&s, "list:packing", "tent", None).await;
    let stove = super::lists::add_entry(&s, "list:packing", "stove", None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "packed the tent",
        "tick": {"entry": tent, "act": "done"}}),
    )
    .await;
    assert_eq!(
        super::list_ticks::read_states(&s, "list:packing").await,
        ["stove:open", "tent:done"]
    );
    s.call(
        "capture",
        json!({"subject": format!("{RULES}/20"), "content": "packed the stove",
        "tick": {"entry": stove, "act": "done"}}),
    )
    .await;
    assert_eq!(
        super::list_ticks::read_states(&s, "list:packing").await,
        ["tent:done", "stove:done"]
    );
    story.finish().await;
}

#[tokio::test]
async fn b5_story_list_entries_use_the_member() {
    let story = Story::begin("bot:gamma").await;
    let s = list_world(&story).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "entries": ["tent"]}),
    )
    .await;
    assert_eq!(super::lists::read_back(&s, "list:packing").await, ["tent"]);
    s.call(
        "capture",
        json!({"subject": format!("{RULES}/20"), "entries": ["stove", "map"]}),
    )
    .await;
    assert_eq!(
        super::lists::read_back(&s, "list:packing").await,
        ["tent", "stove", "map"]
    );
    story.finish().await;
}

#[tokio::test]
async fn b5_story_list_ticks_use_the_member() {
    let story = Story::begin("bot:gamma").await;
    let s = list_world(&story).await;
    let tent = super::lists::add_entry(&s, "list:packing", "tent", None).await;
    let stove = super::lists::add_entry(&s, "list:packing", "stove", None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "ticks": [{"entry": tent, "act": "done"}]}),
    )
    .await;
    assert_eq!(
        super::list_ticks::read_states(&s, "list:packing").await,
        ["stove:open", "tent:done"]
    );
    s.call(
        "capture",
        json!({"subject": format!("{RULES}/20"), "ticks": [{"entry": stove, "act": "done"}]}),
    )
    .await;
    assert_eq!(
        super::list_ticks::read_states(&s, "list:packing").await,
        ["tent:done", "stove:done"]
    );
    story.finish().await;
}

#[tokio::test]
async fn b5_story_list_prefix_does_not_make_a_decision_a_list() {
    let story = Story::begin("bot:gamma").await;
    let s = list_world(&story).await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "shelbyville-40", "name": "Ruling",
        "source": "user-named", "parent": "list:packing", "sets": {"number": "2"}}),
    )
    .await;
    let entry = super::lists::add_entry(&s, "list:packing", "tent", None).await;
    for subject in ["decision:shelbyville-40", "list:packing/2"] {
        s.refused("capture", json!({"subject": subject, "entries": ["stove"]}))
            .await;
        s.refused(
            "capture",
            json!({"subject": subject, "ticks": [{"entry": entry, "act": "done"}]}),
        )
        .await;
        s.refused("capture", json!({"subject": subject, "content": "packed", "tick": {"entry": entry, "act": "done"}})).await;
    }
    assert_eq!(super::lists::read_back(&s, "list:packing").await, ["tent"]);
    story.finish().await;
}

#[tokio::test]
async fn b5_story_rename_moves_the_member_and_its_references() {
    let story = Story::begin("bot:gamma").await;
    let s = world(&story).await;
    s.call(
        "capture",
        json!({"subject": "thing:omega", "content": "the ledger follows the rule",
        "sets": {"keeps": "decision:springfield-12"}}),
    )
    .await;
    s.call(
        "rename_entity",
        json!({"handle": "decision:springfield-12", "to": "decision:springfield-13"}),
    )
    .await;
    s.call(
        "rename_entity",
        json!({"handle": format!("{RULES}/12"), "to": "decision:springfield-14"}),
    )
    .await;
    let member = s
        .call("recall", json!({"subject": format!("{RULES}/12")}))
        .await
        .json();
    assert_eq!(member["objects"][0]["id"], "decision:springfield-14");
    let ledger = s
        .call("recall", json!({"subject": "thing:omega"}))
        .await
        .json();
    assert_eq!(
        ledger["objects"][0]["fields"]["keeps"],
        "decision:springfield-14"
    );
    s.refused(
        "rename_entity",
        json!({"handle": "decision:springfield-13", "to": "decision:springfield-12"}),
    )
    .await;
    s.refused(
        "rename_entity",
        json!({"handle": format!("{RULES}/999"), "to": "decision:springfield-12"}),
    )
    .await;
    story.finish().await;
}

#[tokio::test]
async fn b5_story_parent_selects_the_members_children() {
    let story = Story::begin("bot:gamma").await;
    let s = world(&story).await;
    s.call(
        "add_entity",
        json!({"kind": "thing", "handle": "springfield-camping",
        "name": "Camping index", "source": "user-named", "parent": "decision:springfield-12",
        "sets": {"keeps": "decision:springfield-12"}}),
    )
    .await;
    for kind in [None, Some("thing")] {
        let mut args = json!({"parent": "decision:springfield-12"});
        if let Some(kind) = kind {
            args["kind"] = json!(kind);
        }
        let control = s.call("recall", args.clone()).await.json();
        assert_eq!(control["objects"][0]["id"], "thing:springfield-camping");
        args["parent"] = json!(format!("{RULES}/12"));
        let numbered = s.call("recall", args).await.json();
        assert_eq!(
            numbered["objects"], control["objects"],
            "a numbered parent selects its member's children"
        );
    }
    s.refused(
        "recall",
        json!({"parent": format!("{RULES}/999"), "kind": "thing"}),
    )
    .await;
    story.finish().await;
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
    assert_eq!(
        s.recall("thing:omega")
            .await
            .claim(direct["address"].as_str().expect("claim address"))
            .json()["edge"]["object"],
        "place:springfield"
    );
    let numbered = s.call("capture", visit("topic:alpha/2")).await.json();
    assert_eq!(
        s.recall("thing:omega")
            .await
            .claim(numbered["address"].as_str().expect("claim address"))
            .json()["edge"]["object"],
        "place:springfield"
    );
    let address = numbered["address"].as_str().expect("a claim");
    let edited = s
        .call(
            "update_fact",
            json!({"address": address, "shape": "location", "object": "topic:alpha/2"}),
        )
        .await
        .json();
    assert_eq!(
        s.recall("thing:omega")
            .await
            .claim(edited["address"].as_str().expect("claim address"))
            .json()["edge"]["object"],
        "place:springfield"
    );
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
