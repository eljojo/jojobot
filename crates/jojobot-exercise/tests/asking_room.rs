//! **The asking room, held for free.**
//!
//! Nothing here drives a model. The year is played by hand, through the verbs,
//! the way a sitting that reached for the right things would play it, and then
//! each cold question is asked of the served surface. Two kinds of case, kept
//! apart by name, because they fail for different reasons and only one of them
//! looks like a defect in the product:
//!
//! * **`solvable_…`** — the year written properly, then one cold question asked
//!   through the surface and the answer checked to be exactly the one the room
//!   is built around. A room whose own arithmetic is wrong is unreachable by
//!   every session that ever enters it, the discrimination cases still pass,
//!   and the failure surfaces on a paid run reading as a defect in the product.
//! * **`an_asking_room_nobody_worked_in_fails_every_lock`** and
//!   **`every_lock_holds_once_the_year_is_worked`** — the locks discriminate: a
//!   room nobody put anything into holds none, and the year worked the way it is
//!   meant to be holds all.

use jojobot_exercise::expectations::{self, ASKING_ROOM};
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{
    Boundary, Observed, Outcome, boundary, boundary_asking, boundary_names, phase_end_queries,
};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// **The day this room's server runs on**, carried to the child as its own
/// clock. Every moment the server stamps is this day, whatever the real date is,
/// so no needle can be met by a stamp on one real day. It is a day no needle
/// of this room holds, and `no_needle_here_is_the_servers_own_day` asks that.
const SERVER_DAY: &str = "2026-12-02";

/// The document a run is driven by, read from the root of the workspace.
#[allow(dead_code)]
fn room_document() -> Playbook {
    Playbook::read(&expectations::room_document(ASKING_ROOM))
        .unwrap_or_else(|e| panic!("the shipped room must read: {e:#}"))
}

/// A room furnished the way a run furnishes it.
async fn furnished() -> (Room, Surface) {
    let (room, surface) =
        Room::open_with_client_stating(&server_binary().expect("a jojobot binary"), SERVER_DAY)
            .await
            .expect("a room");
    expectations::seed_for(ASKING_ROOM)
        .expect("the room has furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");
    (room, surface)
}

/// **A session for one sitting, in the day that sitting is in** — the same
/// shape `vault_room.rs` takes: each phase is a run of its own, on its own day.
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
         quiet in this run's frame: {working:?}",
    );
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

/// **A call the occupant would make, which must not be refused.** A year that
/// cannot be written is a room that cannot be solved, so a refusal here is the
/// case failing, not an answer to read.
async fn did(room: &Surface, sid: &str, verb: &str, mut args: Value) -> Value {
    args["sid"] = json!(sid);
    let text = room.call(verb, args.clone()).await;
    let body: Value = serde_json::from_str(&text)
        .unwrap_or_else(|e| panic!("{verb} answered something that is not json ({e}): {text}"));
    assert!(
        body["status"] != "blocked" && body["transport_error"].is_null(),
        "{verb} was refused, so the year could not be written: {text}\n  sent: {args}",
    );
    body
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

/// One ride, filed on the bike it was on, under the key the operator measures
/// by.
async fn ride(room: &Surface, sid: &str, bike: &str, km: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": format!("thing:{bike}"), "content": format!("rode {km} km"),
               "provenance": "testimony", "sets": {"km": km}}),
    )
    .await;
}

/// A routine done, recorded as a check-in on the day of the sitting.
async fn check_in(room: &Surface, sid: &str, routine: &str, day: &str, what: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": format!("rhythm:{routine}"), "content": what,
               "provenance": "testimony", "check_in": "ran", "recorded_at": day}),
    )
    .await;
}

/// A guest, recorded as attending the party.
async fn attending(room: &Surface, sid: &str, who: &str, said: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": format!("person:{who}"), "content": said,
               "provenance": "testimony", "shape": "attendance",
               "object": "event:goodbye-party"}),
    )
    .await;
}

/// A piece of the move, recorded under its status.
async fn part_of_the_move(room: &Surface, sid: &str, part: &str, status: &str, said: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": format!("work:{part}"), "content": said,
               "provenance": "testimony", "sets": {"status": status}}),
    )
    .await;
}

/// The beat a sitting leaves in its own chronology, and the close of it.
async fn journalled(room: &Surface, sid: &str, entry: &str, focus: &str) {
    did(
        room,
        sid,
        "journal",
        json!({"entry": entry, "focus": focus}),
    )
    .await;
    did(room, sid, "wrap_session", json!({"story": entry})).await;
}

/// January: the year begins. The word the operator uses once, a pair of rides,
/// a party floated, and what they always want to know about a shop visit.
async fn january(room: &Surface, sid: &str) {
    // "My bikes" is a question asked by name: a bike is anything categorised
    // as one that is still owned.
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "view", "handle": "my-bikes", "name": "My bikes",
               "source": "the operator", "sets": {"selects": "thing"}}),
    )
    .await;
    for (key, value) in [("category", "bike"), ("owned", "yes")] {
        did(
            room,
            sid,
            "capture",
            json!({"subject": "view:my-bikes", "content": format!("what makes a bike mine: {key} is {value}"),
                   "provenance": "testimony", "sets": {"key": key, "value": value}}),
        )
        .await;
    }
    for (bike, km) in [("gravel-bike", "62"), ("road-bike", "41")] {
        did(
            room,
            sid,
            "capture",
            json!({"subject": format!("thing:{bike}"), "content": format!("one of my bikes; rode {km} km"),
                   "provenance": "testimony",
                   "sets": {"category": "bike", "owned": "yes", "km": km}}),
        )
        .await;
    }
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "goodbye-party", "name": "The Goodbye Party",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:goodbye-party", "content": "a goodbye party at home on 13 June",
               "provenance": "testimony", "sets": {"planned_for": "2026-06-13"}}),
    )
    .await;
    journalled(
        room,
        sid,
        "2026-01-10: Started the year: a my bikes list to pull up any time, a goodbye party at home on \
         13 June, and for any bike shop visit who did the work and what they did.",
        "the year begins",
    )
    .await;
}

/// February: the gravel bike goes in, and the chain is oiled.
async fn february(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "chain-and-pads", "name": "Chain and brake pads",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:chain-and-pads",
               "content": "the gravel bike went into Costington's for a new chain and new brake pads",
               "provenance": "testimony",
               "sets": {"mechanic": "person:kenny", "work": "new chain and new brake pads",
                          "bike": "thing:gravel-bike", "shop": "org:costingtons"}}),
    )
    .await;
    for _ in 0..3 {
        ride(room, sid, "gravel-bike", "41").await;
    }
    check_in(room, sid, "oil-the-chain", "2026-02-21", "oiled the chain").await;
    journalled(
        room,
        sid,
        "2026-02-21: The gravel bike had a new chain and brake pads at Costington's, Kenny did the work. \
         Three rides of 41 km. Oiled the chain. The knee was sore after the long one.",
        "the gravel bike goes in",
    )
    .await;
}

