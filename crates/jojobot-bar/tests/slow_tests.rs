//! **`scripts/slow-tests` prints the cases a run's logs show over a bound, and
//! their count, from the run's number alone.**
//!
//! Every case runs it against a stand-in forge on a local port. The shapes it
//! answers with are the ones the real forge answered for a real run: the artifact
//! list of a run names `bar-logs-<job>` archives, and an archive holds `plan.log`,
//! `verdict-<job>.json` and the job's `<job>.log`, whose case lines are the raw
//! report-time lines of `cargo test`. Those lines are cut from a real run of this
//! workspace (`report-time-with-ignored.txt`) with the times of named cases set.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::json;

const REPO: &str = "/api/v1/repos/homer/jojobot";
const REAL: &str = include_str!("fixtures/report-time-with-ignored.txt");
const ALPHA: &str = "attention::tests::a_blank_snooze_day_is_no_snooze";
const BETA: &str = "clock::tests::a_stated_clock_stamps_the_day_it_states";
const GAMMA: &str = "attention::tests::a_promise_is_owed_on_its_day_until_it_ends";

/// The real report-time lines with the time of each named case replaced.
fn log_with(times: &[(&str, &str)]) -> String {
    REAL.lines()
        .map(|line| {
            let mut line = line.to_string();
            for (case, seconds) in times {
                if line.starts_with(&format!("test {case} ... ")) {
                    line = format!("test {case} ... ok <{seconds}s>");
                }
            }
            format!("{line}\n")
        })
        .collect()
}

/// One job's uploaded logs: its name, whether the forge has expired them, and the
/// text of its `<job>.log`.
struct Logs {
    job: &'static str,
    expired: bool,
    log: String,
}

fn logs(job: &'static str, log: String) -> Logs {
    Logs {
        job,
        expired: false,
        log,
    }
}

/// The archive the forge serves for one job's logs, as the real one is laid out.
fn zip_of(logs: &Logs) -> Vec<u8> {
    let files = json!({
        "plan.log": "plan: a plan line\n",
        format!("verdict-{}.json", logs.job): "{\"verdict\":\"green\"}",
        format!("{}.log", logs.job): logs.log,
    });
    let script = "import io, json, sys, zipfile\n\
                  files = json.load(sys.stdin)\n\
                  out = io.BytesIO()\n\
                  with zipfile.ZipFile(out, 'w') as archive:\n    \
                  [archive.writestr(name, text) for name, text in files.items()]\n\
                  sys.stdout.buffer.write(out.getvalue())\n";
    let mut child = Command::new("python3")
        .args(["-I", "-c", script])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("python3 runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(files.to_string().as_bytes())
        .expect("the files are written");
    child.wait_with_output().expect("the archive").stdout
}

/// A forge holding one run, number 700: its artifacts, `bar-logs-<job>` for each of
/// `jobs` and one more that is not a bar log.
fn stand(jobs: Vec<Logs>) -> String {
    stand_with_runs(jobs, json!([]))
}

/// A run as the forge lists it: the keys the tool reads, with the values the real
/// list carried for a pull-request run and for a push run.
fn listed_run(id: u64, event: &str, base: &str, status: &str, hours_ago: u64) -> serde_json::Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_secs();
    let then = now - hours_ago * 3600;
    // Civil date from days since 1970 (the days-to-civil method), in UTC.
    let (days, rest) = ((then / 86_400) as i64, then % 86_400);
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    let stopped = format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}+00:00",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    );
    json!({
        "id": id,
        "workflow_id": if event == "push" { "push.yml" } else { "pull-request.yml" },
        "event": event,
        "trigger_event": event,
        "status": status,
        "stopped": stopped,
        // The forge keeps the event payload as JSON text; a push has no pull request in it.
        "event_payload": if event == "push" {
            json!({ "ref": "refs/heads/dev-work" }).to_string()
        } else {
            json!({ "action": "opened", "pull_request": { "base": { "ref": base, "label": base } } }).to_string()
        },
    })
}

fn stand_with_runs(jobs: Vec<Logs>, runs: serde_json::Value) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = format!(
        "http://{}{REPO}",
        listener.local_addr().expect("an address")
    );
    let archives: Vec<Vec<u8>> = jobs.iter().map(zip_of).collect();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            let path = text.split_whitespace().nth(1).unwrap_or_default();
            let rest = path.strip_prefix(REPO).unwrap_or("");
            let (code, body): (u16, Vec<u8>) = if rest.starts_with("/actions/runs?") {
                (
                    200,
                    json!({ "workflow_runs": runs }).to_string().into_bytes(),
                )
            } else if rest == "/actions/runs/700/artifacts" {
                let mut list: Vec<_> = jobs
                    .iter()
                    .enumerate()
                    .map(|(index, logs)| {
                        json!({
                            "id": 9000 + index,
                            "name": format!("bar-logs-{}", logs.job),
                            "expired": logs.expired,
                            "run_id": 700,
                        })
                    })
                    .collect();
                list.push(json!({
                    "id": 9999, "name": "coverage", "expired": false, "run_id": 700
                }));
                (200, json!(list).to_string().into_bytes())
            } else if let Some(index) = rest
                .strip_prefix("/actions/artifacts/")
                .and_then(|r| r.strip_suffix("/zip"))
                .and_then(|id| id.parse::<usize>().ok())
                .and_then(|id| id.checked_sub(9000))
                .filter(|index| *index < jobs.len())
            {
                (200, archives[index].clone())
            } else {
                (404, b"404 page not found".to_vec())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(&body);
        }
    });
    addr
}

