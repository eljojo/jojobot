//! **A test binary that passes its deadline fails the run and names itself.** A
//! hung binary used to hold a runner until the job's own timeout and leave nothing
//! behind. The bar kills it with the processes it started, marks the job red,
//! names the binary in the verdict and keeps what it had printed in the log. The
//! clock is the binary's own: compiling and the sum of several honest executables
//! in one `cargo test` do not count against one. The real `bar` binary runs against
//! a stub `cargo` and, for the bundled path, stub executables;
//! `BAR_BINARY_DEADLINE_SECS` sets the deadline to seconds, where the real one is
//! minutes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

mod support;

/// **A fake `cargo` with three test files.** The job of `beta` prints a compile
/// line, waits `COMPILE_FOR` seconds, and then runs `EXECUTABLES` executables one
/// after another: each prints its `Running` line and waits for `HANG_FOR` seconds,
/// except the second, which waits for `SECOND_FOR`. The wait is a child of the
/// stub that records its process id, so only a kill of the whole process group
/// ends it early.
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
        echo '   Compiling the_crate v0.1.0'
        sleep "$COMPILE_FOR"
        n=1
        while [ "$n" -le "$EXECUTABLES" ]; do
          # Each executable is a different real file, named by its line.
          if [ $((n % 2)) = 1 ]; then tool="$(command -v sleep)"; else tool="$(command -v python3)"; fi
          echo "     Running tests/beta_$n.rs ($tool)"
          echo "test the_slow_case_$n ... started and printed before the hang"
          wait_for="$HANG_FOR"
          [ "$n" = 2 ] && wait_for="$SECOND_FOR"
          if [ $((n % 2)) = 1 ]; then "$tool" "$wait_for" & else "$tool" -c 'import sys, time; time.sleep(float(sys.argv[1]))' "$wait_for" & fi
          echo $! > "$(dirname "$0")/sleep.pid"
          wait
          echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
          n=$((n + 1))
        done
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

/// How one run of the stub behaves.
struct Setup {
    deadline: &'static str,
    compile_for: &'static str,
    executables: &'static str,
    hang_for: &'static str,
    second_for: &'static str,
}

/// **One executable that hangs** for twenty seconds against a one-second deadline.
const HANGS: Setup = Setup {
    deadline: "1",
    compile_for: "0",
    executables: "1",
    hang_for: "20",
    second_for: "20",
};

struct Ran {
    stdout: String,
    success: bool,
    took: Duration,
    log: String,
    started: Vec<String>,
}