/// March: the move is decided, and the movers quote.
async fn march(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "place", "handle": "quahog", "name": "Quahog", "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "project", "handle": "the-move", "name": "The Move",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "project:the-move",
               "content": "decided to move to Quahog by the end of October",
               "provenance": "testimony",
               "sets": {"status": "now", "destination": "place:quahog", "due_by": "2026-10-31"}}),
    )
    .await;
    for (part, name) in [
        ("the-lease", "The lease"),
        ("the-movers", "The movers"),
        ("the-vet-certificate", "Snowball's vet certificate"),
        ("the-insurance", "The insurance"),
        ("the-utilities", "The utilities"),
    ] {
        did(
            room,
            sid,
            "add_entity",
            json!({"kind": "work", "handle": part, "name": name,
                   "source": "the operator", "parent": "project:the-move"}),
        )
        .await;
    }
    part_of_the_move(room, sid, "the-lease", "done", "the lease is signed").await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "work:the-movers", "content": "the movers quoted 1,800",
               "provenance": "testimony", "sets": {"status": "next", "quote": "1800"}}),
    )
    .await;
    part_of_the_move(room, sid, "the-vet-certificate", "next", "still to do").await;
    part_of_the_move(room, sid, "the-insurance", "next", "still to do").await;
    part_of_the_move(room, sid, "the-utilities", "next", "still to do").await;
    for _ in 0..2 {
        ride(room, sid, "road-bike", "55").await;
    }
    check_in(room, sid, "call-randy", "2026-03-30", "called Randy").await;
    journalled(
        room,
        sid,
        "2026-03-30: Decided to move to Quahog by the end of October. The move is the lease, the movers, \
         Snowball's vet certificate, the insurance and the utilities. The lease is signed. The \
         movers quoted 1,800. Called Randy. Two rides on the road bike, 55 km each.",
        "the move is decided",
    )
    .await;
}

/// April: the guests say yes, and a bike is bought in passing.
async fn april(room: &Surface, sid: &str) {
    for who in ["stan", "kyle", "wendy", "token", "randy", "sharon"] {
        attending(room, sid, who, "said yes to the goodbye party").await;
    }
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "person", "handle": "plus-one", "name": "Butters's plus-one",
               "source": "the operator"}),
    )
    .await;
    attending(
        room,
        sid,
        "butters",
        "said yes to the goodbye party, and is bringing his partner",
    )
    .await;
    attending(
        room,
        sid,
        "plus-one",
        "is coming to the goodbye party with Butters",
    )
    .await;
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "thing", "handle": "fat-bike", "name": "The Fat Bike",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:fat-bike", "content": "bought a fat bike",
               "provenance": "testimony", "sets": {"category": "bike", "owned": "yes"}}),
    )
    .await;
    for _ in 0..4 {
        ride(room, sid, "gravel-bike", "36").await;
    }
    journalled(
        room,
        sid,
        "2026-04-26: Stan, Kyle, Wendy, Token, Randy and Sharon said yes to the party and Butters is bringing \
         his partner. Bought a fat bike. Four rides on the gravel bike, 36 km each.",
        "the guests say yes",
    )
    .await;
}

/// May: the road bike goes in, and Kyle cannot come.
async fn may(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "wheel-truing", "name": "Wheel truing",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:wheel-truing",
               "content": "the road bike went into Costington's and the wheel was trued",
               "provenance": "testimony",
               "sets": {"mechanic": "person:token", "work": "trued the wheel",
                          "bike": "thing:road-bike", "shop": "org:costingtons"}}),
    )
    .await;
    let kyle = address_of(room, "person:kyle", "said yes").await;
    did(
        room,
        sid,
        "update_fact",
        json!({"address": kyle, "status": "archived",
               "details": "Kyle cannot make the party any more"}),
    )
    .await;
    part_of_the_move(room, sid, "the-movers", "now", "the movers are booked").await;
    check_in(room, sid, "oil-the-chain", "2026-05-31", "oiled the chain").await;
    journalled(
        room,
        sid,
        "2026-05-31: The road bike had its wheel trued at Costington's, Token did the work. Kyle cannot make \
         the party any more. The movers are booked for the move. Oiled the chain. The knee is fine \
         now that the saddle is lower.",
        "the road bike goes in",
    )
    .await;
}

/// June: the party happened, and a plus-one has a name.
async fn june(room: &Surface, sid: &str) {
    june_naming_the_partner(room, sid, true).await;
}

/// June, with the partner's name given either to the person April made
/// (`renamed`) or to a person made new, leaving April's placeholder standing.
async fn june_naming_the_partner(room: &Surface, sid: &str, renamed: bool) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "person", "handle": "apu", "name": "Apu", "source": "the operator"}),
    )
    .await;
    attending(room, sid, "apu", "came to the goodbye party with Wendy").await;
    match renamed {
        true => {
            did(
                room,
                sid,
                "rename_entity",
                json!({"handle": "person:plus-one", "to": "person:louise"}),
            )
            .await;
            did(
                room,
                sid,
                "update_entity",
                json!({"handle": "person:louise", "name": "Louise"}),
            )
            .await;
        }
        false => {
            did(
                room,
                sid,
                "add_entity",
                json!({"kind": "person", "handle": "louise", "name": "Louise",
                       "source": "the operator"}),
            )
            .await;
            attending(
                room,
                sid,
                "louise",
                "came to the goodbye party with Butters",
            )
            .await;
        }
    }
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:goodbye-party",
               "content": "the party went great, though we ran out of chairs for ten minutes",
               "provenance": "testimony", "sets": {"held_on": "2026-06-13"}}),
    )
    .await;
    check_in(
        room,
        sid,
        "back-up-the-laptop",
        "2026-06-14",
        "backed up the laptop",
    )
    .await;
    journalled(
        room,
        sid,
        "2026-06-14: The party was yesterday and went great, though we ran out of chairs for ten minutes. \
         Everybody who said yes came except Kyle. Wendy brought Apu. Butters's partner is \
         Louise. Backed up the laptop.",
        "the party",
    )
    .await;
}

