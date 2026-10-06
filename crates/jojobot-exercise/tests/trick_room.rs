//! **The trick room, held for free.**
//!
//! Nothing here drives a model. Each lock is run against a real room whose
//! state is put there by hand, through the verbs, the way an occupant would.
//!
//! **Eleven locks, and each is proven twice.** A first sitting that wrote every
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

fn document() -> Playbook {
    Playbook::read(&expectations::room_document(TRICK_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it, and a session to write into it
/// as the occupant would.
async fn furnished() -> (Room, Surface, String) {
    let (handle, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
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
const MAUDE: usize = 6;
const NUMBERS: usize = 7;
const COUNT: usize = 8;
const RULE: usize = 9;
const HOLD: usize = 10;
const LOCKS: usize = 11;

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
}

/// **Where the first standing rule goes.**
#[derive(Clone, Copy, PartialEq, Eq)]
enum Rule {
    /// Only the first rule carried to the boot.
    FirstOnly,
    /// Every rule carried to the boot, which is more than it has seats for.
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
}

/// **One first sitting, every choice named.** `right()` is the sitting that
/// wrote each fact where its reader looks; a case changes exactly one field.
#[derive(Clone, Copy)]
struct Sitting {
    fern: Fern,
    chairs: Chairs,
    maude: Maude,
    desk: Desk,
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
                json!({"check_in": "snoozed", "fields": {"snoozed_until": "2026-11-20"}}),
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
            json!({"fields": {"promised_by": day, "regarding": "thing:folding-chairs"}}),
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
        json!({"fields": {key: "2026-12-03"}}),
    )
    .await;
    match sitting.desk {
        Desk::Corrected | Desk::InventedKey => {
            note(
                room,
                sid,
                "thing:standing-desk",
                "The warranty runs out, corrected.",
                json!({"fields": {key: "2026-12-10"}}),
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

    // The donuts.
    let (said_by, worked_by, recap_by) = match sitting.donuts {
        Donuts::Apart => ("testimony", "inference", "inference"),
        Donuts::AllDefault => ("inference", "inference", "inference"),
        Donuts::RecapAsTestimony => ("testimony", "inference", "testimony"),
    };
    for (content, provenance) in [
        (
            "Twelve people are coming and each of them eats three donuts.",
            said_by,
        ),
        ("36 donuts to buy.", worked_by),
        (
            "Recap: twelve people at three donuts each, so 36 donuts to buy.",
            recap_by,
        ),
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
        let extra = if starred {
            json!({"fields": {"starred": "true"}})
        } else {
            json!({})
        };
        note(room, sid, "bot:assistant", rule, extra).await;
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

/// **Eleven locks, all in the first phase.**
#[test]
fn the_room_has_eleven_locks_in_the_first_sitting() {
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
    // The fern is owed before the day, so the first fern lock reds. The three
    // other absences hold on nothing, and each rests on a loop that is there.
    assert_eq!(
        held,
        all_but(&[
            FERN_BEFORE,
            CHAIRS_AFTER,
            MAUDE,
            DESK_AFTER,
            NUMBERS,
            COUNT,
            RULE,
            HOLD
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
    assert!(walked.contains("\"subject\":\"person:maude\""), "{walked}");
    for still_to_ask in ["person:homer", "person:ned-flanders"] {
        assert!(
            !walked.contains(&format!("\"subject\":\"{still_to_ask}\"")),
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

/// **Solvable from what a model reads: the surface says the oldest rule was
/// pushed off.** The write that takes the sixth marked rule over the seats
/// answers with what was dropped, and the boot says how many seats there are.
/// Without both, starring everything and losing the first would be a trap
/// nothing on the surface explains.
#[tokio::test]
async fn solvable_the_surface_names_the_rule_a_sixth_marked_rule_pushes_off() {
    let (_room, surface, sid) = furnished().await;
    let mut receipts = Vec::new();
    for at in 0..7 {
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
    assert!(receipts[4]["seats"].is_null(), "{}", receipts[4]);
    assert_eq!(
        receipts[5]["seats"]["dropped"].as_str(),
        Some(first.as_str()),
        "{}",
        receipts[5]
    );
    let boot = surface
        .call("start_here", json!({"bot": "assistant", "brief": true}))
        .await;
    let boot: Value = serde_json::from_str(&boot).expect("a boot answer");
    assert!(
        boot["identity"]["rules_note"]
            .as_str()
            .is_some_and(|n| !n.is_empty()),
        "{}",
        boot["identity"]
    );
    let carried = boot["identity"]["rules"].as_array().expect("rules");
    assert!(
        carried
            .iter()
            .all(|rule| rule["address"].as_str() != Some(first.as_str())),
        "the first rule still rides: {carried:?}"
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

/// **Every rule carried is more than the boot has seats for.** The first is the
/// oldest, and it is the one that goes.
#[tokio::test]
async fn the_rule_lock_reds_when_every_rule_is_carried_and_the_first_is_pushed_off() {
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
    assert!(live.contains("\"subject\":\"person:maude\""), "{live}");
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
        json!({"check_in": "snoozed", "fields": {"snoozed_until": "2026-11-20"}}),
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
