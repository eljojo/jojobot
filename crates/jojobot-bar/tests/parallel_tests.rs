//! **The bar's test phase runs the test binaries side by side, and runs all
//! of them.** A stub `cargo` records every call the real `bar` binary makes and
//! prints what cargo prints, so the assertions are over what the bar spawned.
//! No real cargo runs.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;
use support::{fenced_spawn, output_fenced};

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
    echo '  Executable unittests src/lib.rs (/x/deps/jojobot_domain-1)'
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
    # FAKE_MEET=<tenths of a second>: wait until FAKE_MEET_JOBS jobs (two when
    # it is unset) are running, for at most that long. An overlap is then
    # something the job sees happen, not something a one-second sleep has to be
    # long enough to catch.
    # Only the first FAKE_MEET_JOBS jobs to start wait; the jobs after them
    # have nothing left to meet.
    echo started >> "$(dirname "$0")/started.log"
    # Where this job was told to put its temp files, and a directory in it that
    # nothing removes, as a fixture that leaks one leaves it.
    echo "$TMPDIR" >> "$(dirname "$0")/tmpdir.log"
    mkdir -p "$TMPDIR/leaked-by-job-$$"
    ordinal="$(wc -l < "$(dirname "$0")/started.log")"
    if [ -n "$FAKE_MEET" ] && [ "$ordinal" -le "${FAKE_MEET_JOBS:-2}" ]; then
      n=0
      while [ "$(ls "$running" | wc -l)" -lt "${FAKE_MEET_JOBS:-2}" ] && [ "$n" -lt "$FAKE_MEET" ]; do
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
    support::write_executable(&path, script);
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

/// A directory no other run in this process uses. The shared serial separates
/// callers even when threads start together; the clock alone once let two bars
/// share a call log and remove a directory under each other.
fn a_directory_of_this_runs_own() -> PathBuf {
    std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-par"))
}

fn run_check(env: &[(&str, &str)]) -> Ran {
    run_bar(&["check"], env)
}

