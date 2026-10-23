//! **A run of the bar leaves its test history in a directory of its own.** The
//! real `bar` binary runs against a stub `cargo` and a history directory the
//! case names, so the assertions are over the files the bar left. The real
//! history directory is never written.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

mod support;

/// **A fake `cargo` that lists three test binaries.** Every `test` job prints a
/// `test the_<job>_case ...` line and a suite line, after sleeping
/// `FAKE_SLEEP` seconds. With `FAKE_BETA=red` the job named `beta` fails.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --no-run "*)
    echo '  Executable unittests src/lib.rs (/x/deps/jojobot_domain-1)'
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    echo '  Executable tests/beta.rs (/x/deps/beta-3)'
    exit 0 ;;
esac
case "$1" in
  test)
    sleep "${FAKE_SLEEP:-0}"
    job="$(echo "$*" | sed -n 's/.*--test \([a-z]*\).*/\1/p')"
    [ -z "$job" ] && job="other"
    if [ "$FAKE_BETA" = red ] && [ "$job" = beta ]; then
      echo "test the_${job}_case ... FAILED"
      echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 2.50s'
      exit 101
    fi
    echo "test the_${job}_case ... ok"
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.25s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#;
    support::write_executable(&path, script);
    path
}

/// A directory no other case in this process uses.
fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-history"));

    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    dir
}

fn bar(dir: &Path, args: &[&str], env: &[(&str, &str)]) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(args)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", write_cargo(dir))
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env_remove("BAR_JOBS")
        .envs(env.iter().copied());
    command
}

/// The one run directory under the history directory.
fn the_run_directory(dir: &Path) -> PathBuf {
    let runs: Vec<PathBuf> = fs::read_dir(dir.join("history"))
        .expect("the history directory exists")
        .map(|entry| entry.expect("an entry").path())
        .collect();
    assert_eq!(runs.len(), 1, "one run, one directory: {runs:?}");
    runs.into_iter().next().expect("one run")
}

