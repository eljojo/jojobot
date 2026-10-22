//! "I declared that a pledge needs a witness, and the write said so."
//!
//! A type can carry a rule its claims must meet. The rule is one saved
//! expression over what a claim carries, and a claim that does not meet it is
//! turned away, naming the rule, the keys it asks for and the call that fixes
//! it. A claim that meets it lands whatever the value says, and a blank value
//! is no value. A rule that does not read is refused when it is declared.

use serde_json::json;

use super::dsl::Story;

fn pledge_type(must: &str) -> serde_json::Value {
    json!({
        "name": "pledge",
        "fields": [{"key": "witness"}, {"key": "receipt_note"}],
        "must": must,
    })
}

#[tokio::test]
async fn a_claim_missing_what_the_types_rule_asks_for_is_refused_and_says_what_fixes_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    let rule = "!(purpose = pledge) || witness || receipt_note";
    let declared = s.call("declare_type", pledge_type(rule)).await.json();
    assert_eq!(declared["type"]["must"], rule, "{declared}");

    // ── a pledge with neither key is turned away, with the way forward ──────
    let refused = s
        .refused(
            "capture",
            json!({"subject": "thing:kettle", "content": "will bring the cups",
                   "provenance": "testimony", "fields": {"purpose": "pledge"}}),
        )
        .await;
    refused
        .says("pledge")
        .says("witness or receipt_note")
        .says("capture");
    let held = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    assert_eq!(
        held["objects"][0]["facts"].as_array().map(Vec::len),
        Some(0),
        "a refused claim wrote nothing: {held}"
    );

    // ── a blank value is no value ───────────────────────────────────────────
    s.refused(
        "capture",
        json!({"subject": "thing:kettle", "content": "will bring the cups",
               "provenance": "testimony",
               "fields": {"purpose": "pledge", "witness": "   "}}),
    )
    .await;

    // ── any value counts, and so does another purpose ───────────────────────
    s.call(
        "capture",
        json!({"subject": "thing:kettle", "content": "will bring the cups",
               "provenance": "testimony",
               "fields": {"purpose": "pledge", "receipt_note": "see the log"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "thing:kettle", "content": "a plain note",
               "provenance": "testimony", "fields": {"purpose": "memo"}}),
    )
    .await;
    story.finish().await;
}

#[tokio::test]
async fn a_rule_that_does_not_read_is_refused_when_the_type_is_declared() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.refused("declare_type", pledge_type("witness &&")).await;
    let types = s.call("declare_type", pledge_type("witness")).await.json();
    assert_eq!(types["type"]["must"], "witness", "{types}");
    story.finish().await;
}
