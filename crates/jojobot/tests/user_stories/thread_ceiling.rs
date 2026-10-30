//! "Ralph's assistant keeps adding thoughts to the moving thread, and the
//! thread only holds three. Can it raise the limit itself?"
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
    for bot in ["marge", "patty", "nelson", "moe", "ralph"] {
        s.add(&format!("bot:{bot}"), bot).await;
    }
    let marge = story.as_bot("bot:marge").await;
    let patty = story.as_bot("bot:patty").await;
    let nelson = story.as_bot("bot:nelson").await;
    let ralph = story.as_bot("bot:ralph").await;
    let reports_to = |bot: &str, manager: &str| {
        json!({
            "subject": bot, "content": format!("{bot} reports up"),
            "provenance": "testimony", "sets": {"reports_to": manager},
        })
    };

    // Two charts: ralph reports to patty, patty to marge; nelson to moe.
    let assistant = story.as_bot("bot:assistant").await;
    assistant
        .call("capture", reports_to("bot:patty", "bot:marge"))
        .await;
    assistant
        .call("capture", reports_to("bot:ralph", "bot:patty"))
        .await;
    assistant
        .call("capture", reports_to("bot:nelson", "bot:moe"))
        .await;

    // ── nobody has written into the thread: any bot sets its first ceiling ──
    let address = nelson
        .call(
            "capture",
            json!({
                "subject": "thread:milhouse-moves", "content": "how much the move holds",
                "sets": {"thought_capacity": "3"},
            }),
        )
        .await
        .field("address");
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"3\"");

    // ── ralph writes a thought into the thread ──────────────────────────────
    ralph
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "the move is still on",
                "shape": "connection", "object": "thread:milhouse-moves",
            }),
        )
        .await;
    let raise = |value: &str| json!({"address": address, "sets": {"thought_capacity": value}});

    // ── ralph never raises it, and the refusal names the bots that may ──────
    ralph
        .refused("update_fact", raise("50"))
        .await
        .says("thought_capacity")
        .says("bot:patty")
        .says("bot:marge")
        .says("\"wrote\":false");
    ralph
        .refused(
            "capture",
            json!({
                "subject": "thread:milhouse-moves", "content": "a larger ceiling",
                "sets": {"thought_capacity": "50"},
            }),
        )
        .await
        .says("bot:patty");
    // ── nor does a bot on another chart, which the same write from patty shows
    //    is not about the write ───────────────────────────────────────────────
    nelson
        .refused("update_fact", raise("50"))
        .await
        .says("bot:patty")
        .says("\"wrote\":false");
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"3\"");

    // ── patty, ralph's manager, raises it, and the head above patty does too ──
    patty.call("update_fact", raise("5")).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"5\"");
    marge.call("update_fact", raise("8")).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"8\"");

    // ── taking it off follows the same rule ─────────────────────────────────
    let off = json!({"address": address, "clear_fields": ["thought_capacity"]});
    for outsider in [&ralph, &nelson] {
        outsider
            .refused("update_fact", off.clone())
            .await
            .says("thought_capacity")
            .says("bot:patty");
    }
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_capacity\":\"8\"");
    patty.call("update_fact", off).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .never_says("thought_capacity");

    // ── how long a thought may go untouched on the thread follows the rule ──
    let ages = |value: &str| {
        json!({
            "subject": "thread:milhouse-moves", "content": "how long the move's thoughts last",
            "sets": {"thought_ages_after_runs": value},
        })
    };
    ralph
        .refused("capture", ages("9"))
        .await
        .says("thought_ages_after_runs")
        .says("bot:patty")
        .says("\"wrote\":false");
    nelson
        .refused("capture", ages("9"))
        .await
        .says("bot:patty")
        .says("\"wrote\":false");
    let ageing = patty.call("capture", ages("4")).await.field("address");
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_ages_after_runs\":\"4\"");
    let age = |value: &str| json!({"address": ageing, "sets": {"thought_ages_after_runs": value}});
    ralph
        .refused("update_fact", age("9"))
        .await
        .says("bot:patty");
    nelson
        .refused("update_fact", age("9"))
        .await
        .says("bot:patty");
    marge.call("update_fact", age("6")).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .says("\"thought_ages_after_runs\":\"6\"");
    let off = json!({"address": ageing, "clear_fields": ["thought_ages_after_runs"]});
    ralph
        .refused("update_fact", off.clone())
        .await
        .says("bot:patty");
    patty.call("update_fact", off).await;
    s.call("recall", json!({"subject": "thread:milhouse-moves"}))
        .await
        .never_says("thought_ages_after_runs");

    // ── a bot's own ceiling stays any different identity's to set ───────────
    ralph
        .refused(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "its own room",
                "sets": {"thought_capacity": "9"},
            }),
        )
        .await
        .says("bot:ralph")
        .says("\"wrote\":false");
    nelson
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "its room, set by another",
                "sets": {"thought_capacity": "9"},
            }),
        )
        .await;
    s.call("recall", json!({"subject": "bot:ralph"}))
        .await
        .says("\"thought_capacity\":\"9\"");
    ralph
        .refused(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "its own ageing",
                "sets": {"thought_ages_after_runs": "7"},
            }),
        )
        .await
        .says("bot:ralph")
        .says("\"wrote\":false");
    nelson
        .call(
            "capture",
            json!({
                "subject": "bot:ralph", "content": "its ageing, set by another",
                "sets": {"thought_ages_after_runs": "7"},
            }),
        )
        .await;
    s.call("recall", json!({"subject": "bot:ralph"}))
        .await
        .says("\"thought_ages_after_runs\":\"7\"");

    s.wrap("kept a thread's ceiling above its writers").await;
    story.finish().await;
}
