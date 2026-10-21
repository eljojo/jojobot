//! "The merge said I could clear the ceiling first, and the clear said no."
//!
//! A merge that would carry a ceiling onto a thread is refused for the bots that
//! write into the thread, and the refusal may name a way forward: take the
//! ceiling off the duplicate first. **The route a refusal names is a route that
//! works**: the clear is judged by the same bots the refusal was judged by, and
//! a bot that wrote a record of the duplicate is one of them. Roster names
//! throughout.

use serde_json::json;

use super::dsl::Story;

const SURVIVOR: &str = "thread:milhouse-moves";
const DUPLICATE: &str = "thread:simpson-kitchen";

/// **bart wrote a note on the duplicate, so bart is one of the bots that write
/// into it, and a clear of its ceiling is refused to bart.** The merge refusal
/// must not name that clear to bart, and bart's clear is refused. Paired with
/// otto, above bart and a writer of no record there: the route is named to otto, the clear lands
/// and the merge then lands.
#[tokio::test]
async fn a_merge_refusal_names_a_clear_only_to_a_caller_whose_clear_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let marge = story.as_bot("bot:marge").await;
    let bart = story.as_bot("bot:bart").await;
    // bart reports to otto, so otto is above bart.
    s.call(
        "capture",
        json!({
            "subject": "bot:bart", "content": "bart reports up",
            "provenance": "testimony", "sets": {"reports_to": "bot:otto"},
        }),
    )
    .await;
    // marge keeps a thought in the survivor, and nobody is above her.
    s.add(SURVIVOR, "Milhouse moves").await;
    marge
        .call(
            "capture",
            json!({
                "subject": "bot:marge", "content": "marge follows it",
                "shape": "connection", "object": SURVIVOR,
            }),
        )
        .await;
    // The duplicate: otto sets its ceiling, then bart writes a note on it.
    s.add(DUPLICATE, "Simpson kitchen").await;
    s.call(
        "capture",
        json!({
            "subject": DUPLICATE, "content": "how many thoughts it holds",
            "sets": {"thought_capacity": "5"},
        }),
    )
    .await;
    bart.call(
        "capture",
        json!({"subject": DUPLICATE, "content": "a note from bart"}),
    )
    .await;
    let merge = json!({"duplicate": DUPLICATE, "survivor": SURVIVOR, "reason": "one line"});
    let address =
        super::thread_records::the_record_setting(&s, DUPLICATE, "thought_capacity").await;
    let clear = json!({"address": address, "clear_fields": ["thought_capacity"]});

    // ── bart wrote on the duplicate: no clear is named, and none would land ──
    bart.refused("merge_entities", merge.clone())
        .await
        .says("thought_capacity")
        .never_says("clear_fields");
    bart.refused("update_fact", clear.clone()).await;

    // ── otto wrote no record there: the clear is named, lands, and the merge too ──
    s.refused("merge_entities", merge.clone())
        .await
        .says("clear_fields");
    s.call("update_fact", clear).await;
    s.call("merge_entities", merge).await;
    s.call("recall", json!({"subject": DUPLICATE}))
        .await
        .says(SURVIVOR);
    story.finish().await;
}

/// **A bot that set a ceiling on a thread and took it off again is no writer of
/// it.** The record that held the ceiling is the setting of it, once or now, and
/// not a fill, so marge, who wrote nothing else there, may still set the thread's
/// record ceiling. Paired with a thread where a bot wrote an ordinary note, who
/// may not.
#[tokio::test]
async fn a_bot_that_set_a_ceiling_and_took_it_off_is_not_made_a_writer_by_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let marge = story.as_bot("bot:marge").await;
    let bart = story.as_bot("bot:bart").await;
    s.add("thread:alpha", "Alpha").await;
    s.add("thread:beta", "Beta").await;
    let address = marge
        .call(
            "capture",
            json!({
                "subject": "thread:alpha", "content": "how many thoughts it holds",
                "sets": {"thought_capacity": "3"},
            }),
        )
        .await
        .field("address");
    marge
        .call(
            "update_fact",
            json!({"address": address, "clear_fields": ["thought_capacity"]}),
        )
        .await;
    bart.call(
        "capture",
        json!({"subject": "thread:beta", "content": "a note from bart"}),
    )
    .await;

    let record_ceiling = |thread: &str| {
        json!({
            "subject": thread, "content": "how many notes it holds",
            "sets": {"record_capacity": "5"},
        })
    };
    // The paired positive: the note's writer is refused where marge is not.
    bart.refused("capture", record_ceiling("thread:beta"))
        .await
        .says("record_capacity");
    marge.call("capture", record_ceiling("thread:alpha")).await;
    story.finish().await;
}
