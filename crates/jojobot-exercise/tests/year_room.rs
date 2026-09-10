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
const JUNE: [usize; 3] = [10, 11, 12];
const JULY: [usize; 1] = [13];
const AUGUST: [usize; 2] = [14, 15];
const SEPTEMBER: [usize; 1] = [16];
const OCTOBER: [usize; 2] = [17, 18];
const LATE_OCTOBER: [usize; 4] = [19, 20, 21, 22];
const LATE_NOVEMBER: [usize; 2] = [23, 24];

/// How many locks the year carries.
const LATE_DECEMBER: [usize; 2] = [25, 26];

const LOCKS: usize = 27;

/// **The sittings a person reads**, which assert nothing and must not.
const READ_THESE: [&str; 2] = ["Phase 12", "Phase 14"];

/// The document a run is driven by.
fn room_document() -> Playbook {
    let path = expectations::room_document(expectations::YEAR_ROOM);
    Playbook::read(&path).unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a handle to write into it
/// as an occupant would.
/// ⛔️ **It boots nothing.** A sitting is a run of its own and brings its own
/// session, in its own day. A session minted here would be a run in the day the
/// server is actually having — months after the year's first sitting — and the
/// sweep answering in January's frame would see a beat from its own future and
/// keep offering it.
async fn furnished() -> (Room, Surface) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::YEAR_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    (room, surface)
}

/// **A session for one sitting, in the day that sitting is in.**
///
/// 🚨 **Each sitting is a RUN OF ITS OWN.** The year used to thread one session
/// through fifteen phases and name the day on every write; a sitting is a run
/// on its own day, and the day belongs to the run rather than to each call.
///
/// ⭐ **No `resume` is sent, and that is an assertion rather than an omission.**
/// A boot meeting a run still in flight hands back the resume-or-new choice and
/// no handle. Each sitting here states a day weeks after the last one, so every
/// earlier run has gone quiet in the frame the sweep answers in — **and a
/// handle coming straight back is the sweep having worked.** A choice arriving
/// instead means it did not, and the panic says which runs were offered.
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

    // 🚨 **A choice here is the SWEEP HAVING WORKED, and it is asserted rather
    // than answered blind.** The sitting before this one stopped without
    // wrapping — which is what a sitting does — so booting a day later finds it
    // gone quiet in this run's frame and offers it back.
    //
    // ⛔️ **An `active` run in that list is the sweep NOT having worked**: it
    // means the earlier run still reads as working, which is what a year acted
    // out in minutes looks like when nobody states a day.
    let offered = booted["session"]["choices"]
        .as_array()
        .unwrap_or_else(|| panic!("the boot on {day} handed back neither a handle nor a choice"));
    let working: Vec<&Value> = offered
        .iter()
        .filter(|run| run["state"] == "active")
        .collect();
    assert!(
        working.is_empty(),
        "the boot on {day} was offered a run still ACTIVE, so an earlier sitting did not go \
         quiet in this run's frame and the sweep answered on the clock instead: {working:?}",
    );

    // **A new run, deliberately.** Each sitting is its own; resuming the one
    // before it would make the year one long session again by another route.
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

/// **What June's survey claim says, in the four shapes the year can be driven
/// with.**
///
/// They differ in one thing: what the claim's own words POINT AT. Everything
/// else the sitting does is the same under all of them, so a case that moves
/// the lock about handles is measuring the sentence rather than a sitting that
/// did less.
///
/// ⚠️ **One line each, and that is the point of them being constants.** A
/// wrapped literal whose continuation is lost stores runs of spaces
/// mid-sentence, every check on it still passes, and the fault reaches a
/// reader — five instances of it across three crates so far.
const WHAT_JUNE_SAW: &str =
    "@event:trail-survey ran on @place:north-trail with @person:milhouse and @person:nelson";

/// The three nouns as words, which is what a sitting that never reached for a
/// mention leaves behind.
const IN_WORDS: &str = "the trail survey ran on the north trail with Milhouse and Nelson";

/// **Two kinds on one record and not the three the lock used to name.** The
/// event is the claim's own topic and is spelled rather than pointed at; the
/// place and the people are pointers, so the claim leads somewhere in two
/// directions.
const A_PLACE_AND_A_PERSON: &str =
    "the survey ran on @place:north-trail with @person:milhouse and @person:nelson";

/// **Pointers, all of one kind.** Every mention leads to a person, so no claim
/// links a thing of one kind to a thing of another.
const PEOPLE_ONLY: &str = "@person:milhouse and @person:nelson were both at the survey";

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

