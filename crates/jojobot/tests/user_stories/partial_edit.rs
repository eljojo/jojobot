//! "The third paragraph of a long note has a typo. Fix that, and nothing else."
//!
//! **An edit can name the words to change instead of resending the text.**
//! `update_fact` takes `find`, `replace` and `field`. The words have to occur
//! exactly once in the text as it stands, and the answer shows what was taken
//! out and what was put in, so a loss is visible at once.
//!
//! **The negatives are what make the edit safe.** Words that occur twice, and
//! words that occur nowhere, are refused with the count and the text is
//! unchanged. An edit made against a revision the claim has moved past is
//! refused too.

use serde_json::{Value, json};

use super::dsl::Story;

const NOTE: &str = "first paragraph\n\nsecond paragraph has a typo in it\n\nthird paragraph";

/// The claim's `details` as `recall` serves them, whole.
fn details_of(recalled: &Value, address: &str) -> Value {
    recalled["objects"][0]["facts"]
        .as_array()
        .and_then(|facts| facts.iter().find(|fact| fact["address"] == address))
        .map(|fact| fact["details"].clone())
        .unwrap_or_else(|| panic!("the claim {address} is not in {recalled}"))
}

#[tokio::test]
async fn one_span_of_a_long_note_is_changed_and_every_other_byte_stays() {
    let story = Story::begin("bot:gamma").await;
    let run = story.session().await;
    run.add("person:kenny", "Kenny").await;
    let claim = run
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "a long note",
                "details": NOTE, "provenance": "inference",
            }),
        )
        .await
        .json();
    let claim = claim["address"]
        .as_str()
        .or_else(|| claim["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();

    // **The words occur three times: refused with the count, text unchanged.**
    let refused = run
        .refused(
            "update_fact",
            json!({
                "address": claim, "find": "paragraph", "replace": "part",
                "field": "details", "provenance": "inference",
            }),
        )
        .await;
    refused.says("\"matches\":3").says("\"fix_by\":\"change\"");
    assert_eq!(
        details_of(&run.recall("person:kenny").await.json(), &claim),
        NOTE
    );

    // **Words that occur nowhere: the count is 0.**
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "a TYPO", "replace": "a fix",
            "field": "details", "provenance": "inference",
        }),
    )
    .await
    .says("\"matches\":0");

    // **One occurrence lands, and the answer shows what changed.**
    let receipt = run
        .call(
            "update_fact",
            json!({
                "address": claim, "find": "a typo", "replace": "a fix",
                "field": "details", "provenance": "inference",
            }),
        )
        .await
        .json();
    assert_eq!(receipt["text_edit"]["removed"], "a typo", "{receipt}");
    assert_eq!(receipt["text_edit"]["inserted"], "a fix", "{receipt}");
    assert_eq!(
        receipt["text_edit"]["bytes_after"].as_u64().unwrap() + 1,
        receipt["text_edit"]["bytes_before"].as_u64().unwrap(),
        "{receipt}"
    );
    let recalled = run.recall("person:kenny").await.json();
    assert_eq!(
        details_of(&recalled, &claim),
        "first paragraph\n\nsecond paragraph has a fix in it\n\nthird paragraph"
    );

    // **A caller that read an earlier revision is refused, whatever the words.**
    run.refused(
        "update_fact",
        json!({
            "address": claim, "find": "a fix", "replace": "a better fix",
            "field": "details", "provenance": "inference", "expected_revision": 1,
        }),
    )
    .await
    .says("\"fix_by\":\"retry\"")
    .says("expected_revision: 2");
    story.finish().await;
}
