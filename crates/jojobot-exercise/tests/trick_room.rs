//! **The trick room, held for free.**
//!
//! Nothing here drives a model. Each lock is run against a real room whose
//! state is put there by hand, through the verbs, the way an occupant would.
//!
//! **Sixteen locks, and each is proven twice.** A first sitting that wrote every
//! fact where its reader looks holds all of them. A first sitting that wrote
//! ONE fact in the convenient wrong place reds exactly the lock that reads it.
//! Those are the discrimination cases, and the solvability cases are named
//! apart from them: the question asked through the surface after a correct
//! first sitting returns the intended answer.

use jojobot_exercise::expectations::{self, TRICK_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Observed, Outcome, boundary, boundary_asking, phase_end_queries,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};
use std::sync::LazyLock;

/// **The day this room's server runs on**, read from the room's own document
/// (the one the paid path reads) and carried to the child as its own clock. Every moment the server stamps is this day, whatever the real date is,
/// so no needle can be met by a stamp on one real day. It is a day no needle
/// of this room holds, and `no_needle_here_is_the_servers_own_day` asks that.
/// **It comes before every day the sittings write:** a snooze has to end after
/// the check-in's own day, and a sitting that states no day checks in on this one.
static SERVER_DAY: LazyLock<String> = LazyLock::new(|| {
    document()
        .stated_server_day()
        .unwrap_or_else(|e| panic!("{e:#}"))
        .to_string()
});

fn document() -> Playbook {
    Playbook::read(&expectations::room_document(TRICK_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a session to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (handle, surface) = Room::open_with_client_stating(
        &server_binary().expect("a jojobot binary"),
        SERVER_DAY.as_str(),
    )
    .await
    .expect("a room");
    expectations::seed_for(TRICK_ROOM)
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

/// **The boundaries a run records around the brief's sitting**: one ahead of
/// each phase, labelled with the phase about to run. The reading after the
/// brief is the one ahead of the reader.
async fn around_the_brief(room: &Surface) -> Vec<Boundary> {
    let locks = expectations::for_playbook(TRICK_ROOM).expect("the room asserts");
    vec![
        boundary(room, "Phase 1 \u{2014} the brief").await,
        boundary_asking(
            room,
            "Phase 2 \u{2014} the reader",
            &phase_end_queries(&locks, "Phase 1"),
        )
        .await,
    ]
}

async fn judge_all(room: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let seen = Observed { room, boundaries };
    let mut outcomes = Vec::new();
    for check in expectations::for_playbook(TRICK_ROOM).expect("the room asserts") {
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

/// **The locks, in the order the room lists them.**
const FERN_BEFORE: usize = 0;
const FERN_AFTER: usize = 1;
const CHAIRS_BEFORE: usize = 2;
const CHAIRS_AFTER: usize = 3;
const DESK_BEFORE: usize = 4;
const DESK_AFTER: usize = 5;
const TICKET_OWED: usize = 6;
const NOT_ON_BART: usize = 7;
const MAUDE: usize = 8;
const NUMBERS: usize = 9;
const COUNT: usize = 10;
const RULE: usize = 11;
const HOLD: usize = 12;
const FURNACE_OWED: usize = 13;
const NOT_ON_MR_PLOW: usize = 14;
const FAIR: usize = 15;
const LOCKS: usize = 16;

/// **Where the fern's pause goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fern {
    /// A snooze check-in naming the day the loop comes back.
    Snoozed,
    /// A note on the assistant's own record and nothing on the loop.
    NoteOnly,
    /// The loop taken out for good.
    Archived,
}

/// **Where the folding chairs' pause goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Chairs {
    /// A promise owed on the day the operator gave.
    PromiseOnTheDay,
    /// A note on the assistant's own record and nothing that falls due.
    NoteOnly,
    /// A promise owed from the first day, which is a reminder and not a pause.
    PromiseFromNow,
}

/// **Where the guest's yes goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Maude {
    /// A claim on Maude drawing an attendance edge at the party.
    Edge,
    /// A note on the assistant's own record.
    NoteOnly,
    /// A claim on the party's own record, with no edge.
    ProseOnTheParty,
}

/// **Where the corrected day goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Desk {
    /// The same key written again with the corrected day.
    Corrected,
    /// A sentence beside the first claim, and the key left as it was.
    Beside,
    /// Both days under a key of the model's own, which nothing reads as a day
    /// something is owed.
    InventedKey,
}

/// **Where the refund window's closing day goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ticket {
    /// The tickets as a thing of their own, carrying the day the window runs
    /// out.
    OnTheTickets,
    /// A promise to claim the refund, owed on the day and regarding the
    /// tickets.
    AsAPromise,
    /// The day carried by the person who bought them.
    OnThePerson,
    /// The tickets as a thing of their own, the day in a sentence and under a
    /// key nothing reads as a day something is owed.
    InventedKey,
    /// The tickets as a thing of their own, the day only in a sentence.
    InAProse,
}

/// **Where the furnace's free service lapse day goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Furnace {
    /// The furnace carrying the day its free service runs out.
    OnTheMachine,
    /// The day carried by the company that serviced it.
    OnTheCompany,
    /// The day on the furnace in a sentence and under a key nothing reads as a
    /// day something is owed.
    InventedKey,
    /// The day on the furnace only in a sentence.
    InAProse,
}

/// **Where the certificate's source goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fair {
    /// The requirement on the fair, the course on the college, and a record on
    /// the fair that refers to the college.
    PointsAtTheCollege,
    /// The requirement on the fair and the course on the college, with nothing
    /// on the fair that points at the college.
    OnlyOnTheCollege,
}

