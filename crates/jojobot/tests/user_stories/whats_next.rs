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

    // ── a status that is free text lands, and the answer says what the key
    // holds ──────────────────────────────────────────────────────────────
    let refused = s
        .call(
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

    // **A word outside the project's columns lands and the list is named**, while a work item under no project is held to the five: the
    // project's own column is no column there.
    let outside = s
        .call(
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
    // The answer names the five and not the project's list, which the call
    // could not echo: the caller sent `inbox`, so that word proves nothing.
    s.call(
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

    // The three words above landed, so the three items are put back where they
    // stood before the cold session asks what is next.
    for (subject, status) in [
        ("work:phi", "next"),
        ("work:first-mix", "now"),
        ("work:sigma", "waiting"),
    ] {
        s.event_with(subject, "put back", json!({"status": status}), &[])
            .await;
    }

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

/// "The note told me to write columns on the project, I did, and it named the
/// word again." A project filed under no project has nowhere else to write its
/// columns, so its own list governs its own status, and the route the note names
/// works. One filed under a project keeps that project's list, and a list of its
/// own does not widen it.
#[tokio::test]
async fn a_project_under_no_project_is_held_to_its_own_columns_and_one_under_a_project_to_its_parents()
 {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("project:visa", "Visa").await;
    s.add("project:the-shed", "The Shed").await;
    s.add("project:kitchen-floor", "Kitchen Floor").await;
    s.add_under(
        "project:kitchen-floor",
        "project:jojobot-server",
        "Jojobot server",
    )
    .await;

    // ── under no project: the note is followed to the end ───────────────────
    let status = |subject: &str, word: &str| {
        json!({
            "subject": subject, "content": "where it stands",
            "provenance": "testimony", "fields": {"status": word},
        })
    };
    let write = status("project:visa", "considering");
    // ① The word lands. The answer names the project whose columns govern,
    // which is this one, and the shipped words it always keeps.
    // **Read in the note itself.** The receipt repeats the subject the call
    // named, so a needle matched anywhere in the answer would be satisfied by
    // the call's own input; only a note that names the board does it here.
    let refused = s.call("capture", write.clone()).await;
    refused.holds_at("/status_word_note", "project:visa");
    for word in ["someday", "next", "now", "waiting", "done"] {
        refused.holds_at("/status_word_note", word);
    }
    // ② The columns are written where the note says.
    s.event_with(
        "project:visa",
        "the board's columns",
        json!({"columns": "considering, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    // ③ The same write now lands with nothing more to say, and ④ the status
    // reads back.
    let quiet = s.call("capture", write).await.json();
    assert!(quiet.get("status_word_note").is_none(), "{quiet}");
    s.recall("project:visa")
        .await
        .says("\"status\":\"considering\"");
    // The list that governs is the one the note now names, in full.
    let again = s.call("capture", status("project:visa", "pondering")).await;
    again.holds_at("/status_word_note", "considering");
    again.holds_at("/status_word_note", "project:visa");

    // The list and the word it adds may arrive in one write.
    s.event_with(
        "project:the-shed",
        "the board and where it stands",
        json!({
            "columns": "drafting, someday, next, now, waiting, done",
            "status": "drafting",
        }),
        &[],
    )
    .await;
    s.recall("project:the-shed")
        .await
        .says("\"status\":\"drafting\"");

    // ── under a project: the parent's list governs, and the child's own does
    // not widen it ───────────────────────────────────────────────────────────
    s.event_with(
        "project:kitchen-floor",
        "the board's columns",
        json!({"columns": "inbox, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    s.event_with(
        "project:jojobot-server",
        "the child's own columns",
        json!({"columns": "backlog, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    // The note names the parent, whose columns govern, and its words.
    let child = s
        .call("capture", status("project:jojobot-server", "backlog"))
        .await;
    child.holds_at("/status_word_note", "project:kitchen-floor");
    child.holds_at("/status_word_note", "inbox");
    child.never_holds_at("/status_word_note", "project:jojobot-server");
    s.event_with(
        "project:jojobot-server",
        "where it stands",
        json!({"status": "inbox"}),
        &[],
    )
    .await;
    s.recall("project:jojobot-server")
        .await
        .says("\"status\":\"inbox\"");

    story.finish().await;
}

/// "The orientation said no write is ever blocked by a type, and the project
/// told me my status was outside its words." The essay a cold session reads says
/// which key a kind holds to a named set and where that set is extended, and the
/// session that follows the note the essay prepared it for ends with a write
/// that no longer needs one.
#[tokio::test]
async fn a_cold_session_taught_by_the_essay_follows_a_status_note_to_a_write_that_needs_none() {
    let story = Story::begin("bot:otto").await;

    // ── the essay, read by a session that was told nothing ──────────────────
    let (anon, _) = story.call("start_here", json!({})).await;
    let essay = anon.json()["orientation"]
        .as_str()
        .expect("the boot carries the essay")
        .to_string();
    let taught = essay
        .lines()
        .find(|line| line.contains("Declare a type"))
        .expect("the essay teaches declaring a type");
    for token in ["`status`", "`columns`", "someday", "KIND"] {
        assert!(
            taught.contains(token),
            "the paragraph that says what a type refuses does not name {token}: {taught}"
        );
    }

    // ── the session writes a status the project does not list ───────────────
    let s = story.session().await;
    s.add("project:atlas", "Atlas").await;
    let write = json!({
        "subject": "project:atlas", "content": "where it stands",
        "provenance": "testimony", "fields": {"status": "considering"},
    });
    let landed = s.call("capture", write.clone()).await;
    // The answer names the project that holds the list, as the essay said it would.
    landed.holds_at("/status_word_note", "project:atlas");
    landed.holds_at("/status_word_note", "columns");
    s.recall("project:atlas")
        .await
        .says("\"status\":\"considering\"");

    // ── it follows the note: columns on that project, then the same write ───
    s.event_with(
        "project:atlas",
        "the board's columns",
        json!({"columns": "considering, someday, next, now, waiting, done"}),
        &[],
    )
    .await;
    let quiet = s.call("capture", write).await.json();
    assert!(quiet.get("status_word_note").is_none(), "{quiet}");

    story.finish().await;
}
