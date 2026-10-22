//! **The verdict of a check says how long each phase took.** A run that prints
//! only the total, or only the test jobs' wall time, leaves a person guessing
//! which phase to look at when the bar is slow. Every phase this build runs
//! prints one line, with the wall time of that phase alone.
//!
//! The real `bar` binary runs against a stub `cargo` that sleeps a different,
//! known time in each phase, so the figures printed can be held to what the
//! stub spent and not only to their shape.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod support;

/// A fake `cargo` that sleeps a known time per phase and passes. The listing
/// names no test binary, so the only test job is the doc-tests.
fn write_fake_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case "$1" in
  fmt) sleep 0.2; exit 0 ;;
  build) sleep 0.3; exit 0 ;;
  clippy) sleep 0.5; exit 0 ;;
  test)
    case " $* " in
      *" --no-run "*) sleep 0.4; exit 0 ;;
    esac
    sleep 0.1
    echo 'test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

fn scratch_dir(tag: &str) -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-times-{tag}-{}-{nanos}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    dir
}

/// The seconds a line ends with, `wall 1.2s` or `wall 1s`.
fn wall_of(line: &str) -> f64 {
    let after = line
        .rsplit_once("wall ")
        .unwrap_or_else(|| panic!("no wall time on: {line}"))
        .1;
    let digits: String = after
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("no number after wall on: {line}"))
}

fn run(level: &str) -> (String, Duration) {
    let dir = scratch_dir(level);
    let cargo = write_fake_cargo(&dir);
    let started = Instant::now();
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg(level)
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &cargo)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env_remove("BAR_JOBS"),
    )
    .expect("run bar");
    let took = started.elapsed();
    let _ = fs::remove_dir_all(&dir);
    (String::from_utf8_lossy(&out.stdout).into_owned(), took)
}

/// **Each phase has a line, and the figure on it is that phase's own.** The
/// stub spends a different time in each, so a line that carried a total, or the
/// same figure as its neighbour, fails the floor it was given; the phases'
/// figures together cannot add up to more than the run took, so a cumulative
/// reading fails the ceiling.
#[test]
fn a_check_and_a_report_say_how_long_each_phase_took() {
    for level in ["check", "report"] {
        let (stdout, took) = run(level);
        assert!(stdout.contains("verdict: GREEN"), "{level}: {stdout}");
        let mut total = 0.0;
        for (phase, floor) in [
            ("fmt-check", 0.2),
            ("build", 0.3),
            ("test-compile", 0.4),
            ("lint", 0.5),
            ("doc-tests", 0.1),
        ] {
            let line = stdout
                .lines()
                .find(|line| line.starts_with(&format!("{phase}: ")))
                .unwrap_or_else(|| panic!("{level}: no {phase} line: {stdout}"));
            let wall = wall_of(line);
            assert!(
                wall >= floor - 0.05,
                "{level}: {phase} took at least {floor}s: {line}"
            );
            total += wall;
        }
        let jobs = stdout
            .lines()
            .find(|line| line.starts_with("jobs: "))
            .unwrap_or_else(|| panic!("{level}: no jobs line: {stdout}"));
        assert!(jobs.contains("wall "), "{level}: {jobs}");
        assert!(
            total <= took.as_secs_f64() + 0.1,
            "{level}: the phases' figures ({total}s) are each their own, so together they are \
             no more than the run ({}s): {stdout}",
            took.as_secs_f64()
        );
    }
}
