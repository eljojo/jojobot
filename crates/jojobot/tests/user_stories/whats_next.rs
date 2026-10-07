//! "What is next, and what is waiting on whom?"
//!
//! A bot's own work lives in jojobot on keys the software ships, so no bot
//! invents its own. A session that was told nothing asks what is next and what
//! waits on whom, and gets both from the keys on the work: the status says
//! where a piece of work stands, `waiting_on` says whose move it is, and
//! `depends_on` lists what stands behind what — a record is found from every
//! item in that list, not only the first. A project may add columns of its own
//! beside the five, and the work filed under it moves through those.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_cold_session_finds_what_is_next_and_what_waits_on_whom_from_the_keys() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("project:atlas", "Atlas").await;
    s.add_under("project:atlas", "work:phi", "Phi").await;
    s.add("work:sigma", "Sigma").await;
    s.add_under("project:atlas", "work:first-mix", "First mix")
        .await;
    s.add_under("project:atlas", "work:loop-check-in", "Loop check-in")
        .await;
    s.add("person:lisa", "Lisa").await;
    s.add("person:ned-flanders", "Ned").await;

    // The project's own columns, in its own order, around the shipped five.
    s.event_with(
        "project:atlas",
        "the board's columns",
        json!({"columns": "inbox, this release, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    s.event_with(
        "work:phi",
        "the next piece, ready to start",
        json!({"status": "next", "owner": "person:lisa"}),
        &[],
    )
    .await;
    s.event_with(
        "work:first-mix",
        "under way and committed",
        json!({
            "status": "now", "owner": "person:lisa",
            "commit": "0a1b2c3", "verified_by": "the whole suite",
        }),
        &[],
    )
    .await;
    s.event_with(
        "work:sigma",
        "cannot start until sign-off, and needs two others first",
        json!({
            "status": "waiting", "waiting_on": "person:ned-flanders",
            "depends_on": "work:phi, work:first-mix",
        }),
        &[],
    )
    .await;
    s.event_with(
        "work:loop-check-in",
        "not looked at yet",
        json!({"status": "inbox"}),
        &[],
    )
    .await;
    s.event_with(
        "project:atlas",
        "the project those belong to",
        json!({"status": "now"}),
        &[],
    )
    .await;

    // ── a status that is free text is refused, and the refusal says what the
    // key holds ──────────────────────────────────────────────────────────────
    let refused = s
        .refused(
            "capture",
            json!({
                "subject": "work:phi", "content": "where it stands",
                "provenance": "testimony",
                "fields": {"status": "drafted, not asked"},
            }),
        )
        .await;
    for word in ["someday", "next", "now", "waiting", "done"] {
        refused.says(word);
    }
    // The list is the project's columns, and writing columns extends it.
    refused.says("columns");

    // **A word outside the project's columns is refused and the list is
    // named**, while a work item under no project is held to the five: the
    // project's own column is no column there.
    let outside = s
        .refused(
            "capture",
            json!({
                "subject": "work:first-mix", "content": "where it stands",
                "provenance": "testimony",
                "fields": {"status": "backlog"},
            }),
        )
        .await;
    outside.says("inbox");
    outside.says("this release");
    outside.says("columns");
    // The refusal names the five and not the project's list, which the call
    // could not echo: the caller sent `inbox`, so that word proves nothing.
    s.refused(
        "capture",
        json!({
            "subject": "work:sigma", "content": "where it stands",
            "provenance": "testimony",
            "fields": {"status": "inbox"},
        }),
    )
    .await
    .says("someday")
    .never_says("this release");

    // ── a cold session, told nothing, asks ──────────────────────────────────
    let cold = story.as_bot("bot:assistant").await;

    let next = cold
        .shape(
            "what is next",
            json!({"kind": "work", "fields": [{"key": "status", "value": "next"}]}),
        )
        .await;
    next.says("work:phi");
    next.never_says("work:sigma");
    next.never_says("work:loop-check-in");

    let inbox = cold
        .shape(
            "what is in the inbox",
            json!({"kind": "work", "fields": [{"key": "status", "value": "inbox"}]}),
        )
        .await;
    inbox.says("work:loop-check-in");
    inbox.never_says("work:phi");

    let waiting = cold
        .shape(
            "what is waiting, and on whom",
            json!({"kind": "work", "fields": [{"key": "waiting_on"}]}),
        )
        .await;
    waiting.says("work:sigma");
    waiting.says("person:ned-flanders");
    // Only sigma holds the key: its dependency list names phi, so the handle
    // is in the answer, but phi is not an object in it.
    waiting.number("/count", 1);

    let behind_phi = cold
        .shape(
            "what stands on phi",
            json!({
                "subject": "work:phi",
                "follow": {"relation": "depends_on", "direction": "in"},
            }),
        )
        .await;
    behind_phi.says("work:sigma");
    // **The second item of the list finds the record too.**
    let behind_mix = cold
        .shape(
            "what stands on the first mix",
            json!({
                "subject": "work:first-mix",
                "follow": {"relation": "depends_on", "direction": "in"},
            }),
        )
        .await;
    behind_mix.says("work:sigma");
    behind_mix.never_says("work:loop-check-in");

    let projects = cold
        .shape(
            "which projects are under way",
            json!({"kind": "project", "fields": [{"key": "status", "value": "now"}]}),
        )
        .await;
    projects.says("project:atlas");

    s.wrap("work laid down on the shipped keys").await;
    story.finish().await;
}

/// "What is behind this?" — the question asked of a thing, with no key named.
///
/// `depends_on` lists what stands behind a piece of work, and a list that names
/// two things names both. A reader who walks IN from either one, or searches
/// for what points at it, finds the work, the same as the relation walk does by
/// the key's own name (decision log 374: a link is any field that names another
/// thing). A list with one item that is not a handle is prose, and the work
/// holding it is found from neither.
#[tokio::test]
async fn a_work_item_depending_on_two_things_is_found_from_each_of_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("work:phi", "Phi").await;
    s.add("work:sigma", "Sigma").await;
    s.add("work:first-mix", "First mix").await;
    s.add("work:loop-check-in", "Loop check-in").await;
    s.event_with(
        "work:first-mix",
        "waits on two things",
        json!({"depends_on": "work:phi, work:sigma"}),
        &[],
    )
    .await;
    s.event_with(
        "work:loop-check-in",
        "mentions one thing and something undecided",
        // An undeclared key: `depends_on` is declared to hold references, and
        // a write putting a non-handle in it is refused.
        json!({"see_also": "work:phi, not yet decided"}),
        &[],
    )
    .await;

    // ── an unscoped walk in, from each of the two ───────────────────────────
    for behind in ["work:phi", "work:sigma"] {
        s.call(
            "recall",
            json!({"subject": behind, "follow": {"direction": "in"}}),
        )
        .await
        .says("work:first-mix");
    }
    // …and the list with an item that is not a handle reaches nobody, which
    // the first walk above would not show on its own: it asked about the one
    // thing both lists name.
    s.call(
        "recall",
        json!({"subject": "work:phi", "follow": {"direction": "in"}}),
    )
    .await
    .never_says("work:loop-check-in");

    // ── the search edge, by the other end of the list ───────────────────────
    s.call(
        "search",
        json!({"edge": {"object": "work:sigma", "shape": "connection"}}),
    )
    .await
    .says("work:first-mix");

    s.wrap("looked behind two things and found the work that waits on both")
        .await;
    story.finish().await;
}
