//! **The reader is held against the documents a run is actually driven by.**
//!
//! The unit cases in the reader use a fixture, and a fixture is something this
//! crate wrote — so on its own it proves the reader can read what this crate
//! expects. The documents are authored beside them, and the seam between the
//! two is exactly where this breaks: a heading style that changes, a marker
//! written another way, a block that stops being a block quote.
//!
//! **And the registry is the other half.** A room whose document nobody
//! registered is a document no run will drive, and a room registered under a
//! name no document carries is a run that refuses at the door. Both are silent,
//! and both are what this file is for.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{Room, server_binary};
use jojobot_exercise::surface::Seed;

/// Every document this build ships, read off the registry rather than listed
/// here — a list in this file goes stale the day a room is added, and reads
/// green while the new room is driven by nothing.
fn shipped() -> Vec<(&'static str, Playbook)> {
    expectations::shipped_rooms()
        .map(|name| {
            let read = Playbook::read(&expectations::room_document(name))
                .unwrap_or_else(|e| panic!("the shipped room {name} must read: {e:#}"));
            (name, read)
        })
        .collect()
}

/// **Every shipped room parses, and every phase says something.**
#[test]
fn every_shipped_room_reads_as_phases_with_something_to_say() {
    let rooms = shipped();
    assert!(
        rooms.len() >= 2,
        "one room is not a suite: {:?}",
        rooms.iter().map(|(name, _)| name).collect::<Vec<_>>(),
    );
    for (name, room) in &rooms {
        assert!(!room.phases.is_empty(), "{name} carries no phase");
        for phase in &room.phases {
            assert!(
                !phase.prompt.is_empty(),
                "{name}: {:?} has no block addressed to the model",
                phase.name,
            );
            // The document addresses its maintainer outside the block and the
            // occupant inside it. A reader that took the whole section would
            // sweep up the notes, and the occupant would be told it is in a
            // test.
            assert!(
                !phase.prompt.contains("**Session:"),
                "{name}: {:?} carries the maintainer's session marker into what the occupant is \
                 told: {:?}",
                phase.name,
                phase.prompt,
            );
        }
    }
}

/// **Every shipped room is registered, both halves.**
///
/// A document with no expectations is a run that refuses at the door, and a
/// registration with no document is a room nobody can enter. The negative rides
/// with them: a name nobody registered still gets nothing.
#[test]
fn every_shipped_room_has_expectations_and_furniture() {
    for (name, _) in shipped() {
        let checks = expectations::for_playbook(name)
            .unwrap_or_else(|| panic!("{name} is shipped and nothing asserts it"));
        assert!(!checks.is_empty(), "{name} came back with an empty list");
        expectations::seed_for(name)
            .unwrap_or_else(|e| panic!("{name}'s seed does not even BUILD: {e:#}"));
    }
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
    );
}

/// **Every shipped room's starting world actually APPLIES, not only builds.**
///
/// The check above only calls [`expectations::seed_for`], which is a pure
/// builder: `Seed::entity`/`::fact`/etc. touch no store, so a seed whose
/// writes collide with each other once they are actually sent somewhere
/// reads as fine there. The only place a seed's writes are sent anywhere is
/// [`Seed::furnish`], against a real room — the same thing a `make paid` run
/// does — which is what this proves for every room this build ships, rather
/// than for the ones somebody remembered to write a bespoke room file for.
///
/// **Sequential, not concurrent.** This suite has form: standing a real
/// store up per room, overlapped, has taken the machine down before (see
/// `transcripts/run-18-rooms-2026-09-09.md`). One room at a time, opened and
/// dropped before the next, costs a few seconds total here and carries none
/// of that risk.
///
/// Paired with the case below: that one proves a colliding world FAILS to
/// furnish; this proves every room actually shipped does not.
#[tokio::test]
async fn every_shipped_rooms_seed_furnishes_a_real_room() {
    let binary = server_binary().expect("a jojobot binary");
    let mut checked = 0;
    for name in expectations::shipped_rooms() {
        let seed = expectations::seed_for(name)
            .unwrap_or_else(|e| panic!("{name}'s seed does not even build: {e:#}"));
        let (_room, surface) = Room::open_with_client(&binary)
            .await
            .unwrap_or_else(|e| panic!("{name}: a room would not open: {e:#}"));
        seed.furnish(&surface)
            .await
            .unwrap_or_else(|e| panic!("{name}'s own starting world does not furnish: {e:#}"));
        checked += 1;
    }
    // A loop over an empty registry holds every claim above without proving
    // any of them — the same shape `judge_all`'s own comment names.
    assert!(
        checked > 0,
        "shipped_rooms() named nothing, so this proved zero rooms furnish",
    );
}

/// **The other half: a world that cannot furnish fails, by name.**
///
/// `thing:kettl` and `thing:kettle` are one edit apart — inside jojobot's own
/// resemblance guard's budget (`NEAR` = 2, `jojobot_domain::memory::guard`) —
/// so the second write is refused the moment it is actually sent. A check
/// that only builds a `Seed` never reaches this, because building draws no
/// comparison against anything: furnishing against a real room is the one
/// thing that does, which is the exact shape a room can ship unrunnable in
/// and still read green.
#[tokio::test]
async fn a_world_with_two_near_identical_things_cannot_furnish_a_room() {
    let seed = Seed::new()
        .entity("thing", "kettl", "Kettl")
        .expect("a plain entity builds")
        .entity("thing", "kettle", "Kettle")
        .expect("a plain entity builds");
    let (_room, surface) = Room::open_with_client(&server_binary().expect("a jojobot binary"))
        .await
        .expect("a room");
    let furnished = seed.furnish(&surface).await;
    assert!(
        furnished.is_err(),
        "thing:kettl and thing:kettle are one edit apart, inside the resemblance guard's budget \
         of 2 — furnishing both must be refused, not silently accepted: {furnished:?}",
    );
}

/// **The second year room ships, as decision log 299 rules: the year room a
/// paid run uses unless another is named.**
#[test]
fn the_second_year_room_is_shipped() {
    assert!(
        expectations::shipped_rooms().any(|name| name == "rooms/vault.md"),
        "rooms/vault.md is not registered in ROOMS",
    );
}

/// **A room's occupant arrives cold, in every phase.**
///
/// The shape the rooms are built on: inside one session a model answers from
/// its own context, so a phase that carries the one before it makes the store
/// optional. Every phase of every room starts fresh, and a room that stopped
/// doing that would quietly stop measuring what it says it measures.
#[test]
fn every_phase_of_every_room_starts_cold() {
    for (name, room) in shipped() {
        for phase in &room.phases {
            assert!(
                phase.fresh_session,
                "{name}: {:?} carries the session before it",
                phase.name,
            );
        }
    }
}
