//! **`scripts/check-report` checks the facts in a line report's header before
//! anyone reads the prose.**
//!
//! The report arrives on stdin. Its top carries `COMMIT:`, `BRANCH:`, `BASE:`,
//! `RUN:`, `OWNER:`, `RETIRES:`, `FALSIFY:` and `SELF-REVIEW:` lines, and the text
//! below holds the bar's net table. The tool prints one line per key, `pass` or
//! `FAIL`, and exits 0 only when all nine pass. COMMIT has to be the head the
//! forge holds for BRANCH. BASE has to be an ancestor of COMMIT in the checkout
//! the tool runs in (it fetches nothing). RUN has to be a run of COMMIT on BRANCH
//! that succeeded, found with the lookup `scripts/wait-run` uses. The other four
//! have to be named, and a RETIRES of none needs its reason. A missing key is a
//! failure that names it.
//!
//! Every case runs the tool against a stand-in forge on a local port and a
//! scratch repository. The forge answers in the shapes the real one does; they
//! are the shapes `wait_run.rs` already holds.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::{Value, json};

const REPO: &str = "/api/v1/repos/homer/jojobot";
const BRANCH: &str = "dev9-2649-work";

/// The bar's net table as `bar size --net` prints it, in the report's prose.
const NET_TABLE: &str = "net lines by area since abc:   source    tests     text  area\n\
                         -4        0        0  crates/jojobot-bar\n\
                         -4        0        0  total; source plus tests -4\n";

fn tool() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check-report")
}

/// What the stand-in forge holds.
struct Forge {
    /// Branches and their heads.
    branches: Vec<(String, String)>,
    /// Runs: id, ref, commit, status.
    runs: Vec<(u64, String, String, String)>,
}

/// A stand-in forge on a free local port, answering until the test ends.
fn stand(forge: Forge) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("a local port");
    let addr = format!("http://{}", listener.local_addr().expect("an address"));
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut request = [0u8; 4096];
            let read = stream.read(&mut request).unwrap_or(0);
            let text = String::from_utf8_lossy(&request[..read]).to_string();
            let path = text.split_whitespace().nth(1).unwrap_or_default();
            let (code, body) = answer(&forge, path);
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
    if let Some(name) = rest.strip_prefix("/branches/") {
        return match forge.branches.iter().find(|(held, _)| held == name) {
            Some((name, head)) => (
                200,
                json!({"name": name, "commit": {"id": head}}).to_string(),
            ),
            None => (404, "404 page not found".into()),
        };
    }
    if let Some(query) = rest.strip_prefix("/actions/runs?") {
        let head = query
            .split('&')
            .find_map(|pair| pair.strip_prefix("head_sha="));
        let listed: Vec<Value> = forge
            .runs
            .iter()
            .filter(|(_, _, sha, _)| head.is_none_or(|head| head == sha))
            .map(|(id, branch, sha, status)| {
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
            })
            .collect();
        return (
            200,
            json!({"total_count": listed.len(), "workflow_runs": listed}).to_string(),
        );
    }
    (404, "404 page not found".into())
}

fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-check-report-{tag}"
    )));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Run git in `dir`, with no configuration of the caller's own reaching in.
fn git(dir: &Path, args: &[&str]) -> String {
    let ran = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Case")
        .env("GIT_AUTHOR_EMAIL", "case@example.invalid")
        .env("GIT_COMMITTER_NAME", "Case")
        .env("GIT_COMMITTER_EMAIL", "case@example.invalid")
        .output()
        .expect("git runs");
    assert!(
        ran.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&ran.stderr)
    );
    String::from_utf8_lossy(&ran.stdout).trim().to_string()
}

/// A scratch repository: `base` is the first commit, `head` follows it, and
/// `side` follows `base` on another line, so it is not an ancestor of `head`.
struct Repository {
    dir: PathBuf,
    base: String,
    head: String,
    side: String,
}

fn repository() -> Repository {
    let dir = scratch("repository");
    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["commit", "--allow-empty", "-m", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["commit", "--allow-empty", "-m", "head"]);
    let head = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["switch", "-c", "side", &base]);
    git(&dir, &["commit", "--allow-empty", "-m", "side"]);
    let side = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["switch", "main"]);
    Repository {
        dir,
        base,
        head,
        side,
    }
}

/// The forge as a clean report finds it: the branch at the head, and a run of
/// the head on that branch that succeeded.
fn green(repo: &Repository) -> Forge {
    Forge {
        branches: vec![(BRANCH.into(), repo.head.clone())],
        runs: vec![(700, BRANCH.into(), repo.head.clone(), "success".into())],
    }
}