/// **What each number is backed as.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Donuts {
    /// The operator's numbers as testimony, the count and its recap as
    /// inference.
    Apart,
    /// Every claim left on the default backing.
    AllDefault,
    /// The recap that restates the count filed as the operator's word.
    RecapAsTestimony,
    /// The number of people filed as the operator's word and the three each
    /// never recorded as theirs. A standing rule says "three" elsewhere.
    EachNeverRecorded,
    /// The count and its recap written in words, "thirty six" with a space,
    /// and as inference. The figure is right and no digit states it.
    SpelledOut,
}

/// **Where the first standing rule goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Only the first rule carried to the boot.
    FirstOnly,
    /// Every rule starred, which is more than the boot has seats for: each
    /// refused star is let in by unstarring the oldest seated rule, so the first
    /// rule is the one given up.
    Every,
    /// Every rule written and none carried.
    Nothing,
}

/// **Where the hold goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Hold {
    /// A rule on the helper itself, marked to ride its boot.
    OnTheHelper,
    /// A note on the assistant's own record.
    OnTheAssistant,
    /// A message in the helper's box and nothing on the helper.
    MessageOnly,
    /// A claim on the helper that is not marked to ride its boot.
    NotCarried,
    /// A line in the helper's charter, which its boot carries.
    InCharter,
    /// A rule on the helper marked to ride its boot whose words say nothing of
    /// the job and whose details hold the line.
    InTheRulesDetails,
    /// A rule on the helper marked to ride its boot that hands it the job and
    /// holds nothing back.
    HandedTheJob,
}

/// **One first sitting, every choice named.** `right()` is the sitting that
/// wrote each fact where its reader looks; a case changes exactly one field.
#[derive(Clone, Copy)]
struct Sitting {
    fern: Fern,
    chairs: Chairs,
    maude: Maude,
    desk: Desk,
    ticket: Ticket,
    furnace: Furnace,
    fair: Fair,
    donuts: Donuts,
    rule: Rule,
    hold: Hold,
}

fn right() -> Sitting {
    Sitting {
        fern: Fern::Snoozed,
        chairs: Chairs::PromiseOnTheDay,
        maude: Maude::Edge,
        desk: Desk::Corrected,
        ticket: Ticket::OnTheTickets,
        furnace: Furnace::OnTheMachine,
        fair: Fair::PointsAtTheCollege,
        donuts: Donuts::Apart,
        rule: Rule::FirstOnly,
        hold: Hold::OnTheHelper,
    }
}

async fn note(room: &Surface, sid: &str, subject: &str, content: &str, extra: Value) {
    let mut args = json!({"subject": subject, "content": content, "provenance": "testimony"});
    for (key, value) in extra.as_object().expect("an object") {
        args[key] = value.clone();
    }
    let said = as_the_occupant(room, sid, "capture", args).await;
    assert!(
        !said.contains("\"status\":\"blocked\""),
        "the sitting's own write was refused: {said}"
    );
}

