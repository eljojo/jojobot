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
    Playbook::read(&expectations::room_document(ASKING_ROOM))
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
    february_after_the_visit(room, sid).await;
}

/// February with the visit set on the bike, which each visit overwrites.
async fn february_with_the_visit_on_the_bike(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:gravel-bike",
               "content": "the gravel bike went into Costington's for a new chain and new brake pads",
               "provenance": "testimony",
               "sets": {"mechanic": "person:kenny", "work": "new chain and new brake pads",
                          "shop": "org:costingtons"}}),
    )
    .await;
    february_after_the_visit(room, sid).await;
}

/// February with the visit only said, as a sentence on the bike.
async fn february_with_the_visit_only_said(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:gravel-bike",
               "content": "the gravel bike went into Costington's, Kenny put a new chain and new brake pads on it",
               "provenance": "testimony"}),
    )
    .await;
    february_after_the_visit(room, sid).await;
}

/// What February does besides the shop visit.
async fn february_after_the_visit(room: &Surface, sid: &str) {
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
    april_after_the_guests(room, sid).await;
}

/// April, with Butters saying yes and no word of a plus-one: no placeholder is
/// made and no claim says a partner is coming.
async fn april_without_the_plus_one(room: &Surface, sid: &str) {
    for who in ["stan", "kyle", "wendy", "token", "randy", "sharon"] {
        attending(room, sid, who, "said yes to the goodbye party").await;
    }
    attending(room, sid, "butters", "said yes to the goodbye party").await;
    april_after_the_guests(room, sid).await;
}

/// What April does besides the guests: a bike is bought in passing.
async fn april_after_the_guests(room: &Surface, sid: &str) {
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
    may_after_the_visit(room, sid).await;
}

/// May with the visit set on the bike.
async fn may_with_the_visit_on_the_bike(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:road-bike",
               "content": "the road bike went into Costington's and the wheel was trued",
               "provenance": "testimony",
               "sets": {"mechanic": "person:token", "work": "trued the wheel",
                          "shop": "org:costingtons"}}),
    )
    .await;
    may_after_the_visit(room, sid).await;
}

/// May with the visit only said, as a sentence on the bike.
async fn may_with_the_visit_only_said(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:road-bike",
               "content": "the road bike went into Costington's, Token trued the wheel",
               "provenance": "testimony"}),
    )
    .await;
    may_after_the_visit(room, sid).await;
}

/// What May does besides the shop visit.
async fn may_after_the_visit(room: &Surface, sid: &str) {
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
    june_naming_the_partner(room, sid, true, LinkToButters::None).await;
}

/// How June links the partner's person to Butters, beyond naming her.
#[derive(Clone, Copy)]
enum LinkToButters {
    /// No link.
    None,
    /// An edge on a claim of her own.
    Edge,
    /// A field on a claim of her own that holds his handle.
    Field,
}

/// June, with the partner's name given either to the person April made
/// (`renamed`) or to a person made new, leaving April's placeholder standing.
async fn june_naming_the_partner(room: &Surface, sid: &str, renamed: bool, link: LinkToButters) {
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
    match link {
        LinkToButters::None => {}
        LinkToButters::Edge => {
            did(
                room,
                sid,
                "capture",
                json!({"subject": "person:louise", "content": "is Butters's partner",
                       "provenance": "testimony", "shape": "connection",
                       "object": "person:butters"}),
            )
            .await;
        }
        LinkToButters::Field => {
            did(
                room,
                sid,
                "capture",
                json!({"subject": "person:louise", "content": "is Butters's partner",
                       "provenance": "testimony",
                       "sets": {"partner_of": "person:butters"}}),
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
    september_after_the_visit(room, sid).await;
}

/// September with the visit set on the bike.
async fn september_with_the_visit_on_the_bike(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:gravel-bike",
               "content": "the gravel bike went into Costington's again for new tyres",
               "provenance": "testimony",
               "sets": {"mechanic": "person:kenny", "work": "new tyres",
                          "shop": "org:costingtons"}}),
    )
    .await;
    september_after_the_visit(room, sid).await;
}

