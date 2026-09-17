//! **The piano's two check-in locks, held against a real room.**
//!
//! Both locks used to read `{"kind": "rhythm", "history": "last_check_in"}`
//! — a listing, not a question about the piano specifically. In a real
//! paid run of `vault.md` (`transcripts/year-run-23-2026-09-17-opus`, kept
//! outside this repo, not copied here), the operator later asked to stop
//! being reminded about the piano, and the sitting archived
//! `rhythm:sit-at-the-piano` — its own correct act, done in December.
//! `recall`'s kind-scoped listings exclude an archived entity, so by the
//! time every lock runs against the finished room, that later, unrelated
//! archival silently removed the piano from both April's and August's
//! listings — failing them for a reason that has nothing to do with
//! whether the check-in was actually written on the day. The operator's
//! own ruling is that a day is judged by its own end state, never by what
//! a later day does to it; a kind-scoped lock on an entity anything might
//! reasonably archive cannot honour that. Both locks now name the subject
//! directly, the same way the room's own December lock on this rhythm
//! already does.

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

async fn check_in(surface: &Surface, sid: &str, content: &str, day: &str) {
    surface
        .must(
            "capture",
            json!({
                "subject": "rhythm:sit-at-the-piano",
                "content": content,
                "check_in": "ran",
                "happened_at": day,
                "provenance": "testimony",
                "recorded_at": day,
                "sid": sid,
            }),
        )
        .await
        .expect("the check-in is captured");
}

/// December's real act, verbatim: dropping the reminder archives the loop.
async fn archive_the_piano(surface: &Surface, sid: &str) {
    surface
        .must(
            "archive_entity",
            json!({
                "handle": "rhythm:sit-at-the-piano",
                "reason": "Dropped at their instruction — Louise keeps them honest on the \
                           piano now, so no reminder is wanted.",
                "sid": sid,
            }),
        )
        .await
        .expect("the archival is accepted");
}

fn the_april_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("so its record of being played is short a turn")
        })
        .expect("the vault ships April's piano lock")
}

fn the_august_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("so the last logged turn at the piano is missing")
        })
        .expect("the vault ships August's piano lock")
}

/// **The fix, proven: a later, unrelated archival does not unmake April's
/// own correctly-written day.**
#[tokio::test]
async fn aprils_lock_holds_even_after_the_piano_is_later_archived() {
    let (_room, surface, sid) = furnished().await;
    check_in(
        &surface,
        &sid,
        "Sat at the piano on Sunday the 12th",
        "2026-04-12",
    )
    .await;
    archive_the_piano(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_april_lock().check(&seen).await;
    assert!(
        outcome.held,
        "April's own check-in was correctly written, and a later archival still broke the \
         lock: {}",
        outcome.saying,
    );
}

/// **The paired negative: the lock still reddens when the check-in
/// genuinely was never written**, archival or not.
#[tokio::test]
async fn aprils_lock_reds_when_the_check_in_was_never_written() {
    let (_room, surface, sid) = furnished().await;
    archive_the_piano(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_april_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "no check-in was ever written, and the lock held anyway: {}",
        outcome.saying,
    );
}

/// **The same fix, proven for August.**
#[tokio::test]
async fn augusts_lock_holds_even_after_the_piano_is_later_archived() {
    let (_room, surface, sid) = furnished().await;
    check_in(
        &surface,
        &sid,
        "Sat down at the piano on Tuesday the 11th",
        "2026-08-11",
    )
    .await;
    archive_the_piano(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_august_lock().check(&seen).await;
    assert!(
        outcome.held,
        "August's own check-in was correctly written, and a later archival still broke the \
         lock: {}",
        outcome.saying,
    );
}

/// **The paired negative for August.**
#[tokio::test]
async fn augusts_lock_reds_when_the_check_in_was_never_written() {
    let (_room, surface, sid) = furnished().await;
    archive_the_piano(&surface, &sid).await;

    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };
    let outcome = the_august_lock().check(&seen).await;
    assert!(
        !outcome.held,
        "no check-in was ever written, and the lock held anyway: {}",
        outcome.saying,
    );
}
