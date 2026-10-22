//! "I declared that a pledge needs a witness, and the write said so."
//!
//! A type can carry a rule its claims must meet. The rule is one saved
//! expression over what a claim carries, and a claim that does not meet it is
//! turned away, naming the rule, the keys it asks for and the call that fixes
//! it. A claim that meets it lands whatever the value says, and a blank value
//! is no value. A rule that does not read is refused when it is declared.
//!
//! **A rule is a norm, and no norm is a dead end**: the refusal hands back a
//! token, and the same call with the token and a written reason lands the claim.
//! The claim carries the reason, and the thing waits in defrag's queue again.

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

/// The token a refusal names, read out of its advice as a caller has to.
fn token_in(refusal: &serde_json::Value) -> String {
    refusal["how_to_proceed"]
        .as_str()
        .and_then(|advice| advice.split("override_token: \"").nth(1))
        .and_then(|rest| rest.split('"').next())
        .expect("the advice names the token")
        .to_string()
}

/// What defrag's queue holds now.
async fn queue(defrag: &super::dsl::Session) -> Vec<String> {
    defrag
        .call("recall", json!({"unreviewed": true}))
        .await
        .json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn a_refused_claim_lands_with_its_token_and_a_written_reason_and_the_thing_waits_for_review()
{
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.call(
        "declare_type",
        pledge_type("!(purpose = pledge) || witness || receipt_note"),
    )
    .await;
    let defrag = story.as_bot("bot:defrag").await;
    defrag
        .call(
            "update_entity",
            json!({"handle": "thing:kettle", "reviewed": true}),
        )
        .await;
    assert!(
        !queue(&defrag).await.contains(&"thing:kettle".to_string()),
        "the kettle is reviewed before the pledge"
    );

    let claim = |extra: serde_json::Value| {
        let mut call = json!({"subject": "thing:kettle", "content": "will bring the cups",
                              "provenance": "testimony", "fields": {"purpose": "pledge"}});
        for (key, value) in extra.as_object().expect("an object") {
            call[key] = value.clone();
        }
        call
    };

    // ── the refusal names the way through ───────────────────────────────────
    let refused = s.refused("capture", claim(json!({}))).await;
    refused.says("override_token").says("override_reason");
    let token = token_in(&refused.json());

    // ── a token with no reason, and a reason with another token, lift nothing ─
    s.refused("capture", claim(json!({"override_token": token})))
        .await
        .says("reason");
    s.refused(
        "capture",
        claim(json!({"override_token": "not-the-token", "override_reason": "because"})),
    )
    .await;
    assert!(
        !queue(&defrag).await.contains(&"thing:kettle".to_string()),
        "a refused claim wrote nothing, so the kettle is still reviewed"
    );

    // ── the token with a written reason lands the claim, which keeps the reason ─
    s.call(
        "capture",
        claim(json!({"override_token": token, "override_reason": "the log was lost"})),
    )
    .await;
    let held = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    // The spelling is stored, so it is pinned by this literal.
    assert_eq!(
        held["objects"][0]["facts"][0]["fields"]["must_overridden"], "pledge: the log was lost",
        "{held}"
    );
    assert!(
        queue(&defrag).await.contains(&"thing:kettle".to_string()),
        "the claim that landed under an override puts the kettle back in the queue"
    );
    story.finish().await;
}

#[tokio::test]
async fn an_edit_the_rule_turns_away_lands_with_its_token_and_a_written_reason() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    s.call("declare_type", pledge_type("witness || receipt_note"))
        .await;
    // A claim written while the rule asked for a different key, to be edited.
    let first = s
        .call(
            "capture",
            json!({"subject": "thing:kettle", "content": "will bring the cups",
                   "provenance": "testimony", "fields": {"witness": "the log"}}),
        )
        .await
        .json();
    let address = first["address"].as_str().expect("an address").to_string();

    // ── taking the only key off breaks the rule, and the edit is refused ────
    let edit = |extra: serde_json::Value| {
        let mut call = json!({"address": address, "clear_fields": ["witness"]});
        for (key, value) in extra.as_object().expect("an object") {
            call[key] = value.clone();
        }
        call
    };
    let refused = s.refused("update_fact", edit(json!({}))).await;
    refused.says("override_token");
    let token = token_in(&refused.json());

    // ── the token with a written reason lands the edit ──────────────────────
    s.call(
        "update_fact",
        edit(json!({"override_token": token, "override_reason": "the log was lost"})),
    )
    .await;
    let held = s
        .call("recall", json!({"subject": "thing:kettle", "facts": true}))
        .await
        .json();
    assert_eq!(
        held["objects"][0]["facts"][0]["fields"]["must_overridden"], "pledge: the log was lost",
        "{held}"
    );
    story.finish().await;
}

/// **The verbs that carry no override still name a way through.** A rule that
/// turns away the claim of an `update_entity` or an `add_entity` names the keys
/// it asks for, the argument they go in on that verb, and the verbs whose refusal
/// carries the override.
#[tokio::test]
async fn a_verb_without_an_override_names_the_key_and_the_verbs_that_have_one() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thing:kettle", "Kettle").await;
    // The rule asks a thing set through these verbs to carry a `witness`.
    s.call("declare_type", pledge_type("witness || receipt_note"))
        .await;

    let refused = s
        .refused(
            "update_entity",
            json!({"handle": "thing:kettle", "sets": {"colour": "red"}}),
        )
        .await;
    refused
        .says("witness or receipt_note")
        .says("`sets`")
        .says("capture")
        .says("update_fact")
        .says("override_token");

    let created = s
        .refused(
            "add_entity",
            json!({"kind": "thing", "handle": "teapot", "name": "Teapot",
                   "source": "user-named", "sets": {"colour": "red"}}),
        )
        .await;
    created
        .says("witness or receipt_note")
        .says("`sets`")
        .says("capture")
        .says("override_token");
    story.finish().await;
}
