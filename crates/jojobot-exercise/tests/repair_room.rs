//! **The repair room, held for free.**
//!
//! Nothing here drives a model. Each lock is run against a real room whose
//! state is put there by hand, through the verbs, the way an occupant would.
//!
//! **Six locks, and each is proven to red on its own act.** A room worked in full holds
//! every lock; leaving exactly one repair undone reds exactly that lock and no
//! other, so no lock rests on another's work. A room nobody touched fails every
//! lock.

use jojobot_exercise::expectations::{self, REPAIR_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

fn document() -> Playbook {
    Playbook::read(&expectations::room_document(REPAIR_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a session to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (handle, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(REPAIR_ROOM)
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
    (handle, surface, sid)
}

async fn as_the_occupant(room: &Surface, sid: &str, verb: &str, mut args: Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

async fn judge_all(room: &Surface) -> Vec<Outcome> {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let mut outcomes = Vec::new();
    for check in expectations::for_playbook(REPAIR_ROOM).expect("the room asserts") {
        outcomes.push(check.check(&seen).await);
    }
    assert!(!outcomes.is_empty(), "the room registered no locks");
    outcomes
}

fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .map(|o| format!("\n  [{}] {} — {}", o.held, o.name, o.saying))
        .collect()
}

/// **The one repair a case leaves undone.** `None` is the room worked in full.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Undone {
    /// The loan is never filed at all.
    Unfiled,
    Loan,
    Homer,
    Bart,
    Merge,
    /// The place is mended by copying the claim across and archiving the other
    /// place: another route to the same end state, with every other repair made.
    MergeByCopy,
    /// The claim is copied across and both places are left standing: not one
    /// place yet.
    CopiedOnly,
    /// The other place is archived and the claim never moved: one place, with
    /// half of what was known.
    ArchivedOnly,
    Loop,
}

/// **The room worked the way the surface allows, one repair at a time.**
///
/// The loan is filed under a key of the model's own when it is the one left
/// undone, which is the real mistake: the same fact, the wrong key.
async fn worked(room: &Surface, sid: &str, undone: Option<Undone>) {
    as_the_occupant(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "kettle", "name": "The Kettle", "source": "user-named"}),
    )
    .await;
    let key = if undone == Some(Undone::Loan) {
        "needed_by"
    } else {
        "runs_out"
    };
    if undone != Some(Undone::Unfiled) {
        as_the_occupant(
            room,
            sid,
            "capture",
            json!({"subject": "thing:kettle", "content": "On loan; it has to go back.",
                   "provenance": "testimony", "fields": {key: "2026-11-03"}}),
        )
        .await;
    }

    if undone != Some(Undone::Homer) {
        as_the_occupant(
            room,
            sid,
            "capture",
            json!({"subject": "person:homer", "content": "Allergic to peanuts.",
                   "provenance": "testimony", "fields": {"allergy": "peanuts"}}),
        )
        .await;
    }
    if undone != Some(Undone::Bart) {
        let read = as_the_occupant(
            room,
            sid,
            "recall",
            json!({"subject": "person:bart", "facts": true}),
        )
        .await;
        let read: Value = serde_json::from_str(&read).expect("json");
        let address = read["objects"][0]["facts"]
            .as_array()
            .and_then(|facts| {
                facts.iter().find(|f| {
                    f["content"]
                        .as_str()
                        .is_some_and(|c| c.contains("allergic"))
                })
            })
            .and_then(|f| f["address"].as_str())
            .unwrap_or_else(|| panic!("the allergy claim is on Bart: {read}"))
            .to_string();
        as_the_occupant(
            room,
            sid,
            "update_fact",
            json!({"address": address, "status": "archived", "details": "it is Homer's"}),
        )
        .await;
    }
    match undone {
        Some(Undone::Merge) => {}
        Some(Undone::ArchivedOnly) => {
            as_the_occupant(
                room,
                sid,
                "archive_entity",
                json!({"handle": "place:bet", "reason": "the same place as the Atlas Tavern"}),
            )
            .await;
        }
        Some(Undone::MergeByCopy) | Some(Undone::CopiedOnly) => {
            as_the_occupant(
                room,
                sid,
                "capture",
                json!({"subject": "place:atlas",
                       "content": "The Thursday quiz night is at Betty's Bar.",
                       "provenance": "testimony"}),
            )
            .await;
            if undone == Some(Undone::MergeByCopy) {
                as_the_occupant(
                    room,
                    sid,
                    "archive_entity",
                    json!({"handle": "place:bet", "reason": "the same place as the Atlas Tavern"}),
                )
                .await;
            }
        }
        _ => {
            as_the_occupant(
                room,
                sid,
                "merge_entities",
                json!({"duplicate": "place:bet", "survivor": "place:atlas"}),
            )
            .await;
        }
    }
    if undone != Some(Undone::Loop) {
        as_the_occupant(
            room,
            sid,
            "archive_entity",
            json!({"handle": "rhythm:water-the-fern", "reason": "given away"}),
        )
        .await;
    }
}

/// Which locks held, in the order the room lists them: the loan filed, the loan
/// under its shipped key, Homer, Bart, the place, the fern.
async fn held_after(undone: Option<Undone>) -> (Vec<bool>, String) {
    let outcomes = outcomes_after(undone).await;
    (outcomes.iter().map(|o| o.held).collect(), saying(&outcomes))
}

/// The outcomes themselves, for a case that reads what a lock said.
async fn outcomes_after(undone: Option<Undone>) -> Vec<Outcome> {
    let (_room, surface, sid) = furnished().await;
    worked(&surface, &sid, undone).await;
    judge_all(&surface).await
}

/// **Three cold sittings, one line each, asking for no report.**
#[test]
fn the_room_is_three_cold_sittings_of_one_line() {
    let room = document();
    assert_eq!(
        room.phases.len(),
        3,
        "{:?}",
        room.phases.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    for phase in &room.phases {
        assert!(
            phase.fresh_session,
            "{}: every sitting arrives cold",
            phase.name
        );
        assert_eq!(
            phase.deliveries.len(),
            1,
            "{}: {:?}",
            phase.name,
            phase.deliveries
        );
        assert_eq!(
            phase.prompt.lines().count(),
            1,
            "{}: {:?}",
            phase.name,
            phase.prompt
        );
        for asked in ["PASS", "FAIL", "PARTIAL", "Report"] {
            assert!(
                !phase.prompt.contains(asked),
                "{}: asks for a verdict",
                phase.name
            );
        }
    }
}

/// **The entries give every fact and never the move.**
///
/// No verb the room serves, held against the verbs it actually serves; and none
/// of the words that name a repair or a shipped key.
#[tokio::test]
async fn the_entries_name_no_verb_no_key_and_no_repair() {
    let (_room, surface, _sid) = furnished().await;
    let verbs = surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(verbs.len() > 5, "{} verbs served", verbs.len());
    for phase in &document().phases {
        for verb in &verbs {
            let name = verb["name"].as_str().expect("a verb has a name");
            assert!(
                !phase.prompt.contains(name),
                "{} names the verb {name}",
                phase.name
            );
        }
        let said = phase.prompt.to_lowercase();
        for coached in [
            "runs_out",
            "runs-out",
            "decide_by",
            "merge",
            "archive",
            "retract",
            "duplicate",
            "clear_fields",
            "derived_from",
        ] {
            assert!(
                !said.contains(coached),
                "{} says {coached}: {:?}",
                phase.name,
                phase.prompt
            );
        }
    }
}

/// **Six locks, each keyed to the phase it belongs to.**
#[test]
fn the_room_has_six_locks_in_three_phases() {
    let checks = expectations::for_playbook(REPAIR_ROOM).expect("the room has expectations");
    let names: Vec<String> = checks.iter().map(|c| c.name().to_string()).collect();
    assert_eq!(names.len(), 6, "{names:?}");
    assert_eq!(
        names.iter().filter(|n| n.starts_with("Phase 1")).count(),
        1,
        "{names:?}"
    );
    assert_eq!(
        names.iter().filter(|n| n.starts_with("Phase 2")).count(),
        1,
        "{names:?}"
    );
    assert_eq!(
        names.iter().filter(|n| n.starts_with("Phase 3")).count(),
        4,
        "{names:?}"
    );
}

/// **A room nobody touched fails every lock.**
#[tokio::test]
async fn every_lock_fails_on_a_room_nobody_worked_in() {
    let (_room, surface, _sid) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "an untouched room held a lock: {}",
            saying(&outcomes)
        );
    }
}

