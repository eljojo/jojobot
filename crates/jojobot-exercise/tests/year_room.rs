//! **The year, held for free.**
//!
//! Thirteen cold sittings over a fictional year. The locks that matter are late
//! and the work that earns them is early, so the case that says the year
//! measures anything is the one that works only its second half: a store that
//! nobody wrote in until June cannot answer the questions September and October
//! ask.
//!
//! Both kinds of case, named apart: the ones that prove the locks discriminate,
//! and the one that proves the year is solvable at all. A year whose own
//! arithmetic is wrong is unreachable by every session that will ever enter it,
//! every discriminating case still passes, and the failure surfaces on a paid
//! run reading as a defect in the product.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Observed, Outcome, boundary, boundary_names, days_claimed, uncovered_phases,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// **The locks the year carries, in the order its document writes them.**
///
/// The names are this file's rather than the document's: what is pinned is the
/// order and what each lock is about, so a sentence rewritten in the room does
/// not break a case here.
const JANUARY: [usize; 3] = [0, 1, 2];
const FEBRUARY: [usize; 3] = [3, 4, 5];
const MARCH: [usize; 1] = [6];
const APRIL: [usize; 2] = [7, 8];
const MAY: [usize; 1] = [9];
const JUNE: [usize; 2] = [10, 11];
const JULY: [usize; 1] = [12];
const AUGUST: [usize; 2] = [13, 14];
const SEPTEMBER: [usize; 1] = [15];
const OCTOBER: [usize; 1] = [16];
const LATE_OCTOBER: [usize; 2] = [17, 18];

/// How many locks the year carries.
const LOCKS: usize = 19;

/// **The sittings a person reads**, which assert nothing and must not.
const READ_THESE: [&str; 2] = ["Phase 12", "Phase 13"];

/// The document a run is driven by.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::YEAR_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as an occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::YEAR_ROOM)
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

/// A call an occupant would make.
async fn did(room: &Surface, sid: &str, verb: &str, mut args: Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// Every lock the year registers, run against the room as it stands.
async fn judge_all(room: &Surface) -> Vec<Outcome> {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let checks = expectations::for_playbook(expectations::YEAR_ROOM).expect("the year asserts");
    let mut outcomes = Vec::new();
    for check in checks {
        outcomes.push(check.check(&seen).await);
    }
    assert!(
        !outcomes.is_empty(),
        "the year registered no locks, so a run would report a pass over an empty list",
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

/// The address of the record on `subject` whose content carries `needle`.
async fn address_of(room: &Surface, subject: &str, needle: &str) -> String {
    let read = room
        .call("recall", json!({"subject": subject, "facts": true}))
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"][0]["facts"]
        .as_array()
        .expect("the records behind the fields")
        .iter()
        .find(|fact| {
            fact["content"]
                .as_str()
                .is_some_and(|said| said.contains(needle))
        })
        .and_then(|fact| fact["address"].as_str())
        .unwrap_or_else(|| panic!("no record on {subject} says {needle:?}: {read}"))
        .to_string()
}

// ── the twelve sittings, each done the way a session that read the surface
//    would do it, and each carrying the day the document says it happens on ──

async fn january(room: &Surface, sid: &str) {
    did(room, sid, "read_mailbox", json!({})).await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "rhythm", "handle": "chain-check", "name": "Chain check",
               "source": "the operator", "parent": "thing:gravel-bike"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "look at the bike chain every ninety days",
               "provenance": "testimony", "date": "2026-01-12",
               "fields": {"name": "Chain check", "last_check_in": "2025-12-20",
                          "cadence_days": "90", "counts_from": "2025-12-20",
                          "advances_from": "due_date"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:milhouse", "content": "lives in Springfield",
               "provenance": "testimony", "date": "2026-01-12",
               "shape": "location", "object": "place:springfield"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:milhouse", "content": "rides with the club",
               "provenance": "testimony", "date": "2026-01-12",
               "shape": "membership", "object": "org:north-trail-club"}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "trail-survey", "name": "The trail survey",
               "source": "the operator"}),
    )
    .await;
    // **The brief asked for this in the operator's own words** — leave it so
    // whoever picks it up in a month has what they need — so a sitting that
    // writes a note for the next one is doing what it was told.
    did(
        room,
        sid,
        "post_message",
        json!({"to": "assistant", "subject": "where the year stands",
               "body": "The bike chain loop is on file and the club is on the roster."}),
    )
    .await;
}

