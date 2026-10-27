//! CI supplies the deployed store; local narrowing keeps the fixture-free cases.

use std::fs;
use std::process::Command;

mod support;

const NEEDS_FIXTURE: &str = "the_current_binary_boots_on_the_last_deployed_store";
const LOCAL: &str = "the_statement_splitter_keeps_a_semicolon_inside_a_value";

fn narrow(krate: &str, names: &[&str], fixture: bool) -> (Option<i32>, String, String) {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-upgrade"));
    fs::create_dir_all(&dir).unwrap();
    support::loop_repository(&dir);
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) printf '%s\n' "$STUB_LIST" ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s' ;;
esac
"#,
    );
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(["narrow", "--crate", krate])
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &cargo)
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env(
            "STUB_LIST",
            names
                .iter()
                .map(|name| format!("{name}: test\n"))
                .collect::<String>(),
        );
    if fixture {
        command.env("UPGRADE_FIXTURE_DIR", "ci-supplied");
    } else {
        command.env_remove("UPGRADE_FIXTURE_DIR");
    }
    let output = support::output_fenced(&mut command).unwrap();
    let argv = fs::read_to_string(dir.join("argv.log")).unwrap();
    let _ = fs::remove_dir_all(&dir);
    (
        output.status.code(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        argv,
    )
}

#[test]
fn a_local_root_narrow_names_the_ci_case_and_still_runs_its_local_neighbor() {
    let (code, stdout, argv) = narrow("jojobot", &[NEEDS_FIXTURE, LOCAL], false);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("1 runnable cases"), "{stdout}");
    assert!(stdout.contains(&format!("selected: {LOCAL}")), "{stdout}");
    assert!(
        !stdout.contains(&format!("selected: {NEEDS_FIXTURE}")),
        "{stdout}"
    );
    assert!(
        stdout.contains(&format!("skipped by design: {NEEDS_FIXTURE}")),
        "{stdout}"
    );
    assert!(stdout.contains("UPGRADE_FIXTURE_DIR"), "{stdout}");
    let run = argv
        .lines()
        .find(|line| line.starts_with("test ") && !line.contains("--list"))
        .unwrap();
    assert!(
        run.contains(&format!(" -- --skip {NEEDS_FIXTURE}")),
        "{argv}"
    );
    assert!(!run.contains(&format!("--skip {LOCAL}")), "{argv}");
}

#[test]
fn selecting_only_ci_upgrade_cases_is_a_refusal_and_never_an_empty_green() {
    let (code, stdout, argv) = narrow("jojobot", &[NEEDS_FIXTURE], false);
    assert_eq!(code, Some(2), "{stdout}");
    assert!(stdout.contains("UPGRADE_FIXTURE_DIR"), "{stdout}");
    assert!(
        !argv
            .lines()
            .any(|line| line.starts_with("test ") && !line.contains("--list")),
        "{argv}"
    );
}

#[test]
fn a_ci_supplied_fixture_keeps_every_upgrade_case_selected() {
    let (code, stdout, argv) = narrow("jojobot", &[NEEDS_FIXTURE, LOCAL], true);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("2 runnable cases"), "{stdout}");
    assert!(
        stdout.contains(&format!("selected: {NEEDS_FIXTURE}")),
        "{stdout}"
    );
    assert!(!argv.contains("--skip"), "{argv}");
}

#[test]
fn the_same_case_name_in_another_crate_is_not_ci_only() {
    let (code, stdout, argv) = narrow("jojobot-bar", &[NEEDS_FIXTURE, LOCAL], false);
    assert_eq!(code, Some(0), "{stdout}");
    assert!(stdout.contains("2 runnable cases"), "{stdout}");
    assert!(!argv.contains("--skip"), "{argv}");
}

#[test]
fn a_skip_that_would_also_drop_another_case_is_refused() {
    for names in [
        vec![NEEDS_FIXTURE, NEEDS_FIXTURE],
        vec![NEEDS_FIXTURE, &format!("{NEEDS_FIXTURE}_neighbor")],
    ] {
        let (code, stdout, argv) = narrow("jojobot", &names, false);
        assert_eq!(code, Some(1), "{stdout}");
        assert!(stdout.contains("ambiguous"), "{stdout}");
        assert!(
            !argv
                .lines()
                .any(|line| line.starts_with("test ") && !line.contains("--list")),
            "{argv}"
        );
    }
}
