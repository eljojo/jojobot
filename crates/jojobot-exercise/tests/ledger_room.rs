//! **The ledger room, held four ways, for free.**
//!
//! Two phases, the second cold. The terminal question is which job is filled in
//! with a word the operator does not use, and the set of words that are in use
//! is said once, in a brief the cold session never sees. **A declaration is the
//! only thing that carries the set across**, so a first phase that wrote the
//! jobs down and declared nothing leaves a cold session unable to tell `sent`
//! from `paid`.
//!
//! Both kinds of case are here and they are named apart: the ones that prove
//! the checks discriminate, and the one that proves the room is solvable.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome, uncovered_phases};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// **The locks this room carries, in the order its document writes them.**
///
/// The names are this file's rather than the document's: what is pinned is the
/// order of the locks and what each one is about, so a sentence rewritten in
/// the room does not break a case here.
const BRIEF_LEFT_THE_BOX: usize = 0;
const THE_PUMP_JOB: usize = 1;
const THE_CHAIN_JOB: usize = 2;
const THE_KETTLE_IS_PUT_RIGHT: usize = 3;
const THE_AIR_FILTER_IS_PUT_RIGHT: usize = 4;
const THE_JUKEBOX_IS_UNTOUCHED: usize = 5;
const THE_TORQUE_WRENCH_IS_UNTOUCHED: usize = 6;

/// **The locks that assert the furniture survived**, which hold on a room
/// nobody worked in — the positive half the two above them rest on.
const THE_FURNITURE: [usize; 2] = [THE_JUKEBOX_IS_UNTOUCHED, THE_TORQUE_WRENCH_IS_UNTOUCHED];

/// How many locks the room carries.
const LOCKS: usize = 7;

/// What the run would report, given the locks that held and no others.
fn only(held: &[usize]) -> Vec<bool> {
    let mut want = vec![false; LOCKS];
    for at in held {
        want[*at] = true;
    }
    want
}

/// The document a run is driven by, read from the root of the workspace.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::LEDGER_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::LEDGER_ROOM)
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
    let checks = expectations::for_playbook(expectations::LEDGER_ROOM).expect("the room asserts");
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

/// The whole of phase one done properly: the brief taken, the operator's three
/// words declared as a set, and both new jobs written under the keys the older
/// jobs use.
async fn worked_the_first_phase(room: &Surface, sid: &str) {
    as_the_occupant(room, sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        room,
        sid,
        "declare_type",
        json!({
            "name": "job",
            "fields": [
                {"key": "cost", "holds": "number"},
                {"key": "settled", "holds": "text",
                 "one_of": ["invoiced", "paid", "waived"]},
            ],
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike", "content": "new chain",
            "provenance": "testimony",
            "fields": {"cost": "60", "settled": "invoiced"},
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "thing:floor-pump", "content": "serviced",
            "provenance": "testimony",
            "fields": {"cost": "35", "settled": "paid"},
        }),
    )
    .await;
}

