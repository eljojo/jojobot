//! **A bar says, before it does anything, how long it is expected to take and
//! what a caller that runs it in the background has to do.** The real `bar`
//! binary runs against a stub `cargo` and a history directory the case seeds
//! with finished runs written by the bar's own recorder, so the expectation is
//! read from the shape the bar leaves, never from a made-up file.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_bar::{Run, RunHeader};

mod support;

/// A `cargo` that lists no test binary and succeeds at everything else. The
/// cases are about the first lines the bar prints, not about any test.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --no-run "*) exit 0 ;;
esac
exit 0
"#;
    support::write_executable(&path, script);
    path
}

fn scratch() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-start-lines-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

fn bar(dir: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(args)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", write_cargo(dir))
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env_remove("BAR_JOBS");
    command
}

/// **Leave one finished run of a level in the history**, as the bar leaves it:
/// a header written when the run began and written again with its end.
fn seed_finished_run(dir: &Path, level: &str, n: u32, started_at: &str, ended_at: &str) {
    let run = Run::begin(
        &dir.join("history"),
        RunHeader {
            run_id: format!("seed-{level}-{n}"),
            level: level.to_string(),
            checkout: "seed".to_string(),
            head: Some(format!("head-{n}")),
            dirty: Some(false),
            started_at: started_at.to_string(),
            bar_jobs: 4,
            load_average: None,
            cpus: Some(4),
        },
    )
    .expect("begin a seeded run");
    run.finish(ended_at).expect("finish a seeded run");
}

/// The first two lines the bar printed on stdout.
fn start_lines(dir: &Path, args: &[&str]) -> Vec<String> {
    let output = support::output_fenced(&mut bar(dir, args)).expect("run bar");
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .take(2)
        .map(str::to_string)
        .collect()
}

/// **The first line names the duration of this target from its recorded
/// runs.** Three finished runs of 20, 22 and 30 minutes: the middle one is the
/// expectation, so one slow run does not move it.
#[test]
fn the_first_line_names_the_duration_the_recorded_runs_of_this_target_took() {
    let dir = scratch();
    seed_finished_run(
        &dir,
        "check",
        1,
        "2026-10-08T10:00:00Z",
        "2026-10-08T10:20:00Z",
    );
    seed_finished_run(
        &dir,
        "check",
        2,
        "2026-10-09T10:00:00Z",
        "2026-10-09T10:22:00Z",
    );
    seed_finished_run(
        &dir,
        "check",
        3,
        "2026-10-10T10:00:00Z",
        "2026-10-10T10:30:00Z",
    );

    let lines = start_lines(&dir, &["check"]);

    assert_eq!(lines.len(), 2, "two start lines: {lines:?}");
    assert!(lines[0].contains("22m"), "{lines:?}");
    assert!(
        !lines[0].contains("no history"),
        "there is history for check: {lines:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **The second line says how long a background run has to be allowed and where
/// the exit code is.** It is printed whether or not there is history.
#[test]
fn the_second_line_names_the_timeout_and_where_the_exit_code_is() {
    let dir = scratch();
    seed_finished_run(
        &dir,
        "check",
        1,
        "2026-10-08T10:00:00Z",
        "2026-10-08T10:20:00Z",
    );

    let lines = start_lines(&dir, &["check"]);

    assert_eq!(lines.len(), 2, "two start lines: {lines:?}");
    assert!(lines[1].contains("7200000"), "{lines:?}");
    assert!(lines[1].contains("exit code"), "{lines:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// **With no history the first line says so**, and the second is still there.
#[test]
fn with_no_history_the_first_line_says_so_and_the_timeout_line_is_still_there() {
    let dir = scratch();

    let lines = start_lines(&dir, &["check"]);

    assert_eq!(lines.len(), 2, "two start lines: {lines:?}");
    assert!(lines[0].contains("no history yet"), "{lines:?}");
    assert!(lines[1].contains("7200000"), "{lines:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A run of another target is not this target's past.** The history holds
/// three runs of `check` and none of `report`, so `report` has no history.
#[test]
fn the_runs_of_another_target_are_not_this_targets_history() {
    let dir = scratch();
    seed_finished_run(
        &dir,
        "check",
        1,
        "2026-10-08T10:00:00Z",
        "2026-10-08T10:20:00Z",
    );
    seed_finished_run(
        &dir,
        "check",
        2,
        "2026-10-09T10:00:00Z",
        "2026-10-09T10:22:00Z",
    );
    seed_finished_run(
        &dir,
        "check",
        3,
        "2026-10-10T10:00:00Z",
        "2026-10-10T10:30:00Z",
    );

    let lines = start_lines(&dir, &["report"]);

    assert_eq!(lines.len(), 2, "two start lines: {lines:?}");
    assert!(lines[0].contains("no history yet"), "{lines:?}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A run that never finished is no data.** A bar that was killed leaves a
/// header with no end, and its length is unknown, so it is not counted.
#[test]
fn a_run_with_no_end_is_not_counted() {
    let dir = scratch();
    Run::begin(
        &dir.join("history"),
        RunHeader {
            run_id: "seed-check-killed".to_string(),
            level: "check".to_string(),
            checkout: "seed".to_string(),
            head: None,
            dirty: None,
            started_at: "2026-10-08T10:00:00Z".to_string(),
            bar_jobs: 4,
            load_average: None,
            cpus: None,
        },
    )
    .expect("begin a run that is never finished");

    let lines = start_lines(&dir, &["check"]);

    assert_eq!(lines.len(), 2, "two start lines: {lines:?}");
    assert!(lines[0].contains("no history yet"), "{lines:?}");
    let _ = fs::remove_dir_all(&dir);
}