/// A forge that answers `answer(path)` with a status and a body: one that fails
/// where the real one does, which `stand` cannot.
fn stand_by(answer: impl Fn(&str) -> (u16, String) + Send + 'static) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = format!(
        "http://{}{REPO}",
        listener.local_addr().expect("an address")
    );
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            let path = text.split_whitespace().nth(1).unwrap_or_default();
            let (code, body) = answer(path.strip_prefix(REPO).unwrap_or(""));
            let _ = write!(
                stream,
                "HTTP/1.1 {code} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    addr
}

fn tool() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/slow-tests")
}

fn slow(api: Option<&str>, args: &[&str]) -> (i32, String, String) {
    let mut command = Command::new(tool());
    command.args(args).env_remove("GITHUB_ACTIONS");
    match api {
        Some(api) => command.env("WAIT_RUN_API", api),
        None => command.env_remove("WAIT_RUN_API"),
    };
    let out = command.output().expect("the tool runs");
    (
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}

mod slow_tests {
    use super::*;

    #[test]
    fn the_cases_over_sixty_seconds_are_listed_slowest_first_with_their_count() {
        let api = stand(vec![logs(
            "check",
            log_with(&[(ALPHA, "75.3"), (BETA, "61.0"), (GAMMA, "59.9")]),
        )]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        // Positive: the two cases over the bound, slowest first. Negative: the case
        // at 59.9 s and the hundreds of fast ones are not named.
        assert_eq!(
            out.lines().collect::<Vec<_>>(),
            vec![
                format!("75.3s  check  {ALPHA}"),
                format!("61.0s  check  {BETA}"),
                "slow tests over 60 s: 2 (run #700)".to_string(),
            ],
            "{out}"
        );
    }

    #[test]
    fn a_case_two_jobs_ran_is_counted_once_with_the_longest_time() {
        let api = stand(vec![
            logs("report", log_with(&[(ALPHA, "88.4")])),
            logs("check", log_with(&[(ALPHA, "70.0")])),
        ]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert_eq!(
            out.lines().collect::<Vec<_>>(),
            vec![
                format!("88.4s  report  {ALPHA}"),
                "slow tests over 60 s: 1 (run #700)".to_string(),
            ],
            "{out}"
        );
    }

    #[test]
    fn the_bound_can_be_named_and_a_job_with_no_timed_line_is_named() {
        let api = stand(vec![
            logs("check", log_with(&[(ALPHA, "12.5")])),
            logs("static", "verdict: GREEN\n".into()),
        ]);
        let (code, out, err) = slow(Some(&api), &["700", "--over", "10"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(out.contains(&format!("12.5s  check  {ALPHA}")), "{out}");
        // The real capture holds one case of its own at 12.8 s, so two are over 10 s.
        assert!(
            out.contains("12.8s  check  memory::tests::fake_satisfies_the_contract"),
            "{out}"
        );
        assert!(
            out.contains("no timed case line in the logs of the static job"),
            "{out}"
        );
        assert!(
            out.ends_with("slow tests over 10 s: 2 (run #700)\n"),
            "{out}"
        );
    }

    #[test]
    fn an_expired_job_is_named_and_an_artifact_that_is_no_bar_log_is_not_read() {
        let mut expired = logs("rooms", log_with(&[(BETA, "99.0")]));
        expired.expired = true;
        let api = stand(vec![logs("check", log_with(&[(ALPHA, "75.3")])), expired]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(
            out.contains("the logs of the rooms job have expired"),
            "{out}"
        );
        assert!(
            !out.contains(BETA),
            "an expired archive is not read, whatever it held: {out}"
        );
        assert!(!out.contains("coverage"), "{out}");
        assert!(
            out.ends_with("slow tests over 60 s: 1 (run #700)\n"),
            "{out}"
        );
    }

    #[test]
    fn an_expired_only_run_is_not_counted() {
        // A pull-request run up to seven days old is read, and the forge keeps its
        // artifacts seven days: a run near the edge has only expired ones. The weekly
        // step must go on to print the size, so this is a line, not an exit 1.
        let mut check = logs("check", log_with(&[(ALPHA, "75.3")]));
        check.expired = true;
        let mut rooms = logs("rooms", log_with(&[(BETA, "99.0")]));
        rooms.expired = true;
        let api = stand(vec![check, rooms]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(
            out.contains("the logs of the check job have expired"),
            "{out}"
        );
        assert_eq!(
            out.lines().last(),
            Some("slow tests over 60 s: not counted (run #700: every job's logs have expired)"),
            "{out}"
        );
    }

    /// The weekly step runs the reader before `make size`, so every way the forge can
    /// fail to give logs ends in the same last line and exit 0.
    #[test]
    fn a_forge_that_errors_leaves_the_count_not_counted() {
        let list_fails = stand_by(|_| (500, "boom".into()));
        let (code, out, err) = slow(Some(&list_fails), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(
            out.lines()
                .last()
                .is_some_and(|line| line.starts_with("slow tests over 60 s: not counted (run #700")),
            "an artifact list that fails: {out}{err}"
        );

        let zip_fails = stand_by(|path| {
            if path == "/actions/runs/700/artifacts" {
                (
                    200,
                    json!([{"id": 9000, "name": "bar-logs-check", "expired": false}]).to_string(),
                )
            } else {
                (500, "boom".into())
            }
        });
        let (code, out, err) = slow(Some(&zip_fails), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(
            out.lines()
                .last()
                .is_some_and(|line| line.starts_with("slow tests over 60 s: not counted (run #700")),
            "an artifact download that fails: {out}{err}"
        );

        let runs_fail = stand_by(|_| (500, "boom".into()));
        let (code, out, err) = slow(Some(&runs_fail), &["--newest-main"]);
        assert_eq!(code, 0, "{out}{err}");
        assert!(
            out.lines()
                .last()
                .is_some_and(|line| line.starts_with("slow tests over 60 s: not counted (")),
            "a run list that fails: {out}{err}"
        );
    }

    #[test]
    fn a_run_that_kept_no_bar_logs_is_not_counted() {
        let api = stand(vec![]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 0, "{out}{err}");
        assert_eq!(
            out.lines().last(),
            Some("slow tests over 60 s: not counted (run #700: the run kept no bar logs)"),
            "{out}"
        );
    }

    #[test]
    fn a_run_whose_logs_hold_no_timed_line_says_the_count_would_mean_nothing() {
        let api = stand(vec![logs("static", "verdict: GREEN\n".into())]);
        let (code, out, err) = slow(Some(&api), &["700"]);
        assert_eq!(code, 1, "{out}{err}");
        assert!(!out.contains("slow tests over"), "{out}");
        assert!(err.contains("the count says nothing"), "{err}");
    }

    #[test]
    fn the_newest_successful_pull_request_run_of_the_week_is_the_one_read() {
        // Newer runs that are not it: a push run, a dispatched run of the same workflow,
        // a pull request into a deploy branch, a failed pull-request run. Older: a
        // pull-request run of the week, and one past the week.
        let api = stand_with_runs(
            vec![logs("check", log_with(&[(ALPHA, "75.3")]))],
            json!([
                listed_run(800, "push", "main", "success", 1),
                listed_run(790, "workflow_dispatch", "main", "success", 1),
                listed_run(780, "pull_request", "deploy-9.3", "success", 1),
                listed_run(750, "pull_request", "main", "failure", 2),
                listed_run(700, "pull_request", "main", "success", 5),
                listed_run(650, "pull_request", "main", "success", 48),
            ]),
        );
        let (code, out, err) = slow(Some(&api), &["--newest-main"]);
        assert_eq!(code, 0, "{out}{err}");
        assert_eq!(
            out.lines().last(),
            Some("slow tests over 60 s: 1 (run #700)"),
            "{out}"
        );
    }

    #[test]
    fn a_week_with_no_successful_pull_request_run_counts_nothing_and_reads_no_other_run() {
        // The push run holds artifacts no one may read in its place: id 700 is the only
        // run the stand-in has logs for, and it is a push run.
        let api = stand_with_runs(
            vec![logs("check", log_with(&[(ALPHA, "75.3")]))],
            json!([
                listed_run(700, "push", "main", "success", 1),
                listed_run(690, "pull_request", "deploy-9.3", "success", 2),
                listed_run(650, "pull_request", "main", "success", 24 * 9),
            ]),
        );
        let (code, out, err) = slow(Some(&api), &["--newest-main"]);
        assert_eq!(code, 0, "{out}{err}");
        assert_eq!(
            out.lines().collect::<Vec<_>>(),
            vec![
                "slow tests over 60 s: not counted (no successful pull-request run in the last 7 days)"
            ],
            "{out}"
        );
    }

    #[test]
    fn a_run_and_the_newest_main_flag_are_asked_one_at_a_time() {
        let api = stand(vec![]);
        let (code, _, _) = slow(Some(&api), &["700", "--newest-main"]);
        assert_eq!(code, 2);
        let (code, _, _) = slow(Some(&api), &[]);
        assert_eq!(code, 2);
    }

    #[test]
    fn a_run_the_forge_does_not_hold_and_a_missing_api_root_exit_four() {
        let api = stand(vec![]);
        let (code, _, err) = slow(Some(&api), &["701"]);
        assert_eq!(code, 4, "{err}");
        let (code, _, err) = slow(None, &["700"]);
        assert_eq!(code, 4, "{err}");
        assert!(err.contains("WAIT_RUN_API"), "{err}");
    }
}
