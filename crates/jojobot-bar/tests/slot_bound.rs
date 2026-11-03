//! **A run bound to its runner slot bounds every child it starts, whatever the
//! target, and a value that is not a core count is refused before any child runs.**
//! The real `bar` binary is driven over a stub `cargo` that records, for each call,
//! the two variables a bound sets, so what a child was given is read from the child.

use std::fs;
use std::process::Command;

mod support;

/// A stub `cargo` that logs its first argument and the bound it was given, answers
/// the test listing with one test, and succeeds at everything else.
fn write_logging_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
printf '%s jobs=%s threads=%s\n' "$1" "$CARGO_BUILD_JOBS" "$RUST_TEST_THREADS" >> "$(dirname "$0")/calls.log"
case " $* " in
  *" --list "*)
    case " $* " in
      *" --ignored "*) ;;
      *) printf '%s\n' 'alpha::first: test' ;;
    esac
    exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_loop_cargo(&path, script);
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-slot-{tag}"
    )));
    fs::create_dir_all(&dir).expect("create scratch dir");
    support::loop_repository(&dir);
    dir
}

/// What the bar did: its exit code, what it said on stderr and the calls the stub saw.
struct Ran {
    code: Option<i32>,
    stdout: String,
    stderr: String,
    calls: Vec<String>,
}

/// Run `bar <args>` over the logging stub with the slot variable and the job limit
/// given (or removed), and read back the stub's calls.
fn run(tag: &str, args: &[&str], slot: Option<&str>, jobs: Option<&str>) -> Ran {
    run_inheriting(tag, args, slot, jobs, (None, None))
}