/// August: the road bike goes, and the vet certificate is done.
async fn august(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:road-bike", "content": "sold the road bike to Stan; it is Stan's now",
               "provenance": "testimony", "sets": {"owned": "no", "sold_to": "person:stan"}}),
    )
    .await;
    part_of_the_move(
        room,
        sid,
        "the-vet-certificate",
        "done",
        "the vet certificate is done",
    )
    .await;
    for _ in 0..3 {
        ride(room, sid, "fat-bike", "20").await;
    }
    for _ in 0..2 {
        ride(room, sid, "gravel-bike", "61").await;
    }
    journalled(
        room,
        sid,
        "2026-08-09: Sold the road bike to Stan. Snowball's vet certificate for the move is done. Three rides \
         on the fat bike, 20 km each, and two on the gravel bike, 61 km each.",
        "the road bike goes",
    )
    .await;
}

/// September: tyres, and the insurance.
async fn september(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": "event", "handle": "new-tyres", "name": "New tyres",
               "source": "the operator"}),
    )
    .await;
    did(
        room,
        sid,
        "capture",
        json!({"subject": "event:new-tyres",
               "content": "the gravel bike went into Costington's again for new tyres",
               "provenance": "testimony",
               "sets": {"mechanic": "person:kenny", "work": "new tyres",
                          "bike": "thing:gravel-bike", "shop": "org:costingtons"}}),
    )
    .await;
    part_of_the_move(room, sid, "the-insurance", "done", "the insurance is done").await;
    check_in(room, sid, "call-randy", "2026-09-27", "called Randy").await;
    journalled(
        room,
        sid,
        "2026-09-27: The gravel bike had new tyres at Costington's, Kenny did the work. The insurance for the \
         move is done. Called Randy. The knee flared up again on the climb.",
        "tyres and the insurance",
    )
    .await;
}

/// October: the move.
async fn october(room: &Surface, sid: &str) {
    october_backing_up(room, sid, true).await;
}

/// October, with the laptop's backup either filed as a check-in (`filed`) or
/// only said, as a sentence on the routine.
async fn october_backing_up(room: &Surface, sid: &str, filed: bool) {
    part_of_the_move(
        room,
        sid,
        "the-movers",
        "done",
        "the movers came, we moved today",
    )
    .await;
    ride(room, sid, "gravel-bike", "105").await;
    match filed {
        true => {
            check_in(
                room,
                sid,
                "back-up-the-laptop",
                "2026-10-31",
                "backed up the laptop before the move",
            )
            .await;
        }
        false => {
            did(
                room,
                sid,
                "capture",
                json!({"subject": "rhythm:back-up-the-laptop",
                       "content": "backed up the laptop on 2026-10-31, before the move",
                       "provenance": "testimony"}),
            )
            .await;
        }
    }
    journalled(
        room,
        sid,
        "2026-10-31: We moved to Quahog today. Everything for the move is done except the utilities, which \
         are not transferred yet. Backed up the laptop. One ride in Quahog on the gravel bike, \
         105 km.",
        "the move",
    )
    .await;
}

/// One answer, kept under the slot the operator named, on the thing it is about.
async fn answered(room: &Surface, sid: &str, subject: &str, slot: &str, answer: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": subject, "content": format!("the answer to question {slot}"),
               "provenance": "testimony", "sets": {format!("answer_{slot}"): answer}}),
    )
    .await;
}

/// **The answers a sitting that found everything gives**, as
/// `(slot, the thing in the worked year, the thing in a year written in prose,
/// the answer)`. The text is the same in both: the prose year differs only in
/// what the year left in the store behind the answers.
const ANSWERS: [(&str, &str, &str, &str); 16] = [
    (
        "1",
        "bot:lisa",
        "bot:lisa",
        "Assistant: keeps the operator's year. Lisa: keeps the household's paperwork, the \
         garden calendar and the appointments.",
    ),
    (
        "2",
        "rhythm:oil-the-chain",
        "rhythm:oil-the-chain",
        "Oil the chain: behind, last done 2026-05-31",
    ),
    (
        "2",
        "rhythm:call-randy",
        "rhythm:call-randy",
        "Call Randy: behind, last done 2026-09-27",
    ),
    (
        "3",
        "project:the-move",
        "project:the-move",
        "Ten sessions. In the one of 2026-03-30 the movers quoted 1,800.",
    ),
    (
        "4",
        "view:my-bikes",
        "thing:gravel-bike",
        "The gravel bike and the fat bike",
    ),
    (
        "5",
        "project:the-move",
        "project:the-move",
        "2026-01-10 the party on 13 June; 2026-03-30 the move to Quahog by the end of October; \
         2026-08-09 sell the road bike",
    ),
    (
        "6",
        "bot:lisa",
        "bot:lisa",
        "Lisa keeps the household's paperwork, the garden calendar and the appointments. Her \
         mailbox is hers and not mine to open.",
    ),
    (
        "7",
        "thing:road-bike",
        "thing:road-bike",
        "The knee came up in the sessions of 2026-02-21, 2026-05-31 and 2026-09-27.",
    ),
    (
        "8",
        "thing:gravel-bike",
        "thing:gravel-bike",
        "Gravel: 556 km against 151 km for the road bike, so 405 km more.",
    ),
    (
        "9",
        "event:goodbye-party",
        "event:goodbye-party",
        "No: nine people and eight chairs, so one short.",
    ),
    ("10", "person:louise", "person:butters", "Louise"),
    (
        "11",
        "thing:fat-bike",
        "thing:fat-bike",
        "2026-01: 103 km; 2026-02: 123 km; 2026-03: 110 km; 2026-04: 144 km; 2026-08: 182 km; \
         2026-10: 105 km; nothing in the other months.",
    ),
    (
        "12",
        "work:the-utilities",
        "project:the-move",
        "The utilities are still open. The lease, the movers, the vet certificate and the \
         insurance are done.",
    ),
    (
        "13",
        "event:goodbye-party",
        "event:goodbye-party",
        "It went great. Stan, Wendy, Apu, Token, Randy, Sharon, Butters and Louise came. \
         Kyle cancelled.",
    ),
    (
        "14",
        "org:costingtons",
        "org:costingtons",
        "2026-02-21 Kenny: new chain and new brake pads; 2026-05-31 Token: trued the wheel; \
         2026-09-27 Kenny: new tyres.",
    ),
    (
        "15",
        "project:the-move",
        "project:the-move",
        "2026-03-30, 2026-05-31, 2026-08-09, 2026-09-27 and 2026-10-31",
    ),
];

/// **December: the cold sitting writes its fifteen answers**, the way a sitting
/// that found everything would, each under the slot the operator named.
async fn december(room: &Surface, sid: &str) {
    for (slot, subject, _, answer) in ANSWERS {
        answered(room, sid, subject, slot, answer).await;
    }
}

type Sitting = for<'a> fn(
    &'a Surface,
    &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + 'a>>;