fn bar(dir: &Path, level: &str, setup: &Setup, bundle: Option<&Path>) -> Ran {
    let began = Instant::now();
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .arg(level)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", write_cargo(dir))
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env("BAR_JOBS", "2")
        .env("BAR_BINARY_DEADLINE_SECS", setup.deadline)
        .env("COMPILE_FOR", setup.compile_for)
        .env("EXECUTABLES", setup.executables)
        .env("HANG_FOR", setup.hang_for)
        .env("SECOND_FOR", setup.second_for)
        .env_remove("BAR_BUNDLE");
    if let Some(manifest) = bundle {
        command.env("BAR_BUNDLE", manifest);
    }
    let out = support::output_fenced(&mut command).expect("run bar");
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

fn timed_out(ran: &Ran) -> Vec<&str> {
    ran.stdout
        .lines()
        .filter(|line| line.contains("TIMED OUT"))
        .collect()
}

/// **A hung binary ends in its deadline, not after its hang.** The run is red, the
/// verdict names the binary, the log keeps what it printed before it hung and says
/// it was killed, and the binaries beside it still ran.
#[test]
fn a_binary_past_its_deadline_fails_the_check_and_is_named() {
    let ran = bar(&scratch(), "check", &HANGS, None);
    assert!(!ran.success, "{}", ran.stdout);
    assert!(
        ran.took < Duration::from_secs(12),
        "the run waited {:?} for a hung binary\n{}",
        ran.took,
        ran.stdout
    );
    assert!(
        timed_out(&ran).iter().any(|l| l.contains("beta")),
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

/// **The processes the binary started die with it.** The stub's wait is a child
/// that wrote its process id; a kill of the stub alone would leave it running.
#[test]
fn a_binary_past_its_deadline_takes_the_processes_it_started_with_it() {
    let dir = scratch();
    let ran = bar(&dir, "check", &HANGS, None);
    assert!(!ran.success, "{}", ran.stdout);
    let pid = fs::read_to_string(dir.join("sleep.pid")).expect("the stub recorded its child");
    let gone = (0..30).any(|_| {
        let alive = Command::new("kill")
            .args(["-0", pid.trim()])
            .output()
            .expect("run kill")
            .status
            .success();
        if alive {
            std::thread::sleep(Duration::from_millis(100));
        }
        !alive
    });
    assert!(gone, "process {} outlived its binary", pid.trim());
}

/// **At the report level a timed-out binary is the first red job**, so the run ends
/// there as it does for any red job.
#[test]
fn a_binary_past_its_deadline_is_the_first_red_job_of_the_report() {
    let ran = bar(&scratch(), "report", &HANGS, None);
    assert!(!ran.success, "{}", ran.stdout);
    assert!(
        ran.took < Duration::from_secs(12),
        "the report waited {:?} for a hung binary\n{}",
        ran.took,
        ran.stdout
    );
    assert!(
        timed_out(&ran).iter().any(|l| l.contains("beta")),
        "{}",
        ran.stdout
    );
}

/// **Compiling and the sum of honest executables do not count against one.** The
/// job compiles for three seconds and then runs two executables of 1.2 seconds, so
/// the cargo command lasts over five seconds against a two-second deadline, and no
/// executable passes it. **Paired**: the same job with a second executable that
/// does hang is killed, and the verdict names that executable and not the first.
#[test]
fn the_deadline_is_each_executables_and_not_a_cargo_commands() {
    let honest = Setup {
        deadline: "2",
        compile_for: "3",
        executables: "2",
        hang_for: "1.2",
        second_for: "1.2",
    };
    let ran = bar(&scratch(), "check", &honest, None);
    assert!(
        ran.success && timed_out(&ran).is_empty(),
        "an honest job was killed\n{}\n{}",
        ran.stdout,
        ran.log
    );
    assert!(ran.took >= Duration::from_secs(5), "{:?}", ran.took);

    let second_hangs = Setup {
        second_for: "20",
        ..honest
    };
    let ran = bar(&scratch(), "check", &second_hangs, None);
    assert!(!ran.success, "{}", ran.stdout);
    let named = timed_out(&ran);
    assert!(
        named.iter().any(|l| l.contains("beta_2")) && !named.iter().any(|l| l.contains("beta_1")),
        "the verdict does not name the executable that hung\n{}",
        ran.stdout
    );
}

/// **A bundled group keeps the line of the binary that timed out.** The `--bins`
/// job runs two executables one after the other into one log. The first hangs; the
/// second prints more than the marker is long. Writing the marker through another
/// open file made the second binary's output start over it.
#[test]
fn a_timeout_in_a_bundled_group_is_not_written_over_by_the_next_binary() {
    let dir = scratch();
    let root = fs::canonicalize(&dir).expect("canonical scratch");
    support::write_executable(
        &root.join("bin_a"),
        "#!/bin/sh\necho printed-before-the-bundled-hang\nsleep 20\n",
    );
    support::write_executable(
        &root.join("bin_b"),
        "#!/bin/sh\ni=0\nwhile [ $i -lt 300 ]; do echo \"bin_b filler line $i\"; i=$((i+1)); done\n\
         echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'\n",
    );
    let exes: Vec<_> = [("bin_a", "unittests src/main.rs"), ("bin_b", "unittests src/bin/b.rs")]
        .into_iter()
        .map(|(path, describes)| {
            serde_json::json!({"lane": "check", "describes": describes, "path": path, "dir": "."})
        })
        .collect();
    let manifest = root.join("bundle.json");
    fs::write(
        &manifest,
        serde_json::json!({"root": root, "arch": std::env::consts::ARCH, "exes": exes}).to_string(),
    )
    .unwrap();
    let ran = bar(&root, "check", &HANGS, Some(&manifest));
    assert!(!ran.success, "{}", ran.stdout);
    assert!(
        ran.took < Duration::from_secs(15),
        "waited {:?}\n{}",
        ran.took,
        ran.stdout
    );
    assert!(
        timed_out(&ran).iter().any(|l| l.contains("bin_a")),
        "the verdict does not name the bundled binary\n{}",
        ran.stdout
    );
    assert!(
        ran.log.contains("TIMED OUT") && ran.log.contains("printed-before-the-bundled-hang"),
        "the log lost the timeout line or what came before it\n{}",
        ran.log
    );
    assert!(
        ran.log.contains("bin_b filler line 299"),
        "the second binary did not run\n{}",
        ran.log
    );
}