/// A report whose header is `header` and whose prose follows it.
fn report(header: &[(&str, &str)]) -> String {
    let mut text = String::new();
    for (key, value) in header {
        text.push_str(&format!("{key}: {value}\n"));
    }
    text.push_str("\nThe change does what the card said.\nCOMMIT: 0000000\n");
    text.push_str(NET_TABLE);
    text
}

fn header(repo: &Repository) -> Vec<(&'static str, String)> {
    vec![
        ("COMMIT", repo.head.clone()),
        ("BRANCH", BRANCH.to_string()),
        ("BASE", repo.base.clone()),
        ("RUN", "700".to_string()),
        ("OWNER", "scripts/check-report".to_string()),
        ("RETIRES", "none: nothing was made redundant".to_string()),
        ("FALSIFY", "git grep -n x -- y".to_string()),
        ("SELF-REVIEW", "read the diff once".to_string()),
    ]
}

fn with<'a>(
    header: &'a [(&'static str, String)],
    key: &str,
    value: &'a str,
) -> Vec<(&'static str, &'a str)> {
    header
        .iter()
        .map(|(k, v)| (*k, if *k == key { value } else { v.as_str() }))
        .collect()
}

/// The tool on `text`, in `repo`, against the forge at `api`.
fn check(repo: &Repository, api: &str, text: &str) -> (i32, String, String) {
    let mut child = Command::new(tool())
        .current_dir(&repo.dir)
        .env("WAIT_RUN_API", api)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tool runs");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(text.as_bytes())
        .expect("the report goes in");
    let ran = child.wait_with_output().expect("it exits");
    (
        ran.status.code().expect("it exits"),
        String::from_utf8_lossy(&ran.stdout).to_string(),
        String::from_utf8_lossy(&ran.stderr).to_string(),
    )
}

/// The `FAIL` lines of an answer.
fn failures(out: &str) -> Vec<&str> {
    out.lines().filter(|l| l.starts_with("FAIL")).collect()
}

/// **A header whose four facts hold passes, one line per key, and exits zero.**
/// The prose after the header, even a line that looks like a key, is not read.
#[test]
fn a_header_whose_facts_hold_passes_every_key_and_exits_zero() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let (code, out, err) = check(&repo, &api, &report(&with(&head, "RUN", "700")));
    assert_eq!(code, 0, "{out}\n{err}");
    for key in [
        "COMMIT",
        "BRANCH",
        "BASE",
        "RUN",
        "OWNER",
        "RETIRES",
        "FALSIFY",
        "SELF-REVIEW",
        "NET",
    ] {
        let lines: Vec<&str> = out.lines().filter(|l| l.contains(key)).collect();
        assert!(
            lines.iter().any(|l| l.starts_with("pass")),
            "{key} has no pass line: {out}"
        );
    }
    assert!(failures(&out).is_empty(), "{out}");
}

/// **A commit that is not the head of the branch fails, and only that key
/// does.** A branch that moved on after the report was written reads this way.
#[test]
fn a_commit_that_is_not_the_head_of_the_branch_fails() {
    let repo = repository();
    let mut forge = green(&repo);
    forge.branches = vec![(BRANCH.into(), repo.side.clone())];
    let api = stand(forge);
    let head = header(&repo);
    let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", "700")));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("COMMIT"), "{out}");
}

/// **A base that is not an ancestor of the commit fails.** The side commit
/// shares history with the head and is not part of it.
#[test]
fn a_base_that_is_not_an_ancestor_of_the_commit_fails() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let (code, out, _) = check(&repo, &api, &report(&with(&head, "BASE", &repo.side)));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("BASE"), "{out}");
}

/// **A base the checkout does not hold fails and says so**, and the tool does
/// not fetch to find it.
#[test]
fn a_base_the_checkout_does_not_hold_fails() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let unknown = "1".repeat(40);
    let (code, out, _) = check(&repo, &api, &report(&with(&head, "BASE", &unknown)));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("BASE"), "{out}");
}

/// **A run that did not succeed fails, and so does a run of another commit or
/// another branch, or a run the forge does not hold.**
#[test]
fn a_run_that_is_not_a_green_run_of_the_commit_on_the_branch_fails() {
    let repo = repository();
    let head = header(&repo);
    let cases: Vec<(&str, Forge)> = vec![
        ("failed", {
            let mut forge = green(&repo);
            forge.runs[0].3 = "failure".into();
            forge
        }),
        ("other branch", {
            let mut forge = green(&repo);
            forge.runs[0].1 = "dev9-other-work".into();
            forge
        }),
        ("other commit", {
            let mut forge = green(&repo);
            forge.runs[0].2 = repo.side.clone();
            forge
        }),
        ("no such run", {
            let mut forge = green(&repo);
            forge.runs[0].0 = 701;
            forge
        }),
    ];
    for (name, forge) in cases {
        let api = stand(forge);
        let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", "700")));
        assert_eq!(code, 1, "{name}: {out}");
        let failed = failures(&out);
        assert_eq!(failed.len(), 1, "{name}: {out}");
        assert!(failed[0].contains("RUN"), "{name}: {out}");
    }
}

