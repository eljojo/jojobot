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
/// branches, `pulled`, `fresh` and `guarded`, each an ancestor of main. Returns
/// the branch list as the forge answers it.
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
    let now = chrono_now();
    let entry = |name: &str, sha: &str, stamp: &str, protected: bool| {
        json!({"name": name, "protected": protected,
               "commit": {"id": sha, "timestamp": stamp}})
    };
    vec![
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

/// What the fake forge was asked to do, and how it differs from its own list.
#[derive(Default)]
struct Forge {
    /// Branches whose single read answers another head than the list did: a push
    /// that landed while the prune was deciding.
    moved: Vec<(String, String)>,
    /// Branches whose delete the forge refuses.
    refusing: Vec<String>,
}

#[derive(Default)]
struct Notes {
    /// `(branch, authorization header)` for each DELETE.
    deletes: Vec<(String, String)>,
}

/// Answer the forge's lists, single branch reads and deletes.
fn serve(
    forge: Forge,
    branches: Vec<serde_json::Value>,
    pulls: Vec<serde_json::Value>,
) -> (u16, Arc<Mutex<Notes>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let notes = Arc::new(Mutex::new(Notes::default()));
    let seen = notes.clone();
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
                if forge.refusing.contains(&name) {
                    ("500 Internal Server Error", String::new())
                } else {
                    ("204 No Content", String::new())
                }
            } else if bare.ends_with("/pulls") {
                let list = if page_one { pulls.clone() } else { vec![] };
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
                ("200 OK", serde_json::to_string(&one).unwrap())
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
}

impl Pruned {
    fn deleted(&self) -> Vec<String> {
        let mut names: Vec<String> = self.notes.deletes.iter().map(|(n, _)| n.clone()).collect();
        names.sort();
        names
    }
}

/// Run the prune over the scratch repository against the fake forge.
fn pruned_with(tag: &str, forge: Forge, args: &[&str], token: Option<&str>) -> Pruned {
    let dir: PathBuf = std::env::temp_dir().join(format!("prune-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let branches = repository(&dir);
    let pulls = vec![json!({"head": {"ref": "pulled"}})];
    let (port, notes) = serve(forge, branches, pulls);
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
    Pruned {
        said,
        code: out.status.code().unwrap_or(-1),
        notes,
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
fn prune_live_deletes_the_landed_branches_logs_each_head_and_sends_the_token() {
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
    for (name, auth) in &run.notes.deletes {
        assert!(
            auth.ends_with("token the-job-token"),
            "the delete of {name} carried {auth:?}"
        );
    }
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
fn prune_names_a_refused_delete_goes_on_and_exits_one() {
    let forge = Forge {
        refusing: vec!["merged".into()],
        ..Forge::default()
    };
    let run = pruned_with("refused", forge, &["--delete"], Some("the-job-token"));
    assert!(
        run.said.contains("could not delete merged: 500"),
        "{}",
        run.said
    );
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