/// **The first sitting, played through the verbs.**
async fn played(room: &Surface, sid: &str, sitting: Sitting) {
    // The fern.
    match sitting.fern {
        Fern::Snoozed => {
            note(
                room,
                sid,
                "rhythm:water-the-fern",
                "away until the twentieth",
                json!({"check_in": "snoozed", "sets": {"snoozed_until": "2026-11-20"}}),
            )
            .await
        }
        Fern::NoteOnly => {
            note(
                room,
                sid,
                "bot:assistant",
                "the operator is away until 2026-11-20 and does not want to hear about the fern",
                json!({}),
            )
            .await
        }
        Fern::Archived => {
            let said = as_the_occupant(
                room,
                sid,
                "archive_entity",
                json!({"handle": "rhythm:water-the-fern", "reason": "away"}),
            )
            .await;
            assert!(!said.contains("blocked"), "{said}");
        }
    }

    // The folding chairs.
    if sitting.chairs != Chairs::NoteOnly {
        as_the_occupant(
            room,
            sid,
            "add_entity",
            json!({"kind": "promise", "handle": "return-the-desk",
                   "name": "Hold off on the folding chairs", "source": "user-named",
                   "parent": "person:homer"}),
        )
        .await;
        let day = match sitting.chairs {
            Chairs::PromiseFromNow => "2026-10-08",
            _ => "2026-11-20",
        };
        note(
            room,
            sid,
            "promise:return-the-desk",
            "Not to be reminded about before the day.",
            json!({"sets": {"promised_by": day, "regarding": "thing:folding-chairs"}}),
        )
        .await;
    } else {
        note(
            room,
            sid,
            "bot:assistant",
            "no reminders about the folding chairs until 2026-11-20",
            json!({}),
        )
        .await;
    }

    // Maude.
    match sitting.maude {
        Maude::Edge => {
            note(
                room,
                sid,
                "person:maude",
                "Said yes to the birthday party.",
                json!({"shape": "attendance", "object": "event:birthday-party"}),
            )
            .await
        }
        Maude::NoteOnly => {
            note(
                room,
                sid,
                "bot:assistant",
                "Maude already said yes to the birthday party",
                json!({}),
            )
            .await
        }
        Maude::ProseOnTheParty => {
            note(
                room,
                sid,
                "event:birthday-party",
                "Maude already said yes.",
                json!({}),
            )
            .await
        }
    }

    // The desk.
    let key = match sitting.desk {
        Desk::InventedKey => "warranty_ends",
        _ => "runs_out",
    };
    note(
        room,
        sid,
        "thing:standing-desk",
        "The warranty runs out.",
        json!({"sets": {key: "2026-12-03"}}),
    )
    .await;
    match sitting.desk {
        Desk::Corrected | Desk::InventedKey => {
            note(
                room,
                sid,
                "thing:standing-desk",
                "The warranty runs out, corrected.",
                json!({"sets": {key: "2026-12-10"}}),
            )
            .await
        }
        Desk::Beside => {
            note(
                room,
                sid,
                "thing:standing-desk",
                "Correction: the warranty runs out on 2026-12-10.",
                json!({}),
            )
            .await
        }
    }

    // The tickets, and the day the refund window closes.
    if sitting.ticket != Ticket::OnThePerson {
        as_the_occupant(
            room,
            sid,
            "add_entity",
            json!({"kind": "thing", "handle": "tau", "name": "The show tickets",
                   "source": "user-named"}),
        )
        .await;
    }
    match sitting.ticket {
        Ticket::OnTheTickets => {
            note(
                room,
                sid,
                "thing:tau",
                "The refund window on the tickets closes.",
                json!({"sets": {"runs_out": "2026-12-01"}}),
            )
            .await
        }
        Ticket::AsAPromise => {
            as_the_occupant(
                room,
                sid,
                "add_entity",
                json!({"kind": "promise", "handle": "return-the-wrench",
                       "name": "Claim the refund", "source": "user-named",
                       "parent": "person:bart"}),
            )
            .await;
            note(
                room,
                sid,
                "promise:return-the-wrench",
                "Claim the refund before the window closes.",
                json!({"sets": {"promised_by": "2026-12-01", "regarding": "thing:tau"}}),
            )
            .await
        }
        Ticket::OnThePerson => {
            note(
                room,
                sid,
                "person:bart",
                "Bought tickets for the show on 2026-11-14.",
                json!({"sets": {"runs_out": "2026-12-01"}}),
            )
            .await
        }
        Ticket::InventedKey => {
            note(
                room,
                sid,
                "thing:tau",
                "The refund window closes on 2026-12-01.",
                json!({"sets": {"refund_by": "2026-12-01"}}),
            )
            .await
        }
        Ticket::InAProse => {
            note(
                room,
                sid,
                "thing:tau",
                "The refund window closes on 2026-12-01.",
                json!({}),
            )
            .await
        }
    }

    // The furnace, and the day its free service lapses.
    match sitting.furnace {
        Furnace::OnTheMachine => {
            note(
                room,
                sid,
                "thing:the-furnace",
                "The free yearly service lapses unless it is used before 1 February 2027.",
                json!({"sets": {"runs_out": "2027-01-31"}}),
            )
            .await
        }
        Furnace::OnTheCompany => {
            note(
                room,
                sid,
                "org:mr-plow",
                "Serviced the furnace. The free yearly service lapses before 1 February 2027.",
                json!({"sets": {"runs_out": "2027-01-31"}}),
            )
            .await
        }
        Furnace::InventedKey => {
            note(
                room,
                sid,
                "thing:the-furnace",
                "The free yearly service lapses unless it is used before 1 February 2027.",
                json!({"sets": {"free_service_by": "2027-01-31"}}),
            )
            .await
        }
        Furnace::InAProse => {
            note(
                room,
                sid,
                "thing:the-furnace",
                "The free yearly service lapses unless it is used before 1 February 2027.",
                json!({}),
            )
            .await
        }
    }

    // The fair's certificate, and the course it comes from.
    note(
        room,
        sid,
        "event:wagstaff-fair",
        "Whoever runs the stall has to hold a food-handling certificate.",
        json!({}),
    )
    .await;
    note(
        room,
        sid,
        "org:quahog-community-college",
        "Runs a food-handling course over two evenings; an intake starts on the first Tuesday of each month.",
        json!({}),
    )
    .await;
    if sitting.fair == Fair::PointsAtTheCollege {
        note(
            room,
            sid,
            "event:wagstaff-fair",
            "The certificate for the stall comes from the college's food-handling course.",
            json!({"refs": ["org:quahog-community-college"]}),
        )
        .await
    }

    // The donuts.
    let (said_by, worked_by, recap_by) = match sitting.donuts {
        Donuts::Apart => ("testimony", "inference", "inference"),
        Donuts::AllDefault => ("inference", "inference", "inference"),
        Donuts::RecapAsTestimony => ("testimony", "inference", "testimony"),
        Donuts::EachNeverRecorded | Donuts::SpelledOut => ("testimony", "inference", "inference"),
    };
    let operator_said = match sitting.donuts {
        Donuts::EachNeverRecorded => "Twelve people are coming.",
        _ => "Twelve people are coming and each of them eats three donuts.",
    };
    let (worked, recap) = match sitting.donuts {
        Donuts::SpelledOut => (
            "Thirty six donuts to buy.",
            "Recap: twelve people at three donuts each, so thirty six donuts to buy.",
        ),
        _ => (
            "36 donuts to buy.",
            "Recap: twelve people at three donuts each, so 36 donuts to buy.",
        ),
    };
    for (content, provenance) in [
        (operator_said, said_by),
        (worked, worked_by),
        (recap, recap_by),
    ] {
        let said = as_the_occupant(
            room,
            sid,
            "capture",
            json!({"subject": "event:birthday-party", "content": content,
                   "provenance": provenance}),
        )
        .await;
        assert!(!said.contains("\"status\":\"blocked\""), "{said}");
    }

    // The standing rules, the morning one first.
    let rules = [
        "Nothing before 10am, for anything set up for the operator.",
        "Keep every reply short.",
        "Never book anything on a Sunday.",
        "Ask before spending anything.",
        "Say the day of the week whenever a date is given.",
        "Write times in the twenty-four hour clock.",
        "Give one option, not three.",
    ];
    for (at, rule) in rules.iter().enumerate() {
        let starred = match sitting.rule {
            Rule::FirstOnly => at == 0,
            Rule::Every => true,
            Rule::Nothing => false,
        };
        if !starred {
            note(room, sid, "bot:assistant", rule, json!({})).await;
            continue;
        }
        let args = json!({"subject": "bot:assistant", "content": rule,
                          "provenance": "testimony", "fields": {"starred": "true"}});
        let said: Value =
            serde_json::from_str(&as_the_occupant(room, sid, "capture", args.clone()).await)
                .expect("a capture answer");
        if said["status"] != "blocked" {
            continue;
        }
        // **A star into full seats is refused (rule 591)**: this sitting gives
        // up the oldest seated rule to let the new one in.
        let oldest = said["seated"][0]["address"]
            .as_str()
            .unwrap_or_else(|| panic!("the refusal names the seated rules: {said}"));
        as_the_occupant(
            room,
            sid,
            "update_fact",
            json!({"address": oldest, "fields": {"starred": "false"}}),
        )
        .await;
        let again = as_the_occupant(room, sid, "capture", args).await;
        assert!(!again.contains("\"status\":\"blocked\""), "{again}");
    }

    // The helper, and the hold.
    as_the_occupant(
        room,
        sid,
        "add_entity",
        json!({"kind": "bot", "handle": "gamma", "name": "Gamma", "source": "the operator"}),
    )
    .await;
    note(
        room,
        sid,
        "bot:gamma",
        "For the reading: power loss, and consensus.",
        json!({}),
    )
    .await;
    match sitting.hold {
        Hold::OnTheHelper => {
            note(
                room,
                sid,
                "bot:gamma",
                "Stay off the espresso manual until told otherwise.",
                json!({"fields": {"starred": "true"}}),
            )
            .await
        }
        Hold::NotCarried => {
            note(
                room,
                sid,
                "bot:gamma",
                "Stay off the espresso manual until told otherwise.",
                json!({}),
            )
            .await
        }
        Hold::InTheRulesDetails => {
            note(
                room,
                sid,
                "bot:gamma",
                "A standing rule for the reading.",
                json!({"fields": {"starred": "true"},
                       "details": "Stay off the espresso manual until told otherwise."}),
            )
            .await
        }
        Hold::InCharter => {
            let said = as_the_occupant(
                room,
                sid,
                "set_charter",
                json!({"bot": "gamma",
                       "prose": "You read what the operator does not get to. Stay off the espresso manual until told otherwise."}),
            )
            .await;
            assert!(!said.contains("\"status\":\"blocked\""), "{said}");
        }
        Hold::HandedTheJob => {
            note(
                room,
                sid,
                "bot:gamma",
                "Your reading: power loss, consensus, and the espresso manual.",
                json!({"fields": {"starred": "true"}}),
            )
            .await
        }
        Hold::OnTheAssistant => {
            note(
                room,
                sid,
                "bot:assistant",
                "the second assistant is to stay off the espresso manual",
                json!({}),
            )
            .await
        }
        Hold::MessageOnly => {
            let said = as_the_occupant(
                room,
                sid,
                "post_message",
                json!({"to": "gamma", "subject": "a hold",
                       "body": "Stay off the espresso manual until told otherwise."}),
            )
            .await;
            assert!(!said.contains("\"status\":\"blocked\""), "{said}");
        }
    }
}

