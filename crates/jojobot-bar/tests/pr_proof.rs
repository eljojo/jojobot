//! **`make prepush` skips its serial bar arm only when the forge holds a
//! successful pull-request run of the exact head, and names that run.**
//!
//! `scripts/pr-proof` asks the forge, read-only and with no credential, for the
//! runs of the head commit. It exits 0 and prints the run it relied on only when
//! a run of the pull-request workflow, started by a pull request, on exactly this
//! commit, ended `success` and every job the workflow file defines ended
//! `success` in it. Every other answer exits 1 and says why, and the Makefile
//! then runs the arm in full. A forge that does not answer is one of those.
//!
//! The cases run the tool against a stand-in forge on a local port. A run and a
//! job carry the fields the real forge lists them with (`id`, `workflow_id`,
//! `prettyref`, `commit_sha`, `event`, `status`, `html_url`; a job `run_id`,
//! `name`, `status`), and the arm's steps are read from the real Makefile, so the case moves when a
//! step is added there.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Value, json};

const REPO: &str = "/api/v1/repos/homer/jojobot";
const HEAD: &str = "5bd016dd9bb84bcd03726abab28b71fd61a0dbb2";
const OTHER: &str = "65c9ebcb5e15fe82d91e0064c00611a5ae7ac057";

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn tool() -> PathBuf {
    root().join("scripts/pr-proof")
}

/// The steps of the real bar arm, from the Makefile's `BAR_ARM` line.
fn workflow_jobs() -> Vec<String> {
    arm_of(&std::fs::read_to_string(root().join("Makefile")).expect("the Makefile"))
}

fn arm_of(makefile: &str) -> Vec<String> {
    makefile
        .lines()
        .find_map(|line| line.strip_prefix("BAR_ARM"))
        .map(|rest| {
            rest.trim_start_matches([' ', '=', ':', '?'])
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
        .expect("a BAR_ARM line")
}

/// One run, in the shape the forge lists it.
fn run(id: u64, sha: &str, event: &str, workflow: &str, status: &str) -> Value {
    json!({
        "id": id,
        "title": "A change",
        "workflow_id": workflow,
        "index_in_repo": id - 5,
        "prettyref": "#68",
        "commit_sha": sha,
        "event": event,
        "trigger_event": event,
        "status": status,
        "html_url": format!("http://forge.invalid/homer/jojobot/actions/runs/{}", id - 5),
    })
}

fn pr_run(id: u64, status: &str) -> Value {
    run(id, HEAD, "pull_request", "pull-request.yml", status)
}

/// The jobs of a run, one per step of the arm, each with this status.
fn jobs_of(run_id: u64, status: &str) -> Vec<Value> {
    jobs_named(run_id, &workflow_jobs(), status)
}

fn jobs_named(run_id: u64, names: &[String], status: &str) -> Vec<Value> {
    names
        .iter()
        .enumerate()
        .map(|(n, name)| {
            json!({
                "id": run_id * 10 + n as u64,
                "run_id": run_id,
                "attempt": 1,
                "name": name,
                "needs": null,
                "runs_on": ["nix"],
                "status": status,
            })
        })
        .collect()
}

#[derive(Default)]
struct Forge {
    /// Every run the list answers with, whatever the filter asked.
    runs: Vec<Value>,
    /// The jobs of a run, by run id.
    jobs: Vec<(u64, Vec<Value>)>,
    /// The forge answers 500 to everything.
    down: bool,
}

fn stand(forge: Forge) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            let path = text
                .split_whitespace()
                .nth(1)
                .unwrap_or_default()
                .to_string();
            let (code, body) = if forge.down {
                (500, "the forge is down".to_string())
            } else {
                answer(&forge, &path)
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("{addr}{REPO}")
}

fn answer(forge: &Forge, path: &str) -> (u16, String) {
    let Some(rest) = path.strip_prefix(REPO) else {
        return (404, "404 page not found".into());
    };
    let route = rest.split('?').next().unwrap_or_default();
    if route == "/actions/runs" {
        return (200, json!({ "workflow_runs": forge.runs }).to_string());
    }
    if let Some(id) = route
        .strip_prefix("/actions/runs/")
        .and_then(|r| r.strip_suffix("/jobs"))
    {
        let id: u64 = id.parse().unwrap_or(0);
        let jobs = forge
            .jobs
            .iter()
            .find(|(run, _)| *run == id)
            .map(|(_, jobs)| jobs.clone())
            .unwrap_or_default();
        return (200, json!(jobs).to_string());
    }
    (404, "404 page not found".into())
}

/// The tool for `sha`, with the exit code and both streams.
fn proof(api: Option<&str>, sha: &str) -> (i32, String, String) {
    let mut command = Command::new(tool());
    command.arg(sha).current_dir(root());
    command.env_remove("WAIT_RUN_API");
    command.env_remove("GITHUB_ACTIONS");
    if let Some(api) = api {
        command.env("WAIT_RUN_API", api);
    }
    let ran = command.output().expect("the tool runs");
    (
        ran.status.code().expect("it exits"),
        String::from_utf8_lossy(&ran.stdout).to_string(),
        String::from_utf8_lossy(&ran.stderr).to_string(),
    )
}

fn refused(forge: Forge, why: &str) {
    let api = stand(forge);
    let (code, out, err) = proof(Some(&api), HEAD);
    assert_eq!(code, 1, "{why}: it must not vouch: {out}{err}");
    assert!(
        !out.contains("reusing"),
        "{why}: it named a run it must not rely on: {out}"
    );
}

/// **A successful pull-request run of the exact head, every job green, is
/// reused and named.** The positive every refusal below depends on.
#[test]
fn a_green_pull_request_run_of_the_head_is_reused_and_named() {
    let api = stand(Forge {
        runs: vec![pr_run(997, "success")],
        jobs: vec![(997, jobs_of(997, "success"))],
        ..Default::default()
    });
    let (code, out, err) = proof(Some(&api), HEAD);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("#997"), "it names the run: {out}");
    assert!(
        out.contains("/actions/runs/992"),
        "it gives the run's page: {out}"
    );
    assert!(out.contains(HEAD), "it names the commit: {out}");
    assert_eq!(out.lines().count(), 1, "one line: {out}");
}

#[test]
fn full_dispatch_run_of_the_exact_head_is_reused_and_targeted_is_not() {
    let api = stand(Forge {
        runs: vec![run(
            997,
            HEAD,
            "workflow_dispatch",
            "pull-request.yml",
            "success",
        )],
        jobs: vec![(997, jobs_of(997, "success"))],
        ..Default::default()
    });
    let (code, out, err) = proof(Some(&api), HEAD);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("#997") && out.contains(HEAD), "{out}");
    for (sha, workflow) in [(OTHER, "pull-request.yml"), (HEAD, "targeted.yml")] {
        refused(
            Forge {
                runs: vec![run(998, sha, "workflow_dispatch", workflow, "success")],
                jobs: vec![(998, jobs_of(998, "success"))],
                ..Default::default()
            },
            "a dispatch of another SHA or targeted workflow",
        );
    }
}