/// September with the visit only said, as a sentence on the bike.
async fn september_with_the_visit_only_said(room: &Surface, sid: &str) {
    did(
        room,
        sid,
        "capture",
        json!({"subject": "thing:gravel-bike",
               "content": "the gravel bike went into Costington's again, Kenny put new tyres on it",
               "provenance": "testimony"}),
    )
    .await;
    september_after_the_visit(room, sid).await;
}

/// What September does besides the shop visit.
async fn september_after_the_visit(room: &Surface, sid: &str) {
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
    /// June gives her name on a person made new, and links her to Butters by an
    /// edge on her own claim.
    LouiseIsMadeNewAndLinkedByAnEdge,
    /// June gives her name on a person made new, and links her to Butters by a
    /// field that holds his handle.
    LouiseIsMadeNewAndLinkedByAField,
    /// Each shop visit is a write on the bike, which the next visit overwrites,
    /// and no entity is made for a visit.
    TheVisitsAreSetOnTheBike,
    /// April never says a partner is coming, and June gives a new Louise an
    /// edge at Butters.
    LouiseIsLinkedButAprilNeverSaidAPartner,
    /// February's visit is a sentence on the bike and no key.
    TheFebruaryVisitIsOnlySaid,
    /// May's visit is a sentence on the bike and no key.
    TheMayVisitIsOnlySaid,
    /// September's visit is a sentence on the bike and no key.
    TheSeptemberVisitIsOnlySaid,
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
                june_naming_the_partner(room, &sid, false, LinkToButters::None).await
            }
            (Slip::TheFebruaryVisitIsOnlySaid, "2026-02-21") => {
                february_with_the_visit_only_said(room, &sid).await
            }
            (Slip::TheMayVisitIsOnlySaid, "2026-05-31") => {
                may_with_the_visit_only_said(room, &sid).await
            }
            (Slip::TheSeptemberVisitIsOnlySaid, "2026-09-27") => {
                september_with_the_visit_only_said(room, &sid).await
            }
            (Slip::LouiseIsLinkedButAprilNeverSaidAPartner, "2026-04-26") => {
                april_without_the_plus_one(room, &sid).await
            }
            (Slip::LouiseIsLinkedButAprilNeverSaidAPartner, "2026-06-14") => {
                june_naming_the_partner(room, &sid, false, LinkToButters::Edge).await
            }
            (Slip::TheVisitsAreSetOnTheBike, "2026-02-21") => {
                february_with_the_visit_on_the_bike(room, &sid).await
            }
            (Slip::TheVisitsAreSetOnTheBike, "2026-05-31") => {
                may_with_the_visit_on_the_bike(room, &sid).await
            }
            (Slip::TheVisitsAreSetOnTheBike, "2026-09-27") => {
                september_with_the_visit_on_the_bike(room, &sid).await
            }
            (Slip::LouiseIsMadeNewAndLinkedByAnEdge, "2026-06-14") => {
                june_naming_the_partner(room, &sid, false, LinkToButters::Edge).await
            }
            (Slip::LouiseIsMadeNewAndLinkedByAField, "2026-06-14") => {
                june_naming_the_partner(room, &sid, false, LinkToButters::Field).await
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

/// **Case 1. The bots here are the assistant, defrag and Lisa, and each says what it
/// is for.** The room makes one bot and the build ships the other two; a read of
/// the kind returns exactly those three, each with a charter, and Lisa's is the room's one line.
#[tokio::test]
async fn solvable_the_bots_here_are_the_assistant_and_lisa_and_each_says_what_it_is_for() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let read = surface
        .call(
            "recall",
            json!({"kind": "bot", "charter": true, "sid": sid}),
        )
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("json");
    let bots = parsed["objects"].as_array().expect("objects");
    let mut ids: Vec<&str> = bots.iter().filter_map(|bot| bot["id"].as_str()).collect();
    ids.sort_unstable();
    assert_eq!(ids, ["bot:assistant", "bot:defrag", "bot:lisa"], "{read}");
    for bot in bots {
        assert!(
            bot["charter"].as_str().is_some_and(|said| !said.is_empty()),
            "each bot says what it is for: {bot}",
        );
    }
    let lisa = bots
        .iter()
        .find(|bot| bot["id"] == "bot:lisa")
        .expect("lisa is here");
    assert!(
        lisa["charter"]
            .as_str()
            .is_some_and(|said| said.contains("paperwork")),
        "Lisa's charter is the room's line about the household's paperwork: {lisa}",
    );
}