/// The outcomes after one first sitting.
async fn outcomes_after(sitting: Sitting) -> Vec<Outcome> {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, sitting).await;
    let boundaries = around_the_brief(&surface).await;
    judge_all(&surface, &boundaries).await
}

/// Which locks held, in the room's order, and what they said.
async fn held_after(sitting: Sitting) -> (Vec<bool>, String) {
    let outcomes = outcomes_after(sitting).await;
    (outcomes.iter().map(|o| o.held).collect(), saying(&outcomes))
}

/// **Every lock held except these.**
fn all_but(reds: &[usize]) -> Vec<bool> {
    (0..LOCKS).map(|at| !reds.contains(&at)).collect()
}

// ── The shape of the room ────────────────────────────────────────────────────

/// **A brief told in turns and a cold reader, each turn one line, asking for no
/// report.** The entry is the first line of the first turn.
#[test]
fn the_room_is_a_brief_told_in_turns_and_a_cold_reader() {
    let room = document();
    assert_eq!(
        room.phases.len(),
        2,
        "{:?}",
        room.phases.iter().map(|p| &p.name).collect::<Vec<_>>()
    );
    for phase in &room.phases {
        assert!(
            phase.fresh_session,
            "{}: every sitting arrives cold",
            phase.name
        );
        for delivery in &phase.deliveries {
            assert_eq!(delivery.lines().count(), 1, "{}: {delivery:?}", phase.name);
        }
        for asked in ["PASS", "FAIL", "PARTIAL", "Report"] {
            assert!(
                !phase.prompt.contains(asked),
                "{}: asks for a verdict",
                phase.name
            );
        }
    }
}

/// **The entries name no verb, no key and no place to look.**
#[tokio::test]
async fn the_entries_name_no_verb_no_key_and_no_place() {
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
            "snooze",
            "snoozed_until",
            "promise",
            "promised_by",
            "runs_out",
            "due_on",
            "starred",
            "charter",
            "provenance",
            "testimony",
            "inference",
            "attendance",
            "edge",
            "archive",
            "retract",
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

/// **Sixteen locks, all in the first phase.**
#[test]
fn the_room_has_sixteen_locks_in_the_first_sitting() {
    let checks = expectations::for_playbook(TRICK_ROOM).expect("the room has expectations");
    let names: Vec<String> = checks.iter().map(|c| c.name().to_string()).collect();
    assert_eq!(names.len(), LOCKS, "{names:?}");
    assert!(names.iter().all(|n| n.starts_with("Phase 1")), "{names:?}");
}

/// **The room is not the default.**
#[test]
fn the_trick_room_is_shipped_and_is_not_the_default() {
    assert!(expectations::shipped_rooms().any(|room| room == TRICK_ROOM));
    assert_ne!(expectations::default_room(), TRICK_ROOM);
}

/// **A room nobody touched.** The locks that need something written fail. The
/// locks asking what is NOT owed before a day hold on nothing at all, and each
/// rests on a loop that is there, so the unworked room cannot read as a pause.
#[tokio::test]
async fn an_untouched_room_holds_only_the_locks_that_ask_for_an_absence() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries = around_the_brief(&surface).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    // The fern is owed before the day, so the first fern lock reds. Five locks ask
    // for an absence and hold on nothing: FERN_AFTER, CHAIRS_BEFORE, DESK_BEFORE,
    // NOT_ON_BART and NOT_ON_MR_PLOW. Each rests on a loop that is there. Every
    // other lock needs something written, so it reds.
    assert_eq!(
        held,
        all_but(&[
            FERN_BEFORE,
            CHAIRS_AFTER,
            MAUDE,
            DESK_AFTER,
            TICKET_OWED,
            NUMBERS,
            COUNT,
            RULE,
            HOLD,
            FURNACE_OWED,
            FAIR
        ]),
        "{}",
        saying(&outcomes)
    );
}

