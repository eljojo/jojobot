//! "I told my assistant the furnace's service is due in March, and it filed
//! that under the company that services it. Does anything tell it the furnace
//! is a place the claim could belong?"
//!
//! The write is never refused: the claim may be exactly where it belongs. The
//! receipt of a claim whose words say another known thing's whole name or
//! alias says so, names the route that moves a claim, and says it once per
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
                "content": "The air filter was swapped in June",
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

    // A long look-back that shares single words with a thing, an org
    // and a project names none of them ...
    s.add("thing:dining-chairs", "Dining chairs").await;
    s.add("project:kitchen-floor", "Kitchen floor").await;
    s.add("org:springfield-movers", "Springfield movers").await;
    let look_back = "Looked back on the week: the chairs were scuffed, the kitchen is clean, the \
                     floor was waxed and Springfield was quiet.";
    let single = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": look_back,
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    assert!(single.get("address").is_some(), "the write lands: {single}");
    assert!(
        single.get("possible_subject_note").is_none(),
        "single shared words draw no note: {single}"
    );
    // ... and the same claim saying one thing's whole name does.
    let whole = s
        .call(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": format!("{look_back} The dining chairs are next."),
                "provenance": "testimony",
            }),
        )
        .await
        .json();
    let note = whole["possible_subject_note"]
        .as_str()
        .unwrap_or_else(|| panic!("the whole name is noted: {whole}"));
    assert!(note.contains("thing:dining-chairs"), "{note}");
    assert!(!note.contains("project:kitchen-floor"), "{note}");

    story.finish().await;
}
