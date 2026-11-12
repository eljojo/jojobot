//! **A cargo the bar starts sees neither `MAKEFLAGS` nor `MFLAGS`.** `make narrow
//! CRATE=x FILTER=y` hands its recipe `MAKEFLAGS` with the command-line variables
//! after a double dash, and cargo gives a build script the same text as
//! `CARGO_MAKEFLAGS`. A crate that builds C through `make`, as `tikv-jemalloc-sys`
//! does, then runs `make` with ` -- FILTER=y CRATE=x -j ...` and make reads the
//! `-j` as a target. The bar is the one place every phase's cargo starts, so it
//! takes the two variables out of its own environment before it starts one.
//! A stub `cargo` records what it was given, so no real cargo runs.

use std::fs;
use std::process::Command;

mod support;

/// A fake `cargo` that appends the two variables, as it sees them, to a file.
fn write_recording_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
printf 'MAKEFLAGS=%s MFLAGS=%s\n' "${MAKEFLAGS-unset}" "${MFLAGS-unset}" >> "$STUB_SEEN"
case " $* " in
  *" --list "*) printf '%s\n' "$STUB_ALL"; exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_loop_cargo(&path, script);
    path
}

#[test]
fn a_cargo_child_of_make_narrow_sees_no_make_flags() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-test-makeflags"));
    fs::create_dir_all(&dir).expect("create scratch dir");
    support::loop_repository(&dir);
    let fake_cargo = write_recording_cargo(&dir);
    let seen = dir.join("seen");
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(["narrow", "--crate", "jojobot-bar"])
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &fake_cargo)
        .env("STUB_ALL", "alpha::first: test")
        .env("STUB_SEEN", &seen)
        // A run that names no history directory writes into the real one.
        .env("BAR_HISTORY_DIR", dir.join("history-none"))
        // What `make narrow CRATE=jojobot FILTER=ci` gives its recipe, and what
        // make passes to a child it runs as a sub-make.
        .env(
            "MAKEFLAGS",
            " -- FILTER=ci CRATE=jojobot -j --jobserver-auth=3,4",
        )
        .env("MFLAGS", "-j");
    let output = support::output_fenced(&mut command).expect("run bar narrow");
    let calls = fs::read_to_string(&seen).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    assert!(
        calls.lines().count() > 0,
        "the stub cargo was never run: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for call in calls.lines() {
        assert_eq!(
            call, "MAKEFLAGS=unset MFLAGS=unset",
            "a cargo child saw make's flags"
        );
    }
}
