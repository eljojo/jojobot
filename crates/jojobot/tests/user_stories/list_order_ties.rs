//! "My chores list reads the same way every time."
//!
//! **Two entries that hold one order key read the lower claim number first,
//! whatever the digits sort like as text.** Only a hand-written key makes the tie.
//!
//! The roster names are the cast: Homer's chores.

use serde_json::json;

use super::dsl::Story;
use super::lists::{a_list_on_the_trip, read_back};

/// **Two entries that hold one order key read the lower claim number first.**
/// Only a hand-written key makes the tie, so the two are written in the store
/// under the server. Claim 9 and claim 10 sort the other way round as text. The
/// paired control is the entries around them, whose keys differ and read by key.
#[tokio::test]
async fn entries_with_one_order_key_read_by_claim_number_not_by_text() {
    use jojobot_domain::memory::{EntityId, NewFact};

    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    a_list_on_the_trip(&s, "homer-chores", json!({"tick": "sink"})).await;
    // The list's own first claim is `f1`: nine more entries make the last two
    // `f9` and `f10`.
    let memory = story.memory_store();
    let today: jiff::civil::Date = "2026-10-01".parse().expect("a day");
    let list = EntityId("list:homer-chores".into());
    let keys = ["b", "c", "d", "e", "f", "g", "h", "n", "n"];
    let words = [
        "mow", "rake", "sweep", "mulch", "prune", "weed", "edge", "water", "trim",
    ];
    let mut ids = Vec::new();
    for (key, word) in keys.iter().zip(words) {
        let mut entry = NewFact::about(list.clone(), word.to_string(), today);
        entry.fields = [("order".to_string(), key.to_string())]
            .into_iter()
            .collect();
        let jojobot_domain::memory::Guarded::Written(fact) =
            memory.capture(entry).await.expect("an entry lands")
        else {
            panic!("an entry on a list that exists lands");
        };
        ids.push(fact.id.0.clone());
    }
    story.the_store_changed_underneath().await;
    assert_eq!(
        ids[7..],
        ["f9".to_string(), "f10".to_string()],
        "the two that share a key are claims 9 and 10: {ids:?}"
    );

    assert_eq!(
        read_back(&s, "list:homer-chores").await,
        [
            "mow", "rake", "sweep", "mulch", "prune", "weed", "edge", "water", "trim"
        ]
    );
    story.finish().await;
}

/// **Two sections of a document that hold one order key read by claim number
/// too**, because a document's sections and a list's entries are placed by the
/// same function. The tie is made under the server for the same reason: a caller
/// who sends `order` has the server's own key written in its place, so no call
/// makes a tie. Claims 9 and 10 sort the other way round as text. The paired
/// control is the sections around them, whose keys differ and read by key.
#[tokio::test]
async fn document_sections_with_one_order_key_read_by_claim_number_not_by_text() {
    use jojobot_domain::memory::{EntityId, NewFact};

    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("document:homer", "Homer").await;
    let memory = story.memory_store();
    let today: jiff::civil::Date = "2026-10-01".parse().expect("a day");
    let document = EntityId("document:homer".into());
    let keys = ["a", "b", "c", "d", "e", "f", "g", "h", "n", "n"];
    let words = [
        "wake", "mow", "rake", "sweep", "mulch", "prune", "weed", "edge", "water", "trim",
    ];
    let mut ids = Vec::new();
    for (key, word) in keys.iter().zip(words) {
        let mut section = NewFact::about(document.clone(), word.to_string(), today);
        section.fields = [("order".to_string(), key.to_string())]
            .into_iter()
            .collect();
        let jojobot_domain::memory::Guarded::Written(fact) =
            memory.capture(section).await.expect("a section lands")
        else {
            panic!("a section on a document that exists lands");
        };
        ids.push(fact.id.0.clone());
    }
    story.the_store_changed_underneath().await;
    assert_eq!(
        ids[8..],
        ["f9".to_string(), "f10".to_string()],
        "the two that share a key are claims 9 and 10: {ids:?}"
    );

    let read = s
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    let text = read["markdown"]
        .as_str()
        .unwrap_or_else(|| panic!("{read}"));
    let at = |word: &str| {
        text.find(word)
            .unwrap_or_else(|| panic!("{word} in {text}"))
    };
    let positions: Vec<usize> = words.iter().map(|word| at(word)).collect();
    assert!(
        positions.windows(2).all(|pair| pair[0] < pair[1]),
        "the sections read in key order and the tied pair by claim number: {text}"
    );
    // The addresses the read names carry the same order: the tied pair last, 9 then 10.
    let addresses: Vec<&str> = read["addresses"]
        .as_array()
        .expect("the read names its sections")
        .iter()
        .map(|address| address.as_str().expect("an address"))
        .collect();
    assert_eq!(
        addresses[8..],
        ["document:homer#f9", "document:homer#f10"],
        "{read}"
    );
    story.finish().await;
}
