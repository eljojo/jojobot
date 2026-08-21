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

pub use crate::handover_room::HANDOVER_ROOM;
pub use crate::loop_room::LOOP_ROOM;

/// **The bike room**, converted: its world and its locks are in its document,
/// and the two claims no query expresses are named checks.
pub const BIKE_ROOM: &str = "rooms/bike.md";

/// **The ledger room**, which has no Rust half: its world and its locks are
/// written in its own document, so the name is all there is to register.
pub const LEDGER_ROOM: &str = "rooms/ledger.md";

/// **The year** — twelve cold sittings, one a month. Written in its document
/// from the beginning, which is what the format was built for.
pub const YEAR_ROOM: &str = "rooms/year.md";

/// **The Rust half of a room that still has one** — what must be true of it
/// afterwards, and what it is furnished with before anybody arrives.
type InRust = (
    fn() -> Vec<Box<dyn Expectation>>,
    fn() -> anyhow::Result<Seed>,
);

/// One room: the document it is driven by, and its Rust half when it has one.
type Room = (&'static str, Option<InRust>);

/// **Every room this build ships.** A room is added here in one line, and a
/// document that is not on this list is a document no run will drive.
///
/// **`None` is a converted room**: its world and its locks are in its own
/// document, which is where both are read from for every room — the Rust below
/// is only what a room that has not been converted still falls back to.
const ROOMS: [Room; 5] = [
    (BIKE_ROOM, None),
    (
        LOOP_ROOM,
        Some((crate::loop_room::expectations, crate::loop_room::seed)),
    ),
    (LEDGER_ROOM, None),
    (YEAR_ROOM, None),
    (
        HANDOVER_ROOM,
        Some((
            crate::handover_room::expectations,
            crate::handover_room::seed,
        )),
    ),
];

/// **Where a shipped room's document is on disk.**
///
/// Resolved from THIS CRATE rather than from wherever a caller happened to
/// stand: the documents sit beside the code that reads them, and a path built
/// from the working directory works only for whoever runs it from the right
/// one. The binary takes a path from its caller, and everything else asks here.
pub fn room_document(name: &str) -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(name)
}

/// The documents this build ships, by name — what a caller may point a run at.
pub fn shipped_rooms() -> impl Iterator<Item = &'static str> {
    ROOMS.iter().map(|(document, _)| *document)
}

/// The expectations for a playbook, or nothing when none are written.
///
/// **A room's own document is asked first.** The locks written beside the
/// phases they belong to are the room's checks; the Rust list below is what a
/// room used to be, and a room that has been converted is not in it.
pub fn for_playbook(source: &str) -> Option<Vec<Box<dyn Expectation>>> {
    if let Some(written) = locks_in(source) {
        return Some(written);
    }
    ROOMS
        .iter()
        .find(|(document, _)| source.ends_with(document))
        .and_then(|(_, rust)| rust.map(|(checks, _)| checks()))
}

/// The locks a room's document carries, or nothing when it carries none.
///
/// **A document that cannot be read is not a room with no locks.** A parse
/// failure here would otherwise read as "nobody wrote any", and the run would
/// refuse to start saying the wrong thing about why.
fn locks_in(source: &str) -> Option<Vec<Box<dyn Expectation>>> {
    let document = std::fs::read_to_string(source)
        .or_else(|_| std::fs::read_to_string(room_document(source)))
        .ok()?;
    match crate::lock::read(&document) {
        Ok(locks) if locks.is_empty() => None,
        Ok(locks) => Some(
            locks
                .into_iter()
                .map(|mut lock| {
                    // **The hatch, given its Rust.** A lock asking a query
                    // needs nothing; a lock naming a check the build does not
                    // ship stays unresolved and fails saying which name.
                    lock.resolve(&named_check);
                    Box::new(lock) as Box<dyn Expectation>
                })
                .collect(),
        ),
        // **Refused loudly rather than silently ignored.** A malformed lock is
        // an author's mistake and a run that started anyway would report a pass
        // over the checks that happened to parse.
        Err(e) => panic!("the locks in {source} cannot be read: {e:#}"),
    }
}

/// **Every Rust check this build ships, by the name a document calls it.**
///
/// A hatch is a claim jojobot's query surface cannot say, so each entry here
/// names a specific limit of that surface rather than a convenience. **The run
/// counts and prints the ones a room took**, and a room that cannot reach zero
/// is telling you something.
fn named_check(name: &str) -> Option<Box<dyn crate::run::Checks>> {
    crate::checks::CHECKS
        .iter()
        .find(|(known, _)| *known == name)
        .map(|(_, make)| make())
}

/// **What a room is furnished with before its occupant arrives**, and an empty
/// seed for a playbook nobody wrote any for.
///
/// It is keyed the same way as the checks and beside them, because the
/// furniture is the other half of what a check reads: an assertion about a
/// message that was never posted fails on the harness rather than on the
/// product, and the two halves in two files drift apart.
pub fn seed_for(source: &str) -> anyhow::Result<Seed> {
    // **The document furnishes its own room when it says how.** The Rust
    // builders below are what a room used to be; a converted room's world is
    // beside its story.
    let document = std::fs::read_to_string(source)
        .or_else(|_| std::fs::read_to_string(room_document(source)))
        .unwrap_or_default();
    let written = crate::world::read(&document)?;
    if !written.is_empty() {
        return Ok(written);
    }
    match ROOMS
        .iter()
        .find(|(document, _)| source.ends_with(document))
    {
        Some((_, Some((_, furniture)))) => furniture(),
        Some((_, None)) | None => Ok(Seed::new()),
    }
}
