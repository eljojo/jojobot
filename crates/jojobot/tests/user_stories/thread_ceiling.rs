//! "Who may raise how much a thread holds?"
//!
//! A thread is bounded on purpose, and the bots that write thoughts into it are
//! the ones the bound is for. **A bot that writes into a thread never changes its
//! ceiling; a bot above one of them on the chart does; a bot on another chart does
//! not.** Taking the ceiling off follows the same rule. A thread nobody has written
//! into binds nobody yet, so any bot sets its first ceiling.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_threads_ceiling_is_raised_by_a_bot_above_its_writers_and_never_by_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("thread:milhouse-moves", "Milhouse moves").await;
    for bot in ["alpha", "beta", "sigma", "epsilon", "omega"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let alpha = story.as_bot("bot:alpha").await;
    let beta = story.as_bot("bot:beta").await;
    let sigma = story.as_bot("bot:sigma").await;
    let epsilon = story.as_bot("bot:epsilon").await;
    let omega = story.as_bot("bot:omega").await;
    let reports_to = |bot: &str, manager: &str| {
        json!({
            "subject": bot, "content": format!("{bot} reports up"),
            "provenance": "testimony", "fields": {"reports_to": manager},
        })
    };

    // Two charts: omega reports to beta, beta to alpha; sigma to epsilon.
    alpha
        .call("capture", reports_to("bot:beta", "bot:alpha"))
        .await;
    beta.call("capture", reports_to("bot:omega", "bot:beta"))
        .await;
    epsilon
        .call("capture", reports_to("bot:sigma", "bot:epsilon"))
        .await;

    // ── nobody has written into the thread: any bot sets its first ceiling ──
    let address = sigma
        .call(
            "capture",
            json!({
                "subject": "thread:milhouse-moves", "content": "how much the move holds",
                "fields": {"thought_capacity": "3"},
            }),
        )
        .await
        .field("address");
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"3\"");

    // ── omega writes a thought into the thread ──────────────────────────────
    omega
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "the move is still on",
                "shape": "connection", "object": "thread:milhouse-moves",
            }),
        )
        .await;
    let raise = |value: &str| json!({"address": address, "fields": {"thought_capacity": value}});

    // ── omega never raises it, and the refusal names the bots that may ──────
    omega
        .refused("update_fact", raise("50"))
        .await
        .says("thought_capacity")
        .says("bot:beta")
        .says("bot:alpha")
        .says("\"wrote\":false");
    omega
        .refused(
            "capture",
            json!({
                "subject": "thread:milhouse-moves", "content": "a larger ceiling",
                "fields": {"thought_capacity": "50"},
            }),
        )
        .await
        .says("bot:beta");
    // ── nor does a bot on another chart, which the same write from beta shows
    //    is not about the write ───────────────────────────────────────────────
    sigma
        .refused("update_fact", raise("50"))
        .await
        .says("bot:beta")
        .says("\"wrote\":false");
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"3\"");

    // ── beta, omega's manager, raises it, and the head above beta does too ──
    beta.call("update_fact", raise("5")).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"5\"");
    alpha.call("update_fact", raise("8")).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"8\"");

    // ── taking it off follows the same rule ─────────────────────────────────
    let off = json!({"address": address, "clear_fields": ["thought_capacity"]});
    for outsider in [&omega, &sigma] {
        outsider
            .refused("update_fact", off.clone())
            .await
            .says("thought_capacity")
            .says("bot:beta");
    }
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"8\"");
    beta.call("update_fact", off).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .never_says("thought_capacity");

    // ── a bot's own ceiling stays any different identity's to set ───────────
    omega
        .refused(
            "capture",
            json!({
                "subject": "bot:omega", "content": "its own room",
                "fields": {"thought_capacity": "9"},
            }),
        )
        .await
        .says("bot:omega")
        .says("\"wrote\":false");
    sigma
        .call(
            "capture",
            json!({
                "subject": "bot:omega", "content": "its room, set by another",
                "fields": {"thought_capacity": "9"},
            }),
        )
        .await;
    s.call("recall", json!({"subject": "bot:omega"}))
        .await
        .says("\"thought_capacity\":\"9\"");

    s.wrap("kept a thread's ceiling above its writers").await;
    story.finish().await;
}
