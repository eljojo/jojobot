//! **The report prints the build cache's hit rate for its own run.** Driven
//! through the real binary with a stand-in sccache that answers the way the
//! real one does (`--show-stats --stats-format json`, shape read from sccache
//! 0.15.0): the first answer is the server's totals before the run, the second
//! the totals after it. The line is the difference, it comes after the verdict,
//! and it never moves the exit code.

use std::fs;
use std::process::Command;

mod support;

const FAKE_CARGO: &str = r#"#!/bin/sh
case " $* " in
  *" --list "*) echo 'some::test::name: test'; exit 0 ;;
esac
case "$1" in
  test) echo "${SCCACHE_SERVER_UDS:-unset}" >> "$(dirname "$0")/child-uds"; echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
  *) exit 0 ;;
esac
"#;

/// The stats an sccache server answers with: only the two maps the bar reads
/// carry numbers, the rest is the shape the real answer has.
fn stats(hits: u64, misses: u64) -> String {
    format!(
        r#"{{"stats":{{"compile_requests":{total},"cache_errors":{{"counts":{{}},"adv_counts":{{}}}},"cache_hits":{{"counts":{{"Rust":{hits}}},"adv_counts":{{"rust":{hits}}}}},"cache_misses":{{"counts":{{"Rust":{misses}}},"adv_counts":{{"rust":{misses}}}}}}},"version":"0.15.0"}}"#,
        total = hits + misses,
    )
}

/// An sccache that answers `before` the first time it is asked and `after`
/// every time after, counting its asks in a file beside it.
fn stand_in(dir: &std::path::Path, before: &str, after: &str) -> std::path::PathBuf {
    let path = dir.join("sccache");
    let script = format!(
        r#"#!/bin/sh
echo "$* uds=${{SCCACHE_SERVER_UDS:-unset}}" >> "$(dirname "$0")/calls"
case " $* " in
  *" --stop-server "*) exit 0 ;;
  *" --show-stats --stats-format json "*) ;;
  *) echo "unexpected arguments: $*" >&2; exit 64 ;;
esac
asked="$(dirname "$0")/asked"
echo x >> "$asked"
if [ "$(wc -l < "$asked")" -eq 1 ]; then
  printf '%s' '{before}'
else
  printf '%s' '{after}'
fi
"#
    );
    support::write_executable(&path, &script);
    path
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-sccache-test-{tag}"
    )));
    fs::create_dir_all(&dir).expect("scratch dir");
    let git = |args: &[&str]| {
        support::status_fenced(Command::new("git").args(args).current_dir(&dir)).expect("git");
    };
    git(&["init", "-q"]);
    git(&["config", "user.email", "scratch@example.com"]);
    git(&["config", "user.name", "scratch"]);
    fs::write(dir.join("file"), "x").unwrap();
    git(&["add", "file"]);
    git(&["commit", "-q", "-m", "base"]);
    git(&["update-ref", "refs/remotes/origin/main", "HEAD"]);
    dir
}

fn run_check(dir: &std::path::Path, sccache: Option<&std::path::Path>) -> (bool, String) {
    let cargo = dir.join("cargo");
    support::write_executable(&cargo, FAKE_CARGO);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .arg("check")
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", &cargo)
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env_remove("JOJOBOT_SCCACHE")
        .env_remove("SCCACHE_SERVER_UDS");
    if let Some(path) = sccache {
        command.env("JOJOBOT_SCCACHE", path);
    }
    let output = support::output_fenced(&mut command).expect("run bar check");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
    )
}

fn sccache_line(stdout: &str) -> &str {
    stdout
        .lines()
        .find(|line| line.starts_with("sccache:"))
        .unwrap_or_else(|| panic!("no sccache line: {stdout}"))
}

