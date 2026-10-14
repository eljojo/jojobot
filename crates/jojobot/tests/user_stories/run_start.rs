//! "I kept two thoughts in my second run. Why does the first read as aged?"
//!
//! A bot holds a room of three thoughts that age after two runs. Run one writes
//! three thoughts and wraps. Run two keeps the first two, and run three reads the
//! room. **A write is never stamped as earlier than the run that made it**, so a
//! thought kept in run two was touched at or after run two began and reads live,
//! whichever call was the run's first. The thought nobody kept ages, in both
//! orders: that is what shows the ageing is on.

use serde_json::json;

use super::dsl::{Answer, Story};

/// Three runs of `bot:otto`, and what the third reads of the room. In run two
/// the first act is a `keep` on the first two thoughts, or a journal entry and
/// then the keeps.
async fn the_room_in_the_third_run(journal_first: bool) -> (Answer, Vec<String>) {
    let story = Story::begin("bot:otto").await;
    let first = story.session().await;
    first.add("bot:marge", "Marge").await;
    // The ceilings of a room are set by another identity, never by its bot.
    let marge = story.as_bot("bot:marge").await;
    marge
        .call(
            "capture",
            json!({
                "subject": "bot:otto", "content": "the room's limits",
                "provenance": "testimony",
                "sets": {"thought_capacity": "3", "thought_ages_after_runs": "2"},
            }),
        )
        .await;

    let mut thoughts = Vec::new();
    for (thing, name) in [
        ("thing:the-couch", "The Couch"),
        ("thing:the-radiator", "The Radiator"),
        ("thing:jukebox", "The Jukebox"),
    ] {
        first.add(thing, name).await;
        let thought = first
            .call(
                "capture",
                json!({
                    "subject": "bot:otto", "content": format!("{name} needs attention"),
                    "shape": "connection", "object": thing,
                }),
            )
            .await
            .field("address");
        thoughts.push(thought);
    }
    first.wrap("wrote three thoughts").await;

    let second = story.session().await;
    if journal_first {
        second.journal("starting the second run").await;
    }
    for thought in &thoughts[..2] {
        second
            .call("update_fact", json!({"address": thought, "keep": true}))
            .await;
    }
    second.wrap("kept the first two thoughts").await;

    let third = story.session().await;
    third.journal("starting the third run").await;
    let room = third.recall("bot:otto").await;
    story.finish().await;
    (room, thoughts)
}

#[tokio::test]
async fn a_thought_kept_as_a_runs_first_act_reads_live_in_the_next_run() {
    let (room, thoughts) = the_room_in_the_third_run(false).await;

    room.claim(&thoughts[0]).never_says("aged_out");
    room.claim(&thoughts[1]).never_says("aged_out");
    room.claim(&thoughts[2]).says("\"aged_out\":true");
}

#[tokio::test]
async fn a_thought_kept_after_the_runs_first_journal_entry_reads_live_in_the_next_run() {
    let (room, thoughts) = the_room_in_the_third_run(true).await;

    room.claim(&thoughts[0]).never_says("aged_out");
    room.claim(&thoughts[1]).never_says("aged_out");
    room.claim(&thoughts[2]).says("\"aged_out\":true");
}