/// **The one entity of `kind`'s current handle.** Looked up rather than
/// hardcoded: October may have renamed it by the time this is asked, and a
/// literal written here would go stale under exactly the rename this room
/// now weaves in.
async fn handle_of_kind(room: &Surface, kind: &str) -> String {
    let read = room.call("recall", json!({"kind": kind})).await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"][0]["id"]
        .as_str()
        .unwrap_or_else(|| panic!("no {kind} is on the board: {read}"))
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
    // **The loop is declared, then opened by a back-dated check-in.** The
    // cadence and the policy are the operator's word and no check-in can state
    // them. The basis is not sent: the chain was last done before this sitting
    // opened the loop, so the opening turn is a check-in dated that day and
    // jojobot derives `counts_from` from it. Sending the basis by hand is what
    // this room's own lock on derivations exists to catch.
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "look at the bike chain every ninety days",
               "provenance": "testimony",
               "fields": {"name": "Chain check", "cadence_days": "90",
                          "advances_from": "due_date"}}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "last did the chain just before christmas",
               "provenance": "testimony",
               "check_in": "ran", "recorded_at": "2025-12-20"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:milhouse", "content": "lives in Springfield",
               "provenance": "testimony",
               "shape": "location", "object": "place:springfield"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:milhouse", "content": "rides with the club",
               "provenance": "testimony",
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
               "provenance": "testimony",
               "shape": "connection", "object": "person:ralph"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:nelson", "content": "joined the club",
               "provenance": "testimony",
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
               "provenance": "testimony", "recorded_at": "2026-02-08"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:nelson", "content": "joined the club",
               "provenance": "testimony",
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
               "provenance": "testimony", "recorded_at": "2026-03-15"}),
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
               "provenance": "testimony",
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
               "provenance": "testimony", "recorded_at": "2026-05-10"}),
    )
    .await;
}

async fn june(room: &Surface, sid: &str) {
    june_saying(room, sid, WHAT_JUNE_SAW).await;
}

/// **June, with the survey claim saying `said`.**
///
/// One body for every June the suite drives, because the sittings differ in one
/// sentence and separate bodies for them are copies of the same twenty lines.
/// The attendance
/// edges are drawn and the loop is checked in whatever the claim says, so every
/// other June lock holds under all of them — which is what makes a case over
/// the lock about handles a case about the sentence rather than about a sitting
/// that did less.
///
/// The claim is filed on the club because the good version of it names the
/// event, and a claim cannot mention the thing it is already filed against.
async fn june_saying(room: &Surface, sid: &str, said: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "org:north-trail-club",
               "content": said,
               "provenance": "testimony"}),
    )
    .await;
    for who in ["person:milhouse", "person:nelson"] {
        did(
            room,
            sid,
            "capture",
            json!({"subject": who, "content": "was at the trail survey",
                   "provenance": "testimony",
                   "shape": "attendance", "object": "event:trail-survey"}),
        )
        .await;
    }
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "did the bike chain this morning",
               "provenance": "testimony",
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
                   "provenance": "testimony",
                   "shape": "attendance", "object": "event:trail-survey"}),
        )
        .await;
    }
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:chain-check", "content": "did the bike chain this morning",
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
               "fields": {"last_check_in": "2026-11-22"}}),
    )
    .await;
}

/// **A late November that reaches for the operator's own words, finds
/// nothing under January's handle, and stands up a second loop instead of
/// the first.**
///
/// The room's own document names this failure at Phase 13: the store then
/// holds two loops, each with half the history, and neither can say when the
/// chain was last done. This one is also written by hand rather than checked
/// in, so the second loop carries the same defect June and this sitting's
/// other guilty variant do — a fixture giving the derivations lock's own
/// scoping something to be wrong about, since January's loop is left
/// untouched and entirely clean.
async fn late_november_stands_up_a_second_loop(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "rhythm", "handle": "orphan", "name": "Drivetrain service",
               "source": "the operator", "parent": "thing:gravel-bike"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "rhythm:orphan", "content": "did the drivetrain again today",
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
               "recorded_at": "2026-07-05"}),
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
               "recorded_at": "2026-07-05"}),
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
               "provenance": "inference",
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
               "provenance": "testimony", "recorded_at": "2026-08-16"}),
    )
    .await;
}

async fn september(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        // ⚠️ **The one write in the year about a day that is not the
        // sitting's.** Every other claim here is recorded on the day it is
        // made, which the run's own frame now supplies. This one says WHEN
        // THE THING HAPPENED — the pump came back at the June survey — and
        // that is a different field from when the record was made.
        json!({"subject": "thing:floor-pump", "content": "came back at the survey",
                   "provenance": "testimony", "happened_at": "2026-06-14",
                   "shape": "connection", "object": "person:ralph"}),
    )
    .await;
}

/// **A September that thinks it is June.**
///
/// The fault the day assertion exists for, and the one the exemption used to
/// hide: a sitting told in prose that it is September, whose writes land under
/// some other day. **Nothing else in the run fails** — the prose is in period,
/// the claim is right, and the year quietly collapses.
///
/// ⚠️ **It is played by booting in the wrong day rather than by naming one on
/// the write.** A run's day is where a claim's day now comes from, so sitting
/// in the wrong day IS the fault; naming a day on the call is a different
/// thing and one the surface is about to refuse.
async fn september_sits_in_the_wrong_day(room: &Surface, _sid: &str) {
    let confused = sitting(room, "2026-06-14").await;
    did(
        room,
        &confused,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "came back at the survey",
               "provenance": "testimony", "happened_at": "2026-06-14",
               "shape": "connection", "object": "person:ralph"}),
    )
    .await;
}

