//! **The desk's deadline lock, held against a real room.**
//!
//! The fixture here is not invented: it replays the exact sequence a real
//! paid run of `vault.md` made in its own February and March sittings
//! (`transcripts/year-run-23-2026-09-17-opus`, kept outside this repo) —
//! the same entity, the same content, the same fields, the same dates. That
//! run's March sitting, told only "the desk stays", called `update_fact`
//! with `clear_fields` naming `runs_out` and `due_on` on the record that
//! carried the desk's return window — a write nobody asked for, which is
//! what this lock watches for.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `vault_room.rs` opens its rooms with.
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

/// **The desk, exactly as February's real sitting left it.** Not in the
/// room's own shipped world — `thing:standing-desk` is a live creation of
/// that sitting, same as it was in the real run, so this recreates it
/// through the surface rather than reading it from furniture that was
/// never there.
async fn desk_with_its_march_deadline(surface: &Surface, sid: &str) {
    surface
        .must(
            "add_entity",
            json!({
                "handle": "standing-desk",
                "kind": "thing",
                "name": "The Standing Desk",
                "source": "user-named",
                "sid": sid,
            }),
        )
        .await
        .expect("the desk is added");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:standing-desk",
                "content": "The standing desk came on the 6th, and I have thirty days from \
                            then to send it back if I don't get on with it",
                "happened_at": "2026-02-06",
                "recorded_at": "2026-02-08",
                "provenance": "testimony",
                "standing": "settled",
                "sid": sid,
            }),
        )
        .await
        .expect("the desk's arrival is captured");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:standing-desk",
                "content": "The last day to send the standing desk back is 2026-03-08",
                "derived_from": "thing:standing-desk#f1",
                "details": "Worked out, not stated: thirty days counted from the 6th of \
                            February, the day it came.",
                "fields": {"runs_out": "2026-03-08"},
                "provenance": "inference",
                "standing": "open",
                "recorded_at": "2026-02-08",
                "sid": sid,
            }),
        )
        .await
        .expect("the desk's deadline is captured");
}

/// **The lock, read straight off the shipped document.** Matched by its own
/// sentence rather than by position, so a lock added above it in the same
/// phase does not silently start pointing this case at the wrong one.
fn the_desk_history_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("nothing later can find the deadline that closed")
        })
        .expect("the vault ships the desk's history lock")
}

/// **The control: nobody touched the desk's deadline, and the lock holds.**
#[tokio::test]
async fn the_desk_history_lock_holds_when_the_window_is_left_alone() {
    let (_room, surface, sid) = furnished().await;
    desk_with_its_march_deadline(&surface, &sid).await;
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_desk_history_lock().check(&seen).await;
    assert!(
        outcome.held,
        "the desk's deadline was never touched, and the history lock still failed: {}",
        outcome.saying,
    );
}

/// **The exact real regression: told "the desk stays", a sitting clears the
/// key that carries the deadline rather than leaving it on record.** This
/// replays `update_fact address=thing:standing-desk#f2 clear_fields=[runs_out,
/// due_on]`, the call Run 23's March sitting actually made.
#[tokio::test]
async fn the_desk_history_lock_reds_when_the_window_is_cleared_outright() {
    let (_room, surface, sid) = furnished().await;
    desk_with_its_march_deadline(&surface, &sid).await;
    surface
        .must(
            "update_fact",
            json!({
                "address": "thing:standing-desk#f2",
                "clear_fields": ["runs_out", "due_on"],
                "details": "The window is closed and the operator decided the desk stays, so \
                            it will stop coming up.",
                "recorded_at": "2026-03-15",
                "sid": sid,
            }),
        )
        .await
        .expect("the clearing write itself is accepted — it is a real, sanctioned call");
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_desk_history_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "runs_out was cleared outright and the history lock held anyway: {}",
        outcome.saying,
    );
}
