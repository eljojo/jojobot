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
/// `ahead` (a commit main lacks), `merger` (the patched commit and a merge main
/// lacks), and the held `deploy-9`, `em-carry-x`, the named operator and card
/// branches, `pulled`, `fresh` (an old landed commit the forge saw pushed just
/// now), `unrecorded` (no push of its head in the forge's activity) and
/// `guarded`, each an ancestor of main. Returns the branch list as the forge
/// answers it. Every commit date is old: the prune must not read it as a push time.
fn repository(dir: &Path) -> Vec<serde_json::Value> {
    git(dir, &["init", "-q", "-b", "main"]);
    let base = commit(dir, "base", "base\n");
    let merged = commit(dir, "merged", "merged\n");
    git(dir, &["switch", "-q", "-c", "patched", &base]);
    let patched = commit(dir, "patch", "patch\n");
    git(dir, &["switch", "-q", "-c", "merger", &patched]);
    git(dir, &["merge", "-q", "--no-ff", "-m", "merge", &merged]);
    let merger = git(dir, &["rev-parse", "HEAD"]);
    git(dir, &["switch", "-q", "-c", "ahead", &base]);
    let ahead = commit(dir, "ahead", "ahead\n");
    git(dir, &["switch", "-q", "main"]);
    git(dir, &["cherry-pick", &patched]);
    git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    let main = git(dir, &["rev-parse", "HEAD"]);
    let old = "2026-01-01T00:00:00+00:00";
    let entry = |name: &str, sha: &str, stamp: &str, protected: bool| {
        json!({"name": name, "protected": protected,
               "commit": {"id": sha, "timestamp": stamp}})
    };
    let branches = vec![
        entry("main", &main, old, false),
        entry("merged", &merged, old, false),
        entry("patched", &patched, old, false),
        entry("merger", &merger, old, false),
        entry("ahead", &ahead, old, false),
        entry("deploy-9", &merged, old, false),
        entry("em-carry-x", &merged, old, false),
        entry("dev-unreadable-922-work", &merged, old, false),
        entry("dev-unreadable-fix2-922-work", &merged, old, false),
        entry("dev-sol-scratch-names-work", &merged, old, false),
        entry("pulled", &merged, old, false),
        entry("fresh", &merged, old, false),
        entry("unrecorded", &merged, old, false),
        entry("guarded", &merged, old, true),
    ];
    // The fake forge lists these names, so the remote must hold them to delete.
    for branch in &branches {
        let (name, sha) = (
            branch["name"].as_str().unwrap(),
            branch["commit"]["id"].as_str().unwrap(),
        );
        git(dir, &["update-ref", &format!("refs/heads/{name}"), sha]);
    }
    branches
}

