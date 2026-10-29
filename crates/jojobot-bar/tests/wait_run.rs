//! **`scripts/wait-run` waits for the run of a commit and prints only its
//! verdict.**
//!
//! The tool reads the Forgejo actions API for the repository, finds the run for a
//! commit (a branch resolves to the head the forge holds), polls until the run
//! ends, and prints the run, its ref and commit and the bar's verdict block from
//! the report job's log. Its exit code is the run's: 0 for success, 1 for a
//! failure, 2 for a run that was cancelled, 3 when no run ends in time and 4 for
//! a branch or a commit that names nothing.
//!
//! Every case runs it against a stand-in forge on a local port. The shapes the
//! stand-in answers with are the ones the real forge answers with (a run carries
//! `id`, `prettyref`, `commit_sha`, `status` and `html_url`, a job list is a bare
//! array, a log is plain text with a timestamp on every line), and the two job
//! logs are cut from real runs of the report job: one red, one green.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

/// A code proof licenses only the docs/ratchet fast path. Release readiness
/// remains the release dispatch's verdict, including the static gate.
fn code_forge() -> Forge {
    let mut candidate = run(997, "#68", GREEN_SHA, "success");
    candidate["workflow_id"] = json!("pull-request.yml");
    candidate["event"] = json!("pull_request");
    Forge {
        runs: vec![(candidate, String::new())],
        job_name: Some("observer"),
        more_jobs: ["static", "build", "check", "rooms", "year"]
            .into_iter()
            .map(|name| (997, name, "success", String::new()))
            .collect(),
        ..Default::default()
    }
}

#[test]
fn code_proof_accepts_exact_pr_and_dispatch_with_only_release_jobs_skipped() {
    for event in ["pull_request", "workflow_dispatch"] {
        for status in ["success", "skipped"] {
            let mut forge = code_forge();
            forge.runs[0].0["event"] = json!(if event == "workflow_dispatch" {
                ""
            } else {
                event
            });
            forge.runs[0].0["trigger_event"] = json!(event);
            for name in ["nix-build", "flake-check"] {
                forge.more_jobs.push((997, name, status, String::new()));
            }
            let standing = stand(forge);
            let (code, out, err) = wait(&standing, &[GREEN_SHA, "--code-proof"]);
            assert_eq!(code, 0, "{out}{err}");
            assert!(
                out.contains("#995") && out.contains(GREEN_SHA) && out.contains("/runs/995"),
                "{out}"
            );
            assert_eq!(standing.asked_for("/actions/runs?"), 1);
            assert_eq!(standing.asked_for("/logs"), 0);
        }
    }
}

#[test]
fn code_proof_refuses_unproved_metadata_missing_or_bad_jobs_and_outages_without_waiting() {
    for (key, value) in [
        ("commit_sha", RED_SHA),
        ("workflow_id", "branch.yml"),
        ("workflow_id", "targeted.yml"),
        ("event", "push"),
        ("status", "failure"),
        ("status", "cancelled"),
        ("status", "running"),
        ("status", "skipped"),
    ] {
        let mut forge = code_forge();
        forge.runs[0].0[key] = json!(value);
        forge.runs[0].0["trigger_event"] = json!("push");
        forge.ignore_head_sha = key == "commit_sha";
        let standing = stand(forge);
        let (code, out, err) = wait(&standing, &[GREEN_SHA, "--code-proof"]);
        assert_eq!(code, 1, "{key}={value}: {out}{err}");
        assert_eq!(standing.asked_for("/actions/runs?"), 1);
        assert_eq!(standing.asked_for(&format!("head_sha={GREEN_SHA}")), 1);
    }
    for name in [
        "static",
        "build",
        "check",
        "rooms",
        "year",
        "nix-build",
        "flake-check",
        "observer",
    ] {
        for status in ["failure", "cancelled", "running", "skipped", "missing"] {
            if ["nix-build", "flake-check"].contains(&name)
                && ["skipped", "missing"].contains(&status)
            {
                continue;
            }
            let mut forge = code_forge();
            forge.more_jobs.retain(|(_, n, _, _)| *n != name);
            if name == "observer" {
                forge.job_name = Some("extra");
            }
            if status != "missing" {
                forge.more_jobs.push((997, name, status, String::new()));
            } else if name == "observer" {
                continue;
            }
            let standing = stand(forge);
            let (code, out, err) = wait(&standing, &[GREEN_SHA, "--code-proof"]);
            assert_eq!(code, 1, "{name}={status}: {out}{err}");
        }
    }
    for forge in [
        Forge::default(),
        Forge {
            down_for: 20,
            ..Default::default()
        },
    ] {
        let standing = stand(forge);
        let (code, out, err) = wait(&standing, &[GREEN_SHA, "--code-proof"]);
        assert_eq!(code, 1, "{out}{err}");
        assert_eq!(standing.asked_for("/actions/runs?"), 1);
    }
    let no_api = Command::new(tool())
        .args([GREEN_SHA, "--code-proof"])
        .env_remove("WAIT_RUN_API")
        .env_remove("GITHUB_ACTIONS")
        .output()
        .unwrap();
    assert_eq!(no_api.status.code(), Some(1), "{no_api:?}");
}

#[test]
fn code_proof_uses_an_older_green_run_and_leaves_the_checkout_clean() {
    let mut forge = code_forge();
    let mut newer = forge.runs[0].0.clone();
    newer["id"] = json!(1001);
    newer["status"] = json!("cancelled");
    forge.runs.insert(0, (newer, String::new()));
    let standing = stand(forge);
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-code-proof"));
    std::fs::create_dir_all(dir.join("scripts")).unwrap();
    std::fs::copy(tool(), dir.join("scripts/wait-run")).unwrap();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(args)
            .current_dir(&dir)
            .output()
            .unwrap();
        assert!(out.status.success(), "{out:?}");
        String::from_utf8(out.stdout).unwrap()
    };
    git(&["init", "-q"]);
    git(&["config", "core.excludesFile", "/dev/null"]);
    git(&["add", "scripts/wait-run"]);
    git(&[
        "-c",
        "user.name=scratch",
        "-c",
        "user.email=scratch@example.com",
        "commit",
        "-qm",
        "base",
    ]);
    let out = Command::new(dir.join("scripts/wait-run"))
        .args([GREEN_SHA, "--code-proof"])
        .current_dir(&dir)
        .env("WAIT_RUN_API", format!("{}{REPO}", standing.addr))
        .env_remove("PYTHONDONTWRITEBYTECODE")
        .env_remove("PYTHONPYCACHEPREFIX")
        .output()
        .unwrap();
    assert!(out.status.success(), "{out:?}");
    let text = String::from_utf8_lossy(&out.stdout);
    // The forge id rides beside the number, so a pick of the newer cancelled run
    // (number 999, forge id 1001) cannot print the older run's number.
    assert!(
        text.contains("#995 (forge id 997)") && !text.contains("forge id 1001"),
        "{text}"
    );
    assert!(git(&["status", "--porcelain", "--untracked-files=all"]).is_empty());
    std::fs::remove_dir_all(dir).unwrap();
}

