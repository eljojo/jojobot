//! The branch prune deletes the branches whose work is on main, and nothing else.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde_json::json;

mod support;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = support::output_fenced(
        Command::new("git")
            .args(["-c", "user.name=bar", "-c", "user.email=bar@example.com"])
            .args(args)
            .current_dir(dir),
    )
    .expect("run git");
    assert!(out.status.success(), "git {args:?}: {out:?}");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn commit(dir: &Path, file: &str, text: &str) -> String {
    std::fs::write(dir.join(file), text).unwrap();
    git(dir, &["add", file]);
    git(dir, &["commit", "-q", "-m", &format!("write {file}")]);
    git(dir, &["rev-parse", "HEAD"])
}

/// A scratch repository whose `origin/main` holds `merged`, with these branches:
/// `merged` (an ancestor of main), `patched` (a commit main holds a copy of),
/// `ahead` (a commit main lacks), and the held `deploy-9`, `em-carry-x`, the named
/// operator and card branches, `pulled`
/// and `fresh`, each an ancestor of main. Returns the branch list as the forge
/// answers it.
fn repository(dir: &Path) -> Vec<serde_json::Value> {
    git(dir, &["init", "-q", "-b", "main"]);
    let base = commit(dir, "base", "base\n");
    let merged = commit(dir, "merged", "merged\n");
    git(dir, &["switch", "-q", "-c", "patched", &base]);
    let patched = commit(dir, "patch", "patch\n");
    git(dir, &["switch", "-q", "-c", "ahead", &base]);
    let ahead = commit(dir, "ahead", "ahead\n");
    git(dir, &["switch", "-q", "main"]);
    git(dir, &["cherry-pick", &patched]);
    git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let main = git(dir, &["rev-parse", "HEAD"]);
    let old = "2026-01-01T00:00:00+00:00";
    let now = chrono_now();
    let entry = |name: &str, sha: &str, stamp: &str, protected: bool| {
        json!({"name": name, "protected": protected,
               "commit": {"id": sha, "timestamp": stamp}})
    };
    vec![
        entry("main", &main, old, false),
        entry("merged", &merged, old, false),
        entry("patched", &patched, old, false),
        entry("ahead", &ahead, old, false),
        entry("deploy-9", &merged, old, false),
        entry("em-carry-x", &merged, old, false),
        entry("dev-unreadable-922-work", &merged, old, false),
        entry("dev-unreadable-fix2-922-work", &merged, old, false),
        entry("dev-sol-scratch-names-work", &merged, old, false),
        entry("pulled", &merged, old, false),
        entry("fresh", &merged, &now, false),
        entry("guarded", &merged, old, true),
    ]
}

fn chrono_now() -> String {
    let out = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%S+00:00"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// Answer the forge's lists and note each DELETE. Returns the port and the notes.
fn forge(
    branches: Vec<serde_json::Value>,
    pulls: Vec<serde_json::Value>,
) -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let deletes = Arc::new(Mutex::new(Vec::new()));
    let seen = deletes.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut buffer = [0u8; 4096];
            let n = stream.read(&mut buffer).unwrap_or(0);
            let request = String::from_utf8_lossy(&buffer[..n]).into_owned();
            let line = request.lines().next().unwrap_or("").to_string();
            let (method, path) = {
                let mut parts = line.split_whitespace();
                (parts.next().unwrap_or(""), parts.next().unwrap_or(""))
            };
            let page_one = path.contains("page=1");
            let body = if method == "DELETE" {
                seen.lock()
                    .unwrap()
                    .push(path.rsplit('/').next().unwrap().to_string());
                String::new()
            } else if path.contains("/pulls") {
                serde_json::to_string(&if page_one { pulls.clone() } else { vec![] }).unwrap()
            } else {
                serde_json::to_string(&if page_one { branches.clone() } else { vec![] }).unwrap()
            };
            let status = if method == "DELETE" {
                "204 No Content"
            } else {
                "200 OK"
            };
            let _ = write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    (port, deletes)
}

/// Run the prune over the scratch repository; returns what it printed and the
/// branches the forge was told to delete.
fn pruned(tag: &str, args: &[&str]) -> (String, Vec<String>) {
    let dir: PathBuf = std::env::temp_dir().join(format!("prune-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let branches = repository(&dir);
    let pulls = vec![json!({"head": {"ref": "pulled"}})];
    let (port, deletes) = forge(branches, pulls);
    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/branch-prune");
    let out = support::output_fenced(
        Command::new("python3")
            .arg(tool)
            .args(args)
            .current_dir(&dir)
            .env("GITHUB_API_URL", format!("http://127.0.0.1:{port}/api/v1"))
            .env("GITHUB_REPOSITORY", "homer/jojobot")
            .env("TOKEN", "the-job-token"),
    )
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    let said = String::from_utf8_lossy(&out.stdout).into_owned();
    let sent = deletes.lock().unwrap().clone();
    (said, sent)
}

#[test]
fn prune_dry_run_names_the_landed_branches_and_deletes_nothing() {
    let (said, sent) = pruned("dry", &[]);
    assert!(said.contains("would delete merged"), "{said}");
    assert!(
        said.contains("would delete patched"),
        "a patch-equal branch is landed: {said}"
    );
    assert!(sent.is_empty(), "a dry run deleted: {sent:?}");
}

#[test]
fn prune_keeps_a_branch_with_work_main_lacks() {
    let (said, _) = pruned("ahead", &["--delete"]);
    assert!(said.contains("keep ahead"), "{said}");
}

#[test]
fn prune_holds_main_the_release_lines_the_open_pull_request_and_the_new_and_protected() {
    let (said, sent) = pruned("held", &["--delete"]);
    for name in [
        "main",
        "deploy-9",
        "em-carry-x",
        "dev-unreadable-922-work",
        "dev-unreadable-fix2-922-work",
        "dev-sol-scratch-names-work",
        "pulled",
        "fresh",
        "guarded",
    ] {
        assert!(
            said.contains(&format!("hold {name} ")),
            "{name} is not held: {said}"
        );
        assert!(
            !sent.contains(&name.to_string()),
            "{name} was deleted: {sent:?}"
        );
    }
}

#[test]
fn prune_live_deletes_the_landed_branches_and_logs_each_head() {
    let (said, mut sent) = pruned("live", &["--delete"]);
    sent.sort();
    assert_eq!(sent, ["merged", "patched"], "{said}");
    assert!(
        said.lines()
            .any(|l| l.starts_with("deleted merged ") && l.len() > 20),
        "the log names the head: {said}"
    );
}
