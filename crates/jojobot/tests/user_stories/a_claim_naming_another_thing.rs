//! "I told my assistant the furnace's service is due in March, and it filed
//! that under the company that services it. Does anything tell it the furnace
//! is a place the claim could belong?"
//!
//! The write is never refused: the claim may be exactly where it belongs. The
//! receipt of a claim whose words share a rare name word with another known
//! thing says so, names the route that moves a claim, and says it once per
//! thing in a session. A thing the claim already points at is not named.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_receipt_names_another_known_thing_the_claims_words_name_once_a_session() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:the-furnace", "The furnace").await;
    s.add("thing:the-air-filter", "The air filter").await;
    s.add("org:mr-plow", "Mr. Plow").await;

    // The furnace is named in the words, and the claim is filed on the company.
    let filed = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": "The furnace's service is due in March",
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    let address = filed["address"].as_str().expect("the claim's address");
    let note = filed["possible_subject_note"]
        .as_str()
        .unwrap_or_else(|| panic!("the receipt names the furnace: {filed}"));
    assert!(note.contains("thing:the-furnace"), "{note}");
    assert!(
        note.contains(address),
        "the note names the claim to move: {note}"
    );

    // The same thing again in this session: the write lands and the note does not repeat.
    let again = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": "The furnace was cleaned in April",
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    assert!(again.get("address").is_some(), "the write lands: {again}");
    assert!(
        again.get("possible_subject_note").is_none(),
        "the furnace is named once: {again}"
    );

    // A claim that already points at the air filter is not told about it ...
    let pointed = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": "The filter was swapped in May",
                "provenance": "testimony",
                "shape": "about",
                "object": "thing:the-air-filter",
            }),
        )
        .await
        .json();
    assert!(
        pointed.get("possible_subject_note").is_none(),
        "the claim points at the filter: {pointed}"
    );
    // ... and the same words without the pointer name it: the row was not spent.
    let unpointed = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": "The filter was swapped in June",
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    assert!(
        unpointed["possible_subject_note"]
            .as_str()
            .is_some_and(|note| note.contains("thing:the-air-filter")),
        "the filter is named: {unpointed}"
    );

    story.finish().await;
}
