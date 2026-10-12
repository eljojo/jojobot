//! "The operator told one run how the shop closes. A later run reads it and
//! thinks it knows better."
//!
//! **What a testimony claim says includes its details**, so a session that did
//! not write the claim does not rewrite them in place, and the refusal names the
//! route: archive the claim with a reason, then capture the correction derived
//! from it. **The session that wrote it still edits its own details**, and a
//! later session archiving it, with the reason in the details, is the route
//! itself.

use serde_json::json;

use super::dsl::Story;

/// **The second run's edit of the details is refused and the claim keeps the
/// operator's words; the route works; the first run still edits its own.**
#[tokio::test]
async fn a_later_session_does_not_rewrite_the_details_of_the_operators_word() {
    let story = Story::begin("bot:gamma").await;
    let first = story.session().await;
    first.add("person:kenny", "Kenny").await;
    let said = first
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "keeps the shop open late",
                "details": "closes at ten", "provenance": "testimony",
            }),
        )
        .await
        .json();
    let claim = said["address"]
        .as_str()
        .or_else(|| said["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    let second = story.session_in("UTC", Some("new")).await;

    // The refusal names the route, and nothing was written.
    let refused = second
        .refused(
            "update_fact",
            json!({
                "address": claim, "field": "details",
                "find": "ten", "replace": "nine",
            }),
        )
        .await;
    refused.says("archive").says("derived_from").says("details");
    second
        .recall("person:kenny")
        .await
        .claim(&claim)
        .says("closes at ten")
        .never_says("closes at nine");

    // The route: archive with the reason in the details, then capture the
    // correction derived from the archived claim.
    second
        .call(
            "update_fact",
            json!({
                "address": claim, "status": "archived",
                "details": "the operator said it closes at nine now",
            }),
        )
        .await;
    second
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "keeps the shop open late",
                "details": "closes at nine", "provenance": "inference",
                "derived_from": claim,
            }),
        )
        .await;
    second
        .recall("person:kenny")
        .await
        .says("closes at nine")
        .says(&claim);

    // The writing run still edits its own details.
    let own = first
        .call(
            "capture",
            json!({
                "subject": "person:kenny", "content": "opens at seven",
                "details": "sharp", "provenance": "testimony",
            }),
        )
        .await
        .json();
    let own = own["address"]
        .as_str()
        .or_else(|| own["fact"]["address"].as_str())
        .expect("a capture hands back its address")
        .to_string();
    first
        .call(
            "update_fact",
            json!({
                "address": own, "field": "details",
                "find": "sharp", "replace": "to the minute",
            }),
        )
        .await;
    first
        .recall("person:kenny")
        .await
        .claim(&own)
        .says("to the minute");
}
