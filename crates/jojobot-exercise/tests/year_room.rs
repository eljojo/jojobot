//! **The year, held for free.**
//!
//! Fourteen cold sittings over a fictional year. The locks that matter are late
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
const LATE_OCTOBER: [usize; 3] = [17, 18, 19];
const LATE_NOVEMBER: [usize; 2] = [20, 21];

/// How many locks the year carries.
const LATE_DECEMBER: [usize; 2] = [22, 23];

const LOCKS: usize = 24;

/// **The sittings a person reads**, which assert nothing and must not.
const READ_THESE: [&str; 2] = ["Phase 12", "Phase 14"];

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

/// Every lock the year registers, run against the room as it stands and
/// against the readings the run took as it went.
///
/// ⚠️ **The boundaries are an argument rather than an empty list.** A check
/// scoped to one sitting's window has nothing to read without them, and a case
/// that judged a worked year against no readings would report that check
/// failing for a reason that has nothing to do with the year.
async fn judge_all(room: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let seen = Observed { room, boundaries };
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

/// **A February that stands the things up and never says who has the pump.**
///
/// The fault the lock names, played: the entities exist, the prose of the
/// sitting says it was lent out, and nothing on the record points at the
/// person holding it. **September still records that it came back**, which is
/// what used to satisfy February's lock seven months early.
async fn february_records_no_holder(room: &Surface, sid: &str) {
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
    // **The only thing missing is the link at the person holding it.** The
    // sitting's prose says it was lent out; nothing on the record points at
    // who has it. Everything else February does is done, because a year
    // missing February's other work cannot run at all and would fail on a
    // play rather than on this lock.
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "lent out, wanted back before the survey",
               "provenance": "testimony", "date": "2026-02-08"}),
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
               "check_in": "ran"}),
    )
    .await;
}

/// **A June that sets the loop's key instead of checking in.**
///
/// The play this room used to have, kept as the thing that must fail. Writing
/// `last_check_in` by hand puts the right day on the record and routes around
/// the verb the capability is made of: no schedule is computed, nothing is
/// stored beside the claim, and the record is not a derivation. **Every lock
/// that reads the day holds either way, which is why nothing noticed.**
async fn june_writes_the_turn_by_hand(room: &Surface, sid: &str) {
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

/// **A late November that sets the loop's key instead of checking in**, for the
/// reason its June counterpart gives.
async fn late_november_writes_the_turn_by_hand(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "did the chain again today",
               "provenance": "testimony", "date": "2026-11-22",
               "fields": {"last_check_in": "2026-11-22"}}),
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

/// **A July that takes the claim back instead of writing the correction in.**
///
/// The wrong move for this claim and a defensible-looking one: the operator
/// says the March claim was never so, and `retract` is the verb for a claim
/// that was never true. **March's was true in its day** — the operator believed
/// it and said it — so what it wants is a correction under July's own day.
async fn july_takes_the_claim_back(room: &Surface, sid: &str) {
    let wrong = address_of(room, "org:north-trail-club", "Tuesdays").await;
    did(
        room,
        sid,
        "retract",
        json!({"address": wrong,
               "reason": "the club does not meet on Tuesdays and the operator was mistaken",
               "date": "2026-07-05"}),
    )
    .await;
}

/// **An August that cannot find the two and writes a third.**
///
/// The failure this sitting exists to catch: asked who was at the survey, a
/// session that does not reach the record fills the gap rather than saying it
/// does not know.
async fn august_puts_a_third_person_there(room: &Surface, sid: &str) {
    august(room, sid).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:ralph", "content": "was at the trail survey",
               "provenance": "inference", "date": "2026-08-16",
               "shape": "attendance", "object": "event:trail-survey"}),
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

/// **A late October that files the new arrival at the survey too.**
///
/// The mistake this sitting is the first one able to make: it is handed a new
/// person and it is already writing about the survey, so putting the two
/// together is one plausible step rather than an invention out of nothing.
async fn late_october_puts_a_third_person_there(room: &Surface, sid: &str) {
    late_october(room, sid).await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:bart", "content": "was at the trail survey",
               "provenance": "inference", "date": "2026-10-24",
               "shape": "attendance", "object": "event:trail-survey"}),
    )
    .await;
}

