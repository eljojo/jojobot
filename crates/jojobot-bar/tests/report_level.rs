//! **The report level runs the boundary level less the jobs the history lets
//! go.** The real `bar` binary runs against a stub `cargo` and a history
//! directory the case seeds through the writer the bar itself uses, so the
//! history has the shape a real run leaves. The real history is never read.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_bar::{JobRecord, Run, RunHeader};

mod support;

/// **A fake `cargo` that lists the library group, three test files and the
/// upgrade gate, logs every call, and passes.**
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let script = r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case " $* " in
  *" --no-run "*)
    echo '  Executable unittests src/lib.rs (/x/deps/lib-1)'
    echo '  Executable tests/alpha.rs (/x/deps/alpha-2)'
    echo '  Executable tests/beta.rs (/x/deps/beta-3)'
    echo '  Executable tests/upgrade_gate.rs (/x/deps/gate-4)'
    exit 0 ;;
esac
case "$1" in
  test)
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
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-report"));

    fs::create_dir_all(&dir).expect("scratch dir");
    support::git_init(&dir);
    dir
}

/// **Record one run of one job in the history under `dir`**, as the bar does.
fn seed(dir: &Path, n: usize, job: &[&str], ms: u64) {
    let header = RunHeader {
        run_id: format!("2026100{n}000000-seed-{n}"),
        level: "check".into(),
        checkout: "seed".into(),
        head: Some(format!("head{n}")),
        dirty: Some(false),
        started_at: "2026-10-01T00:00:00Z".into(),
        bar_jobs: 8,
        load_average: Some(1.0),
        cpus: Some(4),
    };
    let root = dir.join("history");
    let run = if root.join(&header.run_id).exists() {
        // Another job of this run was seeded first: the writer begins a run
        // once, so a second job is written into the run that exists.
        Run::begin(&root, header).expect("the run exists already")
    } else {
        Run::begin(&root, header).expect("begin")
    };
    let job: Vec<String> = job.iter().map(|part| part.to_string()).collect();
    let record = JobRecord {
        tests: vec![],
        duration_ms: Some(ms),
    };
    run.record_job(&job, true, &record).expect("seed a job");
}

/// Five runs of the jobs: the slow ones with a median over thirty seconds and
/// five runs, the fast one under it, the library group with four runs only.
fn seed_a_history(dir: &Path) {
    for n in 1..=5 {
        seed(dir, n, &["--test", "alpha"], 40_000);
        seed(dir, n, &["--test", "beta"], 1_000);
        seed(dir, n, &["--test", "upgrade_gate"], 90_000);
        if n <= 4 {
            seed(dir, n, &["--lib"], 60_000);
        }
    }
}

struct Ran {
    stdout: String,
    success: bool,
    jobs: Vec<String>,
}

fn bar(dir: &Path, args: &[&str]) -> Ran {
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(args)
            .current_dir(dir)
            .env("TMPDIR", dir)
            .env("CARGO", write_cargo(dir))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env_remove("BAR_JOBS"),
    )
    .expect("run bar");
    let jobs = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("test ") && !call.contains("--no-run"))
        .map(|call| {
            call.replace("test --workspace ", "")
                .replace(" --no-fail-fast --locked", "")
        })
        .collect();
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        success: out.status.success(),
        jobs,
    }
}

fn sorted(mut jobs: Vec<String>) -> Vec<String> {
    jobs.sort();
    jobs
}

