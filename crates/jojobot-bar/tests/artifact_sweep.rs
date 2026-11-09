//! The artifact sweep deletes the large artifacts of a run that ended, and keeps the logs.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::json;

mod support;

/// One artifact as the forge lists it. Ids fall as the list goes on: the forge
/// answers newest first.
fn artifact(id: u64, run: u64, name: &str, age_days: i64, expired: bool) -> serde_json::Value {
    let created = chrono_days_ago(age_days);
    json!({"id": id, "name": name, "size_in_bytes": 1_230_556_583u64, "run_id": run,
           "expired": expired, "created_at": created})
}

fn chrono_days_ago(days: i64) -> String {
    let out = Command::new("date")
        .args([
            "-u",
            "-d",
            &format!("{days} days ago"),
            "+%Y-%m-%dT%H:%M:%S+00:00",
        ])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[derive(Default)]
struct Notes {
    /// `(artifact id, authorization header)` of each DELETE.
    deletes: Vec<(u64, String)>,
}

/// Answer as Forgejo 16 does: lists are plain arrays, newest first, at most 50 to a
/// page whatever `limit` asks; a run's artifacts are filtered by the run; a run
/// answers its status. A DELETE removes the artifact, except for the ids in
/// `refusing`, which answer 403.
fn serve(
    artifacts: Vec<serde_json::Value>,
    runs: Vec<(u64, &'static str)>,
    refusing: Vec<u64>,
) -> (u16, Arc<Mutex<Notes>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let notes = Arc::new(Mutex::new(Notes::default()));
    let seen = notes.clone();
    let mut artifacts = artifacts;
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buffer = [0u8; 4096];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
            let line = request.lines().next().unwrap_or("").to_string();
            let mut parts = line.split_whitespace();
            let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let (bare, query) = path.split_once('?').unwrap_or((path, ""));
            let page: usize = query
                .split('&')
                .find_map(|q| q.strip_prefix("page="))
                .and_then(|p| p.parse().ok())
                .unwrap_or(1);
            let tail: Vec<&str> = bare.rsplit('/').take(2).collect();
            let (status, body) = if method == "DELETE" {
                let id: u64 = tail[0].parse().unwrap();
                let auth = request
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                    .unwrap_or("")
                    .to_string();
                seen.lock().unwrap().deletes.push((id, auth));
                if refusing.contains(&id) {
                    ("403 Forbidden", String::new())
                } else {
                    artifacts.retain(|a| a["id"] != id);
                    ("204 No Content", String::new())
                }
            } else if bare.ends_with("/artifacts") && bare.contains("/runs/") {
                let run: u64 = tail[1].parse().unwrap();
                let list: Vec<_> = artifacts.iter().filter(|a| a["run_id"] == run).collect();
                ("200 OK", serde_json::to_string(&list).unwrap())
            } else if bare.ends_with("/actions/artifacts") {
                let list: Vec<_> = artifacts.iter().skip((page - 1) * 50).take(50).collect();
                ("200 OK", serde_json::to_string(&list).unwrap())
            } else if let Some((_, state)) = runs.iter().find(|(id, _)| id.to_string() == tail[0]) {
                (
                    "200 OK",
                    json!({"id": tail[0].parse::<u64>().unwrap(), "status": state}).to_string(),
                )
            } else {
                ("404 Not Found", String::new())
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, notes)
}

struct Swept {
    said: String,
    code: i32,
    /// The ids the forge was asked to delete, in order.
    asked: Vec<u64>,
    auth: Vec<String>,
}

fn sweep(
    artifacts: Vec<serde_json::Value>,
    runs: Vec<(u64, &'static str)>,
    refusing: Vec<u64>,
    args: &[&str],
    token: Option<&str>,
) -> Swept {
    let (port, notes) = serve(artifacts, runs, refusing);
    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/artifact-sweep");
    let mut command = Command::new("python3");
    command
        .arg(tool)
        .args(args)
        .env("GITHUB_API_URL", format!("http://127.0.0.1:{port}/api/v1"))
        .env("GITHUB_REPOSITORY", "homer/jojobot")
        .env_remove("TOKEN");
    if let Some(token) = token {
        command.env("TOKEN", token);
    }
    let out = support::output_fenced(&mut command).unwrap();
    let said =
        String::from_utf8_lossy(&out.stdout).into_owned() + &String::from_utf8_lossy(&out.stderr);
    let notes = std::mem::take(&mut *notes.lock().unwrap());
    Swept {
        said,
        code: out.status.code().unwrap_or(-1),
        asked: notes.deletes.iter().map(|(id, _)| *id).collect(),
        auth: notes.deletes.into_iter().map(|(_, a)| a).collect(),
    }
}

/// Run 7 holds a bundle, a fixture, a store and three logs; run 8 holds a bundle.
fn two_runs() -> Vec<serde_json::Value> {
    vec![
        artifact(60, 8, "test-bundle", 0, false),
        artifact(59, 7, "bar-logs-year", 0, false),
        artifact(58, 7, "bar-logs-rooms", 0, false),
        artifact(57, 7, "bar-logs-check", 0, false),
        artifact(56, 7, "boot-store", 0, false),
        artifact(55, 7, "test-bundle", 0, false),
        artifact(54, 7, "upgrade-fixture", 0, false),
        artifact(53, 7, "bar-logs-build", 0, false),
        // A proof a later step reads after the run: not named as large, so it stays.
        artifact(52, 7, "gate-f-proof", 0, false),
    ]
}

#[test]
fn a_run_deletes_its_own_large_artifacts_and_keeps_its_logs() {
    let swept = sweep(
        two_runs(),
        vec![(7, "success"), (8, "running")],
        vec![],
        &["--run", "7", "--delete"],
        Some("t0ken"),
    );
    assert_eq!(swept.code, 0, "{}", swept.said);
    assert_eq!(swept.asked, [56, 55, 54], "{}", swept.said);
    assert!(
        swept.auth.iter().all(|a| a.ends_with("token t0ken")),
        "{:?}",
        swept.auth
    );
}

#[test]
fn a_run_without_delete_says_what_it_would_delete_and_deletes_nothing() {
    let swept = sweep(
        two_runs(),
        vec![(7, "success")],
        vec![],
        &["--run", "7"],
        None,
    );
    assert_eq!(swept.code, 0, "{}", swept.said);
    assert!(swept.asked.is_empty(), "{}", swept.said);
    assert!(swept.said.contains("test-bundle"), "{}", swept.said);
}

#[test]
fn delete_without_a_token_deletes_nothing_and_fails() {
    let swept = sweep(
        two_runs(),
        vec![(7, "success")],
        vec![],
        &["--run", "7", "--delete"],
        None,
    );
    assert_ne!(swept.code, 0, "{}", swept.said);
    assert!(swept.asked.is_empty(), "{}", swept.said);
}

#[test]
fn a_refused_delete_fails_the_run_after_the_others_are_tried() {
    let swept = sweep(
        two_runs(),
        vec![(7, "success")],
        vec![56],
        &["--run", "7", "--delete"],
        Some("t0ken"),
    );
    assert_ne!(swept.code, 0, "{}", swept.said);
    assert_eq!(swept.asked, [56, 55, 54], "{}", swept.said);
}

#[test]
fn the_sweep_deletes_what_ended_runs_left_and_not_what_runs_in_flight_hold() {
    // Run 10 failed, 11 was cancelled, 12 succeeded, 13 runs, 14 waits.
    // Sixty artifacts of one finished run come first, so the sweep has to read a
    // second page to reach the rest. An expired artifact is gone from disk already.
    let mut list = vec![
        artifact(500, 13, "test-bundle", 0, false),
        artifact(499, 14, "upgrade-fixture", 0, false),
        artifact(498, 10, "test-bundle", 0, false),
        artifact(497, 11, "test-bundle", 0, false),
        artifact(494, 10, "gate-j-proof", 0, false),
        artifact(496, 11, "bar-logs-check", 0, false),
        artifact(495, 12, "boot-store", 0, true),
    ];
    for id in 100..160 {
        list.push(artifact(id, 12, "bar-logs-year", 0, false));
    }
    list.push(artifact(99, 12, "test-bundle", 1, false));
    let swept = sweep(
        list,
        vec![
            (10, "failure"),
            (11, "cancelled"),
            (12, "success"),
            (13, "running"),
            (14, "waiting"),
        ],
        vec![],
        &["--finished", "--delete"],
        Some("t0ken"),
    );
    assert_eq!(swept.code, 0, "{}", swept.said);
    assert_eq!(swept.asked, [498, 497, 99], "{}", swept.said);
}

#[test]
fn a_run_the_forge_does_not_know_is_held_not_deleted() {
    let swept = sweep(
        vec![artifact(70, 99, "test-bundle", 0, false)],
        vec![],
        vec![],
        &["--finished", "--delete"],
        Some("t0ken"),
    );
    assert!(swept.asked.is_empty(), "{}", swept.said);
}

/// The sweep reads three days back. An older artifact is left to its own retention, and
/// one old artifact in a page never hides the newer artifacts beside it.
#[test]
fn the_sweep_leaves_artifacts_older_than_its_window_and_still_reads_the_newer_ones() {
    let swept = sweep(
        vec![
            artifact(91, 10, "test-bundle", 5, false),
            artifact(90, 10, "boot-store", 0, false),
        ],
        vec![(10, "success")],
        vec![],
        &["--finished", "--delete"],
        Some("t0ken"),
    );
    assert_eq!(swept.code, 0, "{}", swept.said);
    assert_eq!(swept.asked, [90], "{}", swept.said);
}