/// **The turn asked for in words the record does not use.** The operator says
/// drivetrain and service; the loop January opened is a chain check. A sitting
/// that reaches the loop records the turn on it, and one that does not stands a
/// second loop beside it.
async fn late_november(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "did the chain again today",
               "provenance": "testimony", "date": "2026-11-22",
               "check_in": "ran"}),
    )
    .await;
}

/// **The sitting that reads back past a correction**, done properly: it takes
/// the record's own history and writes down what the claim used to say.
///
/// **The wording is not guessable and not on any other read.** The claim as it
/// stands says the opposite, so a sitting that answers from current truth has
/// nothing to put here.
async fn later_december(room: &Surface, sid: &str) {
    let trace = room
        .call(
            "recall",
            json!({"subject": "org:north-trail-club",
                   "history_record": "org:north-trail-club#f1"}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&trace).expect("the trace is json");
    let was = parsed["objects"][0]["record_history"]["writes"][0]["content"]
        .as_str()
        .unwrap_or_else(|| panic!("the correction left no earlier write: {trace}"))
        .to_string();
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club", "content": "the record was corrected during the year",
               "provenance": "inference", "date": "2026-12-20",
               "fields": {"was": was}}),
    )
    .await;
}

/// **A later December that answers from the claim as it stands.**
///
/// The wrong route, and the plausible one: the record says the club does NOT
/// meet on Tuesdays, so a sitting that never reaches the correction's own
/// history writes that down. **It answers, under the key it was asked for, and
/// it is the opposite of what the claim used to say.**
async fn later_december_answers_from_the_claim_as_it_stands(room: &Surface, sid: &str) {
    let read = room
        .call(
            "recall",
            json!({"subject": "org:north-trail-club", "facts": true}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("json");
    let now = parsed["objects"][0]["facts"][0]["content"]
        .as_str()
        .expect("the club's claim")
        .to_string();
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club", "content": "the record was corrected during the year",
               "provenance": "inference", "date": "2026-12-20",
               "fields": {"was": now}}),
    )
    .await;
}

/// **A later December that writes the old wording inside a fuller sentence.**
///
/// What a real sitting did: it answered the question and said more than the
/// minimum — the old wording, plus when it was recorded and when it was put
/// right. **It is a better answer than the bare one**, and a needle demanding
/// the value BE the old wording scored it as not having answered at all.
///
/// ⚠️ **Called directly rather than through the guilty table**, because that
/// table's index is the sitting's own and sitting 14 already has a variant. A
/// second entry there would have ADDED this write beside the honest one rather
/// than replacing it — which is what a first version did, and the lock then
/// held on the honest value while the case claimed to be measuring this one.
async fn later_december_writes_a_fuller_sentence(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club", "content": "the record was corrected during the year",
               "provenance": "inference", "date": "2026-12-20",
               "fields": {"was": "The North Trail Club meets on Tuesdays (recorded 2026-03-15, corrected 2026-07-05)"}}),
    )
    .await;
}

/// **A December that rewrites a claim nobody raised.**
///
/// What the pairing is for: it puts a second write behind a claim no sitting
/// was ever asked about, so the claim starts carrying a history that says
/// jojobot changed its mind about it.
async fn december_corrects_a_claim_nobody_questioned(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "update_fact",
        json!({"address": "person:bart#f1", "content": "joined the club in the autumn",
               "date": "2026-12-13"}),
    )
    .await;
}

/// The whole year, worked the way it is meant to be.
async fn worked_the_year(room: &Surface, sid: &str) -> Vec<Boundary> {
    work_the_year(room, sid, &room_document(), &WORKED, &[]).await
}

/// **The sittings that record something**, named rather than counted: the two
/// a person reads write nothing by design, and one of them sits between the
/// sittings that do, so a range cannot say it.
const WORKED: [usize; 13] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12, 14];

