//! **The expectations, held both ways, for free.**
//!
//! An expectation is the part of this tier that decides whether a paid run
//! passed, and it is the part nobody would notice was broken: a check that can
//! never fail reports a pass over a run that did nothing, and a check that can
//! never hold turns a working surface into a bill and a red bar. Both are
//! invisible from a transcript.
//!
//! So each one is exercised against a real room whose state is put there by
//! hand, through the verbs — the room a phase WOULD leave, and the room a phase
//! that did nothing would leave. No model is driven and nothing is billed.
//!
//! What this cannot prove is that a real agent produces that state. That is the
//! paid run's job, and it is the only job the paid run has.

use jojobot_exercise::expectations;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// A room, and a handle to write into it as the shipped identity — which is
/// how a phase's own writes reach it.
async fn room() -> (Room, Surface, String) {
    let room = Room::open(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");
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

/// A write a phase would make, as that phase's agent would make it.
async fn as_the_agent(room: &Surface, sid: &str, verb: &str, mut args: Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// The one expectation named, run against this room with no phase boundaries
/// worth speaking of.
async fn judge(room: &Surface, named: &str) -> Outcome {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let all =
        expectations::for_playbook(&format!("some/path/{}", expectations::COLD_SESSION_SUITE))
            .expect("the suite has expectations");
    let one = all
        .into_iter()
        .find(|e| e.name().starts_with(named))
        .unwrap_or_else(|| panic!("no expectation named {named}"));
    one.check(&seen).await
}

/// **A playbook nobody has written expectations for gets none**, which is what
/// makes a run refuse rather than pass over an empty list. Paired with the
/// positive: the suite that does have them gets them.
#[test]
fn expectations_are_keyed_to_the_playbook_that_has_them() {
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
    );
    let suite = expectations::for_playbook(&format!("a/b/{}", expectations::COLD_SESSION_SUITE))
        .expect("the suite has expectations");
    assert!(
        !suite.is_empty(),
        "the suite came back with an empty list, which a run would report a pass over",
    );
}

/// **Phase 2, both ways.** The claim is an absence, and the room it is checked
/// against is nearly empty — so the case that matters is the one where the
/// absence is there for the wrong reason.
#[tokio::test]
async fn the_roster_check_holds_on_a_clean_room_and_fails_on_a_minted_one() {
    let (_room, surface, sid) = room().await;

    let clean = judge(&surface, "Phase 2").await;
    assert!(
        clean.held,
        "a room whose door minted nothing must hold: {}",
        clean.saying,
    );

    // The failure the phase provokes: a bot standing there under the name the
    // agent was told to try.
    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({
            "kind": "bot", "handle": "zzz-not-a-bot", "name": "Not A Bot",
            "source": "the room",
        }),
    )
    .await;
    let minted = judge(&surface, "Phase 2").await;
    assert!(
        !minted.held,
        "an identity was minted and the check still held: {}",
        minted.saying,
    );
}

/// **Phase 8, both ways, part by part.**
///
/// The room a phase that did nothing leaves must fail it, and each part of the
/// state a working phase leaves must be required — a check that held once the
/// person existed would pass a run that never wrote the claim, the bot or the
/// message.
#[tokio::test]
async fn the_writes_check_fails_on_an_empty_room_and_holds_once_every_part_is_there() {
    let (_room, surface, sid) = room().await;

    let nothing = judge(&surface, "Phase 8").await;
    assert!(
        !nothing.held,
        "a room where the phase wrote nothing must fail: {}",
        nothing.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({
            "kind": "person", "handle": "smoke-alpha", "name": "Smoke Alpha",
            "source": "the room",
        }),
    )
    .await;
    let only_the_person = judge(&surface, "Phase 8").await;
    assert!(
        !only_the_person.held,
        "the person alone must not satisfy it: {}",
        only_the_person.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "person:smoke-alpha",
            "content": "was mentioned once in passing",
            "provenance": "inference",
        }),
    )
    .await;
    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({
            "kind": "bot", "handle": "smoke-gamma", "name": "Smoke Gamma",
            "source": "the room",
        }),
    )
    .await;
    let without_mail = judge(&surface, "Phase 8").await;
    assert!(
        !without_mail.held,
        "the message is part of what the phase leaves: {}",
        without_mail.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "mailbox": "assistant",
            "subject": "a smoke message",
            "body": "Something for whoever comes next.",
        }),
    )
    .await;
    let whole = judge(&surface, "Phase 8").await;
    assert!(
        whole.held,
        "every part is in the room and the check did not hold: {}",
        whole.saying,
    );
}