async fn february(room: &Surface, sid: &str) {
    for (kind, handle, name) in [
        ("person", "ralph", "Ralph"),
        ("person", "nelson", "Nelson"),
        ("thing", "floor-pump", "The Floor Pump"),
    ] {
        did(
            room,
            sid,
            "add_entity",
            json!({"kind": kind, "handle": handle, "name": name, "source": "the operator"}),
        )
        .await;
    }
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "lent out, wanted back before the survey",
               "provenance": "testimony", "date": "2026-02-08",
               "shape": "connection", "object": "person:ralph"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:nelson", "content": "joined the club",
               "provenance": "testimony", "date": "2026-02-08",
               "shape": "membership", "object": "org:north-trail-club"}),
    )
    .await;
}

async fn march(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club", "content": "meets on Tuesdays",
               "provenance": "testimony", "date": "2026-03-15"}),
    )
    .await;
}

async fn april(room: &Surface, sid: &str) {
    let was = address_of(room, "person:milhouse", "Springfield").await;
    did(
        room,
        sid,
        "update_fact",
        json!({"address": was, "status": "superseded"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:milhouse", "content": "moved to Shelbyville",
               "provenance": "testimony", "date": "2026-04-19",
               "shape": "location", "object": "place:shelbyville"}),
    )
    .await;
}

async fn may(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "place:north-trail", "content": "washed out at the top end this spring",
               "provenance": "testimony", "date": "2026-05-10"}),
    )
    .await;
}

async fn june(room: &Surface, sid: &str) {
    for who in ["person:milhouse", "person:nelson"] {
        did(
            room,
            sid,
            "capture",
            json!({"subject": who, "content": "was at the trail survey",
                   "provenance": "testimony", "date": "2026-06-14",
                   "shape": "attendance", "object": "event:trail-survey"}),
        )
        .await;
    }
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "did the bike chain this morning",
               "provenance": "testimony", "date": "2026-06-14",
               "fields": {"last_check_in": "2026-06-14"}}),
    )
    .await;
}

async fn july(room: &Surface, sid: &str) {
    let wrong = address_of(room, "org:north-trail-club", "Tuesdays").await;
    did(
        room,
        sid,
        "update_fact",
        json!({"address": wrong,
               "content": "The North Trail Club does not meet on Tuesdays — the operator was mistaken in March; that never stood.",
               "date": "2026-07-05"}),
    )
    .await;
}

async fn august(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club", "content": "the operator is standing for election to the club board",
               "provenance": "testimony", "date": "2026-08-16"}),
    )
    .await;
}

async fn september(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "came back at the survey",
               "provenance": "testimony", "date": "2026-06-14",
               "shape": "connection", "object": "person:ralph"}),
    )
    .await;
}

async fn october(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "brought round over the summer",
               "provenance": "testimony", "date": "2026-10-11",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
}

/// **The retraction case, worked for the first time all year.** Nelson's
/// survey attendance was never true, and the record it corrects is a June
/// fact that only exists because January stood up the event and February
/// stood up Nelson — the dependency the room is built to test. Bart's
/// membership is folded in beside it, the same shape February used for
/// Nelson himself.
async fn late_october(room: &Surface, sid: &str) {
    let wrong = address_of(room, "person:nelson", "trail survey").await;
    did(
        room,
        sid,
        "retract",
        json!({"address": wrong,
               "reason": "Nelson never actually made it to the survey — he was fixing a flat that morning",
               "date": "2026-10-24"}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "person", "handle": "bart", "name": "Bart", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:bart", "content": "joined the club",
               "provenance": "testimony", "date": "2026-10-24",
               "shape": "membership", "object": "org:north-trail-club"}),
    )
    .await;
}