const REPO: &str = "/api/v1/repos/homer/jojobot";
const GREEN_SHA: &str = "5bd016dd9bb84bcd03726abab28b71fd61a0dbb2";
const RED_SHA: &str = "65c9ebcb5e15fe82d91e0064c00611a5ae7ac057";

fn tool() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/wait-run")
}

fn fixture(name: &str) -> String {
    std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name),
    )
    .expect("a fixture")
}

/// One run, in the shape the forge lists it.
fn run(id: u64, branch: &str, sha: &str, status: &str) -> Value {
    json!({
        "id": id,
        "title": "A change",
        "workflow_id": "branch.yml",
        "index_in_repo": id - 2,
        "prettyref": branch,
        "commit_sha": sha,
        "event": "push",
        "trigger_event": "push",
        "status": status,
        "html_url": format!("http://forge.invalid/homer/jojobot/actions/runs/{}", id - 2),
    })
}

/// What the stand-in forge holds, and how often each path was asked.
#[derive(Default)]
struct Forge {
    /// Every run, newest first, as `(run, job log)`.
    runs: Vec<(Value, String)>,
    /// Branches the forge holds: name and head.
    branches: Vec<(&'static str, &'static str)>,
    /// A run that reads `running` for this many asks of the list, then its own status.
    running_for: usize,
    /// The forge answers 500 to this many asks before it answers anything.
    down_for: usize,
    /// Return a foreign run despite the requested head, so the reader must check it.
    ignore_head_sha: bool,
    /// The name of the job each run has; `report` when none.
    job_name: Option<&'static str>,
    /// The primary job's log has been removed by the forge.
    gone_log: bool,
    /// Jobs beyond the first of a run: the run's id, the job's name and
    /// status, and its log.
    more_jobs: Vec<(u64, &'static str, &'static str, String)>,
    /// The forge answers 500 to the list that names no head: the read that looks
    /// for a newer run on the same ref.
    fails_the_unfiltered_list: bool,
}

struct Standing {
    addr: String,
    asked: Arc<Mutex<Vec<String>>>,
}

impl Standing {
    fn asked_for(&self, part: &str) -> usize {
        self.asked
            .lock()
            .expect("the log")
            .iter()
            .filter(|path| path.contains(part))
            .count()
    }
}

