//! **The bar's own exit status must survive the whole path**, from cargo's
//! exit code to the process that ran `bar`. Testing `test_healthy` alone
//! proves the library function; it says nothing about whether `run_check`
//! actually calls it with the real exit status rather than a discarded one.
//! A stub `cargo` drives the real `bar` binary end to end to prove that.

use std::fs;
use std::process::Command;

mod support;

#[test]
fn the_ci_verdict_names_ten_timed_failures_and_counts_the_rest() {
    let dir = scratch_dir("ci-names");
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
case " $* " in *" --no-run "*) exit 0 ;; esac
if [ "$1" = test ]; then cat "$BAR_CASE_FAILURES"; exit 101; fi
"#,
    );
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("CARGO", cargo)
            .env("TMPDIR", &dir)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env(
                "BAR_CASE_FAILURES",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/timed-failures.txt"),
            ),
    )
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("test: FAILED — 2 suites, 2601 passed, 13 failed"),
        "{stdout}"
    );
    assert_eq!(
        stdout
            .lines()
            .filter(|line| line.starts_with("  FAILED: "))
            .count(),
        10,
        "{stdout}"
    );
    assert!(
        stdout.contains(
            "  FAILED: working_on_page::a_bot_whose_writer_wrapped_moves_to_the_past_list"
        ),
        "{stdout}"
    );
    assert!(!stdout.contains("  FAILED: working_on_page::a_live_bot_is_on_the_page_of_its_work_and_of_the_project_above_it"), "{stdout}");
    assert!(stdout.contains("...and 3 more"), "{stdout}");
    let _ = fs::remove_dir_all(dir);
}

/// **A red run leaves its machine verdict where the workflow uploads it.** The
/// file is read back after the real binary ran: it names the job, says red and
/// lists every failing test, not the ten the summary shows.
#[test]
fn a_red_check_leaves_a_verdict_file_listing_every_failing_test() {
    let dir = scratch_dir("verdict-file");
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
case " $* " in *" --no-run "*) exit 0 ;; esac
if [ "$1" = test ]; then cat "$BAR_CASE_FAILURES"; exit 101; fi
"#,
    );
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("CARGO", cargo)
            .env("TMPDIR", &dir)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env(
                "BAR_CASE_FAILURES",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures/timed-failures.txt"),
            ),
    )
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let file = fs::read_to_string(dir.join("target/bar/verdict-check.json"))
        .unwrap_or_else(|e| panic!("no verdict file: {e}\n{stdout}"));
    let value: serde_json::Value = serde_json::from_str(&file).expect("the file is JSON");
    assert_eq!(value["overall"], "red", "{file}");
    assert_eq!(value["job"], "check", "{file}");
    let failing: usize = value["red_suites"]
        .as_array()
        .expect("red_suites is a list")
        .iter()
        .map(|suite| suite["failing"].as_array().map_or(0, Vec::len))
        .sum();
    assert_eq!(failing, 13, "{file}");
    assert!(
        stdout.contains("verdict file: target/bar/verdict-check.json — red"),
        "{stdout}"
    );
    let _ = fs::remove_dir_all(dir);
}

/// **Run `bar check` over a fake cargo and return the verdict file it left.**
/// `script` is the body the fake runs for the commands the case cares about.
fn check_verdict_file(tag: &str, script: &str, stdout_of_test: &str) -> (String, String) {
    let dir = scratch_dir(tag);
    let cargo = dir.join("cargo");
    support::write_executable(&cargo, &format!("#!/bin/sh\n{script}\n"));
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("CARGO", cargo)
            .env("TMPDIR", &dir)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env(
                "BAR_CASE_FAILURES",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/fixtures")
                    .join(stdout_of_test),
            ),
    )
    .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let file = fs::read_to_string(dir.join("target/bar/verdict-check.json"))
        .unwrap_or_else(|e| panic!("no verdict file: {e}\n{stdout}"));
    let _ = fs::remove_dir_all(dir);
    (file, stdout)
}

/// **Green tests and a failed lint are a red job**, and the file says which
/// phase failed. The file is written when the job ends, not when its tests do.
#[test]
fn a_lint_that_fails_after_green_tests_leaves_a_red_verdict_file() {
    let (file, stdout) = check_verdict_file(
        "verdict-lint",
        r#"case " $* " in *" --no-run "*) exit 0 ;; esac
if [ "$1" = test ]; then cat "$BAR_CASE_FAILURES"; exit 0; fi
if [ "$1" = clippy ]; then echo 'error: a lint'; exit 1; fi"#,
        "green-full-check.txt",
    );
    let value: serde_json::Value = serde_json::from_str(&file).expect("JSON");
    assert_eq!(value["overall"], "red", "{file}\n{stdout}");
    assert_eq!(value["failed_phases"][0], "lint", "{file}");
}

