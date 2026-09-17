//! **The trip-dates lock, held against a real room.**
//!
//! The fixture replays a real regression from a paid run of `vault.md`
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo, not
//! copied here). Told about a trip booked for 12 to 22 November, the
//! sitting recorded the span using the claim-timing convention —
//! `happened_at`/`happened_through`, when a claim's own event happened —
//! rather than the shipped `trip` type's own required fields, `leaves_on`
//! and `returns_on`. The dates were real and on record; they were simply
//! not written under the keys the shipped "when am I next away" question
//! reads by.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `vault_room.rs`, `desk_lock.rs`,
/// `wharf_lock.rs` and `furnace_lock.rs` open their rooms with.
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
fn the_trip_dates_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("invisible to the question \"when am I next away\"")
        })
        .expect("the vault ships the trip dates lock")
}

/// **The control: the trip's dates filed as `leaves_on`/`returns_on`, the
/// shipped `trip` type's own required fields.**
#[tokio::test]
async fn the_trip_dates_lock_holds_when_filed_under_the_shipped_keys() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "place:capital-city",
                "content": "@person:linda and I are going to @place:capital-city from 12 to \
                            22 November, booked on the 12th",
                "object": "person:linda",
                "shape": "connection",
                "fields": {"leaves_on": "2026-11-12", "returns_on": "2026-11-22"},
                "provenance": "testimony",
                "standing": "settled",
                "recorded_at": "2026-09-12",
                "sid": sid,
            }),
        )
        .await
        .expect("the trip is captured under the shipped keys");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_trip_dates_lock().check(&seen).await;
    assert!(
        outcome.held,
        "filed under leaves_on/returns_on, the lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression: the same trip, dated with `happened_at` and
/// `happened_through` — a claim's own timing — instead of the shipped
/// `trip` type's fields.**
#[tokio::test]
async fn the_trip_dates_lock_reds_when_filed_as_happened_at_and_through() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "place:capital-city",
                "content": "@person:linda and I are going to @place:capital-city from 12 to \
                            22 November, booked on the 12th",
                "details": "Dated the 12th, the day the booking was made — 2026-09-12 is when \
                            this was said.",
                "object": "person:linda",
                "shape": "connection",
                "fields": {"booked_on": "2026-09-12"},
                "happened_at": "2026-11-12",
                "happened_through": "2026-11-22",
                "provenance": "testimony",
                "standing": "settled",
                "recorded_at": "2026-09-12",
                "sid": sid,
            }),
        )
        .await
        .expect(
            "the trip is captured — dated by happened_at/through, exactly as the real sitting \
             did",
        );

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_trip_dates_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the trip's dates were filed as happened_at/happened_through instead of the shipped \
         fields, and the lock held anyway: {}",
        outcome.saying,
    );
}