/// **The ten sittings and the day each is in**, in order.
const SITTINGS: [(&str, Sitting); 10] = [
    ("2026-01-10", |r, s| Box::pin(january(r, s))),
    ("2026-02-21", |r, s| Box::pin(february(r, s))),
    ("2026-03-30", |r, s| Box::pin(march(r, s))),
    ("2026-04-26", |r, s| Box::pin(april(r, s))),
    ("2026-05-31", |r, s| Box::pin(may(r, s))),
    ("2026-06-14", |r, s| Box::pin(june(r, s))),
    ("2026-08-09", |r, s| Box::pin(august(r, s))),
    ("2026-09-27", |r, s| Box::pin(september(r, s))),
    ("2026-10-31", |r, s| Box::pin(october(r, s))),
    ("2026-12-01", |r, s| Box::pin(december(r, s))),
];

/// **The nine sittings of the year**, without the cold one.
async fn work_the_year(room: &Surface) {
    for (day, work) in &SITTINGS[..9] {
        let sid = sitting(room, day).await;
        work(room, &sid).await;
    }
}

/// **One thing the year does wrong**, in the sitting that does it.
#[derive(Clone, Copy)]
enum Slip {
    /// October says the backup in words and files no check-in for it.
    TheBackupIsOnlySaid,
    /// June gives Butters's partner her name on a person made new.
    LouiseIsMadeNew,
}

/// **The nine sittings of the year with one slip in one of them**, every other
/// sitting as it is meant to be.
async fn work_the_year_slipping(room: &Surface, slip: Slip) {
    for (day, work) in &SITTINGS[..9] {
        let sid = sitting(room, day).await;
        match (slip, *day) {
            (Slip::TheBackupIsOnlySaid, "2026-10-31") => {
                october_backing_up(room, &sid, false).await
            }
            (Slip::LouiseIsMadeNew, "2026-06-14") => {
                june_naming_the_partner(room, &sid, false).await
            }
            _ => work(room, &sid).await,
        }
    }
}

/// A furnished room with the year worked in it.
async fn year_worked() -> (Room, Surface) {
    let (room, surface) = furnished().await;
    work_the_year(&surface).await;
    (room, surface)
}

/// **All ten sittings, with the readings a run takes between them** — one
/// before the first and one after every phase, the same shape `vault_room.rs`
/// takes, for the same reason: a lock scoped to one phase's window has nothing
/// to read without the boundaries.
async fn worked_everything(room: &Surface, document: &Playbook) -> Vec<Boundary> {
    let named = boundary_names(document);
    let locks = expectations::for_playbook(ASKING_ROOM).expect("the asking room asserts");
    let mut boundaries = vec![boundary(room, &named[0]).await];
    for (at, (day, work)) in SITTINGS.iter().enumerate() {
        let sid = sitting(room, day).await;
        work(room, &sid).await;
        let asked = phase_end_queries(&locks, &format!("Phase {}", at + 1));
        boundaries.push(boundary_asking(room, &named[at + 1], &asked).await);
    }
    boundaries
}