/// **Infra tests and a failed lint are a red job.** The file keeps the infra
/// class and names the lint, so the code failure is not read as the machine's.
#[test]
fn a_lint_that_fails_beside_infra_tests_leaves_a_red_verdict_file() {
    let (file, stdout) = check_verdict_file(
        "verdict-mixed-phase",
        r#"case " $* " in *" --no-run "*) exit 0 ;; esac
if [ "$1" = test ]; then cat "$BAR_CASE_FAILURES"; exit 101; fi
if [ "$1" = clippy ]; then echo 'error: a lint'; exit 1; fi"#,
        "verdict-killed-binary.txt",
    );
    let value: serde_json::Value = serde_json::from_str(&file).expect("JSON");
    assert_eq!(value["overall"], "red", "{file}\n{stdout}");
    assert_eq!(value["infra"], "killed", "{file}");
    assert_eq!(value["failed_phases"][1], "lint", "{file}");
}

/// **A build that ran out of disk leaves a verdict file of the class infra**,
/// where it used to leave none.
#[test]
fn a_build_that_ran_out_of_disk_leaves_an_infra_verdict_file() {
    let (file, stdout) = check_verdict_file(
        "verdict-build",
        r#"if [ "$1" = build ]; then
  echo 'error: failed to write `/x/y.d`'
  echo '  No space left on device (os error 28)'
  exit 1
fi"#,
        "green-full-check.txt",
    );
    let value: serde_json::Value = serde_json::from_str(&file).expect("JSON");
    assert_eq!(value["overall"], "infra", "{file}\n{stdout}");
    assert_eq!(value["infra"], "disk-full", "{file}");
}

/// **A fake `cargo` that mimics a crashed sibling suite.** `fmt`, `build`
/// and `clippy` succeed silently; a real `-- --list` still lists one test,
/// so `narrow`'s zero-selection guard does not fire before the phase this
/// test targets ever runs, and lists none of them as ignored; a real test
/// run prints one clean `test result:`
/// line — as a sibling suite under `--no-fail-fast` would — and then exits
/// non-zero, exactly as the crashed suite that printed nothing could not
/// stop it from doing.
fn write_fake_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) echo 'some::test::name: test' ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 1 ;;
  *) exit 0 ;;
esac
"#;
    support::write_loop_cargo(&path, script);
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-test-{tag}"
    )));

    fs::create_dir_all(&dir).expect("create scratch dir");
    support::loop_repository(&dir);
    dir
}

/// **A crashed sibling suite must red the whole `bar check` run**, not read
/// as a clean pass because the one suite that printed a `test result:` line
/// printed a clean one. This is the exact hazard `run_check` used to miss:
/// it decided the test phase from the text alone, discarding cargo's own
/// exit status.
#[test]
fn a_crashed_sibling_suite_reds_the_whole_check_run() {
    let dir = scratch_dir("check");
    let fake_cargo = write_fake_cargo(&dir);

    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &fake_cargo)
            .env("BAR_HISTORY_DIR", dir.join("history")),
    )
    .expect("run bar check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "a crashed sibling suite must not exit zero: {stdout}"
    );
    assert!(
        stdout.contains("verdict: RED"),
        "a crashed sibling suite must not read GREEN: {stdout}"
    );
    assert!(
        stdout.contains("crashed or was killed"),
        "the reader needs to know this was not a counted test failure: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// **The same hazard, on `bar narrow`.** `run_narrow` decides its own test
/// phase the same way `run_check` does, at a separate call site — a fix to
/// one is not a fix to the other. The fake `cargo` lists one test so the
/// zero-selection guard does not fire before the phase this test targets.
#[test]
fn a_crashed_sibling_suite_reds_the_whole_narrow_run() {
    let dir = scratch_dir("narrow");
    let fake_cargo = write_fake_cargo(&dir);

    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(["narrow", "--crate", "jojobot-bar"])
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &fake_cargo)
            .env("BAR_HISTORY_DIR", dir.join("history")),
    )
    .expect("run bar narrow");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "a crashed sibling suite must not exit zero: {stdout}"
    );
    assert!(
        stdout.contains("verdict: RED"),
        "a crashed sibling suite must not read GREEN: {stdout}"
    );
    assert!(
        stdout.contains("crashed or was killed"),
        "the reader needs to know this was not a counted test failure: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}
