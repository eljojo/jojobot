//! "What can I start next on the shed, and why can't I start the rest?"
//!
//! Four pieces of work on one shed, two of them waiting on something. The
//! session asks one named question and is told which can be started now, how
//! many it left out, and the call that says why. It makes that call for one piece
//! of work and is told what stops it. The read works because the question is
//! shipped: nobody declared a view first.
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

/// A piece of work as `add_entity` takes it.
fn work(slug: &str, name: &str, sets: serde_json::Value) -> serde_json::Value {
    json!({"kind": "work", "handle": slug, "name": name,
           "source": "user-named", "sets": sets})
}

/// A piece of work filed under a project.
fn work_under(slug: &str, name: &str, parent: &str, sets: serde_json::Value) -> serde_json::Value {
    json!({"kind": "work", "handle": slug, "name": name, "parent": parent,
           "source": "user-named", "sets": sets})
}

#[tokio::test]
async fn what_can_be_started_and_what_stops_the_rest() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;

    s.call(
        "add_entity",
        work("pour-the-slab", "Pour the slab", json!({"status": "done"})),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "order-the-lumber",
            "Order the lumber",
            json!({"status": "now"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "frame-the-walls",
            "Frame the walls",
            json!({"status": "next", "depends_on": "work:pour-the-slab"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "build-the-roof",
            "Build the roof",
            json!({"status": "next",
                   "depends_on": "work:pour-the-slab, work:order-the-lumber"}),
        ),
    )
    .await;

    // ── the question, by name ───────────────────────────────────────────────
    let ready = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&ready),
        ["work:frame-the-walls", "work:order-the-lumber"],
        "{}",
        ready.raw()
    );
    // Two are left out: the finished one and the one that waits on work in hand.
    let left_out = &ready.json()["ready_left_out"];
    assert_eq!(left_out["count"], 2, "{}", ready.raw());

    // ── the call the count names, for one piece of work ─────────────────────
    let why = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:build-the-roof"}),
        )
        .await;
    let object = &why.json()["objects"][0];
    assert_eq!(object["ready"], false, "{}", why.raw());
    assert_eq!(
        object["blocked_by"],
        json!([{"blocker": "dependency", "handle": "work:order-the-lumber",
                "why": "unfinished"}]),
        "{}",
        why.raw()
    );

    // ── finishing what it waited on moves it onto the list ──────────────────
    s.call(
        "capture",
        json!({"subject": "work:order-the-lumber", "content": "done",
               "provenance": "testimony", "sets": {"status": "done"}}),
    )
    .await;
    let after = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&after),
        ["work:build-the-roof", "work:frame-the-walls"],
        "{}",
        after.raw()
    );

    story.finish().await;
}