fn file_names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("a directory")
        .map(|entry| {
            entry
                .expect("an entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

/// **A finished check leaves a run directory with a finished header and one file
/// per job.** The header names the level and ends with the time the run ended;
/// each job file opens with the job's own line, then one line per test.
#[test]
fn a_finished_check_leaves_a_header_and_a_file_per_job() {
    let dir = scratch();
    let output = support::output_fenced(&mut bar(&dir, &["check"], &[])).expect("run bar");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let run = the_run_directory(&dir);
    assert_eq!(
        file_names(&run),
        [
            "doc.jsonl",
            "lib-jojobot-domain.jsonl",
            "run.json",
            "test-alpha.jsonl",
            "test-beta.jsonl"
        ],
        "the run directory holds the header and a file per job, and no temporary file"
    );
    let header = fs::read_to_string(run.join("run.json")).expect("the header");
    assert!(header.contains("\"level\":\"check\""), "{header}");
    assert!(
        header.contains("\"ended_at\":\""),
        "a finished run says when: {header}"
    );
    let name = run
        .file_name()
        .expect("a name")
        .to_string_lossy()
        .into_owned();
    assert!(
        header.contains(&format!("\"run_id\":\"{name}\"")),
        "the header names the directory it sits in: {header}"
    );

    let alpha = fs::read_to_string(run.join("test-alpha.jsonl")).expect("the alpha job");
    let lines: Vec<&str> = alpha.lines().collect();
    assert_eq!(lines.len(), 2, "the job line and one test line: {alpha}");
    assert!(lines[0].contains("\"kind\":\"job\""), "{alpha}");
    assert!(lines[0].contains("\"job\":\"test-alpha\""), "{alpha}");
    assert!(lines[0].contains("\"ok\":true"), "{alpha}");
    assert!(lines[0].contains("\"duration_ms\":1250"), "{alpha}");
    assert!(lines[1].contains("\"kind\":\"test\""), "{alpha}");
    assert!(lines[1].contains("\"test\":\"the_alpha_case\""), "{alpha}");
    assert!(lines[1].contains("\"verdict\":\"ok\""), "{alpha}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A red job is recorded as one: its job line says it did not pass and its
/// test line says failed.** The paired positive is the case above.
#[test]
fn a_red_job_is_recorded_as_failed() {
    let dir = scratch();
    let output = support::output_fenced(&mut bar(&dir, &["check"], &[("FAKE_BETA", "red")]))
        .expect("run bar");
    assert!(!output.status.success(), "the red job reddens the bar");

    let beta =
        fs::read_to_string(the_run_directory(&dir).join("test-beta.jsonl")).expect("the beta job");
    assert!(
        beta.lines()
            .next()
            .expect("a job line")
            .contains("\"ok\":false"),
        "{beta}"
    );
    assert!(beta.contains("\"verdict\":\"failed\""), "{beta}");
    assert!(beta.contains("\"duration_ms\":2500"), "{beta}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A rooms run records its rooms under the level `rooms`.**
#[test]
fn a_rooms_run_records_each_room_as_a_job() {
    let dir = scratch();
    let output = support::output_fenced(&mut bar(
        &dir,
        &["rooms", "--test", "alpha", "--test", "beta"],
        &[],
    ))
    .expect("run bar");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );

    let run = the_run_directory(&dir);
    assert_eq!(
        file_names(&run),
        ["run.json", "test-alpha.jsonl", "test-beta.jsonl"]
    );
    let header = fs::read_to_string(run.join("run.json")).expect("the header");
    assert!(header.contains("\"level\":\"rooms\""), "{header}");
    assert!(header.contains("\"ended_at\":\""), "{header}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A run that is killed leaves a header with no end, and only whole job
/// files.** The jobs run one at a time and each takes a second, so the bar is
/// killed with one job written and the others unfinished. A file is written
/// under another name and renamed into place, so what a reader finds under a
/// job's name is all of it.
#[test]
fn a_killed_run_leaves_no_end_and_only_whole_job_files() {
    let dir = scratch();
    let mut child = support::fenced_spawn(
        bar(&dir, &["check"], &[("BAR_JOBS", "1"), ("FAKE_SLEEP", "1")])
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    )
    .expect("start bar");
    let history = dir.join("history");
    let started = std::time::Instant::now();
    let written = loop {
        let found = fs::read_dir(&history)
            .ok()
            .and_then(|runs| runs.flatten().next())
            .map(|run| file_names(&run.path()))
            .filter(|names| names.iter().any(|n| n.ends_with(".jsonl")));
        if let Some(names) = found {
            break names;
        }
        assert!(started.elapsed().as_secs() < 60, "no job file in a minute");
        std::thread::sleep(std::time::Duration::from_millis(50));
    };
    child.kill().expect("kill bar");
    child.wait().expect("reap bar");

    let run = the_run_directory(&dir);
    let header = fs::read_to_string(run.join("run.json")).expect("the header");
    assert!(
        !header.contains("ended_at"),
        "a killed run did not end: {header}"
    );
    let names = file_names(&run);
    assert!(
        names.len() < 5,
        "the run was killed before every job finished: {names:?} (first seen {written:?})"
    );
    for name in names.iter().filter(|n| n.ends_with(".jsonl")) {
        let text = fs::read_to_string(run.join(name)).expect("a job file");
        assert!(text.ends_with('\n'), "{name} is cut short: {text:?}");
        assert!(
            text.starts_with("{\"kind\":\"job\""),
            "{name} has no job line: {text:?}"
        );
    }
    assert!(
        names
            .iter()
            .all(|n| n == "run.json" || n.ends_with(".jsonl")),
        "no temporary file is left under a name a reader could take: {names:?}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A history that cannot be written never changes the verdict.** The history
/// directory is a file, so no run directory can be made under it; the bar still
/// runs, still passes, and says on stderr that nothing was recorded.
#[test]
fn a_history_that_cannot_be_written_does_not_change_the_verdict() {
    let dir = scratch();
    fs::write(dir.join("history"), b"a file where a directory should be").expect("a file");
    let output = support::output_fenced(&mut bar(&dir, &["check"], &[])).expect("run bar");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("history"),
        "the failure is said: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **No case in this crate runs the bar without naming a history directory.**
/// A run that does writes into the real one. The check reads the test files
/// that start the binary. `narrow` records nothing, so the file that starts the
/// bar for nothing else is not held to it.
#[test]
fn every_case_that_starts_the_bar_names_a_history_directory() {
    let tests = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut unnamed: Vec<String> = Vec::new();
    for entry in fs::read_dir(&tests).expect("the tests directory") {
        let path = entry.expect("an entry").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let text = fs::read_to_string(&path).expect("a test file");
        let narrow_only = path
            .file_name()
            .is_some_and(|name| name == "narrow_ignored.rs" || name == "narrow_loop.rs");
        if text.contains("CARGO_BIN_EXE_jojobot-bar")
            && !narrow_only
            && !text.contains("BAR_HISTORY_DIR")
        {
            unnamed.push(
                path.file_name()
                    .expect("a name")
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    assert!(
        unnamed.is_empty(),
        "these start the bar and never name a history directory: {unnamed:?}"
    );
}
