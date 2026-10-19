//! "Where I make a list, I learn which setting fits what I am making."
//!
//! **The text of `add_entity` names the three `tick` settings by what they make**
//! (a checklist, a queue, a rotation) and says an entry is ticked with `capture`.
//! A model reads it there, makes a list from it, and ticks. The story reads the
//! served text, takes the setting for a rotation from it, and acts on it.

use serde_json::json;

use super::dsl::Story;
use super::lists::{a_list_on_the_trip, add_entry};

/// The setting a served text pairs with a noun: the backticked word nearest
/// before the noun, after the first mention of `tick`.
fn setting_for(text: &str, noun: &str) -> String {
    let from = text
        .find("`tick`")
        .unwrap_or_else(|| panic!("the text names tick: {text}"));
    let rest = &text[from..];
    let at = rest
        .find(noun)
        .unwrap_or_else(|| panic!("the text names a {noun}: {text}"));
    let before = &rest[..at];
    let end = before
        .rfind('`')
        .unwrap_or_else(|| panic!("a setting before {noun}: {text}"));
    let start = before[..end]
        .rfind('`')
        .unwrap_or_else(|| panic!("a setting before {noun}: {text}"));
    before[start + 1..end].to_string()
}

#[tokio::test]
async fn the_text_that_makes_a_list_says_which_setting_fits_and_how_to_tick() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    // ── what the text says ──────────────────────────────────────────────────
    let kind = s.argument_description("add_entity", "kind").await;
    assert_eq!(setting_for(&kind, "checklist"), "sink", "{kind}");
    assert_eq!(setting_for(&kind, "queue"), "leave", "{kind}");
    let rotation = setting_for(&kind, "rotation");
    assert_eq!(rotation, "back", "{kind}");
    assert!(
        kind.contains("`capture`") && kind.contains("`act`"),
        "the text says an entry is ticked with capture, naming its act: {kind}"
    );

    // ── what a model does with it ───────────────────────────────────────────
    a_list_on_the_trip(&s, "packing", json!({"tick": rotation})).await;
    let tent = add_entry(&s, "list:packing", "tent", None).await;
    add_entry(&s, "list:packing", "stove", None).await;
    s.call(
        "capture",
        json!({"subject": "list:packing", "content": "done it", "provenance": "testimony",
               "tick": {"entry": tent, "act": "done"}}),
    )
    .await;
    let read = s.call("recall", json!({"subject": "list:packing"})).await;
    let states: Vec<String> = read.json()["objects"][0]["entries"]
        .as_array()
        .unwrap_or_else(|| panic!("a list answers with its entries: {}", read.raw()))
        .iter()
        .map(|entry| {
            format!(
                "{}:{}",
                entry["text"].as_str().unwrap_or_default(),
                entry["state"].as_str().unwrap_or_default()
            )
        })
        .collect();
    assert_eq!(
        states,
        ["stove:open", "tent:open"],
        "the ticked entry went to the end of the rotation and is still open"
    );
    story.finish().await;
}