/// **A forge with no run of the head gives no proof.**
#[test]
fn no_run_of_the_head_gives_no_proof() {
    refused(Forge::default(), "no run");
}

/// **A failed run gives no proof.**
#[test]
fn a_failed_run_gives_no_proof() {
    refused(
        Forge {
            runs: vec![pr_run(997, "failure")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "failure",
    );
}

/// **A cancelled run gives no proof**, even when every job it listed is green.
#[test]
fn a_cancelled_run_gives_no_proof() {
    refused(
        Forge {
            runs: vec![pr_run(997, "cancelled")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "cancelled",
    );
}

/// **A run that is still going gives no proof.**
#[test]
fn a_running_run_gives_no_proof() {
    refused(
        Forge {
            runs: vec![pr_run(997, "running")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "running",
    );
}

/// **A green run of another commit gives no proof**, even when the forge's list
/// holds it.
#[test]
fn a_run_of_another_commit_gives_no_proof() {
    refused(
        Forge {
            runs: vec![run(
                997,
                OTHER,
                "pull_request",
                "pull-request.yml",
                "success",
            )],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "another commit",
    );
}

/// **A green branch run of the head gives no proof**: only the pull-request
/// workflow runs the suites the arm runs.
#[test]
fn a_green_branch_run_of_the_head_gives_no_proof() {
    refused(
        Forge {
            runs: vec![run(997, HEAD, "push", "branch.yml", "success")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "branch run",
    );
}

/// **The event and the workflow each decide on their own.** A run of the
/// pull-request workflow that a push started, and a run of another workflow that a
/// pull request started, are both turned back; the pair of the case above changed
/// both at once, so either could have been the reason.
#[test]
fn the_event_and_the_workflow_each_refuse_on_their_own() {
    refused(
        Forge {
            runs: vec![run(997, HEAD, "push", "pull-request.yml", "success")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "the pull-request workflow, started by a push",
    );
    refused(
        Forge {
            runs: vec![run(997, HEAD, "pull_request", "branch.yml", "success")],
            jobs: vec![(997, jobs_of(997, "success"))],
            ..Default::default()
        },
        "another workflow, started by a pull request",
    );
}

/// **A job the arm does not name still has to be green.** The run is the
/// aggregate of every job it holds, not only the ones the arm runs. Paired with
/// the same extra job green, which is vouched.
#[test]
fn a_job_outside_the_arm_that_is_not_green_gives_no_proof() {
    let extra = |status: &str| {
        let mut jobs = jobs_of(997, "success");
        jobs.extend(jobs_named(
            997,
            &["a-job-the-arm-does-not-run".to_string()],
            status,
        ));
        // A job id of its own.
        jobs.last_mut().expect("the extra job")["id"] = json!(99_999);
        jobs
    };
    let api = stand(Forge {
        runs: vec![pr_run(997, "success")],
        jobs: vec![(997, extra("success"))],
        ..Default::default()
    });
    let (code, out, err) = proof(Some(&api), HEAD);
    assert_eq!(code, 0, "a green extra job is vouched: {out}{err}");
    for bad in ["failure", "cancelled", "skipped"] {
        refused(
            Forge {
                runs: vec![pr_run(997, "success")],
                jobs: vec![(997, extra(bad))],
                ..Default::default()
            },
            bad,
        );
    }
}

/// **A green run whose job was cancelled, skipped, failed or missing gives no
/// proof.** The run's own status is the aggregate; each job is read too.
#[test]
fn a_run_with_a_job_that_is_not_green_gives_no_proof() {
    for bad in ["cancelled", "skipped", "failure"] {
        let mut jobs = jobs_of(997, "success");
        jobs[1]["status"] = json!(bad);
        refused(
            Forge {
                runs: vec![pr_run(997, "success")],
                jobs: vec![(997, jobs)],
                ..Default::default()
            },
            bad,
        );
    }
    let mut jobs = jobs_of(997, "success");
    jobs.pop();
    refused(
        Forge {
            runs: vec![pr_run(997, "success")],
            jobs: vec![(997, jobs)],
            ..Default::default()
        },
        "a required job missing",
    );
}

/// **A forge that does not answer gives no proof.**
#[test]
fn a_forge_that_does_not_answer_gives_no_proof() {
    refused(
        Forge {
            down: true,
            ..Default::default()
        },
        "down",
    );
    let (code, out, err) = proof(None, HEAD);
    assert_eq!(code, 1, "no forge named: {out}{err}");
    let unreachable = "http://127.0.0.1:1/api/v1/repos/homer/jojobot";
    let (code, out, err) = proof(Some(unreachable), HEAD);
    assert_eq!(code, 1, "nothing listening: {out}{err}");
}

/// **A newer cancelled run does not hide an older green one of the same
/// commit**, and the older one is the one named.
#[test]
fn an_older_green_run_is_reused_past_a_newer_cancelled_one() {
    let api = stand(Forge {
        runs: vec![pr_run(1001, "cancelled"), pr_run(997, "success")],
        jobs: vec![
            (997, jobs_of(997, "success")),
            (1001, jobs_of(1001, "cancelled")),
        ],
        ..Default::default()
    });
    let (code, out, err) = proof(Some(&api), HEAD);
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("#997") && !out.contains("#1001"), "{out}");
}

/// **`make prepush` runs the bar arm in full unless the tool vouches, and
/// always runs the rest.** The real `Makefile` runs with every step stubbed and
/// a stub tool in the scratch directory, the way prepush_order.rs does.
fn prepush_with(tool_exit: i32) -> Vec<String> {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-pr-proof"));

    std::fs::create_dir_all(dir.join("scripts")).expect("scratch dir");
    let log = dir.join("steps.log");
    let stub = dir.join("scripts/pr-proof");
    std::fs::write(
        &stub,
        format!("#!/bin/sh\necho 'reusing run #1'\nexit {tool_exit}\n"),
    )
    .expect("stub tool");
    let mut mode = std::fs::metadata(&stub).expect("stub").permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o755);
    std::fs::set_permissions(&stub, mode).expect("executable");
    let mut makefile = format!("include {}\n", root().join("Makefile").display());
    for name in [
        "check",
        "integration",
        "rooms",
        "year",
        "nix-build",
        "flake-check",
    ] {
        makefile.push_str(&format!("{name}:\n\t@echo {name} >> {}\n", log.display()));
    }
    std::fs::write(dir.join("Makefile"), makefile).expect("scratch Makefile");
    let ran = Command::new("make")
        .current_dir(&dir)
        .args(["-k", "prepush"])
        .output()
        .expect("make runs");
    assert!(
        ran.status.success(),
        "{}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    std::fs::read_to_string(&log)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

#[test]
fn prepush_skips_the_bar_arm_only_when_the_tool_vouches_and_never_the_rest() {
    let vouched = prepush_with(0);
    for kept in ["nix-build", "flake-check"] {
        assert!(
            vouched.iter().any(|s| s == kept),
            "{kept} must run: {vouched:?}"
        );
    }
    for skipped in ["check", "rooms", "year"] {
        assert!(
            !vouched.iter().any(|s| s == skipped),
            "{skipped} must be skipped on a vouched head: {vouched:?}"
        );
    }
    let unvouched = prepush_with(1);
    for ran in ["check", "rooms", "year", "nix-build", "flake-check"] {
        assert!(
            unvouched.iter().any(|s| s == ran),
            "{ran} must run when the tool does not vouch: {unvouched:?}"
        );
    }
    // `check` runs the suites `integration` ran, and no pull-request job is
    // named for it, so a proof could never vouch for it.
    assert!(
        !unvouched.iter().any(|s| s == "integration"),
        "integration is not a step of the arm: {unvouched:?}"
    );
    let missing_tool_ran_all = {
        // No tool at all is the same as no proof.
        let dir_log = prepush_with(127);
        dir_log.len()
    };
    assert_eq!(missing_tool_ran_all, 5);
}

/// A scratch checkout holding a copy of the tool, so the tool reads the scratch
/// `Makefile` and workflow beside it and not this repository's. Returns the tool.
fn checkout_with(arm: &str, workflow_jobs: &[&str]) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-pr-proof-root"));

    std::fs::create_dir_all(dir.join("scripts")).expect("scratch scripts");
    std::fs::create_dir_all(dir.join(".forgejo/workflows")).expect("scratch workflows");
    for name in ["pr-proof", "wait-run"] {
        std::fs::copy(
            root().join("scripts").join(name),
            dir.join("scripts").join(name),
        )
        .expect("copy the tool");
    }
    std::fs::write(dir.join("Makefile"), format!("BAR_ARM = {arm}\n")).expect("Makefile");
    let jobs: String = workflow_jobs
        .iter()
        .map(|j| format!("  {j}:\n    runs-on: nix\n"))
        .collect();
    std::fs::write(
        dir.join(".forgejo/workflows/pull-request.yml"),
        format!("name: pull-request\njobs:\n{jobs}"),
    )
    .expect("workflow");
    dir.join("scripts/pr-proof")
}

fn proof_with(tool: &Path, forge: Forge) -> (i32, String, String) {
    let api = stand(forge);
    let ran = Command::new(tool)
        .arg(HEAD)
        .env("WAIT_RUN_API", api)
        .env_remove("GITHUB_ACTIONS")
        .output()
        .expect("the tool runs");
    (
        ran.status.code().expect("it exits"),
        String::from_utf8_lossy(&ran.stdout).to_string(),
        String::from_utf8_lossy(&ran.stderr).to_string(),
    )
}

fn names(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

/// **The required jobs are the arm's own, so a run that lacks a job the arm names
/// gives no proof, and the refusal names the job.** The workflow in the scratch
/// checkout no longer defines `integration`; the arm still runs it.
#[test]
fn a_run_lacking_a_job_the_arm_names_gives_no_proof_and_names_it() {
    let tool = checkout_with("check integration rooms year", &["check", "rooms", "year"]);
    let (code, out, err) = proof_with(
        &tool,
        Forge {
            runs: vec![pr_run(997, "success")],
            jobs: vec![(
                997,
                jobs_named(997, &names(&["check", "rooms", "year"]), "success"),
            )],
            ..Default::default()
        },
    );
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        err.contains("integration"),
        "it names the missing job: {err}"
    );
    assert!(!out.contains("reusing"), "{out}");
}

/// **A job the workflow defines and the arm does not name is not needed.** The
/// positive that the case above depends on: the arm's list, and not the
/// workflow's, decides.
#[test]
fn a_job_only_the_workflow_defines_is_not_required() {
    let tool = checkout_with("check rooms", &["check", "rooms", "docs"]);
    let (code, out, err) = proof_with(
        &tool,
        Forge {
            runs: vec![pr_run(997, "success")],
            jobs: vec![(997, jobs_named(997, &names(&["check", "rooms"]), "success"))],
            ..Default::default()
        },
    );
    assert_eq!(code, 0, "{out}{err}");
    assert!(out.contains("#997"), "{out}");
}

/// **`make prepush` runs the steps `BAR_ARM` names**, so the list the tool reads
/// and the list the arm runs are one.
#[test]
fn the_arm_runs_the_steps_bar_arm_names() {
    let ran = Command::new("make")
        .current_dir(root())
        .args(["-n", "bar-arm", "BAR_ARM=year check"])
        .output()
        .expect("make runs");
    let text = String::from_utf8_lossy(&ran.stdout).to_string();
    assert!(
        text.contains("-k year check"),
        "the arm is not built from BAR_ARM: {text}"
    );
}
