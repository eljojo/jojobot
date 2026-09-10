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
use jojobot_exercise::run::{Boundary, Expectation, Observed, Outcome};
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
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
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

/// **`one_record_points_at_two_kinds` is windowed to June, not to the
/// finished board.**
///
/// 🚨 **The false green this proves against.** The check used to run a live
/// `search` over the whole room, so any sitting that ever wrote two handles
/// into one record satisfied it — June discriminated only because it was the
/// one sitting the reference transcript happened to write a pointer in, not
/// because the check looked at June's window. This drives the two cases apart:
/// a later sitting writes the pointer alone, outside June's own window, and
/// the check must still fail — paired with June doing the thing itself, which
/// must still hold.
#[tokio::test]
async fn the_pointer_check_is_windowed_to_june_and_not_to_a_later_sitting() {
    let make = jojobot_exercise::checks::CHECKS
        .iter()
        .find(|(name, _)| *name == "one_record_points_at_two_kinds")
        .map(|(_, make)| *make)
        .expect("the check ships");

    // June's window shows nothing, and a later sitting writes the pointer on
    // its own — the false green this check must no longer produce.
    {
        let (_room, surface, sid) = furnished(expectations::YEAR_ROOM).await;
        let before = jojobot_exercise::run::boundary(&surface, "Phase 6 — testing").await;
        let after_june = jojobot_exercise::run::boundary(&surface, "Phase 7 — after").await;
        did(
            &surface,
            &sid,
            "capture",
            json!({"subject": "org:north-trail-club",
                   "content": "@place:north-trail and @person:milhouse came up today",
                   "provenance": "testimony"}),
        )
        .await;
        let after_everything = jojobot_exercise::run::boundary(&surface, "the end").await;
        let boundaries = vec![before, after_june, after_everything];
        let seen = Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let outcome = make().run(&seen).await;
        assert!(
            outcome.is_err(),
            "a later sitting wrote the pointer and June's own window shows nothing, so the \
             check must not credit June for it: {outcome:?}",
        );
    }

    // June itself writes the pointer, inside its own window — the positive
    // the case above rests on.
    {
        let (_room, surface, sid) = furnished(expectations::YEAR_ROOM).await;
        let before = jojobot_exercise::run::boundary(&surface, "Phase 6 — testing").await;
        did(
            &surface,
            &sid,
            "capture",
            json!({"subject": "org:north-trail-club",
                   "content": "@place:north-trail and @person:milhouse came up today",
                   "provenance": "testimony"}),
        )
        .await;
        let after_june = jojobot_exercise::run::boundary(&surface, "Phase 7 — after").await;
        let boundaries = vec![before, after_june];
        let seen = Observed {
            room: &surface,
            boundaries: &boundaries,
        };
        let outcome = make().run(&seen).await;
        assert!(
            outcome.is_ok(),
            "June wrote the pointer inside its own window and the check still failed: \
             {outcome:?}",
        );
    }
}

/// 🚨 **A lock whose own query is REFUSED says so, rather than counting in a
/// refusal.**
///
/// A blocked read carries no objects, so every `carries` fails and every
/// `lacks` holds — and both look exactly like a room that was asked a fair
/// question. ⛔️ **A paid run scored a lock zero when `recall` had come back
/// *not an entity jojobot knows*: the needle was never looked for, and the
/// failure named the needle.**
///
/// **Paired, because the risk is that this becomes an excuse.** A read that
/// reaches the store and finds nothing must still fail on its needle — that is
/// an answer, and *no* is what it says.
#[tokio::test]
async fn a_refused_query_is_told_apart_from_an_answer_that_says_no() {
    let (_room, surface) = bare().await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };

    // ① The query names somebody the empty room does not hold, so the read is
    //    refused before any needle is looked for. **The same subject answers
    //    the second half through a different verb**, which is what makes the
    //    pair about the refusal rather than about the name.
    let refused = jojobot_exercise::lock::read(
        "```locks\n\
         recall {\"subject\": \"person:milhouse\"}\n\
         carries \"name\":\"Milhouse\"\n\
         say     the person is not on the record\n\
         ```\n",
    )
    .expect("the lock parses");
    let outcome = refused[0].check(&seen).await;
    assert!(!outcome.held, "a refused query is not a lock that held");
    assert!(
        outcome.saying.contains("refused"),
        "the failure blames the needle rather than saying the query was refused: {}",
        outcome.saying,
    );
    assert!(
        !outcome.saying.contains("Milhouse"),
        "the failure names the needle, which was never looked for: {}",
        outcome.saying,
    );

    // ② A read that reaches the store and finds nothing still fails on its
    //    needle. **Without this the fix is an excuse rather than a
    //    distinction.**
    let answered = jojobot_exercise::lock::read(
        "```locks\n\
         list_entities {\"kind\": \"person\"}\n\
         carries person:milhouse\n\
         say     nobody is on the roster\n\
         ```\n",
    )
    .expect("the lock parses");
    let outcome = answered[0].check(&seen).await;
    assert!(
        !outcome.held,
        "an empty roster satisfied a lock that asks for somebody on it",
    );
    assert!(
        !outcome.saying.contains("refused"),
        "a read that reached the store was reported as a refusal: {}",
        outcome.saying,
    );
}
