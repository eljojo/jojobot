//! **The bar's test phase runs the test binaries side by side, and runs all
//! of them.** A stub `cargo` records every call the real `bar` binary makes and
//! prints what cargo prints, so the assertions are over what the bar spawned.
//! No real cargo runs.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// **A fake `cargo` that lists four test binaries and fails one of them.**
/// `--no-run` prints the `Executable` lines cargo prints; each `test` job
/// sleeps one second, so a serial run cannot finish quickly, and the job named
/// `beta` fails with a named case and a non-zero exit, as a red suite does.
/// `FAKE_BETA=ok` makes it pass. `FAKE_CRASH=<file>` kills that job with
/// SIGKILL before it prints anything, as a crashed suite is.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --no-run "*)
    echo '    Finished `test` profile [unoptimized + debuginfo] target(s) in 0.10s'
    echo '  Executable unittests src/lib.rs (/x/deps/lib-1)'
    if [ -n "$FAKE_FILES" ]; then
      i=1
      while [ "$i" -le "$FAKE_FILES" ]; do
        echo "  Executable tests/file$i.rs (/x/deps/file$i-$i)"
        i=$((i + 1))
      done
    else
      echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
      echo '  Executable tests/beta.rs (/x/deps/beta-3)'
      echo '  Executable tests/gamma.rs (/x/deps/gamma-4)'
    fi
    exit 0 ;;
esac
case "$1" in
  test)
    # How many jobs are running right now, counting this one.
    running="$(dirname "$0")/running"
    mkdir -p "$running"
    touch "$running/$$"
    ls "$running" | wc -l >> "$(dirname "$0")/peak.log"
    # FAKE_MEET=<tenths of a second>: wait until a second job is running too,
    # for at most that long. An overlap is then something the job sees happen,
    # not something a one-second sleep has to be long enough to catch.
    if [ -n "$FAKE_MEET" ]; then
      n=0
      while [ "$(ls "$running" | wc -l)" -lt 2 ] && [ "$n" -lt "$FAKE_MEET" ]; do
        sleep 0.1
        n=$((n + 1))
      done
      ls "$running" | wc -l >> "$(dirname "$0")/peak.log"
    fi
    sleep 1
    rm -f "$running/$$"
    case " $* " in
      *" --test $FAKE_CRASH "*) kill -9 $$ ;;
    esac
    if [ "$FAKE_BETA" != ok ]; then
      case " $* " in
        *" --test beta "*)
          echo 'test the_beta_case ... FAILED'
          echo 'test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s'
          exit 101 ;;
      esac
    fi
    echo 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.00s'
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

struct Ran {
    stdout: String,
    success: bool,
    calls: Vec<String>,
    log: String,
    /// The most jobs the stub saw running at once.
    peak: usize,
    exit: Option<i32>,
    stderr: String,
}

fn run_check(env: &[(&str, &str)]) -> Ran {
    run_bar(&["check"], env)
}

fn run_bar(args: &[&str], env: &[(&str, &str)]) -> Ran {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("jojobot-bar-par-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&dir).expect("scratch dir");
    let cargo = write_cargo(&dir);
    let out = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .args(args)
        .current_dir(&dir)
        .env("CARGO", &cargo)
        // The caller's own limit must not decide what a case measures.
        .env_remove("BAR_JOBS")
        .envs(env.iter().copied())
        .output()
        .expect("run bar");
    let calls = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let peak = fs::read_to_string(dir.join("peak.log"))
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.trim().parse().ok())
        .max()
        .unwrap_or(0);
    let log = fs::read_to_string(dir.join("target/bar/check.log")).unwrap_or_default();
    let _ = fs::remove_dir_all(&dir);
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        success: out.status.success(),
        calls,
        log,
        peak,
        exit: out.status.code(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    }
}

/// **Each listed binary runs once, and a red one does not stop the rest.**
/// The calls are the plain workspace run split by target, each still
/// `--no-fail-fast` and `--locked`. The counts add up across jobs, and the
/// failing case is named.
#[test]
fn every_listed_job_runs_once_and_a_red_job_stops_none_of_the_others() {
    let ran = run_check(&[]);
    let jobs: Vec<&String> = ran
        .calls
        .iter()
        .filter(|c| c.starts_with("test ") && !c.contains("--no-run"))
        .collect();
    for wanted in [
        "--lib",
        "--test alpha",
        "--test beta",
        "--test gamma",
        "--doc",
    ] {
        let hits: Vec<&&String> = jobs.iter().filter(|j| j.contains(wanted)).collect();
        assert_eq!(hits.len(), 1, "{wanted} must run exactly once: {jobs:?}");
        for flag in ["--workspace", "--no-fail-fast", "--locked"] {
            assert!(
                hits[0].split_whitespace().any(|a| a == flag),
                "{wanted} lost {flag}: {}",
                hits[0]
            );
        }
    }
    assert_eq!(jobs.len(), 5, "a job nobody listed ran: {jobs:?}");
    assert!(
        !ran.success,
        "a red job must redden the bar: {}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("the_beta_case"),
        "the failing case is not named: {}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("14 passed"),
        "the counts of the five jobs are not summed (4 x 3 + 2): {}",
        ran.stdout
    );
}