// ── Solvability ──────────────────────────────────────────────────────────────

/// **Solvable: the whole room, every fact where its reader looks.**
#[tokio::test]
async fn solvable_every_lock_holds_when_every_fact_is_where_its_reader_looks() {
    let (held, said) = held_after(right()).await;
    assert_eq!(held, vec![true; LOCKS], "{said}");
}

/// **Solvable: the pause on a loop that exists.** The question asked on a day
/// before the operator is back and on a day after.
#[tokio::test]
async fn solvable_the_fern_is_out_of_the_owed_answer_until_the_day_and_in_it_after() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let before = surface
        .call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": "2026-11-12"}}),
        )
        .await;
    assert!(before.contains("rhythm:swap-the-air-filter"), "{before}");
    assert!(!before.contains("rhythm:water-the-fern"), "{before}");
    let after = surface
        .call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": "2026-11-21"}}),
        )
        .await;
    assert!(after.contains("rhythm:water-the-fern"), "{after}");
}

/// **Solvable: the pause on a request with no loop behind it.**
#[tokio::test]
async fn solvable_the_chairs_photos_are_not_owed_until_the_day_and_are_after() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let owed = |day: &'static str| {
        surface.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": day}}),
        )
    };
    let before = owed("2026-11-12").await;
    assert!(before.contains("rhythm:swap-the-air-filter"), "{before}");
    assert!(!before.contains("thing:folding-chairs"), "{before}");
    assert!(owed("2026-11-21").await.contains("thing:folding-chairs"));
}

/// **Solvable: who has said yes is a walk, and Maude is on it alone.**
#[tokio::test]
async fn solvable_the_walk_to_who_is_attending_lists_maude_and_nobody_else() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let walked = surface
        .call(
            "search",
            json!({"query": "*", "kind": "person",
                   "edge": {"shape": "attendance", "object": "event:birthday-party"}}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&walked).expect("json");
    let subjects: Vec<_> = parsed["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter_map(jojobot_exercise::isolate::fact_subject)
        .collect();
    assert!(subjects.contains(&"person:maude"), "{walked}");
    for still_to_ask in ["person:homer", "person:ned-flanders"] {
        assert!(
            !subjects.contains(&still_to_ask),
            "{still_to_ask} is listed as attending: {walked}"
        );
    }
}

/// **Solvable: the warranty is owed on the corrected day and not the first.**
#[tokio::test]
async fn solvable_the_desk_is_owed_on_the_corrected_day_and_not_the_first() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let owed = |day: &'static str| {
        surface.call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": day}}),
        )
    };
    assert!(!owed("2026-12-05").await.contains("standing-desk"));
    assert!(owed("2026-12-11").await.contains("standing-desk"));
}

/// **Solvable: which numbers the operator gave and which were worked out is
/// read off the backing.**
#[tokio::test]
async fn solvable_the_backing_tells_what_was_said_from_what_was_worked_out() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let said = surface
        .call(
            "search",
            json!({"query": "*", "provenance": "testimony", "limit": 200}),
        )
        .await;
    assert!(said.contains("Twelve people are coming"), "{said}");
    assert!(!said.contains("36 donuts"), "{said}");
    let worked = surface
        .call(
            "search",
            json!({"query": "*", "provenance": "inference", "limit": 200}),
        )
        .await;
    assert!(worked.contains("36 donuts to buy"), "{worked}");
}

/// **Solvable: the boot hands a later session the first rule.**
#[tokio::test]
async fn solvable_the_boot_carries_the_first_standing_rule() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let boot = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let parsed: Value = serde_json::from_str(&boot).expect("a boot answer");
    let carried: Vec<&str> = parsed["identity"]["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .filter_map(|rule| rule["content"].as_str())
        .collect();
    assert!(
        carried.iter().any(|rule| rule.contains("10am")),
        "{carried:?}"
    );
}

/// **Solvable: a read of the helper says what it is held off.**
#[tokio::test]
async fn solvable_a_read_of_the_helper_says_what_it_is_held_off() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let read = surface
        .call("recall", json!({"subject": "bot:gamma", "facts": true}))
        .await;
    assert!(read.contains("espresso"), "{read}");
}

/// **Solvable: a hold written in the details of a carried rule is a hold.** The
/// rule's words say nothing of the job, so the lock can only see the line in the
/// details the world snapshot carries.
#[tokio::test]
async fn solvable_a_hold_in_the_details_of_a_carried_rule_holds_the_lock() {
    let (held, said) = held_after(Sitting {
        hold: Hold::InTheRulesDetails,
        ..right()
    })
    .await;
    assert_eq!(held, vec![true; LOCKS], "{said}");
}

/// **Solvable: a line in the helper's charter is the other right home.**
#[tokio::test]
async fn solvable_the_hold_in_the_helpers_charter_holds_the_lock() {
    let (held, said) = held_after(Sitting {
        hold: Hold::InCharter,
        ..right()
    })
    .await;
    assert_eq!(held, vec![true; LOCKS], "{said}");
}

