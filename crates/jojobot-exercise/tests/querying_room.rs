//! **The querying room, held for free.**
//!
//! Nothing here drives a model. The board is furnished by the room's own
//! document, a sitting is played by hand through the verbs the way one that
//! found the right things would play it, and each question is asked of the served
//! surface. Two kinds of case, kept apart by name, because they fail for
//! different reasons and only one of them looks like a defect in the product:
//!
//! * **`solvable_…`** — one question, asked as ONE recall expression, and the
//!   answer checked to be exactly the one the room is built around. A room whose
//!   own arithmetic is wrong, or whose question the build cannot put in one call,
//!   is unreachable by every session that ever enters it, the discrimination cases
//!   still pass, and the failure surfaces on a paid run reading as a defect in the
//!   product.
//! * **`a_querying_room_nobody_worked_in_fails_every_lock`**,
//!   **`every_lock_holds_once_the_questions_are_answered`** and the
//!   **`…_fails_only_the_…_lock`** cases — the locks discriminate: a room nobody
//!   put an answer into holds none, the answers given the way they are meant to be
//!   hold all, and one wrong answer fails the one lock it is about.

use jojobot_exercise::expectations::{self, QUERYING_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// **The day this room's server runs on**, read from the room's own document
/// (the one the paid path reads) and carried to the child as its own clock. Every moment the server stamps is this day, whatever the real date is,
/// so no needle can be met by a stamp on one real day. It is a day no needle
/// of this room holds, and `no_needle_here_is_the_servers_own_day` asks that.
static SERVER_DAY: LazyLock<String> = LazyLock::new(|| {
    room_document()
        .stated_server_day()
        .unwrap_or_else(|e| panic!("{e:#}"))
        .to_string()
});

/// The document a run is driven by, read from the root of the workspace.
#[allow(dead_code)]
fn room_document() -> Playbook {
    Playbook::read(&expectations::room_document(QUERYING_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it.
async fn furnished() -> (Room, Surface) {
    let (room, surface) = Room::open_with_client_stating(
        &server_binary().expect("a jojobot binary"),
        SERVER_DAY.as_str(),
    )
    .await
    .expect("a room");
    expectations::seed_for(QUERYING_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    (room, surface)
}

/// **A session for the one cold sitting, on the day it states.**
async fn sitting(room: &Surface, day: &str) -> String {
    let booted = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day}),
        )
        .await
        .expect("the shipped identity boots");
    if let Some(sid) = booted["session"]["sid"].as_str() {
        return sid.to_string();
    }
    let answered = room
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": day, "resume": "new"}),
        )
        .await
        .expect("the boot answering the choice is ok");
    answered["session"]["sid"]
        .as_str()
        .unwrap_or_else(|| panic!("answering `new` on {day} handed back no handle: {answered}"))
        .to_string()
}

/// The day the room's one sitting states.
const DAY: &str = "2026-10-12";

/// **A call the occupant would make, which must not be refused.** A sitting that
/// cannot write its answers is a room that cannot be solved, so a refusal here is
/// the case failing, not an answer to read.
async fn did(room: &Surface, sid: &str, verb: &str, mut args: Value) -> Value {
    args["sid"] = json!(sid);
    let text = room.call(verb, args.clone()).await;
    let body: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{verb} answered something that is not json ({e}): {text}"));
    assert!(
        body["status"] != "blocked" && body["transport_error"].is_null(),
        "{verb} was refused, so the answers could not be written: {text}\n  sent: {args}",
    );
    body
}

/// **The one expression each answerable question is.** Question 5 is a question
/// about git and 8 is the board's own text, so neither is here. Each is one call
/// to `recall`, with no `sid`, exactly as a lock's query is.
fn expression(question: u8) -> Value {
    match question {
        // 1. Busy, by line.
        1 => json!({"kind": "work", "where": "status = working", "order": {"key": "line"}}),
        // 2. Dispatchable: queued, and startable as the ready view reads it.
        2 => json!({"view": "ready", "where": "status = queued"}),
        // 3. Accepted and not yet carried, in the order they will be carried.
        3 => json!({"kind": "work", "where": "status = accepted", "order": {"key": "carry"}}),
        // 4. One line's plate, in the order it comes.
        4 => json!({
            "kind": "work",
            "where": "line = beta && status in (queued, working)",
            "order": {"key": "position"},
        }),
        // 6. Open, and touching one file.
        6 => json!({
            "kind": "work",
            "where": "touches contains \"src/ui.rs\" && !(status in (done, carried))",
        }),
        // 7. The stages in use under the project.
        7 => json!({"parent": "project:migration", "values": "status"}),
        other => panic!("question {other} is not one expression on this build"),
    }
}

/// The handles an answer lists, in the order it lists them.
fn listed(answer: &Value) -> Vec<String> {
    answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists no objects: {answer}"))
        .iter()
        .filter_map(|object| object["id"].as_str().map(str::to_string))
        .collect()
}

/// Ask a question as its one expression, with no session.
async fn ask(surface: &Surface, question: u8) -> Value {
    let text = surface.call("recall", expression(question)).await;
    serde_json::from_str(&text).unwrap_or_else(|e| {
        panic!("question {question} answered something that is not json ({e}): {text}")
    })
}

/// **What each answerable question is built to come back with.**
const BUSY: [&str; 2] = ["work:delta", "work:tau"];
const HAND_OUT: [&str; 3] = ["work:epsilon", "work:rho", "work:omicron"];
const ACCEPTED_IN_ORDER: [&str; 3] = ["work:kappa", "work:iota", "work:lambda"];
const BETAS_PLATE_IN_ORDER: [&str; 3] = ["work:tau", "work:theta", "work:omicron"];
const TOUCHING_UI: [&str; 3] = ["work:delta", "work:epsilon", "work:lambda"];
const STAGES: [&str; 5] = ["working", "queued", "accepted", "carried", "done"];
/// The four of the board's nine stages that no piece is in. The surface's own answer for
/// the stages in use leaves them out; no lock demands the same of a sitting's words.
const UNUSED_STAGES: [&str; 4] = ["someday", "next", "now", "waiting"];

fn sorted(mut handles: Vec<String>) -> Vec<String> {
    handles.sort();
    handles
}

fn owned(handles: &[&str]) -> Vec<String> {
    handles.iter().map(|h| (*h).to_string()).collect()
}

/// **Question 1.** Who is busy: the pieces in the working stage, by line.
#[tokio::test]
async fn solvable_delta_on_alpha_and_tau_on_beta_are_the_busy_ones() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 1).await;
    assert_eq!(
        listed(&answer),
        owned(&BUSY),
        "the working pieces, alpha's first: {answer}"
    );
}