/// **The jobs overlap.** Each job waits until it sees a second job running, and
/// the stub counts how many it saw at once. A run that overlapped them saw more
/// than one. Neither a wall time nor a one-second sleep can say this on a loaded
/// machine, where a start can lag by seconds, so the jobs wait for each other
/// for up to ten seconds, and the count is what is asserted. A bar that ran them
/// one at a time waits that long in every job and still sees one. The positive
/// keeps it from passing over a run that did nothing: the five jobs above are
/// all recorded.
#[test]
fn the_jobs_run_side_by_side() {
    let ran = run_check(&[("FAKE_MEET", "100")]);
    assert!(
        ran.calls.iter().filter(|c| c.starts_with("test ")).count() >= 5,
        "the jobs did not run: {:?}",
        ran.calls
    );
    assert!(
        ran.peak > 1,
        "five one-second jobs never ran two at once: peak {}",
        ran.peak
    );
}

/// **The log keeps every job's own output under the command that ran it**,
/// in the order the jobs were listed whatever order they finished in.
#[test]
fn the_log_carries_every_jobs_output_in_listing_order() {
    let ran = run_check(&[]);
    let positions: Vec<usize> = [
        "--lib",
        "--test alpha",
        "--test beta",
        "--test gamma",
        "--doc",
    ]
    .iter()
    .map(|job| {
        ran.log
            .find(&format!("$ cargo test --workspace {job}"))
            .unwrap_or_else(|| panic!("the log has no section for {job}: {}", ran.log))
    })
    .collect();
    assert!(
        positions.windows(2).all(|w| w[0] < w[1]),
        "the sections are not in listing order: {positions:?}"
    );
}

/// **A job that crashes reddens the bar, and the others still run.** A killed
/// suite prints no `test result:` line, so the text alone reads clean and the
/// job's exit status is the only witness. Every other job here passes, so
/// nothing but that status can make the bar red.
#[test]
fn a_job_that_crashes_reddens_the_bar_and_stops_none_of_the_others() {
    let ran = run_check(&[("FAKE_BETA", "ok"), ("FAKE_CRASH", "gamma")]);
    let jobs = ran
        .calls
        .iter()
        .filter(|c| c.starts_with("test ") && !c.contains("--no-run"))
        .count();
    assert_eq!(jobs, 5, "a crash stopped the other jobs: {:?}", ran.calls);
    assert!(
        ran.stdout.contains("12 passed") && ran.stdout.contains("0 failed"),
        "the four jobs that reported are not summed (4 x 3, none failed): {}",
        ran.stdout
    );
    assert!(
        !ran.success,
        "a crashed job reads as a clean run: {}",
        ran.stdout
    );
}

/// **No more jobs run at once than the limit says, and the limit is reached.**
/// Twelve test files and the doc-tests make thirteen jobs of one second each;
/// with the limit at two the stub never sees a third running, and does see two.
/// The second half keeps the first from passing over a run that never overlapped.
#[test]
fn never_more_jobs_run_at_once_than_the_limit_says() {
    let ran = run_check(&[("FAKE_FILES", "12"), ("FAKE_BETA", "ok"), ("BAR_JOBS", "2")]);
    assert!(ran.success, "the run was meant to be green: {}", ran.stdout);
    assert_eq!(
        ran.peak, 2,
        "the jobs ran {} at once under a limit of 2",
        ran.peak
    );
}

/// **With no variable set the limit is eight.** The verdict names the limit
/// the run used, which is what says eight. How many jobs the stub saw at once
/// depends on how fast a loaded machine starts them, so the count is only held
/// to be above one and no higher than the limit.
#[test]
fn the_limit_is_eight_when_nothing_sets_it() {
    let ran = run_check(&[("FAKE_FILES", "12"), ("FAKE_BETA", "ok")]);
    assert!(ran.success, "the run was meant to be green: {}", ran.stdout);
    let line = ran
        .stdout
        .lines()
        .find(|l| l.starts_with("jobs:"))
        .unwrap_or_else(|| panic!("no jobs line on the verdict: {}", ran.stdout));
    assert!(
        line.contains(" 8 "),
        "the default is not eight jobs at once: {line}"
    );
    assert!(
        ran.peak > 1 && ran.peak <= 8,
        "the jobs ran {} at once under a limit of 8",
        ran.peak
    );
}

