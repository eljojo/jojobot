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

/// Every document this build ships, read off the registry rather than listed
/// here — a list in this file goes stale the day a room is added, and reads
/// green while the new room is driven by nothing.
fn shipped() -> Vec<(&'static str, Playbook)> {
    expectations::shipped_rooms()
        .map(|name| {
            let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(name);
            let read = Playbook::read(&path)
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
            .unwrap_or_else(|e| panic!("{name} cannot be furnished: {e:#}"));
    }
    assert!(
        expectations::for_playbook("docs/SOMETHING-ELSE.md").is_none(),
        "a playbook nobody has written expectations for was given some",
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
