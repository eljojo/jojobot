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
    /// The name of the job each run has; `report` when none.
    job_name: Option<&'static str>,
    /// Jobs beyond the first of a run: the run's id, the job's name and
    /// status, and its log.
    more_jobs: Vec<(u64, &'static str, &'static str, String)>,
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
            .filter(|run| query("head_sha").is_none_or(|sha| run["commit_sha"] == sha))
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
        if job >= 9000 {
            return match forge.more_jobs.get((job - 9000) as usize) {
                Some((_, _, _, log)) => (200, log.clone()),
                None => (404, "404 page not found".into()),
            };
        }
        let id = job.saturating_sub(1000);
        return match forge.runs.iter().find(|(run, _)| run["id"] == id) {
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
        "run #225",
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
        out.contains("#219"),
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
    assert!(out.contains("run #219"), "{out}");
    assert!(out.contains("also on this commit: #225"), "{out}");
}

/// **A commit names the newest run it has, and says there are others.** Its exit
/// code is that run's, so the same commit read by branch and by commit can differ.
#[test]
fn a_commit_names_its_newest_run_and_says_what_else_holds_it() {
    let forge = stand(two_runs_of_one_commit());
    let (code, out, err) = wait(&forge, &[GREEN_SHA]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("run #225"), "{out}");
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
        out.starts_with("run #225 ") && out.contains(GREEN_SHA),
        "{out}"
    );
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
            "run #227",
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
        out.contains("run #230") && out.contains("cancelled"),
        "{out}"
    );
    assert!(out.contains("no verdict block"), "{out}");
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

/// **A cancelled or a skipped run exits two**, and a run with no complete block in
/// its log says so rather than guessing where a block ends.
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
        assert!(
            out.contains("no verdict block"),
            "{status}: a verdict with no closing line is no block: {out}"
        );
    }
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
    assert!(String::from_utf8_lossy(&ran.stdout).contains("run #225"));
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
    assert!(String::from_utf8_lossy(&ran.stdout).contains("run #225"));
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-wait-run-{tag}"
    )));

    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}