fn chrono_now() -> String {
    let out = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%S+00:00"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// What the fake forge was asked to do, and how it differs from its own list.
#[derive(Default)]
struct Forge {
    /// Branches whose single read answers another head than the list did: a push
    /// that landed while the prune was deciding.
    moved: Vec<(String, String)>,
    /// Branches that get a new commit on the remote while the forge answers the
    /// read, which still names the head that was judged: a push that landed
    /// between the check and the delete.
    moves_on_read: Vec<String>,
    /// Branches the forge deleted and made again after their last recorded push:
    /// the feed holds the old push of that name and head, and a newer delete.
    recreated: Vec<String>,
    /// Branches that get a delete event in the feed while the forge answers the
    /// read: the branch was deleted and made again after the prune read the feed.
    activity_on_read: Vec<String>,
    /// Branches whose delete the remote's pre-receive hook declines.
    refusing: Vec<String>,
}

#[derive(Default)]
struct Notes {
    /// `(branch, authorization header)` for each DELETE.
    deletes: Vec<(String, String)>,
}

/// Answer the forge's lists, activity feed, single branch reads and deletes. Like
/// the real forge, a DELETE removes the branch from `remote` whatever head it
/// holds. `work` is the checkout a push to `remote` is made from.
fn serve(
    forge: Forge,
    branches: Vec<serde_json::Value>,
    pulls: Vec<serde_json::Value>,
    feed: Vec<serde_json::Value>,
    remote: PathBuf,
    work: PathBuf,
) -> (u16, Arc<Mutex<Notes>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let notes = Arc::new(Mutex::new(Notes::default()));
    let seen = notes.clone();
    let mut feed = feed;
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
            let bare = path.split('?').next().unwrap_or("");
            let page_one = path.contains("page=1");
            let name = bare.rsplit('/').next().unwrap_or("").to_string();
            let (status, body) = if method == "DELETE" {
                let auth = request
                    .lines()
                    .find(|l| l.to_ascii_lowercase().starts_with("authorization:"))
                    .unwrap_or("")
                    .to_string();
                seen.lock().unwrap().deletes.push((name.clone(), auth));
                let _ = Command::new("git")
                    .arg("--git-dir")
                    .arg(&remote)
                    .args(["update-ref", "-d", &format!("refs/heads/{name}")])
                    .output();
                ("204 No Content", String::new())
            } else if bare.ends_with("/pulls") {
                let list = if page_one { pulls.clone() } else { vec![] };
                ("200 OK", serde_json::to_string(&list).unwrap())
            } else if bare.ends_with("/activities/feeds") {
                // The newest event first, as the forge lists them.
                let mut list = if page_one { feed.clone() } else { vec![] };
                list.sort_by_key(|e| std::cmp::Reverse(e["id"].as_u64().unwrap()));
                ("200 OK", serde_json::to_string(&list).unwrap())
            } else if bare.ends_with("/branches") {
                let list = if page_one { branches.clone() } else { vec![] };
                ("200 OK", serde_json::to_string(&list).unwrap())
            } else {
                let mut one = branches
                    .iter()
                    .find(|b| b["name"] == name.as_str())
                    .cloned()
                    .unwrap_or_else(|| json!({}));
                if let Some((_, sha)) = forge.moved.iter().find(|(n, _)| *n == name) {
                    one["commit"]["id"] = json!(sha);
                }
                let body = serde_json::to_string(&one).unwrap();
                if forge.moves_on_read.contains(&name) {
                    push_onto(&work, &name);
                }
                if forge.activity_on_read.contains(&name) {
                    let id = feed
                        .iter()
                        .map(|e| e["id"].as_u64().unwrap())
                        .max()
                        .unwrap()
                        + 1;
                    feed.push(deleted_event(id, &name, &chrono_now()));
                }
                ("200 OK", body)
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

struct Pruned {
    said: String,
    code: i32,
    notes: Notes,
    /// The branches the remote no longer holds, sorted.
    gone: Vec<String>,
}

impl Pruned {
    fn deleted(&self) -> Vec<String> {
        self.gone.clone()
    }
}

/// A bare repository that holds every branch of `dir`, set up as its `origin`.
/// A pre-receive hook declines the deletion of each branch in `refusing`.
fn remote_of(dir: &Path, refusing: &[String]) -> PathBuf {
    let bare = dir.join("remote.git");
    git(dir, &["clone", "-q", "--bare", ".", bare.to_str().unwrap()]);
    git(dir, &["remote", "add", "origin", bare.to_str().unwrap()]);
    // The hook reads the pushed ref lines once, so it checks them all in one pass.
    let checks: String = refusing
        .iter()
        .map(|n| format!("echo \"$lines\" | grep -q '^[0-9a-f]* 0* refs/heads/{n}$' && exit 1\n"))
        .collect();
    let path = bare.join("hooks/pre-receive");
    std::fs::write(&path, format!("#!/bin/sh\nlines=$(cat)\n{checks}exit 0\n")).unwrap();
    Command::new("chmod").arg("+x").arg(&path).output().unwrap();
    bare
}

/// Put a new commit on top of `name` in `remote`, as a push from a developer does.
fn push_onto(dir: &Path, name: &str) {
    let tree = git(dir, &["rev-parse", &format!("{name}^{{tree}}")]);
    let child = git(dir, &["commit-tree", &tree, "-p", name, "-m", "later work"]);
    git(
        dir,
        &[
            "push",
            "-q",
            "origin",
            &format!("{child}:refs/heads/{name}"),
        ],
    );
}

/// A feed event for the deletion of branch `name`, as the forge records one.
fn deleted_event(id: u64, name: &str, when: &str) -> serde_json::Value {
    json!({"id": id, "op_type": "delete_branch",
           "ref_name": format!("refs/heads/{name}"), "created": when, "content": ""})
}

/// Run the prune over the scratch repository against the fake forge.
fn pruned_with(tag: &str, forge: Forge, args: &[&str], token: Option<&str>) -> Pruned {
    let dir: PathBuf = std::env::temp_dir().join(format!("prune-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let branches = repository(&dir);
    let remote = remote_of(&dir, &forge.refusing);
    let pulls = vec![json!({"head": {"ref": "pulled"}})];
    let listed: Vec<String> = branches
        .iter()
        .map(|b| b["name"].as_str().unwrap().to_string())
        .collect();
    let now = chrono_now();
    let mut feed: Vec<serde_json::Value> = branches
        .iter()
        .filter(|b| b["name"] != "unrecorded")
        .enumerate()
        .map(|(at, b)| {
            let name = b["name"].as_str().unwrap();
            let sha = b["commit"]["id"].as_str().unwrap();
            let when = if name == "fresh" {
                now.as_str()
            } else {
                "2026-01-01T00:00:00-04:00"
            };
            json!({"id": at + 1, "op_type": "commit_repo",
                   "ref_name": format!("refs/heads/{name}"),
                   "created": when,
                   "content": json!({"HeadCommit": {"Sha1": sha}}).to_string()})
        })
        .collect();
    for name in &forge.recreated {
        let id = feed
            .iter()
            .map(|e| e["id"].as_u64().unwrap())
            .max()
            .unwrap()
            + 1;
        feed.push(deleted_event(id, name, &now));
    }
    let (port, notes) = serve(forge, branches, pulls, feed, remote.clone(), dir.clone());
    let tool = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/branch-prune");
    let mut command = Command::new("python3");
    command
        .arg(tool)
        .args(args)
        .current_dir(&dir)
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
    let mut gone: Vec<String> = listed
        .into_iter()
        .filter(|n| {
            !Command::new("git")
                .arg("--git-dir")
                .arg(&remote)
                .args(["rev-parse", "-q", "--verify", &format!("refs/heads/{n}")])
                .output()
                .unwrap()
                .status
                .success()
        })
        .collect();
    gone.sort();
    Pruned {
        said,
        code: out.status.code().unwrap_or(-1),
        notes,
        gone,
    }
}

fn pruned(tag: &str, args: &[&str]) -> Pruned {
    pruned_with(tag, Forge::default(), args, Some("the-job-token"))
}

#[test]
fn prune_dry_run_names_the_landed_branches_and_deletes_nothing() {
    let run = pruned("dry", &[]);
    assert!(run.said.contains("would delete merged"), "{}", run.said);
    assert!(
        run.said.contains("would delete patched"),
        "a patch-equal branch is landed: {}",
        run.said
    );
    assert!(
        run.notes.deletes.is_empty(),
        "a dry run deleted: {:?}",
        run.notes.deletes
    );
    assert!(run.deleted().is_empty(), "a dry run deleted: {}", run.said);
}

#[test]
fn prune_keeps_a_branch_with_work_main_lacks() {
    let run = pruned("ahead", &["--delete"]);
    assert!(run.said.contains("keep ahead"), "{}", run.said);
}

#[test]
fn prune_keeps_a_branch_that_holds_a_merge_main_lacks() {
    // git cherry skips a merge commit, so this branch reads as patch-equal.
    let run = pruned("merger", &["--delete"]);
    assert!(run.said.contains("keep merger"), "{}", run.said);
    assert!(
        !run.deleted().contains(&"merger".to_string()),
        "{}",
        run.said
    );
}

#[test]
fn prune_holds_main_the_release_lines_the_open_pull_request_and_the_new_and_protected() {
    let run = pruned("held", &["--delete"]);
    let sent = run.deleted();
    for name in [
        "main",
        "deploy-9",
        "em-carry-x",
        "dev-unreadable-922-work",
        "dev-unreadable-fix2-922-work",
        "dev-sol-scratch-names-work",
        "pulled",
        "fresh",
        "unrecorded",
        "guarded",
    ] {
        assert!(
            run.said.contains(&format!("hold {name} ")),
            "{name} is not held: {}",
            run.said
        );
        assert!(
            !sent.contains(&name.to_string()),
            "{name} was deleted: {sent:?}"
        );
    }
}

#[test]
fn prune_live_deletes_the_landed_branches_and_logs_each_head() {
    let run = pruned("live", &["--delete"]);
    assert_eq!(run.deleted(), ["merged", "patched"], "{}", run.said);
    assert_eq!(run.code, 0, "{}", run.said);
    assert!(
        run.said
            .lines()
            .any(|l| l.starts_with("deleted merged ") && l.len() > 20),
        "the log names the head: {}",
        run.said
    );
    // The API delete names no head, so the prune never sends one.
    assert!(
        run.notes.deletes.is_empty(),
        "an unconditional API delete was sent: {:?}",
        run.notes.deletes
    );
}

#[test]
fn prune_keeps_a_branch_pushed_to_between_the_check_and_the_delete() {
    // The forge's list and read still answer the judged head: the push lands
    // while the read is answered, so only the delete itself can see it.
    let forge = Forge {
        moves_on_read: vec!["merged".into()],
        ..Forge::default()
    };
    let run = pruned_with("late-push", forge, &["--delete"], Some("the-job-token"));
    assert_eq!(run.deleted(), ["patched"], "{}", run.said);
    assert!(run.said.contains("keep merged"), "{}", run.said);
    assert_eq!(run.code, 0, "{}", run.said);
}

#[test]
fn prune_keeps_a_branch_that_moved_since_it_was_judged() {
    let forge = Forge {
        moved: vec![("merged".into(), "0".repeat(40))],
        ..Forge::default()
    };
    let run = pruned_with("moved", forge, &["--delete"], Some("the-job-token"));
    assert_eq!(run.deleted(), ["patched"], "{}", run.said);
    assert!(
        run.said.contains("changed since judged") && run.said.contains("merged"),
        "{}",
        run.said
    );
}

#[test]
fn prune_holds_a_branch_made_again_at_the_head_it_was_pushed_at() {
    // The feed holds an old push of this name and head, and a later delete. The
    // branch that stands now is a new one at the same head, pushed no one knows
    // when, so the old push says nothing about its age.
    let forge = Forge {
        recreated: vec!["merged".into()],
        ..Forge::default()
    };
    let run = pruned_with("recreated", forge, &["--delete"], Some("the-job-token"));
    assert_eq!(run.deleted(), ["patched"], "{}", run.said);
    assert!(run.said.contains("hold merged "), "{}", run.said);
}

#[test]
fn prune_keeps_a_branch_made_again_after_the_feed_was_read() {
    // The branch is deleted and made again at the same head between the prune's
    // read of the feed and its delete. The lease cannot see it: the head is the
    // same. Only the feed can.
    let forge = Forge {
        activity_on_read: vec!["merged".into()],
        ..Forge::default()
    };
    let run = pruned_with("late-recreate", forge, &["--delete"], Some("the-job-token"));
    assert_eq!(run.deleted(), ["patched"], "{}", run.said);
    assert!(run.said.contains("keep merged"), "{}", run.said);
    assert_eq!(run.code, 0, "{}", run.said);
}

#[test]
fn prune_names_a_refused_delete_goes_on_and_exits_one() {
    let forge = Forge {
        refusing: vec!["merged".into()],
        ..Forge::default()
    };
    let run = pruned_with("refused", forge, &["--delete"], Some("the-job-token"));
    assert!(run.said.contains("could not delete merged"), "{}", run.said);
    assert!(
        run.deleted().contains(&"patched".to_string()),
        "the run stopped at the refusal: {}",
        run.said
    );
    assert_eq!(run.code, 1, "{}", run.said);
}

#[test]
fn prune_live_without_a_token_says_so_before_it_asks_anything() {
    let run = pruned_with("no-token", Forge::default(), &["--delete"], None);
    assert_ne!(run.code, 0, "{}", run.said);
    assert!(run.said.contains("needs the job's token"), "{}", run.said);
    assert!(run.notes.deletes.is_empty(), "{:?}", run.notes.deletes);
}
