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

#[test]
fn the_startup_branch_banner_names_the_selected_workflow_jobs() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = fs::read_to_string(root.join(".forgejo/workflows/branch.yml")).unwrap();
    let env = text
        .split("\nenv:\n")
        .nth(1)
        .unwrap()
        .split("\njobs:")
        .next()
        .unwrap();
    assert!(
        env.lines().any(|line| line.trim()
            == "BAR_WORKFLOW_JOBS: ${{ forgejo.event.inputs.job || 'report, rooms' }}"),
        "the dispatch selection must supply the banner's job list: {env}"
    );
    for job in text.split("    steps:\n").skip(1) {
        let banner = job.split("      - uses:").next().unwrap();
        assert!(banner.contains("$BAR_WORKFLOW_JOBS"), "{banner}");
    }
}

#[test]
fn the_startup_branch_banner_names_omitted_work_for_each_selection() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = fs::read_to_string(root.join(".forgejo/workflows/branch.yml")).unwrap();
    let left = text
        .lines()
        .find_map(|line| line.strip_prefix("  BAR_BOUNDARY_LEFT: "))
        .expect("the selected jobs must also decide what is left");
    let omitted = |job| {
        let condition = format!("forgejo.event.inputs.job == '{job}' && '");
        left.split(&condition)
            .nth(1)
            .unwrap()
            .split('\'')
            .next()
            .unwrap()
    };
    let both = left
        .rsplit("|| '")
        .next()
        .unwrap()
        .split('\'')
        .next()
        .unwrap();
    for (selection, words, report_left, rooms_left) in [
        ("report", omitted("report"), false, true),
        ("rooms", omitted("rooms"), true, false),
        ("default", both, false, false),
    ] {
        assert_eq!(
            words.contains("report"),
            report_left,
            "{selection}: {words}"
        );
        assert_eq!(words.contains("rooms"), rooms_left, "{selection}: {words}");
        assert!(words.contains("year"), "{selection}: {words}");
    }
    for job in text.split("    steps:\n").skip(1) {
        let banner = job.split("      - uses:").next().unwrap();
        assert!(banner.contains("$BAR_BOUNDARY_LEFT"), "{banner}");
    }
}

#[test]
fn the_startup_workflow_banner_precedes_checkout_and_its_plan_follows_checkout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for workflow in ["branch", "pull-request", "targeted"] {
        let text = fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml")))
            .expect("the workflow");
        for (index, job) in text.split("    steps:\n").skip(1).enumerate() {
            let starts: Vec<_> = job.match_indices("      - ").map(|(at, _)| at).collect();
            let steps: Vec<_> = starts
                .iter()
                .enumerate()
                .map(|(n, at)| &job[*at..starts.get(n + 1).copied().unwrap_or(job.len())])
                .collect();
            assert!(steps[0].contains("level:"), "{workflow}: {}", steps[0]);
            let checkout = steps
                .iter()
                .position(|s| s.contains("/checkout@"))
                .expect("the checkout step");
            assert!(checkout > 0, "{workflow}: checkout precedes the banner");
            let static_gate = matches!(workflow, "branch" | "pull-request") && index == 0;
            assert!(
                if static_gate {
                    steps[checkout + 1].contains("make static")
                } else {
                    steps[checkout + 1].contains(" plan ")
                        || (!job.contains("id: plan")
                            && (steps[checkout + 1].contains("make nix-build")
                                || steps[checkout + 1].contains("make flake-check")))
                },
                "{workflow}: first step after checkout is {}",
                steps[checkout + 1]
            );
        }
    }
}

