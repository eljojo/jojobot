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
            "to": "assistant",
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
            "to": "assistant",
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

/// **Phase 13 — the cold reader picked up what was left, both ways.**
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
            .find(|e| e.name().starts_with("Phase 13"))
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
            reading(&bare, "", "Phase 13 — the reader"),
            reading(&bare, "", "Phase 14 — the ending"),
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
            reading(&waiting, "", "Phase 13 — the reader"),
            reading(&waiting, "", "Phase 14 — the ending"),
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
            reading(&waiting, "", "Phase 13 — the reader"),
            reading(&handled, "", "Phase 14 — the ending"),
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

    // Phase 12 leaves the run OPEN, so a board offering nothing back fails it
    // however much was written earlier.
    let closed = judge_across(
        vec![reading(&grown, "Phase 12"), reading(&empty, "Phase 13")],
        "Phase 12",
    )
    .await;
    assert!(
        !closed.held,
        "a run nobody left open must not hold: {}",
        closed.saying,
    );

    // Phase 14 is an absence, and it must refuse an empty ending rather than
    // read it as "the run is gone".
    let nothing_at_the_end = judge_across(
        vec![reading(&grown, "Phase 13"), reading(&empty, "the end")],
        "Phase 14",
    )
    .await;
    assert!(
        !nothing_at_the_end.held,
        "an empty board at the end satisfied the absence: {}",
        nothing_at_the_end.saying,
    );
}

/// **Phase 13 relates the two sides of its boundary, or it holds on the wrong
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
            before: "Phase 13 — the reader".to_string(),
            mail: waiting,
            world: String::new(),
            board: String::new(),
            runs_offered: 0,
        },
        Boundary {
            before: "Phase 14 — the ending".to_string(),
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
        .find(|e| e.name().starts_with("Phase 13"))
        .expect("the phase 10 expectation");
    let outcome = one.check(&seen).await;
    assert!(
        !outcome.held,
        "the waiting message was never taken and the check held anyway: {}",
        outcome.saying,
    );
}

/// **Phase 13 names the message it means, or the furniture answers for it.**
///
/// When the cold session arrives, the room a run really builds has TWO
/// unfinished messages in that box: the one the room was furnished with, and
/// the handoff phase 8 left. A session that retires the furniture and never
/// touches the handoff moves the unfinished count down by one, so a check
/// reading counts holds over exactly the run the phase exists to catch.
///
/// The room is furnished the way the binary furnishes one, because a case that
/// wrote its own second message could not see this. Both ways in one case: the
/// same room with the handoff retired as well must hold, or a check that
/// refused everything would pass the half above on its own.
#[tokio::test]
async fn the_cold_reader_check_is_not_satisfied_by_retiring_the_furniture() {
    let (_room, surface, sid) = room().await;
    expectations::seed_for(expectations::COLD_SESSION_SUITE)
        .expect("a seed for the suite")
        .furnish(&surface)
        .await
        .expect("the room is furnished");

    let board = async || {
        surface
            .call(
                "search",
                json!({"query": "*", "include_mail": true, "limit": 200}),
            )
            .await
    };
    let retire = async |id: &str, notes: &str| {
        as_the_agent(&surface, &sid, "read_message", json!({"message_id": id})).await;
        as_the_agent(
            &surface,
            &sid,
            "mark_processed",
            json!({"message_id": id, "notes": notes}),
        )
        .await;
    };

    // The handoff phase 8 leaves, beside the furniture.
    let posted = as_the_agent(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "assistant",
            "subject": "what I worked out about smoke-alpha",
            "body": "Written for whoever comes next.",
        }),
    )
    .await;
    let handoff = serde_json::from_str::<Value>(&posted)
        .ok()
        .and_then(|b| b["id"].as_str().map(str::to_string))
        .expect("the message id");
    let waiting = board().await;

    // The positive the whole case rests on: the furnished room really does put
    // a second message in that box, so the confusion below is available.
    let furniture = message_ids(&waiting)
        .into_iter()
        .find(|id| id != &handoff)
        .expect("the furnished room left no message of its own for the handoff to hide behind");

    // The session that read the box, retired the furniture with a note, and
    // left the handoff exactly where it was.
    retire(&furniture, "noted and filed").await;
    let wrong_one = board().await;

    let boundaries = |after: &str| {
        vec![
            Boundary {
                before: "Phase 13 — the reader".to_string(),
                mail: waiting.clone(),
                world: String::new(),
                board: String::new(),
                runs_offered: 0,
            },
            Boundary {
                before: "Phase 14 — the ending".to_string(),
                mail: after.to_string(),
                world: String::new(),
                board: String::new(),
                runs_offered: 0,
            },
        ]
    };
    let judge_across = async |after: &str| -> Outcome {
        let boundaries = boundaries(after);
        let seen = Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let all =
            expectations::for_playbook(expectations::COLD_SESSION_SUITE).expect("expectations");
        let one = all
            .into_iter()
            .find(|e| e.name().starts_with("Phase 13"))
            .expect("the phase 10 expectation");
        one.check(&seen).await
    };

    let missed_it = judge_across(&wrong_one).await;
    assert!(
        !missed_it.held,
        "the handoff was never touched and the check held on the furniture instead: {}",
        missed_it.saying,
    );

    // And the run the phase is written for: the handoff itself, taken and
    // retired with a note.
    retire(&handoff, "picked it up and carried on").await;
    let picked_up = judge_across(&board().await).await;
    assert!(
        picked_up.held,
        "the handoff was taken and retired with a note and the check did not hold: {}",
        picked_up.saying,
    );
}