/// **Question 2.** What could be handed out. Rho is startable only because the
/// instance reads a carried piece as a finished one; sigma waits on a piece that is
/// not finished and theta waits on a colleague.
#[tokio::test]
async fn solvable_epsilon_rho_and_omicron_could_be_handed_out() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 2).await;
    assert_eq!(
        sorted(listed(&answer)),
        sorted(owned(&HAND_OUT)),
        "queued, waiting on nobody, and every piece it depends on finished: {answer}"
    );
}

/// **Question 3.** Accepted and not yet carried, in the order they will be carried.
#[tokio::test]
async fn solvable_kappa_iota_and_lambda_are_accepted_in_the_order_they_will_be_carried() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 3).await;
    assert_eq!(
        listed(&answer),
        owned(&ACCEPTED_IN_ORDER),
        "the accepted pieces by the day each is carried: {answer}"
    );
}

/// **Question 4.** Beta's plate, in the order it comes. Iota is on beta and is
/// accepted, so it is not on the plate. Omicron is tenth: only a key declared a
/// number orders it after the second.
#[tokio::test]
async fn solvable_taus_thetas_and_omicrons_are_betas_plate_in_order() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 4).await;
    assert_eq!(
        listed(&answer),
        owned(&BETAS_PLATE_IN_ORDER),
        "beta's queued and working pieces by position: {answer}"
    );
}

/// **Question 6.** Open and touching `src/ui.rs`. Xi is done and omega is carried, and
/// both touch it.
#[tokio::test]
async fn solvable_delta_epsilon_and_lambda_are_open_and_touch_the_ui_file() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 6).await;
    assert_eq!(
        sorted(listed(&answer)),
        sorted(owned(&TOUCHING_UI)),
        "open pieces holding the file: {answer}"
    );
}

/// **Question 7.** The stages in use under the project: five of the nine the board
/// lists.
#[tokio::test]
async fn solvable_five_of_the_boards_nine_stages_are_in_use() {
    let (_room, surface) = furnished().await;
    let answer = ask(&surface, 7).await;
    let text = answer["values"].to_string();
    for stage in STAGES {
        assert!(text.contains(stage), "{stage} is in use: {answer}");
    }
    for unused in UNUSED_STAGES {
        assert!(
            !text.contains(unused),
            "{unused} is on the board's list and no piece is in it: {answer}"
        );
    }
}