/// The same, with the runner's own `CARGO_BUILD_JOBS` and `RUST_TEST_THREADS` given.
fn run_inheriting(
    tag: &str,
    args: &[&str],
    slot: Option<&str>,
    jobs: Option<&str>,
    inherited: (Option<&str>, Option<&str>),
) -> Ran {
    let dir = scratch_dir(tag);
    let cargo = write_logging_cargo(&dir);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(args)
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("CARGO", &cargo)
        // The history of a run is written under the scratch directory, never the real one.
        .env("BAR_HISTORY_DIR", dir.join("history"));
    for (name, value) in [
        ("BAR_SLOT_CORES", slot),
        ("BAR_JOBS", jobs),
        ("CARGO_BUILD_JOBS", inherited.0),
        ("RUST_TEST_THREADS", inherited.1),
    ] {
        match value {
            Some(value) => command.env(name, value),
            None => command.env_remove(name),
        };
    }
    let output = support::output_fenced(&mut command).expect("run the bar");
    let calls = fs::read_to_string(dir.join("calls.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let _ = fs::remove_dir_all(&dir);
    Ran {
        code: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        calls,
    }
}

/// **`bar narrow` bounds every cargo call it makes.** Six cores and two binaries at
/// once is six build jobs and three test threads; the test call is among them.
#[test]
fn a_narrow_run_gives_every_child_the_slots_bound() {
    let ran = run(
        "narrow-bound",
        &["narrow", "--crate", "jojobot-bar"],
        Some("6"),
        Some("2"),
    );
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(
        ran.calls.iter().any(|call| call.starts_with("test ")),
        "the run called cargo test: {:?}",
        ran.calls
    );
    // The calls that build and test carry the bound. The read-only `fmt` and
    // `metadata` calls inherit the caller's environment and are left alone.
    let working: Vec<_> = ran
        .calls
        .iter()
        .filter(|call| {
            ["test ", "build ", "clippy "]
                .iter()
                .any(|word| call.starts_with(word))
        })
        .collect();
    assert!(!working.is_empty(), "{:?}", ran.calls);
    assert!(
        working
            .iter()
            .all(|call| call.ends_with("jobs=6 threads=3")),
        "every build and test call carries the bound: {:?}",
        ran.calls
    );
}

/// **The bound never raises what the runner set.** A runner that gave two build jobs
/// and one test thread keeps them under a six-core slot; a runner that gave more than
/// the slot's share is brought down to it.
#[test]
fn a_narrow_run_never_raises_what_the_runner_set() {
    let working = |ran: &Ran| -> Vec<String> {
        ran.calls
            .iter()
            .filter(|call| {
                ["test ", "build ", "clippy "]
                    .iter()
                    .any(|word| call.starts_with(word))
            })
            .cloned()
            .collect()
    };
    let lower = run_inheriting(
        "narrow-lower",
        &["narrow", "--crate", "jojobot-bar"],
        Some("6"),
        Some("2"),
        (Some("2"), Some("1")),
    );
    assert_eq!(lower.code, Some(0), "{}", lower.stderr);
    let calls = working(&lower);
    assert!(!calls.is_empty(), "{:?}", lower.calls);
    assert!(
        calls.iter().all(|call| call.ends_with("jobs=2 threads=1")),
        "the runner's lower numbers stay: {:?}",
        lower.calls
    );
    let higher = run_inheriting(
        "narrow-higher",
        &["narrow", "--crate", "jojobot-bar"],
        Some("6"),
        Some("2"),
        (Some("20"), Some("9")),
    );
    assert_eq!(higher.code, Some(0), "{}", higher.stderr);
    assert!(
        working(&higher)
            .iter()
            .all(|call| call.ends_with("jobs=6 threads=3")),
        "numbers above the share come down to it: {:?}",
        higher.calls
    );
}

/// **With no slot variable no child is bounded**, the positive beside the case above.
#[test]
fn a_narrow_run_with_no_slot_variable_bounds_no_child() {
    let ran = run(
        "narrow-unbound",
        &["narrow", "--crate", "jojobot-bar"],
        None,
        None,
    );
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    assert!(
        ran.calls.iter().any(|call| call.starts_with("test ")),
        "{:?}",
        ran.calls
    );
    assert!(
        ran.calls
            .iter()
            .all(|call| call.ends_with("jobs= threads=")),
        "no call carries a bound: {:?}",
        ran.calls
    );
}

/// **A value that is not a core count is refused by name before any child runs**, by
/// each target that starts children: narrow and bundle both.
#[test]
fn a_bad_slot_value_is_refused_before_cargo_runs_by_narrow_and_by_bundle() {
    for (tag, args) in [
        ("bad-narrow", &["narrow", "--crate", "jojobot-bar"][..]),
        ("bad-bundle", &["bundle"][..]),
    ] {
        let ran = run(tag, args, Some("many"), None);
        assert_eq!(ran.code, Some(2), "{tag}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("BAR_SLOT_CORES") && ran.stderr.contains("\"many\""),
            "{tag}: the refusal names the variable and the value: {}",
            ran.stderr
        );
        assert!(
            ran.calls.is_empty(),
            "{tag}: no cargo ran before the refusal: {:?}",
            ran.calls
        );
    }
}

/// **A bad job limit is still refused by name when the slot is set**, by narrow and by
/// bundle, which never read the limit before.
#[test]
fn a_bad_job_limit_is_refused_by_name_when_the_slot_is_set() {
    for (tag, args) in [
        ("jobs-narrow", &["narrow", "--crate", "jojobot-bar"][..]),
        ("jobs-bundle", &["bundle"][..]),
    ] {
        let ran = run(tag, args, Some("4"), Some("many"));
        assert_eq!(ran.code, Some(2), "{tag}: {}", ran.stderr);
        assert!(
            ran.stderr.contains("BAR_JOBS") && ran.stderr.contains("\"many\""),
            "{tag}: the refusal names the limit and its value: {}",
            ran.stderr
        );
        assert!(ran.calls.is_empty(), "{tag}: {:?}", ran.calls);
    }
}

/// **The rooms lane gives its children the bound and says the numbers it used.** The
/// verdict's jobs line names the variable, the build jobs and the test threads, which
/// the unit case of the line cannot show reaching a real run.
#[test]
fn the_rooms_lane_bounds_its_children_and_says_the_numbers() {
    let ran = run(
        "rooms-bound",
        &["rooms", "--test", "adoption_room"],
        Some("6"),
        Some("2"),
    );
    assert_eq!(ran.code, Some(0), "{}{}", ran.stdout, ran.stderr);
    let working: Vec<_> = ran
        .calls
        .iter()
        .filter(|call| {
            ["test ", "build ", "clippy "]
                .iter()
                .any(|word| call.starts_with(word))
        })
        .collect();
    assert!(!working.is_empty(), "{:?}", ran.calls);
    assert!(
        working
            .iter()
            .all(|call| call.ends_with("jobs=6 threads=3")),
        "every build and test call carries the bound: {:?}",
        ran.calls
    );
    for said in ["BAR_SLOT_CORES", "6 build jobs", "3 test threads"] {
        assert!(
            ran.stdout.contains(said),
            "the verdict says {said:?}: {}",
            ran.stdout
        );
    }
}

/// **A slot that names more cores than the host has builds with no more than the
/// host.** The slot is four times the host's cores and more; every build and test call
/// the stub sees carries the host's own core count as its build jobs, which is the
/// number cargo would have used with nothing set.
#[test]
fn a_slot_naming_more_cores_than_the_host_builds_with_no_more_than_the_host() {
    let host = std::thread::available_parallelism().map_or(1, |cores| cores.get());
    let slot = (host * 4 + 100).to_string();
    let ran = run(
        "narrow-big-slot",
        &["narrow", "--crate", "jojobot-bar"],
        Some(&slot),
        Some("2"),
    );
    assert_eq!(ran.code, Some(0), "{}", ran.stderr);
    let working: Vec<_> = ran
        .calls
        .iter()
        .filter(|call| {
            ["test ", "build ", "clippy "]
                .iter()
                .any(|word| call.starts_with(word))
        })
        .collect();
    assert!(!working.is_empty(), "{:?}", ran.calls);
    let wanted = format!("jobs={host} ");
    assert!(
        working.iter().all(|call| call.contains(&wanted)),
        "every build and test call builds with the host's {host} cores: {:?}",
        ran.calls
    );
}
