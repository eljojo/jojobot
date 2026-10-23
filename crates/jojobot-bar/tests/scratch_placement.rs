//! **A bar run's children get their temp directory in memory when the host has
//! room, and on disk when it has not.**
//!
//! Many Dolt servers syncing to one disk stall each other's commits past a
//! server's readiness deadline. The test stores hold nothing that must outlive
//! the run, so the run root goes under the memory-backed directory. The pure
//! choice is read here on its own, and the real `bar` binary is driven over a
//! stub `cargo` that records the temp directory it was started with.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_bar::scratch::{MEMORY_DIR, MINIMUM_FREE_BYTES, Placement, free_bytes, place};

mod support;

const MEMORY: &str = "/dev/shm";
const DISK: &str = "/var/tmp/disk-example";

#[test]
fn a_memory_directory_with_room_is_chosen() {
    let placed = place(
        Path::new(MEMORY),
        Some(MINIMUM_FREE_BYTES + 1),
        MINIMUM_FREE_BYTES,
        Path::new(DISK),
    );
    assert_eq!(placed, Placement::Memory(PathBuf::from(MEMORY)));
    assert_eq!(placed.base(), Path::new(MEMORY));
}

/// **Exactly the minimum is enough**, and one byte less is not: the boundary is
/// read from both sides.
#[test]
fn the_minimum_is_the_edge_of_the_choice() {
    let at = place(
        Path::new(MEMORY),
        Some(MINIMUM_FREE_BYTES),
        MINIMUM_FREE_BYTES,
        Path::new(DISK),
    );
    assert_eq!(at, Placement::Memory(PathBuf::from(MEMORY)));
    let below = place(
        Path::new(MEMORY),
        Some(MINIMUM_FREE_BYTES - 1),
        MINIMUM_FREE_BYTES,
        Path::new(DISK),
    );
    assert!(matches!(below, Placement::Disk { .. }), "{below:?}");
}

#[test]
fn a_memory_directory_too_small_falls_back_to_disk_and_says_why() {
    let placed = place(
        Path::new(MEMORY),
        Some(1),
        MINIMUM_FREE_BYTES,
        Path::new(DISK),
    );
    let Placement::Disk { base, reason } = placed else {
        panic!("a directory with 1 byte free must not be chosen");
    };
    assert_eq!(base, Path::new(DISK));
    assert!(reason.contains(MEMORY), "{reason}");
}

#[test]
fn a_missing_memory_directory_falls_back_to_disk_and_says_why() {
    let placed = place(Path::new(MEMORY), None, MINIMUM_FREE_BYTES, Path::new(DISK));
    let Placement::Disk { base, reason } = placed else {
        panic!("a directory that cannot be read must not be chosen");
    };
    assert_eq!(base, Path::new(DISK));
    assert!(reason.contains(MEMORY), "{reason}");
}

/// **The free space is read from the system**, for a directory that exists and
/// for one that does not.
#[test]
fn the_free_space_of_a_real_directory_is_read_and_a_missing_one_is_none() {
    let here = std::env::temp_dir();
    assert!(free_bytes(&here).is_some_and(|free| free > 0), "{here:?}");
    assert_eq!(
        free_bytes(Path::new("/nonexistent/jojobot-bar-example")),
        None
    );
}

/// A fake `cargo` that lists one test, lists one test file for a report, passes
/// every phase, and records the `TMPDIR` each `test` call that runs was started
/// with.
fn write_recording_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) echo 'alpha::first: test' ;;
    esac
    exit 0 ;;
  *" --no-run "*)
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    exit 0 ;;
esac
case "$1" in
  test)
    echo "$TMPDIR" >> "$(dirname "$0")/tmpdir.log"
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

fn scratch_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-test-{tag}"
    )));

    fs::create_dir_all(&dir).expect("create scratch dir");
    support::git_init(&dir);
    dir
}

/// Runs `bar <args>` over the stub, with the disk temp directory `dir`.
fn bar(dir: &Path, args: &[&str]) -> std::process::Output {
    let cargo = write_recording_cargo(dir);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(args)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", &cargo)
        .env("BAR_HISTORY_DIR", dir.join("history-none"))
        .env_remove("BAR_JOBS");
    support::output_fenced(&mut command).expect("run bar")
}

/// The ways to run the bar that start test processes, other than the boundary
/// and the rooms.
const TARGETS: [&[&str]; 2] = [&["narrow", "--crate", "jojobot-bar"], &["report"]];

/// **The temp directory a test process is started with is the one the choice
/// made**, for every target that runs tests. With room in memory it is a run
/// root under the memory-backed directory and the run says nothing; without, it
/// is a run root on disk and the run says in one line that it fell back and why.
#[test]
fn a_test_process_starts_with_its_temp_directory_where_the_choice_put_it() {
    for args in TARGETS {
        let dir = scratch_dir("placement");
        let output = bar(&dir, args);
        assert_eq!(output.status.code(), Some(0), "{args:?}: {output:?}");
        let recorded = fs::read_to_string(dir.join("tmpdir.log"))
            .unwrap_or_else(|_| panic!("{args:?}: the stub ran no test"));
        let given = recorded
            .lines()
            .next()
            .expect("a temp directory was recorded");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let memory = Path::new(MEMORY_DIR);
        match place(memory, free_bytes(memory), MINIMUM_FREE_BYTES, &dir) {
            Placement::Memory(base) => {
                assert!(
                    given.starts_with(&format!("{}/jojobot-bar-", base.display())),
                    "{args:?}: {given}"
                );
                assert!(!stderr.contains("go on disk"), "{args:?}: {stderr}");
            }
            Placement::Disk { base, reason } => {
                assert!(
                    given.starts_with(&format!("{}/jojobot-bar-", base.display())),
                    "{args:?}: {given}"
                );
                let said: Vec<&str> = stderr
                    .lines()
                    .filter(|l| l.contains("go on disk"))
                    .collect();
                assert_eq!(said.len(), 1, "{args:?}: one line, once: {stderr}");
                assert!(said[0].contains(&reason), "{args:?}: {stderr}");
            }
        }
        let _ = fs::remove_dir_all(&dir);
    }
}

/// **A run root a killed run left in memory is removed at the next run's
/// start**, because it holds memory until the host restarts. Its maker is a
/// process that is gone, and the directory is the kind the bar names.
#[test]
fn a_run_root_a_killed_run_left_in_memory_is_swept() {
    assert!(
        Path::new(MEMORY_DIR).is_dir(),
        "this host has no {MEMORY_DIR}"
    );
    let dead = {
        let mut child = Command::new("true").spawn().expect("a child to start");
        let pid = child.id();
        child.wait().expect("the child ends");
        pid
    };
    let stale = Path::new(MEMORY_DIR).join(format!("jojobot-bar-{dead}-1791471848664039428"));
    fs::create_dir_all(stale.join("db")).expect("a stale run root");
    fs::write(stale.join("db").join("file"), b"x").expect("a file in it");

    let dir = scratch_dir("memory-sweep");
    let output = bar(&dir, TARGETS[0]);
    assert_eq!(output.status.code(), Some(0), "{output:?}");

    let left = stale.exists();
    let _ = fs::remove_dir_all(&stale);
    let _ = fs::remove_dir_all(&dir);
    assert!(!left, "the stale run root in memory was kept");
}
