//! **A windowed lock, proved against a real room, both halves of the claim.**
//!
//! A lock that reads its own phase's boundary must (a) stay blind to a write
//! that happens after that phase, which is the entire reason to build this,
//! and (b) stay blind to a DIFFERENT subject's write in the SAME phase, which
//! is what the isolation half is for. Both are proved by running the actual
//! shipped `Lock::check`, never a reimplementation of it.
//!
//! `Observed::across(phase)` reads the boundary labelled with that phase's own
//! name as the BEFORE snapshot and the very next one as the AFTER — the same
//! shape `run::go()` records: one boundary ahead of every phase, labelled
//! with the phase about to run. So proving anything about "Phase 1"'s own
//! boundary needs two boundaries: one taken before phase 1's writes, labelled
//! "Phase 1", and one taken after them, labelled with whatever comes next.

use jojobot_exercise::lock::{Lock, read};
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::{Boundary, Expectation, Observed, boundary};
use jojobot_exercise::surface::Seed;
use serde_json::json;

fn windowed_lock(subject: &str, carries: &str, lacks: &str) -> Lock {
    let doc = format!(
        "## Phase 1 — the room\n\n\
         ```locks\n\
         recall {{\"subject\": \"{subject}\", \"facts\": true}}\n\
         carries {carries}\n\
         lacks   {lacks}\n\
         say     windowed to phase 1\n\
         window  own-phase\n\
         ```\n"
    );
    read(&doc).expect("the lock reads").remove(0)
}

fn live_lock(subject: &str, carries: &str, lacks: &str) -> Lock {
    let doc = format!(
        "## Phase 1 — the room\n\n\
         ```locks\n\
         recall {{\"subject\": \"{subject}\", \"facts\": true}}\n\
         carries {carries}\n\
         lacks   {lacks}\n\
         say     read live, against the finished room\n\
         ```\n"
    );
    read(&doc).expect("the lock reads").remove(0)
}

/// **The core claim: a windowed lock cannot see a write made after its own
/// phase, where a live lock — reading the finished room — cannot help but
/// see it.** This is the exact shape of the kitchen-floor/piano bug this
/// capability exists to let a future lock avoid.
#[tokio::test]
async fn a_windowed_lock_stays_blind_to_a_write_made_after_its_own_phase() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    Seed::new()
        .entity("person", "milhouse", "Milhouse")
        .expect("an entity builds")
        .furnish(&surface)
        .await
        .expect("furnished");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("boot");
    let sid = booted["session"]["sid"].as_str().expect("sid").to_string();

    // Before phase 1's own write.
    let before_phase_one: Boundary = boundary(&surface, "Phase 1 — the room").await;

    surface
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "phase one's own marker qwertyone",
                "provenance": "testimony",
                "sid": &sid,
            }),
        )
        .await;
    // After phase 1's own write — labelled with the phase that comes next,
    // exactly as `run::go()` labels it.
    let after_phase_one: Boundary = boundary(&surface, "Phase 2 — later").await;

    // A write that happens AFTER phase one's boundary was taken — the same
    // shape as a later phase superseding or archiving what an earlier lock
    // checked.
    surface
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "a later phase's own marker qwertytwo",
                "provenance": "testimony",
                "sid": &sid,
            }),
        )
        .await;

    let boundaries = vec![before_phase_one, after_phase_one];
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };

    let windowed = windowed_lock("person:milhouse", "qwertyone", "qwertytwo");
    let outcome = windowed.check(&seen).await;
    assert!(
        outcome.held,
        "the windowed lock saw the later write, so windowing bought nothing: {}",
        outcome.saying,
    );

    // **Paired with the failure this replaces.** The identical needles, read
    // live: by the time `check` runs, both facts exist, so `lacks
    // "qwertytwo"` cannot hold. This is the bug windowing exists to avoid.
    let live = live_lock("person:milhouse", "qwertyone", "qwertytwo");
    let live_outcome = live.check(&seen).await;
    assert!(
        !live_outcome.held,
        "the live lock did not see the later write either, so this pair proves nothing: {}",
        live_outcome.saying,
    );
}

/// **The isolation half, proved through the same `check` path**: two
/// subjects written in the SAME phase, one lock windowed and scoped to one
/// of them must not trip on the other's value — the exact collision found
/// live between vault.md's October drive locks.
#[tokio::test]
async fn a_windowed_lock_stays_blind_to_a_different_subjects_write_in_the_same_phase() {
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    Seed::new()
        .entity("person", "milhouse", "Milhouse")
        .expect("an entity builds")
        .entity("person", "bart", "Bart")
        .expect("an entity builds")
        .furnish(&surface)
        .await
        .expect("furnished");
    let booted = surface
        .must("start_here", json!({"bot": "assistant", "brief": true}))
        .await
        .expect("boot");
    let sid = booted["session"]["sid"].as_str().expect("sid").to_string();

    let before_phase_one: Boundary = boundary(&surface, "Phase 1 — the room").await;

    surface
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "milhouse's own marker qwertymilhouse",
                "provenance": "testimony",
                "sid": &sid,
            }),
        )
        .await;
    surface
        .call(
            "capture",
            json!({
                "subject": "person:bart",
                "content": "bart's own marker qwertybart",
                "provenance": "testimony",
                "sid": &sid,
            }),
        )
        .await;
    let after_phase_one: Boundary = boundary(&surface, "Phase 2 — later").await;

    let boundaries = vec![before_phase_one, after_phase_one];
    let seen = Observed {
        room: &surface,
        boundaries: &boundaries,
    };

    // Milhouse's own lock must carry his marker and lack bart's — a flat,
    // unisolated dump would fail the `lacks` half, since bart's marker is
    // genuinely present in the same phase's boundary text.
    let windowed = windowed_lock("person:milhouse", "qwertymilhouse", "qwertybart");
    let outcome = windowed.check(&seen).await;
    assert!(
        outcome.held,
        "isolating by person:milhouse still tripped on bart's marker: {}",
        outcome.saying,
    );
}
