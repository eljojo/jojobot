//! **The handover room, held five ways, for free.**
//!
//! Two phases, the second cold. The terminal question is how much of a pile
//! somebody else has picked up, and that is state the store owns: no session
//! could have written it down, because the session that would have was not
//! there when it changed.
//!
//! Both kinds of case, named apart — the ones that prove the checks
//! discriminate, and the one that proves the room is solvable.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// **The lock this file's cases name**, at the position its document writes it.
/// The name is this file's: what is pinned is the order, so a sentence
/// rewritten in the room does not break a case here.
const COLLEAGUE: usize = 1;

/// The document a run is driven by, read from the root of the workspace.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::HANDOVER_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::HANDOVER_ROOM)
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
    let checks = expectations::for_playbook(expectations::HANDOVER_ROOM).expect("the room asserts");
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

/// The whole of phase one done properly: the brief taken, a colleague stood up
/// with the box that comes with it, chartered, and the pile handed over one
/// thing at a time.
async fn worked_the_first_phase(room: &Surface, sid: &str) {
    as_the_occupant(room, sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        room,
        sid,
        "add_entity",
        json!({
            "kind": "bot", "handle": "gamma", "name": "Gamma",
            "source": "the operator",
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "set_charter",
        json!({
            "bot": "gamma",
            "charter": "You read what the operator does not get to, and you say what is in it.",
        }),
    )
    .await;
    for (subject, body) in [
        (
            "power loss",
            "The long piece on how filesystems handle power loss.",
        ),
        ("consensus", "The paper about consensus everybody quotes."),
        (
            "the espresso machine",
            "Forty pages of manual, and it is never getting read otherwise.",
        ),
    ] {
        as_the_occupant(
            room,
            sid,
            "post_message",
            json!({"to": "gamma", "subject": subject, "body": body}),
        )
        .await;
    }
}

/// The cold phase, done properly: what became of the pile goes onto the
/// colleague, where the operator asked for it.
async fn worked_the_cold_phase(room: &Surface, sid: &str) {
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "bot:gamma",
            "content": "has started on none of the reading — all 3 are still waiting",
            "provenance": "inference",
        }),
    )
    .await;
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
    // reaches no locks.
    let checks =
        expectations::for_playbook(expectations::HANDOVER_ROOM).expect("the room has expectations");
    assert_eq!(
        checks.len(),
        4,
        "the room's document carries four locks: {}",
        checks.len(),
    );
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
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
        vec![true, true, true, false],
        "the first phase's locks hold and the terminal one does not: {}",
        saying(&outcomes),
    );
}

/// **The terminal lock reads what the session wrote, never what the store
/// stamped.**
///
/// The lock asks whether the count is on the colleague. A read of that
/// colleague carries more than the session's own words: the day each claim is
/// about, the moment the store took it in, and the address that edits it. Those
/// carry digits nobody chose, so a lock that searches the whole payload opens
/// on a room where the cold session wrote a claim and never answered the
/// question — and, on a run whose date carries the digit, on a room where it
/// wrote nothing at all.
///
/// **This is a room a real session could leave**: it looked, it wrote
/// something onto the colleague, and what it wrote does not say how much of the
/// pile is waiting. The lock is the last gate before a deploy, so it has to
/// tell that room from the one that answered.
#[tokio::test]
async fn the_terminal_lock_ignores_a_number_the_store_stamped() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    // The claim says nothing about the count. Its DATE carries the digit, and
    // the date is the store's cell rather than the session's answer.
    as_the_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "bot:gamma",
            "content": "had a look at what the second assistant is carrying",
            "provenance": "inference",
            "date": "2026-07-03",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        vec![true, true, true, false],
        "the terminal lock opened on a digit the store stamped, so a session that never \
         answered the question passed the last gate: {}",
        saying(&outcomes),
    );
}

/// **The pile handed over as one message is a different end state**, and the
/// lock that counts must say so: a colleague left one thing to finish rather
/// than three has been handed the pile in a shape nobody can work separately.
#[tokio::test]
async fn the_counting_lock_fails_when_the_pile_arrived_as_one_message() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "bot", "handle": "gamma", "name": "Gamma", "source": "the operator"}),
    )
    .await;
    as_the_occupant(
        &surface,
        &sid,
        "post_message",
        json!({
            "to": "gamma", "subject": "the reading pile",
            "body": "Three things: power loss, consensus, and the espresso manual.",
        }),
    )
    .await;

    let outcomes = judge_all(&surface).await;
    assert!(
        outcomes[COLLEAGUE].held,
        "the colleague stands, so the count is what is being measured here: {}",
        saying(&outcomes),
    );
    assert!(
        !outcomes[2].held,
        "the whole pile arrived as one message and the counting lock held: {}",
        saying(&outcomes),
    );
}