/// One answer, kept under the slot the operator named, on the project it is about.
async fn answered(room: &Surface, sid: &str, slot: &str, answer: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:migration",
               "content": format!("the answer to question {slot}"),
               "provenance": "testimony", "sets": {format!("answer_{slot}"): answer}}),
    )
    .await;
}

fn joined(handles: &[&str]) -> String {
    handles.join(", ")
}

/// **The answers a sitting that found everything gives**, as `(slot, answer)`.
/// Questions 5 and 8 are answered in prose and no lock reads them.
fn the_answers() -> Vec<(&'static str, String)> {
    vec![
        ("1", joined(&BUSY)),
        ("2", joined(&HAND_OUT)),
        ("3", joined(&ACCEPTED_IN_ORDER)),
        ("4", joined(&BETAS_PLATE_IN_ORDER)),
        (
            "5",
            "The store change is not in jojobot; ask git.".to_string(),
        ),
        ("6", joined(&TOUCHING_UI)),
        ("7", STAGES.join(", ")),
        ("8", "Theta is blocked on Kyle.".to_string()),
    ]
}

/// A sitting that writes `answers`, then the final reading the locks take.
async fn answers_written(answers: &[(&str, String)]) -> (Room, Surface) {
    let (room, surface) = furnished().await;
    let sid = sitting(&surface, DAY).await;
    for (slot, answer) in answers {
        answered(&surface, &sid, slot, answer).await;
    }
    (room, surface)
}

async fn judge_all(room: &Surface) -> Vec<Outcome> {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let mut outcomes = Vec::new();
    for check in expectations::for_playbook(QUERYING_ROOM).expect("the querying room asserts") {
        outcomes.push(check.check(&seen).await);
    }
    assert!(
        !outcomes.is_empty(),
        "the querying room registered no locks, so a run would report a pass over an empty list",
    );
    outcomes
}

/// What the run would say, so a failure names the lock that failed.
fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .enumerate()
        .map(|(at, o)| format!("\n  {at:>2} [{}] {}", o.held, o.saying))
        .collect()
}

/// **A room nobody answered in holds none of the locks.** Furnished with the
/// board and nothing written: the work is all there, and not one question has an
/// answer.
#[tokio::test]
async fn a_querying_room_nobody_worked_in_fails_every_lock() {
    let (_room, surface) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a room nobody worked in held a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The questions answered the way they are meant to be, and the locks all
/// hold.**
#[tokio::test]
async fn every_lock_holds_once_the_questions_are_answered() {
    let (_room, surface) = answers_written(&the_answers()).await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "the questions answered the way they are meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// One answer, kept as a document whose handle is the slot: `document:answer-1`. A
/// handle holds no underscore, so the sitting's `answer_1` becomes `answer-1`. A cold
/// sitting keeps what it is asked to keep "under the name I give it", and a document
/// is a place it reaches for.
async fn answered_in_a_document(room: &Surface, sid: &str, slot: &str, answer: &str) {
    let handle = format!("answer-{slot}");
    let mut made = json!({"kind": "document", "handle": handle, "name": handle,
                          "source": "the operator", "sid": sid});
    // **Each answer after the first is a near-miss of the one before**, so the guard
    // blocks the creation with a token and the sitting sends it again with the token and
    // a reason, as the refusal says to.
    let first: Value = serde_json::from_str(&room.call("add_entity", made.clone()).await)
        .expect("add_entity answers json");
    if first["status"] == "blocked" {
        // The token is named in the refusal's own words, as a caller reads it.
        let token = first["how_to_proceed"]
            .as_str()
            .and_then(|said| said.split("override_token: \"").nth(1))
            .and_then(|rest| rest.split('"').next())
            .unwrap_or_else(|| panic!("the refusal names no token: {first}"));
        made["override_token"] = json!(token);
        made["override_reason"] = json!("a different question's answer");
        let again: Value = serde_json::from_str(&room.call("add_entity", made.clone()).await)
            .expect("add_entity answers json");
        assert!(
            again["status"] != "blocked",
            "the override was refused: {again}"
        );
    }
    did(
        room,
        sid,
        "put_document",
        json!({"document": format!("document:{handle}"),
               "markdown": format!("# Answer {slot}\n\n{answer}\n"),
               "provenance": "testimony"}),
    )
    .await;
}

/// **An answer kept as a document named for its slot is an answer.** The slot is
/// the name the operator gave, and a sitting that keeps it as a document under that
/// name has kept it where it was asked to.
#[tokio::test]
async fn every_lock_holds_when_the_answers_are_kept_as_documents_named_for_the_slot() {
    let (_room, surface) = furnished().await;
    let sid = sitting(&surface, DAY).await;
    for (slot, answer) in the_answers() {
        answered_in_a_document(&surface, &sid, slot, &answer).await;
    }
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "answers kept as documents named for their slots failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **A slot nothing was kept for says so**, and does not say the answer was wrong.
/// The room is furnished and not one answer is written, so every slot lock names its
/// own slot. Paired with the case above, where the same locks hold.
#[tokio::test]
async fn a_slot_with_no_answer_anywhere_says_no_answer_was_kept_for_it() {
    let (_room, surface) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for slot in [1, 2, 3, 4, 6, 7] {
        let said = format!("no answer was kept for slot {slot}");
        assert!(
            outcomes.iter().any(|o| !o.held && o.saying.contains(&said)),
            "no lock says {said:?}: {}",
            saying(&outcomes),
        );
    }
}

/// **One slot left unanswered fails its own lock with that sentence, and the others
/// hold.** A wrong answer in a slot still fails with the wrong-answer sentence, which
/// the cases below hold.
#[tokio::test]
async fn one_unanswered_slot_fails_only_its_lock_and_says_nothing_was_kept() {
    let answers: Vec<_> = the_answers()
        .into_iter()
        .filter(|(slot, _)| *slot != "4")
        .collect();
    let (_room, surface) = answers_written(&answers).await;
    let outcomes = judge_all(&surface).await;
    only_one_fails(&outcomes, "no answer was kept for slot 4");
    let failed = outcomes.iter().find(|o| !o.held).expect("one failed");
    assert!(
        !failed
            .saying
            .contains("is not tau, theta and omicron alone"),
        "an unanswered slot is not a wrong answer: {}",
        failed.saying
    );
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface) = answers_written(&the_answers()).await;
    let boundaries: Vec<Boundary> = Vec::new();
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(QUERYING_ROOM),
        &boundaries,
    )
    .await;
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere, so either its lock is failing or the walk could not read the \
         answer — and those are different: {:?}",
        summary.nowhere,
    );
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else, with nothing else in that lock \
         only its own sitting could satisfy: {:?}",
        summary.findings,
    );
}