/// The oldest day among the records on `subject` that `query` finds, as search
/// serves them: the day a thing was first said is the day it was decided.
async fn first_day_found(surface: &Surface, sid: &str, query: &str, subject: &str) -> String {
    let read = surface
        .call("search", json!({"query": query, "sid": sid}))
        .await;
    let parsed: Value = serde_json::from_str(&read).expect("json");
    let mut days: Vec<&str> = parsed["results"]
        .as_array()
        .expect("results")
        .iter()
        .filter(|hit| hit["hit"] == "fact" && hit["parent"]["id"] == subject)
        .filter_map(|hit| hit["recorded_at"].as_str())
        .collect();
    days.sort_unstable();
    days.first()
        .unwrap_or_else(|| panic!("no record on {subject} answers {query:?}: {read}"))
        .to_string()
}

/// **Case 5. What the year decided is on record with its day, and the days put
/// it oldest first.** The party in January, the move in March and the sale of the
/// road bike in August; each is a record on the thing it is about, found by the
/// words the operator used, and the oldest record of each carries its day.
#[tokio::test]
async fn solvable_what_the_year_decided_is_on_record_with_its_day_oldest_first() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let party = first_day_found(&surface, &sid, "goodbye party", "event:goodbye-party").await;
    let the_move = first_day_found(&surface, &sid, "decided move Quahog", "project:the-move").await;
    let sale = first_day_found(&surface, &sid, "sold", "thing:road-bike").await;
    assert_eq!(
        [party.as_str(), the_move.as_str(), sale.as_str()],
        ["2026-01-10", "2026-03-30", "2026-08-09"],
        "the three decisions carry the days of the sittings that made them",
    );
    let mut oldest_first = [party.clone(), the_move.clone(), sale.clone()];
    oldest_first.sort_unstable();
    assert_eq!(
        oldest_first,
        [party, the_move, sale],
        "read oldest first, they are already in the order the year made them",
    );
}

