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

/// A dated sitting that says its subject is an earlier day, and the marker on
/// a line of its own so this document does not also prove the marker line is
/// shared with the session marker.
const WRITES_EARLIER: &str = "\
## Phase 1 — the September sitting\n\
\n\
**Session: fresh.** **Day: 2026-09-13.**\n\
\n\
**Writes about an earlier day.**\n\
\n\
> it is the thirteenth of September and the pump came back at the June survey\n";

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

/// **A sitting that writes about an earlier day gets no generated assertion,
/// and the same sitting without the marker gets one.**
///
/// The assertion's premise is that a sitting writes under the day it was told.
/// That premise is false for a sitting whose subject is an earlier day: a
/// question about a thing lent seven months ago is answered by a record dated
/// the day the thing came back, and a date says when a claim is TRUE OF rather
/// than when somebody typed it. Asserting the sitting's own day there fails the
/// right answer.
///
/// **Both halves, because the marker is an exemption.** A build that generated
/// nothing for any dated sitting would satisfy the first assertion alone, and
/// it would take the year's strongest check off every other sitting silently.
#[test]
fn a_sitting_that_writes_about_an_earlier_day_gets_no_assertion() {
    let marked = Playbook::parse("rooms/whatever.md", WRITES_EARLIER).expect("the document reads");
    let phase = &marked.phases[0];
    assert!(
        phase.about_an_earlier_day,
        "the marker is not read off the document, so nothing below is about it",
    );
    assert_eq!(
        phase.day.as_deref(),
        Some("2026-09-13"),
        "the sitting still claims its own day — the marker says what it writes under, not that \
         it is undated",
    );
    assert!(
        days_claimed(&marked).is_empty(),
        "the run generated a day assertion for a sitting the room says writes about an earlier \
         day, so the right answer is marked a failure",
    );

    let plain = WRITES_EARLIER.replace("**Writes about an earlier day.**\n\n", "");
    let unmarked = Playbook::parse("rooms/whatever.md", &plain).expect("the document reads");
    assert!(
        !unmarked.phases[0].about_an_earlier_day,
        "this half is reading the wrong document: the marker is still on it",
    );
    assert_eq!(
        days_claimed(&unmarked).len(),
        1,
        "the same sitting without the marker gets its assertion, so the exemption is the marker \
         rather than something that stopped generating them at all",
    );
}