/// The whole year, worked the way it is meant to be.
async fn worked_the_year(room: &Surface, sid: &str) {
    january(room, sid).await;
    february(room, sid).await;
    march(room, sid).await;
    april(room, sid).await;
    may(room, sid).await;
    june(room, sid).await;
    july(room, sid).await;
    august(room, sid).await;
    september(room, sid).await;
    october(room, sid).await;
    late_october(room, sid).await;
}

// ────────────────────────────── the cases ──────────────────────────────

/// **Thirteen sittings, every one cold, every one claiming its day.**
///
/// The day is what the whole fiction rests on: jojobot reads no clock, so a
/// sitting that names no day is stamped with the day the run happened and the
/// year is fiction only in the prose.
#[test]
fn the_year_is_thirteen_cold_sittings_and_every_one_claims_its_day() {
    let year = room_document();
    assert_eq!(
        year.phases.len(),
        13,
        "the year carries one extra sitting, in October: {:?}",
        year.phases.iter().map(|p| &p.name).collect::<Vec<_>>(),
    );
    let mut days: Vec<&str> = Vec::new();
    for phase in &year.phases {
        assert!(
            phase.fresh_session,
            "{} carries the sitting before it, so the store is optional in it",
            phase.name,
        );
        let day = phase
            .day
            .as_deref()
            .unwrap_or_else(|| panic!("{} claims no day", phase.name));
        assert!(
            phase.prompt.contains(&day[..4]),
            "{} claims {day} where the harness reads and does not say a date where the occupant \
             does, so nothing carries it into a call: {:?}",
            phase.name,
            phase.prompt,
        );
        days.push(day);
    }
    let mut sorted = days.clone();
    sorted.sort_unstable();
    assert_eq!(days, sorted, "the year does not run forwards: {days:?}");
    sorted.dedup();
    assert_eq!(
        sorted.len(),
        days.len(),
        "two sittings claim the same day, so neither generated assertion can say which one \
         wrote: {days:?}",
    );
}

/// **The entries name no verb the room serves.**
///
/// A room gives the player every fact the task needs and never the move.
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
        let said = phase.prompt.to_lowercase();
        for pointed in ["mail", "box", "message", "waiting"] {
            assert!(
                !said.contains(pointed),
                "{} points the occupant at its mail, so the first lock opens itself: {:?}",
                phase.name,
                phase.prompt,
            );
        }
    }
}

/// **The two sittings a person must read are marked, and nothing asserts over
/// them. Every other sitting is asserted over.**
///
/// Both halves. A shape landing in READ is not a failure of the format; a
/// shape landing in READ that the format then buries is one, and a sitting
/// nobody wrote a lock for that is NOT marked to be read is a sitting that
/// silently measures nothing.
#[test]
fn the_sittings_a_person_reads_are_marked_and_every_other_one_is_locked() {
    let year = room_document();
    let checks = expectations::for_playbook(expectations::YEAR_ROOM).expect("the year asserts");
    assert_eq!(
        checks.len(),
        LOCKS,
        "the year carries {} locks and the roll-call above names {LOCKS}",
        checks.len(),
    );

    let unasserted = uncovered_phases(&year, &checks);
    let read_this: Vec<&str> = year
        .phases
        .iter()
        .filter(|p| p.read_this)
        .map(|p| p.name.as_str())
        .collect();
    assert_eq!(
        read_this.len(),
        READ_THESE.len(),
        "the sittings marked to be read are {read_this:?}",
    );
    for key in READ_THESE {
        assert!(
            read_this.iter().any(|name| name.starts_with(key)),
            "{key} is not marked to be read: {read_this:?}",
        );
        assert!(
            unasserted.iter().any(|name| name.starts_with(key)),
            "{key} is marked to be read and a lock asserts over it anyway: {unasserted:?}",
        );
    }
    assert_eq!(
        unasserted.len(),
        READ_THESE.len(),
        "a sitting asserts nothing and is not marked to be read, so it measures nothing and \
         says nothing: {unasserted:?}",
    );
}

