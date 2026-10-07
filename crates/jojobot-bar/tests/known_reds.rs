//! **A known red is annotated at every place the bar renders a verdict, and
//! nothing else about the run changes.** The library tests prove the rendering;
//! these drive the real `bar` binary, because the list has to be read and handed
//! to each of the three verdicts, and a library call proves none of that wiring.

use std::fs;
use std::io::Write as _;
use std::process::{Command, Output};

const LISTED: &str = "known::tests::a_listed_red";
const UNLISTED: &str = "fresh::tests::an_unlisted_red";

/// **A fake `cargo` whose test run fails two named tests** and exits non-zero,
/// the way a real run does. The lines are the shape `cargo test` prints.
fn write_fake_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let mut f = fs::File::create(&path).expect("write fake cargo");
    let script = format!(
        r#"#!/bin/sh
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) echo 'some::test::name: test' ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test)
    echo 'test {LISTED} ... FAILED'
    echo 'test {UNLISTED} ... FAILED'
    echo 'test result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
    exit 101 ;;
  *) exit 0 ;;
esac
"#
    );
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
        "jojobot-bar-known-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn listing(test: &str) -> String {
    format!(
        "[[known_red]]\ntest = \"{test}\"\ncard = 2036\nowner = \"dev3\"\nsince = \"2026-10-07\"\n"
    )
}

/// Run `bar` with `args` in a scratch directory holding `known_reds`.
fn bar(tag: &str, args: &[&str], known_reds: Option<&str>) -> (Output, String) {
    let dir = scratch_dir(tag);
    let fake_cargo = write_fake_cargo(&dir);
    if let Some(text) = known_reds {
        fs::write(dir.join("known-reds.toml"), text).expect("write the list");
    }
    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .args(args)
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .output()
        .expect("run bar");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let _ = fs::remove_dir_all(&dir);
    (output, stdout)
}

/// **The same assertions at each of the three verdicts**: the listed red is
/// annotated, the unlisted one is plain, the verdict is red and the exit is
/// not zero.
fn assert_annotated_and_red(output: &Output, stdout: &str) {
    assert!(
        stdout.contains(&format!(
            "  FAILED: {LISTED} (known red: card 2036, dev3, since 2026-10-07)"
        )),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|l| l == format!("  FAILED: {UNLISTED}")),
        "an unlisted red prints plain: {stdout}"
    );
    assert!(stdout.contains("verdict: RED"), "{stdout}");
    assert!(
        !output.status.success(),
        "a known red must not change the exit code: {stdout}"
    );
}

#[test]
fn bar_check_annotates_a_known_red_and_stays_red() {
    let (output, stdout) = bar("check", &["check"], Some(&listing(LISTED)));
    assert_annotated_and_red(&output, &stdout);
}

#[test]
fn bar_narrow_annotates_a_known_red_and_stays_red() {
    let (output, stdout) = bar(
        "narrow",
        &["narrow", "--crate", "jojobot-bar"],
        Some(&listing(LISTED)),
    );
    assert_annotated_and_red(&output, &stdout);
}

#[test]
fn bar_rooms_annotates_a_known_red_and_stays_red() {
    let (output, stdout) = bar(
        "rooms",
        &["rooms", "--test", "any_room"],
        Some(&listing(LISTED)),
    );
    assert_annotated_and_red(&output, &stdout);
}

/// **With no list the same run prints no annotation**: the paired negative,
/// so the cases above cannot pass over a bar that annotates everything.
#[test]
fn without_a_list_nothing_is_annotated() {
    let (output, stdout) = bar("none", &["check"], None);
    assert!(
        stdout.contains(&format!("  FAILED: {LISTED}\n")),
        "{stdout}"
    );
    assert!(!stdout.contains("known red"), "{stdout}");
    assert!(!output.status.success(), "{stdout}");
}

/// **A malformed list is refused with its line, before any phase runs.**
#[test]
fn a_malformed_list_is_refused_with_its_line_and_nothing_runs() {
    let (output, stdout) = bar(
        "malformed",
        &["check"],
        Some("[[known_red]]\ntest = \"a::b\"\nbogus = 1\n"),
    );
    assert_eq!(output.status.code(), Some(2), "{stdout}");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("known-reds.toml") && stderr.contains("line 3"),
        "{stderr}"
    );
    assert!(
        !stdout.contains("verdict:"),
        "no phase may run behind a refused list: {stdout}"
    );
}
