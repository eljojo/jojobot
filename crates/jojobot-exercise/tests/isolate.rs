//! **Isolation, proved against a boundary a real room actually produced** —
//! never a hand-rolled fixture standing in for one.

use jojobot_exercise::isolate::boundary_text;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::boundary;
use jojobot_exercise::surface::Seed;
use serde_json::json;

async fn two_subjects_boundary() -> (String, &'static str, &'static str) {
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
    surface
        .call(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "milhouse carries the marker qwertymilhouse",
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await;
    surface
        .call(
            "capture",
            json!({
                "subject": "person:bart",
                "content": "bart carries the marker qwertybart",
                "provenance": "testimony",
                "sid": sid,
            }),
        )
        .await;
    let world = boundary(&surface, "test").await.world;
    (world, "qwertymilhouse", "qwertybart")
}

/// **The proof this module exists for.** Both markers are in the same
/// boundary blob (both subjects were captured before it was taken) — naming
/// one subject must keep its own marker and drop the other's.
#[tokio::test]
async fn isolating_by_subject_keeps_that_subjects_facts_and_drops_the_others() {
    let (world, milhouse_marker, bart_marker) = two_subjects_boundary().await;
    assert!(
        world.contains(milhouse_marker) && world.contains(bart_marker),
        "the boundary itself does not carry both markers, so this proves nothing: {world}",
    );

    let isolated = boundary_text(&world, Some("person:milhouse"));
    assert!(
        isolated.contains(milhouse_marker),
        "isolating by person:milhouse dropped its own marker: {isolated}",
    );
    assert!(
        !isolated.contains(bart_marker),
        "isolating by person:milhouse kept bart's marker too: {isolated}",
    );
}

/// **The other subject, isolated the same way** — proves the function is not
/// hard-wired to whichever subject the first test happened to name.
#[tokio::test]
async fn isolating_by_the_other_subject_keeps_its_facts_and_drops_the_first() {
    let (world, milhouse_marker, bart_marker) = two_subjects_boundary().await;
    let isolated = boundary_text(&world, Some("person:bart"));
    assert!(
        isolated.contains(bart_marker),
        "isolating by person:bart dropped its own marker: {isolated}",
    );
    assert!(
        !isolated.contains(milhouse_marker),
        "isolating by person:bart kept milhouse's marker too: {isolated}",
    );
}

/// **No subject named is a pass-through**, unchanged from the boundary's own
/// text — a kind-scoped or store-wide lock has no folding or filtering to
/// protect against, so there is nothing here to isolate.
#[tokio::test]
async fn naming_no_subject_returns_the_boundary_unchanged() {
    let (world, milhouse_marker, bart_marker) = two_subjects_boundary().await;
    let passthrough = boundary_text(&world, None);
    assert_eq!(
        passthrough, world,
        "a call naming no subject must be the boundary's own text, unchanged",
    );
    assert!(passthrough.contains(milhouse_marker) && passthrough.contains(bart_marker));
}
