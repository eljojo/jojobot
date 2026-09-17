//! **The vault's archived-exclusion counter, held against a real room.**
//!
//! There is no synthetic occupant for `vault.md` yet — unlike `year.md`, no
//! driver plays its thirteen sittings by hand. What is under test here is
//! one lock in isolation: December's everyday listing of people says how
//! many an ordinary browse left out, not only which ones. The room is
//! furnished for real and the archival is made through the surface, the way
//! a sitting would make it — the lock is then read straight off the shipped
//! document and run against that state.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `bike_room.rs` opens its rooms with.
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

/// **December's counted listing, read straight off the shipped document.**
///
/// Matched by its own sentence rather than by position, so a lock added
/// above it in the same phase does not silently start pointing this case at
/// the wrong one.
fn the_counted_listing_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("has no way to tell an empty vault from one quietly missing a name")
        })
        .expect("the vault ships the everyday-listing count lock")
}

async fn archive(surface: &Surface, sid: &str, handle: &str, reason: &str) {
    surface
        .must(
            "archive_entity",
            json!({"handle": handle, "reason": reason, "sid": sid}),
        )
        .await
        .unwrap_or_else(|e| panic!("{handle} did not archive: {e:#}"));
}

/// **The negative this rests on: nobody archived, so the lock must red.**
///
/// Hugo is furniture and stands by default — an everyday listing finds him,
/// which fails both the presence half of the lock and, since he was never
/// excluded, the count half too.
#[tokio::test]
async fn the_count_lock_reds_when_nobody_was_archived() {
    let (_room, surface, _sid) = furnished().await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "a room nobody touched held the count lock: {}",
        outcome.saying,
    );
}

/// **The second negative, isolated from the first: the exclusion is right
/// and the COUNT alone is wrong.**
///
/// Hugo is archived, so the presence half of the lock is satisfied on its
/// own — Hugo is out, Gayle is still in. A second person is archived beside
/// him, so the everyday listing now excludes two rather than one. A lock
/// that held here would be proving nothing about the count it names.
#[tokio::test]
async fn the_count_lock_reds_when_the_count_alone_is_wrong() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    archive(&surface, &sid, "person:gene", "not relevant to this test").await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the everyday listing excluded two rather than one, and the count lock held anyway: {}",
        outcome.saying,
    );
}

/// **The positive: exactly Hugo archived, and the lock holds.**
#[tokio::test]
async fn the_count_lock_holds_once_exactly_hugo_is_archived() {
    let (_room, surface, sid) = furnished().await;
    archive(&surface, &sid, "person:hugo", "not real to me").await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_counted_listing_lock().check(&seen).await;
    assert!(
        outcome.held,
        "exactly one archival (Hugo) did not satisfy the count lock: {}",
        outcome.saying,
    );
}