/// **Phase 7, and the trap in it.**
///
/// The claim is that nothing moved, so the check compares the two sides of the
/// phase rather than reading the finished room. Three cases, because two of
/// them are ways of passing for the wrong reason: no boundary recorded at all,
/// and a board with no mail on it to leave alone.
#[tokio::test]
async fn the_no_change_check_needs_a_boundary_and_needs_mail_to_be_there() {
    let (_room, surface, sid) = room().await;
    let judge_across = async |boundaries: Vec<Boundary>| -> Outcome {
        let seen = Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let all =
            expectations::for_playbook(expectations::COLD_SESSION_SUITE).expect("expectations");
        let one = all
            .into_iter()
            .find(|e| e.name().starts_with("Phase 7"))
            .expect("the phase 7 expectation");
        one.check(&seen).await
    };

    // No boundary at all: reported, never swallowed.
    let unrecorded = judge_across(Vec::new()).await;
    assert!(
        !unrecorded.held,
        "a run with no boundary for the phase must not pass it: {}",
        unrecorded.saying,
    );

    let reading = |mail: &str, before: &str| Boundary {
        before: before.to_string(),
        mail: mail.to_string(),
        world: String::new(),
        board: String::new(),
        runs_offered: 0,
    };

    // A board with nothing on it: identical either side, and the claim was
    // never tested. This is the case an ordinary equality check would pass.
    //
    // **The reading is taken from the real room, before any message exists.**
    // A hand-written empty string is a shape the run never produces: a board
    // reading is the whole answer `search` returns, and that answer carries its
    // envelope whether or not a single message matched. A check guarded on the
    // string being empty is guarded on nothing.
    let bare = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let empty = judge_across(vec![
        reading(&bare, "Phase 7 — mail"),
        reading(&bare, "Phase 8 — writes"),
    ])
    .await;
    assert!(
        !empty.held,
        "an empty board satisfied 'nothing moved' without testing it: {}",
        empty.saying,
    );

    // Mail on the board, unmoved: the case that must hold. The reading is
    // taken from a real board so the check is comparing what it will really be
    // handed.
    as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "mailbox": "assistant",
            "subject": "left where it was",
            "body": "Nobody is to take delivery of this.",
        }),
    )
    .await;
    let board = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let unmoved = judge_across(vec![
        reading(&board, "Phase 7 — mail"),
        reading(&board, "Phase 8 — writes"),
    ])
    .await;
    assert!(
        unmoved.held,
        "a board nobody touched must hold: {}",
        unmoved.saying,
    );

    // And the failure it exists for: the same board with a message moved out
    // of `new`.
    let moved = judge_across(vec![
        reading(&board, "Phase 7 — mail"),
        reading(&board.replace("\"new\"", "\"read\""), "Phase 8 — writes"),
    ])
    .await;
    assert!(
        !moved.held,
        "a poll took delivery and the check held anyway: {}",
        moved.saying,
    );
}

