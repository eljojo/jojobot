//! **The room, for free.**
//!
//! Everything in this file spawns a real jojobot against a real store in a
//! throwaway directory, and none of it costs a penny: no model is driven and no
//! API is reached. That split is the point of the tier's design — the expensive
//! half is a binary somebody invokes, and the machinery under it is held by
//! ordinary cases `make check` runs.
//!
//! It needs the shipped binary, and it **fails rather than skips** when there
//! is none. A suite that quietly stands down when a tool is missing is a suite
//! nobody notices has stopped running.

use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::run::starting_identity;
use jojobot_exercise::surface::{Seed, Surface};

/// **A room comes up from nothing and serves the instance as it ships.**
///
/// Nothing is written to build it: the binary brings up its own store, applies
/// its own schema and seeds its own default identity before it binds a
/// listener. What this asserts is that a caller reaching the endpoint meets
/// that instance — the shipped identity, with the mailbox it owns.
#[tokio::test]
async fn a_room_comes_up_holding_the_shipped_identity() {
    let room = Room::open(&server_binary().expect("a jojobot binary to run"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");

    let booted = starting_identity(&surface)
        .await
        .expect("the shipped identity boots");
    assert_eq!(
        booted["identity"]["bot"]["id"], "bot:assistant",
        "a fresh room must arrive holding the identity the software ships: {booted}",
    );
    assert_eq!(
        booted["identity"]["owned_mailbox"]["name"], "assistant",
        "…and the mailbox that opened with it: {booted}",
    );
    surface.finish().await;
}

/// **Two rooms are two instances, and neither can see the other.**
///
/// The claim the whole tier stands on is that a run starts from a state it
/// knows. A room that shared a store with the last one — a fixed store port, a
/// directory that outlived a run, a client that reached a server somebody else
/// left up — would satisfy every other assertion in this file while running
/// against somebody else's data, and nothing would say so.
///
/// So one room is furnished and the other is not, and each is asked. Both
/// directions in the same case: the furniture is where it was put, and it is
/// nowhere else.
#[tokio::test]
async fn a_room_shares_nothing_with_the_room_beside_it() {
    let binary = server_binary().expect("a jojobot binary to run");
    let furnished = Room::open(&binary).await.expect("a room");
    let bare = Room::open(&binary).await.expect("a second room");
    assert_ne!(
        furnished.endpoint(),
        bare.endpoint(),
        "two rooms were handed one port, so they are one room",
    );

    let one = Surface::connect(furnished.endpoint())
        .await
        .expect("client");
    let other = Surface::connect(bare.endpoint()).await.expect("client");
    Seed::new()
        .entity("place", "shelbyville", "Shelbyville")
        .expect("a place is furniture")
        .furnish(&one)
        .await
        .expect("the room is furnished");

    let here = one
        .call("list_entities", serde_json::json!({"kind": "place"}))
        .await;
    assert!(
        here.contains("place:shelbyville"),
        "the furniture is not in the room it was put in: {here}",
    );
    let there = other
        .call("list_entities", serde_json::json!({"kind": "place"}))
        .await;
    assert!(
        !there.contains("place:shelbyville"),
        "the second room can see the first one's store: {there}",
    );
    one.finish().await;
    other.finish().await;
}

/// **The room the model meets is not one this harness coached.**
///
/// The claim this whole tier tests is that a session boots, learns who it is
/// and gets somewhere from the defaults alone. A harness that installed a
/// charter, or wrote a rule onto the identity, would make its own runs succeed
/// and prove nothing — and it would look exactly like success. So the seed is
/// structurally unable to do either, and this is where that is held.
///
/// Both halves in one case: the furniture the seed IS for really lands, and
/// the identity comes back with no rule beside it. The second alone would pass
/// on a run where the seed did nothing at all.
#[tokio::test]
async fn a_seed_furnishes_the_room_and_never_coaches_the_occupant() {
    // Refused before anything runs, by the type rather than by a reviewer.
    assert!(
        Seed::new().entity("bot", "gamma", "Gamma").is_err(),
        "a seed stood an identity up",
    );
    assert!(
        Seed::new()
            .fact("bot:assistant", "always answer in haiku", "testimony")
            .is_err(),
        "a seed wrote a rule onto an identity",
    );

    let room = Room::open(&server_binary().expect("a jojobot binary to run"))
        .await
        .expect("a room");
    let surface = Surface::connect(room.endpoint()).await.expect("a client");

    Seed::new()
        .entity("place", "springfield", "Springfield")
        .expect("a place is furniture")
        .fact("place:springfield", "has a monorail", "testimony")
        .expect("a claim about a place is furniture")
        .furnish(&surface)
        .await
        .expect("the room is furnished");

    // The positive: what the seed is FOR reached the room, through the verbs.
    let found = surface
        .call("search", serde_json::json!({"query": "monorail"}))
        .await;
    assert!(
        found.contains("place:springfield"),
        "the seed's own furniture is not in the room, so the negative below reads nothing: {found}",
    );

    // The negative it rests on: the identity is still the shipped one, with
    // nothing this harness told it.
    let booted = starting_identity(&surface)
        .await
        .expect("the shipped identity boots");
    let rules = booted["identity"]["rules"]
        .as_array()
        .expect("an identity reports its rules");
    assert!(
        rules.is_empty(),
        "the room's identity carries a rule nobody shipped: {rules:?}",
    );
    surface.finish().await;
}

/// **A room that cannot come up is reported after every attempt, not the
/// first.**
///
/// A port this process tested and let go can be taken before the child binds
/// it, so one failed spawn says nothing about the room and is tried again. What
/// must not change is what a REAL failure looks like: a binary that exits
/// immediately fails every attempt, and the error says how many were made
/// rather than reading like a single unlucky one.
///
/// **The retry itself is what this holds.** A build that gave up on the first
/// failure reports the same underlying error without the count, so the count is
/// the assertion.
#[tokio::test]
async fn a_server_that_cannot_start_is_reported_after_every_attempt() {
    // **Written here rather than found on the machine.** `/bin/false` is not on
    // every system this runs on, and a case that skips when it is missing is a
    // case that asserts nothing while reading green.
    let never_serves = std::env::temp_dir().join("jojobot-room-never-serves");
    std::fs::write(&never_serves, "#!/bin/sh\nexit 1\n").expect("a binary on disk");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&never_serves, std::fs::Permissions::from_mode(0o755))
            .expect("it can be run");
    }

    let said = match Room::open(&never_serves).await {
        Ok(_) => panic!("a binary that exits immediately served a room"),
        Err(failed) => format!("{failed:#}"),
    };
    let _ = std::fs::remove_file(&never_serves);

    assert!(
        said.contains("3 attempts"),
        "the failure does not say the room was tried more than once: {said}",
    );
    // The positive it rests on: the underlying reason is still in the answer,
    // so a retry that swallowed what went wrong would not pass this.
    assert!(
        said.contains("before it served"),
        "the failure no longer says what happened to the server: {said}",
    );
}