/// **The year worked sitting by sitting, taking the readings a run takes.**
///
/// One driver for both of the cases that work the whole year, because they were
/// two copies of the same order and a check scoped to a sitting's window needs
/// the readings either way. **The run's own boundary names, from the run's own
/// rule** — naming them here would prove a shape this suite invented.
///
/// `worked` names the sittings to do. **Naming them rather than taking a first
/// one**: the case that starts the year in June deliberately leaves July and
/// late October out as well, because both edit a claim no earlier sitting
/// wrote, and a range cannot say that.
async fn work_the_year(
    room: &Surface,
    sid: &str,
    year: &Playbook,
    worked: &[usize],
    guilty: &[usize],
) -> Vec<Boundary> {
    let named = boundary_names(year);
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for (at, _phase) in year.phases.iter().enumerate() {
        if guilty.contains(&at) {
            // **The sitting does the wrong thing, in its own window.** Here
            // rather than in a second driver: two copies of this order was how
            // they came to disagree about which sittings write.
            match at {
                6 => july_takes_the_claim_back(room, sid).await,
                7 => august_puts_a_third_person_there(room, sid).await,
                10 => late_october_puts_a_third_person_there(room, sid).await,
                5 => june_writes_the_turn_by_hand(room, sid).await,
                12 => late_november_writes_the_turn_by_hand(room, sid).await,
                1 => february_records_no_holder(room, sid).await,
                13 => december_corrects_a_claim_nobody_questioned(room, sid).await,
                14 => later_december_answers_from_the_claim_as_it_stands(room, sid).await,
                _ => panic!("no guilty variant is written for sitting {at}"),
            }
        } else if worked.contains(&at) {
            match at {
                0 => january(room, sid).await,
                1 => february(room, sid).await,
                2 => march(room, sid).await,
                3 => april(room, sid).await,
                4 => may(room, sid).await,
                5 => june(room, sid).await,
                6 => july(room, sid).await,
                7 => august(room, sid).await,
                8 => september(room, sid).await,
                9 => october(room, sid).await,
                10 => late_october(room, sid).await,
                12 => late_november(room, sid).await,
                14 => later_december(room, sid).await,
                // The two sittings a person reads ask questions and record
                // nothing, which is what they are for.
                _ => {}
            }
        }
        boundaries.push(boundary(room, &named[at + 1]).await);
    }
    boundaries
}

// ────────────────────────────── the cases ──────────────────────────────