async fn october(room: &Surface, sid: &str) {
    october_writes_the_pump_and_the_place(room, sid).await;
    // **The reason is given in words; the rename is the move it was given
    // for.** The operator never names the verb, and finding it is the
    // measurement — the same posture this room takes with every walk it
    // asks for elsewhere.
    did(
        room,
        sid,
        "rename_entity",
        json!({"handle": "event:trail-survey", "to": "event:erosion-review"}),
    )
    .await;
}

/// **October, minus the one move its reason was given for.**
///
/// Everything else this sitting is asked to do is done — the pump's account
/// and the survey's place are both on the record — so a run built on this
/// fails only the lock that watches for the rename, never a lock that would
/// fail for an unrelated reason.
async fn october_without_renaming_the_survey(room: &Surface, sid: &str) {
    october_writes_the_pump_and_the_place(room, sid).await;
}

async fn october_writes_the_pump_and_the_place(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "brought round over the summer",
               "provenance": "testimony",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
    // **The note lands on the place, which this sitting had to find.** The
    // operator says where we held the survey and never says where that was;
    // June's claim is the one record that answers it.
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:trail-survey",
               "content": "the ground needs a look before next year",
               "provenance": "testimony",
               "shape": "location", "object": "place:north-trail"}),
    )
    .await;
}

/// **An October that picks a winner between the two accounts of how the pump
/// came back, instead of leaving both to stand.**
///
/// The pump lock's own comment says what is locked is the WRITE: neither
/// account quietly replacing the other. This is that replacement, played —
/// September's account is retracted rather than left standing, in the same
/// sitting that records Nelson's. The location note is still filed exactly as
/// the honest October files it, so this targets one lock and not two.
async fn october_removes_one_of_the_two_accounts(room: &Surface, sid: &str) {
    let ralphs = address_of(room, "thing:floor-pump", "came back at the survey").await;
    did(
        room,
        sid,
        "retract",
        json!({"address": ralphs,
               "reason": "Nelson brought it back, not Ralph",
               "recorded_at": "2026-10-11"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "brought round over the summer",
               "provenance": "testimony",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:trail-survey",
               "content": "the ground needs a look before next year",
               "provenance": "testimony",
               "shape": "location", "object": "place:north-trail"}),
    )
    .await;
}

/// **An October that writes the note and never says where.**
///
/// The sitting does everything else it is asked and files the note on the event
/// itself, so the pump lock still holds and the note is really there. **What it
/// never did is work out WHERE the survey was held** — the one thing only
/// June's claim says, and the one thing the walk asks for.
async fn october_files_the_note_on_the_event(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:floor-pump", "content": "brought round over the summer",
               "provenance": "testimony",
               "shape": "connection", "object": "person:nelson"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:trail-survey",
               "content": "the ground needs a look before next year",
               "provenance": "testimony"}),
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
               "recorded_at": "2026-10-24"}),
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
               "provenance": "testimony",
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
    let survey = handle_of_kind(room, "event").await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "person:bart", "content": "was at the trail survey",
               "provenance": "inference",
               "shape": "attendance", "object": survey}),
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
               "provenance": "testimony",
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
               "provenance": "inference",
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
               "provenance": "inference",
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
               "provenance": "inference",
               "fields": {"was": "The North Trail Club meets on Tuesdays (recorded 2026-03-15, corrected 2026-07-05)"}}),
    )
    .await;
}

/// **A December that rewrites a claim nobody raised — and this is LEGITIMATE.**
///
/// A sitting may notice its own mistake and correct it, on any record it made.
/// The pairing's lock used to score this as a fault, because it demanded the
/// record carry one write and no other; that asserted what the model happened
/// to do rather than anything about jojobot, and a paid run failed on it while
/// the product did nothing wrong. **The play is kept and its verdict is
/// flipped**: a run that really made a second write must produce a trace
/// reporting two.
async fn december_corrects_a_claim_nobody_questioned(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "update_fact",
        json!({"address": "person:bart#f1", "content": "joined the club in the autumn",
               "recorded_at": "2026-12-13"}),
    )
    .await;
}

/// The whole year, worked the way it is meant to be.
async fn worked_the_year(room: &Surface) -> Vec<Boundary> {
    work_the_year(room, &room_document(), &WORKED, &[]).await
}

/// Where June sits in the year, named because two guilty plays share it.
const JUNE_AT: usize = 5;

/// **June's other guilts, named by indices no sitting has.**
///
/// The guilty list is indexed by sitting and June already carries one wrong —
/// setting the loop's key by hand. These are the ones about what its claim
/// POINTS AT, and each must stay apart from the others: a play carrying two
/// would turn two locks red at once and no case could say which wrong it
/// measured.
///
/// ⚠️ **The year holds fifteen sittings, so 15 and above address none of
/// them.** They name variants rather than phases, which is why they are
/// constants with this paragraph beside them rather than numbers in a call.
const JUNE_IN_WORDS: usize = 15;
const JUNE_WITHOUT_THE_EVENT: usize = 16;
const JUNE_POINTING_AT_ONE_KIND: usize = 17;

/// What June's claim says under each of them. **The first one named wins**, so
/// a caller naming two gets the first rather than a silent mixture.
const JUNE_VARIANTS: [(usize, &str); 3] = [
    (JUNE_IN_WORDS, IN_WORDS),
    (JUNE_WITHOUT_THE_EVENT, A_PLACE_AND_A_PERSON),
    (JUNE_POINTING_AT_ONE_KIND, PEOPLE_ONLY),
];