/// The selected jobs can read their plan before they start, including the job
/// left to the boundary. A line printed only with the verdict is too late.
#[test]
fn the_selected_job_plan_is_announced_before_those_jobs_run() {
    for (target, expected) in [("report", "4 of 5"), ("check", "boundary, 5 jobs")] {
        let dir = scratch();
        seed_a_history(&dir);
        let cargo = write_cargo(&dir);
        let script = fs::read_to_string(&cargo).expect("the stub");
        support::write_executable(
            &cargo,
            &script.replace(
                "  test)\n",
                "  test)\n    grep '^level:' \"$BAR_CASE_STDOUT\" >> \"$BAR_CASE_PLAN\" || true\n",
            ),
        );
        let stdout = dir.join("stdout");
        let plan = dir.join("plan-before-tests");
        let output = support::fenced_spawn(
            Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
                .arg(target)
                .current_dir(&dir)
                .env("TMPDIR", &dir)
                .env("CARGO", cargo)
                .env("BAR_HISTORY_DIR", dir.join("history"))
                .env("BAR_CASE_STDOUT", &stdout)
                .env("BAR_CASE_PLAN", &plan)
                .env_remove("BAR_JOBS")
                .stdout(fs::File::create(&stdout).expect("the output file"))
                .stderr(std::process::Stdio::piped()),
        )
        .expect("start bar")
        .wait_with_output()
        .expect("bar ends");
        let printed = fs::read_to_string(&stdout).expect("the bar output");
        assert!(output.status.success(), "{printed}\n{output:?}");
        let calls = fs::read_to_string(dir.join("argv.log")).expect("the jobs ran");
        assert!(calls.contains("test --workspace --test beta"), "{calls}");
        let early = fs::read_to_string(&plan).expect("the jobs inspected the output");
        assert!(early.contains(expected), "{target}: {early:?}\n{printed}");
        if target == "report" {
            assert!(early.contains("test-alpha"), "{early}");
        }
        fs::remove_dir_all(dir).expect("cleanup");
    }
}

/// Planning reads cargo's targets and the bar's history, before compilation or
/// test execution can hide the jobs left to the boundary.
#[test]
fn the_startup_plan_reads_history_without_building_or_running_tests() {
    let dir = scratch();
    seed_a_history(&dir);
    let cargo = dir.join("cargo");
    support::write_executable(
        &cargo,
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
if [ "$1" != metadata ]; then exit 1; fi
cat <<'JSON'
{"workspace_members":["pa"],"packages":[{"id":"pa","name":"pa","targets":[
{"kind":["lib"],"name":"pa","test":true},
{"kind":["test"],"name":"alpha","test":true},
{"kind":["test"],"name":"beta","test":true},
{"kind":["test"],"name":"upgrade_gate","test":true},
{"kind":["test"],"name":"room_year","test":true,"required-features":["rooms"]}
]}],"resolve":{"nodes":[{"id":"pa","features":[]}]}}
JSON
"#,
    );
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .args(["plan", "report"])
            .current_dir(&dir)
            .env("CARGO", cargo)
            .env("BAR_HISTORY_DIR", dir.join("history")),
    )
    .expect("run the plan");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{out:?}");
    let first = text.lines().next().expect("the level is printed");
    assert!(
        first.starts_with("level:") && first.contains("4 of 5"),
        "{text}"
    );
    assert!(first.contains("test-alpha"), "{text}");
    let run = first.split("; run: ").nth(1).unwrap();
    assert!(run.split(", ").any(|job| job == "doc"), "{first}");
    assert!(
        text.contains("test-beta") && text.contains("test-upgrade_gate"),
        "{text}"
    );
    assert!(
        !text.contains("room_year"),
        "a gated room is not a check job: {text}"
    );
    let calls = fs::read_to_string(dir.join("argv.log")).expect("the metadata call");
    assert!(
        calls.lines().all(|call| call.starts_with("metadata ")),
        "{calls}"
    );
    fs::remove_dir_all(dir).expect("cleanup");
}

