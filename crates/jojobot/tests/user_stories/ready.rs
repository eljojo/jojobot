//! "What can I start next, and why can't I start the rest?"
//!
//! Four pieces of work, two of them waiting on something. The session asks one
//! named question and is told which can be started now, how many it left out,
//! and the call that says why. It makes that call for one piece of work and is
//! told what stops it. The read works because the question is shipped: nobody
//! declared a view first.
//!
//! **A dependency counts as finished by the same word the rest of the instance
//! uses**, and finishing the one it waited on is what moves the work onto the
//! list.

use serde_json::json;

use super::dsl::Story;

/// The handles an answer lists, in order.
fn listed(answer: &super::dsl::Answer) -> Vec<String> {
    answer.json()["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| o["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn what_can_be_started_and_what_stops_the_rest() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    let work = |handle: &'static str, sets: serde_json::Value| {
        let slug = handle.strip_prefix("work:").expect("a work handle");
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": sets})
    };
    s.call("add_entity", work("work:sigma", json!({"status": "done"})))
        .await;
    s.call("add_entity", work("work:tau", json!({"status": "now"})))
        .await;
    s.call(
        "add_entity",
        work(
            "work:phi",
            json!({"status": "next", "depends_on": "work:sigma"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "work:first-mix",
            json!({"status": "next", "depends_on": "work:sigma, work:tau"}),
        ),
    )
    .await;

    // ── the question, by name ───────────────────────────────────────────────
    let ready = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(listed(&ready), ["work:phi", "work:tau"], "{}", ready.raw());
    // Two are left out: the finished one and the one that waits on work in hand.
    let left_out = &ready.json()["ready_left_out"];
    assert_eq!(left_out["count"], 2, "{}", ready.raw());

    // ── the call the count names, for one piece of work ─────────────────────
    let why = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:first-mix"}),
        )
        .await;
    let object = &why.json()["objects"][0];
    assert_eq!(object["ready"], false, "{}", why.raw());
    assert_eq!(
        object["blocked_by"],
        json!([{"blocker": "dependency", "handle": "work:tau", "why": "unfinished"}]),
        "{}",
        why.raw()
    );

    // ── finishing what it waited on moves it onto the list ──────────────────
    s.call(
        "capture",
        json!({"subject": "work:tau", "content": "done", "provenance": "testimony",
               "sets": {"status": "done"}}),
    )
    .await;
    let after = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&after),
        ["work:first-mix", "work:phi"],
        "{}",
        after.raw()
    );

    story.finish().await;
}

/// "Ready includes what I have already started. How do I get only what I have
/// not?" The ready answer carries one line beside its list: ready means
/// unblocked and not finished, so work in progress is in it, and `where` narrows
/// the list. The line is on the ready view and nowhere else, and the narrowing it
/// names does narrow.
#[tokio::test]
async fn the_ready_answer_says_where_narrows_it_and_an_ordinary_read_does_not() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    let work = |slug: &str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": slug,
               "source": "user-named", "sets": sets})
    };
    s.call("add_entity", work("sigma", json!({"status": "done"})))
        .await;
    s.call("add_entity", work("tau", json!({"status": "now"})))
        .await;
    s.call(
        "add_entity",
        work("phi", json!({"status": "next", "depends_on": "work:sigma"})),
    )
    .await;

    // The positive: the ready answer carries the line, and the work in progress
    // is in the list it sits beside.
    let ready = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(listed(&ready), ["work:phi", "work:tau"], "{}", ready.raw());
    let note = ready.json()["ready_note"]
        .as_str()
        .unwrap_or_else(|| {
            panic!(
                "the ready answer carries a line about where: {}",
                ready.raw()
            )
        })
        .to_string();
    assert!(note.contains("where"), "{note}");
    assert!(!note.contains('\n'), "one line: {note}");

    // The narrowing the line shows is one that works, and it keeps the answer's
    // own line.
    assert!(note.contains("status = next"), "{note}");
    // It names the move and gives the word as an example, so a board with
    // stages of its own adapts it rather than copying it.
    assert!(note.contains("not-started"), "{note}");
    let narrowed = s
        .call("recall", json!({"view": "ready", "where": "status = next"}))
        .await;
    assert_eq!(listed(&narrowed), ["work:phi"], "{}", narrowed.raw());

    // The negative: an ordinary read of the same work carries no such line.
    let plain = s.call("recall", json!({"kind": "work"})).await;
    assert!(
        plain.json().get("ready_note").is_none(),
        "an ordinary read names no ready line: {}",
        plain.raw()
    );
    story.finish().await;
}

/// "Can I start that one?" asked of a piece of work that was archived. An
/// archived piece of work is not ready, however it is named, and the answer
/// says why; the same piece of work before it was archived was ready.
#[tokio::test]
async fn an_archived_piece_of_work_is_not_ready_when_named_and_says_why() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.call(
        "add_entity",
        json!({"kind": "work", "handle": "omega", "name": "omega",
               "source": "user-named", "sets": {"status": "now"}}),
    )
    .await;

    // The positive the negative rests on: named by handle, it is ready.
    let before = s
        .call("recall", json!({"view": "ready", "subject": "work:omega"}))
        .await;
    let object = &before.json()["objects"][0];
    assert_eq!(object["ready"], true, "{}", before.raw());

    s.call(
        "archive_entity",
        json!({"handle": "work:omega", "reason": "no longer wanted"}),
    )
    .await;
    let after = s
        .call("recall", json!({"view": "ready", "subject": "work:omega"}))
        .await;
    let object = &after.json()["objects"][0];
    assert_eq!(object["ready"], false, "{}", after.raw());
    assert_eq!(
        object["blocked_by"],
        json!([{"blocker": "archived"}]),
        "{}",
        after.raw()
    );
    story.finish().await;
}

/// "What can I start on this project?" The ready view takes the project as
/// `parent` and lists that project's ready work only, and the inbound walk of
/// `depends_on` lists what a piece of work holds up. These are the two calls the
/// projects skill teaches beside the named read.
#[tokio::test]
async fn the_ready_view_narrows_to_one_project_and_a_walk_lists_what_it_unblocks() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:atlas", "The Campaign").await;
    s.add("project:visa", "The App").await;
    let work = |slug: &'static str, parent: &'static str, sets: serde_json::Value| {
        json!({"kind": "work", "handle": slug, "name": slug, "parent": parent,
               "source": "user-named", "sets": sets})
    };
    s.call(
        "add_entity",
        work("phi", "project:atlas", json!({"status": "next"})),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "sigma",
            "project:atlas",
            json!({"status": "next", "depends_on": "work:phi"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work("tau", "project:visa", json!({"status": "next"})),
    )
    .await;

    // The whole board lists both projects' ready work; the project narrows it.
    let board = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(listed(&board), ["work:phi", "work:tau"], "{}", board.raw());
    let one = s
        .call(
            "recall",
            json!({"view": "ready", "parent": "project:atlas"}),
        )
        .await;
    assert_eq!(listed(&one), ["work:phi"], "{}", one.raw());

    // What finishing phi unblocks: the work that names it in depends_on.
    let unblocks = s
        .call(
            "recall",
            json!({"subject": "work:phi",
                   "follow": {"relation": "depends_on", "direction": "in"}}),
        )
        .await;
    let connected = unblocks.json()["objects"][0]["connected"].to_string();
    assert!(
        connected.contains("work:sigma") && !connected.contains("work:tau"),
        "{}",
        unblocks.raw()
    );
    story.finish().await;
}
