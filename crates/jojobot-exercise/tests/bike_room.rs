//! **The bike room, held three ways, for free.**
//!
//! The room is one goal, one line of entry and four intermediates asserted on
//! what the occupant left in the store. Nothing here drives a model: each check
//! is run against a real room whose state is put there by hand, through the
//! verbs.
//!
//! **Three rooms, because two are not enough.** The room a run that never found
//! the brief leaves must fail every check. The room a run that worked the goal
//! leaves must hold. And a room reached by a DIFFERENT route through the same
//! surface must hold too — an intermediate that only passes for the sequence
//! its author imagined is measuring the route rather than the goal, and nothing
//! but a second route can tell those apart.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// The document a run is driven by, read from the root of the workspace.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::BIKE_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let room = Room::open(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");
    expectations::seed_for(expectations::BIKE_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("the shipped identity boots");
    let sid = booted["session"]["sid"]
        .as_str()
        .expect("a handle")
        .to_string();
    (room, surface, sid)
}

/// A call the occupant would make.
async fn as_the_occupant(room: &Surface, sid: &str, verb: &str, mut args: Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// Every check this room registers, run against it.
async fn judge_all(room: &Surface) -> Vec<Outcome> {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let checks = expectations::for_playbook(expectations::BIKE_ROOM).expect("the room asserts");
    let mut outcomes = Vec::new();
    for check in checks {
        outcomes.push(check.check(&seen).await);
    }
    // **Said here rather than in each case**: a for-loop over an empty list
    // holds every claim in this file, so a room that registered no checks would
    // read as a room where every check passed.
    assert!(
        !outcomes.is_empty(),
        "the room registered no checks, so a run would report a pass over an empty list",
    );
    outcomes
}

/// What the run's report would say, so a failure names the check that failed.
fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .map(|o| format!("\n  [{}] {} — {}", o.held, o.name, o.saying))
        .collect()
}

/// The id of the message the room was furnished with.
async fn brief_in_the_box(room: &Surface) -> String {
    let board = room
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&board).expect("the board is json");
    parsed["results"]
        .as_array()
        .expect("a result list")
        .iter()
        .find(|hit| hit["hit"] == "message")
        .and_then(|hit| hit["id"].as_str())
        .unwrap_or_else(|| panic!("the room was furnished with no brief: {board}"))
        .to_string()
}

/// **The room is one goal behind one line, and the line asks for no report.**
///
/// The shape is the whole slice. A phase that arrives as a numbered list is the
/// instrument the cold-session suite already is, and a phase that asks for a
/// verdict has told its occupant that somebody is reading its prose.
#[test]
fn the_room_is_one_phase_delivered_whole() {
    let room = room_document();
    assert_eq!(
        room.phases.len(),
        1,
        "the room is one goal: {:?}",
        room.phases.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );
    let entry = &room.phases[0];
    assert!(
        entry.fresh_session,
        "the occupant arrives with no memory of anything: {}",
        entry.name,
    );
    assert_eq!(
        entry.deliveries.len(),
        1,
        "the entry is one line, so there is nothing to hold back: {:?}",
        entry.deliveries,
    );
    assert_eq!(
        entry.prompt.lines().count(),
        1,
        "the entry runs to more than one line: {:?}",
        entry.prompt,
    );
    for asked in ["PASS", "FAIL", "PARTIAL", "Report"] {
        assert!(
            !entry.prompt.contains(asked),
            "the entry asks the occupant for a verdict, which tells it a test is running: {:?}",
            entry.prompt,
        );
    }
    // **The entry may name the identity and never the box.** Which bot to be is
    // not what this room measures; finding the mail is, and an entry that says
    // where the work is waiting trades the first lock for a green run.
    let said = entry.prompt.to_lowercase();
    for pointed in ["mail", "box", "message", "waiting"] {
        assert!(
            !said.contains(pointed),
            "the entry points the occupant at its mail, so the first lock opens itself: {:?}",
            entry.prompt,
        );
    }
}

/// **Expectations are registered for this room, and a room nobody wrote any
/// for still gets none.**
///
/// The negative is what makes the tier honest — a run with nothing to assert
/// refuses before it bills anything — and it passes on a build where the
/// registry answers nothing to everybody, so the positive rides with it.
#[test]
fn the_room_has_expectations_of_its_own() {
    let checks = expectations::for_playbook(&format!("a/b/{}", expectations::BIKE_ROOM))
        .expect("the room has expectations");
    assert!(
        (3..=4).contains(&checks.len()),
        "the room asserts on {} intermediate(s), and the shape is three or four",
        checks.len(),
    );
    for check in &checks {
        assert!(
            check.name().starts_with("Phase 1"),
            "a check keyed to no phase of this room leaves the phase reading as uncovered: {}",
            check.name(),
        );
    }
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
    );
}

/// **The entry names no verb**, held against the verbs the room actually
/// serves rather than against a list written here — a list of names in this
/// file goes stale the day a verb is renamed, and reads green while the entry
/// coaches.
#[tokio::test]
async fn the_entry_names_no_verb_the_room_serves() {
    let (_room, surface, _sid) = furnished().await;
    let verbs = surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(
        verbs.len() > 5,
        "the room served {} verbs, so finding none in the entry proves nothing",
        verbs.len(),
    );
    let entry = &room_document().phases[0].prompt;
    for verb in &verbs {
        let name = verb["name"].as_str().expect("a verb has a name");
        assert!(
            !entry.contains(name),
            "the entry names the verb {name}, so the occupant did not have to find it: {entry:?}",
        );
    }
}