/// Where October sits in the year, named for the reason `JUNE_AT` is.
const OCTOBER_AT: usize = 9;

/// **October's other guilt, named by an index no sitting has**, for the same
/// reason June's variants are — the year holds fifteen sittings, so 19
/// addresses none of them. Index 9 already carries a different wrong (the
/// note filed on the event rather than the place); this is a second, about
/// the pump's two accounts rather than the survey's location, and the two
/// must stay apart for the reason June's own variants do.
const OCTOBER_REMOVES_ONE_OF_THE_TWO_ACCOUNTS: usize = 19;

/// **October's third guilt, named the same way.** Everything else this
/// sitting is asked to do is done; the one thing missing is the rename its
/// own reason was given for.
const OCTOBER_DOES_NOT_RENAME_THE_SURVEY: usize = 20;

/// Where late November sits in the year, named for the reason `JUNE_AT` is.
const LATE_NOVEMBER_AT: usize = 12;

/// **Late November's other guilt, named by an index no sitting has**, for the
/// same reason June's variants are — the year holds fifteen sittings, so 18
/// addresses none of them.
///
/// This is the failure the room's own document reads out at Phase 13: a
/// sitting that reaches for the operator's words, finds nothing under
/// January's handle, and stands up a SECOND loop rather than the first —
/// after which the store holds two, each with half the history.
const LATE_NOVEMBER_STANDS_UP_A_SECOND_LOOP: usize = 18;

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
    year: &Playbook,
    worked: &[usize],
    guilty: &[usize],
) -> Vec<Boundary> {
    let named = boundary_names(year);
    // What June's claim says, when the guilty list names one of its variants.
    let variant = JUNE_VARIANTS
        .iter()
        .find(|(which, _)| guilty.contains(which))
        .map(|(_, said)| *said);
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for (at, phase) in year.phases.iter().enumerate() {
        let june_variant = match at == JUNE_AT {
            true => variant,
            false => None,
        };
        let late_november_variant =
            at == LATE_NOVEMBER_AT && guilty.contains(&LATE_NOVEMBER_STANDS_UP_A_SECOND_LOOP);
        let october_variant =
            at == OCTOBER_AT && guilty.contains(&OCTOBER_REMOVES_ONE_OF_THE_TWO_ACCOUNTS);
        let october_no_rename_variant =
            at == OCTOBER_AT && guilty.contains(&OCTOBER_DOES_NOT_RENAME_THE_SURVEY);
        // ⛔️ **Only a sitting that ACTS gets a run.** A boot mints nothing until
        // its first write, so a run for a sitting this drive skips is a run that
        // never happened — and it would sit in the day that sitting claims,
        // where the next drive of the same day meets it still working.
        if !worked.contains(&at)
            && !guilty.contains(&at)
            && june_variant.is_none()
            && !late_november_variant
            && !october_variant
            && !october_no_rename_variant
        {
            boundaries.push(boundary(room, &named[at + 1]).await);
            continue;
        }
        let sid = &sitting(
            room,
            phase
                .day
                .as_deref()
                .unwrap_or_else(|| panic!("{} claims no day", phase.name)),
        )
        .await;
        // **The sitting does the wrong thing, in its own window.** Here rather
        // than in a second driver: two copies of this order was how they came
        // to disagree about which sittings write.
        if let Some(said) = june_variant {
            june_saying(room, sid, said).await;
        } else if late_november_variant {
            late_november_stands_up_a_second_loop(room, sid).await;
        } else if october_variant {
            october_removes_one_of_the_two_accounts(room, sid).await;
        } else if october_no_rename_variant {
            october_without_renaming_the_survey(room, sid).await;
        } else if guilty.contains(&at) {
            match at {
                9 => october_files_the_note_on_the_event(room, sid).await,
                6 => july_takes_the_claim_back(room, sid).await,
                7 => august_puts_a_third_person_there(room, sid).await,
                10 => late_october_puts_a_third_person_there(room, sid).await,
                5 => june_writes_the_turn_by_hand(room, sid).await,
                12 => late_november_writes_the_turn_by_hand(room, sid).await,
                1 => february_records_no_holder(room, sid).await,
                8 => september_sits_in_the_wrong_day(room, sid).await,
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
    let (_room, surface) = furnished().await;
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
    let (_room, surface) = furnished().await;
    // The readings a run takes, with nothing done between them: a check scoped
    // to one sitting must see an empty window rather than no window at all.
    let boundaries = work_the_year(&surface, &room_document(), &[], &[]).await;
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
    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
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
    let (_room, surface) = furnished().await;
    // June onwards, done as well as a session can do it against a store that
    // holds nothing any of it refers to.
    let boundaries = work_the_year(&surface, &room_document(), &[5, 7, 8, 9], &[]).await;
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
    let (_room, surface) = furnished().await;
    // **One sitting per DAY, not one per claim.** Two runs on the same stated
    // day are both still working in that frame, so the second meets the first
    // rather than sweeping it — which is right, and means a day gets one run.
    let mut opened: std::collections::HashMap<&str, String> = std::collections::HashMap::new();
    let opening = sitting(&surface, "2026-01-12").await;
    opened.insert("2026-01-12", opening.clone());
    did(&surface, &opening, "read_mailbox", json!({})).await;
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
        // **Each claim under the day its own sitting is in.** The prose year
        // is still a year: a run per day, saying nothing but sentences.
        let said_on = match opened.get(day) {
            Some(open) => open.clone(),
            None => {
                let fresh = sitting(&surface, day).await;
                opened.insert(day, fresh.clone());
                fresh
            }
        };
        did(
            &surface,
            &said_on,
            "capture",
            json!({"subject": subject, "content": said, "provenance": "testimony"}),
        )
        .await;
    }
    let boundaries = work_the_year(&surface, &room_document(), &[], &[]).await;
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
    let (_room, surface) = furnished().await;
    let year = room_document();
    // The run's own reads, in the run's own order and with the run's own names,
    // taken by the one driver the lock case uses. Two copies of this order was
    // how they came to disagree about which sittings write.
    let boundaries = worked_the_year(&surface).await;

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
    assert!(
        missed.is_empty(),
        "a run of this year would fail {} of its generated date assertions, so it is not \
         winnable by anybody: {}",
        missed.len(),
        missed.join("\n  "),
    );
    // ⚠️ **The count comes after the failures, and the order is load-bearing.**
    // It is a calibration: it moves whenever a sitting is added or an exemption
    // comes out, so asserting it first masks the assertion underneath — the
    // case stops before saying which sitting failed and why.
    assert_eq!(
        asked, 13,
        "the year generates one assertion per dated sitting, less the two a person reads — and \
         this asked about {asked}",
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
    let (_room, surface) = furnished().await;
    let boundaries = work_the_year(&surface, &room_document(), &without, &[]).await;
    let missing = judge_all(&surface, &boundaries).await;
    assert!(
        !missing[MARCH[0]].held,
        "a year where March recorded nothing held March's lock, so the window is reading \
         something else: {}",
        saying(&missing),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
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
    let (_room, surface) = furnished().await;
    let guilty = work_the_year(&surface, &room_document(), &WORKED, &[6]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[JULY[0]].held,
        "a July that took the claim back held July's lock, so the check can no longer fail for \
         the reason it exists: {}",
        saying(&judged),
    );

    // ② The year worked honestly, and then a later sitting takes a club claim
    //    back — the legitimate act that used to redden July.
    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let committee = address_of(&surface, "org:north-trail-club", "standing for election").await;
    // **A sitting after the year's last**, which is what makes it a later one.
    let later = sitting(&surface, "2026-12-22").await;
    did(
        &surface,
        &later,
        "retract",
        json!({"address": committee,
               "reason": "the operator never stood for the board and this was never so",
               "recorded_at": "2026-12-13"}),
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
    let (_room, surface) = furnished().await;
    let guilty = work_the_year(&surface, &room_document(), &WORKED, &[7]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[AUGUST[0]].held,
        "an August that put a third person at the survey held August's lock, so the check can no \
         longer fail for the reason it exists: {}",
        saying(&judged),
    );

    // ② The year worked honestly, and then a LATER sitting makes that mistake.
    //    It is a fault, and it is not August's.
    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let survey = handle_of_kind(&surface, "event").await;
    // **A sitting after the year's last**, which is what makes it a later one.
    let later = sitting(&surface, "2026-12-22").await;
    did(
        &surface,
        &later,
        "capture",
        json!({"subject": "person:bart", "content": "was at the trail survey",
               "provenance": "inference",
               "shape": "attendance", "object": survey}),
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
    let (_room, surface) = furnished().await;
    let guilty = work_the_year(&surface, &room_document(), &WORKED, &[10]).await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[LATE_OCTOBER[2]].held,
        "a late October that put the new arrival at the survey held its own lock, so nothing in \
         this room catches an invented attendee under the name of whoever invented one: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_OCTOBER[2]].held,
        "the honest late October failed its own lock, so the check cannot tell an attendance \
         taken away from one added: {}",
        saying(&judged),
    );
}

/// **An October that gives the reason and never makes the move it was given
/// for.** Everything else it is asked to do is done, so a red here is about
/// the rename and nothing else.
#[tokio::test]
async fn junes_mention_lock_catches_a_survey_that_was_never_renamed() {
    let (_room, surface) = furnished().await;
    let guilty = work_the_year(
        &surface,
        &room_document(),
        &WORKED,
        &[OCTOBER_DOES_NOT_RENAME_THE_SURVEY],
    )
    .await;
    let judged = judge_all(&surface, &guilty).await;
    assert!(
        !judged[LATE_OCTOBER[3]].held,
        "an October that was given a reason to rename the survey and never did held the lock \
         that watches for it: {}",
        saying(&judged),
    );
    // The half that stops the assertion above passing on a room where every
    // lock fails: October's other work is still there.
    assert!(
        judged[OCTOBER[1]].held,
        "the sitting that skipped the rename failed a lock that has nothing to do with it: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_OCTOBER[3]].held,
        "the honest year, which does rename the survey, failed the lock that watches for it: {}",
        saying(&judged),
    );
}

/// 🚨 **A June that wrote WORDS where it could have written handles fails, and
/// everything else it recorded still holds.**
///
/// ⛔️ **This is the discrimination the room can make, and it is about the
/// SITTING rather than about the build.** A mention is stored as a badge and
/// served as the handle it wears today, so on a build with nothing renaming
/// anything the served text is what was typed either way — text in, same text
/// out, whether or not the layer is in the chain. **What a room measures is the
/// model**: whether a session told nothing reached for the capability. Whether
/// the layer works is the contract's question, and it is asked of both stores
/// there.
///
/// **Paired in one case**, because a lock that only fails is a lock that could
/// be failing for any reason: the guilty June draws the same edges and checks
/// the same loop in, so its other locks hold and the one that moves is the one
/// about handles.
#[tokio::test]
async fn a_june_that_wrote_words_leaves_a_later_sitting_nothing_to_follow() {
    let (_room, surface) = furnished().await;
    let in_words = work_the_year(&surface, &room_document(), &WORKED, &[JUNE_IN_WORDS]).await;
    let judged = judge_all(&surface, &in_words).await;
    assert!(
        !judged[JUNE[2]].held,
        "a June that named three things as words held the lock about handles, so the room cannot tell a pointer from a sentence: {}",
        saying(&judged),
    );
    // **The positive that says the guilty sitting is otherwise a good one.**
    // Without it this passes on a June that did nothing at all, and the lock
    // would be reporting an absent sitting rather than a worded one.
    assert!(
        judged[JUNE[0]].held && judged[JUNE[1]].held,
        "the guilty June failed a lock it was meant to hold, so the case above is measuring a sitting that did not happen: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[JUNE[2]].held,
        "the year wrote the operator's three nouns as handles and the lock still failed: {}",
        saying(&judged),
    );
}

/// 🚨 **The lock about handles reads TWO KINDS ON ONE RECORD rather than a
/// named three.**
///
/// ⛔️ **It used to demand a person, a place and an event together.** Nothing in
/// the year's own story asks a sitting to name all three in one sentence, so a
/// June that wrote pointers everywhere and never that one combination failed a
/// lock about pointers. **A red that survives the fix it is asking for is a red
/// nobody trusts the next time it fires.**
///
/// **Two readings in one case**, over Junes that differ in nothing but what the
/// survey claim points at:
///
/// * a claim pointing at a place and a person HOLDS — two kinds on one record
///   is a link between two things, whichever two they are;
/// * a claim pointing at people only FAILS — every pointer leads to the same
///   kind of thing, so no claim links one kind to another and the floor is not
///   simply *a handle was written somewhere*.
///
/// The third reading — a claim written in words — is the case above this one,
/// which is where the failure this lock exists for is already watched.
#[tokio::test]
async fn the_handle_lock_reads_two_kinds_on_one_record_rather_than_a_named_three() {
    let (_room, surface) = furnished().await;
    let two_kinds = work_the_year(
        &surface,
        &room_document(),
        &WORKED,
        &[JUNE_WITHOUT_THE_EVENT],
    )
    .await;
    let judged = judge_all(&surface, &two_kinds).await;
    assert!(
        judged[JUNE[2]].held,
        "a June whose claim points at a place and a person failed the lock about handles, so it \
         still asks for one combination of kinds the year never calls for: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let one_kind = work_the_year(
        &surface,
        &room_document(),
        &WORKED,
        &[JUNE_POINTING_AT_ONE_KIND],
    )
    .await;
    let judged = judge_all(&surface, &one_kind).await;
    assert!(
        !judged[JUNE[2]].held,
        "a June whose claim points only at people held the lock, so it asks whether a handle was \
         written anywhere rather than whether one record links two kinds of thing: {}",
        saying(&judged),
    );
    // **The positive that says the guilty sitting is otherwise a good one.**
    // Without it this passes on a June that did nothing at all, and the lock
    // would be reporting an absent sitting rather than a one-sided one.
    assert!(
        judged[JUNE[0]].held && judged[JUNE[1]].held,
        "the June that pointed at one kind failed a lock it was meant to hold, so the case above \
         is measuring a sitting that did not happen: {}",
        saying(&judged),
    );
}

/// 🚨 **An October that recorded the note and never said WHERE fails, and
/// everything else it recorded still holds.**
///
/// The operator asks where the survey was held and never says. **June's claim
/// is the one record in the year that answers it**, so a sitting that never
/// read it has the note and not the place.
///
/// ⛔️ **Whether the sitting FOLLOWED the mention is not what moves this.**
/// Following leaves no trace: a sitting that read the claim and one that
/// guessed leave the same store, which is what this room already says about
/// August's walk. **What moves it is the write.**
///
/// **Paired in one case.** The guilty October files the note on the event and
/// records the pump exactly as the good one does, so its other lock holds and
/// the one that moves is the walk to the place.
#[tokio::test]
async fn an_october_that_never_said_where_leaves_the_survey_unplaceable() {
    let (_room, surface) = furnished().await;
    let nowhere = work_the_year(&surface, &room_document(), &WORKED, &[9]).await;
    let judged = judge_all(&surface, &nowhere).await;
    assert!(
        !judged[OCTOBER[1]].held,
        "an October that never said where the survey was held satisfied the walk to the place, so the lock is not about where it happened: {}",
        saying(&judged),
    );
    // **The positive that says the guilty sitting is otherwise a good one.**
    // Without it this passes on an October that did nothing at all.
    assert!(
        judged[OCTOBER[0]].held,
        "the guilty October failed the lock it was meant to hold, so the case above is measuring a sitting that did not happen: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[OCTOBER[1]].held,
        "the year recorded where the survey was held and the walk still failed: {}",
        saying(&judged),
    );
}

/// 🚨 **The walk to the place holds whatever the event is actually called.**
///
/// A paid run named the event `event:trail-survey-2026` — a reasonable slug
/// nothing in the year rules out. The lock used to name `event:trail-survey`
/// literally, so that run's October was refused rather than measured: the
/// walk never ran, and the tally read as a product failure that was really a
/// mismatched string. The scripted sittings above always use the one slug
/// this file's own `january` picks, so they cannot catch that — this test
/// picks a different one on purpose.
#[tokio::test]
async fn the_walk_to_the_place_holds_whatever_the_event_is_actually_called() {
    let (_room, surface) = furnished().await;
    let sid = sitting(&surface, "2026-06-14").await;
    did(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "event", "handle": "trail-survey-2026", "name": "The trail survey",
               "source": "the operator"}),
    )
    .await;
    did(
        &surface,
        &sid,
        "capture",
        json!({"subject": "event:trail-survey-2026",
               "content": "the ground needs a look before next year",
               "provenance": "testimony",
               "shape": "location", "object": "place:north-trail"}),
    )
    .await;

    let judged = judge_all(&surface, &[]).await;
    assert!(
        !judged[OCTOBER[1]].refused,
        "the walk to the place was refused rather than measured, so the lock still depends on \
         January's exact slug: {}",
        saying(&judged),
    );
    assert!(
        judged[OCTOBER[1]].held,
        "the walk to the place failed against an event named something other than \
         event:trail-survey: {}",
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
    let (_room, surface) = furnished().await;
    let by_hand = work_the_year(&surface, &room_document(), &WORKED, &[5, 12]).await;
    let judged = judge_all(&surface, &by_hand).await;
    assert!(
        !judged[LATE_NOVEMBER[0]].held,
        "a year that set the loop's key by hand held the lock, so the room cannot tell a \
         check-in from a write and December has nothing to notice: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_NOVEMBER[0]].held,
        "the year checked in and the turns are not on file as derivations: {}",
        saying(&judged),
    );
}

/// 🚨 **The derivations lock is scoped to January's own loop, and a second
/// loop cannot drag it down.**
///
/// A universal that named no loop would fail the moment ANY rhythm anywhere
/// carried a hand-set turn — including one January never opened and this
/// year's story never asked about. **This is what gives that scoping
/// something to be wrong about**: January's loop is left entirely clean here,
/// and a second, unrelated loop carries the only bad turn in the store.
///
/// ⭐ **This case is the fixture half of the proof, and it is not the whole
/// proof.** It passes today whether the lock is scoped correctly or not,
/// because nothing in this suite widens the measurement back to every
/// rhythm — that half is watched with `scripts/sabotage` against
/// `the_years_turns_are_on_file_as_derivations` in `src/checks.rs`, which
/// must turn this case red.
#[tokio::test]
async fn the_derivations_lock_is_scoped_to_januarys_loop_and_not_dragged_down_by_a_second() {
    let (_room, surface) = furnished().await;
    let boundaries = work_the_year(
        &surface,
        &room_document(),
        &WORKED,
        &[LATE_NOVEMBER_STANDS_UP_A_SECOND_LOOP],
    )
    .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        judged[LATE_NOVEMBER[0]].held,
        "January's loop is untouched and entirely clean, and the derivations lock failed \
         anyway — a second loop's hand-set turn is reaching a check that names one loop: {}",
        saying(&judged),
    );
    assert!(
        !judged[LATE_NOVEMBER[1]].held,
        "a second loop stood up beside the first and the lock built for exactly this case did \
         not catch it: {}",
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
    let (_room, surface) = furnished().await;
    let boundaries = work_the_year(&surface, &room_document(), &WITHOUT_JUNE, &[]).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[JUNE[1]].held,
        "a year where June never touched the loop held June's chain lock, so it is satisfied by \
         something other than that sitting: {}",
        saying(&judged),
    );

    const WITHOUT_LATE_NOVEMBER: [usize; 11] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10];
    let (_room, surface) = furnished().await;
    let boundaries = work_the_year(&surface, &room_document(), &WITHOUT_LATE_NOVEMBER, &[]).await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[LATE_NOVEMBER[1]].held,
        "a year where the late sitting recorded no turn held its lock, so it is satisfied by \
         something other than that sitting: {}",
        saying(&judged),
    );
}

/// 🚨 **A retracted account does not stand, and `carries` could not see that.**
///
/// A paid run had October retract September's account of the pump rather than
/// leave it beside Nelson's — a sitting picking a winner, which is exactly the
/// thing the lock beside it exists to catch. **The old lock held anyway**: a
/// retraction is marked rather than filtered, so `recall` served the retracted
/// record's own text back, edge and all, and a substring assertion cannot see
/// the `status` key that would have told the two apart.
#[tokio::test]
async fn a_retracted_account_does_not_satisfy_the_pump_lock() {
    let (_room, surface) = furnished().await;
    let boundaries = work_the_year(
        &surface,
        &room_document(),
        &WORKED,
        &[OCTOBER_REMOVES_ONE_OF_THE_TWO_ACCOUNTS],
    )
    .await;
    let judged = judge_all(&surface, &boundaries).await;
    assert!(
        !judged[OCTOBER[0]].held,
        "October retracted Ralph's account instead of leaving it beside Nelson's, and the pump \
         lock held anyway: {}",
        saying(&judged),
    );
    assert!(
        judged[OCTOBER[0]].saying.contains("picked a winner"),
        "a retraction is a verified act, not an absence, and the failure text should say so \
         rather than reading as though nothing was ever written: {}",
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
///
/// ⚠️ **The pairing's failing half is NOT asserted here, and it cannot be.** A
/// play can only make writes that really happened, so every play produces a
/// trace agreeing with the boundaries. The failure is a product fault — a
/// trace reporting a write nobody made — and it is watched by breaking the
/// trace under `scripts/sabotage`, never by driving the year differently. What
/// this case holds is that a legitimate correction passes and the worked year
/// passes, which is the half a play can reach.
#[tokio::test]
async fn the_trace_locks_tell_a_correction_from_a_claim_nobody_touched() {
    let (_room, surface) = furnished().await;
    let wrong_route = work_the_year(&surface, &room_document(), &WORKED, &[14]).await;
    let judged = judge_all(&surface, &wrong_route).await;
    assert!(
        !judged[LATE_DECEMBER[0]].held,
        "a sitting that answered from the claim as it stands held the lock, so the room cannot \
         tell a read of the record's history from a read of current truth: {}",
        saying(&judged),
    );

    // ⛔️ **A legitimate correction PASSES, and that is the fix.** The occupant
    // owns this record and may rewrite it; the trace then truthfully reports
    // two writes, and two writes is what the run made. A lock that failed here
    // was measuring the model rather than the product.
    let (_room, surface) = furnished().await;
    let corrected = work_the_year(&surface, &room_document(), &WORKED, &[13]).await;
    let judged = judge_all(&surface, &corrected).await;
    assert!(
        judged[LATE_DECEMBER[1]].held,
        "a sitting corrected a record it made and the trace reported that correction, and the \
         lock still failed — so it is scoring what the occupant did rather than whether the \
         trace agrees with it: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
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
    let (_room, surface) = furnished().await;
    let silent = work_the_year(&surface, &room_document(), &WORKED, &[1]).await;
    let judged = judge_all(&surface, &silent).await;
    assert!(
        !judged[FEBRUARY[1]].held,
        "a February that recorded nothing about who had the pump held its lock, so the sitting \
         that returns it seven months later is still answering for this one: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let boundaries = worked_the_year(&surface).await;
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
    let (_room, surface) = furnished().await;
    let _ = worked_the_year(&surface).await;
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
        summary.ambiguous, 1,
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
    let (_room, surface) = furnished().await;
    let fuller = work_the_year(&surface, &room_document(), &WITHOUT_LATER_DECEMBER, &[]).await;
    let settling = sitting(&surface, "2026-12-20").await;
    later_december_writes_a_fuller_sentence(&surface, &settling).await;
    let judged = judge_all(&surface, &fuller).await;
    assert!(
        judged[LATE_DECEMBER[0]].held,
        "a sitting that wrote the old wording inside a fuller sentence was scored as not having \
         written it: {}",
        saying(&judged),
    );

    let (_room, surface) = furnished().await;
    let wrong = work_the_year(&surface, &room_document(), &WORKED, &[14]).await;
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
    let (_room, surface) = furnished().await;
    let _ = worked_the_year(&surface).await;
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

/// 🚨 **September is graded like every other sitting now, and it still catches
/// a sitting in the wrong day.**
///
/// The room used to exempt it by hand: it records something true of June while
/// sitting in September, and one column carried both meanings, so its own day
/// never reached the record. **The split ended that** — the day it writes under
/// and the day the thing happened are different fields — **and the exemption
/// was dead before this removed it.**
///
/// ⛔️ **Which is exactly why the pairing matters.** A marker removed because
/// the check can no longer catch anything looks identical to a marker removed
/// because the check no longer needs it.
#[tokio::test]
async fn september_is_graded_and_a_sitting_in_the_wrong_day_still_fails() {
    let year = room_document();
    let (_room, surface) = furnished().await;
    let confused = work_the_year(&surface, &year, &WORKED, &[8]).await;
    let seen = Observed {
        room: &surface,
        boundaries: &confused,
    };
    let mut missed = Vec::new();
    for dated in days_claimed(&year) {
        let outcome = dated.check(&seen).await;
        if !outcome.held && outcome.applies {
            missed.push(outcome.name.clone());
        }
    }
    assert!(
        missed.iter().any(|name| name.starts_with("Phase 9")),
        "a September that wrote under June was not caught, so taking its exemption out left a \
         check with nothing to catch: {missed:?}",
    );
}