/// **Every lock but one holds**, and the one is the one named. A discriminating
/// case is worth nothing if it reds everything: the answers are otherwise right, so
/// a red on any other lock is the case being wrong, and a green on the named one is
/// the lock being blind.
fn only_one_fails(outcomes: &[Outcome], piece: &str) {
    let failed: Vec<&Outcome> = outcomes.iter().filter(|o| !o.held).collect();
    assert_eq!(
        failed.len(),
        1,
        "exactly the one lock fails, and it is the one named: {}",
        saying(outcomes),
    );
    assert!(
        failed[0].name.contains(piece) || failed[0].saying.contains(piece),
        "the lock that failed is not the one the wrong answer is about ({piece:?}): {}",
        saying(outcomes),
    );
}

/// The answers, with one slot given a different answer.
fn answers_but(slot: &str, wrong: &str) -> Vec<(&'static str, String)> {
    the_answers()
        .into_iter()
        .map(|(at, answer)| {
            if at == slot {
                (at, wrong.to_string())
            } else {
                (at, answer)
            }
        })
        .collect()
}

/// **A queued piece was counted as busy.** The wrong answer: epsilon is waiting its
/// turn on alpha, not in hand.
#[tokio::test]
async fn an_answer_that_counted_a_queued_piece_as_busy_fails_only_the_busy_lock() {
    let (_room, surface) =
        answers_written(&answers_but("1", "work:delta, work:tau, work:epsilon")).await;
    only_one_fails(&judge_all(&surface).await, "the pieces in hand");
}

/// **A piece that waits was offered to be handed out.** The wrong answer: sigma
/// depends on epsilon, which is not finished.
#[tokio::test]
async fn an_answer_that_offered_a_waiting_piece_fails_only_the_hand_out_lock() {
    let (_room, surface) = answers_written(&answers_but(
        "2",
        "work:epsilon, work:rho, work:omicron, work:sigma",
    ))
    .await;
    only_one_fails(&judge_all(&surface).await, "what could be handed out");
}

