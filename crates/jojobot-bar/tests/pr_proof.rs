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
//! `name`, `status`), and the pull-request workflow's four jobs are read from
//! the real workflow file, so the case moves when a job is added there.

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

/// The job ids of the real pull-request workflow, in file order.
fn workflow_jobs() -> Vec<String> {
    let yml = std::fs::read_to_string(root().join(".forgejo/workflows/pull-request.yml"))
        .expect("the pull-request workflow");
    let mut in_jobs = false;
    let mut found = Vec::new();
    for line in yml.lines() {
        if line.starts_with("jobs:") {
            in_jobs = true;
        } else if in_jobs && !line.starts_with(' ') && !line.is_empty() && !line.starts_with('#') {
            in_jobs = false;
        } else if in_jobs
            && line.starts_with("  ")
            && !line.starts_with("   ")
            && line.trim_end().ends_with(':')
            && !line.trim_start().starts_with('#')
        {
            found.push(line.trim().trim_end_matches(':').to_string());
        }
    }
    found
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

/// The jobs of a run, one per job of the workflow, each with this status.
fn jobs_of(run_id: u64, status: &str) -> Vec<Value> {
    workflow_jobs()
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
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-pr-proof-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
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
    for skipped in ["check", "integration", "rooms", "year"] {
        assert!(
            !vouched.iter().any(|s| s == skipped),
            "{skipped} must be skipped on a vouched head: {vouched:?}"
        );
    }
    let unvouched = prepush_with(1);
    for ran in [
        "check",
        "integration",
        "rooms",
        "year",
        "nix-build",
        "flake-check",
    ] {
        assert!(
            unvouched.iter().any(|s| s == ran),
            "{ran} must run when the tool does not vouch: {unvouched:?}"
        );
    }
    let missing_tool_ran_all = {
        // No tool at all is the same as no proof.
        let dir_log = prepush_with(127);
        dir_log.len()
    };
    assert_eq!(missing_tool_ran_all, 6);
}
