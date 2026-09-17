//! **The college course's span lock, held against a real room.**
//!
//! The fixture replays a real regression from a paid run of `vault.md`
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo, not
//! copied here). Told the operator had started the October intake — Tuesday
//! evenings from the 6th through to 10 November — the sitting captured the
//! span correctly, as `happened_at`/`happened_through` on one claim, which
//! is the right mechanism: contrast `trip_lock.rs`, where that same
//! mechanism is the WRONG one. The mistake here is purely which entity: the
//! sitting filed it on `event:kitchen-safety-course-oct-2026`, an event it
//! created fresh, rather than `org:quahog-community-college` — the subject
//! a different sitting, three months earlier, had already established as
//! where this course's facts belong.
//!
//! This lock could not be proven until `happened_through` actually reached
//! a caller — see the commit that fixed `wire.rs`'s fact renderer. Held
//! until then rather than built around the gap.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape the other lock-proof files open their
/// rooms with.
async fn furnished() -> (Room, Surface, String) {
    let (room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    expectations::seed_for(expectations::VAULT_ROOM)
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

/// **The lock, read straight off the shipped document.** Matched by its own
/// sentence rather than by position, so a lock added above it in the same
/// phase does not silently start pointing this case at the wrong one.
fn the_course_span_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("a stretch of many Tuesdays reads as a single day")
        })
        .expect("the vault ships the course span lock")
}

/// **The control: the same span, filed on the college the course actually
/// belongs to.**
#[tokio::test]
async fn the_course_span_lock_holds_when_filed_on_the_college() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "org:quahog-community-college",
                "content": "Started the October intake — Tuesday evenings from the 6th \
                            through to 10 November",
                "happened_at": "2026-10-06",
                "happened_through": "2026-11-10",
                "provenance": "testimony",
                "standing": "settled",
                "recorded_at": "2026-10-11",
                "sid": sid,
            }),
        )
        .await
        .expect("the course span is captured on the college");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_course_span_lock().check(&seen).await;
    assert!(
        outcome.held,
        "filed on the college, the lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression: the same span, filed on an event the
/// sitting invented instead of the college three months' worth of facts
/// already point to.**
#[tokio::test]
async fn the_course_span_lock_reds_when_filed_on_an_invented_event() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "add_entity",
            json!({
                "handle": "kitchen-safety-course-oct-2026",
                "kind": "event",
                "name": "Kitchen Safety Course, October 2026 Intake",
                "source": "user-named",
                "sid": sid,
            }),
        )
        .await
        .expect("the invented event is added — the real sitting made this same call");
    surface
        .must(
            "capture",
            json!({
                "subject": "event:kitchen-safety-course-oct-2026",
                "content": "Started the October intake — Tuesday evenings from the 6th \
                            through to 10 November",
                "details": "The operator's words on 2026-10-11: \"I have started the college \
                            course anyway, the October intake — Tuesday evenings from the 6th \
                            to 10 November\".",
                "fields": {"evenings": "Tuesday", "runs": "2026-10-06/2026-11-10"},
                "happened_at": "2026-10-06",
                "happened_through": "2026-11-10",
                "provenance": "testimony",
                "recorded_at": "2026-10-11",
                "sid": sid,
            }),
        )
        .await
        .expect(
            "the course span is captured — on the invented event, exactly as the real sitting \
             did",
        );

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_course_span_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the span was filed on an invented event instead of the college, and the lock held \
         anyway: {}",
        outcome.saying,
    );
}