/// **Solvable from what a model reads: a star into full seats is refused,
/// naming the seated rules (rule 591).** The sixth marked rule is refused with
/// nothing written, the refusal names the five that hold the seats, the first
/// among them, and the boot still carries the first. Starring everything cannot
/// lose the first rule silently.
#[tokio::test]
async fn solvable_a_sixth_marked_rule_is_refused_naming_the_seated_ones() {
    let (_room, surface, sid) = furnished().await;
    let mut receipts = Vec::new();
    for at in 0..6 {
        let said = as_the_occupant(
            &surface,
            &sid,
            "capture",
            json!({"subject": "bot:assistant", "content": format!("Standing rule number {at}."),
                   "provenance": "testimony", "fields": {"starred": "true"}}),
        )
        .await;
        receipts.push(serde_json::from_str::<Value>(&said).expect("a receipt"));
    }
    let first = receipts[0]["address"]
        .as_str()
        .expect("an address")
        .to_string();
    assert_ne!(receipts[4]["status"], "blocked", "{}", receipts[4]);
    assert_eq!(receipts[5]["status"], "blocked", "{}", receipts[5]);
    assert!(
        receipts[5]["seated"]
            .as_array()
            .expect("the seated rules")
            .iter()
            .any(|rule| rule["address"].as_str() == Some(first.as_str())),
        "{}",
        receipts[5]
    );
    let boot = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let boot: Value = serde_json::from_str(&boot).expect("a boot answer");
    let carried = boot["identity"]["rules"].as_array().expect("rules");
    assert!(
        carried
            .iter()
            .any(|rule| rule["address"].as_str() == Some(first.as_str())),
        "the first rule rides: {carried:?}"
    );
}

// ── Discrimination: the convenient wrong place IS the break ──────────────────

/// **A note on the assistant's own record is not a pause on the fern.** Only
/// the lock for the day before reds: the loop is still owed, and after the day
/// it is owed anyway.
#[tokio::test]
async fn the_fern_lock_reds_when_the_pause_is_a_note_and_not_on_the_loop() {
    let (held, said) = held_after(Sitting {
        fern: Fern::NoteOnly,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[FERN_BEFORE]), "{said}");
}

/// **A loop taken out for good is not put off.** The lock for the day after
/// reds, and the one for the day before holds, so a pause that simply deleted
/// the loop does not read as a good one.
#[tokio::test]
async fn the_fern_lock_reds_when_the_loop_is_taken_out_for_good() {
    let (held, said) = held_after(Sitting {
        fern: Fern::Archived,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[FERN_AFTER]), "{said}");
}

/// **A note is not a pause on the folding chairs.** Nothing falls due on the
/// day, so the lock for the day after reds.
#[tokio::test]
async fn the_chairs_lock_reds_when_the_pause_is_a_note_that_never_falls_due() {
    let (held, said) = held_after(Sitting {
        chairs: Chairs::NoteOnly,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[CHAIRS_AFTER]), "{said}");
}

/// **A promise owed from the first day is a reminder.** The lock for the day
/// before reds.
#[tokio::test]
async fn the_chairs_lock_reds_when_the_promise_is_owed_before_the_day() {
    let (held, said) = held_after(Sitting {
        chairs: Chairs::PromiseFromNow,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[CHAIRS_BEFORE]), "{said}");
}

/// **Maude's yes in a note is not attendance.**
#[tokio::test]
async fn the_maude_lock_reds_when_the_yes_is_a_note_to_self() {
    let (held, said) = held_after(Sitting {
        maude: Maude::NoteOnly,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[MAUDE]), "{said}");
}

/// **Maude's yes as a claim on the party with no edge is not attendance
/// either.**
#[tokio::test]
async fn the_maude_lock_reds_when_the_yes_is_prose_on_the_party() {
    let (held, said) = held_after(Sitting {
        maude: Maude::ProseOnTheParty,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[MAUDE]), "{said}");
}

/// **A correction beside the first claim leaves the warranty on the first
/// day.**
#[tokio::test]
async fn the_desk_lock_reds_when_the_correction_sits_beside_the_first_day() {
    let (held, said) = held_after(Sitting {
        desk: Desk::Beside,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[DESK_BEFORE]), "{said}");
}

/// **A day under a key nothing reads is never owed.** The lock for the day
/// after reds, and the one for the day before holds because nothing is owed
/// then either.
#[tokio::test]
async fn the_desk_lock_reds_when_the_day_is_under_a_key_of_the_models_own() {
    let (held, said) = held_after(Sitting {
        desk: Desk::InventedKey,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[DESK_AFTER]), "{said}");
}

/// **A deadline on the person who bought the tickets is owed as the person.**
/// The lock that asks whether the window is owed holds, because something
/// carries the day, and the one that asks that the person is not what falls due
/// reds. Paired with the solvable cases, which hold both.
#[tokio::test]
async fn the_ticket_lock_reds_when_the_deadline_is_on_the_person() {
    let (held, said) = held_after(Sitting {
        ticket: Ticket::OnThePerson,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[NOT_ON_BART]), "{said}");
}

/// **A day under a key nothing reads is never owed**, and a day in a sentence
/// is not either. Either way the window is lost: the lock that asks whether it
/// is owed reds, and the one about the person holds because nobody carries it.
#[tokio::test]
async fn the_ticket_lock_reds_when_the_day_is_under_a_key_of_the_models_own() {
    let (held, said) = held_after(Sitting {
        ticket: Ticket::InventedKey,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[TICKET_OWED]), "{said}");
}

#[tokio::test]
async fn the_ticket_lock_reds_when_the_day_is_only_in_a_sentence() {
    let (held, said) = held_after(Sitting {
        ticket: Ticket::InAProse,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[TICKET_OWED]), "{said}");
}