/// **The room is solvable: every repair made holds every lock.**
#[tokio::test]
async fn every_lock_holds_when_every_repair_is_made() {
    let (held, said) = held_after(None).await;
    assert_eq!(held, vec![true; 6], "{said}");
}

/// **The filing lock reds when the loan is never filed. The key lock reds with
/// it, because a loan that was not filed is under no key; the room's other
/// locks do not.**
#[tokio::test]
async fn the_filing_lock_reds_when_the_loan_is_never_filed() {
    let (held, said) = held_after(Some(Undone::Unfiled)).await;
    assert_eq!(held, vec![false, false, true, true, true, true], "{said}");
}

/// **The loan lock reds when the day stays under a key of the model's own, and
/// nothing else does.**
#[tokio::test]
async fn the_loan_lock_reds_when_the_loan_stays_under_an_invented_key() {
    let (held, said) = held_after(Some(Undone::Loan)).await;
    assert_eq!(held, vec![true, false, true, true, true, true], "{said}");
}

/// **Homer's lock reds when the correction never reaches him, and nothing else
/// does.**
#[tokio::test]
async fn the_homer_lock_reds_when_the_correction_never_reaches_homer() {
    let (held, said) = held_after(Some(Undone::Homer)).await;
    assert_eq!(held, vec![true, true, false, true, true, true], "{said}");
}

