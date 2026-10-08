//! "It refused me. Is it worth trying again, or is something else needed?"
//!
//! **Every refusal says who fixes it, in a word read before the prose.** `retry`
//! means the same call may succeed later; `change` means the call itself has to
//! change, and the refusal names what; `person` means damage or a decision only
//! a person can make, where sending the call again will not help. An agent that
//! met a refusal and had to read a paragraph to learn whether to try again would
//! try again, so the word comes first.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_refusal_says_whether_a_person_is_needed_before_anything_is_retried() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:sigma", "Sigma").await;
    let sigma = story.as_bot("bot:sigma").await;

    // ── the positive: an ordinary read is no refusal and carries no word ────
    sigma.drain().await.never_says("fix_by");

    // ── damage: a bot holding two boxes. The first answer names the person ───
    story.plant_a_second_box("bot:sigma", "kiln-spare").await;
    let met = sigma.refused("read_mailbox", json!({})).await;
    assert_eq!(met.json()["fix_by"], "person", "{}", met.raw());
    let raw = met.raw();
    assert!(
        raw.find("\"fix_by\"") < raw.find("\"how_to_proceed\""),
        "the word is read before the prose: {raw}"
    );
    // Trying again changes nothing, which is what the word said.
    let again = sigma.refused("read_mailbox", json!({})).await;
    assert_eq!(again.json()["fix_by"], "person", "{}", again.raw());

    // ── a mistake in the call: the word says the call has to change ─────────
    let mistaken = s
        .refused("read_message", json!({"message_id": "no-such-message"}))
        .await;
    assert_eq!(mistaken.json()["fix_by"], "change", "{}", mistaken.raw());
    mistaken.never_says("\"fix_by\":\"person\"");

    s.wrap("met a refusal that needed a person and one that needed a change")
        .await;
    story.finish().await;
}