/// **Solvable: a promise regarding the tickets is the other right home.** The
/// lock reads the outcome and not the route, so it holds.
#[tokio::test]
async fn solvable_a_promise_regarding_the_tickets_holds_both_ticket_locks() {
    let (held, said) = held_after(Sitting {
        ticket: Ticket::AsAPromise,
        ..right()
    })
    .await;
    assert_eq!(held, vec![true; LOCKS], "{said}");
}

/// **Solvable: the tickets are owed after the day and the person never is.**
#[tokio::test]
async fn solvable_the_window_is_owed_on_the_tickets_after_it_closes_and_the_person_is_not() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let owed = surface
        .call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2026-12-02"}}),
        )
        .await;
    assert!(owed.contains("\"id\":\"thing:tau\""), "{owed}");
    assert!(!owed.contains("\"id\":\"person:bart\""), "{owed}");
}

/// **The furnace's lapse day filed on the company that serviced it.** The day is
/// owed, so the lock that asks whether anything owes it holds, and the one that
/// asks that the company is not what falls due reds.
#[tokio::test]
async fn the_furnace_lock_reds_when_the_lapse_day_is_on_the_company() {
    let (held, said) = held_after(Sitting {
        furnace: Furnace::OnTheCompany,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[NOT_ON_MR_PLOW]), "{said}");
}

/// **A lapse day under a key nothing reads, or only in a sentence, is never
/// owed.** The lock that asks whether it is owed reds, and the one about the
/// company holds because nobody carries it.
#[tokio::test]
async fn the_furnace_lock_reds_when_the_day_is_under_a_key_of_the_models_own() {
    let (held, said) = held_after(Sitting {
        furnace: Furnace::InventedKey,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[FURNACE_OWED]), "{said}");
}

#[tokio::test]
async fn the_furnace_lock_reds_when_the_day_is_only_in_a_sentence() {
    let (held, said) = held_after(Sitting {
        furnace: Furnace::InAProse,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[FURNACE_OWED]), "{said}");
}

/// **Solvable: the furnace is owed after its free service lapses and the company
/// never is.**
#[tokio::test]
async fn solvable_the_furnace_is_owed_after_its_free_service_lapses_and_the_company_is_not() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let owed = surface
        .call(
            "recall",
            json!({"fields": [{"key": "due_on"}], "overdue": {"as_of": "2027-02-02"}}),
        )
        .await;
    assert!(owed.contains("\"id\":\"thing:the-furnace\""), "{owed}");
    assert!(!owed.contains("\"id\":\"org:mr-plow\""), "{owed}");
}

/// **The course filed on the college, and nothing on the fair points at it.** The
/// question of where the stall's certificate comes from is asked of the fair, so
/// the fair lock reds and nothing else does.
#[tokio::test]
async fn the_fair_lock_reds_when_the_course_is_only_on_the_college() {
    let (held, said) = held_after(Sitting {
        fair: Fair::OnlyOnTheCollege,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[FAIR]), "{said}");
}

/// **Solvable: a read of the fair walks to the college.**
#[tokio::test]
async fn solvable_a_read_of_the_fair_names_the_college_the_certificate_comes_from() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let fair = surface
        .call(
            "recall",
            json!({"subject": "event:wagstaff-fair", "facts": true}),
        )
        .await;
    assert!(fair.contains("org:quahog-community-college"), "{fair}");
}

/// **The operator's numbers on the default backing read back as a guess.**
#[tokio::test]
async fn the_numbers_lock_reds_when_the_operators_words_are_left_on_the_default() {
    let (held, said) = held_after(Sitting {
        donuts: Donuts::AllDefault,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[NUMBERS]), "{said}");
}

/// **A "three" that is not about the donuts is not the operator's number.**
/// The standing rule "give me one option, not three" is the operator's word and
/// carries the token. The lock asked for any such claim, so it held on a room
/// where the donuts each were never written down as theirs.
#[tokio::test]
async fn the_numbers_lock_reds_when_only_a_rule_about_something_else_says_three() {
    let (held, said) = held_after(Sitting {
        donuts: Donuts::EachNeverRecorded,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[NUMBERS]), "{said}");
}

/// **A count written in words with a space still counts as stated.** The
/// count lock read "36", "thirty-six" and "three dozen", so a right answer
/// written "thirty six" left no claim stating the count and the lock reddened
/// on a correct sitting.
#[tokio::test]
async fn the_count_lock_holds_when_the_count_is_written_thirty_six_with_a_space() {
    let (held, said) = held_after(Sitting {
        donuts: Donuts::SpelledOut,
        ..right()
    })
    .await;
    assert_eq!(held, vec![true; LOCKS], "{said}");
}

/// **A recap that restates the worked-out count as the operator's word.**
#[tokio::test]
async fn the_count_lock_reds_when_the_recap_files_the_count_as_the_operators_word() {
    let (held, said) = held_after(Sitting {
        donuts: Donuts::RecapAsTestimony,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[COUNT]), "{said}");
}

/// **Every rule starred is more than the boot has seats for.** Each refused
/// star is let in by unstarring the oldest seated rule, so the first is the one
/// that goes.
#[tokio::test]
async fn the_rule_lock_reds_when_every_rule_is_starred_and_the_first_is_given_up() {
    let (held, said) = held_after(Sitting {
        rule: Rule::Every,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[RULE]), "{said}");
}

/// **A rule written and not carried is not handed to the next session.**
#[tokio::test]
async fn the_rule_lock_reds_when_no_rule_is_carried() {
    let (held, said) = held_after(Sitting {
        rule: Rule::Nothing,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[RULE]), "{said}");
}

/// **A hold on the assistant's own record is not on the helper.**
#[tokio::test]
async fn the_hold_lock_reds_when_the_hold_is_a_note_on_the_assistant() {
    let (held, said) = held_after(Sitting {
        hold: Hold::OnTheAssistant,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[HOLD]), "{said}");
}

