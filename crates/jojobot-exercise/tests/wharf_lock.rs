//! **The wharf road's timing lock, held against a real room.**
//!
//! The fixture replays a real regression from a paid run of `vault.md`
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo, not
//! copied here). Told "on Friday the 6th I went in by the wharf road — 52
//! minutes, never again", that sitting invented a new entity —
//! `place:wharf-road` — and filed the timing there, rather than recognising
//! `place:wonder-wharf`: furniture this room already ships under that exact
//! name. The fact was real; it was filed on a place nothing else in the
//! store points at, so October's own question about it finds nothing.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `vault_room.rs` and `desk_lock.rs` open
/// their rooms with.
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
fn the_wharf_timing_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("so October cannot see that the verdict rests on one morning")
        })
        .expect("the vault ships the wharf timing lock")
}

/// **The control: the timing filed on the real, furnished place.**
#[tokio::test]
async fn the_wharf_timing_lock_holds_when_filed_on_the_real_place() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "place:wonder-wharf",
                "content": "Went in to work by the wharf road on Friday the 6th while the \
                            bridge is shut — 52 minutes, never again",
                "fields": {"minutes": "52"},
                "happened_at": "2026-02-06",
                "recorded_at": "2026-02-08",
                "provenance": "testimony",
                "standing": "settled",
                "sid": sid,
            }),
        )
        .await
        .expect("the wharf timing is captured");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_wharf_timing_lock().check(&seen).await;
    assert!(
        outcome.held,
        "filed correctly on the real place, the lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression: the same fact, filed on an invented entity
/// instead of the furniture that already represents it.**
#[tokio::test]
async fn the_wharf_timing_lock_reds_when_filed_on_an_invented_entity() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "add_entity",
            json!({
                "handle": "wharf-road",
                "kind": "place",
                "name": "The Wharf Road",
                "source": "user-named",
                "sid": sid,
            }),
        )
        .await
        .expect("the invented entity is added — the real sitting made this same call");
    surface
        .must(
            "capture",
            json!({
                "subject": "place:wharf-road",
                "content": "Went in to work by the wharf road on Friday the 6th while the \
                            bridge is shut — 52 minutes, never again",
                "fields": {"minutes": "52"},
                "happened_at": "2026-02-06",
                "recorded_at": "2026-02-08",
                "provenance": "testimony",
                "standing": "settled",
                "sid": sid,
            }),
        )
        .await
        .expect("the timing is captured — on the wrong place, exactly as the real sitting did");

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_wharf_timing_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "the timing was filed on an invented entity instead of the real place, and the lock \
         held anyway: {}",
        outcome.saying,
    );
}