/// The cold phase, done properly: the two jobs carrying words nobody agreed to
/// are put right, and nothing else is touched.
async fn worked_the_cold_phase(room: &Surface, sid: &str) {
    for (subject, was) in [
        ("thing:kettle", "descaled by the shop"),
        ("thing:the-air-filter", "filter swap"),
    ] {
        let read = room
            .call("recall", json!({"subject": subject, "facts": true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("the read is json");
        let address = parsed["objects"][0]["facts"]
            .as_array()
            .expect("the records behind the fields")
            .iter()
            .find(|fact| fact["content"] == was)
            .and_then(|fact| fact["address"].as_str())
            .unwrap_or_else(|| panic!("the room was furnished with no job called {was}: {read}"))
            .to_string();
        as_the_occupant(
            room,
            sid,
            "update_fact",
            json!({"address": address, "fields": {"settled": "invoiced"}}),
        )
        .await;
    }
}

/// **The room is two phases, the second cold, and each is one line.**
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
    // reaches no locks — which is the run refusing at the door rather than
    // running a room against the checks of a file somewhere else.
    let checks =
        expectations::for_playbook(expectations::LEDGER_ROOM).expect("the room has expectations");
    assert_eq!(
        checks.len(),
        LOCKS,
        "the room asserts on {} things and the roll-call above names {LOCKS}",
        checks.len(),
    );
    // **The locks come out of the document and nowhere else.** This room has no
    // Rust half, so a reader that stopped finding the written locks would fall
    // through to a registry entry that no longer exists and the room would run
    // asserting nothing.
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
    );
}

/// **No phase of this room reads as having no check.**
///
/// A lock is keyed to a phase by the `Phase N` its name opens with, and a lock
/// written in a document says which phase it belongs to by where it is
/// written. A room whose locks are keyed to nothing runs them and reports both
/// phases as uncovered, which reads to a person as a room that measures
/// nothing.
#[test]
fn every_phase_of_this_room_is_asserted_over() {
    let checks =
        expectations::for_playbook(expectations::LEDGER_ROOM).expect("the room has expectations");
    assert_eq!(
        uncovered_phases(&room_document(), &checks),
        Vec::<String>::new(),
        "a phase of this room is asserted over by nothing: {:?}",
        checks.iter().map(|c| c.name()).collect::<Vec<_>>(),
    );
    // The positive control: the rule these names are held against is the one
    // that reports a phase, so a phase nothing names must come back named.
    let none: Vec<Box<dyn jojobot_exercise::run::Expectation>> = Vec::new();
    assert_eq!(
        uncovered_phases(&room_document(), &none).len(),
        room_document().phases.len(),
        "a room with no checks at all reported some phase as covered",
    );
}

/// **The entries name no verb**, held against the verbs the room serves.
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

/// **A room the occupant never got anywhere in fails every lock that measures
/// work, and holds only the two that measure the furniture.**
///
/// The furnished room already carries two jobs with the operator's own words on
/// them, and those two locks are the positive half the cold phase's locks rest
/// on: they hold here by construction. Every other lock must fail, and the two
/// halves are asserted together because a case that only said "some failed"
/// would pass on a build where nothing is checked at all.
#[tokio::test]
async fn a_room_nobody_worked_in_fails_every_lock_but_the_furniture() {
    let (_room, surface, _sid) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for (at, outcome) in outcomes.iter().enumerate() {
        assert_eq!(
            outcome.held,
            THE_FURNITURE.contains(&at),
            "a furnished room nobody touched: {}",
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

/// **The terminal lock measures the cold phase**, so a room whose first phase
/// was worked and whose second never happened must fail it and hold the rest.
#[tokio::test]
async fn the_terminal_lock_fails_when_the_cold_phase_did_nothing() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        only(&[
            BRIEF_LEFT_THE_BOX,
            THE_PUMP_JOB,
            THE_CHAIN_JOB,
            THE_JUKEBOX_IS_UNTOUCHED,
            THE_TORQUE_WRENCH_IS_UNTOUCHED,
        ]),
        "the first phase's locks hold, the two jobs the cold phase is for do not, and the \
         furniture is as it was: {}",
        saying(&outcomes),
    );
}

/// **A cold phase that rewrote every job to one word answers nothing**, and the
/// absence half alone would call it a pass.
#[tokio::test]
async fn the_terminal_lock_fails_when_a_word_that_was_right_was_painted_over() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;
    worked_the_cold_phase(&surface, &sid).await;

    // The jukebox's job was already one of the operator's words. A run that
    // swept every job to one word leaves no word nobody agreed to, which the
    // absence half alone would call a pass.
    let read = surface
        .call("recall", json!({"subject": "thing:jukebox", "facts": true}))
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    let address = parsed["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("the job on the jukebox")
        .to_string();
    as_the_occupant(
        &surface,
        &sid,
        "update_fact",
        json!({"address": address, "fields": {"settled": "invoiced"}}),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    assert!(
        !outcomes[THE_JUKEBOX_IS_UNTOUCHED].held,
        "a word that was already right was painted over and the lock on it held: {}",
        saying(&outcomes),
    );
    // The half that stops the assertion above passing on a room where every
    // lock fails: the jobs the cold phase was for were still put right.
    for at in [THE_KETTLE_IS_PUT_RIGHT, THE_AIR_FILTER_IS_PUT_RIGHT] {
        assert!(
            outcomes[at].held,
            "the cold phase's own work was undone as well: {}",
            saying(&outcomes),
        );
    }
}

/// **The room a session that did everything in PROSE leaves.**
///
/// Both new jobs are written down, in sentences, and nothing is declared. The
/// vocabulary lock fails because the operator's question does not reach a
/// sentence, and the terminal lock fails because the cold session has no way to
/// learn which words the operator uses.
#[tokio::test]
async fn the_locks_fail_on_a_room_written_in_prose() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:gravel-bike",
            "content": "new chain last week, sixty, and they have invoiced it",
            "provenance": "testimony",
        }),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "thing:floor-pump",
            "content": "serviced last week, thirty five, paid on the spot",
            "provenance": "testimony",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        only(&[
            BRIEF_LEFT_THE_BOX,
            THE_JUKEBOX_IS_UNTOUCHED,
            THE_TORQUE_WRENCH_IS_UNTOUCHED,
        ]),
        "the box was opened, nothing a question can reach was written, and the furniture is as \
         it was: {}",
        saying(&outcomes),
    );
}

/// **The room is solvable, and this is the case that says so.**
///
/// A different kind of case from every one above: those prove the checks
/// discriminate between rooms somebody put state into by hand. This one proves
/// the terminal question can be ANSWERED — that a session which declared the
/// operator's words is told which jobs do not carry them, rather than having to
/// guess.
///
/// Without it, a room whose own set was wrong would be unreachable by every
/// session that will ever enter it, every case above would still pass, and the
/// failure would surface on a paid run reading as a defect in the product.
#[tokio::test]
async fn the_question_the_cold_phase_asks_can_be_answered() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    // The question the cold session has, and the only way it is answerable:
    // a value outside the set does not count as holding its key, so the things
    // whose current word is nobody's come back marked.
    let asked = as_the_occupant(
        &surface,
        &sid,
        "recall",
        json!({"kind": "thing", "answers_type": "job"}),
    )
    .await;
    let parsed: Value = serde_json::from_str(&asked).expect("the answer is json");
    let objects = parsed["objects"].as_array().expect("objects come back");

    let marked: Vec<&str> = objects
        .iter()
        .filter(|object| {
            object["answers"]["mistyped"]
                .as_array()
                .is_some_and(|m| !m.is_empty())
        })
        .filter_map(|object| object["id"].as_str())
        .collect();
    assert_eq!(
        marked.len(),
        2,
        "the answer marks {marked:?} rather than the two jobs carrying a word nobody agreed to: \
         {asked}",
    );
    for thing in ["thing:kettle", "thing:the-air-filter"] {
        assert!(
            marked.contains(&thing),
            "{thing} carries a word nobody agreed to and the answer does not mark it: {asked}",
        );
    }
    // The half that stops the assertion above passing on an answer that marks
    // everything: the jobs that were already right are not marked.
    for thing in ["thing:jukebox", "thing:torque-wrench"] {
        assert!(
            !marked.contains(&thing),
            "{thing} carries one of the operator's own words and was marked anyway: {asked}",
        );
    }
}