async fn judge_all(room: &Surface, boundaries: &[Boundary]) -> Vec<Outcome> {
    let seen = Observed { room, boundaries };
    let mut outcomes = Vec::new();
    for check in expectations::for_playbook(ASKING_ROOM).expect("the asking room asserts") {
        outcomes.push(check.check(&seen).await);
    }
    assert!(
        !outcomes.is_empty(),
        "the asking room registered no locks, so a run would report a pass over an empty list",
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

/// **One lock, read straight off the shipped document by a piece of its own
/// sentence**, so a lock added above it does not silently point a case at the
/// wrong one. Run against the room as it stands.
async fn the_lock_saying(room: &Surface, piece: &str) -> Outcome {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let checks = expectations::for_playbook(ASKING_ROOM).expect("the asking room asserts");
    let found = checks
        .iter()
        .find(|check| check.name().contains(piece))
        .unwrap_or_else(|| panic!("the asking room ships no lock saying {piece:?}"));
    found.check(&seen).await
}

/// **Case 2. Which routines are behind as of 1 December, and when each was last
/// done.** The chain was last oiled on 31 May and Randy last called on 27
/// September, so both are behind. The laptop was last backed up on 31 October
/// with forty-five days between, so it is not due until 15 December.
#[tokio::test]
async fn solvable_the_chain_and_randy_are_behind_and_the_laptop_is_not() {
    let (_room, surface) = year_worked().await;
    let asked = surface
        .call(
            "recall",
            json!({"kind": "rhythm", "overdue": {"as_of": "2026-12-01"}}),
        )
        .await;
    let answer: Value = serde_json::from_str(&asked).expect("the read is json");
    let behind: Vec<(&str, &str)> = answer["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .map(|o| {
            (
                o["id"].as_str().unwrap_or_default(),
                o["fields"]["last_check_in"].as_str().unwrap_or_default(),
            )
        })
        .collect();
    assert_eq!(
        behind.len(),
        2,
        "exactly two routines are behind on 1 December: {asked}"
    );
    assert!(
        behind.contains(&("rhythm:oil-the-chain", "2026-05-31")),
        "the chain was last oiled on 31 May: {asked}",
    );
    assert!(
        behind.contains(&("rhythm:call-randy", "2026-09-27")),
        "Randy was last called on 27 September: {asked}",
    );
    assert!(
        !asked.contains("rhythm:back-up-the-laptop"),
        "the laptop was backed up on 31 October and is not due until 15 December: {asked}",
    );
}

/// **The cold sitting's boot, for a case that asks the surface a question as
/// that sitting would.**
async fn cold(surface: &Surface) -> String {
    sitting(surface, "2026-12-01").await
}

/// Every session a search for `words` finds, once each, as `(focus, hit)`.
async fn sessions_found(surface: &Surface, sid: &str, words: &str) -> Vec<(String, String)> {
    let found: Value = serde_json::from_str(
        &surface
            .call("search", json!({"query": words, "sid": sid, "limit": 100}))
            .await,
    )
    .expect("the search is json");
    let mut sessions: Vec<(String, String)> = Vec::new();
    for hit in found["results"].as_array().cloned().unwrap_or_default() {
        if hit["hit"] != "session" {
            continue;
        }
        let id = hit["session"].as_str().unwrap_or_default().to_string();
        if sessions.iter().any(|(seen, _)| *seen == id) {
            continue;
        }
        sessions.push((
            id,
            hit["working_on"].as_str().unwrap_or_default().to_string(),
        ));
    }
    sessions
        .into_iter()
        .map(|(id, focus)| (focus, id))
        .collect()
}

/// A past session's whole chronology, by the focus a search named it by.
async fn chronology_of(surface: &Surface, sid: &str, focus: &str) -> String {
    let read = surface
        .call(
            "recall",
            json!({"kind": "session", "prose": true, "sid": sid}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|object| object["name"] == focus)
        .and_then(|object| object["prose"].as_str())
        .unwrap_or_else(|| panic!("no run of the cold sitting's own is called {focus:?}: {read}"))
        .to_string()
}

/// The day a past session says it started on, by the focus a search named it by.
async fn started_on_of(surface: &Surface, sid: &str, focus: &str) -> String {
    let read = surface
        .call("recall", json!({"kind": "session", "sid": sid}))
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    parsed["objects"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .find(|object| object["name"] == focus)
        .and_then(|object| object["fields"]["started_on"].as_str())
        .unwrap_or_else(|| panic!("no run called {focus:?} says when it started: {read}"))
        .to_string()
}

/// **Case 3. What the movers quoted is in the March session, and the cold
/// sitting is shown ten of its own runs and none of Lisa's.** The room's
/// furnishing run and the nine sittings are the occupant's; Lisa's three are
/// hers, and a list of "my past sessions" must not hand them over.
#[tokio::test]
async fn solvable_the_movers_quote_is_in_the_march_session_and_lisas_sessions_are_not_listed() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let runs = did(&surface, &sid, "list_runs", json!({})).await;
    assert_eq!(
        runs["count"], 10,
        "the occupant's own runs are the furnishing and the nine sittings: {runs}",
    );
    let read = surface
        .call(
            "recall",
            json!({"kind": "session", "prose": true, "sid": sid}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    let march: Vec<&str> = parsed["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|object| object["prose"].as_str())
        .filter(|prose| prose.contains("2026-03-30"))
        .collect();
    assert_eq!(march.len(), 1, "exactly one run is the March one: {read}");
    assert!(
        march[0].contains("1,800"),
        "the March run says what the movers quoted: {}",
        march[0],
    );
    assert!(
        !read.contains("household folder") && !read.contains("physiotherapist"),
        "a run of Lisa's was handed to the occupant: {read}",
    );
}

/// **Case 4. "My bikes" run today returns the gravel bike and the fat bike.**
/// The fat bike was bought in April and the road bike sold in August, so a list
/// typed out in January fails; a view run now does not.
#[tokio::test]
async fn solvable_my_bikes_run_today_is_the_gravel_bike_and_the_fat_bike() {
    let (_room, surface) = year_worked().await;
    let held = the_lock_saying(&surface, "no view the year made returns exactly").await;
    assert!(held.held, "the view lock did not hold: {}", held.saying);
    let ran = surface.call("recall", json!({"view": "my-bikes"})).await;
    assert!(
        ran.contains("thing:fat-bike") && ran.contains("thing:gravel-bike"),
        "the view returns both bikes still owned: {ran}",
    );
    assert!(
        !ran.contains("thing:road-bike"),
        "the road bike was sold in August and is not mine: {ran}",
    );
}

/// **Case 7. The knee came up in three of the operator's own sittings and
/// none of Lisa's.** A search of the occupant's own sessions finds exactly the
/// February, May and September ones, and each carries its day in its own beat.
#[tokio::test]
async fn solvable_the_knee_is_in_three_sittings_and_not_in_lisas() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let found = sessions_found(&surface, &sid, "knee").await;
    let mut focuses: Vec<&str> = found.iter().map(|(focus, _)| focus.as_str()).collect();
    focuses.sort_unstable();
    assert_eq!(
        focuses,
        [
            "the gravel bike goes in",
            "the road bike goes in",
            "tyres and the insurance"
        ],
        "the knee came up in the February, May and September sittings: {found:?}",
    );
    let mut days = Vec::new();
    let mut started: Vec<String> = Vec::new();
    for (focus, _) in &found {
        started.push(started_on_of(&surface, &sid, focus).await);
        let prose = chronology_of(&surface, &sid, focus).await;
        assert!(
            !prose.contains("physiotherapist"),
            "a session of Lisa's was read as the operator's: {prose}",
        );
        for day in ["2026-02-21", "2026-05-31", "2026-09-27"] {
            if prose.contains(day) {
                days.push(day);
            }
        }
    }
    days.sort_unstable();
    assert_eq!(
        days,
        ["2026-02-21", "2026-05-31", "2026-09-27"],
        "each of the three says the day it was in: {found:?}",
    );
    started.sort_unstable();
    assert_eq!(
        started,
        ["2026-02-21", "2026-05-31", "2026-09-27"],
        "each of the three is also placed on the day it stated, by the run's own record: {found:?}",
    );
}

/// The km of every ride, by the day it was filed: `(day, bike, km)`.
async fn rides(surface: &Surface) -> Vec<(String, String, u32)> {
    let read = surface
        .call(
            "recall",
            json!({"kind": "thing", "fields": [{"key": "km"}], "history": "km",
                   "history_most": 100}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("the read is json");
    let mut all = Vec::new();
    for bike in parsed["objects"].as_array().cloned().unwrap_or_default() {
        for write in bike["history"]["writes"]
            .as_array()
            .cloned()
            .unwrap_or_default()
        {
            all.push((
                write["recorded_at"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string(),
                bike["id"].as_str().unwrap_or_default().to_string(),
                write["value"]
                    .as_str()
                    .and_then(|km| km.parse().ok())
                    .unwrap_or_else(|| panic!("a ride with no km: {write}")),
            ));
        }
    }
    all
}

/// **Case 8. Gravel was ridden most, by 405 km over the road bike.** Eleven
/// rides against three would say eight, and eight is not the margin.
#[tokio::test]
async fn solvable_gravel_is_ridden_most_by_405_km_over_the_road_bike() {
    let (_room, surface) = year_worked().await;
    let all = rides(&surface).await;
    let total = |bike: &str| -> u32 {
        all.iter()
            .filter(|(_, id, _)| id == bike)
            .map(|(_, _, km)| km)
            .sum()
    };
    let count = |bike: &str| all.iter().filter(|(_, id, _)| id == bike).count();
    assert_eq!(total("thing:gravel-bike"), 556, "gravel: {all:?}");
    assert_eq!(total("thing:road-bike"), 151, "road: {all:?}");
    assert_eq!(total("thing:fat-bike"), 60, "fat: {all:?}");
    assert_eq!(
        total("thing:gravel-bike") - total("thing:road-bike"),
        405,
        "the margin over the second bike",
    );
    assert_eq!(
        (count("thing:gravel-bike"), count("thing:road-bike")),
        (11, 3),
        "the count of rides is not the margin: {all:?}",
    );
}

/// **Case 9. Nine people against eight chairs: one short.** The chairs are the
/// room's own furniture and the guests are what the year recorded.
#[tokio::test]
async fn solvable_nine_people_met_eight_chairs() {
    let (_room, surface) = year_worked().await;
    let chairs: u32 = {
        let mut sum = 0;
        for set in ["thing:folding-chairs", "thing:dining-chairs"] {
            let read = surface.call("recall", json!({"subject": set})).await;
            let parsed: Value = serde_json::from_str(&read).expect("json");
            sum += parsed["objects"][0]["fields"]["count"]
                .as_str()
                .and_then(|count| count.parse::<u32>().ok())
                .unwrap_or_else(|| panic!("{set} holds no count: {read}"));
        }
        sum
    };
    let walked = surface
        .call(
            "recall",
            json!({"kind": "event",
                   "follow": {"shape": "attendance", "direction": "in"}}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&walked).expect("json");
    let guests = parsed["objects"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|event| event["connected"].as_array().cloned().unwrap_or_default())
        .filter(|guest| guest["via"]["type"] == "attendee" && guest["via"]["retracted"].is_null())
        .count() as u32;
    assert_eq!(chairs, 8, "four folding and four dining");
    assert_eq!(guests, 8, "eight guests came: {walked}");
    assert_eq!(
        guests + 1 - chairs,
        1,
        "the host makes nine, one more than the chairs"
    );
}

/// **Case 10. Butters's plus-one is Louise, and there is one of them.** The
/// April placeholder was renamed, so no person is left standing as "plus-one".
#[tokio::test]
async fn solvable_the_plus_one_and_louise_are_one_person() {
    let (_room, surface) = year_worked().await;
    let people = surface
        .call("list_entities", json!({"kind": "person"}))
        .await;
    assert!(
        people.contains("person:louise") && !people.contains("plus-one"),
        "the April placeholder became Louise: {people}",
    );
    let parsed: Value = serde_json::from_str(&people).expect("json");
    assert_eq!(
        parsed["count"], 10,
        "the cast of ten: eight guests, Kyle and Kenny — no eleventh: {people}",
    );
}

/// **Case 11. Km by month: 103, 123, 110, 144, 182 and 105.** The rides were
/// said in six different sittings, and none is in the other five months.
#[tokio::test]
async fn solvable_the_monthly_totals_are_103_123_110_144_182_and_105() {
    let (_room, surface) = year_worked().await;
    let mut months: std::collections::BTreeMap<String, u32> = Default::default();
    for (day, _, km) in rides(&surface).await {
        *months.entry(day[..7].to_string()).or_default() += km;
    }
    let expected: std::collections::BTreeMap<String, u32> = [
        ("2026-01", 103),
        ("2026-02", 123),
        ("2026-03", 110),
        ("2026-04", 144),
        ("2026-08", 182),
        ("2026-10", 105),
    ]
    .into_iter()
    .map(|(month, km)| (month.to_string(), km))
    .collect();
    assert_eq!(months, expected, "the months the rides were filed in");
}

/// **Cases 12, 13 and 14. The work of the move, the guests and the shop
/// visits are each held by the year as it was worked.** One case per lock
/// group, kept apart by name, so a red names the group.
#[tokio::test]
async fn solvable_the_move_is_open_only_on_the_utilities() {
    let (_room, surface) = year_worked().await;
    let held = the_lock_saying(&surface, "the work of the move is not done except").await;
    assert!(held.held, "{}", held.saying);
}

#[tokio::test]
async fn solvable_the_party_was_attended_by_the_eight_who_came_and_not_by_kyle() {
    let (_room, surface) = year_worked().await;
    let held = the_lock_saying(&surface, "the guests recorded are not the eight").await;
    assert!(held.held, "{}", held.saying);
}

#[tokio::test]
async fn solvable_three_shop_visits_each_hold_a_mechanic_and_the_work() {
    let (_room, surface) = year_worked().await;
    for piece in [
        "of the February visit",
        "of the September visit",
        "of the May visit",
    ] {
        let held = the_lock_saying(&surface, piece).await;
        assert!(held.held, "{piece}: {}", held.saying);
    }
}

/// **Case 15. The move came up in five sittings: March, May, August,
/// September and October.** A search of the occupant's own sessions finds
/// exactly those five, each saying its own day, and nothing of Lisa's paperwork.
#[tokio::test]
async fn solvable_the_move_came_up_in_five_sittings_and_not_in_lisas_paperwork() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let found = sessions_found(&surface, &sid, "move").await;
    assert_eq!(found.len(), 5, "five sittings, once each: {found:?}");
    let mut days = Vec::new();
    let mut started: Vec<String> = Vec::new();
    for (focus, _) in &found {
        started.push(started_on_of(&surface, &sid, focus).await);
        let prose = chronology_of(&surface, &sid, focus).await;
        assert!(
            !prose.contains("household folder"),
            "Lisa's paperwork was read as the operator's: {prose}",
        );
        for day in [
            "2026-03-30",
            "2026-05-31",
            "2026-08-09",
            "2026-09-27",
            "2026-10-31",
        ] {
            if prose.contains(day) {
                days.push(day);
            }
        }
    }
    days.sort_unstable();
    assert_eq!(
        days,
        [
            "2026-03-30",
            "2026-05-31",
            "2026-08-09",
            "2026-09-27",
            "2026-10-31"
        ],
        "each of the five says its day: {found:?}",
    );
    started.sort_unstable();
    assert_eq!(
        started,
        [
            "2026-03-30",
            "2026-05-31",
            "2026-08-09",
            "2026-09-27",
            "2026-10-31"
        ],
        "each of the five is also placed on the day it stated, by the run's own record: {found:?}",
    );
}

/// **A room nobody worked in holds none of the locks.** Furnished, plausible,
/// with Lisa and her sessions in it, and nothing done: the shop, the bikes, the
/// people and the routines are all there, and not one question has an answer.
#[tokio::test]
async fn an_asking_room_nobody_worked_in_fails_every_lock() {
    let (_room, surface) = furnished().await;
    let outcomes = judge_all(&surface, &[]).await;
    for outcome in &outcomes {
        assert!(
            !outcome.held,
            "a room nobody worked in held a lock: {}",
            saying(&outcomes),
        );
    }
}

/// **The year worked the way it is meant to be, and the locks all hold.** The
/// ten sittings, the last of which writes the fifteen answers, with a reading
/// taken between each as a run takes them.
#[tokio::test]
async fn every_lock_holds_once_the_year_is_worked() {
    let (_room, surface) = furnished().await;
    let boundaries = worked_everything(&surface, &room_document()).await;
    let outcomes = judge_all(&surface, &boundaries).await;
    for outcome in &outcomes {
        assert!(
            outcome.held,
            "a year worked the way it is meant to be failed a lock: {}",
            saying(&outcomes),
        );
    }
}

/// 🚨 **No lock in this room rests on a needle that matches somewhere else.**
/// The reasoning is written where this was first built, in the year room's own
/// case.
#[tokio::test]
async fn no_lock_here_rests_on_a_needle_that_matches_somewhere_else() {
    let (_room, surface) = furnished().await;
    let boundaries = worked_everything(&surface, &room_document()).await;
    let summary = jojobot_exercise::lock::needle_summary(
        &surface,
        &jojobot_exercise::lock::locks_of(ASKING_ROOM),
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
/// case is worth nothing if it reds everything: the room is otherwise worked
/// the way it is meant to be, so a red on any other lock is the case being
/// wrong, and a green on the named one is the lock being blind.
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
        "the lock that failed is not the one the wrong act is about ({piece:?}): {}",
        saying(outcomes),
    );
}

/// A room worked in full, the way it is meant to be, and the December
/// sitting's own handle — the wrong act in each case below happens inside that
/// sitting, on its own day.
async fn worked_in_full() -> (Room, Surface, String) {
    let (room, surface) = furnished().await;
    work_the_year(&surface).await;
    let sid = sitting(&surface, "2026-12-01").await;
    december(&surface, &sid).await;
    (room, surface, sid)
}

/// **No view held "my bikes", only a list.** The wrong act: the operator's
/// word was never made into a question, so the answer is whatever was true on
/// the day it was typed. Everything else is right.
#[tokio::test]
async fn a_year_that_kept_no_view_fails_only_the_bikes_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    did(
        &surface,
        &december_sid,
        "archive_entity",
        json!({"handle": "view:my-bikes", "reason": "kept as a list instead"}),
    )
    .await;
    only_one_fails(&judge_all(&surface, &[]).await, "no view the year made");
}

/// **Kyle's yes was never taken back.** The wrong act: the guest list is April's
/// list, with the guest who cancelled still on it.
#[tokio::test]
async fn a_year_that_left_kyle_on_the_list_fails_only_the_guests_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    attending(
        &surface,
        &december_sid,
        "kyle",
        "said yes to the goodbye party",
    )
    .await;
    only_one_fails(&judge_all(&surface, &[]).await, "the guests recorded");
}

/// **The margin was answered with the count of rides.** The wrong act: eleven
/// rides against three is eight, and eight is not how much further the gravel
/// bike went.
#[tokio::test]
async fn a_year_that_answered_with_the_count_of_rides_fails_only_the_margin_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    answered(
        &surface,
        &december_sid,
        "thing:gravel-bike",
        "8",
        "Gravel, by 8 rides",
    )
    .await;
    only_one_fails(&judge_all(&surface, &[]).await, "the bike ridden more");
}

/// **Lisa's physio session came back as the operator's knee.** The wrong act:
/// what is in the second assistant's own session is in the operator's answer.
#[tokio::test]
async fn a_year_that_read_lisas_session_as_the_operators_fails_only_the_knee_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    answered(
        &surface,
        &december_sid,
        "thing:road-bike",
        "7",
        "The knee came up on 2026-02-21, 2026-05-31 and 2026-09-27, and a physiotherapist is booked.",
    )
    .await;
    only_one_fails(
        &judge_all(&surface, &[]).await,
        "the knee does not come back",
    );
}

/// A room worked as meant except for one slip, and the cold sitting's answers.
async fn worked_slipping(slip: Slip) -> (Room, Surface) {
    let (room, surface) = furnished().await;
    work_the_year_slipping(&surface, slip).await;
    let sid = sitting(&surface, "2026-12-01").await;
    december(&surface, &sid).await;
    (room, surface)
}

/// **The laptop's backup was said and never filed as a check-in.** The wrong
/// act: October's sitting wrote a sentence on the routine, so jojobot still
/// counts the laptop from June and finds it overdue, while the cold sitting's
/// answer, worked out by hand, names only the chain and Randy.
#[tokio::test]
async fn a_year_that_only_said_the_backup_fails_only_the_routines_carrier_lock() {
    let (_room, surface) = worked_slipping(Slip::TheBackupIsOnlySaid).await;
    only_one_fails(
        &judge_all(&surface, &[]).await,
        "jojobot does not find the chain behind",
    );
}

/// **Butters's partner was named on a person made new in June.** The wrong
/// act: the April placeholder stays standing beside Louise, so the guests
/// still total the eight and a host, and the answer still names her.
#[tokio::test]
async fn a_year_that_named_louise_afresh_fails_only_the_plus_one_carrier_lock() {
    let (_room, surface) = worked_slipping(Slip::LouiseIsMadeNew).await;
    only_one_fails(
        &judge_all(&surface, &[]).await,
        "nobody named Louise holds a record from before June",
    );
}

/// A sentence on a thing and nothing else: no field, no edge, no check-in.
async fn said(room: &Surface, sid: &str, subject: &str, content: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": subject, "content": content, "provenance": "testimony"}),
    )
    .await;
}

/// A thing that exists only so a sentence has somewhere to be said.
async fn somewhere(room: &Surface, sid: &str, kind: &str, handle: &str, name: &str) {
    did(
        room,
        sid,
        "add_entity",
        json!({"kind": kind, "handle": handle, "name": name, "source": "the operator"}),
    )
    .await;
}

/// **The same year, said entirely in sentences.** Every fact the worked year
/// keeps as a key, an edge, a check-in, a status or a view is a sentence on the
/// nearest thing here, and the sums are done by hand in the answers. The
/// sittings are the same nine on the same days, each leaving the same dated
/// beat, so a session can still be found by the day it stated.
async fn prose_year(room: &Surface) {
    let days = [
        "2026-01-10",
        "2026-02-21",
        "2026-03-30",
        "2026-04-26",
        "2026-05-31",
        "2026-06-14",
        "2026-08-09",
        "2026-09-27",
        "2026-10-31",
    ];
    for (nth, day) in days.into_iter().enumerate() {
        let sid = sitting(room, day).await;
        match nth {
            0 => {
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "my bikes are the gravel bike and the road bike; rode 62 km this month",
                )
                .await;
                said(room, &sid, "thing:road-bike", "rode 41 km this month").await;
                somewhere(room, &sid, "event", "goodbye-party", "The Goodbye Party").await;
                said(
                    room,
                    &sid,
                    "event:goodbye-party",
                    "a goodbye party at home on 13 June",
                )
                .await;
            }
            1 => {
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "went into Costington's: Kenny put a new chain and new brake pads on it",
                )
                .await;
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "rode 41 km three times this month",
                )
                .await;
                said(
                    room,
                    &sid,
                    "rhythm:oil-the-chain",
                    "oiled the chain on 2026-02-21",
                )
                .await;
            }
            2 => {
                somewhere(room, &sid, "project", "the-move", "The Move").await;
                said(room, &sid, "project:the-move", "moving to Quahog by the end of October: the lease, the movers, the vet certificate, the insurance and the utilities").await;
                said(
                    room,
                    &sid,
                    "project:the-move",
                    "the lease is signed; the movers quoted 1,800",
                )
                .await;
                said(room, &sid, "thing:road-bike", "rode 55 km twice this month").await;
                said(
                    room,
                    &sid,
                    "rhythm:call-randy",
                    "called Randy on 2026-03-30",
                )
                .await;
            }
            3 => {
                said(room, &sid, "event:goodbye-party", "Stan, Kyle, Wendy, Token, Randy and Sharon said yes; Butters is bringing his partner").await;
                somewhere(room, &sid, "thing", "fat-bike", "The Fat Bike").await;
                said(room, &sid, "thing:fat-bike", "bought a fat bike").await;
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "rode 36 km four times this month",
                )
                .await;
            }
            4 => {
                said(
                    room,
                    &sid,
                    "thing:road-bike",
                    "went into Costington's and Token trued the wheel",
                )
                .await;
                said(
                    room,
                    &sid,
                    "event:goodbye-party",
                    "Kyle cannot make the party any more",
                )
                .await;
                said(room, &sid, "project:the-move", "the movers are booked").await;
                said(
                    room,
                    &sid,
                    "rhythm:oil-the-chain",
                    "oiled the chain on 2026-05-31",
                )
                .await;
            }
            5 => {
                said(room, &sid, "event:goodbye-party", "went great, though we ran out of chairs for ten minutes; everybody who said yes came except Kyle; Wendy brought Apu; Butters's partner is called Louise").await;
                said(
                    room,
                    &sid,
                    "rhythm:back-up-the-laptop",
                    "backed up the laptop on 2026-06-14",
                )
                .await;
            }
            6 => {
                said(
                    room,
                    &sid,
                    "thing:road-bike",
                    "sold the road bike to Stan; it is Stan's now",
                )
                .await;
                said(
                    room,
                    &sid,
                    "project:the-move",
                    "the vet certificate is done",
                )
                .await;
                said(
                    room,
                    &sid,
                    "thing:fat-bike",
                    "rode 20 km three times this month",
                )
                .await;
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "rode 61 km twice this month",
                )
                .await;
            }
            7 => {
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "went into Costington's again: Kenny put new tyres on it",
                )
                .await;
                said(room, &sid, "project:the-move", "the insurance is done").await;
                said(
                    room,
                    &sid,
                    "rhythm:call-randy",
                    "called Randy on 2026-09-27",
                )
                .await;
            }
            _ => {
                said(
                    room,
                    &sid,
                    "project:the-move",
                    "we moved; everything is done except the utilities",
                )
                .await;
                said(
                    room,
                    &sid,
                    "thing:gravel-bike",
                    "rode 105 km once this month, in Quahog",
                )
                .await;
                said(
                    room,
                    &sid,
                    "rhythm:back-up-the-laptop",
                    "backed up the laptop on 2026-10-31",
                )
                .await;
            }
        }
        // The same dated beat the worked year leaves, so a session is still
        // found by the day it stated.
        journalled(
            room,
            &sid,
            &format!("{day}: a sitting of the year, written down as sentences."),
            "the year in prose",
        )
        .await;
    }
}