/// **Phase 10 — the cold reader picked up what was left, both ways.**
///
/// The suite's own table calls this phase assertable and names what it leaves:
/// the run wrapped, and the message an earlier phase posted now processed with
/// a note. Both are visible through the surface a later session uses, so both
/// are checked here — and each is paired with the positive it rests on, since
/// "no message is unprocessed" is satisfied by a board that never had one.
#[tokio::test]
async fn the_cold_reader_check_needs_something_to_have_been_picked_up() {
    let (_room, surface, sid) = room().await;

    async fn judge_across(surface: &Surface, boundaries: Vec<Boundary>) -> Outcome {
        let seen = Observed {
            room: surface,
            boundaries: &boundaries,
        };
        let all =
            expectations::for_playbook(&format!("some/path/{}", expectations::COLD_SESSION_SUITE))
                .expect("the suite has expectations");
        let one = all
            .into_iter()
            .find(|e| e.name().starts_with("Phase 10"))
            .expect("the phase 10 expectation");
        one.check(&seen).await
    }

    let reading = |mail: &str, board: &str, before: &str| Boundary {
        before: before.to_string(),
        mail: mail.to_string(),
        world: String::new(),
        board: board.to_string(),
        runs_offered: 0,
    };

    // No boundary at all: reported, never swallowed.
    let unrecorded = judge_across(&surface, Vec::new()).await;
    assert!(
        !unrecorded.held,
        "a run with no boundary for the phase must not pass it: {}",
        unrecorded.saying,
    );

    // A board that never carried a message. "Everything is processed" is true
    // of it and says nothing, which is the shape this check must refuse.
    let bare = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let nothing_left = judge_across(
        &surface,
        vec![
            reading(&bare, "", "Phase 10 — the reader"),
            reading(&bare, "", "Phase 11 — the ending"),
        ],
    )
    .await;
    assert!(
        !nothing_left.held,
        "a board with no message satisfied 'it was picked up' without testing it: {}",
        nothing_left.saying,
    );

    // A message left for the reader, unhandled: the state before the phase.
    let posted = as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "for whoever comes next",
            "body": "The damper is still hand-cut.",
        }),
    )
    .await;
    let id = serde_json::from_str::<Value>(&posted)
        .ok()
        .and_then(|b| b["id"].as_str().map(str::to_string))
        .expect("the message id");
    let waiting = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;

    // The phase that did nothing with it: still waiting either side.
    let untouched = judge_across(
        &surface,
        vec![
            reading(&waiting, "", "Phase 10 — the reader"),
            reading(&waiting, "", "Phase 11 — the ending"),
        ],
    )
    .await;
    assert!(
        !untouched.held,
        "the message was never taken and the check held anyway: {}",
        untouched.saying,
    );

    // **Two different failures must not read as one.** Both cases above are
    // refusals, and a check whose branches collapse into one reason cannot tell
    // "there was nothing to pick up" from "there was, and nobody did" — which
    // is the difference the phase exists to measure, and the difference a
    // reader of the report acts on.
    // The three ways this phase fails are one condition in the check, so what
    // a case proves here is that the condition fires — the reason it gives is
    // for a person reading the report and is not what this asserts on.

    // And the phase doing its job: taken and retired with a note.
    as_the_agent(&surface, &sid, "read_message", json!({"message_id": id})).await;
    as_the_agent(
        &surface,
        &sid,
        "mark_processed",
        json!({"message_id": id, "notes": "read it and carried on"}),
    )
    .await;
    let handled = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    let picked_up = judge_across(
        &surface,
        vec![
            reading(&waiting, "", "Phase 10 — the reader"),
            reading(&handled, "", "Phase 11 — the ending"),
        ],
    )
    .await;
    assert!(
        picked_up.held,
        "a message taken and retired with a note must hold: {}",
        picked_up.saying,
    );
}

/// **The session phases, both ways, off boardings taken by hand.**
///
/// A session's record is served only on the boot path, so these checks read the
/// resume offer at a phase boundary. What they must not do is hold on a run
/// where nothing was journalled — which, since the offer is empty then, is the
/// shape that would pass a naive threshold check.
#[tokio::test]
async fn the_session_checks_need_a_run_that_actually_grew() {
    let (_room, surface, sid) = room().await;

    let reading = |board: &str, before: &str| Boundary {
        before: before.to_string(),
        mail: String::new(),
        world: String::new(),
        board: board.to_string(),
        runs_offered: 0,
    };
    let judge_across = async |boundaries: Vec<Boundary>, named: &str| -> Outcome {
        let seen = Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let all =
            expectations::for_playbook(expectations::COLD_SESSION_SUITE).expect("expectations");
        let one = all
            .into_iter()
            .find(|e| e.name().starts_with(named))
            .unwrap_or_else(|| panic!("no expectation named {named}"));
        one.check(&seen).await
    };

    // A real board with nothing journalled: no run is offered at all.
    let empty = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;

    // Two entries, written the way the phase writes them.
    as_the_agent(
        &surface,
        &sid,
        "journal",
        json!({"entry": "set out to do the thing"}),
    )
    .await;
    as_the_agent(
        &surface,
        &sid,
        "journal",
        json!({"entry": "found the thing", "focus": "the thing"}),
    )
    .await;
    let grown = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;

    let never_wrote = judge_across(
        vec![reading(&empty, "Phase 6"), reading(&empty, "Phase 7")],
        "Phase 6",
    )
    .await;
    assert!(
        !never_wrote.held,
        "a phase that journalled nothing must not hold: {}",
        never_wrote.saying,
    );

    let wrote = judge_across(
        vec![reading(&empty, "Phase 6"), reading(&grown, "Phase 7")],
        "Phase 6",
    )
    .await;
    assert!(
        wrote.held,
        "a run that grew across the phase must hold: {}",
        wrote.saying,
    );

    // Phase 9 leaves the run OPEN, so a board offering nothing back fails it
    // however much was written earlier.
    let closed = judge_across(
        vec![reading(&grown, "Phase 9"), reading(&empty, "Phase 10")],
        "Phase 9",
    )
    .await;
    assert!(
        !closed.held,
        "a run nobody left open must not hold: {}",
        closed.saying,
    );

    // Phase 11 is an absence, and it must refuse an empty ending rather than
    // read it as "the run is gone".
    let nothing_at_the_end = judge_across(
        vec![reading(&grown, "Phase 10"), reading(&empty, "the end")],
        "Phase 11",
    )
    .await;
    assert!(
        !nothing_at_the_end.held,
        "an empty board at the end satisfied the absence: {}",
        nothing_at_the_end.saying,
    );
}

