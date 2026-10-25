use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn leftovers_an_unreadable_run_needs_a_person_and_a_repaired_run_writes() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.journal("a healthy beat").await;
    story.the_run_read_is_unreadable(s.run(), true).await;
    let refused = s
        .refused("journal", json!({"entry": "the refused beat"}))
        .await
        .json();
    assert_eq!(refused["fix_by"], "person", "{refused}");
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(refused["attempted"], s.run(), "{refused}");

    story.the_run_read_is_unreadable(s.run(), false).await;
    s.journal("the refused beat").await;
    let read = s
        .call("recall", json!({"kind": "session", "prose": true}))
        .await
        .json();
    let prose = read["objects"]
        .as_array()
        .unwrap()
        .iter()
        .find(|one| one["id"] == format!("session:{}", s.run()))
        .and_then(|one| one["prose"].as_str())
        .expect("the run's chronology is served");
    assert!(prose.contains("a healthy beat"), "{prose}");
    assert_eq!(prose.matches("the refused beat").count(), 1, "{prose}");
    story.finish().await;
}

#[tokio::test]
async fn leftovers_a_numbered_mention_walks_to_the_member_and_a_miss_writes_nothing() {
    const COLLECTION: &str = "topic:contract-ordinances-story";
    const PARKING: &str = "decision:contract-ordinance-story-parking";
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(COLLECTION, "Town ordinances").await;
    s.call(
        "capture",
        json!({"subject": COLLECTION, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": PARKING, "name": "Parking",
               "source": "user-named", "parent": COLLECTION, "sets": {"number": "2"}}),
    )
    .await;
    s.add("thing:kettle", "Kettle").await;
    s.fact("thing:kettle", &format!("parked under @{COLLECTION}/2"))
        .await;
    let walked = s
        .call(
            "recall",
            json!({"subject": "thing:kettle", "facts": true,
                   "follow": {"direction": "out", "depth": 1}}),
        )
        .await
        .json();
    let linked = walked["objects"][0]["connected"].as_array().unwrap();
    assert_eq!(linked.len(), 1, "{walked}");
    assert_eq!(linked[0]["id"], PARKING, "{walked}");
    assert_eq!(linked[0]["fields"]["number"], "2", "{walked}");
    let before = walked["objects"][0]["facts"].clone();
    let refused = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "provenance": "testimony",
                   "content": format!("parked under @{COLLECTION}/999")}),
        )
        .await
        .json();
    assert_eq!(
        refused["attempted"],
        format!("{COLLECTION}/999"),
        "{refused}"
    );
    assert_eq!(refused["wrote"], false, "{refused}");
    assert_eq!(
        refused["candidates"][0]["handle"],
        format!("{COLLECTION}/2"),
        "{refused}"
    );
    let after = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    assert_eq!(after["objects"][0]["facts"], before, "{after}");
    story.finish().await;
}
