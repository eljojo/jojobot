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

/// **The handover room**, converted: its world and its locks are in its
/// document, and every lock is a named check.
pub const HANDOVER_ROOM: &str = "rooms/handover.md";

/// **The loop room**, converted: its world and its locks are in its document.
pub const LOOP_ROOM: &str = "rooms/loop.md";

/// **The bike room**, converted: its world and its locks are in its document,
/// and the two claims no query expresses are named checks.
pub const BIKE_ROOM: &str = "rooms/bike.md";

/// **The year** — fifteen cold sittings inside one year. Written in its
/// document from the beginning, which is what the format was built for.
pub const YEAR_ROOM: &str = "rooms/year.md";

/// **The second year** — fourteen cold sittings, each filing a fact where a
/// person would file it, so a late question has to reach across subjects
/// rather than read one. Decision log 299: this is the year room a paid run
/// uses unless another is named. `year.md` stays shipped for its own
/// coverage (retraction vs rewrite, rename rendering, the fold, the payment
/// status vocabulary, lineage walk-back, derivation-on-check-in).
pub const VAULT_ROOM: &str = "rooms/vault.md";

/// A real design-rule register filed and queried across cold sittings.
pub const DECISIONS_ROOM: &str = "rooms/decisions.md";

/// **The adoption experiment's baseline (E0)** — one sitting, two facts a
/// shipped type exists for, worded the way the operator talks.
pub const ADOPTION_ROOM: &str = "rooms/adoption.md";

/// **The repair room** — one mistake the model makes and three the room
/// furnishes, each with a repair already on the surface and no move named.
pub const REPAIR_ROOM: &str = "rooms/repair.md";

/// **The trick room** — a brief of ordinary requests, each with a convenient
/// place to put the fact that is not where the question about it is asked.
/// Not the default.
pub const TRICK_ROOM: &str = "rooms/trick.md";

/// **The agency room** — a probe built to fail on the product: one employee,
/// two projects, four cold sittings across eleven months. Not the default.
pub const AGENCY_ROOM: &str = "rooms/agency.md";

/// **The asking room** — nine sittings of a year, then one cold sitting asked a
/// dozen ordinary questions about all of it. Each question is one a cold
/// session cannot answer by remembering: a computation, a word the operator
/// said once, or state the store owns. Not the default.
pub const ASKING_ROOM: &str = "rooms/asking.md";

/// **The querying room** — one cold sitting asked the eight questions a board
/// is asked, in plain words, over a project of twelve pieces of work. Each
/// answerable question is one recall expression on this build; the run prints
/// how many recall calls the room refused and on which question. Not the
/// default.
pub const QUERYING_ROOM: &str = "rooms/querying.md";

/// **The adoption experiment's wording arm (E1)** — the same room with each
/// fact said in its key's own word.
pub const ADOPTION_WORDING_ROOM: &str = "rooms/adoption-wording.md";

/// One room: the document it is driven by, and whether a run that names none
/// gets this one.
type Room = (&'static str, bool);

/// **Every room this build ships.** A room is added here in one line, and a
/// document that is not on this list is a document no run will drive.
///
/// A room's world and its locks are in its own document, which is where both are
/// read from.
///
/// **Exactly one entry carries `true`** — decision log 299 — and this table is
/// the only place that name lives; nothing outside it repeats it.
const ROOMS: [Room; 13] = [
    (BIKE_ROOM, false),
    (LOOP_ROOM, false),
    (YEAR_ROOM, false),
    (HANDOVER_ROOM, false),
    (VAULT_ROOM, true),
    (DECISIONS_ROOM, false),
    (ADOPTION_ROOM, false),
    (ADOPTION_WORDING_ROOM, false),
    (REPAIR_ROOM, false),
    (TRICK_ROOM, false),
    (AGENCY_ROOM, false),
    (ASKING_ROOM, false),
    (QUERYING_ROOM, false),
];

/// **The room a run uses when none is named** — decision log 299.
pub fn default_room() -> &'static str {
    ROOMS
        .iter()
        .find(|(_, is_default)| *is_default)
        .map(|(document, _)| *document)
        .expect("exactly one room in ROOMS must be marked default")
}

/// **Where a shipped room's document is on disk.**
///
/// The bar supplies the running checkout's package directory, so a bundled
/// binary can read rooms on a different runner. Without that runtime directory,
/// use the compiled package directory; neither path depends on the caller's cwd.
pub fn room_document(name: &str) -> std::path::PathBuf {
    std::env::var_os("CARGO_MANIFEST_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")))
        .join(name)
}

/// **A shipped room's document, read** — the one place a test asks for the
/// playbook of a room it holds open, so the day it states reaches the server.
pub fn shipped_playbook(name: &str) -> anyhow::Result<crate::playbook::Playbook> {
    crate::playbook::Playbook::read(&room_document(name))
}

/// The documents this build ships, by name — what a caller may point a run at.
pub fn shipped_rooms() -> impl Iterator<Item = &'static str> {
    ROOMS.iter().map(|(document, _)| *document)
}

/// The expectations for a playbook, or nothing when none are written.
///
/// The locks written beside the phases they belong to are the room's checks.
pub fn for_playbook(source: &str) -> Option<Vec<Box<dyn Expectation>>> {
    locks_in(source)
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
    // **The document furnishes its own room**: its world is beside its story.
    let document = std::fs::read_to_string(source)
        .or_else(|_| std::fs::read_to_string(room_document(source)))
        .unwrap_or_default();
    crate::world::read(&document)
}

#[cfg(test)]
mod tests {
    #[test]
    fn room_paths_without_a_runtime_package_keep_the_compiled_directory() {
        const CHILD: &str = "JOJOBOT_COMPILED_ROOM_PATH_CASE";
        if std::env::var_os(CHILD).is_none() {
            let out = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "expectations::tests::room_paths_without_a_runtime_package_keep_the_compiled_directory",
                    "--nocapture",
                ])
                .env(CHILD, "1")
                .env_remove("CARGO_MANIFEST_DIR")
                .output()
                .unwrap();
            assert!(out.status.success(), "{out:?}");
            return;
        }
        // A cached library and its integration suite need not share a build path.
        assert_eq!(
            super::room_document(super::VAULT_ROOM),
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(super::VAULT_ROOM)
        );
    }
}