/// "What does the whole plan for the shed look like, and what stops each piece?"
/// One call names the plan view and lists every piece of work not finished, with
/// whether it can start and, when it cannot, what stops it: the session does not
/// ask after the pieces one at a time.
#[tokio::test]
async fn the_whole_plan_and_what_stops_each_piece_come_in_one_call() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:the-shed", "The shed").await;
    let piece = |slug: &str, name: &str, sets: serde_json::Value| {
        work_under(slug, name, "project:the-shed", sets)
    };
    s.call(
        "add_entity",
        piece("pour-the-slab", "Pour the slab", json!({"status": "done"})),
    )
    .await;
    s.call(
        "add_entity",
        piece(
            "order-the-lumber",
            "Order the lumber",
            json!({"status": "now"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        piece(
            "frame-the-walls",
            "Frame the walls",
            json!({"status": "next", "depends_on": "work:pour-the-slab"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        piece(
            "build-the-roof",
            "Build the roof",
            json!({"status": "next",
                   "depends_on": "work:pour-the-slab, work:order-the-lumber"}),
        ),
    )
    .await;

    let plan = s
        .call(
            "recall",
            json!({"view": "plan", "parent": "project:the-shed"}),
        )
        .await;

    assert_eq!(
        listed(&plan),
        [
            "work:build-the-roof",
            "work:frame-the-walls",
            "work:order-the-lumber"
        ],
        "{}",
        plan.raw()
    );
    let objects = &plan.json()["objects"];
    assert_eq!(objects[0]["ready"], false, "{}", plan.raw());
    assert_eq!(
        objects[0]["blocked_by"],
        json!([{"blocker": "dependency", "handle": "work:order-the-lumber",
                "why": "unfinished"}]),
        "{}",
        plan.raw()
    );
    assert_eq!(objects[1]["ready"], true, "{}", plan.raw());
    assert_eq!(objects[2]["ready"], true, "{}", plan.raw());
    assert_eq!(
        plan.json()["ready_left_out"]["count"],
        1,
        "the finished piece is left out and counted: {}",
        plan.raw()
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
    s.call(
        "add_entity",
        work("pour-the-slab", "Pour the slab", json!({"status": "done"})),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "order-the-lumber",
            "Order the lumber",
            json!({"status": "now"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work(
            "frame-the-walls",
            "Frame the walls",
            json!({"status": "next", "depends_on": "work:pour-the-slab"}),
        ),
    )
    .await;

    // The positive: the ready answer carries the line, and the work in progress
    // is in the list it sits beside.
    let ready = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&ready),
        ["work:frame-the-walls", "work:order-the-lumber"],
        "{}",
        ready.raw()
    );
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
    assert_eq!(
        listed(&narrowed),
        ["work:frame-the-walls"],
        "{}",
        narrowed.raw()
    );

    // The negative: an ordinary read of the same work carries no such line.
    let plain = s.call("recall", json!({"kind": "work"})).await;
    assert!(
        plain.json().get("ready_note").is_none(),
        "an ordinary read names no ready line: {}",
        plain.raw()
    );
    story.finish().await;
}

/// "Can I start painting the door?" asked of a piece of work that was archived.
/// An archived piece of work is not ready, however it is named, and the answer
/// says why; the same piece of work before it was archived was ready.
#[tokio::test]
async fn an_archived_piece_of_work_is_not_ready_when_named_and_says_why() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.call(
        "add_entity",
        work("paint-the-door", "Paint the door", json!({"status": "now"})),
    )
    .await;

    // The positive the negative rests on: named by handle, it is ready.
    let before = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:paint-the-door"}),
        )
        .await;
    let object = &before.json()["objects"][0];
    assert_eq!(object["ready"], true, "{}", before.raw());

    s.call(
        "archive_entity",
        json!({"handle": "work:paint-the-door", "reason": "no longer wanted"}),
    )
    .await;
    let after = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:paint-the-door"}),
        )
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

/// "What can I start on the shed?" The ready view takes the project as `parent`
/// and lists that project's ready work only, and the inbound walk of `depends_on`
/// lists what a piece of work holds up. These are the two calls the projects
/// skill teaches beside the named read.
#[tokio::test]
async fn the_ready_view_narrows_to_one_project_and_a_walk_lists_what_it_unblocks() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("project:the-shed", "The shed").await;
    s.add("project:kitchen-floor", "The kitchen floor").await;
    s.call(
        "add_entity",
        work_under(
            "frame-the-walls",
            "Frame the walls",
            "project:the-shed",
            json!({"status": "next"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work_under(
            "build-the-roof",
            "Build the roof",
            "project:the-shed",
            json!({"status": "next", "depends_on": "work:frame-the-walls"}),
        ),
    )
    .await;
    s.call(
        "add_entity",
        work_under(
            "lay-the-tiles",
            "Lay the tiles",
            "project:kitchen-floor",
            json!({"status": "next"}),
        ),
    )
    .await;

    // The whole board lists both projects' ready work; the project narrows it.
    let board = s.call("recall", json!({"view": "ready"})).await;
    assert_eq!(
        listed(&board),
        ["work:frame-the-walls", "work:lay-the-tiles"],
        "{}",
        board.raw()
    );
    let one = s
        .call(
            "recall",
            json!({"view": "ready", "parent": "project:the-shed"}),
        )
        .await;
    assert_eq!(listed(&one), ["work:frame-the-walls"], "{}", one.raw());

    // What finishing the walls unblocks: the work that names it in depends_on.
    let unblocks = s
        .call(
            "recall",
            json!({"subject": "work:frame-the-walls",
                   "follow": {"relation": "depends_on", "direction": "in"}}),
        )
        .await;
    let connected = unblocks.json()["objects"][0]["connected"].to_string();
    assert!(
        connected.contains("work:build-the-roof") && !connected.contains("work:lay-the-tiles"),
        "{}",
        unblocks.raw()
    );
    story.finish().await;
}

/// "Is Kenny ready?" asked of the man who will frame the walls. Readiness is a
/// question about work, so the ready view turns a person away, whether the person
/// is named as the subject or the kind asked for is people, and says to name a
/// piece of work. The same view still answers for a piece of work, so the refusal
/// is about what was asked and not about the view. A record's address is no
/// subject for it either.
#[tokio::test]
async fn the_ready_view_turns_a_person_away_and_still_answers_for_work() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("person:kenny", "Kenny").await;
    s.call(
        "add_entity",
        work(
            "frame-the-walls",
            "Frame the walls",
            json!({"status": "now"}),
        ),
    )
    .await;

    // The positive the refusals rest on: a piece of work answers.
    let work = s
        .call(
            "recall",
            json!({"view": "ready", "subject": "work:frame-the-walls"}),
        )
        .await;
    assert_eq!(work.json()["objects"][0]["ready"], true, "{}", work.raw());

    // A person named as the subject is refused, and the refusal names who.
    let person = s
        .refused(
            "recall",
            json!({"view": "ready", "subject": "person:kenny"}),
        )
        .await;
    assert_eq!(person.json()["fix_by"], "change", "{}", person.raw());
    assert!(
        person.raw().contains("person:kenny"),
        "the refusal names the subject it turned away: {}",
        person.raw()
    );

    // A kind other than work is refused the same way, and the refusal names the
    // kind that answers. The call that differs only in that kind is the positive:
    // it lists the piece of work, so the refusal is about the kind and nothing
    // else in the call.
    let kind = s
        .refused("recall", json!({"view": "ready", "kind": "person"}))
        .await;
    assert_eq!(kind.json()["fix_by"], "change", "{}", kind.raw());
    kind.says("work");
    let of_work = s
        .call("recall", json!({"view": "ready", "kind": "work"}))
        .await;
    assert_eq!(
        listed(&of_work),
        ["work:frame-the-walls"],
        "{}",
        of_work.raw()
    );

    // A record's address is no subject for the view, as it is no subject for any
    // recall: it is refused as no handle, and the piece of work it belongs to
    // still answers by its own handle.
    let facts = s
        .call(
            "recall",
            json!({"subject": "work:frame-the-walls", "facts": true}),
        )
        .await;
    let address = facts.json()["objects"][0]["facts"][0]["address"]
        .as_str()
        .unwrap_or_else(|| panic!("the piece of work has a record: {}", facts.raw()))
        .to_string();
    let by_record = s
        .refused("recall", json!({"view": "ready", "subject": address}))
        .await;
    assert_eq!(by_record.json()["fix_by"], "change", "{}", by_record.raw());
    assert!(
        by_record.raw().contains(&address),
        "the refusal names the address it turned away: {}",
        by_record.raw()
    );
    story.finish().await;
}
