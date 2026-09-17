//! **The discriminating case pm asked for**: not a sample, the proof.
//!
//! Uses the real, original text of vault.md's January kitchen-floor lock —
//! `recall {"subject": "project:kitchen-floor"} carries "status":"considering"`
//! — exactly as it shipped before c69e61f converted it to a history read.
//! Replayed against the room's own real narrative (considering in January,
//! doing in April, done in July — vault.md's own December phase says so in
//! as many words), the live version of that lock is run against the
//! finished room and the windowed version against January's own boundary.
//! The live lock must be WRONG (the bug run 23 actually found); the
//! windowed lock must be RIGHT.

use jojobot_exercise::expectations;
use jojobot_exercise::lock::{Lock, read};
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed, boundary};
use jojobot_exercise::surface::Surface;
use serde_json::json;

/// The real, original January lock, exactly as vault.md shipped it before
/// c69e61f -- verified against `git show c69e61f^:crates/jojobot-exercise/rooms/vault.md`.
const ORIGINAL_JANUARY_LOCK: &str = "recall {\"subject\": \"project:kitchen-floor\"}\ncarries \"status\":\"considering\"\nsay     January: the kitchen floor does not hold considering under the operator's own key, so it cannot be told apart from the shed when December asks which one moved";

fn as_shipped(phase: &str) -> Lock {
    let doc = format!("## {phase} — x\n\n```locks\n{ORIGINAL_JANUARY_LOCK}\n```\n");
    read(&doc).expect("the original lock still reads").remove(0)
}

fn windowed(phase: &str) -> Lock {
    let doc =
        format!("## {phase} — x\n\n```locks\n{ORIGINAL_JANUARY_LOCK}\nwindow  own-phase\n```\n");
    read(&doc).expect("the windowed clone reads").remove(0)
}

async fn as_occupant(room: &Surface, sid: &str, verb: &str, mut args: serde_json::Value) -> String {
    args["sid"] = json!(sid);
    room.call(verb, args).await
}

#[tokio::test]
async fn the_live_lock_is_wrong_and_the_windowed_lock_is_right() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
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

    // January: the real value this lock is actually about.
    let before_january: Boundary = boundary(&surface, "Phase 1 — x").await;
    as_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "project:kitchen-floor",
            "content": "the kitchen floor is considering",
            "provenance": "testimony",
            "fields": {"status": "considering"},
        }),
    )
    .await;
    let after_january: Boundary = boundary(&surface, "Phase 2 — x").await;

    // April and July: the room's own real narrative -- "the floor went
    // considering, doing, done" -- landing after January's boundary was
    // already taken.
    as_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "project:kitchen-floor",
            "content": "the kitchen floor is doing",
            "provenance": "testimony",
            "fields": {"status": "doing"},
        }),
    )
    .await;
    as_occupant(
        &surface,
        &sid,
        "capture",
        json!({
            "subject": "project:kitchen-floor",
            "content": "the kitchen floor is done",
            "provenance": "testimony",
            "fields": {"status": "done"},
        }),
    )
    .await;

    // THE LIVE LOCK: the original, unmodified shipped text, run the way
    // every lock in every room is run today -- against the finished room.
    let final_boundaries: Vec<Boundary> = Vec::new();
    let final_seen = Observed {
        room: &surface,
        boundaries: &final_boundaries,
    };
    let live = as_shipped("Phase 1");
    let live_outcome = live.check(&final_seen).await;
    assert!(
        !live_outcome.held,
        "the live lock held against the finished room, so this is not the bug run 23 found: {}",
        live_outcome.saying,
    );

    // THE WINDOWED LOCK: identical query and needle, read at January's own
    // boundary instead.
    let january_boundaries = vec![before_january, after_january];
    let january_seen = Observed {
        room: &surface,
        boundaries: &january_boundaries,
    };
    let windowed_outcome = windowed("Phase 1").check(&january_seen).await;
    assert!(
        windowed_outcome.held,
        "the windowed lock did not recover the January value: {}",
        windowed_outcome.saying,
    );
}