/// **A carried piece was listed as accepted and waiting.** The wrong answer: omega is
/// carried already.
#[tokio::test]
async fn an_answer_that_listed_a_carried_piece_fails_only_the_accepted_lock() {
    let (_room, surface) = answers_written(&answers_but(
        "3",
        "work:kappa, work:iota, work:lambda, work:omega",
    ))
    .await;
    only_one_fails(&judge_all(&surface).await, "accepted and not yet carried");
}

/// **An accepted piece of beta's was on beta's plate.** The wrong answer: iota is
/// beta's, and it is accepted.
#[tokio::test]
async fn an_answer_that_put_an_accepted_piece_on_the_plate_fails_only_the_plate_lock() {
    let (_room, surface) = answers_written(&answers_but(
        "4",
        "work:tau, work:theta, work:omicron, work:iota",
    ))
    .await;
    only_one_fails(&judge_all(&surface).await, "beta's plate");
}

/// **A finished piece was listed as open.** The wrong answer: xi is done and
/// touches the file.
#[tokio::test]
async fn an_answer_that_listed_a_finished_piece_as_open_fails_only_the_file_lock() {
    let (_room, surface) = answers_written(&answers_but(
        "6",
        "work:delta, work:epsilon, work:lambda, work:xi",
    ))
    .await;
    only_one_fails(&judge_all(&surface).await, "touches src/ui.rs");
}

/// **A sentence that names the unused stages is a right answer, and the lock holds.**
/// The answer says which stages are in use and which are empty, as a person would.
/// Whether the stage words were read from the work or from the board's list is a
/// person's reading, so no lock refuses the sentence.
#[tokio::test]
async fn an_answer_that_names_the_empty_stages_beside_the_five_in_use_holds_every_lock() {
    let (_room, surface) = answers_but_written(
        "7",
        "Five of the board's nine stages are in use: working, queued, accepted, carried, done. \
         Empty: someday, next, now, waiting.",
    )
    .await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "the stages in use, with the empty ones named, failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **An answer that leaves a stage in use out still fails the stages lock.** Paired with
/// the case above: the lock still reads what is there.
#[tokio::test]
async fn an_answer_that_leaves_a_stage_in_use_out_fails_only_the_stages_lock() {
    let (_room, surface) =
        answers_written(&answers_but("7", "working, queued, accepted, carried")).await;
    only_one_fails(&judge_all(&surface).await, "the stages in use");
}

/// A sitting that writes `answers` with one slot given a different answer.
async fn answers_but_written(slot: &str, answer: &str) -> (Room, Surface) {
    answers_written(&answers_but(slot, answer)).await
}

/// **The pieces named by name, and no handle written, is a right answer.** A person
/// answering "who is busy" writes Delta and Tau, so a lock that wants `work:delta`
/// reads a right answer as wrong. The lock accepts the name of the entity the handle
/// stands for, as a whole word in any case.
#[tokio::test]
async fn an_answer_in_names_holds_the_lock_and_a_wrong_answer_in_names_still_fails_it() {
    let (_room, surface) =
        answers_but_written("1", "Delta on alpha and Tau on beta are in hand.").await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "an answer in names failed a lock: {}",
            saying(&outcomes),
        );
    }
    // The wrong answer in names: epsilon is queued, not in hand.
    let (_room, surface) = answers_but_written(
        "1",
        "Delta on alpha, Tau on beta and Epsilon on alpha are in hand.",
    )
    .await;
    only_one_fails(&judge_all(&surface).await, "the pieces in hand");
    // The name does not stand for a piece that is not named: an answer that names
    // only Delta still lacks tau.
    let (_room, surface) = answers_but_written("1", "Delta on alpha is in hand.").await;
    only_one_fails(&judge_all(&surface).await, "the pieces in hand");
}

/// **The room's server runs on the day the room gave it, and never on the wall
/// clock.** `ping` says which day the server is acting out. A server on the real
/// clock says none, so this fails on every day if the pin is missing.
#[tokio::test]
async fn the_rooms_server_runs_on_the_day_the_room_gave_it() {
    let (_room, surface) = furnished().await;
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some(SERVER_DAY.as_str()),
        "the server is not acting out the day the room gave it"
    );
}

/// **No needle of this room holds the day its server runs on.** A needle is
/// matched as text in an answer, and the server writes its day into the moments
/// it stamps, so a needle holding that day could be met by a stamp.
#[test]
fn no_needle_here_is_the_servers_own_day() {
    let named = jojobot_exercise::lock::needles_naming(
        &jojobot_exercise::lock::locks_of(QUERYING_ROOM),
        SERVER_DAY.as_str(),
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}