/// **A missing key is a failure that names it.** The prose below the header is
/// not searched for it: a `COMMIT:` line there does not count.
#[test]
fn a_missing_key_fails_and_names_it() {
    let repo = repository();
    let api = stand(green(&repo));
    for missing in [
        "COMMIT",
        "BRANCH",
        "BASE",
        "RUN",
        "OWNER",
        "RETIRES",
        "FALSIFY",
        "SELF-REVIEW",
    ] {
        let owned = header(&repo);
        let kept: Vec<(&str, &str)> = owned
            .iter()
            .filter(|(k, _)| *k != missing)
            .map(|(k, v)| (*k, v.as_str()))
            .collect();
        let (code, out, _) = check(&repo, &api, &report(&kept));
        assert_eq!(code, 1, "{missing}: {out}");
        assert!(
            failures(&out)
                .iter()
                .any(|l| l.contains(missing) && l.contains("missing")),
            "{missing} is not named as missing: {out}"
        );
    }
}

/// **A report with no net table fails NET, and a RETIRES of `none` with no reason
/// fails RETIRES.**
#[test]
fn a_missing_net_table_and_a_bare_none_fail_like_a_missing_key() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let text = report(&with(&head, "RUN", "700")).replace(NET_TABLE, "");
    let (code, out, _) = check(&repo, &api, &text);
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out).iter().any(|l| l.contains("NET")), "{out}");
    for none in ["none", "None", "NONE."] {
        let bare = report(&with(&head, "RETIRES", none));
        let (code, out, _) = check(&repo, &api, &bare);
        assert_eq!(code, 1, "{none}: {out}");
        assert!(
            failures(&out).iter().any(|l| l.contains("RETIRES")),
            "{none}: {out}"
        );
    }
}

/// **Each half of the net table is needed.** A report with the heading only, and
/// one with the total line only, each fail NET.
#[test]
fn the_net_table_needs_its_heading_and_its_total_like_a_missing_key() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let (heading, total) = NET_TABLE.split_at(NET_TABLE.find("-4").unwrap());
    for (kept, dropped) in [(heading, total), (total, heading)] {
        let text = report(&with(&head, "RUN", "700")).replace(NET_TABLE, kept);
        assert_ne!(text, report(&with(&head, "RUN", "700")), "{dropped}");
        let (code, out, _) = check(&repo, &api, &text);
        assert_eq!(code, 1, "{out}");
        assert!(failures(&out).iter().any(|l| l.contains("NET")), "{out}");
    }
}

/// **A preamble above the header hides it**: the header is the top of the report,
/// so the keys below a paragraph are missing.
#[test]
fn a_preamble_above_the_header_leaves_the_keys_missing_like_a_missing_key() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let text = format!(
        "A line of preamble.\n{}",
        report(&with(&head, "RUN", "700"))
    );
    let (code, out, _) = check(&repo, &api, &text);
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out)
            .iter()
            .any(|l| l.contains("COMMIT") && l.contains("missing")),
        "{out}"
    );
}

/// **A call with no API root exits four and names the variable**, like
/// `scripts/wait-run`.
#[test]
fn a_call_with_no_api_root_exits_four() {
    let repo = repository();
    let mut child = Command::new(tool())
        .current_dir(&repo.dir)
        .env_remove("WAIT_RUN_API")
        .env_remove("GITHUB_ACTIONS")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tool runs");
    drop(child.stdin.take());
    let ran = child.wait_with_output().expect("it exits");
    assert_eq!(ran.status.code(), Some(4));
    assert!(String::from_utf8_lossy(&ran.stderr).contains("WAIT_RUN_API"));
}

/// Identical labels name separate directories without consulting a clock.
#[test]
fn scratch_names_are_distinct_on_concurrent_calls() {
    let barrier = std::sync::Barrier::new(2);
    let paths = std::thread::scope(|scope| {
        let calls: Vec<_> = (0..2)
            .map(|_| {
                scope.spawn(|| {
                    barrier.wait();
                    scratch("same-tick")
                })
            })
            .collect();
        calls
            .into_iter()
            .map(|c| c.join().expect("scratch call"))
            .collect::<Vec<_>>()
    });
    for path in &paths {
        let _ = std::fs::remove_dir_all(path);
    }
    assert_ne!(
        paths[0], paths[1],
        "two threads received one scratch directory"
    );
}