/// **A year nobody worked in fails every lock.**
///
/// The room is furnished with five nouns and a brief, so nothing any lock
/// claims is true of it — which is what makes the case that follows mean
/// something.
#[tokio::test]
async fn a_year_nobody_worked_in_fails_every_lock() {
    let (_room, surface, _sid) = furnished().await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a furnished year nobody worked in held a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The year is solvable, and this is the case that says so.**
///
/// A different kind of case from the ones above: those prove the locks
/// discriminate between stores somebody put state into by hand. This one proves
/// every question the year asks CAN be answered through the served surface.
///
/// Without it a year whose own arithmetic is wrong would be unreachable by
/// every session that ever enters it, every case above would still pass, and
/// the failure would surface on a paid run reading as a defect in the product.
#[tokio::test]
async fn every_lock_holds_once_the_year_is_worked() {
    let (_room, surface, sid) = furnished().await;
    worked_the_year(&surface, &sid).await;
    let outcomes = judge_all(&surface).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a year worked the way it is meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// 🚨 **The sabotage that says the year measures anything: start it in June.**
///
/// If the second half of the year reads much the same against a store nobody
/// wrote in until June, the year is measuring nothing — it is thirteen rooms
/// in a row rather than one year.
///
/// **Both halves.** The early sittings' locks fail because that work never
/// happened, and the LATE ones fail too — September, October and the sitting
/// after it ask about things February and June created, and no amount of
/// working the second half well can answer them.
///
/// **Late October is not called at all**, the same way July never is: both
/// need an address `recall` would have to find first, and neither address
/// exists in a store that skipped the sitting that wrote it. Not calling them
/// is the honest version of the same failure their locks report on their own.
#[tokio::test]
async fn a_year_that_skipped_its_first_half_cannot_answer_its_second_half() {
    let (_room, surface, sid) = furnished().await;
    // June onwards, done as well as a session can do it against a store that
    // holds nothing any of it refers to.
    june(&surface, &sid).await;
    august(&surface, &sid).await;
    september(&surface, &sid).await;
    october(&surface, &sid).await;

    let outcomes = judge_all(&surface).await;
    // **Eighteen of the nineteen locks fail.** The one that holds is the only
    // claim in the year that rests on nothing before it — August files the
    // committee note against a club that came with the furniture.
    let stands_alone = AUGUST[1];
    for at in JANUARY
        .iter()
        .chain(&FEBRUARY)
        .chain(&MARCH)
        .chain(&APRIL)
        .chain(&MAY)
        .chain(&JUNE)
        .chain(&JULY)
        .chain(&[AUGUST[0]])
        .chain(&SEPTEMBER)
        .chain(&OCTOBER)
        .chain(&LATE_OCTOBER)
    {
        assert!(
            !outcomes[*at].held,
            "a year that began in June answered a question resting on a sitting that never \
             happened, so it is thirteen rooms in a row rather than one year: {}",
            saying(&outcomes),
        );
    }
    // The positive half: this is not a store where every write failed. The one
    // sitting that needed nothing before it did land.
    assert!(
        outcomes[stands_alone].held,
        "not one lock held, so the case above passes on a run where nothing was written at \
         all: {}",
        saying(&outcomes),
    );
}

/// **A sitting that recorded the year in prose leaves the late questions
/// unanswerable**, which is the whole reason the shapes were chosen.
///
/// Every claim is written down, in sentences, and nothing carries a key, an
/// edge or a loop. A reader could answer every question from the prose; a cold
/// session cannot.
#[tokio::test]
async fn the_locks_fail_on_a_year_written_entirely_in_prose() {
    let (_room, surface, sid) = furnished().await;
    did(&surface, &sid, "read_mailbox", json!({})).await;
    for (subject, said, day) in [
        (
            "person:milhouse",
            "he lives in Springfield and rides with the north trail club",
            "2026-01-12",
        ),
        (
            "thing:gravel-bike",
            "the chain wants looking at every ninety days, last done 2025-12-20",
            "2026-01-12",
        ),
        (
            "org:north-trail-club",
            "Ralph has the floor pump on loan and Nelson has joined",
            "2026-02-08",
        ),
    ] {
        did(
            &surface,
            &sid,
            "capture",
            json!({"subject": subject, "content": said,
                   "provenance": "testimony", "date": day}),
        )
        .await;
    }
    let outcomes = judge_all(&surface).await;
    for at in JANUARY[1..].iter().chain(&FEBRUARY).chain(&JUNE) {
        assert!(
            !outcomes[*at].held,
            "a year written in sentences held a lock that needs a key, an edge or a loop: {}",
            saying(&outcomes),
        );
    }
    // The positive that stops the assertion above passing on a store that lost
    // everything: the brief was taken, so this session did reach the room.
    assert!(
        outcomes[JANUARY[0]].held,
        "the box was never opened, so nothing here says the session got as far as the room: {}",
        saying(&outcomes),
    );
}

/// The year's own furniture avoids every day a sitting claims, so no generated
/// date assertion can hold on the furniture alone.
#[test]
fn no_furniture_is_dated_on_a_day_a_sitting_claims() {
    let year = room_document();
    let world = std::fs::read_to_string(expectations::room_document(expectations::YEAR_ROOM))
        .expect("the room reads");
    let block: String = world
        .lines()
        .skip_while(|l| l.trim() != "```world")
        .take_while(|l| l.trim() != "```" || l.trim() == "```world")
        .collect();
    for phase in &year.phases {
        let day = phase.day.as_deref().expect("every sitting claims a day");
        assert!(
            !block.contains(day),
            "{} claims {day} and the room is furnished with a record already carrying it, so \
             that sitting's generated assertion would hold whatever it did",
            phase.name,
        );
    }
}

/// 🚨 **The year is winnable — the half a lock suite cannot show.**
///
/// A run asserts more than the document's locks: it generates one date
/// assertion per dated sitting, and its verdict also needs the room to have
/// changed. **Nothing had ever shown those satisfiable**, so a failure on the
/// first paid run would have been ambiguous between jojobot falling short and
/// the year being unwinnable by anybody.
///
/// This drives the year the way a run does — a boundary read before anything,
/// then one after each sitting, named for the sitting that comes next — and
/// asks the generated assertions the same question the run asks them.
#[tokio::test]
async fn every_assertion_a_run_makes_holds_once_the_year_is_worked() {
    let (_room, surface, sid) = furnished().await;
    let year = room_document();
    // The run's own reads, in the run's own order and with the run's own names.
    // **The run's own names, from the run's own rule.** Naming them here would
    // prove a shape this case invented rather than the one a run takes.
    let named = boundary_names(&year);
    let mut boundaries = vec![boundary(&surface, &named[0]).await];
    for (at, _phase) in year.phases.iter().enumerate() {
        match at {
            0 => january(&surface, &sid).await,
            1 => february(&surface, &sid).await,
            2 => march(&surface, &sid).await,
            3 => april(&surface, &sid).await,
            4 => may(&surface, &sid).await,
            5 => june(&surface, &sid).await,
            6 => july(&surface, &sid).await,
            7 => august(&surface, &sid).await,
            8 => september(&surface, &sid).await,
            9 => october(&surface, &sid).await,
            10 => late_october(&surface, &sid).await,
            // The two sittings a person reads ask questions and record
            // nothing, which is what they are for.
            _ => {}
        }
        boundaries.push(boundary(&surface, &named[at + 1]).await);
    }

    let changed = boundaries[0].world != boundaries[boundaries.len() - 1].world;
    assert!(
        changed,
        "the room is exactly as it was furnished, so a run of this year could not report a pass",
    );

    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    // **Every dated sitting the room asserts a day over.** July rewrites the
    // March claim with `update_fact` and late October retracts a June one with
    // `retract` — both verbs take a date now, so both carry the day the
    // operator names rather than the day the run happened. September is the
    // one sitting that writes about an earlier day, and the room says so.
    let mut missed = Vec::new();
    let mut asked = 0;
    for dated in days_claimed(&year) {
        asked += 1;
        let outcome = dated.check(&seen).await;
        if !outcome.held {
            missed.push(outcome.saying);
        }
    }
    assert_eq!(
        asked, 10,
        "the year generates one assertion per dated sitting, less the two a person reads and \
         September, which writes about the day the pump came back — and this asked about \
         {asked}",
    );
    assert!(
        missed.is_empty(),
        "a run of this year would fail {} of its generated date assertions, so it is not \
         winnable by anybody: {}",
        missed.len(),
        missed.join("\n  "),
    );
}
