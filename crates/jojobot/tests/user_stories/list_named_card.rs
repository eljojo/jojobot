//! "The checklist step names the card, and the checklist shows the card's state."
//!
//! A list entry whose words name exactly one thing is about it: the entry is
//! written with a connection edge to that thing, so the list reads the card's
//! state beside the step and counts it in the list's progress. An entry naming
//! two things draws no edge and the receipt names both, so the writer sends the
//! one the step is about. An object the writer sent always wins.
//!
//! The roster names are the cast: a camping trip, a checklist, pieces of work.

use serde_json::{Value, json};

use super::dsl::{Session, Story};
use super::lists::a_list_on_the_trip;

async fn work(s: &Session, slug: &str, status: &str) {
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": {"status": status, "owner": "bot:gamma"}}),
    )
    .await;
}

async fn entries(s: &Session) -> Vec<Value> {
    let answer = s.call("recall", json!({"subject": "list:checklist"})).await;
    answer.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", answer.raw()))
        .clone()
}

/// **A step that names one card, with no object sent, reads back with the card's
/// state and counts in the list's progress.**
#[tokio::test]
async fn a_step_naming_one_card_reads_back_with_the_cards_state() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "checklist", json!({"tick": "sink"})).await;
    work(&s, "pour-the-slab", "now").await;

    s.call(
        "capture",
        json!({"subject": "list:checklist", "provenance": "testimony",
               "content": "carry @work:pour-the-slab"}),
    )
    .await;

    let read = entries(&s).await;
    let linked = &read[0]["linked"];
    assert_eq!(linked["handle"], "work:pour-the-slab", "{read:?}");
    assert_eq!(linked["status"], "now", "{linked}");
    let list = s.call("recall", json!({"subject": "list:checklist"})).await;
    assert_eq!(
        list.json()["objects"][0]["progress"]["of"],
        1,
        "the step counts in the list's progress: {}",
        list.raw()
    );
    story.finish().await;
}

/// **A step naming two cards draws no edge, and the receipt names both so the
/// writer can send the one; an object the writer sent wins over the words.**
#[tokio::test]
async fn a_step_naming_two_cards_draws_none_and_an_object_wins() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "checklist", json!({"tick": "sink"})).await;
    work(&s, "pour-the-slab", "now").await;
    work(&s, "frame-the-walls", "next").await;

    let two = s
        .call(
            "capture",
            json!({"subject": "list:checklist", "provenance": "testimony",
                   "content": "carry @work:pour-the-slab before @work:frame-the-walls"}),
        )
        .await;
    let hint = two.json()["hint"].as_str().unwrap_or_default().to_string();
    assert!(
        hint.contains("work:pour-the-slab") && hint.contains("work:frame-the-walls"),
        "the receipt names both things: {}",
        two.raw()
    );
    assert!(entries(&s).await[0].get("linked").is_none());

    s.call(
        "capture",
        json!({"subject": "list:checklist", "provenance": "testimony",
               "content": "carry @work:pour-the-slab to @work:frame-the-walls",
               "shape": "about", "object": "work:frame-the-walls"}),
    )
    .await;
    let read = entries(&s).await;
    assert_eq!(
        read[1]["linked"]["handle"], "work:frame-the-walls",
        "{read:?}"
    );
    story.finish().await;
}