#[test]
fn sccache_the_report_prints_the_hit_rate_of_this_runs_compiles_after_the_verdict() {
    let dir = scratch("rate");
    let sccache = stand_in(&dir, &stats(100, 100), &stats(108, 102));
    let (green, stdout) = run_check(&dir, Some(&sccache));
    assert!(green, "{stdout}");
    // 8 hits and 2 misses came during the run: the 100 and 100 before it do not count.
    assert_eq!(
        sccache_line(&stdout),
        "sccache: hit rate 80% (8 hits, 2 misses)",
        "{stdout}"
    );
    let verdict = stdout.find("verdict:").expect(&stdout);
    assert!(verdict < stdout.find("sccache:").unwrap(), "{stdout}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sccache_a_run_with_no_cacheable_compile_says_so_and_names_no_rate() {
    let dir = scratch("none");
    let sccache = stand_in(&dir, &stats(5, 5), &stats(5, 5));
    let (green, stdout) = run_check(&dir, Some(&sccache));
    assert!(green, "{stdout}");
    assert_eq!(
        sccache_line(&stdout),
        "sccache: no cacheable compile in this run",
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sccache_a_host_with_no_sccache_says_the_rate_was_not_read_and_stays_green() {
    let dir = scratch("absent");
    let (green, stdout) = run_check(&dir, None);
    assert!(green, "{stdout}");
    assert_eq!(
        sccache_line(&stdout),
        "sccache: hit rate not read (no sccache on this host)",
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn sccache_that_cannot_answer_does_not_move_the_exit_code() {
    let dir = scratch("broken");
    let broken = dir.join("sccache");
    support::write_executable(&broken, "#!/bin/sh\necho 'not json'\nexit 3\n");
    let (green, stdout) = run_check(&dir, Some(&broken));
    assert!(green, "{stdout}");
    assert!(
        sccache_line(&stdout).starts_with("sccache: hit rate not read"),
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}

fn lines_of(path: std::path::PathBuf) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect()
}

/// **The run counts on a server of its own**, so a compile another run or job
/// makes on the same host is never in its totals: the stats reads and the build
/// children name one socket, and that server is stopped when the run ends.
#[test]
fn sccache_the_run_reads_a_server_its_own_children_compile_on() {
    let dir = scratch("private");
    let sccache = stand_in(&dir, &stats(0, 0), &stats(8, 2));
    let (green, stdout) = run_check(&dir, Some(&sccache));
    assert!(green, "{stdout}");
    let calls = lines_of(dir.join("calls"));
    let socket = calls
        .first()
        .and_then(|call| call.split("uds=").nth(1))
        .unwrap_or_else(|| panic!("no call: {calls:?}"))
        .to_string();
    assert!(socket.ends_with("sccache.sock"), "{calls:?}");
    assert_eq!(calls.len(), 3, "two reads and one stop: {calls:?}");
    assert!(
        calls
            .iter()
            .all(|call| call.ends_with(&format!("uds={socket}"))),
        "every call names the one socket: {calls:?}"
    );
    assert!(calls[2].contains("--stop-server"), "{calls:?}");
    let children = lines_of(dir.join("child-uds"));
    assert!(!children.is_empty(), "no build child ran");
    assert!(
        children.iter().all(|child| *child == socket),
        "a build child compiled on another server: {children:?} against {socket}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A server the environment already names is the job's own** (the CI job
/// wrapper starts one per job), so the run uses it as named and leaves it to
/// the wrapper to stop.
#[test]
fn sccache_a_server_the_job_already_names_is_used_as_named() {
    let dir = scratch("named");
    let sccache = stand_in(&dir, &stats(0, 0), &stats(8, 2));
    let named = dir.join("job.sock");
    let cargo = dir.join("cargo");
    support::write_executable(&cargo, FAKE_CARGO);
    let output = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("check")
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", &cargo)
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("JOJOBOT_SCCACHE", &sccache)
            .env("SCCACHE_SERVER_UDS", &named),
    )
    .expect("run bar check");
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(output.status.success(), "{stdout}");
    let want = format!("uds={}", named.display());
    let calls = lines_of(dir.join("calls"));
    assert_eq!(calls.len(), 2, "two reads and no stop: {calls:?}");
    assert!(calls.iter().all(|call| call.ends_with(&want)), "{calls:?}");
    assert!(
        lines_of(dir.join("child-uds"))
            .iter()
            .all(|child| *child == named.display().to_string()),
        "children keep the job's server"
    );
    assert_eq!(
        sccache_line(&stdout),
        "sccache: hit rate 80% (8 hits, 2 misses)",
        "{stdout}"
    );
    let _ = fs::remove_dir_all(&dir);
}