fn workflow_job(text: &str, name: &str) -> String {
    let marker = format!("\n  {name}:\n");
    text.split_once(&marker)
        .unwrap_or_else(|| panic!("missing {name} job"))
        .1
        .lines()
        .take_while(|line| line.is_empty() || line.starts_with("    ") || line.starts_with("  #"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn workflow_step(job: &str, name: &str) -> String {
    job.split_once(&format!("      - name: {name}\n        run: |\n"))
        .expect("named shell step")
        .1
        .lines()
        .take_while(|line| line.starts_with("          "))
        .map(|line| &line[10..])
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn the_startup_static_gate_stops_expensive_jobs_before_they_start() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for (workflow, dependents) in [
        ("branch", &["report", "rooms"][..]),
        ("pull-request", &["build", "nix-build", "flake-check"][..]),
    ] {
        let text =
            fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml"))).unwrap();
        let gate = workflow_job(&text, "static");
        let first_job = text
            .split_once("\njobs:\n")
            .unwrap()
            .1
            .lines()
            .find(|line| {
                line.starts_with("  ") && !line.starts_with("    ") && !line.starts_with("  #")
            });
        assert_eq!(first_job, Some("  static:"), "{workflow}");
        assert!(!gate.contains("id: plan"), "{workflow}: {gate}");
        let cheap = gate.find("make static").expect("the build-free gate");
        let oracle = gate
            .find("name: Served text snapshot")
            .expect("the existing oracle");
        assert!(cheap < oracle, "{workflow}: {gate}");
        assert!(!gate[..cheap].contains("cargo test"), "{workflow}: {gate}");
        for name in dependents {
            let job = workflow_job(&text, name);
            assert!(
                job.lines().any(|line| line == "    needs: static"),
                "{workflow}/{name}: {job}"
            );
        }
        if workflow == "pull-request" {
            for name in ["check", "rooms", "year"] {
                assert!(workflow_job(&text, name).contains("    needs: [static, build]"));
            }
        }
        let dir = scratch();
        support::write_executable(&dir.join("nix"), "#!/bin/sh\nshift 2\nexec \"$@\"\n");
        support::write_executable(
            &dir.join("make"),
            "#!/bin/sh\necho \"fmt-check: $STATIC_STATE\"\ntest \"$STATIC_STATE\" = ok\n",
        );
        for (state, success) in [("ok", true), ("FAILED", false)] {
            let out = support::output_fenced(
                Command::new("bash")
                    .args(["-euc", &workflow_step(&gate, "Build-free gates")])
                    .current_dir(&dir)
                    .env("STATIC_STATE", state)
                    .env(
                        "PATH",
                        format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
                    ),
            )
            .unwrap();
            assert_eq!(out.status.success(), success, "{workflow}/{state}: {out:?}");
            assert!(
                String::from_utf8_lossy(&out.stdout).contains(&format!("fmt-check: {state}")),
                "the job must print its refusal: {out:?}"
            );
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn the_startup_snapshot_step_refuses_zero_cases_and_keeps_a_red_oracle_red() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut same_step = None;
    for workflow in ["branch", "pull-request"] {
        let text =
            fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml"))).unwrap();
        let gate = workflow_job(&text, "static");
        let step = workflow_step(&gate, "Served text snapshot");
        if let Some(previous) = &same_step {
            assert_eq!(
                previous, &step,
                "the two workflows must run the same oracle"
            );
        }
        same_step = Some(step.clone());
        let dir = scratch();
        support::write_executable(&dir.join("nix"), "#!/bin/sh\nshift 2\nexec \"$@\"\n");
        support::write_executable(
            &dir.join("cargo"),
            r#"#!/bin/sh
echo "$SNAPSHOT_UPDATE $*" >> argv.log
case " $* " in
  *" --list "*)
    if [ "$SNAPSHOT_MODE" != empty ]; then
      echo 'snapshot::every_text_a_model_reads_matches_the_snapshot: test'
    fi
    exit 0 ;;
esac
test "$SNAPSHOT_MODE" != red
"#,
        );
        for (mode, success) in [("same", true), ("empty", false), ("red", false)] {
            fs::write(dir.join("argv.log"), "").unwrap();
            let out = support::output_fenced(
                Command::new("bash")
                    .args(["-euc", &step])
                    .current_dir(&dir)
                    .env("SNAPSHOT_MODE", mode)
                    .env("SNAPSHOT_UPDATE", "1")
                    .env(
                        "PATH",
                        format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()),
                    ),
            )
            .unwrap();
            assert_eq!(out.status.success(), success, "{workflow}/{mode}: {out:?}");
            let calls = fs::read_to_string(dir.join("argv.log")).unwrap();
            assert!(calls.lines().all(|line| line.starts_with("0 test --locked -p jojobot-mcp --lib snapshot::every_text_a_model_reads_matches_the_snapshot -- --exact")), "{calls}");
            assert_eq!(
                calls.lines().count(),
                if mode == "empty" { 1 } else { 2 },
                "{calls}"
            );
            if mode == "empty" {
                assert!(
                    String::from_utf8_lossy(&out.stdout).contains("snapshot case was not listed"),
                    "{out:?}"
                );
            }
        }
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn ci_static_jobs_have_a_bounded_timeout() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for workflow in ["branch", "pull-request"] {
        let text =
            fs::read_to_string(root.join(format!(".forgejo/workflows/{workflow}.yml"))).unwrap();
        assert!(
            workflow_job(&text, "static")
                .lines()
                .any(|line| line == "    timeout-minutes: 120")
        );
    }
}

#[test]
fn ci_required_statuses_fail_when_static_did_not_succeed() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = fs::read_to_string(root.join(".forgejo/workflows/pull-request.yml")).unwrap();
    let dir = scratch();
    for name in ["check", "rooms", "year"] {
        let job = workflow_job(&text, name);
        assert!(
            job.lines()
                .any(|line| line.starts_with("    if: ${{ always()")),
            "{name}: {job}"
        );
        assert!(job.contains("    needs: [static, build]"), "{job}");
        let step = workflow_step(&job, "Level");
        assert!(step.contains("${{ needs.static.result }}"), "{step}");
        assert!(step.contains("${{ needs.build.result }}"), "{step}");
        for (static_result, build_result, success) in [
            ("success", "success", true),
            ("failure", "skipped", false),
            ("skipped", "skipped", false),
            ("cancelled", "skipped", false),
            ("success", "failure", false),
        ] {
            let rendered = step
                .replace("${{ needs.static.result }}", static_result)
                .replace("${{ needs.build.result }}", build_result);
            let out = support::output_fenced(
                Command::new("bash")
                    .args(["-euc", &rendered])
                    .current_dir(&dir)
                    .env("BAR_WORKFLOW_JOBS", "static, build, check, rooms, year"),
            )
            .unwrap();
            assert_eq!(out.status.success(), success, "{name}: {out:?}");
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(text.starts_with("level:"), "{text}");
            if static_result != "success" {
                assert!(text.contains("static prerequisite"), "{text}");
            }
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn full_dispatch_uses_the_pull_request_boundary_jobs_and_a_separate_group() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = fs::read_to_string(root.join(".forgejo/workflows/pull-request.yml")).unwrap();
    assert!(
        text.split("concurrency:")
            .next()
            .unwrap()
            .contains("  workflow_dispatch:"),
        "{text}"
    );
    let group = text
        .lines()
        .find(|line| line.trim().starts_with("group:"))
        .unwrap();
    assert!(
        group.contains("forgejo.event_name") && group.contains("forgejo.ref"),
        "{group}"
    );
    for command in [
        "make bundle CI_ROOT=",
        "make check CI_ROOT=",
        "make rooms CI_ROOT=",
        "make year CI_ROOT=",
    ] {
        assert!(text.contains(command), "missing {command}");
    }
    // Checkout's default is the dispatched ref/SHA. A second input must not
    // run a different tree while the forge records the event's SHA as proof.
    assert!(!text.contains("          ref:"), "{text}");
}

#[test]
fn targeted_dispatch_requires_a_crate_and_one_filter_and_never_full_proof() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = fs::read_to_string(root.join(".forgejo/workflows/targeted.yml"))
        .expect("targeted workflow");
    assert!(
        text.contains("  workflow_dispatch:")
            && !text.contains("  push:")
            && !text.contains("  pull_request:"),
        "{text}"
    );
    for input in ["crate", "FILTER"] {
        let heading = format!("      {input}:");
        let required = text
            .lines()
            .skip_while(|line| *line != heading)
            .skip(1)
            .take_while(|line| line.starts_with("        "))
            .any(|line| line.trim() == "required: true");
        assert!(required, "{input}: {text}");
    }
    assert!(
        text.contains("-- plan targeted \"$CRATE\" \"$FILTER\""),
        "{text}"
    );
    assert!(
        text.contains("make narrow CRATE=\"$CRATE\" FILTER=\"$FILTER\""),
        "{text}"
    );
    assert!(
        !text.contains("make report")
            && !text.contains("make nix-build")
            && !text.contains("make flake-check"),
        "{text}"
    );
    let group = text
        .lines()
        .find(|s| s.trim().starts_with("group:"))
        .unwrap();
    assert!(
        group.contains("forgejo.ref")
            && group.contains("inputs.crate")
            && group.contains("inputs.FILTER"),
        "{group}"
    );
}

#[test]
fn targeted_dispatch_plan_refuses_empty_or_multiple_filters() {
    let dir = scratch();
    for (krate, filter, success) in [
        ("jojobot-adapters", "dolt_case", true),
        ("jojobot-adapters", "", false),
        ("jojobot-adapters", "one two", false),
        ("../outside", "case", false),
        ("jojobot-adapters", "$(command)", false),
    ] {
        let out =
            support::output_fenced(&mut bar(&dir, &["plan", "targeted", krate, filter])).unwrap();
        assert_eq!(out.status.success(), success, "{krate}, {filter}: {out:?}");
        if success {
            let text = String::from_utf8_lossy(&out.stdout);
            assert!(
                text.starts_with("level: boundary, targeted") && text.contains(filter),
                "{text}"
            );
        }
    }
    fs::remove_dir_all(dir).unwrap();
}

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
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-start-lines"));

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
