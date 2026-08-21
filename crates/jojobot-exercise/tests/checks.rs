//! **The positive controls the named checks lean on, watched firing.**
//!
//! Every check here that says *the wrong thing is absent* first says *and the
//! room was furnished at all*. That guard is the rule this project repeats
//! more than any other — a negative alone passes on a store that lost
//! everything — and **nothing had ever watched a single instance of it fire**.
//!
//! It is the same shape in three checks, so it is three cases rather than
//! three unrelated ones: **the control fires on a room with nothing in it, and
//! the check still reaches its real claim on a room that was furnished.**
//! Without the second half, a check that failed on everything would satisfy
//! all three.

use jojobot_exercise::expectations;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, Outcome};
use jojobot_exercise::surface::Surface;
use serde_json::{Value, json};

/// The position each room's document writes the check this file is about.
const BRIEF_LEFT_THE_BOX: usize = 0;
const THE_SERVICE_IS_A_VALUE: usize = 1;
const A_HANDOFF_IS_WAITING: usize = 4;
const THE_PILE_IS_IN_THE_BOX: usize = 2;

/// **A room with nothing in it.** Not a room whose furniture failed — a room
/// nobody furnished, which is what a check's control has to survive.
async fn bare() -> (Room, Surface) {
    let room = Room::open(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");
    (room, surface)
}

/// A room furnished the way a run furnishes it, and a session in it.
async fn furnished(document: &str) -> (Room, Surface, String) {
    let (room, surface) = bare().await;
    expectations::seed_for(document)
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

async fn did(room: &Surface, sid: &str, verb: &str, mut args: Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

/// One of a room's checks, run against the room as it stands.
async fn judge(room: &Surface, document: &str, which: usize) -> Outcome {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room,
        boundaries: &boundaries,
    };
    let checks = expectations::for_playbook(document).expect("the room asserts");
    checks[which].check(&seen).await
}

/// **The control under `the_brief_left_the_box`.**
///
/// The check reads the oldest message on the board. On a room nobody furnished
/// there is no message at all, and a check that read one anyway would be
/// reporting on a board it never saw.
#[tokio::test]
async fn the_brief_check_fails_on_a_room_with_no_mail_and_holds_on_a_furnished_one() {
    let (_bare, empty) = bare().await;
    let nothing = judge(&empty, expectations::HANDOVER_ROOM, BRIEF_LEFT_THE_BOX).await;
    assert!(
        !nothing.held,
        "a room with no mail on it at all held the check that reads whether the brief was \
         collected: {}",
        nothing.saying,
    );

    let (_room, surface, sid) = furnished(expectations::HANDOVER_ROOM).await;
    did(&surface, &sid, "read_mailbox", json!({})).await;
    let taken = judge(&surface, expectations::HANDOVER_ROOM, BRIEF_LEFT_THE_BOX).await;
    assert!(
        taken.held,
        "the check does not hold on a furnished room whose brief was collected, so the case \
         above passes on a check that fails on everything: {}",
        taken.saying,
    );
}

/// **The control under `the_service_day_is_a_value`.**
///
/// The check walks what the bike holds. On a room nobody furnished there is no
/// bike, the read comes back blocked, and **a blocked answer carries no value**
/// — so a check without this guard reports *nothing on the bike mentions the
/// day*, which says the session failed when the room did.
#[tokio::test]
async fn the_service_check_fails_on_a_room_with_no_bike_and_holds_when_the_day_is_a_value() {
    let (_bare, empty) = bare().await;
    let nothing = judge(&empty, expectations::BIKE_ROOM, THE_SERVICE_IS_A_VALUE).await;
    assert!(
        !nothing.held,
        "a room with no bike in it held the check that reads what the bike carries: {}",
        nothing.saying,
    );

    let (_room, surface, sid) = furnished(expectations::BIKE_ROOM).await;
    did(
        &surface,
        &sid,
        "capture",
        json!({"subject": "thing:gravel-bike", "content": "annual service",
               "provenance": "testimony", "fields": {"done_on": "2026-08-11"}}),
    )
    .await;
    let recorded = judge(&surface, expectations::BIKE_ROOM, THE_SERVICE_IS_A_VALUE).await;
    assert!(
        recorded.held,
        "the check does not hold on a bike carrying the day as a value, so the case above \
         passes on a check that fails on everything: {}",
        recorded.saying,
    );
}

/// **The control under `a_handoff_is_waiting`.**
///
/// The check asks whether the occupant left anything behind, which it works
/// out by finding a message that is not the brief. On a room nobody furnished
/// there is no brief to tell apart from anything else, so **every board looks
/// like one nobody left a handoff on**.
#[tokio::test]
async fn the_handoff_check_fails_on_a_room_with_no_brief_and_holds_when_one_was_left() {
    let (_bare, empty) = bare().await;
    let nothing = judge(&empty, expectations::BIKE_ROOM, A_HANDOFF_IS_WAITING).await;
    assert!(
        !nothing.held,
        "a room with no brief on the board held the check that reads whether a handoff was \
         left: {}",
        nothing.saying,
    );

    let (_room, surface, sid) = furnished(expectations::BIKE_ROOM).await;
    did(
        &surface,
        &sid,
        "post_message",
        json!({"to": "assistant", "subject": "the bikes, brought up to date",
               "body": "Wrote down the service and the year's riding."}),
    )
    .await;
    let left = judge(&surface, expectations::BIKE_ROOM, A_HANDOFF_IS_WAITING).await;
    assert!(
        left.held,
        "the check does not hold on a board carrying a message the occupant left, so the case \
         above passes on a check that fails on everything: {}",
        left.saying,
    );
}

/// **The positive under the count of three: they came from the occupant.**
///
/// Three things in the colleague's box is the claim, and a count on its own
/// credits a run for three messages that came from somewhere else. **The
/// sender is what makes the count mean the handover happened.**
///
/// **Both readings in one case**, over the same three messages: three from the
/// occupant holds, and the same three with one of them from somebody else does
/// not.
#[tokio::test]
async fn three_things_from_somebody_else_do_not_count_as_the_handover() {
    let (_room, surface, sid) = furnished(expectations::HANDOVER_ROOM).await;
    did(&surface, &sid, "read_mailbox", json!({})).await;
    did(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "bot", "handle": "gamma", "name": "Gamma", "source": "the operator"}),
    )
    .await;
    for subject in ["power loss", "consensus", "the espresso machine"] {
        did(
            &surface,
            &sid,
            "post_message",
            json!({"to": "gamma", "subject": subject, "body": "one of the three"}),
        )
        .await;
    }
    let handed = judge(
        &surface,
        expectations::HANDOVER_ROOM,
        THE_PILE_IS_IN_THE_BOX,
    )
    .await;
    assert!(
        handed.held,
        "three things the occupant sent did not read as the pile being handed over: {}",
        handed.saying,
    );

    // **A second room, holding three again, one of them from somebody else.**
    // A room rather than an edit of the one above, because `processed` is a
    // terminal archive and stays on the board: retiring a message here would
    // leave FOUR, and the count would fail before the sender was ever read.
    let (_other, surface, sid) = furnished(expectations::HANDOVER_ROOM).await;
    did(&surface, &sid, "read_mailbox", json!({})).await;
    did(
        &surface,
        &sid,
        "add_entity",
        json!({"kind": "bot", "handle": "gamma", "name": "Gamma", "source": "the operator"}),
    )
    .await;
    for subject in ["power loss", "consensus"] {
        did(
            &surface,
            &sid,
            "post_message",
            json!({"to": "gamma", "subject": subject, "body": "one of the three"}),
        )
        .await;
    }
    let mine = surface
        .must("start_here", json!({"bot": "gamma", "brief": true}))
        .await
        .expect("the colleague boots");
    let theirs = mine["session"]["sid"]
        .as_str()
        .expect("a handle")
        .to_string();
    did(
        &surface,
        &theirs,
        "post_message",
        json!({"to": "gamma", "subject": "the espresso machine",
               "body": "not from the assistant"}),
    )
    .await;

    let mixed = judge(
        &surface,
        expectations::HANDOVER_ROOM,
        THE_PILE_IS_IN_THE_BOX,
    )
    .await;
    assert!(
        !mixed.held,
        "a box holding three things, one of them from somebody other than the occupant, read as \
         the pile having been handed over: {}",
        mixed.saying,
    );
}