/// **Phase 10 relates the two sides of its boundary, or it holds on the wrong
/// room.**
///
/// The claim is that the message an earlier phase left was the one picked up.
/// Two existence facts — something was waiting, something was retired with a
/// note — are both true of a room where the waiting message was never touched
/// and a different one was retired beside it, which is the shape below. Paired
/// with the positive in the test above: the room where the waiting message
/// really was taken still holds.
#[tokio::test]
async fn the_cold_reader_check_refuses_a_room_where_a_different_message_was_retired() {
    let (_room, surface, sid) = room().await;

    let board = async || {
        surface
            .call(
                "search",
                json!({"query": "*", "include_mail": true, "limit": 200}),
            )
            .await
    };

    // The message the phase is supposed to pick up, left where it was.
    as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "for whoever comes next",
            "body": "The kappa run is half done.",
        }),
    )
    .await;
    let waiting = board().await;

    // A second message, taken and retired with a note — while the first stays
    // exactly where it was.
    let other = as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "something else entirely",
            "body": "Nothing to do with the message above.",
        }),
    )
    .await;
    let other_id = serde_json::from_str::<Value>(&other)
        .ok()
        .and_then(|b| b["id"].as_str().map(str::to_string))
        .expect("the message id");
    as_the_agent(
        &surface,
        &sid,
        "read_message",
        json!({"message_id": other_id}),
    )
    .await;
    as_the_agent(
        &surface,
        &sid,
        "mark_processed",
        json!({"message_id": other_id, "notes": "read it and carried on"}),
    )
    .await;
    let afterwards = board().await;

    let boundaries = vec![
        Boundary {
            before: "Phase 10 — the reader".to_string(),
            mail: waiting,
            world: String::new(),
            board: String::new(),
            runs_offered: 0,
        },
        Boundary {
            before: "Phase 11 — the ending".to_string(),
            mail: afterwards,
            world: String::new(),
            board: String::new(),
            runs_offered: 0,
        },
    ];
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let all = expectations::for_playbook(expectations::COLD_SESSION_SUITE).expect("expectations");
    let one = all
        .into_iter()
        .find(|e| e.name().starts_with("Phase 10"))
        .expect("the phase 10 expectation");
    let outcome = one.check(&seen).await;
    assert!(
        !outcome.held,
        "the waiting message was never taken and the check held anyway: {}",
        outcome.saying,
    );
}

/// **Phase 7's claim has to be able to hold on the room a run really builds.**
///
/// The check rests on there having been mail to leave alone, and the playbook
/// posts nothing until phase 8 — so unless the seed puts a message in the room,
/// the boundary this phase is read across is a room with no mail at all, the
/// check reports that it tested nothing, and every paid run of the suite fails
/// on the harness rather than on the product.
///
/// So the room here is furnished the way the binary furnishes one, rather than
/// posted into by hand: a case that manufactures its own positive cannot see
/// this.
#[tokio::test]
async fn the_no_change_check_can_hold_on_the_room_the_suite_is_seeded_with() {
    let (_room, surface, _sid) = room().await;
    expectations::seed_for(expectations::COLD_SESSION_SUITE)
        .expect("a seed for the suite")
        .furnish(&surface)
        .await
        .expect("the room is furnished");

    let board = surface
        .call(
            "search",
            json!({"query": "*", "include_mail": true, "limit": 200}),
        )
        .await;
    assert!(
        board.contains("\"hit\":\"message\""),
        "the furnished room carries no mail, so this phase has nothing to leave alone: {board}",
    );

    let reading = |before: &str| Boundary {
        before: before.to_string(),
        mail: board.clone(),
        world: String::new(),
        board: String::new(),
        runs_offered: 0,
    };
    let boundaries = vec![reading("Phase 7 — mail"), reading("Phase 8 — writes")];
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let all = expectations::for_playbook(expectations::COLD_SESSION_SUITE).expect("expectations");
    let one = all
        .into_iter()
        .find(|e| e.name().starts_with("Phase 7"))
        .expect("the phase 7 expectation");
    let outcome = one.check(&seen).await;
    assert!(
        outcome.held,
        "a phase that left the seeded mail where it was did not hold: {}",
        outcome.saying,
    );
}
