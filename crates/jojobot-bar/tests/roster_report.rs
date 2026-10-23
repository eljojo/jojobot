//! **The bar prints the cast names that arrived since `origin/main`, and the
//! handles that name nobody pass silently.** Driven through the real binary
//! against a real git tree, like the size report: a name added to the cast list
//! is in the verdict's output and a name added to the ordinary list is not, and
//! the exit code is the same either way.

use std::fs;
use std::process::Command;

mod support;

const ROSTER_FILE: &str = "crates/jojobot-domain/tests/fixture_roster.rs";

fn write_fake_cargo_always_green(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*) echo 'some::test::name: test'; exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-roster-test-{tag}"
    )));

    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

fn git(dir: &std::path::Path, args: &[&str]) {
    support::status_fenced(Command::new("git").args(args).current_dir(dir)).expect("run git");
}

/// A roster source holding the given cast and ordinary entries.
fn roster_source(cast: &[&str], ordinary: &[&str]) -> String {
    let list = |entries: &[&str]| {
        entries
            .iter()
            .map(|e| format!("    \"{e}\",\n"))
            .collect::<String>()
    };
    format!(
        "const ROSTER: &[&str] = &[\n{}];\n\nconst ORDINARY: &[&str] = &[\n{}];\n",
        list(cast),
        list(ordinary)
    )
}

/// A repository whose `origin/main` holds one cast name and one ordinary name,
/// with the working tree then changed to `cast` and `ordinary`.
fn repo_with(tag: &str, cast: &[&str], ordinary: &[&str]) -> std::path::PathBuf {
    let dir = scratch_dir(tag);
    git(&dir, &["init", "-q"]);
    git(&dir, &["config", "user.email", "scratch@example.com"]);
    git(&dir, &["config", "user.name", "scratch"]);
    let path = dir.join(ROSTER_FILE);
    fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
    fs::write(
        &path,
        roster_source(&["person:stan"], &["thing:gravel-bike"]),
    )
    .expect("base");
    git(&dir, &["add", ROSTER_FILE]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    git(&dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    fs::write(&path, roster_source(cast, ordinary)).expect("changed roster");
    git(&dir, &["add", ROSTER_FILE]);
    git(&dir, &["commit", "-q", "-m", "change"]);
    dir
}

fn run_check(dir: &std::path::Path) -> (bool, String) {
    let fake_cargo = write_fake_cargo_always_green(dir);
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(dir)
            .env("TMPDIR", dir)
            .env("CARGO", &fake_cargo)
            .env("BAR_HISTORY_DIR", dir.join("history")),
    )
    .expect("run bar check");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

#[test]
fn a_new_cast_name_is_printed_and_a_new_ordinary_name_is_not() {
    let dir = repo_with(
        "both",
        &["person:stan", "person:kyle"],
        &["thing:gravel-bike", "thing:folding-chairs"],
    );
    let (green, stdout) = run_check(&dir);
    assert!(
        green,
        "the roster report must not move the exit code: {stdout}"
    );
    assert!(stdout.contains("verdict: GREEN"), "{stdout}");
    assert!(
        stdout.contains("roster names new since") && stdout.contains(": 1"),
        "{stdout}"
    );
    assert!(
        stdout.contains("person:kyle"),
        "the new cast name is named: {stdout}"
    );
    assert!(
        !stdout.contains("folding-chairs"),
        "a new ordinary name passes silently: {stdout}"
    );
    assert!(
        !stdout.contains("person:stan"),
        "a name that was already there is not new: {stdout}"
    );
    let verdict = stdout.find("verdict:").expect(&stdout);
    let roster = stdout.find("person:kyle").expect(&stdout);
    assert!(verdict < roster, "the verdict comes first: {stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_change_that_adds_no_cast_name_prints_none() {
    let dir = repo_with(
        "ordinary-only",
        &["person:stan"],
        &["thing:gravel-bike", "thing:folding-chairs"],
    );
    let (green, stdout) = run_check(&dir);
    assert!(green, "{stdout}");
    assert!(stdout.contains("roster names new since"), "{stdout}");
    assert!(stdout.contains(": 0"), "{stdout}");
    assert!(!stdout.contains("folding-chairs"), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}
