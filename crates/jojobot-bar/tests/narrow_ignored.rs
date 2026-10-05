//! **`bar narrow`'s zero-selection guard counts only tests that will run.**
//! `cargo test -- --list` prints an `#[ignore]`d test beside the others, so a
//! crate whose every case is ignored passed the guard and then ran nothing.
//! A stub `cargo` answers both listings — everything, and `--ignored` alone —
//! so the real `bar` binary is driven end to end and no real cargo runs.

use std::fs;
use std::io::Write as _;
use std::process::Command;

/// **A fake `cargo` whose two listings come from the environment.** A plain
/// `--list` prints `STUB_ALL`; `--list --ignored` prints `STUB_IGNORED`, the
/// way libtest prints only the ignored subset. Every other phase succeeds.
fn write_listing_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let mut f = fs::File::create(&path).expect("write fake cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) printf '%s\n' "$STUB_IGNORED" ;;
      *) printf '%s\n' "$STUB_ALL" ;;
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

/// Runs `bar narrow` over the stub and returns its exit code and stdout.
fn narrow(tag: &str, all: &str, ignored: &str) -> (Option<i32>, String) {
    let dir = scratch_dir(tag);
    let fake_cargo = write_listing_cargo(&dir);
    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .args(["narrow", "--crate", "jojobot-bar"])
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .env("STUB_ALL", all)
        .env("STUB_IGNORED", ignored)
        .output()
        .expect("run bar narrow");
    let _ = fs::remove_dir_all(&dir);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn a_selection_of_only_ignored_tests_is_refused_and_says_they_were_ignored() {
    let both = "alpha::first: test\nalpha::second: test";
    let (code, stdout) = narrow("all-ignored", both, both);
    assert_eq!(
        code,
        Some(2),
        "nothing would run, so the guard refuses: {stdout}"
    );
    assert!(stdout.contains("verdict: RED"), "{stdout}");
    assert!(
        stdout.contains("ignored"),
        "the refusal must say why, not read as an empty selection: {stdout}"
    );
}

/// **The positive the refusal rests on.** One runnable test among ignored
/// ones passes the guard, so the refusal above is about the count of what
/// runs and not about an ignored test being present.
#[test]
fn one_runnable_test_among_ignored_ones_passes_the_guard() {
    let (code, stdout) = narrow(
        "one-runnable",
        "alpha::first: test\nalpha::second: test",
        "alpha::second: test",
    );
    assert_eq!(
        code,
        Some(0),
        "a runnable test must pass the guard: {stdout}"
    );
    assert!(stdout.contains("verdict: GREEN"), "{stdout}");
}

/// **The refusal the guard already had is unchanged.** A listing with no
/// tests at all is still refused, and does not claim anything was ignored.
#[test]
fn a_selection_of_no_tests_is_still_refused_without_claiming_any_were_ignored() {
    let (code, stdout) = narrow("none", "", "");
    assert_eq!(code, Some(2), "{stdout}");
    assert!(stdout.contains("verdict: RED"), "{stdout}");
    assert!(
        !stdout.contains("ignored"),
        "an empty selection is not an ignored one: {stdout}"
    );
}