/// **The room a session that kept the pile to itself leaves.**
///
/// The reading list is written down — as records, carefully, on the colleague
/// — and none of it was handed over. Everything reads as diligence and the
/// colleague has nothing to do, which is the failure this room is for.
#[tokio::test]
async fn the_locks_fail_when_the_pile_was_written_down_instead_of_handed_over() {
    let (_room, surface, sid) = furnished().await;

    as_the_occupant(&surface, &sid, "read_mailbox", json!({})).await;
    as_the_occupant(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "bot", "handle": "gamma", "name": "Gamma", "source": "the operator"}),
    )
    .await;
    for what in ["power loss", "consensus", "the espresso manual"] {
        as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": format!("should read: {what}"),
                "provenance": "testimony",
            }),
        )
        .await;
    }

    let outcomes = judge_all(&surface).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(
        held,
        vec![true, true, false, false],
        "the colleague stands and nothing was handed to it: {}",
        saying(&outcomes),
    );
}

/// 🚨 **A run that stood up more than one colleague is refused.**
///
/// The brief asks for a second assistant. It does not ask for a habit of
/// making them, and an identity is not a thing to mint on the way past: a box
/// opens with every bot, so a run that made three left three boxes nobody
/// asked for and a pile split across them.
///
/// **The branch that refuses this had never been exercised by anything.** It
/// is also the half that stopped this lock becoming a query — the counting
/// assertion has no upper bound, so nothing in a document can say "and only
/// one" — which makes it the half most worth watching work.
///
/// **Both readings in one case.** The one-colleague room still holds the same
/// lock, because a branch that refused every count would satisfy the refusal
/// on its own and prove nothing.
#[tokio::test]
async fn a_second_colleague_is_refused_and_one_colleague_still_holds() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    // One colleague: the lock the whole room turns on holds.
    let one = judge_all(&surface).await;
    assert!(
        one[COLLEAGUE].held,
        "the room a run worked properly failed the colleague lock, so the refusal below would \
         hold for a reason that has nothing to do with counting: {}",
        saying(&one),
    );

    // A second identity nobody asked for, made the same way as the first.
    as_the_occupant(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "bot", "handle": "delta", "name": "Delta", "source": "the operator"}),
    )
    .await;

    let two = judge_all(&surface).await;
    assert!(
        !two[COLLEAGUE].held,
        "a run that stood up two colleagues where the brief asked for one held the lock: {}",
        saying(&two),
    );
    // The sentence a reader gets has to say what happened, or the failure is a
    // boolean somebody has to open the room to understand.
    assert!(
        two[COLLEAGUE].saying.contains('2'),
        "the failure does not say how many identities were made: {}",
        two[COLLEAGUE].saying,
    );
}

/// **The room is solvable, and this is the case that says so.**
///
/// The terminal question is *how much of the pile has the colleague started
/// on*, and the cold session cannot open that box. So the answer has to be
/// reachable from the sender's own view of where its mail got to — and if it
/// were not, the room would be impossible while every case above stayed green.
#[tokio::test]
async fn the_question_the_cold_phase_asks_can_be_answered() {
    let (_room, surface, sid) = furnished().await;
    worked_the_first_phase(&surface, &sid).await;

    let sent = as_the_occupant(&surface, &sid, "list_sent", json!({"to": "gamma"})).await;
    let parsed: Value = serde_json::from_str(&sent).expect("the answer is json");
    let messages = parsed["messages"]
        .as_array()
        .unwrap_or_else(|| panic!("where the mail got to comes back as a list: {sent}"));
    assert_eq!(
        messages.len(),
        3,
        "the sender's own view does not carry the three things handed over: {sent}",
    );
    assert!(
        messages.iter().all(|message| message["state"] == "new"),
        "the pile reads as picked up, and nothing can boot as the colleague to pick it up: {sent}",
    );
    // The half that stops this passing on a view that answers the same for
    // everybody: a colleague nobody wrote to comes back empty rather than with
    // somebody else's mail.
    let empty = as_the_occupant(&surface, &sid, "list_sent", json!({"to": "assistant"})).await;
    assert!(
        !empty.contains("power loss"),
        "the sender's view of one box carried another box's mail: {empty}",
    );
}
