//! **The loop room, held four ways, for free.**
//!
//! The room is two phases and the second is cold. Its terminal lock is a
//! question no sentence answers: which loop has gone quiet as of a named day is
//! cadence plus an anchor plus a date, worked out by the machinery, so a
//! session that recorded its loops as prose cannot get there by reading
//! carefully.
//!
//! Four rooms below, and the fourth is the one that matters: a room where the
//! occupant did everything and did it in prose. **A room that passes that one
//! is measuring diligence rather than use.**

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// The document a run is driven by, read from the root of the workspace.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::LOOP_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let room = Room::open(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");
    expectations::seed_for(expectations::LOOP_ROOM)
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
    let checks = expectations::for_playbook(expectations::LOOP_ROOM).expect("the room asserts");
    let mut outcomes = Vec::new();
    for check in checks {
        outcomes.push(check.check(&seen).await);
    }
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

/// The whole of phase one done properly: the brief taken, both loops standing
/// as loops under the things they belong to, and each carrying the day the
/// brief gave for its last turn.
async fn worked_the_first_phase(room: &Surface, sid: &str) {
    as_the_occupant(room, sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        room,
        sid,
        "add_entity",
        json!({
            "kind": "rhythm", "handle": "descale", "name": "Descale the kettle",
            "source": "the operator", "parent": "thing:kettle",
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "rhythm:descale", "content": "descale the kettle every sixty days",
            "provenance": "testimony", "date": "2026-06-15",
            "fields": {
                "name": "Descale the kettle", "last_check_in": "2026-06-15",
                "cadence_days": "60", "counts_from": "2026-06-15",
                "advances_from": "due_date",
            },
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "add_entity",
        json!({
            "kind": "rhythm", "handle": "swap-the-air-filter", "name": "Swap the air filter",
            "source": "the operator", "parent": "thing:the-air-filter",
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "rhythm:swap-the-air-filter",
            "content": "swap the filter when it looks bad — no schedule",
            "provenance": "testimony", "date": "2026-09-20",
            "fields": {"name": "Swap the air filter", "last_check_in": "2026-09-20"},
        }),
    )
    .await;
}

/// The cold phase, done properly: the loop that has gone quiet as of the day
/// the operator named gets the check-in, and the other two are left alone.
async fn worked_the_cold_phase(room: &Surface, sid: &str) {
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "rhythm:descale", "content": "descaled it, twenty minutes",
            "provenance": "testimony", "check_in": "ran",
        }),
    )
    .await;
}

/// **The room is two phases, the second cold, and each is one line.**
///
/// The shape is what makes the terminal lock mean anything: inside one session
/// the model answers from its own context, so the store is optional. A phase
/// with no memory of the first is what makes it load-bearing.
#[test]
fn the_room_is_two_phases_and_the_second_has_no_memory() {
    let room = room_document();
    assert_eq!(
        room.phases.len(),
        2,
        "the room is a goal and its cold question: {:?}",
        room.phases.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );
    for phase in &room.phases {
        assert!(
            phase.fresh_session,
            "{} carries the session before it, so the store is optional in it",
            phase.name,
        );
        assert_eq!(
            phase.deliveries.len(),
            1,
            "a phase of this room holds nothing back: {:?}",
            phase.deliveries,
        );
        assert_eq!(
            phase.prompt.lines().count(),
            1,
            "the entry runs to more than one line: {:?}",
            phase.prompt,
        );
        for asked in ["PASS", "FAIL", "PARTIAL", "Report"] {
            assert!(
                !phase.prompt.contains(asked),
                "the entry asks for a verdict, which tells the occupant a test is running: {:?}",
                phase.prompt,
            );
        }
        let said = phase.prompt.to_lowercase();
        for pointed in ["mail", "box", "message", "waiting"] {
            assert!(
                !said.contains(pointed),
                "the entry points the occupant at its mail, so the first lock opens itself: {:?}",
                phase.prompt,
            );
        }
    }
}