/// **With no history the report level runs every job the boundary level runs**,
/// and says so. The protecting half: no evidence, no job leaves.
#[test]
fn with_no_history_the_report_level_runs_what_the_boundary_runs() {
    let dir = scratch();
    let boundary = bar(&dir, &["check"]);
    assert!(boundary.success, "{}", boundary.stdout);
    assert_eq!(
        boundary.jobs.len(),
        5,
        "lib, three files and the doc-tests: {:?}",
        boundary.jobs
    );
    let boundary_jobs = sorted(boundary.jobs);

    let other = scratch();
    let report = bar(&other, &["report"]);
    assert!(report.success, "{}", report.stdout);
    assert_eq!(sorted(report.jobs), boundary_jobs);
    assert!(
        report
            .stdout
            .lines()
            .any(|l| l.starts_with("level:") && l.contains("report") && l.contains("5 of 5")),
        "{}",
        report.stdout
    );
    let _ = fs::remove_dir_all(&dir);
    let _ = fs::remove_dir_all(&other);
}

/// **A seeded history moves exactly the jobs the rule names.** The slow job
/// with five clean runs leaves; the fast one, the one with four runs and the
/// upgrade gate (slow, with five clean runs) stay; the doc-tests have no
/// history and stay. The boundary level still runs all five.
#[test]
fn a_seeded_history_moves_exactly_the_jobs_the_rule_names() {
    let dir = scratch();
    seed_a_history(&dir);
    let report = bar(&dir, &["report"]);
    assert!(report.success, "{}", report.stdout);
    let ran = sorted(report.jobs);
    assert_eq!(ran.len(), 4, "one job leaves: {ran:?}");
    assert!(
        !ran.iter().any(|j| j.contains("--test alpha")),
        "alpha left: {ran:?}"
    );
    for stays in ["--lib", "--test beta", "--test upgrade_gate", "--doc"] {
        assert!(
            ran.iter().any(|j| j.contains(stays)),
            "{stays} must stay: {ran:?}"
        );
    }
    let level = report
        .stdout
        .lines()
        .find(|l| l.starts_with("level:"))
        .unwrap_or_else(|| panic!("no level line: {}", report.stdout));
    assert!(
        level.contains("4 of 5") && level.contains("test-alpha"),
        "{level}"
    );

    // The stub logs its calls per directory, and the log holds the report run's.
    fs::remove_file(dir.join("argv.log")).expect("the report run's calls");
    let boundary = bar(&dir, &["check"]);
    assert_eq!(
        boundary.jobs.len(),
        5,
        "the boundary level runs everything: {:?}",
        boundary.jobs
    );
    let level = boundary
        .stdout
        .lines()
        .find(|l| l.starts_with("level:"))
        .unwrap_or_else(|| panic!("no level line: {}", boundary.stdout));
    assert!(
        level.contains("boundary") && level.contains("5 jobs"),
        "{level}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **The wall time is shown on the jobs line**, at both levels.
#[test]
fn the_jobs_line_shows_the_wall_time() {
    let dir = scratch();
    let report = bar(&dir, &["report"]);
    let line = report
        .stdout
        .lines()
        .find(|l| l.starts_with("jobs:"))
        .unwrap_or_else(|| panic!("no jobs line: {}", report.stdout));
    assert!(line.contains("wall"), "{line}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A report run is recorded under its own level**, so its history is told
/// from a boundary run's.
#[test]
fn a_report_run_is_recorded_under_the_level_report() {
    let dir = scratch();
    let report = bar(&dir, &["report"]);
    assert!(report.success, "{}", report.stdout);
    let runs: Vec<PathBuf> = fs::read_dir(dir.join("history"))
        .expect("the history")
        .flatten()
        .map(|e| e.path())
        .collect();
    assert_eq!(runs.len(), 1, "{runs:?}");
    let header = fs::read_to_string(runs[0].join("run.json")).expect("the header");
    assert!(header.contains("\"level\":\"report\""), "{header}");
    let _ = fs::remove_dir_all(&dir);
}

/// **The Makefile has the target a report runs.**
#[test]
fn the_makefile_has_a_report_target() {
    let makefile = fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Makefile"))
        .expect("the Makefile");
    assert!(
        makefile.lines().any(|l| l.starts_with("report:")),
        "no report target"
    );
    assert!(
        makefile.contains("jojobot-bar -- report"),
        "the target does not run the bar's report"
    );
}
