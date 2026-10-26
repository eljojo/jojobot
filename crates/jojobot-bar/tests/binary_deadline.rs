//! **A test binary that passes its deadline fails the run and names itself.** A
//! hung binary used to hold a runner until the job's own timeout and leave nothing
//! behind. The bar kills it with the processes it started, marks the job red,
//! names the binary in the verdict and keeps what it had printed in the log. The
//! real `bar` binary runs against a stub `cargo`; `BAR_BINARY_DEADLINE_SECS` sets
//! the deadline to a second, because the real one is minutes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod support;

/// **A fake `cargo` with three test files, one of which hangs.** The job whose
/// arguments hold the word `beta` prints a line and then sleeps for `HANG_FOR`
/// seconds; the others pass at once. The sleep is a child of the stub, so only a
/// kill of the whole process group ends it early.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --no-run "*)
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    echo '  Executable tests/beta.rs (/x/deps/beta-3)'
    echo '  Executable tests/gamma.rs (/x/deps/gamma-4)'
    exit 0 ;;
esac
case "$1" in
  test)
    case " $* " in
      *" beta "*)
        echo 'test the_slow_case ... started and printed before the hang'
        sleep "$HANG_FOR"
        echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
        exit 0 ;;
    esac
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
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-deadline"));
    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    dir
}

struct Ran {
    stdout: String,
    success: bool,
    took: Duration,
    log: String,
    started: Vec<String>,
}

fn bar(dir: &Path, level: &str) -> Ran {
    let began = Instant::now();
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg(level)
            .current_dir(dir)
            .env("TMPDIR", dir)
            .env("CARGO", write_cargo(dir))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("BAR_JOBS", "2")
            .env("BAR_BINARY_DEADLINE_SECS", "1")
            .env("HANG_FOR", "20"),
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
        log: fs::read_to_string(dir.join(format!("target/bar/{level}.log"))).unwrap_or_default(),
        started,
    }
}

/// **A hung binary ends in its deadline, not after its hang.** The run is red, the
/// verdict names the binary, the log keeps what it printed before it hung and says
/// it was killed, and the binaries beside it still ran.
#[test]
fn a_binary_past_its_deadline_fails_the_check_and_is_named() {
    let ran = bar(&scratch(), "check");
    assert!(!ran.success, "{}", ran.stdout);
    assert!(
        ran.took < Duration::from_secs(12),
        "the run waited {:?} for a hung binary\n{}",
        ran.took,
        ran.stdout
    );
    assert!(
        ran.stdout
            .lines()
            .any(|l| l.contains("TIMED OUT") && l.contains("beta")),
        "the verdict does not name the binary that timed out\n{}",
        ran.stdout
    );
    assert!(
        ran.log.contains("started and printed before the hang"),
        "the output the binary printed was not kept\n{}",
        ran.log
    );
    assert!(
        ran.log.contains("TIMED OUT"),
        "the log does not say the binary was killed\n{}",
        ran.log
    );
    for beside in ["alpha", "gamma"] {
        assert!(
            ran.started.iter().any(|call| call.contains(beside)),
            "the {beside} binary did not run: {:?}",
            ran.started
        );
    }
}

/// **At the report level a timed-out binary is the first red job**, so the run ends
/// there as it does for any red job.
#[test]
fn a_binary_past_its_deadline_is_the_first_red_job_of_the_report() {
    let ran = bar(&scratch(), "report");
    assert!(!ran.success, "{}", ran.stdout);
    assert!(
        ran.took < Duration::from_secs(12),
        "the report waited {:?} for a hung binary\n{}",
        ran.took,
        ran.stdout
    );
    assert!(
        ran.stdout
            .lines()
            .any(|l| l.contains("TIMED OUT") && l.contains("beta")),
        "{}",
        ran.stdout
    );
}
