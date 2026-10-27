//! **`bar narrow` names several cases in one call and ends with its wall
//! time.** A stub `cargo` records every call, so the assertions are over what
//! the real `bar` binary spawned.

use std::fs;
use std::process::Command;

mod support;

fn run_narrow(tag: &str, filter: &str) -> (Option<i32>, String, String, String) {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-test-{tag}"
    )));

    fs::create_dir_all(&dir).expect("create scratch dir");
    support::loop_repository(&dir);
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
subcommand=$1
while [ "$#" -gt 0 ] && [ "$1" != -- ]; do shift; done
if [ "$#" -gt 0 ]; then shift; fi
filters=
for arg in "$@"; do
  case "$arg" in --list|--ignored) ;; *) filters="$filters $arg" ;; esac
done
selection() {
  for name in alpha::first beta::second; do
    if [ -z "$filters" ]; then
      printf '%s\n' "$name"
    else
      for filter in $filters; do
        case "$name" in *"$filter"*) printf '%s\n' "$name"; break ;; esac
      done
    fi
  done
}
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) for name in $(selection); do printf '%s: test\n' "$name"; done ;;
    esac
    exit 0 ;;
esac
case "$subcommand" in
  test) grep '^level:\|^selected:' "$BAR_CASE_STDOUT" > "$(dirname "$0")/before-tests" || true
    set -- $(selection)
    printf 'test result: ok. %s passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s\n' "$#"; exit 0 ;;
  *) exit 0 ;;
esac
"#,
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(["narrow", "--crate", "jojobot-bar", "--filter", filter])
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &cargo)
        .env("BAR_CASE_STDOUT", dir.join("stdout"))
        .stdout(fs::File::create(dir.join("stdout")).unwrap());
    let status = support::fenced_spawn(&mut command)
        .expect("run bar narrow")
        .wait()
        .unwrap();
    let stdout = fs::read_to_string(dir.join("stdout")).unwrap();
    let before = fs::read_to_string(dir.join("before-tests")).unwrap_or_default();
    let argv = fs::read_to_string(dir.join("argv.log")).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    (status.code(), stdout, argv, before)
}

/// **A FILTER of several words reaches cargo as several filters**, each a
/// substring of a test path, so the cases a hand-off names run in one call.
#[test]
fn a_filter_of_several_words_reaches_cargo_as_several_filters() {
    let (code, stdout, argv, _) = run_narrow("several", "alpha::first beta::second");
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
    let (code, stdout, _, _) = run_narrow("wall", "alpha");
    assert_eq!(code, Some(0), "{stdout}");
    let last = stdout.lines().last().expect("output");
    assert!(last.starts_with("wall time: "), "{last}");
}

#[test]
fn targeted_dispatch_names_the_boundary_selection_before_running_it() {
    let (code, stdout, argv, before) = run_narrow("targeted", "alpha");
    assert_eq!(code, Some(0), "{stdout}");
    assert!(
        before.contains("level: boundary, narrow jojobot-bar"),
        "{stdout}"
    );
    assert!(before.contains("selected: alpha::first"), "{stdout}");
    assert!(!before.contains("beta::second"), "{stdout}");
    assert!(
        stdout.find("level:").unwrap() < stdout.find("verdict:").unwrap(),
        "{stdout}"
    );
    let listing = argv
        .lines()
        .find(|s| s.contains("--list") && !s.contains("--ignored"))
        .unwrap();
    assert_eq!(
        listing,
        "test -p jojobot-bar --all-features -- --list alpha"
    );
    assert!(
        argv.lines()
            .any(|s| { s == "test -p jojobot-bar --all-features -- --list --ignored alpha" })
    );
    let test = argv
        .lines()
        .find(|s| s.starts_with("test ") && !s.contains("--list"))
        .unwrap();
    assert!(
        test.contains("--all-features") && test.ends_with(" alpha"),
        "{test}"
    );
}
