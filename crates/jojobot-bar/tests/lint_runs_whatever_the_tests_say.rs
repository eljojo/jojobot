//! **The bar lints whatever the tests did, and says both.** A red test step used
//! to end the run with the lint "not run", so a lint failure sat behind every
//! known red and was found only after the red was gone. The lint is a separate
//! question with its own answer: it runs after the tests whether they passed or
//! not, and the verdict carries both. A phase that came before the tests and
//! failed (the format check, the build) still ends the run, because there is
//! nothing to lint.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// **A fake `cargo`** that records every call and answers by environment:
/// `FAKE_TESTS=fail` fails one test, `FAKE_CLIPPY=fail` fails the lint,
/// `FAKE_FMT=fail` fails the format check.
fn write_fake_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case "$1" in
  fmt) [ "$FAKE_FMT" = fail ] && exit 1; exit 0 ;;
  clippy) [ "$FAKE_CLIPPY" = fail ] && { echo 'error: a lint'; exit 101; }; exit 0 ;;
  test)
    case " $* " in
      *" --list "*)
        case " $* " in
          *" --ignored "*) ;;
          *) echo 'some::test::name: test' ;;
        esac
        exit 0 ;;
    esac
    if [ "$FAKE_TESTS" = fail ]; then
      echo 'test some::tests::a_red ... FAILED'
      echo 'test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
      exit 101
    fi
    echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#;
    let mut f = fs::File::create(&path).expect("write fake cargo");
    f.write_all(script.as_bytes())
        .expect("write fake cargo body");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod");
    path
}

fn scratch_dir(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-lint-{tag}-{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

struct Ran {
    stdout: String,
    success: bool,
    clippy_calls: usize,
}

/// Run `bar` with `args`, the fake cargo answering by `env`.
fn bar(tag: &str, args: &[&str], env: &[(&str, &str)]) -> Ran {
    let dir = scratch_dir(tag);
    let cargo = write_fake_cargo(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .args(args)
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &cargo)
        .env_remove("BAR_JOBS")
        .envs(env.iter().copied())
        .output()
        .expect("run bar");
    let clippy_calls = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("clippy"))
        .count();
    let _ = fs::remove_dir_all(&dir);
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        success: out.status.success(),
        clippy_calls,
    }
}

const CHECK: &[&str] = &["check"];
const NARROW: &[&str] = &["narrow", "--crate", "jojobot-bar"];

/// **Red tests, clean lint: the lint ran and says clean beside the red.**
#[test]
fn a_red_test_step_still_runs_the_lint_and_reports_both() {
    for args in [CHECK, NARROW] {
        let ran = bar("red-clean", args, &[("FAKE_TESTS", "fail")]);
        assert!(
            ran.stdout.contains("test: FAILED"),
            "{args:?}: {}",
            ran.stdout
        );
        assert!(
            ran.stdout.contains("lint: ok — clean"),
            "{args:?} the lint was not run behind a red test step: {}",
            ran.stdout
        );
        assert_eq!(ran.clippy_calls, 1, "{args:?}: {}", ran.stdout);
        assert!(
            ran.stdout.contains("verdict: RED"),
            "{args:?}: {}",
            ran.stdout
        );
        assert!(!ran.success, "{args:?}: the exit hid a red test step");
    }
}

/// **Red tests and a failing lint: both are reported as failed.**
#[test]
fn a_red_test_step_and_a_failing_lint_are_both_reported() {
    for args in [CHECK, NARROW] {
        let ran = bar(
            "red-red",
            args,
            &[("FAKE_TESTS", "fail"), ("FAKE_CLIPPY", "fail")],
        );
        assert!(
            ran.stdout.contains("test: FAILED"),
            "{args:?}: {}",
            ran.stdout
        );
        assert!(
            ran.stdout.contains("lint: FAILED"),
            "{args:?} a failing lint was hidden behind a red test step: {}",
            ran.stdout
        );
        assert!(!ran.success, "{args:?}");
    }
}

/// **The controls**: green tests with a failing lint is red, and all green is
/// green, so the cases above cannot pass over a bar that always lints or never
/// does.
#[test]
fn a_failing_lint_alone_is_red_and_everything_clean_is_green() {
    for args in [CHECK, NARROW] {
        let lint_only = bar("lint-only", args, &[("FAKE_CLIPPY", "fail")]);
        assert!(
            lint_only.stdout.contains("lint: FAILED"),
            "{args:?}: {}",
            lint_only.stdout
        );
        assert!(
            lint_only.stdout.contains("test: ok"),
            "{args:?}: {}",
            lint_only.stdout
        );
        assert!(!lint_only.success, "{args:?}");

        let clean = bar("clean", args, &[]);
        assert!(
            clean.stdout.contains("verdict: GREEN"),
            "{args:?}: {}",
            clean.stdout
        );
        assert!(clean.success, "{args:?}: {}", clean.stdout);
        assert_eq!(clean.clippy_calls, 1, "{args:?}");
    }
}

/// **A phase before the tests that failed still ends the run**: with no format
/// there is nothing to lint, and the verdict says the lint did not run.
#[test]
fn a_failed_format_check_still_ends_the_run_before_the_lint() {
    for args in [CHECK, NARROW] {
        let ran = bar("fmt", args, &[("FAKE_FMT", "fail")]);
        assert!(
            ran.stdout.contains("fmt-check: FAILED"),
            "{args:?}: {}",
            ran.stdout
        );
        assert!(
            ran.stdout.contains("lint: not run"),
            "{args:?}: {}",
            ran.stdout
        );
        assert_eq!(ran.clippy_calls, 0, "{args:?}");
    }
}
