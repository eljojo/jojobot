//! **Where a room's server day is stated.**
//!
//! The paid path refuses a document with no stated day, and each room's test
//! file reads its day from the document. These two hold that without a server,
//! so the bar that does not run the rooms still sees them.

use jojobot_exercise::expectations;
use jojobot_exercise::playbook::Playbook;

/// **Every room this build ships states its server day**, so the paid path
/// accepts each one.
#[test]
fn every_shipped_room_states_its_server_day() {
    for name in expectations::shipped_rooms() {
        let read = Playbook::read(&expectations::room_document(name)).expect("the room reads");
        read.stated_server_day()
            .unwrap_or_else(|refusal| panic!("{refusal}"));
    }
}

/// **No room test file keeps a copy of its day.** A second copy is how the
/// document and the test drift: each file reads the day from the document the
/// paid path reads it from.
#[test]
fn no_room_test_file_declares_a_server_day_of_its_own() {
    let tests = expectations::room_document("tests");
    let mut copies = Vec::new();
    for entry in std::fs::read_dir(&tests).expect("the tests directory reads") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|ext| ext != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a test file reads");
        for (at, line) in source.lines().enumerate() {
            let code = line.trim_start();
            if code.starts_with("const SERVER_DAY") || code.starts_with("static SERVER_DAY: &") {
                copies.push(format!("{}:{}", path.display(), at + 1));
            }
        }
    }
    assert!(
        copies.is_empty(),
        "a day is declared in a test file: {copies:?}"
    );
}

/// The test files that may open a server on the wall clock, each with the
/// reason none of its assertions can meet a stamp. **Every other file that
/// opens a server states its day**, through the document it reads.
///
/// A file is here because it matches no needle against anything the server
/// stamps: its needles are words and handles, or its dates are past, or it
/// asserts on how a room opens and not on what it holds. A file whose locks
/// carry date needles belongs in no list, and the retired lock files are the
/// one frozen group.
const OPENS_ON_THE_WALL_CLOCK: &[(&str, &str)] = &[
    (
        "day_claimed.rs",
        "its boundaries are built by hand; the room is a prop it never asks",
    ),
    ("isolate.rs", "its needles are marker words, no dates"),
    ("seen_question.rs", "its needles are handles, no dates"),
    (
        "phase_end_lock.rs",
        "its needles are marker words, no dates",
    ),
    ("windowed_lock.rs", "its needles are marker words, no dates"),
    (
        "killed_run.rs",
        "asserts on processes and stores, matches no answer",
    ),
    (
        "room.rs",
        "tests how a room opens; its fixture dates are all past",
    ),
    (
        "room_log_filter.rs",
        "asserts on the server's log lines, matches no answer",
    ),
    (
        "room_template_vanished.rs",
        "asserts a room opens after its template is removed, matches no answer",
    ),
    (
        "replay_room.rs",
        "compares the class of answers between two rooms, matches no date",
    ),
    ("reads_the_rooms.rs", "furnishes a room and reads no answer"),
    ("desk_lock.rs", "retired lock file, frozen"),
    ("furnace_lock.rs", "retired lock file, frozen"),
    ("trip_lock.rs", "retired lock file, frozen"),
    ("wharf_lock.rs", "retired lock file, frozen"),
];

/// Whether a line opens a server without stating a day.
fn opens_on_the_wall_clock(line: &str) -> bool {
    [
        "Room::open(",
        "Room::open_with_client(",
        "Room::open_with_client_from(",
        "Room::snapshot_after(",
        "Room::open_without_the_death_signal(",
    ]
    .iter()
    .any(|call| line.contains(call))
}

/// **A test file that opens a server without a stated day says why it may.**
/// The audit of the lock files is this list: a file that is neither on it nor
/// states its day is named here, and a file on it that no longer opens a
/// server on the wall clock is named too, so the list cannot go stale.
#[test]
fn a_test_file_opens_a_server_on_the_wall_clock_only_when_it_is_listed_with_a_reason() {
    let tests = expectations::room_document("tests");
    let mut unlisted = Vec::new();
    let mut opening = std::collections::BTreeSet::new();
    for entry in std::fs::read_dir(&tests).expect("the tests directory reads") {
        let path = entry.expect("an entry").path();
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_string();
        if path.extension().is_none_or(|ext| ext != "rs") || name == "server_day_stated.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a test file reads");
        if source.lines().any(opens_on_the_wall_clock) {
            opening.insert(name.clone());
            if !OPENS_ON_THE_WALL_CLOCK
                .iter()
                .any(|(listed, _)| *listed == name)
            {
                unlisted.push(name);
            }
        }
    }
    assert!(
        unlisted.is_empty(),
        "these open a server on the wall clock and are not listed: {unlisted:?}",
    );
    let stale: Vec<&str> = OPENS_ON_THE_WALL_CLOCK
        .iter()
        .map(|(listed, _)| *listed)
        .filter(|listed| !opening.contains(*listed))
        .collect();
    assert!(
        stale.is_empty(),
        "these are listed and no longer open on the wall clock: {stale:?}"
    );
}
