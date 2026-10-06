//! **The adoption rooms, held for free.**
//!
//! Two documents, one experiment: `adoption.md` is the baseline and
//! `adoption-wording.md` says the same two facts in each key's own word. Nothing
//! here drives a model. Each lock is run against a real room whose state is put
//! there by hand, through the verbs.
//!
//! **Three rooms, as the other rooms hold theirs.** A room nobody worked fails
//! every lock; a room worked the way the entry means holds every lock; and a
//! room worked with the right facts under keys of the occupant's own reds
//! exactly the locks whose keys it replaced and leaves the independent one
//! alone.

use jojobot_exercise::adoption::FACTS;
use jojobot_exercise::expectations::{self, ADOPTION_ROOM, ADOPTION_WORDING_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

const BOTH: [&str; 2] = [ADOPTION_ROOM, ADOPTION_WORDING_ROOM];

fn document(room: &str) -> Playbook {
    Playbook::read(&expectations::room_document(room))
        .unwrap_or_else(|e| panic!("the shipped room {room} must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a session to write into it
/// as the occupant would.
async fn furnished(room: &str) -> (Room, Surface, String) {
    let (handle, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(room)
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

async fn judge_all(room: &Surface, which: &str) -> Vec<Outcome> {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let mut outcomes = Vec::new();
    for check in expectations::for_playbook(which).expect("the room asserts") {
        outcomes.push(check.check(&seen).await);
    }
    // **Said here rather than in each case**: a loop over an empty list holds
    // every claim in this file.
    assert!(!outcomes.is_empty(), "{which} registered no locks");
    outcomes
}

fn saying(outcomes: &[Outcome]) -> String {
    outcomes
        .iter()
        .map(|o| format!("\n  [{}] {} — {}", o.held, o.name, o.saying))
        .collect()
}

/// **The two things the facts are about, made the way an occupant makes them.**
/// A capture on a thing nobody created is refused, so the facts need a subject
/// first.
async fn with_the_two_things(room: &Surface, sid: &str) {
    for (kind, handle, name) in [
        ("thing", "floor-pump", "The Floor Pump"),
        ("event", "wagstaff-fair", "The Wagstaff Fair"),
    ] {
        as_the_occupant(
            room,
            sid,
            "add_entity",
            json!({"kind": kind, "handle": handle, "name": name, "source": "user-named"}),
        )
        .await;
    }
}

/// **The facts, filed the way the shipped types hold them.**
async fn worked_under_the_shipped_keys(room: &Surface, sid: &str) {
    with_the_two_things(room, sid).await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "thing:floor-pump",
            "content": "On loan; it has to go back.",
            "provenance": "testimony",
            "fields": {"runs_out": "2026-11-03"},
        }),
    )
    .await;
    as_the_occupant(
        room,
        sid,
        "capture",
        json!({
            "subject": "event:wagstaff-fair",
            "content": "The tickets have to be sorted by a day.",
            "provenance": "testimony",
            "fields": {"decide_by": "2026-10-20"},
        }),
    )
    .await;
}

/// **The room is one phase, one line, and asks for no report.**
#[test]
fn each_room_is_one_cold_phase_delivered_as_one_line() {
    for which in BOTH {
        let room = document(which);
        assert_eq!(room.phases.len(), 1, "{which}: one sitting");
        let entry = &room.phases[0];
        assert!(entry.fresh_session, "{which}: the occupant arrives cold");
        assert_eq!(entry.deliveries.len(), 1, "{which}: {:?}", entry.deliveries);
        assert_eq!(
            entry.prompt.lines().count(),
            1,
            "{which}: {:?}",
            entry.prompt
        );
        for asked in ["PASS", "FAIL", "PARTIAL", "Report"] {
            assert!(
                !entry.prompt.contains(asked),
                "{which}: the entry asks for a verdict: {:?}",
                entry.prompt,
            );
        }
    }
}

/// **The entry gives every fact and never the move.**
///
/// It names no verb the room serves, held against the verbs the room actually
/// serves rather than a list written here, and it names no type or key of the
/// ones the room is about — in either spelling.
#[tokio::test]
async fn the_entry_names_no_verb_no_type_and_no_key() {
    let (_room, surface, _sid) = furnished(ADOPTION_ROOM).await;
    let verbs = surface
        .tools_for_the_model()
        .await
        .expect("the room lists its verbs");
    assert!(verbs.len() > 5, "{} verbs served", verbs.len());
    for which in BOTH {
        let entry = &document(which).phases[0].prompt;
        for verb in &verbs {
            let name = verb["name"].as_str().expect("a verb has a name");
            assert!(
                !entry.contains(name),
                "{which} names the verb {name}: {entry:?}"
            );
        }
        for fact in FACTS {
            for spelling in [
                fact.key.to_string(),
                fact.key.replace('_', "-"),
                fact.type_name.to_string(),
                fact.type_name.replace('-', "_"),
            ] {
                assert!(
                    !entry.contains(&spelling),
                    "{which} names {spelling}, so the model did not have to find it: {entry:?}",
                );
            }
        }
    }
}

/// **The two entries differ in the wording and in nothing else.**
///
/// E1 is the same room with each fact said in its key's own word. A difference
/// anywhere else — a lock, a day, the session marker — would make the arm a
/// second factor. The dates the locks ask for are in both entries, so neither
/// arm is unsolvable.
#[test]
fn the_wording_arm_changes_the_words_and_nothing_else() {
    let (baseline, wording) = (document(ADOPTION_ROOM), document(ADOPTION_WORDING_ROOM));
    let (a, b) = (&baseline.phases[0], &wording.phases[0]);
    assert_eq!(a.name, b.name);
    assert_eq!(a.fresh_session, b.fresh_session);
    assert_eq!(a.day, b.day);
    assert_ne!(a.prompt, b.prompt, "the arms say the same thing");

    let names = |which| -> Vec<String> {
        expectations::for_playbook(which)
            .expect("the room asserts")
            .iter()
            .map(|check| check.name().to_string())
            .collect()
    };
    assert_eq!(
        names(ADOPTION_ROOM),
        names(ADOPTION_WORDING_ROOM),
        "the arms ask different questions",
    );

    // The wording factor itself: E1 says each key's own word, E0 does not.
    for own_word in ["runs out on", "decide by"] {
        assert!(
            b.prompt.contains(own_word),
            "E1 lacks {own_word:?}: {:?}",
            b.prompt
        );
        assert!(
            !a.prompt.contains(own_word),
            "E0 says {own_word:?}: {:?}",
            a.prompt
        );
    }
    for day in ["2026-11-03", "2026-10-20"] {
        assert!(
            a.prompt.contains(day) && b.prompt.contains(day),
            "{day} is missing"
        );
    }
}

/// **The harness and the room read the same keys.**
///
/// `adoption::FACTS` is what the call-log reader asks about and the locks are
/// what the store is asked about. A key that moved in one and not the other
/// would make the table and the locks disagree about what a run did.
#[test]
fn the_facts_the_harness_reads_are_the_keys_the_locks_ask_for() {
    for which in BOTH {
        let text = std::fs::read_to_string(expectations::room_document(which)).expect("readable");
        for fact in FACTS {
            assert!(
                text.contains(&format!("\"key\": \"{}\"", fact.key)),
                "{which} has no lock asking for {}",
                fact.key,
            );
        }
    }
}

/// **Both rooms carry two locks of their own, and each is keyed to the phase.**
#[test]
fn each_room_has_two_locks_keyed_to_its_phase() {
    for which in BOTH {
        let checks = expectations::for_playbook(which).expect("the room has expectations");
        assert_eq!(checks.len(), 2, "{which}");
        for check in &checks {
            assert!(
                check.name().starts_with("Phase 1"),
                "{which}: {}",
                check.name()
            );
        }
    }
}

/// **A room the occupant never wrote in fails every lock.**
#[tokio::test]
async fn every_lock_fails_on_a_room_nobody_worked_in() {
    for which in BOTH {
        let (_room, surface, _sid) = furnished(which).await;
        let outcomes = judge_all(&surface, which).await;
        for outcome in &outcomes {
            assert!(
                !outcome.held,
                "{which}: an untouched room held: {}",
                saying(&outcomes)
            );
        }
    }
}

/// **The room is solvable: the facts under the shipped keys hold every lock.**
///
/// A room whose own locks cannot be met is unreachable by every occupant and
/// every case above still passes. This is the case that says it is not.
#[tokio::test]
async fn every_lock_holds_when_the_facts_are_filed_under_the_shipped_keys() {
    for which in BOTH {
        let (_room, surface, sid) = furnished(which).await;
        worked_under_the_shipped_keys(&surface, &sid).await;
        let outcomes = judge_all(&surface, which).await;
        for outcome in &outcomes {
            assert!(
                outcome.held,
                "{which}: a worked room failed a lock: {}",
                saying(&outcomes)
            );
        }
    }
}

/// **The loan's lock reds when the day is filed under a key of the model's
/// own, and the decision's lock does not.**
///
/// The real mistake, with everything else right: the loan's last day under
/// `guarantee_expires`, a key a real run invented for the same kind of fact.
#[tokio::test]
async fn the_loan_lock_reds_under_an_invented_key_and_the_decision_lock_holds() {
    for which in BOTH {
        let (_room, surface, sid) = furnished(which).await;
        with_the_two_things(&surface, &sid).await;
        as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({
                "subject": "thing:floor-pump",
                "content": "On loan; it has to go back.",
                "provenance": "testimony",
                "fields": {"guarantee_expires": "2026-11-03"},
            }),
        )
        .await;
        as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The tickets have to be sorted by a day.",
                "provenance": "testimony",
                "fields": {"decide_by": "2026-10-20"},
            }),
        )
        .await;
        let outcomes = judge_all(&surface, which).await;
        let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
        assert_eq!(held, vec![false, true], "{which}: {}", saying(&outcomes));
    }
}

