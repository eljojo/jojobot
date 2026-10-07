//! **The bar's build phase must not rewrite `Cargo.lock`.** An unlocked
//! `cargo build` fixes a drifted lockfile silently, before the locked test and
//! clippy phases can fail on the drift. A stub `cargo` records the arguments
//! of every call the real `bar` binary makes, so the assertion is over what
//! the bar actually spawned and no real cargo runs.

use std::fs;
use std::io::Write as _;
use std::process::Command;

/// **A fake `cargo` that records its argument list and succeeds.** It lists
/// one test so `narrow`'s zero-selection guard does not stop the run before
/// the build phase, and prints one clean `test result:` line so the phases
/// after the build go green.
fn write_recording_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let mut f = fs::File::create(&path).expect("write fake cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) echo 'some::test::name: test' ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;
    f.write_all(script.as_bytes())
        .expect("write fake cargo body");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).expect("stat fake cargo").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod fake cargo");
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Runs the bar with the recording cargo and returns the `build` calls it made.
fn build_calls(tag: &str, args: &[&str]) -> Vec<String> {
    calls_of(tag, args, "build")
}

/// Runs the bar with the recording cargo and returns the calls it made to one
/// cargo verb.
fn calls_of(tag: &str, args: &[&str], verb: &str) -> Vec<String> {
    let dir = scratch_dir(tag);
    let fake_cargo = write_recording_cargo(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .args(args)
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .output()
        .expect("run bar");
    let stdout = String::from_utf8_lossy(&output.stdout);

    let recorded = fs::read_to_string(dir.join("argv.log")).expect("the bar called cargo");
    let _ = fs::remove_dir_all(&dir);
    let builds: Vec<String> = recorded
        .lines()
        .filter(|line| line.split_whitespace().next() == Some(verb))
        .map(str::to_string)
        .collect();
    assert!(
        !builds.is_empty(),
        "the bar never reached its {verb} phase, so nothing here is measured: {recorded}\n{stdout}"
    );
    builds
}

#[test]
fn the_check_build_phase_is_locked() {
    for call in build_calls("locked-check", &["check"]) {
        assert!(
            call.split_whitespace().any(|a| a == "--locked"),
            "an unlocked build can rewrite Cargo.lock: {call}"
        );
    }
}

#[test]
fn the_narrow_build_phase_is_locked() {
    for call in build_calls("locked-narrow", &["narrow", "--crate", "jojobot-bar"]) {
        assert!(
            call.split_whitespace().any(|a| a == "--locked"),
            "an unlocked build can rewrite Cargo.lock: {call}"
        );
    }
}

/// **Lint reads the targets the test phase skips.** A room suite sits behind
/// a cargo feature, so `cargo test` neither builds nor runs it unless asked.
/// Clippy has to be asked for every feature, or a room suite is neither run
/// nor linted by the bar. Both the full bar and the narrow loop are held to it.
#[test]
fn the_lint_phase_reads_feature_gated_targets() {
    let check = calls_of("lint-check", &["check"], "clippy");
    let narrow = calls_of(
        "lint-narrow",
        &["narrow", "--crate", "jojobot-bar"],
        "clippy",
    );
    for call in check.iter().chain(&narrow) {
        assert!(
            call.split_whitespace().any(|a| a == "--all-features"),
            "a feature-gated suite would be neither run nor linted: {call}"
        );
    }
}

/// **A narrow run can reach a feature-gated suite**, and the zero-selection
/// guard counts it. Without the flag a filter naming a room suite lists
/// nothing and the run is refused as a mistyped filter.
#[test]
fn the_narrow_test_phase_can_reach_a_feature_gated_suite() {
    let tests = calls_of("test-narrow", &["narrow", "--crate", "jojobot-bar"], "test");
    assert!(!tests.is_empty());
    for call in &tests {
        assert!(
            call.split_whitespace().any(|a| a == "--all-features"),
            "a filter naming a room suite would select nothing: {call}"
        );
    }
}
