//! "I renamed the dog and wrote down a vet visit. What did that change?"
//!
//! A caller knows what it sent and not what the write did. Every write answers
//! with a `changed` block, so the receipt is enough: a rename names the old
//! and the new display name, and a capture names the claim it made and each
//! key it set on the thing. A key that was not there before has no `from`.
//! A call that changes nothing says so with one word, never with an empty list.
//! The story reads the two receipts and nothing else — no recall.

use serde_json::json;

use super::dsl::Story;

const DOG: &str = "pet:santas-little-helper";
const OLD_NAME: &str = "Santa's Little Helper";
const NEW_NAME: &str = "Santa's Little Helper, the greyhound";
const VISIT: &str = "2026-10-12";

#[tokio::test]
async fn a_receipt_says_what_the_write_changed_with_no_second_read() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add(DOG, OLD_NAME).await;

    // The rename: one entry, on the pet, with the name on both sides.
    let rename = json!({"handle": DOG, "name": NEW_NAME});
    let renamed = s.call("update_entity", rename.clone()).await.json();
    assert_eq!(
        renamed["changed"],
        json!([{"on": DOG, "what": "name", "from": OLD_NAME, "to": NEW_NAME}]),
        "the rename names the old and the new name: {renamed}"
    );

    // The vet visit: a new claim, and a key the pet did not hold before.
    let visit = s
        .call(
            "capture",
            json!({
                "subject": DOG,
                "content": "Went to the vet for a check-up",
                "provenance": "testimony",
                "sets": {"last_vet_visit": VISIT},
            }),
        )
        .await
        .json();
    let address = visit["address"].as_str().expect("the receipt's address");
    let changed = visit["changed"].as_array().expect("a list of changes");
    assert!(
        changed
            .iter()
            .any(|c| c["what"] == "created" && c["on"] == address),
        "the new claim is named at the address the receipt gives: {visit}"
    );
    let key = changed
        .iter()
        .find(|c| c["what"] == "key" && c["key"] == "last_vet_visit")
        .unwrap_or_else(|| panic!("the key the call set is named: {visit}"));
    assert_eq!(key["on"], DOG, "{key}");
    assert_eq!(key["to"], VISIT, "{key}");
    assert!(key.get("from").is_none(), "a new key has no `from`: {key}");

    // The pair: the same rename again changes nothing, and says so.
    let again = s.call("update_entity", rename).await.json();
    assert_eq!(again["changed"], "nothing", "{again}");

    story.finish().await;
}

#[tokio::test]
async fn a_receipt_names_what_the_write_changed_beside_its_own_thing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // A new colleague: the bot, and the box that opens with it.
    let added = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "nelson", "name": "Nelson", "source": "user-named"}),
        )
        .await
        .json();
    assert_eq!(
        added["changed"],
        json!([
            {"on": "bot:nelson", "what": "created"},
            {"on": "nelson", "what": "created", "key": "mailbox"},
        ]),
        "the box is named beside the bot: {added}"
    );
    // The pair: a thing that is not a bot opens no box.
    s.add("pet:santas-little-helper", "Santa's Little Helper")
        .await;

    // A message, then its replacement: the old one is named with the new id.
    let first = s
        .call(
            "post_message",
            json!({"to": "nelson", "body": "Standup is at nine"}),
        )
        .await
        .json();
    let replacement = s
        .call(
            "post_message",
            json!({
                "to": "nelson",
                "body": "Standup is at ten",
                "supersedes": first["id"],
            }),
        )
        .await
        .json();
    assert_eq!(
        replacement["changed"],
        json!([
            {"on": replacement["id"], "what": "created"},
            {"on": first["id"], "what": "superseded_by", "to": replacement["id"]},
        ]),
        "the replaced message is named: {replacement}"
    );
    assert_eq!(
        first["changed"],
        json!([{"on": first["id"], "what": "created"}]),
        "a post that replaces nothing names only itself: {first}"
    );

    // The new colleague's first beat sends no focus; the run still takes one.
    let n = story.as_bot("bot:nelson").await;
    let beat = n
        .call("journal", json!({"entry": "read the standup notice"}))
        .await
        .json();
    let focus = beat["changed"]
        .as_array()
        .and_then(|changed| changed.iter().find(|c| c["what"] == "focus"))
        .unwrap_or_else(|| panic!("the derived focus is named: {beat}"));
    assert_eq!(focus["to"], "read the standup notice", "{beat}");
    assert!(focus.get("from").is_none(), "nothing was held: {beat}");

    story.finish().await;
}
