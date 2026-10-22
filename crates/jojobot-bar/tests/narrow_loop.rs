//! **`bar narrow` names several cases in one call and ends with its wall
//! time.** A stub `cargo` records every call, so the assertions are over what
//! the real `bar` binary spawned.

use std::fs;
use std::process::Command;

mod support;

fn run_narrow(tag: &str, filter: &str) -> (Option<i32>, String, String) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) printf 'alpha::first: test\nbeta::second: test\n' ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#,
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(["narrow", "--crate", "jojobot-bar", "--filter", filter])
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &cargo);
    let output = support::output_fenced(&mut command).expect("run bar narrow");
    let argv = fs::read_to_string(dir.join("argv.log")).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        argv,
    )
}

/// **A FILTER of several words reaches cargo as several filters**, each a
/// substring of a test path, so the cases a hand-off names run in one call.
#[test]
fn a_filter_of_several_words_reaches_cargo_as_several_filters() {
    let (code, stdout, argv) = run_narrow("several", "alpha::first beta::second");
    assert_eq!(code, Some(0), "{stdout}");
    let run = argv
        .lines()
        .find(|call| call.starts_with("test ") && !call.contains("--list"))
        .unwrap_or_else(|| panic!("no test call: {argv}"));
    assert!(
        run.ends_with("alpha::first beta::second"),
        "both filters ride the one call: {run}"
    );
}

/// **A narrow run ends with its whole wall time**, on the last line.
#[test]
fn a_narrow_run_ends_with_its_wall_time() {
    let (code, stdout, _) = run_narrow("wall", "alpha");
    assert_eq!(code, Some(0), "{stdout}");
    let last = stdout.lines().last().expect("output");
    assert!(last.starts_with("wall time: "), "{last}");
}
