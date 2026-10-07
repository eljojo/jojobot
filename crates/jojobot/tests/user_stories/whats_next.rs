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
    s.refused(
        "capture",
        json!({
            "subject": "work:sigma", "content": "where it stands",
            "provenance": "testimony",
            "fields": {"status": "inbox"},
        }),
    )
    .await;

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