/// **Case 6. Lisa's charter reads, and her mailbox does not.** A message left in
/// her box is not the assistant's to open: the read is refused and says who
/// can, and the assistant's own box and its own counts are read as ever.
#[tokio::test]
async fn solvable_lisas_charter_reads_and_her_mailbox_does_not() {
    let (_room, surface) = year_worked().await;
    let sid = cold(&surface).await;
    let charter = surface
        .call(
            "recall",
            json!({"subject": "bot:lisa", "charter": true, "sid": sid}),
        )
        .await;
    assert!(charter.contains("paperwork"), "{charter}");

    let posted: Value = serde_json::from_str(
        &surface
            .call(
                "post_message",
                json!({"to": "lisa", "subject": "a question", "body": "a note for Lisa", "sid": sid}),
            )
            .await,
    )
    .expect("json");
    let id = posted["id"].as_str().expect("the post answers with an id");
    assert_eq!(posted["mailbox"], "lisa", "{posted}");

    let refused: Value = serde_json::from_str(
        &surface
            .call("read_message", json!({"message_id": id, "sid": sid}))
            .await,
    )
    .expect("json");
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert_eq!(refused["fix_by"], "change", "{refused}");

    let booted = surface
        .must(
            "start_here",
            json!({"bot": "assistant", "brief": true, "today": "2026-12-01", "sid": sid}),
        )
        .await
        .expect("the boot answers");
    // **The other bots are read from the boot that names no bot.** A boot as the
    // assistant names the assistant and counts the rest, so Lisa is not in it.
    let roster = surface
        .must("start_here", json!({"brief": true}))
        .await
        .expect("the door answers with no bot named");
    let lisa = roster["snapshot"]["entities"]["bots"]
        .as_array()
        .expect("the snapshot names the bots")
        .iter()
        .find(|bot| bot["handle"] == "bot:lisa")
        .expect("the snapshot names Lisa");
    assert_eq!(lisa["yours"], false, "{lisa}");
    assert!(
        lisa["mail"]["counts"].is_null(),
        "another bot's counts are never shown: {lisa}"
    );
    let bots = booted["snapshot"]["entities"]["bots"]
        .as_array()
        .expect("the snapshot names the bots");
    let mine = bots
        .iter()
        .find(|bot| bot["handle"] == "bot:assistant")
        .expect("the snapshot names the assistant");
    assert!(
        mine["mail"]["counts"]["total"].is_number(),
        "the pair: the assistant's own counts are shown: {mine}"
    );
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

/// **The chairs were answered with the opposite verdict.** The wrong act: the
/// answer says no chair was missing, in words that hold "one more chair". A lock
/// that reads those words as the verdict passes it.
#[tokio::test]
async fn a_year_that_said_no_chair_was_missing_fails_only_the_chairs_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    answered(
        &surface,
        &december_sid,
        "event:goodbye-party",
        "9",
        "Yes, enough: nine people and eight chairs, so no need for one more chair.",
    )
    .await;
    only_one_fails(
        &judge_all(&surface, &[]).await,
        "the answer about the chairs",
    );
}

/// **The plus-one was answered with a wrong name, on Louise's own record.** The
/// wrong act: the answer sits on the person the year named, so her handle and
/// name are in what the slot's read returns whatever the answer says. The
/// answer's own words must carry the name.
#[tokio::test]
async fn a_year_that_named_someone_else_on_louises_record_fails_only_the_plus_one_answer_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    answered(&surface, &december_sid, "person:louise", "10", "Mabel").await;
    only_one_fails(&judge_all(&surface, &[]).await, "the plus-one is not named");
}

/// The pair of the case above: an answer that opens with the name and goes on
/// holds the lock.
#[tokio::test]
async fn a_plus_one_answer_that_opens_with_her_name_and_goes_on_holds_every_lock() {
    let (_room, surface, december_sid) = worked_in_full().await;
    answered(
        &surface,
        &december_sid,
        "person:louise",
        "10",
        "Louise, who came with Butters.",
    )
    .await;
    let outcomes = judge_all(&surface, &[]).await;
    assert!(
        outcomes.iter().all(|o| o.held),
        "every lock holds: {}",
        saying(&outcomes)
    );
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
/// The year as it is meant to be, with the cold sitting's December written.
async fn worked_slipping_nothing() -> (Room, Surface) {
    let (room, surface) = furnished().await;
    work_the_year(&surface).await;
    let sid = sitting(&surface, "2026-12-01").await;
    december(&surface, &sid).await;
    (room, surface)
}

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

/// **The April placeholder renamed to Louise holds the carrier.** The first of
/// the two shapes the carrier accepts: she holds a record older than June.
#[tokio::test]
async fn the_renamed_plus_one_holds_the_plus_one_carrier_lock() {
    let (_room, surface) = worked_slipping_nothing().await;
    the_plus_one_carrier_holds(&judge_all(&surface, &[]).await);
}

/// **A Louise made new in June and linked to Butters by an edge is the partner,
/// and the carrier holds.** The year joins April's plus-one to June's name
/// without the rename: April's claim that Butters was bringing a partner is on
/// Butters, and Louise's own claim draws an edge at him.
#[tokio::test]
async fn a_louise_made_new_and_linked_to_butters_by_an_edge_holds_the_plus_one_carrier_lock() {
    let (_room, surface) = worked_slipping(Slip::LouiseIsMadeNewAndLinkedByAnEdge).await;
    the_plus_one_carrier_holds(&judge_all(&surface, &[]).await);
}

/// **A Louise linked to Butters is not the partner when April never said a
/// partner was coming.** Nothing joins her to April's plus-one, so the carrier
/// fails and no other lock does.
#[tokio::test]
async fn a_linked_louise_without_an_april_claim_fails_only_the_plus_one_carrier_lock() {
    let (_room, surface) = worked_slipping(Slip::LouiseIsLinkedButAprilNeverSaidAPartner).await;
    only_one_fails(
        &judge_all(&surface, &[]).await,
        "nobody named Louise holds a record from before June",
    );
}

/// **A field that holds Butters's handle links Louise to him as an edge does.**
#[tokio::test]
async fn a_louise_made_new_and_linked_to_butters_by_a_field_holds_the_plus_one_carrier_lock() {
    let (_room, surface) = worked_slipping(Slip::LouiseIsMadeNewAndLinkedByAField).await;
    the_plus_one_carrier_holds(&judge_all(&surface, &[]).await);
}

/// The carrier lock held, and the lock is the one that was asked.
fn the_plus_one_carrier_holds(outcomes: &[Outcome]) {
    let carrier: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| {
            o.name
                .contains("nobody named Louise holds a record from before June")
        })
        .collect();
    assert_eq!(carrier.len(), 1, "the carrier lock is not in the room");
    assert!(
        carrier[0].held,
        "a Louise linked to Butters did not hold the carrier: {}",
        carrier[0].saying,
    );
}

