//! **December's two Globex locks, held against a real room.**
//!
//! Both used to pin the literal word "Globex" in a `recall` answer. A
//! sitting that links the purchase correctly — `@org:globex`, jojobot's own
//! mention convention — never produces that word: a stored mention renders
//! back as the handle, never the resolved display name, inside another
//! entity's fact. The rewrite checks for the handle, `org:globex`, instead.

use jojobot_exercise::expectations;
use jojobot_exercise::lock;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// A room furnished with the vault's own cast, and a booted identity to
/// write through — the same shape `desk_lock.rs` opens its rooms with.
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

fn the_laptop_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("the laptop's cover runs to 2028 and was given a note anyway")
        })
        .expect("the vault ships the laptop's Globex lock")
}

fn the_phone_lock() -> lock::Lock {
    lock::locks_of(expectations::VAULT_ROOM)
        .into_iter()
        .find(|found| {
            found
                .name()
                .contains("the phone's cover runs to 2028 and was given a note anyway")
        })
        .expect("the vault ships the phone's Globex lock")
}

async fn check(surface: &Surface, lock: &lock::Lock) -> jojobot_exercise::run::Outcome {
    let boundaries: Vec<Boundary> = Vec::new();
    let seen = Observed {
        room: surface,
        boundaries: &boundaries,
    };
    lock.check(&seen).await
}

/// **The control: the laptop's cover is linked to `org:globex` the way
/// `@org:globex` in a claim's own words actually renders.** No note dated
/// today — the far window is left alone, as the operator asked.
#[tokio::test]
async fn the_laptop_lock_holds_when_the_cover_is_linked_by_handle() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "machine:theta",
                "content": "Bought Theta from @org:globex on 2026-01-10, with two years of \
                            cover from that day.",
                "shape": "about",
                "object": "org:globex",
                "fields": {"runs_out": "2028-01-10"},
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await
        .expect("the cover is captured");
    let outcome = check(&surface, &the_laptop_lock()).await;
    assert!(
        outcome.held,
        "the cover was linked to org:globex by handle, and the lock still failed: {}",
        outcome.saying,
    );
}

/// **The real regression: the cover is on record with the far window
/// correctly left alone, but the source was never linked — only said in
/// prose.** A sitting that writes `@org:globex` never produces the literal
/// word "Globex" in what comes back; one that writes unlinked prose does,
/// and used to pass this lock for the weaker answer.
#[tokio::test]
async fn the_laptop_lock_reds_when_the_source_is_never_linked() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "capture",
            json!({
                "subject": "machine:theta",
                "content": "Bought Theta on 2026-01-10, with two years of cover from that day.",
                "fields": {"runs_out": "2028-01-10"},
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await
        .expect("the cover is captured");
    let outcome = check(&surface, &the_laptop_lock()).await;
    assert!(
        !outcome.held,
        "the source was never linked to org:globex, and the lock held anyway: {}",
        outcome.saying,
    );
}

/// **The same control, the phone.**
#[tokio::test]
async fn the_phone_lock_holds_when_the_cover_is_linked_by_handle() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "add_entity",
            json!({
                "handle": "mobile-phone",
                "kind": "thing",
                "name": "The Phone",
                "source": "user-named",
                "sid": sid,
            }),
        )
        .await
        .expect("the phone is added");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:mobile-phone",
                "content": "Bought the phone from @org:globex on 2026-07-10, with two years \
                            of cover from that day.",
                "shape": "about",
                "object": "org:globex",
                "fields": {"runs_out": "2028-07-10"},
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await
        .expect("the cover is captured");
    let outcome = check(&surface, &the_phone_lock()).await;
    assert!(
        outcome.held,
        "the cover was linked to org:globex by handle, and the lock still failed: {}",
        outcome.saying,
    );
}

/// **The same regression, the phone.**
#[tokio::test]
async fn the_phone_lock_reds_when_the_source_is_never_linked() {
    let (_room, surface, sid) = furnished().await;
    surface
        .must(
            "add_entity",
            json!({
                "handle": "mobile-phone",
                "kind": "thing",
                "name": "The Phone",
                "source": "user-named",
                "sid": sid,
            }),
        )
        .await
        .expect("the phone is added");
    surface
        .must(
            "capture",
            json!({
                "subject": "thing:mobile-phone",
                "content": "Bought the phone on 2026-07-10, with two years of cover from that \
                            day.",
                "fields": {"runs_out": "2028-07-10"},
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await
        .expect("the cover is captured");
    let outcome = check(&surface, &the_phone_lock()).await;
    assert!(
        !outcome.held,
        "the source was never linked to org:globex, and the lock held anyway: {}",
        outcome.saying,
    );
}