/// **Bart's lock reds when the allergy is left on Bart, and nothing else does.**
#[tokio::test]
async fn the_bart_lock_reds_when_the_allergy_is_left_on_bart() {
    let (held, said) = held_after(Some(Undone::Bart)).await;
    assert_eq!(held, vec![true, true, true, false, true, true], "{said}");
}

/// **The place lock reds when the two names are left as two things, and nothing
/// else does.**
#[tokio::test]
async fn the_place_lock_reds_when_the_two_names_are_left_as_two_things() {
    let (held, said) = held_after(Some(Undone::Merge)).await;
    assert_eq!(held, vec![true, true, true, true, false, true], "{said}");
}

/// **The place lock judges the end state and not the route, and says which
/// route held.**
///
/// A merge and a copy-and-archive end in the same place: one place carries both
/// claims and the other is out of the way. Each holds the lock, and the lock's
/// own text names the route so the transcript reader can tell them apart. Every
/// other repair is made in both runs, so only the place differs.
#[tokio::test]
async fn the_place_lock_holds_by_either_route_and_says_which() {
    let by_merge = outcomes_after(None).await;
    assert!(by_merge.iter().all(|o| o.held), "{}", saying(&by_merge));
    let by_copy = outcomes_after(Some(Undone::MergeByCopy)).await;
    assert!(by_copy.iter().all(|o| o.held), "{}", saying(&by_copy));
    let place = |all: &[Outcome]| {
        all.iter()
            .find(|o| o.name.contains("two names for one place"))
            .expect("the place lock")
            .saying
            .clone()
    };
    assert!(
        place(&by_merge).contains("route: merge"),
        "{}",
        place(&by_merge)
    );
    assert!(
        place(&by_copy).contains("route: copy"),
        "{}",
        place(&by_copy)
    );
}

/// **A claim copied across with both places left standing is not one place.**
#[tokio::test]
async fn the_place_lock_reds_when_the_claim_is_copied_and_both_places_remain() {
    let (held, said) = held_after(Some(Undone::CopiedOnly)).await;
    assert_eq!(held, vec![true, true, true, true, false, true], "{said}");
}

/// **Archiving the other place without moving its claim is not one place with
/// everything known.**
#[tokio::test]
async fn the_place_lock_reds_when_the_other_place_is_archived_and_its_claim_never_moved() {
    let (held, said) = held_after(Some(Undone::ArchivedOnly)).await;
    assert_eq!(held, vec![true, true, true, true, false, true], "{said}");
}

/// **The fern lock reds when the fern is left owed, and nothing else does.**
#[tokio::test]
async fn the_fern_lock_reds_when_the_fern_is_left_owed() {
    let (held, said) = held_after(Some(Undone::Loop)).await;
    assert_eq!(held, vec![true, true, true, true, true, false], "{said}");
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
///
/// Run against the room worked in full, so a needle that matches nowhere is a
/// lock failing or a walk that could not read the answer.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface, sid) = furnished().await;
    worked(&surface, &sid, None).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(REPAIR_ROOM),
        &[],
    )
    .await;
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere: {:?}",
        summary.nowhere
    );
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else: {:?}",
        summary.findings,
    );
}