/// **The decision's lock reds under an invented key and the loan's does not.**
///
/// The other half of the pair, so each lock is shown to depend on its own key
/// and on nothing the other one reads.
#[tokio::test]
async fn the_decision_lock_reds_under_an_invented_key_and_the_loan_lock_holds() {
    for which in BOTH {
        let (_room, surface, sid) = furnished(which).await;
        with_the_two_things(&surface, &sid).await;
        as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({
                "subject": "thing:floor-pump",
                "content": "On loan; it has to go back.",
                "provenance": "testimony",
                "fields": {"runs_out": "2026-11-03"},
            }),
        )
        .await;
        as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({
                "subject": "event:wagstaff-fair",
                "content": "The tickets have to be sorted by a day.",
                "provenance": "testimony",
                "fields": {"needed_by": "2026-10-20"},
            }),
        )
        .await;
        let outcomes = judge_all(&surface, which).await;
        let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
        assert_eq!(held, vec![true, false], "{which}: {}", saying(&outcomes));
    }
}

/// 🚨 **No lock in either room rests on a needle that matches somewhere else.**
///
/// The reasoning is written where this was first built, in the year room's own
/// case. Run against a room worked under the shipped keys, so a needle that
/// matches nowhere is a lock failing or a walk that could not read the answer.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    for which in BOTH {
        let (_room, surface, sid) = furnished(which).await;
        worked_under_the_shipped_keys(&surface, &sid).await;
        let summary = jojobot_exercise::lock::needle_summary(
            &surface,
            &jojobot_exercise::lock::locks_of(which),
            &[],
        )
        .await;
        assert!(
            summary.nowhere.is_empty(),
            "{which}: a needle matched nowhere: {:?}",
            summary.nowhere,
        );
        assert!(
            summary.findings.is_empty(),
            "{which}: a lock rests on a needle that matches somewhere else: {:?}",
            summary.findings,
        );
    }
}