/// A stand-in forge on a free local port, answering until the test ends.
fn stand(forge: Forge) -> Standing {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = format!("http://{}", listener.local_addr().expect("an address"));
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = asked.clone();
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
            log.lock().expect("the log").push(path.clone());
            let lists = log
                .lock()
                .expect("the log")
                .iter()
                .filter(|p| p.contains("/actions/runs?"))
                .count();
            let asks = log.lock().expect("the log").len();
            let (code, body) = if asks <= forge.down_for {
                (500, "the forge is down".to_string())
            } else {
                answer(&forge, &path, lists)
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    Standing { addr, asked }
}

fn answer(forge: &Forge, path: &str, lists: usize) -> (u16, String) {
    let Some(rest) = path.strip_prefix(REPO) else {
        return (404, "404 page not found".into());
    };
    let query = |key: &str| {
        rest.split_once('?').and_then(|(_, q)| {
            q.split('&')
                .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
        })
    };
    if let Some(name) = rest.strip_prefix("/branches/") {
        return match forge.branches.iter().find(|(held, _)| *held == name) {
            Some((name, head)) => (
                200,
                json!({"name": name, "commit": {"id": head}}).to_string(),
            ),
            None => (404, "404 page not found".into()),
        };
    }
    if rest.starts_with("/actions/runs?") {
        if forge.fails_the_unfiltered_list && query("head_sha").is_none() {
            return (500, "the forge is down".to_string());
        }
        // A second page is always empty: nothing here holds more than a page.
        let on_page = query("page").is_none_or(|page| page == "1");
        let listed: Vec<Value> = forge
            .runs
            .iter()
            .map(|(run, _)| {
                let mut run = run.clone();
                if lists <= forge.running_for {
                    run["status"] = json!("running");
                }
                run
            })
            .filter(|run| {
                forge.ignore_head_sha
                    || query("head_sha").is_none_or(|sha| run["commit_sha"] == sha)
            })
            .filter(|_| on_page)
            .collect();
        return (
            200,
            json!({"total_count": listed.len(), "workflow_runs": listed}).to_string(),
        );
    }
    if let Some(id) = rest
        .strip_prefix("/actions/runs/")
        .and_then(|r| r.strip_suffix("/jobs"))
    {
        let run_id = id.parse::<u64>().unwrap_or(0);
        let status = forge
            .runs
            .iter()
            .find(|(run, _)| run["id"] == run_id)
            .map_or("success", |(run, _)| {
                run["status"].as_str().unwrap_or("success")
            });
        let mut jobs = vec![json!({"id": 1000 + run_id, "run_id": run_id,
                    "name": forge.job_name.unwrap_or("report"), "status": status})];
        for (index, (of_run, name, job_status, _)) in forge.more_jobs.iter().enumerate() {
            if *of_run == run_id {
                jobs.push(json!({"id": 9000 + index, "run_id": run_id,
                    "name": name, "status": job_status}));
            }
        }
        return (200, json!(jobs).to_string());
    }
    if let Some(job) = rest
        .strip_prefix("/actions/jobs/")
        .and_then(|r| r.strip_suffix("/logs"))
    {
        let job: u64 = job.parse().unwrap_or(0);
        if forge.gone_log && job < 9000 {
            return (404, "404 page not found".into());
        }
        // The forge keeps no log for a job that never ran: a skipped job's log is a 404.
        if job >= 9000 {
            return match forge.more_jobs.get((job - 9000) as usize) {
                // The forge keeps no log for a job that never ran, whatever its status.
                Some((_, _, status, log)) if *status == "skipped" || log.is_empty() => {
                    (404, "404 page not found".into())
                }
                Some((_, _, _, log)) => (200, log.clone()),
                None => (404, "404 page not found".into()),
            };
        }
        let id = job.saturating_sub(1000);
        return match forge.runs.iter().find(|(run, _)| run["id"] == id) {
            Some((run, _)) if run["status"] == "skipped" => (404, "404 page not found".into()),
            Some((_, log)) => (200, log.clone()),
            None => (404, "404 page not found".into()),
        };
    }
    (404, "404 page not found".into())
}

/// The tool, run against a stand-in forge, with the exit code and both streams.
fn wait(standing: &Standing, args: &[&str]) -> (i32, String, String) {
    let ran = Command::new(tool())
        .args(args)
        .args(["--interval", "0.05", "--timeout", "5"])
        .env("WAIT_RUN_API", format!("{}{REPO}", standing.addr))
        .output()
        .expect("the tool runs");
    (
        ran.status.code().expect("it exits"),
        String::from_utf8_lossy(&ran.stdout).to_string(),
        String::from_utf8_lossy(&ran.stderr).to_string(),
    )
}

fn two_runs_of_one_commit() -> Forge {
    Forge {
        runs: vec![
            (
                run(225, "dev9-2374r-work", GREEN_SHA, "success"),
                fixture("wait-run-green-job.txt"),
            ),
            (
                run(219, "dev9-2374-work", GREEN_SHA, "failure"),
                fixture("wait-run-red-job.txt"),
            ),
            (
                run(227, "dev9-2403-work", RED_SHA, "failure"),
                fixture("wait-run-red-job.txt"),
            ),
        ],
        branches: vec![
            ("dev9-2374r-work", GREEN_SHA),
            ("dev9-2374-work", GREEN_SHA),
            ("dev9-2403-work", RED_SHA),
        ],
        ..Default::default()
    }
}

/// **A branch finds its own run, even when another branch holds the same
/// commit and failed.** The output is the run, its ref and commit and the verdict
/// block, and nothing else of the log: not what the bar printed before the
/// verdict, not what the job printed after the block, and no timestamps.
#[test]
fn a_green_branch_run_prints_its_verdict_block_and_exits_zero() {
    let forge = stand(two_runs_of_one_commit());
    let (code, out, err) = wait(&forge, &["dev9-2374r-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    for printed in [
        "run #223",
        "dev9-2374r-work",
        GREEN_SHA,
        "verdict: GREEN",
        "size: ok — 57 files over 2000 lines, 57 listed in size-ratchet.toml",
        "test: ok — 94 suites, 717 passed",
        "log: target/bar/report.log",
    ] {
        assert!(out.contains(printed), "{printed:?} is missing: {out}");
    }
    for left_out in ["bar: report is expected", "2026-10-15T0"] {
        assert!(
            !out.contains(left_out),
            "{left_out:?} is in the output: {out}"
        );
    }
    assert!(
        out.contains("#217"),
        "the other run of the commit is named, not used: {out}"
    );
}

/// **A branch finds its own run when a newer run of the same commit exists on
/// another branch.** The newer one succeeded; this branch's own run failed, and
/// the exit code is the branch's.
#[test]
fn a_branch_finds_its_own_run_when_a_newer_run_of_its_commit_succeeded() {
    let forge = stand(two_runs_of_one_commit());
    let (code, out, err) = wait(&forge, &["dev9-2374-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("run #217"), "{out}");
    assert!(out.contains("also on this commit: #223"), "{out}");
}

/// **A commit names the newest run it has, and says there are others.** Its exit
/// code is that run's, so the same commit read by branch and by commit can differ.
#[test]
fn a_commit_names_its_newest_run_and_says_what_else_holds_it() {
    let forge = stand(two_runs_of_one_commit());
    let (code, out, err) = wait(&forge, &[GREEN_SHA]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("run #223"), "{out}");
    assert!(
        out.contains("dev9-2374-work"),
        "the other ref is named: {out}"
    );
}

#[test]
fn full_dispatch_can_be_read_by_sha_beside_a_newer_targeted_run() {
    let mut full = run(225, "dev-sol-ci-need-work", GREEN_SHA, "failure");
    full["workflow_id"] = json!("pull-request.yml");
    full["event"] = json!("workflow_dispatch");
    let mut targeted = run(229, "dev-sol-ci-need-work", GREEN_SHA, "success");
    targeted["workflow_id"] = json!("targeted.yml");
    let mut other = run(231, "dev-sol-ci-need-work", RED_SHA, "success");
    other["workflow_id"] = json!("pull-request.yml");
    let forge = stand(Forge {
        runs: vec![
            (other, fixture("wait-run-green-job.txt")),
            (targeted, fixture("wait-run-green-job.txt")),
            (full, fixture("wait-run-red-job.txt")),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &[GREEN_SHA, "--workflow", "pull-request.yml"]);
    assert_eq!(code, 1, "{out}{err}");
    assert!(
        out.starts_with("run #223 ") && out.contains(GREEN_SHA),
        "{out}"
    );
}

/// **A dispatched run is found by its workflow's name, with or without `.yml`.**
/// The shape is the one the forge lists for a dispatched prepush run: `event` is
/// empty, `trigger_event` is `workflow_dispatch`, and `workflow_id` is the file
/// name. The paired rows are the file name, which always found it, the bare name a
/// caller types, and a bare name that belongs to another workflow.
#[test]
fn a_dispatched_run_is_found_by_its_workflow_name_with_or_without_yml() {
    let mut dispatched = run(2169, "carry-alpha", GREEN_SHA, "success");
    dispatched["workflow_id"] = json!("prepush.yml");
    dispatched["event"] = json!("");
    dispatched["trigger_event"] = json!("workflow_dispatch");
    let forge = stand(Forge {
        runs: vec![(dispatched, fixture("wait-run-green-job.txt"))],
        ..Default::default()
    });
    for (asked, code) in [("prepush.yml", 0), ("prepush", 0), ("pull-request", 3)] {
        let (got, out, err) = wait(&forge, &[GREEN_SHA, "--workflow", asked]);
        assert_eq!(got, code, "--workflow {asked}: {out}{err}");
    }
}

/// **A red run exits one and names what failed**, by a full commit and by the
/// first eight characters of it.
#[test]
fn a_red_run_exits_one_and_names_the_failed_test() {
    let forge = stand(two_runs_of_one_commit());
    for target in [RED_SHA, &RED_SHA[..8]] {
        let (code, out, err) = wait(&forge, &[target]);
        assert_eq!(code, 1, "{target}: {out}\n{err}");
        for printed in [
            "run #225",
            "dev9-2403-work",
            RED_SHA,
            "verdict: RED",
            "FAILED: every_handle_in_the_workspace_is_on_the_fictional_roster",
        ] {
            assert!(
                out.contains(printed),
                "{target}: {printed:?} is missing: {out}"
            );
        }
        for size in [
            "size: FAILED — 1 offence(s)",
            "memory.rs: grew from 7601 to 7602 lines",
        ] {
            assert!(
                out.contains(size),
                "{target}: the size failure is in the block, {size:?} is missing: {out}"
            );
        }
    }
}

/// **A run that is still going is waited for**, and the verdict comes when it
/// ends. The list is asked again until the status is final.
#[test]
fn a_run_that_is_still_going_is_waited_for() {
    let forge = stand(Forge {
        running_for: 3,
        ..two_runs_of_one_commit()
    });
    let (code, out, err) = wait(&forge, &["dev9-2374r-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("verdict: GREEN"), "{out}");
    assert!(
        forge.asked_for("/actions/runs?") >= 4,
        "it asked until the run ended: {:?}",
        forge.asked.lock().expect("the log")
    );
}

/// **Every place the tool names a run gives its run number first and the forge's
/// database id after it, each labelled.** The forge keeps two numbers for a run:
/// the run's number in the repository, which ends the page address, and a database
/// id that is not it. A line copies the first into `ci_run`. The fixture runs here
/// have ids two above their numbers, as the forge's own do not coincide either.
/// The five places: the pending line, the final line, the other runs of the same
/// commit, the superseder, and the code proof.
#[test]
fn a_run_is_named_by_its_run_number_with_the_forge_id_after_it() {
    let still = stand(Forge {
        running_for: 2,
        ..two_runs_of_one_commit()
    });
    let (code, out, err) = wait(&still, &["dev9-2374r-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        err.contains("run #223 (forge id 225) is running"),
        "the pending line names the number and the id: {err}"
    );
    assert!(
        out.starts_with("run #223 (forge id 225) "),
        "the final line: {out}"
    );
    assert!(
        out.contains("also on this commit: #217 (forge id 219) "),
        "the other runs of the commit: {out}"
    );
    assert!(
        !out.contains("run #225"),
        "the database id is never called the run number: {out}"
    );

    let log = "2026-10-31T04:00:00.0Z started\n".to_string();
    let replaced = stand(Forge {
        runs: vec![
            (pull_request_run(240, 105, RED_SHA, "running"), log.clone()),
            (pull_request_run(238, 105, GREEN_SHA, "cancelled"), log),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&replaced, &[GREEN_SHA]);
    assert_eq!(code, 6, "{out}\n{err}");
    assert!(
        out.contains(&format!(
            "superseded by run #238 (forge id 240) on {RED_SHA}"
        )),
        "{out}"
    );

    let proof = stand(code_forge());
    let (code, out, err) = wait(&proof, &[GREEN_SHA, "--code-proof"]);
    assert_eq!(code, 0, "{out}{err}");
    assert!(
        out.contains("code proof: run #995 (forge id 997) "),
        "{out}"
    );
}

/// **A run record with no run number is named by its forge id alone, and says
/// so.** The tool never fills the run number with the database id. **Paired**
/// with the case above, where the record carries the number.
#[test]
fn a_record_without_a_run_number_is_never_given_the_forge_id_as_one() {
    let mut record = run(231, "dev9-2999-work", GREEN_SHA, "success");
    record.as_object_mut().unwrap().remove("index_in_repo");
    let forge = stand(Forge {
        runs: vec![(record, fixture("wait-run-green-job.txt"))],
        branches: vec![("dev9-2999-work", GREEN_SHA)],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev9-2999-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        out.starts_with("run (forge id 231; the record carries no run number) "),
        "{out}"
    );
    assert!(!out.contains("run #"), "{out}");
}

/// **A cancelled run exits two**, and says its log holds no verdict.
#[test]
fn a_cancelled_run_exits_two() {
    let forge = stand(Forge {
        runs: vec![(
            run(230, "dev4-2413-work", GREEN_SHA, "cancelled"),
            "2026-10-15T02:00:00.0Z started\n".into(),
        )],
        branches: vec![("dev4-2413-work", GREEN_SHA)],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev4-2413-work"]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(
        out.contains("run #228") && out.contains("cancelled"),
        "{out}"
    );
    assert!(out.contains("no verdict block"), "{out}");
}

/// A pull request run, in the shape the forge lists it: the ref reads `#N`.
fn pull_request_run(id: u64, number: u64, sha: &str, status: &str) -> Value {
    let mut run = run(id, &format!("#{number}"), sha, status);
    run["workflow_id"] = json!("pull-request.yml");
    run["event"] = json!("pull_request");
    run["trigger_event"] = json!("pull_request");
    run
}

/// **A cancelled run that a newer push to its pull request superseded says so,
/// and exits six.** The newest such run is named with its commit. Without this a
/// line cannot tell a superseded run from a red trunk. **Paired**: a newer run on
/// another pull request does not supersede it, and a cancelled run with no newer
/// run on its ref exits two and says none was found.
#[test]
fn a_cancelled_run_a_newer_push_superseded_names_it_and_exits_six() {
    let log = "2026-10-31T04:00:00.0Z started\n";
    let forge = stand(Forge {
        runs: vec![
            (
                pull_request_run(240, 105, RED_SHA, "running"),
                log.to_string(),
            ),
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &[GREEN_SHA]);
    assert_eq!(code, 6, "{out}\n{err}");
    assert!(
        out.contains(&format!(
            "superseded by run #238 (forge id 240) on {RED_SHA}"
        )),
        "{out}"
    );

    let other = stand(Forge {
        runs: vec![
            (
                pull_request_run(241, 106, RED_SHA, "running"),
                log.to_string(),
            ),
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&other, &[GREEN_SHA]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(
        out.contains("cancelled; no newer run found"),
        "a run on another pull request was named as the superseder: {out}"
    );
    assert!(!out.contains("superseded by"), "{out}");
}

/// **Each clause of the superseder is held by a case of its own.** A newer run
/// of ANOTHER workflow on the same ref, and an OLDER run of the same workflow,
/// do not supersede; of two newer runs the newest is named, whatever order the
/// forge lists them in. Every refusal is read beside the positive: the newest
/// run is named in the last case, so the first two cannot pass on a tool that
/// names nobody.
#[test]
fn only_a_newer_run_of_the_same_workflow_on_the_same_ref_supersedes_and_the_newest_is_named() {
    let log = "2026-10-31T04:00:00.0Z started\n";
    let mut other_workflow = pull_request_run(240, 105, RED_SHA, "running");
    other_workflow["workflow_id"] = json!("release.yml");
    let different_workflow = stand(Forge {
        runs: vec![
            (other_workflow, log.to_string()),
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&different_workflow, &[GREEN_SHA]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(out.contains("cancelled; no newer run found"), "{out}");

    let older = stand(Forge {
        runs: vec![
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
            (
                pull_request_run(230, 105, RED_SHA, "success"),
                log.to_string(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&older, &[GREEN_SHA]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(out.contains("cancelled; no newer run found"), "{out}");

    // The older of the two newer runs is listed first, so a tool that takes the
    // first match names 239.
    let two_newer = stand(Forge {
        runs: vec![
            (
                pull_request_run(239, 105, RED_SHA, "running"),
                log.to_string(),
            ),
            (
                pull_request_run(240, 105, RED_SHA, "running"),
                log.to_string(),
            ),
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&two_newer, &[GREEN_SHA]);
    assert_eq!(code, 6, "{out}\n{err}");
    assert!(
        out.contains("superseded by run #238 (forge id 240)"),
        "{out}"
    );
}

/// **A read of the forge that failed does not say that no newer run was found.**
/// The failure is not an absence: the line says the forge did not answer, and the
/// exit stays two. **Paired** with the case above, where the same list answers
/// and the absence is said.
#[test]
fn a_failed_read_for_a_newer_run_says_it_failed_and_not_that_none_was_found() {
    let log = "2026-10-31T04:00:00.0Z started\n";
    let forge = stand(Forge {
        runs: vec![
            (
                pull_request_run(240, 105, RED_SHA, "running"),
                log.to_string(),
            ),
            (
                pull_request_run(238, 105, GREEN_SHA, "cancelled"),
                log.to_string(),
            ),
        ],
        fails_the_unfiltered_list: true,
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &[GREEN_SHA]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(err.contains("did not say what replaced it"), "{err}");
    assert!(out.contains("did not say whether a newer run"), "{out}");
    assert!(!out.contains("no newer run found"), "{out}");
}

/// **A skipped run is not superseded.** It exits two like a cancelled one, but
/// nothing cancelled it, so a newer run on its ref does not name a superseder.
#[test]
fn a_skipped_run_with_a_newer_run_on_its_ref_is_not_called_superseded() {
    let forge = stand(Forge {
        runs: vec![
            (
                pull_request_run(240, 105, RED_SHA, "running"),
                String::new(),
            ),
            (
                pull_request_run(238, 105, GREEN_SHA, "skipped"),
                String::new(),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &[GREEN_SHA]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(!out.contains("superseded by"), "{out}");
}

/// **A red job wins over a newer run.** The verdict block says red, and the run
/// exits one, whether or not a newer push exists.
#[test]
fn a_red_job_in_a_superseded_run_still_exits_one() {
    let forge = stand(Forge {
        runs: vec![
            (
                pull_request_run(240, 105, GREEN_SHA, "running"),
                String::new(),
            ),
            (
                pull_request_run(238, 105, RED_SHA, "cancelled"),
                fixture("wait-run-rooms-red-job.txt"),
            ),
        ],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &[RED_SHA]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("verdict: RED"), "{out}");
    assert!(!out.contains("superseded by"), "{out}");
}

/// **A commit with no run in time exits three**, saying what it waited for.
#[test]
fn a_commit_with_no_run_in_time_exits_three() {
    let forge = stand(Forge {
        runs: Vec::new(),
        branches: vec![("dev4-2413-work", GREEN_SHA)],
        ..Default::default()
    });
    let ran = Command::new(tool())
        .args(["dev4-2413-work", "--interval", "0.05", "--timeout", "0.5"])
        .env("WAIT_RUN_API", format!("{}{REPO}", forge.addr))
        .output()
        .expect("the tool runs");
    assert_eq!(ran.status.code(), Some(3));
    let err = String::from_utf8_lossy(&ran.stderr);
    assert!(
        err.contains(GREEN_SHA),
        "it says which commit it waited for: {err}"
    );
}

/// **A branch the forge does not hold exits four**, and says to push it.
#[test]
fn a_branch_the_forge_does_not_hold_exits_four() {
    let forge = stand(two_runs_of_one_commit());
    let (code, _, err) = wait(&forge, &["no-such-branch"]);
    assert_eq!(code, 4, "{err}");
    assert!(
        err.contains("no-such-branch") && err.contains("push"),
        "{err}"
    );
}

/// **The bar's own rendering is what the tool reads.** The job log here is made of
/// what the bar prints, rendered by the bar, between lines the job prints before
/// and after it, so a change to the bar's first or last line of the block is a
/// change this case sees. The block comes out whole.
#[test]
fn the_bar_s_own_block_comes_out_whole() {
    let mut summary = jojobot_bar::Summary::new();
    summary.phase_ok("build", "compiled");
    summary.phase_failed("test", "3 suites, 1 failed");
    summary.log("target/bar/report.log", 12);
    let block = summary.render();
    let stamped: String = block
        .lines()
        .map(|line| format!("2026-10-15T02:36:34.1Z {line}\n"))
        .collect();
    let log = format!(
        "2026-10-15T02:30:00.0Z noise before\n{stamped}2026-10-15T02:36:35.0Z noise after\n"
    );
    let forge = stand(Forge {
        runs: vec![(run(240, "dev4-2413-work", GREEN_SHA, "failure"), log)],
        branches: vec![("dev4-2413-work", GREEN_SHA)],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev4-2413-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(
        out.contains(block.trim_end()),
        "the whole block is printed: {out}"
    );
    assert!(!out.contains("noise"), "{out}");
}

/// **A cancelled run exits two**, and a run with no complete block in its log says
/// so rather than guessing where a block ends. A skipped run exits two as well, and
/// has no log to say anything about.
#[test]
fn a_run_that_did_not_finish_says_its_log_holds_no_block() {
    for status in ["cancelled", "skipped"] {
        let forge = stand(Forge {
            runs: vec![(
                run(230, "dev4-2413-work", GREEN_SHA, status),
                "2026-10-15T02:00:00.0Z verdict: RED\n2026-10-15T02:00:01.0Z test: FAILED\n".into(),
            )],
            branches: vec![("dev4-2413-work", GREEN_SHA)],
            ..Default::default()
        });
        let (code, out, err) = wait(&forge, &["dev4-2413-work"]);
        assert_eq!(code, 2, "{status}: {out}\n{err}");
        if status == "cancelled" {
            assert!(
                out.contains("no verdict block"),
                "{status}: a verdict with no closing line is no block: {out}"
            );
        }
    }
}

/// **A skipped job has no log, and the exit code is still the run's verdict.** A
/// dispatch that names one job leaves the other skipped, and the forge answers 404
/// to the log of a job that never ran. The skipped job is named with its status
/// and no log is asked for.
#[test]
fn a_skipped_job_beside_a_green_one_is_named_and_the_run_exits_zero() {
    let forge = stand(Forge {
        runs: vec![(
            run(251, "dev9-2647-work", GREEN_SHA, "success"),
            fixture("wait-run-green-job.txt"),
        )],
        branches: vec![("dev9-2647-work", GREEN_SHA)],
        more_jobs: vec![(251, "rooms", "skipped", String::new())],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("job rooms  skipped"), "{out}");
    assert!(out.contains("verdict: GREEN"), "{out}");
    assert!(!err.contains("Traceback"), "{err}");
    assert_eq!(
        forge.asked_for("/actions/jobs/9000/logs"),
        0,
        "a skipped job's log was asked for"
    );
}

/// **A skipped job beside a red one changes nothing about the red one.** The exit
/// code is one, the red block is printed, the skipped job is named, and no log is
/// asked for it.
#[test]
fn a_skipped_job_beside_a_red_one_is_named_and_the_run_exits_one() {
    let forge = stand(Forge {
        runs: vec![(
            run(253, "dev9-2647-work", RED_SHA, "failure"),
            fixture("wait-run-rooms-red-job.txt"),
        )],
        branches: vec![("dev9-2647-work", RED_SHA)],
        more_jobs: vec![(253, "rooms", "skipped", String::new())],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("verdict: RED"), "{out}");
    assert!(out.contains("job rooms  skipped"), "{out}");
    assert_eq!(
        forge.asked_for("/actions/jobs/9000/logs"),
        0,
        "a skipped job's log was asked for"
    );
}

/// **A red job makes the run exit one, whatever the run's own status.** The sibling
/// watcher cancels the run when a job goes red, so the run of a red report job reads
/// `cancelled` and its other job read cancelled with no log. The printed block says
/// red, and the exit code says so too, not two.
#[test]
fn a_red_job_in_a_cancelled_run_exits_one() {
    let forge = stand(Forge {
        runs: vec![(
            run(254, "dev9-2647-work", RED_SHA, "cancelled"),
            fixture("wait-run-rooms-red-job.txt"),
        )],
        branches: vec![("dev9-2647-work", RED_SHA)],
        more_jobs: vec![(254, "rooms", "cancelled", String::new())],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("verdict: RED"), "{out}");
    assert!(out.contains("job rooms  cancelled"), "{out}");
}

/// **A job with no log is named whatever its status.** A job cancelled before it
/// started has no log either, and the forge answers 404 to it. The run keeps the
/// exit code of its own status, here two for a cancelled run, and the other job's
/// block still prints.
#[test]
fn a_job_cancelled_before_it_started_has_no_log_and_the_run_exits_two() {
    let forge = stand(Forge {
        runs: vec![(
            run(252, "dev9-2647-work", GREEN_SHA, "cancelled"),
            fixture("wait-run-green-job.txt"),
        )],
        branches: vec![("dev9-2647-work", GREEN_SHA)],
        more_jobs: vec![(252, "rooms", "cancelled", String::new())],
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(out.contains("job rooms  cancelled"), "{out}");
    assert!(out.contains("no log of the rooms job"), "{out}");
    assert!(out.contains("verdict: GREEN"), "{out}");
    assert!(!err.contains("Traceback"), "{err}");
}

/// **Every job of the run prints, whatever its name.** A job called `build`
/// carries a block like any other, and the line above it names the job and its
/// status.
#[test]
fn a_job_prints_its_block_under_its_own_name() {
    let forge = stand(Forge {
        runs: vec![(
            run(231, "dev4-2413-work", GREEN_SHA, "success"),
            fixture("wait-run-green-job.txt"),
        )],
        branches: vec![("dev4-2413-work", GREEN_SHA)],
        job_name: Some("build"),
        ..Default::default()
    });
    let (code, out, err) = wait(&forge, &["dev4-2413-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("job build  success"), "{out}");
    assert!(out.contains("verdict: GREEN"), "{out}");
}

/// A run of the shape a branch push makes: the report job and the rooms job.
fn two_jobs(report: &str, rooms: &str, status: &str) -> Forge {
    Forge {
        runs: vec![(run(250, "dev9-2647-work", RED_SHA, status), fixture(report))],
        branches: vec![("dev9-2647-work", RED_SHA)],
        more_jobs: vec![(250, "rooms", status_of(rooms), fixture(rooms))],
        ..Default::default()
    }
}

/// The status the forge gives a job whose log is this fixture.
fn status_of(log: &str) -> &'static str {
    if log == "wait-run-green-job.txt" {
        "success"
    } else {
        "failure"
    }
}

/// **A run with a green job and a red job prints both blocks and exits one.**
/// The red block is the rooms job's, cut from a real run whose rooms job failed
/// two tests while its report job passed; the line `verdict: RED` in
/// `wait-run-rooms-red-job.txt` is what makes the rooms job red.
#[test]
fn a_green_job_and_a_red_job_both_print_and_the_run_exits_one() {
    let forge = stand(two_jobs(
        "wait-run-green-job.txt",
        "wait-run-rooms-red-job.txt",
        "failure",
    ));
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    for printed in [
        "job report  failure",
        "verdict: GREEN",
        "job rooms  failure",
        "verdict: RED",
        "FAILED: every_check_fails_on_a_room_nobody_worked_in",
        "FAILED: every_check_holds_for_a_route_nobody_predicted",
    ] {
        assert!(out.contains(printed), "{printed:?} is missing: {out}");
    }
}

/// **A job the runner killed prints RUNNER DIED with the line that says so, and
/// a run with no red job exits five.** Each fixture is a cut of a real log of a
/// job the runner lost, and the line that makes the classifier fire is quoted:
/// `RUN signal: terminated` (a runner restart), `context canceled` (the step
/// cancelled after it), `failed to stat working directory` (the runner's work
/// directory gone) and `No such file or directory (os error 2)` (the bar lost
/// its own directory) before any verdict.
#[test]
fn a_job_the_runner_killed_prints_runner_died_and_the_run_exits_five() {
    for (fixture_name, line) in [
        (
            "wait-run-runner-terminated-job.txt",
            "[runner]: RUN signal: terminated",
        ),
        (
            "wait-run-runner-canceled-job.txt",
            "[runner]: context canceled",
        ),
        (
            "wait-run-runner-stat-job.txt",
            "failed to stat working directory",
        ),
        (
            "wait-run-runner-enoent-job.txt",
            "bar: No such file or directory (os error 2)",
        ),
    ] {
        let forge = stand(two_jobs(fixture_name, fixture_name, "failure"));
        let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
        assert_eq!(code, 5, "{fixture_name}: {out}\n{err}");
        for job in ["report", "rooms"] {
            let died = out
                .lines()
                .skip_while(|l| !l.starts_with(&format!("job {job} ")))
                .find(|l| l.starts_with("RUNNER DIED"))
                .unwrap_or_else(|| panic!("{fixture_name}: {job} has no RUNNER DIED: {out}"));
            assert!(
                died.contains(line),
                "{fixture_name}: the line quoted is {died:?}"
            );
        }
        assert!(!out.contains("no verdict block"), "{fixture_name}: {out}");
    }
}

/// **A red job outranks a dead one.** A test that failed is the news, and the
/// dead job is named beside it.
#[test]
fn a_red_job_beside_a_dead_one_exits_one_and_names_both() {
    let forge = stand(two_jobs(
        "wait-run-rooms-red-job.txt",
        "wait-run-runner-terminated-job.txt",
        "failure",
    ));
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 1, "{out}\n{err}");
    assert!(out.contains("verdict: RED"), "{out}");
    assert!(out.contains("RUNNER DIED"), "{out}");
}

/// **A run whose jobs are all green exits zero and prints every block.**
#[test]
fn a_run_of_two_green_jobs_exits_zero() {
    let forge = stand(two_jobs(
        "wait-run-green-job.txt",
        "wait-run-green-job.txt",
        "success",
    ));
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(out.matches("verdict: GREEN").count(), 2, "{out}");
    assert!(!out.contains("RUNNER DIED"), "{out}");
}

/// **A cancelled run stays at exit two even when its logs say the runner
/// cancelled the step.** Cancelling is the run's status, not a runner death.
#[test]
fn a_cancelled_run_whose_log_says_context_canceled_still_exits_two() {
    let forge = stand(two_jobs(
        "wait-run-runner-canceled-job.txt",
        "wait-run-runner-canceled-job.txt",
        "cancelled",
    ));
    let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
    assert_eq!(code, 2, "{out}\n{err}");
}

#[test]
fn a_gone_job_log_keeps_the_run_exit_and_reads_the_other_job() {
    for (status, expected) in [("cancelled", 2), ("success", 0)] {
        let forge = stand(Forge {
            gone_log: true,
            ..two_jobs("wait-run-green-job.txt", "wait-run-green-job.txt", status)
        });
        let (code, out, err) = wait(&forge, &["dev9-2647-work"]);
        assert_eq!(code, expected, "{out}\n{err}");
        assert!(out.contains("no log of the report job"), "{out}");
        assert_eq!(out.matches("verdict: GREEN").count(), 1, "{out}");
        assert!(forge.asked_for("/actions/jobs/9000/logs") > 0, "{out}");
        assert!(!err.contains("Traceback"), "{err}");
    }
}

/// **A forge that does not answer is waited out.** The first asks fail with a 500,
/// and the tool asks again until it is answered.
#[test]
fn a_forge_that_is_down_for_a_while_is_waited_out() {
    let forge = stand(Forge {
        down_for: 4,
        ..two_runs_of_one_commit()
    });
    let (code, out, err) = wait(&forge, &["dev9-2374r-work"]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(err.contains("did not answer"), "{err}");
}

/// **The start of a commit that two commits share is refused**, and so is one that
/// no run and no commit of the checkout starts with.
#[test]
fn a_short_commit_is_refused_when_it_is_two_commits_or_none() {
    let forge = stand(Forge {
        runs: vec![
            (
                run(
                    250,
                    "dev4-a-work",
                    "aaaaaaa1aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "success",
                ),
                String::new(),
            ),
            (
                run(
                    251,
                    "dev4-b-work",
                    "aaaaaaa2aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                    "success",
                ),
                String::new(),
            ),
        ],
        ..Default::default()
    });
    let (code, _, err) = wait(&forge, &["aaaaaaa"]);
    assert_eq!(code, 4, "{err}");
    assert!(err.contains("more than one commit"), "{err}");
    let nowhere = scratch("no-repository");
    let ran = Command::new(tool())
        .args(["bbbbbbb", "--interval", "0.05", "--timeout", "1"])
        .env("WAIT_RUN_API", format!("{}{REPO}", forge.addr))
        .current_dir(&nowhere)
        .output()
        .expect("the tool runs");
    assert_eq!(ran.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&ran.stderr).contains("bbbbbbb"));
}

/// **A commit the checkout knows and no run has yet is waited for.** The start of
/// it is read in the checkout when no run carries it, and the wait times out
/// naming the whole commit.
#[test]
fn a_commit_of_the_checkout_with_no_run_yet_is_waited_for() {
    let repository = scratch("known-commit");
    let git = |args: &[&str]| {
        let ran = Command::new("git")
            .args(args)
            .current_dir(&repository)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_AUTHOR_NAME", "Case")
            .env("GIT_AUTHOR_EMAIL", "case@example.invalid")
            .env("GIT_COMMITTER_NAME", "Case")
            .env("GIT_COMMITTER_EMAIL", "case@example.invalid")
            .output()
            .expect("git runs");
        assert!(ran.status.success(), "{args:?}");
        String::from_utf8_lossy(&ran.stdout).trim().to_string()
    };
    git(&["init", "-b", "main"]);
    git(&["commit", "--allow-empty", "-m", "a commit"]);
    let whole = git(&["rev-parse", "HEAD"]);
    let forge = stand(Forge::default());
    let ran = Command::new(tool())
        .args([&whole[..9], "--interval", "0.05", "--timeout", "0.5"])
        .env("WAIT_RUN_API", format!("{}{REPO}", forge.addr))
        .current_dir(&repository)
        .output()
        .expect("the tool runs");
    assert_eq!(ran.status.code(), Some(3));
    assert!(String::from_utf8_lossy(&ran.stderr).contains(&whole));
}

/// **A call with no target is a usage error and exits four**, not the two argparse
/// would give, which is a cancelled run's.
#[test]
fn a_call_with_no_target_exits_four() {
    let ran = Command::new(tool()).output().expect("the tool runs");
    assert_eq!(ran.status.code(), Some(4));
}

/// **The default wait outlasts the longest job a workflow allows.** A report or
/// rooms job may run as long as its `timeout-minutes`, and the run waits in the
/// queue first, so a default shorter than that bound plus the queue makes a healthy
/// run read as "no run ended" (exit 3) and as a stuck run. The default is read from
/// the tool's own help, the bound from the branch workflow.
#[test]
fn the_default_wait_outlasts_the_longest_job_the_branch_workflow_allows() {
    let help = Command::new(tool()).arg("--help").output().expect("help");
    let help = String::from_utf8_lossy(&help.stdout).into_owned();
    let default: f64 = help
        .split("(default ")
        .nth(1)
        .and_then(|rest| rest.split(')').next())
        .and_then(|seconds| seconds.trim().parse().ok())
        .unwrap_or_else(|| panic!("the help names no default wait: {help}"));
    let workflow = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.forgejo/workflows/branch.yml"),
    )
    .expect("the branch workflow");
    let longest = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("timeout-minutes:"))
        .filter_map(|minutes| minutes.trim().parse::<f64>().ok())
        .fold(0.0, f64::max);
    assert!(longest > 0.0, "the branch workflow bounds no job");
    let queue = 30.0 * 60.0;
    assert!(
        default >= longest * 60.0 + queue,
        "the default wait of {default} s is under the longest job bound of {longest} minutes plus a queue"
    );
}

/// The tool with no API root and no runner context in its environment, however
/// the case runs: a case that itself runs on a runner holds the runner's variables.
fn bare(target: &str, extra: &[(&str, &str)]) -> Command {
    let mut command = Command::new(tool());
    command.args([target, "--interval", "0.05", "--timeout", "2"]);
    for held in [
        "WAIT_RUN_API",
        "GITHUB_ACTIONS",
        "GITHUB_API_URL",
        "GITHUB_REPOSITORY",
    ] {
        command.env_remove(held);
    }
    command.envs(extra.iter().copied());
    command
}

/// **The tool names no forge of its own.** With no API root set, or an empty one,
/// and no runner context, it exits four and names the variable, before it asks
/// anything. A runner's variables count only on a runner (`GITHUB_ACTIONS` true)
/// and only whole (the API URL and the repository both). The proxy points at a
/// closed local port, so a tool that went looking for a forge could reach none and
/// would wait out its timeout instead of answering at once.
#[test]
fn a_call_with_no_api_root_exits_four_and_names_the_variable() {
    let api = "http://127.0.0.1:9/api/v1";
    let cases: [(&str, &[(&str, &str)]); 5] = [
        ("unset", &[]),
        ("empty", &[("WAIT_RUN_API", "")]),
        (
            "runner variables on a host that is no runner",
            &[
                ("GITHUB_API_URL", api),
                ("GITHUB_REPOSITORY", "homer/jojobot"),
            ],
        ),
        (
            "a runner with no repository",
            &[("GITHUB_ACTIONS", "true"), ("GITHUB_API_URL", api)],
        ),
        (
            "a runner with no API URL",
            &[
                ("GITHUB_ACTIONS", "true"),
                ("GITHUB_REPOSITORY", "homer/jojobot"),
            ],
        ),
    ];
    for (what, extra) in cases {
        let ran = bare("a-branch", extra)
            .env("http_proxy", "http://127.0.0.1:9")
            .output()
            .expect("the tool runs");
        let said = String::from_utf8_lossy(&ran.stderr);
        assert_eq!(ran.status.code(), Some(4), "{what}: {said}");
        assert!(said.contains("WAIT_RUN_API"), "{what}: {said}");
        assert!(ran.stdout.is_empty(), "{what}");
    }
}

/// **On a runner the root comes from the runner's own context**: the API URL and
/// the repository the runner sets, joined the way the forge's API is laid out.
/// No host is written in the tool.
#[test]
fn on_a_runner_the_api_root_is_built_from_the_runner_context() {
    let forge = stand(two_runs_of_one_commit());
    let api = format!("{}/api/v1", forge.addr);
    let ran = bare(
        "dev9-2374r-work",
        &[
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_API_URL", &api),
            ("GITHUB_REPOSITORY", "homer/jojobot"),
        ],
    )
    .output()
    .expect("the tool runs");
    let said = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(0), "{said}");
    assert!(String::from_utf8_lossy(&ran.stdout).contains("run #223"));
}

/// **A root that is set wins over the runner's context.**
#[test]
fn a_set_api_root_wins_over_the_runner_context() {
    let forge = stand(two_runs_of_one_commit());
    let root = format!("{}{REPO}", forge.addr);
    let ran = bare(
        "dev9-2374r-work",
        &[
            ("WAIT_RUN_API", &root),
            ("GITHUB_ACTIONS", "true"),
            ("GITHUB_API_URL", "http://127.0.0.1:9/api/v1"),
            ("GITHUB_REPOSITORY", "marge/elsewhere"),
        ],
    )
    .output()
    .expect("the tool runs");
    let said = String::from_utf8_lossy(&ran.stderr);
    assert_eq!(ran.status.code(), Some(0), "{said}");
    assert!(String::from_utf8_lossy(&ran.stdout).contains("run #223"));
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-wait-run-{tag}"
    )));

    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}
