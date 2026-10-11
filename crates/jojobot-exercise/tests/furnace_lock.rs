//! **The furnace's free-service deadline lock, held against a real room.**
//!
//! The fixture replays a real regression from a paid run of `vault.md`
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo, not
//! copied here). Told the furnace's free yearly service lapses if not
//! booked before 1 February, the sitting worked out the last bookable day
//! correctly but filed it as a fact about `org:mr-plow` — the service
//! company — rather than `thing:the-furnace`, the thing the deadline is
//! actually about and the subject every other window in this room is filed
//! under. Both entities are furniture this room already ships; the mistake
//! is which one the fact landed on.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `vault_room.rs`, `desk_lock.rs` and
/// `wharf_lock.rs` open their rooms with.
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
fn the_furnace_deadline_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found.name().contains(
                "the one window nobody would think to call a deadline is not on record as a date",
            )
        })
        .expect("the vault ships the furnace deadline lock")
}

/// **The control: the deadline filed on the furnace itself.**
#[tokio::test]
async fn the_furnace_deadline_lock_holds_when_filed_on_the_furnace() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:the-furnace",
                "content": "The last day to book next year's free service on the Mr. Plow \
                            plan is 1 February 2027",
                "details": "Worked out by the assistant: the operator said the service has to \
                            be booked before 1 February, so the deadline is the last day of \
                            January reads as either 31 January or 1 February depending on how \
                            the boundary counts — filed here as the day the plan itself names.",
                "fields": {"runs_out": "2027-02-01"},
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-05-10",
                "sid": sid,
            }),
        )
        .await
        .expect("the furnace deadline is captured");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_furnace_deadline_lock().check(&seen).await;
    assert!(
        outcome.held,
        "filed correctly on the furnace, the lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression: the same deadline, filed on the service
/// company instead of the thing it services.**
#[tokio::test]
async fn the_furnace_deadline_lock_reds_when_filed_on_the_service_company() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "org:mr-plow",
                "content": "The plan includes one free service a year, and it lapses if it is \
                            not used before 1 February — the next one has to be booked before \
                            1 February 2027",
                "details": "Worked out by the assistant, not said by the operator: the last \
                            bookable day, filed on the plan's own company record.",
                "object": "thing:the-furnace",
                "shape": "about",
                "fields": {"runs_out": "2027-02-01"},
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-05-10",
                "sid": sid,
            }),
        )
        .await
        .expect("the deadline is captured — on the company, exactly as the real sitting filed it");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_furnace_deadline_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the deadline was filed on the service company instead of the furnace, and the lock \
         held anyway: {}",
        outcome.saying,
    );
}
