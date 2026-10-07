//! "Who does this bot report to, and who may change it?"
//!
//! The chart is changed only by a superior. A bot with a manager changes it
//! through the bots above it; a bot with none takes the manager named by that
//! manager, or by a bot above it; and nobody is put under one of its own
//! reports. The four keys a bot cannot write about itself keep their own
//! relation, and the refusals name the bots that may make the write.

use serde_json::json;

use super::dsl::Story;

fn reporting_to(subject: &str, manager: &str) -> serde_json::Value {
    json!({
        "subject": subject, "content": format!("{subject} reports to {manager}"),
        "provenance": "testimony", "fields": {"reports_to": manager},
    })
}

#[tokio::test]
async fn the_chart_is_changed_only_by_a_superior() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:sigma", "Sigma").await;
    let omega = story.as_bot("bot:omega").await;
    let sigma = story.as_bot("bot:sigma").await;

    // ── a bot with no manager takes the one named by that manager ───────────
    //
    // A stranger cannot place sigma under omega; omega adopts it.
    s.refused("capture", reporting_to("bot:sigma", "bot:omega"))
        .await
        .says("\"wrote\":false")
        .says("bot:omega");
    omega
        .call("capture", reporting_to("bot:sigma", "bot:omega"))
        .await;
    s.call("recall", json!({"subject": "bot:sigma"}))
        .await
        .says("\"reports_to\":\"bot:omega\"");

    // ── a bot cannot change who it reports to, and the refusal says who may ─
    let refused = sigma
        .refused("capture", reporting_to("bot:sigma", "bot:otto"))
        .await;
    refused.says("\"wrote\":false").says("bot:omega");
    refused.never_says("operator");
    s.call("recall", json!({"subject": "bot:sigma"}))
        .await
        .says("\"reports_to\":\"bot:omega\"");

    // ── nobody is put under one of its own reports, whoever asks ────────────
    //
    // Omega has no manager, and sigma is the bot named: adoption would allow
    // it, and the chart would invert.
    sigma
        .refused("capture", reporting_to("bot:omega", "bot:sigma"))
        .await
        .says("\"wrote\":false")
        .says("bot:sigma");
    omega
        .refused("capture", reporting_to("bot:omega", "bot:sigma"))
        .await
        .says("\"wrote\":false");

    // ── its manager changes it, and it lands ────────────────────────────────
    omega
        .call("capture", reporting_to("bot:sigma", "bot:otto"))
        .await;
    s.call("recall", json!({"subject": "bot:sigma"}))
        .await
        .says("\"reports_to\":\"bot:otto\"");

    // ── the four keys a bot cannot write about itself behave as they did ────
    sigma
        .refused(
            "capture",
            json!({
                "subject": "bot:sigma", "content": "more seats for me",
                "provenance": "testimony", "fields": {"rule_seats": "9"},
            }),
        )
        .await
        .says("different identity");
    omega
        .call(
            "capture",
            json!({
                "subject": "bot:sigma", "content": "more seats for sigma",
                "provenance": "testimony", "fields": {"rule_seats": "9"},
            }),
        )
        .await;

    s.wrap("looked at who may change the chart").await;
    story.finish().await;
}