/// **A room the occupant never got anywhere in fails every check.**
///
/// The one that matters most. Three of the four are claims about something
/// being present, and a room that has been furnished and left alone satisfies
/// none of them — but only if each check reads what it says it reads. A check
/// that cannot fail reports a pass over a run that did nothing, and nothing in
/// a transcript would say so.
#[tokio::test]
async fn every_check_fails_on_a_room_nobody_worked_in() {
    let (_room, surface, _sid) = furnished().await;
    let outcomes = judge_all(&surface).await;
    assert!(
        !outcomes.is_empty(),
        "the room registered no checks, so a run would report a pass over an empty list",
    );
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished room nobody touched held a check: {}",
            saying(&outcomes),
        );
    }
}

/// **The room a session that worked the goal leaves.**
///
/// Every write here is one the occupant makes through the surface: it takes
/// delivery of the brief, puts the service on the bike as values, writes the
/// year's number under the key the earlier years are under, and leaves a
/// message for whoever comes next.
#[tokio::test]
async fn every_check_holds_once_the_goal_is_worked() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "annual service",
            "provenance": "testimony",
            "fields": {"done_on": "2026-08-11", "work": "chain, cables"},
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "the year's tally for 2026",
            "provenance": "testimony",
            "fields": {"km": "4100", "year": "2026"},
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "the bikes, brought up to date",
            "body": "The service and this year's distance are on the gravel bike. The road bike \
                     is out of warranty and has not moved.",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a room where the goal was worked failed a check: {}",
            saying(&outcomes),
        );
    }
}

/// **The same goal, reached a way nobody predicted, and every check still
/// holds.**
///
/// Four different paths through the same surface: the brief is retired without
/// the box ever being drained, the service day rides a key of the occupant's
/// own naming, this year's distance arrives as an edit of the record beside it
/// rather than as a record of its own, and the handoff is left on the run
/// instead of in a box. An intermediate that fails here is asserting the route.
#[tokio::test]
async fn every_check_holds_for_a_route_nobody_predicted() {
    let (_room, surface, sid) = furnished().await;

    // Retired straight from `new`, which takes delivery of nothing else.
    let brief = brief_in_the_box(&surface).await;
    as_the_occupant(
        &surface,
        &sid,
        "mark_processed",
        json!({"message_id": brief, "notes": "worked it"}),
    )
    .await;

    // A key of the occupant's own, on a record whose wording is nobody's
    // convention.
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "back from the shop",
            "provenance": "testimony",
            "fields": {"serviced_on": "2026-08-11"},
        }),
    )
    .await;

    // This year's number as an edit of the newest tally rather than a record of
    // its own: a key's writes accumulate either way, so the bike reads 4100 and
    // its history carries three writes.
    let newest = newest_tally(&surface).await;
    as_the_occupant(
        &surface,
        &sid,
        "update_fact",
        json!({"address": newest, "fields": {"km": "4100"}}),
    )
    .await;

    // The handoff on the run itself, which is the other rail the product
    // offers for leaving something behind.
    as_the_occupant(
        &surface,
        &sid,
        "journal",
        json!({
            "entry": "Put the service and the year's distance on the gravel bike.",
            "focus": "the bikes — warranty answered, nothing else outstanding",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a room the goal was reached in another way failed a check: {}",
            saying(&outcomes),
        );
    }
}

/// The address of the newest record carrying the bike's distance, read off the
/// room rather than assumed — the furniture writes it and nothing here knows
/// what address it got.
async fn newest_tally(room: &Surface) -> String {
    let read = room
        .call(
            "recall",
            json!({"subject": "thing:gravel-bike", "history": "km"}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"][0]["history"]["writes"]
        .as_array()
        .expect("the writes behind the key")
        .last()
        .and_then(|write| write["record"].as_str())
        .unwrap_or_else(|| panic!("no tally was furnished: {read}"))
        .to_string()
}

/// **The room a session that did the work in PROSE leaves**, which is the
/// failure these locks exist to catch.
///
/// Everything the operator asked for is written down, and none of it is
/// reachable: the service day is inside a sentence, and the year's distance
/// starts a key of its own beside the two the bike already had. A session like
/// this looks diligent in a transcript and leaves a bike that cannot answer
/// either question.
///
/// **Paired with the two locks that DO hold here**, because a room where every
/// check failed would prove only that the writes did not land at all.
#[tokio::test]
async fn the_locks_that_measure_reachability_fail_on_a_room_written_in_prose() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "serviced on 2026-08-11 — chain and cables, bearings left alone",
            "provenance": "testimony",
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "this year's riding",
            "provenance": "testimony",
            "fields": {"distance_this_year": "4100"},
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "the bikes, brought up to date",
            "body": "Wrote down the service and the year's riding.",
        }),
    )
    .await;

    // **Named by position rather than by wording**: the room registers its
    // locks in the order the document numbers them, so this says which two
    // failed instead of counting how many did — a count of two passes when the
    // wrong two fail.
    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        vec![true, false, false, true],
        "the box was opened and a handoff was left, and neither the service day nor the year's \
         distance can be reached: {}",
        saying(&outcomes),
    );
}