/// **The verdict names the limit that produced the run**, so a wall time read
/// later says what it was measured at.
#[test]
fn the_verdict_names_the_limit_it_ran_at() {
    let ran = run_check(&[("FAKE_BETA", "ok"), ("BAR_JOBS", "3")]);
    let line = ran
        .stdout
        .lines()
        .find(|l| l.starts_with("jobs:"))
        .unwrap_or_else(|| panic!("no jobs line on the verdict: {}", ran.stdout));
    assert!(
        line.contains('3'),
        "the line does not carry the limit: {line}"
    );
}

/// **A limit that is not a number is refused before anything runs**, with the
/// variable and the value named, and with exit code 2.
#[test]
fn a_limit_that_is_not_a_number_is_refused_before_any_phase_runs() {
    let ran = run_check(&[("BAR_JOBS", "many")]);
    assert_eq!(
        ran.exit,
        Some(2),
        "stdout: {} stderr: {}",
        ran.stdout,
        ran.stderr
    );
    assert!(
        ran.stderr.contains("BAR_JOBS") && ran.stderr.contains("many"),
        "the refusal does not name the variable and the value: {}",
        ran.stderr
    );
    assert!(
        ran.calls.is_empty(),
        "a phase ran under a limit nobody could read: {:?}",
        ran.calls
    );
}

/// **`bar rooms` runs each named room target as a job of its own, side by
/// side, and builds the workspace once first.** The room suites spawn the built
/// server, so the build comes before any of them. Each job is `cargo test -p
/// jojobot-exercise --features rooms --test <name> --no-fail-fast --locked`.
/// One red room reddens the run and stops none of the others.
#[test]
fn bar_rooms_builds_once_then_runs_each_named_room_and_a_red_one_stops_none() {
    let ran = run_bar(
        &[
            "rooms", "--test", "alpha", "--test", "beta", "--test", "gamma",
        ],
        &[],
    );
    let builds = ran.calls.iter().filter(|c| c.starts_with("build ")).count();
    assert_eq!(builds, 1, "the workspace is built once: {:?}", ran.calls);
    let jobs: Vec<&String> = ran
        .calls
        .iter()
        .filter(|c| c.starts_with("test "))
        .collect();
    assert_eq!(jobs.len(), 3, "one job per room: {jobs:?}");
    for room in ["alpha", "beta", "gamma"] {
        let hits: Vec<&&String> = jobs
            .iter()
            .filter(|j| j.contains(&format!("--test {room} ")))
            .collect();
        assert_eq!(hits.len(), 1, "{room} must run once: {jobs:?}");
        for flag in [
            "-p jojobot-exercise",
            "--features rooms",
            "--no-fail-fast",
            "--locked",
        ] {
            assert!(hits[0].contains(flag), "{room} lost {flag}: {}", hits[0]);
        }
    }
    assert!(
        !ran.success,
        "a red room must redden the run: {}",
        ran.stdout
    );
    assert!(
        ran.stdout.contains("the_beta_case") && ran.stdout.contains("8 passed"),
        "the red case is not named, or the three rooms are not summed (3 + 2 + 3): {}",
        ran.stdout
    );
    assert!(
        ran.stdout.lines().any(|l| l.starts_with("jobs:")),
        "the verdict does not name the limit: {}",
        ran.stdout
    );
}

/// **The rooms overlap, under the same limit as the bar's test phase.** Twelve
/// one-second rooms with the limit at two never show a third running, and show
/// two.
#[test]
fn bar_rooms_honours_the_job_limit() {
    let args: Vec<String> = (1..=12)
        .flat_map(|i| ["--test".to_string(), format!("room{i}")])
        .collect();
    let mut argv: Vec<&str> = vec!["rooms"];
    argv.extend(args.iter().map(String::as_str));
    let ran = run_bar(&argv, &[("FAKE_BETA", "ok"), ("BAR_JOBS", "2")]);
    assert!(ran.success, "the run was meant to be green: {}", ran.stdout);
    assert_eq!(
        ran.peak, 2,
        "rooms ran {} at once under a limit of 2",
        ran.peak
    );
}

/// **A bare `bar rooms` names no room and is refused**, rather than reading as
/// a green run over nothing.
#[test]
fn bar_rooms_with_no_room_named_is_refused() {
    let ran = run_bar(&["rooms"], &[]);
    assert_eq!(
        ran.exit,
        Some(2),
        "stdout: {} stderr: {}",
        ran.stdout,
        ran.stderr
    );
    assert!(ran.calls.is_empty(), "something ran: {:?}", ran.calls);
}