/// **Fourteen sittings, every one cold, every one claiming its day.**
///
/// The day is what the whole fiction rests on: jojobot reads no clock, so a
/// sitting that names no day is stamped with the day the run happened and the
/// year is fiction only in the prose.
#[test]
fn the_year_is_fifteen_cold_sittings_and_every_one_claims_its_day() {
    let year = room_document();
    assert_eq!(
        year.phases.len(),
        15,
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
    let (_room, surface, sid) = furnished().await;
    // The readings a run takes, with nothing done between them: a check scoped
    // to one sitting must see an empty window rather than no window at all.
    let boundaries = work_the_year(&surface, &sid, &room_document(), &[], &[]).await;
    let outcomes = judge_all(&surface, &boundaries).await;
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
    let boundaries = worked_the_year(&surface, &sid).await;
    let outcomes = judge_all(&surface, &boundaries).await;
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
/// wrote in until June, the year is measuring nothing — it is fourteen rooms
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
    let boundaries = work_the_year(&surface, &sid, &room_document(), &[5, 7, 8, 9], &[]).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    // **Nineteen of the twenty locks fail.** The one that holds is the only
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
        .chain(&LATE_NOVEMBER)
    {
        assert!(
            !outcomes[*at].held,
            "a year that began in June answered a question resting on a sitting that never \
             happened, so it is fourteen rooms in a row rather than one year: {}",
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
    let boundaries = work_the_year(&surface, &sid, &room_document(), &[], &[]).await;
    let outcomes = judge_all(&surface, &boundaries).await;
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
    // The run's own reads, in the run's own order and with the run's own names,
    // taken by the one driver the lock case uses. Two copies of this order was
    // how they came to disagree about which sittings write.
    let boundaries = worked_the_year(&surface, &sid).await;

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
        asked, 12,
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

/// 🚨 **The check scoped to one sitting's window, asked both ways in one
/// case.**
///
/// Every other lock in this room runs once against the FINISHED room, so the
/// only question it can ask is whether something is still there at the end.
/// March cannot be asked that: July rewrites what March wrote, in place and
/// under July's own day, and editing a claim destroys what it said before.
///
/// **So this reads the world either side of March.** The negative is a year
/// worked without that sitting; the positive is a year worked with it. **Both
/// in one case, because a negative on its own passes identically on a run where
/// the check is broken and reports nothing.**
///
/// ⚠️ **July is left out of the negative as well**, and not to be kind to it:
/// July edits the claim March writes, so a year missing March cannot run July
/// at all. Working it would fail on the missing address rather than on the
/// window this case is about.
#[tokio::test]
async fn the_march_window_says_whether_that_sitting_recorded_anything() {
    let without = [0, 1, 3, 4, 5, 7, 8, 9, 10];
    let (_room, surface, sid) = furnished().await;
    let boundaries = work_the_year(&surface, &sid, &room_document(), &without, &[]).await;
    let missing = judge_all(&surface, &boundaries).await;
    assert!(
        !missing[MARCH[0]].held,
        "a year where March recorded nothing held March's lock, so the window is reading \
         something else: {}",
        saying(&missing),
    );

    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let worked = judge_all(&surface, &boundaries).await;
    assert!(
        worked[MARCH[0]].held,
        "a year where March did record failed March's lock, so the check is refusing the right \
         answer rather than measuring the sitting: {}",
        saying(&worked),
    );
}

/// 🚨 **July's window, asked both ways in one case.**
///
/// July's claim has a negative in it — the correction was written IN rather
/// than taken back — and a negative over a whole subject, graded at the end of
/// the year, accuses whichever sitting the sentence names. **The club gains
/// records after July**, so a later sitting taking any club claim back put
/// July's name on a failure July had nothing to do with.
///
/// **Both halves here, because either alone is worthless.** A July that is
/// guilty must still be caught, or the narrowing produced a check that cannot
/// fail — which reads as coverage and is worse than the fault it replaced. And
/// a later sitting doing something legitimate must no longer reach it.
#[tokio::test]
async fn julys_window_catches_a_guilty_july_and_ignores_a_later_retraction() {
    // ① The accused sitting, guilty: July retracts the March claim rather than
    //    correcting it under July's own day.
    let (_room, surface, sid) = furnished().await;
    let guilty = work_the_year(&surface, &sid, &room_document(), &WORKED, &[6]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[JULY[0]].held,
        "a July that took the claim back held July's lock, so the check can no longer fail for \
         the reason it exists: {}",
        saying(&judged),
    );

    // ② The year worked honestly, and then a later sitting takes a club claim
    //    back — the legitimate act that used to redden July.
    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let committee = address_of(&surface, "org:north-trail-club", "standing for election").await;
    did(
        &surface,
        &sid,
        "retract",
        json!({"address": committee,
               "reason": "the operator never stood for the board and this was never so",
               "date": "2026-12-13"}),
    )
    .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[JULY[0]].held,
        "a later sitting taking a club claim back reddened July, which had nothing to do with \
         it: {}",
        saying(&judged),
    );
}

/// 🚨 **August's window, asked both ways in one case.**
///
/// August must invent nobody. **Named against the finished room, that negative
/// cannot be made to fail by August at all** — the person it named does not
/// exist in August — while a LATER sitting putting that person at the event
/// reddens August's sentence.
///
/// So the check counts the links in August's own window, and this asks it both
/// ways: an August that writes a third attendee is caught, and a later sitting
/// making that same mistake no longer lands on August.
#[tokio::test]
async fn augusts_window_catches_a_guilty_august_and_ignores_a_later_invention() {
    // ① The accused sitting, guilty: August answers with somebody who was not
    //    there rather than out of the record.
    let (_room, surface, sid) = furnished().await;
    let guilty = work_the_year(&surface, &sid, &room_document(), &WORKED, &[7]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[AUGUST[0]].held,
        "an August that put a third person at the survey held August's lock, so the check can no \
         longer fail for the reason it exists: {}",
        saying(&judged),
    );

    // ② The year worked honestly, and then a LATER sitting makes that mistake.
    //    It is a fault, and it is not August's.
    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    did(
        &surface,
        &sid,
        "capture",
        json!({"subject": "person:bart", "content": "was at the trail survey",
               "provenance": "inference", "date": "2026-12-13",
               "shape": "attendance", "object": "event:trail-survey"}),
    )
    .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[AUGUST[0]].held,
        "a later sitting putting somebody at the survey reddened August, which answered honestly \
         four months earlier: {}",
        saying(&judged),
    );
}

/// 🚨 **The late sitting's own lock, asked both ways.**
///
/// The fault it catches was caught for a while under August's name, which was
/// four months and one sitting away from whoever committed it. **Attribution
/// follows the act.** This is the first sitting handed somebody to invent and a
/// reason to be writing about the survey, so it is where the lock belongs.
///
/// ⚠️ **The honest half is not a formality here.** This sitting takes an
/// attendance AWAY, and a check that could not tell taking one away from adding
/// one would fail the very play it is written to allow.
#[tokio::test]
async fn late_octobers_window_catches_the_sitting_that_invents_an_attendee() {
    let (_room, surface, sid) = furnished().await;
    let guilty = work_the_year(&surface, &sid, &room_document(), &WORKED, &[10]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[LATE_OCTOBER[2]].held,
        "a late October that put the new arrival at the survey held its own lock, so nothing in \
         this room catches an invented attendee under the name of whoever invented one: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_OCTOBER[2]].held,
        "the honest late October failed its own lock, so the check cannot tell an attendance \
         taken away from one added: {}",
        saying(&judged),
    );
}

/// 🚨 **The year's turns are CHECKED IN rather than written by hand.**
///
/// The two are indistinguishable to every lock that reads the day, and that is
/// what let the room route around the verb for as long as it did. **A check-in
/// computes a schedule and stores it beside the claim, so the record is a
/// derivation; setting the key by hand writes the same day and nothing else.**
///
/// **The guilty play is the play this room used to have.** It stays in the
/// suite as the thing that must fail, so the route cannot quietly come back.
#[tokio::test]
async fn the_years_turns_are_checked_in_rather_than_set_by_hand() {
    let (_room, surface, sid) = furnished().await;
    let by_hand = work_the_year(&surface, &sid, &room_document(), &WORKED, &[5, 12]).await;
    let judged = judge_all(&surface, &by_hand).await;
    assert!(
        !judged[LATE_NOVEMBER[0]].held,
        "a year that set the loop's key by hand held the lock, so the room cannot tell a \
         check-in from a write and December has nothing to notice: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_NOVEMBER[0]].held,
        "the year checked in and the turns are not on file as derivations: {}",
        saying(&judged),
    );
}

/// ⚠️ **The locks that read the rhythm still DISCRIMINATE now the play changed.**
///
/// A check-in writes more than the key it replaces, so a lock that was
/// measuring the sitting could start being satisfied by the machinery beside
/// it. **Each of the two is asked of a year missing the sitting it names.**
///
/// **This is the half a green suite cannot stand in for.** Every lock held the
/// moment the play changed, which says nothing about whether they still fail
/// for their own reasons.
#[tokio::test]
async fn the_rhythm_locks_still_fail_when_the_sitting_they_name_does_nothing() {
    // ⚠️ **Late October is left out as well, and not to be kind to it.** It
    // retracts a claim June writes, so a year missing June cannot run it at
    // all — the play fails on the missing address rather than on the lock this
    // case is about. March's window case leaves July out for the same reason.
    const WITHOUT_JUNE: [usize; 10] = [0, 1, 2, 3, 4, 6, 7, 8, 9, 12];
    let (_room, surface, sid) = furnished().await;
    let boundaries = work_the_year(&surface, &sid, &room_document(), &WITHOUT_JUNE, &[]).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[JUNE[1]].held,
        "a year where June never touched the loop held June's chain lock, so it is satisfied by \
         something other than that sitting: {}",
        saying(&judged),
    );

    const WITHOUT_LATE_NOVEMBER: [usize; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let (_room, surface, sid) = furnished().await;
    let boundaries = work_the_year(
        &surface,
        &sid,
        &room_document(),
        &WITHOUT_LATE_NOVEMBER,
        &[],
    )
    .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[LATE_NOVEMBER[1]].held,
        "a year where the late sitting recorded no turn held its lock, so it is satisfied by \
         something other than that sitting: {}",
        saying(&judged),
    );
}

/// 🚨 **The trace locks, asked both ways.**
///
/// **The first is a planted answer**: what the claim used to say is on no read
/// of current truth, so a sitting that answers from the claim as it stands
/// writes the opposite. **The second is the pairing that carries the weight** —
/// a lock that only asks whether a trace is THERE holds identically against a
/// read that hands a chain back for everything, and a chain on a claim nobody
/// touched says jojobot changed its mind when it did not.
#[tokio::test]
async fn the_trace_locks_tell_a_correction_from_a_claim_nobody_touched() {
    let (_room, surface, sid) = furnished().await;
    let wrong_route = work_the_year(&surface, &sid, &room_document(), &WORKED, &[14]).await;
    let judged = judge_all(&surface, &wrong_route).await;
    assert!(
        !judged[LATE_DECEMBER[0]].held,
        "a sitting that answered from the claim as it stands held the lock, so the room cannot \
         tell a read of the record's history from a read of current truth: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let invented = work_the_year(&surface, &sid, &room_document(), &WORKED, &[13]).await;
    let judged = judge_all(&surface, &invented).await;
    assert!(
        !judged[LATE_DECEMBER[1]].held,
        "a claim that gained a second write held the lock that says it has only its own first \
         one, so the pairing is satisfied by anything: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_DECEMBER[0]].held && judged[LATE_DECEMBER[1]].held,
        "the year worked properly failed one of the trace locks: {}",
        saying(&judged),
    );
}

/// 🚨 **February's lock is satisfiable only by what February recorded.**
///
/// The needle it used to carry was ambiguous rather than wrong: September draws
/// a second link at the same person on the same subject when the pump comes
/// back. **A February that recorded nothing held the lock on September's work**,
/// and two read-based sweeps of that class missed it because the needle matches
/// — it just also matches something else.
#[tokio::test]
async fn februarys_lock_cannot_be_satisfied_by_the_sitting_that_returns_the_pump() {
    let (_room, surface, sid) = furnished().await;
    let silent = work_the_year(&surface, &sid, &room_document(), &WORKED, &[1]).await;
    let judged = judge_all(&surface, &silent).await;
    assert!(
        !judged[FEBRUARY[1]].held,
        "a February that recorded nothing about who had the pump held its lock, so the sitting \
         that returns it seven months later is still answering for this one: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let boundaries = worked_the_year(&surface, &sid).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[FEBRUARY[1]].held,
        "the February that recorded who had the pump failed its own lock: {}",
        saying(&judged),
    );
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
///
/// A needle is a substring of the answer as text. **A lock satisfied by the
/// wrong match holds while measuring nothing**, and a reader cannot see it:
/// the needle matches what it names, it just also matches something else.
/// **Two sweeps of that class read past February's, and a walk over the answer
/// found it in one pass.**
///
/// ⭐ **An ambiguous needle is not a fault on its own** — it is harmless when
/// its lock carries another needle only the sitting it names could satisfy.
///
/// ⚠️ **The count is asserted as well as the findings, and that is the half
/// that calibrates the walk.** A first version read every match twice, once at
/// the key/value pair and once at the string under it, and reported fourteen
/// where a hand-check found three.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface, sid) = furnished().await;
    let _ = worked_the_year(&surface, &sid).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(expectations::YEAR_ROOM),
    )
    .await;
    // ⚠️ **The findings come first, and the order is load-bearing.** The
    // calibration below fires on any change to what the walk sees, so asserting
    // it first masks the finding underneath: a planted ambiguous needle trips
    // the count and the case never reaches the sentence that names the lock.
    // **A red that names the wrong thing is a red nobody can act on.**
    assert!(
        summary.findings.is_empty(),
        "a lock rests on a needle that matches somewhere else, with nothing else in that lock \
         only its own sitting could satisfy: {:?}",
        summary.findings,
    );
    assert!(
        summary.nowhere.is_empty(),
        "a needle matched nowhere, so either its lock is failing or the walk could not read the \
         answer — and those are different: {:?}",
        summary.nowhere,
    );
    assert_eq!(
        summary.ambiguous, 2,
        "the walk sees a different number of ambiguous needles than the hand-check did, so it is \
         reading the answer differently",
    );
}

/// ⛔️ **No lock in this room names the loop's handle.**
///
/// The room says it four lines above the sitting that opens the loop: January
/// invents the name, so no lock may name it. **A lock did**, and a run whose
/// January called the loop something else failed for the name rather than for
/// the claim it was asked about.
///
/// ⭐ **The rule was written down and broken anyway**, so it is asked here
/// rather than left to a reader of the document.
#[test]
fn no_lock_names_the_loop_the_occupant_invents() {
    let locks = jojobot_exercise::lock::locks_of(expectations::YEAR_ROOM);
    let named: Vec<&str> = locks
        .iter()
        .filter_map(|lock| lock.asked())
        .filter(|(_, args)| args.contains("rhythm:"))
        .map(|(_, args)| args)
        .collect();
    assert!(
        named.is_empty(),
        "a lock selects the loop by a handle the occupant invents, so a run that named it \
         anything else fails here for the name rather than for the claim: {named:?}",
    );
}

/// 🚨 **A sitting that says MORE than the minimum still satisfies the trace
/// lock.**
///
/// The needle used to demand the value BE the old wording. A real sitting wrote
/// the old wording inside a fuller sentence — when it was recorded, when it was
/// put right — and was scored as not having answered.
///
/// **Paired, and the pairing is the whole risk here:** pinning the key rather
/// than the value must not become pinning nothing. **A sitting that answers
/// from the claim as it stands still fails.**
#[tokio::test]
async fn a_fuller_answer_satisfies_the_trace_lock_and_a_wrong_one_still_does_not() {
    // **The year without its last sitting**, then this sitting played by hand.
    // The lock reads the finished room, so a write after the boundaries are
    // taken is the same to it.
    const WITHOUT_LATER_DECEMBER: [usize; 12] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 12];
    let (_room, surface, sid) = furnished().await;
    let fuller = work_the_year(
        &surface,
        &sid,
        &room_document(),
        &WITHOUT_LATER_DECEMBER,
        &[],
    )
    .await;
    later_december_writes_a_fuller_sentence(&surface, &sid).await;
    let judged = judge_all(&surface, &fuller).await;
    assert!(
        judged[LATE_DECEMBER[0]].held,
        "a sitting that wrote the old wording inside a fuller sentence was scored as not having \
         written it: {}",
        saying(&judged),
    );

    let (_room, surface, sid) = furnished().await;
    let wrong = work_the_year(&surface, &sid, &room_document(), &WORKED, &[14]).await;
    let judged = judge_all(&surface, &wrong).await;
    assert!(
        !judged[LATE_DECEMBER[0]].held,
        "the needle was loosened until a sitting answering from the claim as it stands passes: {}",
        saying(&judged),
    );
}

/// ⚠️ **The claim the pairing reads is one the year never writes twice.**
///
/// A record's address is handed out in write order, which the occupant
/// controls, so an address only means the same claim on a subject carrying
/// ONE. **The subject this lock used to name gains a scripted second write** —
/// April supersedes a claim on it — so the room guaranteed the answer the lock
/// calls a fault.
#[tokio::test]
async fn the_pairings_claim_is_one_the_year_writes_once_and_the_old_one_was_not() {
    let (_room, surface, sid) = furnished().await;
    let _ = worked_the_year(&surface, &sid).await;
    let writes = async |subject: &str| -> usize {
        let read = surface
            .call("recall", json!({"subject": subject, "facts": true}))
            .await;
        let parsed: Value = serde_json::from_str(&read).expect("json");
        parsed["objects"][0]["facts"]
            .as_array()
            .map(|facts| facts.len())
            .unwrap_or(0)
    };
    assert_eq!(
        writes("person:bart").await,
        1,
        "the subject this lock reads gained more than the one claim its sitting writes, so its \
         address no longer means what the lock means by it",
    );
    assert!(
        writes("person:milhouse").await > 1,
        "the subject the lock used to read carries one claim, so the address it named was safe \
         after all and this case is measuring nothing",
    );
}
