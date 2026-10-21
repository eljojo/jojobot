//! "I made a bot with a one-liner. Why can't I find what I wrote about it?"
//!
//! A caller's first claim on a bot made with settings is an ordinary claim. The
//! bookkeeping of the release notes a bot was told is a record of its own, so it
//! never takes that claim out of the default search, and the claim is found
//! beside the bot it is about.

use serde_json::json;

use super::dsl::Story;

/// **The first claim of a bot made with settings is found by a default search.**
/// It is the claim the creation wrote, carrying the settings, and it is found by
/// the bot's name beside the bot it is about.
#[tokio::test]
async fn the_first_claim_on_a_bot_made_with_settings_is_found_by_a_default_search() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let made = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "marge", "name": "Marge",
                   "source": "user-named", "sets": {"one_liner": "keeps the shelves"}}),
        )
        .await
        .json();
    let first = made["first_claim"]["address"]
        .as_str()
        .unwrap_or_else(|| panic!("a creation with settings names its first claim: {made}"))
        .to_string();

    // The caller's claim, read back as a caller reads it, carries no ledger key
    // among its own words, fields or settings.
    let read = s
        .call("recall", json!({"subject": "bot:marge", "facts": true}))
        .await
        .json();
    let claim = read["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["address"] == first.as_str()))
        .unwrap_or_else(|| panic!("the first claim reads back: {read}"));
    assert!(
        !claim.to_string().contains("release_notes_told"),
        "the caller's claim carries the ledger: {claim}"
    );
    // The creation's own first claim is in the default search for the bot's name.
    let found = s
        .call("search", json!({"query": "Marge", "subject": "bot:marge"}))
        .await
        .json();
    let addresses: Vec<&str> = found["results"]
        .as_array()
        .unwrap_or_else(|| panic!("a search lists results: {found}"))
        .iter()
        .filter_map(|hit| hit["address"].as_str())
        .collect();
    assert!(
        addresses.contains(&first.as_str()),
        "the first claim on the new bot is found: {found}"
    );
    story.finish().await;
}
