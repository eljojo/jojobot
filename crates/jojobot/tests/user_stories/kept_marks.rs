//! "I marked the summary as standing for two notes, then changed my mind about
//! which. What did it stand for before, and what did it point at when it was
//! written?"
//!
//! **A change to what a record points at or stands for used to erase what was
//! there.** The tables that answer the ordinary read hold the newest set only,
//! so the first mark left nothing behind once the second replaced it. Each write
//! of the record now keeps its own set, and the record's history shows them.
//!
//! ⛔️ **The ordinary read still shows the newest mark alone.** The history is the
//! place to ask for the earlier ones, not a second copy of the answer.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_reader_can_see_what_a_record_stood_for_before_the_mark_changed() {
    let story = Story::begin("bot:otto").await;
    let s = story.session_on("2026-04-14", None).await;

    s.add("thing:shelf", "The Shelf").await;
    s.add("person:teddy", "Teddy").await;
    let first = s.fact("thing:shelf", "holds the spare keys").await;
    let second = s.fact("thing:shelf", "holds the map").await;
    let summary = s
        .call(
            "capture",
            json!({"subject": "thing:shelf", "content": "holds what Teddy needs",
                   "provenance": "testimony", "refs": ["person:teddy"]}),
        )
        .await
        .json();
    let summary = summary["address"]
        .as_str()
        .expect("a capture answers with the address")
        .to_string();
    for source in [&first, &second] {
        s.call(
            "update_fact",
            json!({"address": &summary, "stands_for": [source], "provenance": "testimony"}),
        )
        .await;
    }

    // The ordinary read shows the newest mark alone.
    let now = s.recall("thing:shelf").await;
    now.claim(&summary);
    let current = now.json()["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| {
            facts
                .iter()
                .find(|fact| fact["address"] == summary.as_str())
        })
        .cloned()
        .unwrap_or_else(|| panic!("the summary is on the shelf: {}", now.raw()));
    assert_eq!(
        current["stands_for"],
        json!([second]),
        "the ordinary read shows the newest mark: {}",
        now.raw()
    );

    // The history shows the set each write had.
    let asked = s
        .shape(
            "what did the summary stand for before?",
            json!({"history_record": &summary}),
        )
        .await;
    asked.number("/objects/0/record_history/count", 3);
    let writes = asked.json()["objects"][0]["record_history"]["writes"].clone();
    assert_eq!(
        writes[0]["refs"],
        json!(["person:teddy"]),
        "the first write does not show what it pointed at: {}",
        asked.raw()
    );
    assert_eq!(writes[0]["stands_for"], json!([]), "{}", asked.raw());
    assert_eq!(
        writes[1]["stands_for"],
        json!([first]),
        "the first mark was erased by the second: {}",
        asked.raw()
    );
    assert_eq!(writes[2]["stands_for"], json!([second]), "{}", asked.raw());
}
