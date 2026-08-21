//! **The assertion every dated sitting gets, and no author writes.**
//!
//! A run acting out a year is fiction inside the test. Nothing in jojobot
//! learns about elapsed time and there is no clock to fool: the surface takes
//! the day from the caller and stamps real today only when a caller sends none.
//!
//! ⚠️ **So the year holds exactly as far as each sitting carries its own date
//! into the calls it makes, and the caller is a real model.** A sitting told in
//! prose that it is March, which then writes without a date, leaves a record
//! stamped with real today — and nothing else in the run fails. The year
//! collapses and the transcript reads perfectly well.
//!
//! It cannot be caught after the fact by a lock somebody remembered to write,
//! so the run generates one per dated sitting.

use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Observed, days_claimed};
use jojobot_exercise::surface::Surface;

/// A document whose first sitting claims a day and whose second names none.
const TWO_SITTINGS: &str = "\
## Phase 1 — the spring sitting\n\
\n\
**Session: fresh.** **Day: 2026-03-04.**\n\
\n\
> it is the fourth of March and the kettle was descaled\n\
\n\
## Phase 2 — a sitting with no day\n\
\n\
**Session: fresh.**\n\
\n\
> say anything at all\n";

fn read() -> Playbook {
    Playbook::parse("rooms/whatever.md", TWO_SITTINGS).expect("the document reads")
}

/// A boundary named for the phase it was taken before, holding one world.
fn at(before: &str, world: &str) -> Boundary {
    Boundary {
        before: before.to_string(),
        mail: String::new(),
        world: world.to_string(),
        board: String::new(),
        runs_offered: 0,
    }
}

/// A room to hand the check, which never asks it anything: the claim is about
/// what the run saw at each boundary, and the boundaries are the argument.
async fn a_room() -> (Room, Surface) {
    let room = Room::open(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");
    (room, surface)
}

/// **One assertion per dated sitting, and none for a sitting that claims no
/// day.**
///
/// **Both halves.** A generator that produced one per phase would pass the
/// count on this document and then assert a date against every room written
/// before the year existed.
#[test]
fn a_dated_sitting_gets_an_assertion_and_an_undated_one_does_not() {
    let made = days_claimed(&read());
    assert_eq!(
        made.len(),
        1,
        "one sitting claims a day: {:?}",
        made.iter().map(|c| c.name()).collect::<Vec<_>>(),
    );
    let name = made[0].name();
    assert!(
        name.contains("Phase 1") && name.contains("2026-03-04"),
        "the assertion does not say which sitting claimed which day: {name}",
    );
}

/// **A sitting that carried its day holds, and one that did not fails saying
/// which sitting and which day.**
///
/// ⭐ **A firing here is not the harness catching itself.** It is the product's
/// most likely silent failure — a caller told a date in prose that does not
/// carry it into the call — caught where a person reads it.
#[tokio::test]
async fn a_sitting_that_did_not_carry_its_day_fails_and_names_itself() {
    let (_room, surface) = a_room().await;
    let made = days_claimed(&read());
    let check = &made[0];

    let carried = vec![
        at("Phase 1 — the spring sitting", "{\"records\":[]}"),
        at(
            "Phase 2 — a sitting with no day",
            "{\"records\":[{\"date\":\"2026-03-04\"}]}",
        ),
    ];
    let held = check
        .check(&Observed {
            room: &surface,
            boundaries: &carried,
        })
        .await;
    assert!(
        held.held,
        "a sitting whose writes carry its day was reported as not carrying it: {}",
        held.saying,
    );

    let stamped_today = vec![
        at("Phase 1 — the spring sitting", "{\"records\":[]}"),
        at(
            "Phase 2 — a sitting with no day",
            "{\"records\":[{\"date\":\"2026-08-21\"}]}",
        ),
    ];
    let missed = check
        .check(&Observed {
            room: &surface,
            boundaries: &stamped_today,
        })
        .await;
    assert!(
        !missed.held,
        "a sitting that wrote under real today was reported as carrying March",
    );
    assert!(
        missed.saying.contains("Phase 1") && missed.saying.contains("2026-03-04"),
        "the failure does not say which sitting claimed which day: {}",
        missed.saying,
    );
}

/// 🚨 **The day must arrive with the run, not with the furniture.**
///
/// A room seeded with a record already dated a sitting's day makes that
/// sitting's assertion hold whatever the occupant does, and it holds silently.
/// The check refuses instead, so the author moves the seed.
#[tokio::test]
async fn a_day_the_room_was_furnished_with_refuses_rather_than_holding() {
    let (_room, surface) = a_room().await;
    let made = days_claimed(&read());
    let furnished = vec![
        at(
            "Phase 1 — the spring sitting",
            "{\"records\":[{\"date\":\"2026-03-04\"}]}",
        ),
        at(
            "Phase 2 — a sitting with no day",
            "{\"records\":[{\"date\":\"2026-03-04\"}]}",
        ),
    ];
    let outcome = made[0]
        .check(&Observed {
            room: &surface,
            boundaries: &furnished,
        })
        .await;
    assert!(
        !outcome.held,
        "the room was furnished with the day this sitting claims, so the assertion holds \
         whatever the occupant does, and it said nothing",
    );
    assert!(
        outcome.saying.contains("furnished"),
        "the refusal does not say the furniture is the problem: {}",
        outcome.saying,
    );

    // The control: the same check, over furniture that does NOT carry the day,
    // must not reach for that refusal. Without this the case passes on a build
    // that calls every room furnished with everything.
    let clean = vec![
        at("Phase 1 — the spring sitting", "{\"records\":[]}"),
        at(
            "Phase 2 — a sitting with no day",
            "{\"records\":[{\"date\":\"2026-03-04\"}]}",
        ),
    ];
    let held = made[0]
        .check(&Observed {
            room: &surface,
            boundaries: &clean,
        })
        .await;
    assert!(
        held.held && !held.saying.contains("furnished"),
        "a room furnished with nothing on that day was called furnished with it: {}",
        held.saying,
    );
}
