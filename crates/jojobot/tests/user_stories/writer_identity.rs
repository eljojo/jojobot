//! "I renamed a bot twice. Did its own thoughts stay its own?"
//!
//! A write keeps the bot that made it as the id the bot is filed under, as every
//! other pointer in the store is, so a rename of the bot changes what a write
//! reads as naming and nothing stored. A thought a bot kept on itself keeps its
//! seat in the bot's room through any number of renames. Another bot's claim on
//! the bot never takes a seat.

use serde_json::json;

use super::dsl::Story;

const THREAD: &str = "thread:milhouse-moves";

#[tokio::test]
async fn a_thought_a_bot_kept_holds_its_seat_in_the_bots_room_after_the_bot_is_renamed_twice() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:alpha", "Alpha").await;
    s.add(THREAD, "Milhouse moves").await;
    // The room holds one thought. A bot does not set its own room, so the
    // story's own bot does.
    s.call(
        "capture",
        json!({"subject": "bot:alpha", "content": "alpha's room holds one thought",
               "provenance": "testimony", "sets": {"thought_capacity": "1"}}),
    )
    .await;
    let alpha = story.as_bot("bot:alpha").await;
    let thought = |on: &str, content: &str| json!({"subject": on, "content": content, "shape": "connection", "object": THREAD});
    alpha
        .call("capture", thought("bot:alpha", "the move is on for spring"))
        .await;

    // ── the bot is renamed twice ─────────────────────────────────────────────
    for (from, to) in [("bot:alpha", "bot:beta"), ("bot:beta", "bot:gamma")] {
        s.call("rename_entity", json!({"handle": from, "to": to}))
            .await;
    }

    // ── its first thought still holds the one seat ───────────────────────────
    alpha
        .refused("capture", thought("bot:gamma", "and the van is booked"))
        .await;

    // ── paired: with a second seat the same thought lands ────────────────────
    s.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "gamma's room holds two thoughts",
               "provenance": "testimony", "sets": {"thought_capacity": "2"}}),
    )
    .await;
    alpha
        .call("capture", thought("bot:gamma", "and the van is booked"))
        .await;
    story.finish().await;
}
