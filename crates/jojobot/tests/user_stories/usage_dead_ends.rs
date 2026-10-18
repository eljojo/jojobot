//! "I asked for a week of usage and got nothing. What do I read instead?"
//!
//! **An agent that has never seen usage reaches the read that answers from any
//! refusal or empty answer it meets.** A week with no run, compaction or call
//! has no record, so a read of it finds nothing, and none of the answers below
//! may send the caller to `add_entity` or tell it to send the same call again:
//! nothing creates a week, and the call itself is what changes.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_read_that_finds_no_week_names_the_read_that_lists_the_weeks() {
    let story = Story::begin_pretending_it_is("bot:bart", "2026-10-07").await;
    let bart = story.session().await;
    bart.journal("working").await;

    // ── a week the bot did nothing in, and a mistyped handle ────────────────
    for asked in ["usage:bart-2026-w40", "usage:bart-2026-w401"] {
        let none = bart.refused("recall", json!({"subject": asked})).await;
        none.says("\"fix_by\":\"change\"")
            .says("kind: usage")
            .never_says("add_entity")
            .never_says("re-call");
    }
    // The read the refusal names lists the week the bot did have.
    let weeks = bart
        .call(
            "recall",
            json!({"view": "usage", "inputs": {"bot": "bot:bart"}}),
        )
        .await;
    weeks.says("\"id\":\"usage:bart-2026-w41\"");

    // ── a caller that sent no session ───────────────────────────────────────
    let (anonymous, _) = story.call("recall", json!({"kind": "usage"})).await;
    anonymous
        .says("withheld_note")
        .says("start_here")
        .never_says("another identity");

    // ── a write onto a week ─────────────────────────────────────────────────
    let captured = bart
        .refused(
            "capture",
            json!({"subject": "usage:bart-2026-w40", "content": "fewer compactions"}),
        )
        .await;
    captured
        .says("\"fix_by\":\"change\"")
        .says("recall")
        .never_says("add_entity")
        .never_says("again");
    let created = bart
        .refused(
            "add_entity",
            json!({
                "kind": "usage", "handle": "usage:bart-2026-w43",
                "name": "A week to come", "source": "the story",
            }),
        )
        .await;
    created
        .says("\"fix_by\":\"change\"")
        .says("recall")
        .never_says("add_entity")
        .never_says("again");
    story.finish().await;
}
