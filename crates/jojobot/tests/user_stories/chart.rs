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

/// "I made a new bot and said who it reports to. Does that count as placing it
/// in the chart?" It does, and the same licence applies as for a capture: a
/// creation naming a manager is judged on who is asking, so a stranger cannot
/// hang a new bot under a manager it does not own.
#[tokio::test]
async fn a_new_bot_is_placed_under_a_manager_only_by_whom_may_place_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    let omega = story.as_bot("bot:omega").await;
    let creating = || {
        json!({
            "kind": "bot", "handle": "sigma", "name": "Sigma", "source": "user-named",
            "sets": {"reports_to": "bot:omega"},
        })
    };

    // ── a stranger's creation naming omega as the manager is refused ────────
    s.refused("add_entity", creating())
        .await
        .says("\"wrote\":false")
        .says("bot:omega");
    s.call("list_entities", json!({"kind": "bot"}))
        .await
        .never_says("bot:sigma");

    // ── the manager's own creation lands, and the bot reads back under it ───
    omega.call("add_entity", creating()).await;
    s.call("recall", json!({"subject": "bot:sigma"}))
        .await
        .says("\"reports_to\":\"bot:omega\"");

    s.wrap("placed a new bot under its manager").await;
    story.finish().await;
}

#[tokio::test]
async fn the_head_of_a_chart_places_itself() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    for (handle, name) in [
        ("bot:alpha", "Alpha"),
        ("bot:beta", "Beta"),
        ("bot:gamma", "Gamma"),
        ("bot:omega", "Omega"),
    ] {
        s.add(handle, name).await;
    }
    let alpha = story.as_bot("bot:alpha").await;
    let omega = story.as_bot("bot:omega").await;

    // ── alpha heads a chart: beta reports to it, and alpha reports to none ──
    alpha
        .call("capture", reporting_to("bot:beta", "bot:alpha"))
        .await;
    s.call("recall", json!({"subject": "bot:beta"}))
        .await
        .says("\"reports_to\":\"bot:alpha\"");

    // ── a bot with no manager cannot name itself the head's manager ─────────
    //
    // Omega is the bot named, so adoption alone would allow it, and neither
    // side has a chain for the cycle check to read. The refusal names the head
    // as the one who may.
    omega
        .refused("capture", reporting_to("bot:alpha", "bot:omega"))
        .await
        .says("\"wrote\":false")
        .says("bot:alpha")
        .never_says("operator");
    s.call("recall", json!({"subject": "bot:alpha"}))
        .await
        .never_says("\"reports_to\":\"bot:omega\"");

    // ── the same through an edit of a record alpha already has ──────────────
    let first = s
        .call(
            "capture",
            json!({"subject": "bot:alpha", "content": "alpha heads the chart",
                   "provenance": "testimony"}),
        )
        .await;
    let address = first.field("address");
    omega
        .refused(
            "update_fact",
            json!({"address": address, "fields": {"reports_to": "bot:omega"}}),
        )
        .await
        .says("\"wrote\":false")
        .says("bot:alpha")
        .never_says("operator");
    s.call("recall", json!({"subject": "bot:alpha"}))
        .await
        .never_says("\"reports_to\":\"bot:omega\"");

    // ── a thing with no reports is still adopted by the manager it names ────
    omega
        .call("capture", reporting_to("bot:gamma", "bot:omega"))
        .await;
    s.call("recall", json!({"subject": "bot:gamma"}))
        .await
        .says("\"reports_to\":\"bot:omega\"");

    // ── and the head places itself ──────────────────────────────────────────
    alpha
        .call("capture", reporting_to("bot:alpha", "bot:omega"))
        .await;
    s.call("recall", json!({"subject": "bot:alpha"}))
        .await
        .says("\"reports_to\":\"bot:omega\"");

    s.wrap("looked at who may place the head of a chart").await;
    story.finish().await;
}
