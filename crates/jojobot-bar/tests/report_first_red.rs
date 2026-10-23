//! **The report level stops at the first red job.** A push that breaks a case
//! is reported red when the first red test binary ends, and the binaries still
//! running are killed, instead of the verdict waiting for every other binary.
//! The boundary level reports every target, so it is untouched. The real `bar`
//! binary runs against a stub `cargo`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod support;

/// **A fake `cargo` with a library, three test files and a slow body.** The job
/// named by `RED_JOB` fails at once; every other test job sleeps for `SLOW`
/// seconds, so a bar that waited for them would take that long.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --no-run "*)
    echo '  Executable unittests src/lib.rs (/x/deps/lib-1)'
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    echo '  Executable tests/beta.rs (/x/deps/beta-3)'
    echo '  Executable tests/gamma.rs (/x/deps/gamma-4)'
    exit 0 ;;
esac
case "$1" in
  test)
    case " $* " in
      *" $RED_JOB "*)
        echo 'test the_case ... FAILED'
        echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
        exit 101 ;;
    esac
    sleep "$SLOW"
    echo 'test the_case ... ok'
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-first-red"));

    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    dir
}

struct Ran {
    stdout: String,
    success: bool,
    took: Duration,
    started: Vec<String>,
}

fn bar(dir: &Path, level: &str, red_job: &str) -> Ran {
    bar_with_jobs(dir, level, red_job, "2")
}

fn bar_with_jobs(dir: &Path, level: &str, red_job: &str, jobs: &str) -> Ran {
    let began = Instant::now();
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg(level)
            .current_dir(dir)
            .env("TMPDIR", dir)
            .env("CARGO", write_cargo(dir))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("BAR_JOBS", jobs)
            .env("RED_JOB", red_job)
            .env("SLOW", "8"),
    )
    .expect("run bar");
    let took = began.elapsed();
    let started = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("test ") && !call.contains("--no-run"))
        .map(str::to_string)
        .collect();
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        success: out.status.success(),
        took,
        started,
    }
}

/// **A red job ends the report at once.** The library job fails while the job
/// beside it sleeps; the bar kills the sleeper, starts no further job, and says
/// how many it did not run. **Paired**: the boundary level, over the same stub,
/// waits for every job, so the short run is the report level's doing.
#[test]
fn report_first_red_stops_the_report_but_not_the_boundary() {
    let dir = scratch();
    let report = bar(&dir, "report", "--lib");
    assert!(!report.success, "{}", report.stdout);
    assert!(
        report.took < Duration::from_secs(6),
        "the report waited {:?} for jobs that were not red\n{}",
        report.took,
        report.stdout
    );
    assert!(
        !report.started.iter().any(|call| call.contains("gamma")),
        "a job started after the first red one: {:?}",
        report.started
    );
    assert!(
        report.stdout.contains("did not run"),
        "the verdict does not say what it left out\n{}",
        report.stdout
    );

    let dir = scratch();
    let boundary = bar(&dir, "check", "--lib");
    assert!(!boundary.success, "{}", boundary.stdout);
    assert!(
        boundary.took >= Duration::from_secs(8),
        "the boundary level stopped early after {:?}",
        boundary.took
    );
    assert!(
        boundary.started.iter().any(|call| call.contains("gamma")),
        "the boundary level skipped a job: {:?}",
        boundary.started
    );
}

/// **A doc-test job the report killed is not a failed doc-test.** Every job starts
/// at once; the library job is red and the bar kills the rest, the doc-test job
/// among them. The verdict names no doc-test failure for a job that never gave an
/// answer. **Paired**: when the doc-test job is the red one, it is named.
#[test]
fn a_killed_doc_test_job_is_not_reported_as_failed_and_a_failing_one_is() {
    let dir = scratch();
    let killed = bar_with_jobs(&dir, "report", "--lib", "8");
    assert!(!killed.success, "{}", killed.stdout);
    assert!(
        killed.stdout.contains("did not run"),
        "the doc-test job was meant to be killed: {}",
        killed.stdout
    );
    assert!(
        !killed.stdout.contains("doc-tests: FAILED"),
        "a killed job was called a failed doc-test: {}",
        killed.stdout
    );

    let dir = scratch();
    let failing = bar_with_jobs(&dir, "report", "--doc", "8");
    assert!(!failing.success, "{}", failing.stdout);
    assert!(
        failing.stdout.contains("doc-tests: FAILED"),
        "a failing doc test was not named: {}",
        failing.stdout
    );
}

#[test]
fn report_first_red_accepts_an_all_green_stub() {
    let dir = scratch();
    let report = bar(&dir, "report", "no-red-job");
    assert!(report.success, "{}", report.stdout);
    assert!(report.started.iter().any(|call| call.contains("gamma")));
    assert!(!report.stdout.contains("did not run"), "{}", report.stdout);
}