/// **A claim on the helper that is not marked to ride its boot is not what its
/// boot carries.**
#[tokio::test]
async fn the_hold_lock_reds_when_the_claim_on_the_helper_is_not_carried() {
    let (held, said) = held_after(Sitting {
        hold: Hold::NotCarried,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[HOLD]), "{said}");
}

/// **A job handed to the helper is not a hold on it.** The rule names the
/// espresso manual and rides the helper's boot, and it says nothing that keeps
/// the helper off the job.
#[tokio::test]
async fn the_hold_lock_reds_when_the_helper_is_handed_the_job_and_nothing_holds_it_back() {
    let (held, said) = held_after(Sitting {
        hold: Hold::HandedTheJob,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[HOLD]), "{said}");
}

/// **A hold written after the brief ended is not a hold the brief wrote.** The
/// brief leaves the hold as a message and nothing on the helper. A charter line
/// is written after that, as a cold reader that found the message would write
/// it. The charter holds the job when the room is read live, and the lock
/// still reds, because it reads the room as the brief left it.
#[tokio::test]
async fn a_hold_written_after_the_brief_does_not_satisfy_the_hold_lock() {
    let (_room, surface, sid) = furnished().await;
    played(
        &surface,
        &sid,
        Sitting {
            hold: Hold::MessageOnly,
            ..right()
        },
    )
    .await;
    let boundaries = around_the_brief(&surface).await;
    let said = as_the_occupant(
        &surface,
        &sid,
        "set_charter",
        json!({"bot": "gamma",
               "prose": "You read what the operator does not get to. Stay off the espresso manual until told otherwise."}),
    )
    .await;
    assert!(!said.contains("\"status\":\"blocked\""), "{said}");
    let live = surface
        .call("recall", json!({"subject": "bot:gamma", "charter": true}))
        .await;
    assert!(live.contains("espresso"), "{live}");
    let outcomes = judge_all(&surface, &boundaries).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(held, all_but(&[HOLD]), "{}", saying(&outcomes));
}

/// **A hold sent as a message and nowhere on the helper is not on the helper.**
#[tokio::test]
async fn the_hold_lock_reds_when_the_hold_is_only_a_message() {
    let (held, said) = held_after(Sitting {
        hold: Hold::MessageOnly,
        ..right()
    })
    .await;
    assert_eq!(held, all_but(&[HOLD]), "{said}");
}

/// **A cold sitting that repairs a misplay cannot hide it from a lock scoped to
/// the end of the first sitting.** Maude's yes is a note when the brief ends. The
/// edge is drawn after that, as a cold reader that found the note would draw it.
/// The edge is there when the room is read live, and the lock still reds,
/// because it reads the room as the brief left it.
#[tokio::test]
async fn a_repair_after_the_brief_does_not_hide_a_misplay_from_the_scoped_lock() {
    let (_room, surface, sid) = furnished().await;
    played(
        &surface,
        &sid,
        Sitting {
            maude: Maude::NoteOnly,
            ..right()
        },
    )
    .await;
    let boundaries = around_the_brief(&surface).await;
    note(
        &surface,
        &sid,
        "person:maude",
        "Said yes to the birthday party.",
        json!({"shape": "attendance", "object": "event:birthday-party"}),
    )
    .await;
    let live = surface
        .call(
            "search",
            json!({"query": "*", "kind": "person",
                   "edge": {"shape": "attendance", "object": "event:birthday-party"}}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&live).expect("json");
    assert!(
        parsed["results"]
            .as_array()
            .expect("results")
            .iter()
            .filter_map(jojobot_exercise::isolate::fact_subject)
            .any(|subject| subject == "person:maude"),
        "{live}"
    );
    let outcomes = judge_all(&surface, &boundaries).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(held, all_but(&[MAUDE]), "{}", saying(&outcomes));
}

/// **A cold sitting that repairs a pause cannot hide it from a lock that asks
/// at the end of the brief.** The fern's pause is a note when the brief ends. The
/// snooze is written after that. Read live, the fern is out of the owed answer
/// before the day; the lock for the day before still reds, because it holds the
/// answer the brief left.
#[tokio::test]
async fn a_repair_after_the_brief_does_not_hide_a_pause_that_missed_the_loop() {
    let (_room, surface, sid) = furnished().await;
    played(
        &surface,
        &sid,
        Sitting {
            fern: Fern::NoteOnly,
            ..right()
        },
    )
    .await;
    let boundaries = around_the_brief(&surface).await;
    note(
        &surface,
        &sid,
        "rhythm:water-the-fern",
        "away until the twentieth",
        json!({"check_in": "snoozed", "sets": {"snoozed_until": "2026-11-20"}}),
    )
    .await;
    let live = surface
        .call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": "2026-11-12"}}),
        )
        .await;
    assert!(!live.contains("rhythm:water-the-fern"), "{live}");
    let outcomes = judge_all(&surface, &boundaries).await;
    let held: Vec<bool> = outcomes.iter().map(|o| o.held).collect();
    assert_eq!(held, all_but(&[FERN_BEFORE]), "{}", saying(&outcomes));
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
///
/// Run against the room worked in full, so a needle that matches nowhere is a
/// lock failing or a walk that could not read the answer.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface, sid) = furnished().await;
    played(&surface, &sid, right()).await;
    let boundaries = around_the_brief(&surface).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(TRICK_ROOM),
        &boundaries,
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

/// **The room's server runs on the day the room gave it, and never on the wall
/// clock.** `ping` says which day the server is acting out. A server on the real
/// clock says none, so this fails on every day if the pin is missing.
#[tokio::test]
async fn the_rooms_server_runs_on_the_day_the_room_gave_it() {
    let (_room, surface, _sid) = furnished().await;
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
        &jojobot_exercise::lock::locks_of(TRICK_ROOM),
        SERVER_DAY.as_str(),
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}