fn run_bar(args: &[&str], env: &[(&str, &str)]) -> Ran {
    let dir = a_directory_of_this_runs_own();
    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    let cargo = write_cargo(&dir);
    let out = output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(args)
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &cargo)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            // The caller's own limit must not decide what a case measures.
            .env_remove("BAR_JOBS")
            .envs(env.iter().copied()),
    )
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
        "-p jojobot-domain --lib",
        "--test alpha",
        "--test beta",
        "--test gamma",
        "--doc",
    ] {
        let hits: Vec<&&String> = jobs.iter().filter(|j| j.contains(wanted)).collect();
        assert_eq!(hits.len(), 1, "{wanted} must run exactly once: {jobs:?}");
        let args: Vec<&str> = hits[0].split_whitespace().collect();
        for flag in ["--no-fail-fast", "--locked"] {
            assert!(args.contains(&flag), "{wanted} lost {flag}: {}", hits[0]);
        }
        // A job that names its own package runs without the workspace scope,
        // and every other job keeps it.
        assert_eq!(
            args.contains(&"--workspace"),
            !wanted.starts_with("-p "),
            "{wanted} has the wrong scope: {}",
            hits[0]
        );
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
        "-p jojobot-domain --lib",
        "--workspace --test alpha",
        "--workspace --test beta",
        "--workspace --test gamma",
        "--workspace --doc",
    ]
    .iter()
    .map(|job| {
        ran.log
            .find(&format!("$ cargo test {job}"))
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

/// **With no variable set, eight jobs run at once.** The verdict names the limit
/// the run used, which is what says eight, and the stub proves it: thirteen jobs
/// each wait until eight are running together, so a bar that ran seven at once
/// sees seven in every job and a bar that ran nine could not, because the
/// limit holds. The peak is eight exactly, on a machine of any speed, since the
/// jobs wait for each other rather than for a clock.
#[test]
fn the_limit_is_eight_when_nothing_sets_it() {
    let ran = run_check(&[
        ("FAKE_FILES", "12"),
        ("FAKE_BETA", "ok"),
        ("FAKE_MEET", "100"),
        ("FAKE_MEET_JOBS", "8"),
    ]);
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
    assert_eq!(
        ran.peak, 8,
        "the jobs ran {} at once under a default of eight",
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

/// **Every job of a run is told to work in a temp directory of the run's own,
/// and the run removes it.** The fake `cargo` writes a directory into the temp
/// directory it is given and removes nothing, as a fixture that leaks does. The
/// directory is named for the run, it is inside the scratch directory the case
/// gave the bar as its own temp directory, and it is gone when the run ends with
/// what the jobs left in it.
#[test]
fn the_jobs_of_a_run_work_in_a_directory_of_its_own_that_is_gone_when_it_ends() {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("run-root-case"));
    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    let cargo = write_cargo(&dir);
    let out = output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &cargo)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env_remove("BAR_JOBS"),
    )
    .expect("run bar");
    let told: Vec<String> = fs::read_to_string(dir.join("tmpdir.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let left: Vec<String> = fs::read_dir(&dir)
        .expect("the scratch dir")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("jojobot-bar-"))
        .collect();
    let _ = fs::remove_dir_all(&dir);
    // The run root is in memory when the host has room, and in the temp directory
    // the bar was given when it has not.
    let memory = Path::new(jojobot_bar::scratch::MEMORY_DIR);
    let base = jojobot_bar::scratch::place(
        memory,
        jojobot_bar::scratch::free_bytes(memory),
        jojobot_bar::scratch::MINIMUM_FREE_BYTES,
        &dir,
    );

    assert!(
        !told.is_empty(),
        "no job reported its temp directory: {out:?}"
    );
    for place in &told {
        let name = Path::new(place)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        assert!(
            name.starts_with("jojobot-bar-"),
            "a job was not told to use a directory of the run's own: {place}"
        );
        assert!(
            Path::new(place).starts_with(base.base()),
            "the run's directory is not inside the directory the bar placed it in: {place}"
        );
        assert!(
            !Path::new(place).exists(),
            "the run's directory was left behind: {place}"
        );
    }
    assert!(
        left.is_empty(),
        "the run's directory was left behind: {left:?}"
    );
}

/// Threads share one clock-free naming rule with the sweep. The old private
/// counter separated calls but made names the sweep could not recognise.
#[test]
fn two_runs_get_distinct_directories_the_sweep_recognises() {
    let names: std::collections::BTreeSet<PathBuf> = std::thread::scope(|scope| {
        (0..32)
            .map(|_| scope.spawn(a_directory_of_this_runs_own))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().expect("a thread"))
            .collect()
    });
    assert_eq!(names.len(), 32, "two callers were given one directory");
    assert!(
        names.iter().all(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .and_then(jojobot_ports::scratch::owner)
                == Some(std::process::id())
        }),
        "every caller uses the sweep's shared naming rule: {names:?}"
    );
}

/// **A stub written while other runs start children still executes.** The cases
/// above run on threads released together, and each writes its own stub `cargo`
/// and has the bar run it. A child started on another thread between the
/// stub's creation and the close of its write handle carries a copy of that
/// handle until it execs, and an exec of the stub in that window fails with
/// "Text file busy". The run then reads as a bar that stopped after one phase,
/// or never ran one.
///
/// So this writes stubs one after another while other threads start children
/// without pause, and executes each one. Every exec has to succeed.
#[test]
fn a_stub_written_while_other_runs_start_children_still_executes() {
    use std::sync::atomic::{AtomicBool, Ordering};
    let stop = AtomicBool::new(false);
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                while !stop.load(Ordering::Relaxed) {
                    let mut child = fenced_spawn(
                        Command::new("true")
                            .stdin(std::process::Stdio::null())
                            .stdout(std::process::Stdio::null())
                            .stderr(std::process::Stdio::null()),
                    )
                    .expect("an ordinary child starts");
                    child.wait().expect("an ordinary child ends");
                }
            });
        }
        let mut refused = Vec::new();
        for _ in 0..400 {
            let dir = a_directory_of_this_runs_own();
            fs::create_dir_all(&dir).expect("scratch dir");
            let cargo = write_cargo(&dir);
            let started = fenced_spawn(
                Command::new(&cargo)
                    .arg("fmt")
                    .env("TMPDIR", &dir)
                    .stdin(std::process::Stdio::null())
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null()),
            );
            match started {
                Ok(mut child) => {
                    child.wait().expect("the stub ends");
                }
                Err(e) => refused.push(e.to_string()),
            }
            let _ = fs::remove_dir_all(&dir);
        }
        stop.store(true, Ordering::Relaxed);
        assert!(
            refused.is_empty(),
            "{} of 400 stubs could not be executed: {:?}",
            refused.len(),
            refused.first()
        );
    });
}