/// **The cold sitting of the prose year**: the same fifteen answers, put on
/// the things a year written as sentences has.
async fn december_in_prose(room: &Surface, sid: &str) {
    for (slot, _, subject, answer) in ANSWERS {
        answered(room, sid, subject, slot, answer).await;
    }
}

/// **A year written entirely in prose still has its answers, and fails the
/// locks that read what was kept.**
///
/// The proving brief's bar is that a year said in sentences FAILS the locks and
/// a year that reached for the capability passes. The unworked room cannot show
/// it, because a lock that holds on any written answer passes both the empty
/// room's opposite and the worked year. Here every fact is a sentence and every
/// answer is correct, said in words with the sums done by hand.
///
/// **What fails** is what reads state the store keeps beyond the answer: the
/// view that was never made (4), the check-ins that were never filed, so that
/// jojobot's own overdue arithmetic finds all three routines behind (the
/// carrier beside 2), the person who was never named (the carrier beside 10),
/// the work of the move that was never filed (12), the guests who were never
/// recorded as attending (13) and the three visits that never held a mechanic
/// as a key (14). That is eight locks. **What holds** is what reads the
/// answer's own words (the answer locks of 2, 3, 7, 8, 9, 10, 11 and 15),
/// because the sums were done correctly by hand; that is right for those
/// locks, and the case says so rather than pretending a prose answer is a
/// wrong one. By question, 2 and 10 now fail and 3, 7, 8, 9, 11 and 15 hold.
#[tokio::test]
async fn a_year_written_entirely_in_prose_fails_every_lock_that_reads_what_was_kept() {
    let (_room, surface) = furnished().await;
    prose_year(&surface).await;
    let sid = sitting(&surface, "2026-12-01").await;
    december_in_prose(&surface, &sid).await;
    let outcomes = judge_all(&surface, &[]).await;
    let mut failed: Vec<&str> = outcomes
        .iter()
        .filter(|outcome| !outcome.held)
        .map(|outcome| outcome.name.as_str())
        .collect();
    failed.sort_unstable();
    let expected = [
        "no view the year made",
        "the work of the move is not done except",
        "the guests recorded",
        "of the February visit",
        "of the September visit",
        "of the May visit",
        "jojobot does not find the chain behind",
        "nobody named Louise holds a record from before June",
    ];
    assert_eq!(
        failed.len(),
        expected.len(),
        "exactly the locks that read kept state fail on a year said in prose: {}",
        saying(&outcomes),
    );
    for piece in expected {
        assert!(
            failed.iter().any(|name| name.contains(piece)),
            "the lock saying {piece:?} held on a year with no view, no work, no guests and no \
             keyed visits: {}",
            saying(&outcomes),
        );
    }
}

/// **The room's server runs on the day the room gave it, and never on the wall
/// clock.** `ping` says which day the server is acting out. A server on the real
/// clock says none, so this fails on every day if the pin is missing.
#[tokio::test]
async fn the_rooms_server_runs_on_the_day_the_room_gave_it() {
    let (_room, surface) = furnished().await;
    assert_eq!(
        surface.stated_day().await.as_deref(),
        Some(SERVER_DAY),
        "the server is not acting out the day the room gave it"
    );
}

/// **No needle of this room holds the day its server runs on.** A needle is
/// matched as text in an answer, and the server writes its day into the moments
/// it stamps, so a needle holding that day could be met by a stamp.
#[test]
fn no_needle_here_is_the_servers_own_day() {
    let named = jojobot_exercise::lock::needles_naming(
        &jojobot_exercise::lock::locks_of(ASKING_ROOM),
        SERVER_DAY,
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}