/// The id of every message hit on a board reading.
fn message_ids(reading: &str) -> Vec<String> {
    serde_json::from_str::<Value>(reading)
        .ok()
        .and_then(|body| {
            body["results"].as_array().map(|hits| {
                hits.iter()
                    .filter(|hit| hit["hit"] == "message")
                    .filter_map(|hit| hit["id"].as_str().map(str::to_string))
                    .collect()
            })
        })
        .unwrap_or_default()
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

/// **Phase 9, and the trap is the loop with NO cadence.**
///
/// Three rooms: nothing written, only the scheduled loop, and both. The middle
/// one is the point — a build that demanded every key its kind names would
/// leave exactly that room, and a check reading only the scheduled loop would
/// call it a pass.
#[tokio::test]
async fn the_loop_check_needs_the_loop_that_has_no_cadence() {
    let (_room, surface, sid) = room().await;

    let nothing = judge(&surface, "Phase 9").await;
    assert!(
        !nothing.held,
        "a room where the phase wrote nothing must fail: {}",
        nothing.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "thing", "handle": "smoke-kettle", "name": "The Kettle",
               "source": "user-named"}),
    )
    .await;
    for loop_handle in ["smoke-descale", "smoke-filter"] {
        as_the_agent(
            &surface,
            &sid,
            "add_entity",
            json!({"kind": "rhythm", "handle": loop_handle, "name": loop_handle,
                   "parent": "thing:smoke-kettle", "source": "user-named"}),
        )
        .await;
    }
    // Only the scheduled one, with its check-in.
    as_the_agent(
        &surface,
        &sid,
        "capture",
        json!({"subject": "rhythm:smoke-filter", "content": "the loop, set up",
               "provenance": "testimony",
               "fields": {"name": "Filter", "last_check_in": "2026-07-01",
                          "counts_from": "2026-07-01", "advances_from": "due_date",
                          "cadence_days": "90", "outcome": "ran"}}),
    )
    .await;
    let only_scheduled = judge(&surface, "Phase 9").await;
    assert!(
        !only_scheduled.held,
        "the loop nobody set a frequency for is part of what the phase leaves: {}",
        only_scheduled.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "capture",
        json!({"subject": "rhythm:smoke-descale", "content": "looked at it",
               "provenance": "testimony",
               "fields": {"name": "Descale", "last_check_in": "2026-07-01"}}),
    )
    .await;
    let both = judge(&surface, "Phase 9").await;
    assert!(
        both.held,
        "with both loops there the check holds: {}",
        both.saying,
    );
}

/// **Phase 11 counts DISTINCT days, so one day is not two.**
///
/// The room a build with a zone of its own would leave — two claims, both
/// stamped the same — has to fail, or the check passes on the defect it exists
/// to find. Paired with the room where the two days differ.
#[tokio::test]
async fn the_frame_check_fails_when_both_claims_landed_on_one_day() {
    let (_room, surface, sid) = room().await;
    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "person", "handle": "smoke-alpha", "name": "Alpha",
               "source": "user-named"}),
    )
    .await;

    let claim = async |day: &str, what: &str| {
        as_the_agent(
            &surface,
            &sid,
            "capture",
            json!({"subject": "person:smoke-alpha", "content": what, "date": day}),
        )
        .await
    };
    claim("2026-08-19", "the first one").await;
    let one_day = judge(&surface, "Phase 11").await;
    assert!(
        !one_day.held,
        "one claim is one day, and one day is not the difference the phase measures: {}",
        one_day.saying,
    );

    claim("2026-08-19", "another, the same day").await;
    let same_day = judge(&surface, "Phase 11").await;
    assert!(
        !same_day.held,
        "two claims on ONE day is the room a server with its own zone leaves, and it must \
         fail: {}",
        same_day.saying,
    );

    claim("2026-08-20", "and one from the other side of the line").await;
    let two_days = judge(&surface, "Phase 11").await;
    assert!(
        two_days.held,
        "two distinct days is what the caller's own frame leaves: {}",
        two_days.saying,
    );
}

/// **Phase 10 holds only once a thing FITS the type**, not once the type
/// exists.
///
/// Three rooms, and the middle one is the trap: a declaration with nothing
/// written under it is a caller who declared a vocabulary and never used it,
/// which is a phase that stopped halfway rather than one that worked.
#[tokio::test]
async fn the_vocabulary_check_needs_a_thing_that_fits_the_type() {
    let (_room, surface, sid) = room().await;

    let nothing = judge(&surface, "Phase 10").await;
    assert!(
        !nothing.held,
        "a room with no such type must fail: {}",
        nothing.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "declare_type",
        json!({"name": "smoke-errand", "fields": [
            {"key": "smoke_stage", "required": true, "one_of": ["draft", "done"]},
            {"key": "smoke_note"}
        ]}),
    )
    .await;
    let declared_only = judge(&surface, "Phase 10").await;
    assert!(
        !declared_only.held,
        "a vocabulary nobody wrote anything under is half a phase: {}",
        declared_only.saying,
    );

    as_the_agent(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "thing", "handle": "smoke-the-errand", "name": "The Errand",
               "source": "user-named"}),
    )
    .await;
    as_the_agent(
        &surface,
        &sid,
        "capture",
        json!({"subject": "thing:smoke-the-errand", "content": "started it",
               "provenance": "testimony",
               "fields": {"smoke_stage": "draft", "smoke_note": "the one that fits"}}),
    )
    .await;
    let fitting = judge(&surface, "Phase 10").await;
    assert!(
        fitting.held,
        "a thing carrying the required key with a value the set names fits: {}",
        fitting.saying,
    );
}
