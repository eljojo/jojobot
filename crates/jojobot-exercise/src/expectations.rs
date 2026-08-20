//! **The registry: which playbook is asserted by what, and furnished with
//! what.**
//!
//! A playbook nobody has written expectations for must not run at all — a run
//! that asserts nothing is a bill with no answer at the end of it — so the
//! refusal is a match on the document's own name, and this is where that match
//! lives.
//!
//! **The checks themselves live with their room**, one file each. They used to
//! live here, when there was one playbook and ten of them; a room is a goal, its
//! furniture and its locks, and those three drift apart the moment they are
//! kept in different places.

use crate::run::Expectation;
use crate::surface::Seed;

pub use crate::bike_room::BIKE_ROOM;
pub use crate::handover_room::HANDOVER_ROOM;
pub use crate::ledger_room::LEDGER_ROOM;
pub use crate::loop_room::LOOP_ROOM;

/// One room: the document it is driven by, what must be true of it afterwards,
/// and what it is furnished with before anybody arrives.
type Room = (
    &'static str,
    fn() -> Vec<Box<dyn Expectation>>,
    fn() -> anyhow::Result<Seed>,
);

/// **Every room this build ships.** A room is added here in one line, and a
/// document that is not on this list is a document no run will drive.
const ROOMS: [Room; 4] = [
    (
        BIKE_ROOM,
        crate::bike_room::expectations,
        crate::bike_room::seed,
    ),
    (
        LOOP_ROOM,
        crate::loop_room::expectations,
        crate::loop_room::seed,
    ),
    (
        LEDGER_ROOM,
        crate::ledger_room::expectations,
        crate::ledger_room::seed,
    ),
    (
        HANDOVER_ROOM,
        crate::handover_room::expectations,
        crate::handover_room::seed,
    ),
];

/// The documents this build ships, by name — what a caller may point a run at.
pub fn shipped_rooms() -> impl Iterator<Item = &'static str> {
    ROOMS.iter().map(|(document, _, _)| *document)
}

/// The expectations for a playbook, or nothing when none are written.
pub fn for_playbook(source: &str) -> Option<Vec<Box<dyn Expectation>>> {
    ROOMS
        .iter()
        .find(|(document, _, _)| source.ends_with(document))
        .map(|(_, checks, _)| checks())
}

/// **What a room is furnished with before its occupant arrives**, and an empty
/// seed for a playbook nobody wrote any for.
///
/// It is keyed the same way as the checks and beside them, because the
/// furniture is the other half of what a check reads: an assertion about a
/// message that was never posted fails on the harness rather than on the
/// product, and the two halves in two files drift apart.
pub fn seed_for(source: &str) -> anyhow::Result<Seed> {
    match ROOMS
        .iter()
        .find(|(document, _, _)| source.ends_with(document))
    {
        Some((_, _, furniture)) => furniture(),
        None => Ok(Seed::new()),
    }
}