/// **Each shop visit set on the bike holds the three visit locks.** A visit is an
/// occasion, so its mechanic and work are keys of the record, and the next visit
/// overwrites the bike's folded value. The locks read each record, so all three
/// visits are found.
#[tokio::test]
async fn visits_set_on_the_bike_hold_all_three_visit_locks() {
    let (_room, surface) = worked_slipping(Slip::TheVisitsAreSetOnTheBike).await;
    let outcomes = judge_all(&surface, &[]).await;
    for piece in [
        "of the February visit",
        "of the September visit",
        "of the May visit",
    ] {
        let found: Vec<&Outcome> = outcomes.iter().filter(|o| o.name.contains(piece)).collect();
        assert_eq!(
            found.len(),
            1,
            "the lock saying {piece:?} is not in the room"
        );
        assert!(
            found[0].held,
            "the visit set on the bike failed the lock saying {piece:?}: {}",
            found[0].saying,
        );
    }
}

/// **February's visit only said fails only the February visit lock.**
#[tokio::test]
async fn a_year_that_only_said_the_february_visit_fails_only_the_february_visit_lock() {
    let (_room, surface) = worked_slipping(Slip::TheFebruaryVisitIsOnlySaid).await;
    only_one_fails(&judge_all(&surface, &[]).await, "of the February visit");
}

/// **May's visit only said fails only the May visit lock.**
#[tokio::test]
async fn a_year_that_only_said_the_may_visit_fails_only_the_may_visit_lock() {
    let (_room, surface) = worked_slipping(Slip::TheMayVisitIsOnlySaid).await;
    only_one_fails(&judge_all(&surface, &[]).await, "of the May visit");
}

/// **September's visit only said fails only the September visit lock.**
#[tokio::test]
async fn a_year_that_only_said_the_september_visit_fails_only_the_september_visit_lock() {
    let (_room, surface) = worked_slipping(Slip::TheSeptemberVisitIsOnlySaid).await;
    only_one_fails(&judge_all(&surface, &[]).await, "of the September visit");
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
        &jojobot_exercise::lock::locks_of(ASKING_ROOM),
        SERVER_DAY.as_str(),
    );
    assert!(
        named.is_empty(),
        "a needle holds the day the server runs on, so a stamp can meet it: {named:?}"
    );
}