/// Expectations are registered for this room, and a room nobody wrote any for
/// still gets none.
#[test]
fn the_room_has_expectations_of_its_own() {
    // **Named as the registry ships it.** This room has no Rust half, so its
    // locks come out of its document and a name that reaches no document
    // reaches no locks.
    let checks =
        expectations::for_playbook(expectations::LOOP_ROOM).expect("the room has expectations");
    assert_eq!(
        checks.len(),
        5,
        "the room's document carries five locks: {}",
        checks.len(),
    );
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
    );
}

/// **The entry names no verb**, held against the verbs the room actually
/// serves rather than against a list written here.
#[tokio::test]
async fn no_entry_names_a_verb_the_room_serves() {
    let (_room, surface, _sid) = furnished().await;
    let verbs = surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(
        verbs.len() > 5,
        "the room served {} verbs, so finding none in the entries proves nothing",
        verbs.len(),
    );
    for phase in &room_document().phases {
        for verb in &verbs {
            let name = verb["name"].as_str().expect("a verb has a name");
            assert!(
                !phase.prompt.contains(name),
                "{} names the verb {name}, so the occupant did not have to find it",
                phase.name,
            );
        }
    }
}

/// **A room the occupant never got anywhere in fails every check.**
#[tokio::test]
async fn every_check_fails_on_a_room_nobody_worked_in() {
    let (_room, surface, _sid) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished room nobody touched held a check: {}",
            saying(&outcomes),
        );
    }
}

/// **The room both phases worked properly leaves.**
#[tokio::test]
async fn every_check_holds_once_both_phases_are_worked() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;
    worked_the_cold_phase(&surface, &sid).await;

    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a room where the goal was worked failed a check: {}",
            saying(&outcomes),
        );
    }
}

/// **The terminal lock is the one that measures the cold phase**, and it must
/// fail on a room where the first phase was worked and the second was not.
///
/// Without this, a check that read only the loops would report the whole room
/// green on a run whose second session never happened.
#[tokio::test]
async fn the_terminal_lock_fails_when_the_cold_phase_did_nothing() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        vec![true, true, true, true, false],
        "the first phase's locks hold and the terminal one does not: {}",
        saying(&outcomes),
    );
}

/// **The room a session that did everything in PROSE leaves.**
///
/// Both loops are written down, both dates are written down, and every word of
/// it is a sentence. The cold phase then asks which one has gone quiet, and
/// there is nothing to ask: a loop written as prose is not a late loop, it is
/// not a loop. **This is the case the raised bar exists for.**
#[tokio::test]
async fn the_locks_fail_on_a_room_written_in_prose() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:kettle",
            "content": "descale it every sixty days — last done on 2026-06-15",
            "provenance": "testimony",
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:the-air-filter",
            "content": "swapped on 2026-09-20, and there is no schedule for it",
            "provenance": "testimony",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        vec![true, false, false, false, false],
        "the box was opened and nothing else can be reached: {}",
        saying(&outcomes),
    );
}

/// **The room is solvable, and this is the case that says so.**
///
/// Every other case here asserts what the checks make of a room somebody put
/// state into by hand. None of them asks the question the cold phase actually
/// has to ask — so if the arithmetic in the brief were wrong, the terminal lock
/// would be unreachable by any session and nothing above would notice: the
/// room would fail every paid run and read as a product defect.
///
/// So this asks it, through the surface, after a first phase done properly.
/// **All three halves**: the loop that is late, the loop whose day has not come
/// round, and the loop that can never be late because nobody gave it a
/// frequency.
#[tokio::test]
async fn the_question_the_cold_phase_asks_has_exactly_one_answer() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    let quiet = as_the_occupant(
        &surface,
        &sid,
        "recall",
        json!({"kind": "rhythm", "overdue": {"as_of": "2026-10-01"}}),
    )
    .await;

    assert!(
        quiet.contains("rhythm:descale"),
        "the loop that went past its day in August is not in the answer: {quiet}",
    );
    assert!(
        !quiet.contains("rhythm:water-the-fern"),
        "the loop that is not due for another month came back as quiet: {quiet}",
    );
    assert!(
        !quiet.contains("rhythm:swap-the-air-filter"),
        "the loop with no frequency came back as quiet, so nothing can be late: {quiet}",
    );
}
