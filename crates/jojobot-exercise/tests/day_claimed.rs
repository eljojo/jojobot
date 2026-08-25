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

/// **A world in the shape a boundary really holds**: the inventory, then
/// everything the index can see, on two lines.
///
/// ⚠️ **The fixtures here used to be a shape no boundary ever has** —
/// `{"records":[…]}` with no address — and the check passed them because it
/// scanned the text for the day rather than reading the records. **A check
/// that reads structure has to be fed structure, and the old fixtures are why
/// a containment scan survived this long.**
fn world(records: &[(&str, &str)]) -> String {
    let hits: Vec<String> = records
        .iter()
        .map(|(address, date)| format!("{{\"address\":\"{address}\",\"recorded_at\":\"{date}\"}}"))
        .collect();
    format!("{{\"entities\":[]}}\n{{\"results\":[{}]}}", hits.join(","))
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
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
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
        at("Phase 1 — the spring sitting", &world(&[])),
        at(
            "Phase 2 — a sitting with no day",
            &world(&[("thing:kettle#f1", "2026-03-04")]),
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
        at("Phase 1 — the spring sitting", &world(&[])),
        at(
            "Phase 2 — a sitting with no day",
            &world(&[("thing:kettle#f1", "2026-08-21")]),
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
/// A room seeded with a record already dated a sitting's day would make that
/// sitting's assertion hold whatever the occupant did.
///
/// ⭐ **The check no longer needs a guard against it.** It asks about the
/// records this sitting created or changed, and furniture is neither: it sits
/// in the reading before as well as the reading after. **A hazard the shape
/// removes needs no rule**, and the sitting comes back as having written
/// nothing rather than as having carried a day it never wrote.
#[tokio::test]
async fn a_day_the_room_was_furnished_with_does_not_hold_for_the_sitting() {
    let (_room, surface) = a_room().await;
    let made = days_claimed(&read());
    let furnished = vec![
        at(
            "Phase 1 — the spring sitting",
            &world(&[("thing:kettle#f1", "2026-03-04")]),
        ),
        at(
            "Phase 2 — a sitting with no day",
            &world(&[("thing:kettle#f1", "2026-03-04")]),
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
        "the room was furnished with the day this sitting claims and the assertion held anyway",
    );
    assert!(
        !outcome.applies,
        "the sitting wrote nothing, so this is neither held nor failed and must say so: {}",
        outcome.saying,
    );
}

/// ⛔️ **A sitting that wrote nothing is neither pass nor fail.**
///
/// Its job was a question: it read the store and answered in prose. **A
/// negative asked over an empty set cannot tell a dropped day from a sitting
/// that never wrote**, and reporting it as a failure blames a sitting for
/// doing what it was asked. **A paid run did exactly that.**
#[tokio::test]
async fn a_sitting_that_wrote_nothing_is_not_applicable_rather_than_failed() {
    let (_room, surface) = a_room().await;
    let made = days_claimed(&read());
    let read_only = vec![
        at(
            "Phase 1 — the spring sitting",
            &world(&[("thing:kettle#f1", "2026-01-01")]),
        ),
        at(
            "Phase 2 — a sitting with no day",
            &world(&[("thing:kettle#f1", "2026-01-01")]),
        ),
    ];
    let outcome = made[0]
        .check(&Observed {
            room: &surface,
            boundaries: &read_only,
        })
        .await;
    assert!(
        !outcome.applies,
        "a sitting that created and changed nothing was graded: {}",
        outcome.saying,
    );
    assert!(
        !outcome.held,
        "a sitting that wrote nothing was reported as having carried its day",
    );
}

/// 🚨 **Typing the day into prose is not stamping a record with it.**
///
/// The failure the old scan could not see: a sitting writes *"(recorded
/// 2026-03-04)"* inside a claim's text and stamps the record with some other
/// day. **A containment scan over the world holds; the claim it is making is
/// false.**
#[tokio::test]
async fn a_day_typed_into_prose_does_not_satisfy_the_sitting() {
    let (_room, surface) = a_room().await;
    let made = days_claimed(&read());
    let typed = vec![
        at("Phase 1 — the spring sitting", &world(&[])),
        at(
            "Phase 2 — a sitting with no day",
            // The day is in the answer's text, and the record is dated
            // something else.
            "{\"entities\":[]}\n{\"results\":[{\"address\":\"thing:kettle#f1\",\
             \"recorded_at\":\"2026-08-21\",\"content\":\"descaled it (recorded 2026-03-04)\"}]}",
        ),
    ];
    let outcome = made[0]
        .check(&Observed {
            room: &surface,
            boundaries: &typed,
        })
        .await;
    assert!(
        !outcome.held,
        "the day appears in the sitting's prose and on no record, and the check held: {}",
        outcome.saying,
    );
    assert!(
        outcome.applies,
        "the sitting wrote a record, so this is a real failure rather than not applicable",
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
