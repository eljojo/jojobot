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
    /// Whether the run records carry no `index_in_repo`, as an older forge's do.
    numberless: bool,
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
                    "index_in_repo": if forge.numberless { Value::Null } else { json!(id - 2) },
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
        numberless: false,
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
        ("RUN", "698".to_string()),
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
    check_card(repo, api, text, None)
}

/// The tool on `text` with `--card FILE`, the file holding `card` as JSON.
fn check_card(
    repo: &Repository,
    api: &str,
    text: &str,
    card: Option<&Value>,
) -> (i32, String, String) {
    let mut command = Command::new(tool());
    if let Some(card) = card {
        let file = scratch("card").join("card.json");
        std::fs::write(&file, card.to_string()).expect("the card file");
        command.arg("--card").arg(file);
    }
    let mut child = command
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
    let (code, out, err) = check(&repo, &api, &report(&with(&head, "RUN", "698")));
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
    let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", "698")));
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
        let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", "698")));
        assert_eq!(code, 1, "{name}: {out}");
        let failed = failures(&out);
        assert_eq!(failed.len(), 1, "{name}: {out}");
        assert!(failed[0].contains("RUN"), "{name}: {out}");
    }
}

/// **The forge id of the run is not its number, and the failure says which
/// number to write.** `scripts/wait-run` prints the run's number first and its
/// forge id in brackets; the report's RUN is the number.
#[test]
fn the_forge_id_written_as_the_run_fails_and_names_the_number_to_use() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", "700")));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("RUN"), "{out}");
    // The structure of the message, not three digits a fixture sha could hold.
    assert!(
        failed[0].contains("700 is the forge id of run #698; write RUN: 698"),
        "the failure names the number to use: {out}"
    );
}

/// **A run record with no number cannot be named by one**, so a report's RUN
/// fails and says no run has that number, whether the line wrote the number or
/// the forge id.
#[test]
fn a_run_record_with_no_number_fails_a_report_that_names_one() {
    let repo = repository();
    let mut forge = green(&repo);
    forge.numberless = true;
    let api = stand(forge);
    let head = header(&repo);
    for written in ["698", "700"] {
        let (code, out, _) = check(&repo, &api, &report(&with(&head, "RUN", written)));
        assert_eq!(code, 1, "{written}: {out}");
        let failed = failures(&out);
        assert_eq!(failed.len(), 1, "{written}: {out}");
        assert!(
            failed[0].contains(&format!("{written} is not a run number of")),
            "{written}: {out}"
        );
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

/// **A FALSIFY whose only evidence is a line window or a count fails, and the
/// refusal names the shape.** A line range, a count and a bare `file:LINE` go
/// stale the day the code above them moves; falsifying by name does not.
#[test]
fn a_falsify_that_is_only_a_line_window_or_a_count_fails_naming_the_shape() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    for (falsify, shape) in [
        ("sed -n '475,500p' crates/x/src/y.rs", "sed -n"),
        ("grep -c keyed crates/x/src/y.rs", "grep -c"),
        ("git grep -c keyed -- crates", "grep -c"),
        ("wc -l crates/x/src/y.rs", "wc -l"),
        ("head -40 crates/x/src/y.rs", "head"),
        ("tail -n 20 crates/x/src/y.rs", "tail"),
        ("crates/x/src/y.rs:475 never runs the check", "file:LINE"),
    ] {
        let (code, out, _) = check(&repo, &api, &report(&with(&head, "FALSIFY", falsify)));
        assert_eq!(code, 1, "{falsify}: {out}");
        assert!(
            failures(&out)
                .iter()
                .any(|l| l.contains("FALSIFY") && l.contains(shape)),
            "{falsify}: the refusal does not name {shape}: {out}"
        );
    }
}

/// **A FALSIFY that carries a window or a count and also falsifies by name
/// passes.** The name is what survives the code moving.
#[test]
fn a_falsify_that_also_greps_for_a_name_passes() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    for falsify in [
        "git grep -n holder_key -- crates/x/src/y.rs; sed -n '475,500p' crates/x/src/y.rs",
        "git grep -n keyed crates/x/src/y.rs | head -5",
        "crates/x/src/y.rs:475 holds a keyed flag; git grep -n keyed crates",
    ] {
        let (code, out, err) = check(&repo, &api, &report(&with(&head, "FALSIFY", falsify)));
        assert_eq!(code, 0, "{falsify}: {out}\n{err}");
    }
}

/// The card's own keys, as `recall` returns them under `fields` for one item.
fn card() -> Value {
    json!({
        "owner_module": "scripts/check-report",
        "retires": "the retyped header lines, because the card carries them",
        "falsify": "git rev-list --count HEAD prints 9 on main abc1234; done when it prints at least 2",
        "self_review": "read the diff once",
    })
}

/// A header of the four facts the forge and the checkout decide, and nothing else.
fn facts_only(repo: &Repository) -> Vec<(&'static str, String)> {
    header(repo).into_iter().take(4).collect()
}

fn as_pairs<'a>(header: &'a [(&'static str, String)]) -> Vec<(&'static str, &'a str)> {
    header.iter().map(|(k, v)| (*k, v.as_str())).collect()
}

/// **A header of COMMIT, BRANCH, BASE and RUN, given the card, passes OWNER,
/// RETIRES, FALSIFY and SELF-REVIEW from the card's keys.** The report types none of
/// the four.
#[test]
fn a_card_fills_the_four_named_keys_a_header_lacks() {
    let repo = repository();
    let api = stand(green(&repo));
    let facts = facts_only(&repo);
    let text = report(&as_pairs(&facts));
    let (code, out, err) = check_card(&repo, &api, &text, Some(&card()));
    assert_eq!(code, 0, "{out}\n{err}");
    for key in ["OWNER", "RETIRES", "FALSIFY", "SELF-REVIEW"] {
        assert!(
            out.lines()
                .any(|l| l.starts_with("pass") && l.contains(key)),
            "{key} has no pass line: {out}"
        );
    }
    let (code, out, _) = check_card(&repo, &api, &text, None);
    assert_eq!(
        code, 1,
        "the same report with no card must still fail: {out}"
    );
}

/// **A value the header holds wins over the card's, in both directions.** A good
/// header FALSIFY passes beside a card FALSIFY that would fail; a bare `none` in
/// the header fails beside a card RETIRES that would pass.
#[test]
fn a_header_value_wins_over_the_cards() {
    let repo = repository();
    let api = stand(green(&repo));
    let mut facts = facts_only(&repo);
    facts.push(("FALSIFY", "git grep -n x -- y".to_string()));
    let mut bad_falsify = card();
    bad_falsify["falsify"] =
        json!("git rev-list --count HEAD | wc -l prints 1; done when it prints exactly 1");
    let (code, out, err) = check_card(&repo, &api, &report(&as_pairs(&facts)), Some(&bad_falsify));
    assert_eq!(code, 0, "{out}\n{err}");

    let mut facts = facts_only(&repo);
    facts.push(("RETIRES", "none".to_string()));
    let (code, out, _) = check_card(&repo, &api, &report(&as_pairs(&facts)), Some(&card()));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out).iter().any(|l| l.contains("RETIRES")),
        "{out}"
    );
}

/// **Each of the four fails from the card as it fails from a header.** A missing
/// or blank value is missing; a RETIRES of `none` needs its reason; a FALSIFY that is
/// only a line window names its shape.
#[test]
fn each_named_key_fails_from_the_card_as_from_a_header() {
    let repo = repository();
    let api = stand(green(&repo));
    let facts = facts_only(&repo);
    let text = report(&as_pairs(&facts));
    for (field, key) in [
        ("owner_module", "OWNER"),
        ("retires", "RETIRES"),
        ("falsify", "FALSIFY"),
        ("self_review", "SELF-REVIEW"),
    ] {
        for blank in [None, Some("   ")] {
            let mut held = card();
            match blank {
                None => {
                    held.as_object_mut().expect("an object").remove(field);
                }
                Some(blank) => held[field] = json!(blank),
            }
            let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
            assert_eq!(code, 1, "{field} {blank:?}: {out}");
            assert!(
                failures(&out)
                    .iter()
                    .any(|l| l.contains(key) && l.contains("missing")),
                "{field} {blank:?}: {key} is not named as missing: {out}"
            );
        }
    }
    let mut held = card();
    held["retires"] = json!("none");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out).iter().any(|l| l.contains("RETIRES")),
        "{out}"
    );
    let mut held = card();
    held["falsify"] = json!("sed -n '475,500p' crates/x/src/y.rs");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out)
            .iter()
            .any(|l| l.contains("FALSIFY") && l.contains("sed -n")),
        "{out}"
    );
}

/// **The card's `review` stands for SELF-REVIEW only once a review happened.** A
/// card the line has just reset to `review: none` holds no review; `self_review`
/// wins over `review` when both are there.
#[test]
fn the_cards_review_counts_for_self_review_unless_it_is_none() {
    let repo = repository();
    let api = stand(green(&repo));
    let facts = facts_only(&repo);
    let text = report(&as_pairs(&facts));
    let mut held = card();
    held.as_object_mut()
        .expect("an object")
        .remove("self_review");
    held["review"] = json!("none");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out)
            .iter()
            .any(|l| l.contains("SELF-REVIEW") && l.contains("missing")),
        "{out}"
    );
    held["review"] = json!("clean");
    let (code, out, err) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 0, "{out}\n{err}");
    let mut both = card();
    both["review"] = json!("none");
    let (code, out, err) = check_card(&repo, &api, &text, Some(&both));
    assert_eq!(code, 0, "{out}\n{err}");
}

/// **A review report is checked as it was with a card beside it.** The card's keys
/// add no check the review did not ask for, so no forge is needed.
#[test]
fn a_card_adds_no_key_to_a_review_report() {
    let files = files_repository();
    let lines: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    let (code, out, err) = check_card(
        &files.repo,
        "http://127.0.0.1:1/api/v1/repos/homer/jojobot",
        &review(&files, &lines),
        Some(&card()),
    );
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(failures(&out).is_empty(), "{out}");
}

/// The card with `falsify` set to `falsify`.
fn card_falsifying(falsify: &str) -> Value {
    let mut held = card();
    held["falsify"] = json!(falsify);
    held
}

/// DONE's line in an answer.
fn done_line(out: &str) -> &str {
    out.lines()
        .find(|l| l.contains("DONE"))
        .unwrap_or_else(|| panic!("no DONE line: {out}"))
}

/// **A card whose FALSIFY states a done line is checked against the checkout it
/// runs in.** The command runs, its number meets the comparison or it does not,
/// and a miss exits 1 with the number the command printed. The scratch repository
/// holds two commits, so `git rev-list --count HEAD` prints 2.
#[test]
fn a_done_line_the_head_meets_passes_and_one_it_misses_fails_with_the_number() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    for (clause, met) in [
        ("under 3", true),
        ("under 2", false),
        ("over 1", true),
        ("over 2", false),
        ("at most 2", true),
        ("at most 1", false),
        ("at least 2", true),
        ("at least 3", false),
        ("exactly 2", true),
        ("exactly 3", false),
    ] {
        let falsify = format!(
            "git rev-list --count HEAD prints 9 on main abc1234; done when it prints {clause}"
        );
        let (code, out, err) = check_card(&repo, &api, &text, Some(&card_falsifying(&falsify)));
        let line = done_line(&out);
        if met {
            assert_eq!(code, 0, "{clause}: {out}\n{err}");
            assert!(line.starts_with("pass"), "{clause}: {line}");
        } else {
            assert_eq!(code, 1, "{clause}: {out}");
            assert!(
                line.starts_with("FAIL") && line.contains("printed 2") && line.contains(clause),
                "{clause}: {line}"
            );
        }
    }
}

/// **A done line the machine cannot run is refused as unrunnable, naming why.** A
/// clause with no comparison, a command that fails and a command that prints no
/// number each fail. The failing command is the shape a card carries, read in a
/// checkout that does not hold the file.
#[test]
fn a_done_line_the_machine_cannot_run_fails_as_unrunnable() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    for falsify in [
        "git rev-list --count HEAD prints 9; done when the design names each home",
        "git show HEAD:crates/jojobot/tests/ui.rs | wc -l prints 7799 on main 0a9e888d4; done when it prints under 5000 and the design names each region's home",
        "git log -1 --format=%s prints a subject; done when it prints under 5",
    ] {
        let (code, out, _) = check_card(&repo, &api, &text, Some(&card_falsifying(falsify)));
        assert_eq!(code, 1, "{falsify}: {out}");
        let line = done_line(&out);
        assert!(
            line.starts_with("FAIL") && line.contains("unrunnable"),
            "{falsify}: {line}"
        );
    }
}

/// **A done line that counts matches passes when nothing matches.** `git grep` exits 1
/// on no match, and under pipefail so does the pipeline, but its stderr is empty and
/// the count it prints is 0: a deletion card's "done when it prints 0" must be able
/// to pass. A failure with an error on stderr stays unrunnable.
#[test]
fn a_done_line_counting_zero_matches_passes_and_an_error_still_fails() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let none = card_falsifying(
        "git grep -n zeta -- . | wc -l prints 3 on main abc1234; done when it prints exactly 0",
    );
    let (code, out, err) = check_card(&repo, &api, &text, Some(&none));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(done_line(&out).starts_with("pass"), "{out}");
    let missing = card_falsifying(
        "git show HEAD:nowhere.rs | wc -l prints 3 on main abc1234; done when it prints exactly 0",
    );
    let (code, out, _) = check_card(&repo, &api, &text, Some(&missing));
    assert_eq!(code, 1, "{out}");
    assert!(done_line(&out).contains("unrunnable"), "{out}");
}

/// **DONE runs at the head the report names, or refuses.** A checkout whose HEAD is
/// not the header's COMMIT would judge another tree, so DONE fails and names both.
#[test]
fn a_done_line_run_in_another_checkout_than_the_head_is_refused() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let held = card_falsifying(
        "git rev-list --count HEAD prints 9 on main abc1234; done when it prints at least 1",
    );
    git(&repo.dir, &["switch", "side"]);
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    let line = done_line(&out);
    assert!(
        line.starts_with("FAIL") && line.contains(&repo.side) && line.contains(&repo.head),
        "{line}"
    );
}

/// **The card's `done_line` key is the done line when it is present, and `falsify`
/// may then hold prose.** A line's report writes `falsify`, so the PM's runnable
/// clause lives in its own key. Paired: a `done_line` the head misses fails with the
/// number, and a card with no `done_line` still reads its clause from `falsify`.
#[test]
fn a_done_line_key_wins_over_the_falsify_key() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let mut held = card_falsifying("git grep -n card -- scripts/check-report prints the lines");
    held["done_line"] = json!("git rev-list --count HEAD prints 9; done when it prints exactly 2");
    let (code, out, err) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(done_line(&out).starts_with("pass"), "{out}");

    held["done_line"] = json!("git rev-list --count HEAD prints 9; done when it prints exactly 3");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    assert!(done_line(&out).contains("printed 2"), "{out}");

    let (code, out, err) = check_card(&repo, &api, &text, Some(&card()));
    assert_eq!(
        code, 0,
        "no done_line reads falsify as before: {out}\n{err}"
    );
}

/// **A `done_line` that holds a clause wins over a `falsify` that holds a different
/// one.** A reading that takes `done_line` only when `falsify` has no clause would
/// pass the case above. Here both keys carry a clause and the head meets only the
/// `done_line`'s.
#[test]
fn a_done_line_wins_over_a_falsify_with_its_own_clause() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let mut held =
        card_falsifying("git rev-list --count HEAD prints 9; done when it prints exactly 3");
    held["done_line"] = json!("git rev-list --count HEAD prints 9; done when it prints exactly 2");
    let (code, out, err) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(done_line(&out).starts_with("pass"), "{out}");
}

/// **The refusal names the key the done line was read from.** A `done_line` with no
/// clause is refused naming `done_line`, even where `falsify` holds a clause; a card
/// with no `done_line` whose `falsify` has none is refused naming `falsify`.
#[test]
fn an_unrunnable_done_line_names_the_key_it_came_from() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let mut held =
        card_falsifying("git rev-list --count HEAD prints 9; done when it prints exactly 2");
    held["done_line"] = json!("git rev-list --count HEAD prints 9");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&held));
    assert_eq!(code, 1, "{out}");
    let line = done_line(&out);
    assert!(
        line.starts_with("FAIL") && line.contains("done_line") && !line.contains("falsify"),
        "{line}"
    );

    let bare = card_falsifying("git grep -n card -- scripts/check-report");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&bare));
    assert_eq!(code, 1, "{out}");
    let line = done_line(&out);
    assert!(
        line.contains("falsify") && !line.contains("done_line"),
        "{line}"
    );
}

/// **A card whose FALSIFY states no done line is refused as unrunnable.** The clause
/// is required, so a card without it cannot be carried. A header with no card holds
/// no DONE line.
#[test]
fn a_card_with_no_done_line_fails_as_unrunnable() {
    let repo = repository();
    let api = stand(green(&repo));
    let text = report(&as_pairs(&facts_only(&repo)));
    let bare = card_falsifying("git grep -n card -- scripts/check-report");
    let (code, out, _) = check_card(&repo, &api, &text, Some(&bare));
    assert_eq!(code, 1, "{out}");
    let line = done_line(&out);
    assert!(
        line.starts_with("FAIL") && line.contains("unrunnable") && line.contains("done when"),
        "{line}"
    );
    let full = report(&as_pairs(&header(&repo)));
    let (code, out, err) = check(&repo, &api, &full);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(!out.contains("DONE"), "{out}");
}

/// **A report with no net table fails NET, and a RETIRES of `none` with no reason
/// fails RETIRES.**
#[test]
fn a_missing_net_table_and_a_bare_none_fail_like_a_missing_key() {
    let repo = repository();
    let api = stand(green(&repo));
    let head = header(&repo);
    let text = report(&with(&head, "RUN", "698")).replace(NET_TABLE, "");
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
        let text = report(&with(&head, "RUN", "698")).replace(NET_TABLE, kept);
        assert_ne!(text, report(&with(&head, "RUN", "698")), "{dropped}");
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
        report(&with(&head, "RUN", "698"))
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

/// A scratch repository whose range `base..head` changes three files.
struct Files {
    repo: Repository,
}

const CHANGED: [&str; 3] = ["src/one.rs", "src/two.rs", "docs/three md.txt"];

fn files_repository() -> Files {
    let dir = scratch("files");
    git(&dir, &["init", "-b", "main"]);
    git(&dir, &["commit", "--allow-empty", "-m", "base"]);
    let base = git(&dir, &["rev-parse", "HEAD"]);
    for path in CHANGED {
        let full = dir.join(path);
        std::fs::create_dir_all(full.parent().expect("a parent")).expect("a directory");
        std::fs::write(&full, "changed\n").expect("a file");
    }
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-m", "head"]);
    let head = git(&dir, &["rev-parse", "HEAD"]);
    Files {
        repo: Repository {
            dir,
            base,
            head,
            side: String::new(),
        },
    }
}

/// A review report: the REVIEW header line, and the COVERAGE block of `lines`,
/// with prose after both.
fn review(files: &Files, lines: &[String]) -> String {
    format!(
        "REVIEW: {}..{}\n\nThe diff reads as the card said.\nCOVERAGE:\n{}\n\nFindings: none.\n",
        files.repo.base,
        files.repo.head,
        lines.join("\n")
    )
}

fn read_by(path: &str) -> String {
    format!("{path} read-by a reader")
}

/// A review needs no forge: the tool is given an API root that nothing answers.
fn check_review(files: &Files, text: &str) -> (i32, String, String) {
    check(
        &files.repo,
        "http://127.0.0.1:1/api/v1/repos/homer/jojobot",
        text,
    )
}

/// **A review whose coverage block marks every file of its range read passes.**
/// The positive every refusal below depends on. A path with a space in it is one
/// of the three, and the report asks for no key of a line report.
#[test]
fn a_review_that_marks_every_changed_file_read_passes() {
    let files = files_repository();
    let lines: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    let (code, out, err) = check_review(&files, &review(&files, &lines));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.lines().any(|l| l.starts_with("pass REVIEW")), "{out}");
    assert!(out.lines().any(|l| l.starts_with("pass COVERAGE")), "{out}");
    assert!(failures(&out).is_empty(), "{out}");
}

/// **A changed file with no line fails, and the failure names it.**
#[test]
fn a_changed_file_with_no_coverage_line_fails_naming_it() {
    let files = files_repository();
    let lines = vec![read_by(CHANGED[0]), read_by(CHANGED[2])];
    let (code, out, _) = check_review(&files, &review(&files, &lines));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains(CHANGED[1]), "{out}");
}

/// **A file marked UNREAD fails, naming it and the reason.**
#[test]
fn a_file_marked_unread_fails_naming_it() {
    let files = files_repository();
    let lines = vec![
        read_by(CHANGED[0]),
        format!("{} UNREAD ran out of time", CHANGED[1]),
        read_by(CHANGED[2]),
    ];
    let (code, out, _) = check_review(&files, &review(&files, &lines));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(
        failed[0].contains(CHANGED[1]) && failed[0].contains("ran out of time"),
        "{out}"
    );
}

/// **A line that names a file outside the range fails, naming it**, even when
/// every file of the range is marked read.
#[test]
fn a_line_naming_a_file_outside_the_range_fails_naming_it() {
    let files = files_repository();
    let mut lines: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    lines.push(read_by("src/never_changed.rs"));
    let (code, out, _) = check_review(&files, &review(&files, &lines));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("src/never_changed.rs"), "{out}");
}

/// **A line with no reader, a line of neither form, a report with no coverage
/// block and a range that is not BASE..H each fail.**
#[test]
fn a_malformed_coverage_fails() {
    let files = files_repository();
    let all: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();

    let mut nameless = all.clone();
    nameless[0] = format!("{} read-by", CHANGED[0]);
    let (code, out, _) = check_review(&files, &review(&files, &nameless));
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains("names no reader"), "{out}");

    let mut odd = all.clone();
    odd.push("something else entirely".to_string());
    let (code, out, _) = check_review(&files, &review(&files, &odd));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out)[0].contains("something else entirely"),
        "{out}"
    );

    let no_block = format!(
        "REVIEW: {}..{}\n\nProse only.\n",
        files.repo.base, files.repo.head
    );
    let (code, out, _) = check_review(&files, &no_block);
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains("COVERAGE"), "{out}");

    let (code, out, _) = check_review(&files, "REVIEW: not-a-range\n\nCOVERAGE:\nx read-by y\n");
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains("BASE..H"), "{out}");
}

/// **A review's range must be one this checkout holds**, and the tool fetches
/// nothing to find it.
#[test]
fn a_range_the_checkout_does_not_hold_fails() {
    let files = files_repository();
    let unknown = "1".repeat(40);
    let text = format!(
        "REVIEW: {unknown}..{}\n\nCOVERAGE:\n{}\n",
        files.repo.head,
        read_by(CHANGED[0])
    );
    let (code, out, _) = check_review(&files, &text);
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains(&unknown), "{out}");
}

/// **In review mode the four keys of a line report are checked when the header
/// holds them.** A wrong BASE beside a complete coverage block still fails.
#[test]
fn a_review_that_carries_a_wrong_key_fails_on_that_key() {
    let files = files_repository();
    let lines: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    let forge = Forge {
        branches: vec![(BRANCH.into(), files.repo.head.clone())],
        runs: vec![(
            700,
            BRANCH.into(),
            files.repo.head.clone(),
            "success".into(),
        )],
        numberless: false,
    };
    let api = stand(forge);
    let text = format!(
        "COMMIT: {head}\nBRANCH: {BRANCH}\nBASE: {wrong}\nRUN: 698\nREVIEW: {base}..{head}\n\nCOVERAGE:\n{}\n",
        lines.join("\n"),
        head = files.repo.head,
        base = files.repo.base,
        wrong = "1".repeat(40),
    );
    let (code, out, _) = check(&files.repo, &api, &text);
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains("BASE"), "{out}");
}

/// A scoped review report: the coverage block opens with the lens's inventory, the
/// command that lists the files of its scope.
fn scoped(files: &Files, inventory: &str, lines: &[String]) -> String {
    let mut block = vec![format!("INVENTORY: {inventory}")];
    block.extend(lines.iter().cloned());
    review(files, &block)
}

/// **A lens that read every file of its own scope passes, and the files of the
/// range outside its scope are not demanded.** The inventory is a git command the
/// report names; `docs/three md.txt` is in the range and outside the scope.
#[test]
fn a_scoped_review_that_read_its_whole_scope_passes() {
    let files = files_repository();
    let inventory = format!(
        "git diff --name-only {}..{} -- src",
        files.repo.base, files.repo.head
    );
    let lines = vec![read_by(CHANGED[0]), read_by(CHANGED[1])];
    let (code, out, err) = check_review(&files, &scoped(&files, &inventory, &lines));
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(
        out.lines()
            .any(|l| l.starts_with("pass COVERAGE") && l.contains("scope")),
        "{out}"
    );
}

/// **One file of the scope with no line fails, naming it**, and so does one marked
/// UNREAD: the scoped gate is as strict inside the scope as the whole-range gate.
#[test]
fn a_scoped_review_fails_naming_a_scope_file_it_did_not_read() {
    let files = files_repository();
    let inventory = format!(
        "git diff --name-only {}..{} -- src",
        files.repo.base, files.repo.head
    );
    let (code, out, _) = check_review(&files, &scoped(&files, &inventory, &[read_by(CHANGED[0])]));
    assert_eq!(code, 1, "{out}");
    let failed = failures(&out);
    assert_eq!(failed.len(), 1, "{out}");
    assert!(failed[0].contains(CHANGED[1]), "{out}");

    let unread = vec![
        read_by(CHANGED[0]),
        format!("{} UNREAD ran out of time", CHANGED[1]),
    ];
    let (code, out, _) = check_review(&files, &scoped(&files, &inventory, &unread));
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains(CHANGED[1]), "{out}");
}

/// **An inventory that names a file outside the range fails, naming it.** The file
/// is staged and not committed, so `git ls-files` lists it and the range does not.
#[test]
fn a_scoped_review_whose_inventory_leaves_the_range_fails_naming_the_file() {
    let files = files_repository();
    std::fs::write(files.repo.dir.join("src/staged.rs"), "staged\n").expect("a file");
    git(&files.repo.dir, &["add", "src/staged.rs"]);
    let mut lines: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    lines.push(read_by("src/staged.rs"));
    let (code, out, _) = check_review(&files, &scoped(&files, "git ls-files -- src", &lines));
    assert_eq!(code, 1, "{out}");
    assert!(
        failures(&out)
            .iter()
            .any(|l| l.contains("src/staged.rs") && l.contains("INVENTORY")),
        "{out}"
    );
}

/// **An inventory is a read-only git listing and nothing else**, and one that
/// lists nothing proves nothing: each is refused, so a report cannot name a command
/// to run or an empty scope to pass.
#[test]
fn an_inventory_that_is_not_a_git_listing_or_lists_nothing_is_refused() {
    let files = files_repository();
    let all: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    for inventory in [
        "touch ran-by-the-report".to_string(),
        "git push forgejo main".to_string(),
        format!(
            "git diff --name-only {}..{} -- nothing/here",
            files.repo.base, files.repo.head
        ),
    ] {
        let (code, out, _) = check_review(&files, &scoped(&files, &inventory, &all));
        assert_eq!(code, 1, "{inventory}: {out}");
        assert!(
            failures(&out).iter().any(|l| l.contains("INVENTORY")),
            "{inventory}: {out}"
        );
    }
    assert!(
        !files.repo.dir.join("ran-by-the-report").exists(),
        "the report's command ran"
    );
}

/// **An inventory only lists: an option that writes a file is refused and nothing is
/// written.** `git diff --output=FILE` writes its diff to FILE, so the report could
/// make the tool write. The same listing without the option, and a path after `--`
/// that merely looks like the option, pass.
#[test]
fn an_inventory_option_that_writes_a_file_is_refused() {
    let files = files_repository();
    let all: Vec<String> = CHANGED.iter().map(|path| read_by(path)).collect();
    let written = files.repo.dir.join("written-by-the-report");
    for option in [
        format!("--output={}", written.display()),
        format!("--output {}", written.display()),
    ] {
        let inventory = format!(
            "git diff {option} --name-only {}..{}",
            files.repo.base, files.repo.head
        );
        let (code, out, _) = check_review(&files, &scoped(&files, &inventory, &all));
        assert_eq!(code, 1, "{inventory}: {out}");
        assert!(
            failures(&out)
                .iter()
                .any(|l| l.contains("INVENTORY") && l.contains("--output")),
            "{inventory}: {out}"
        );
        assert!(!written.exists(), "{inventory}: the report wrote a file");
    }
    let plain = format!(
        "git diff --name-only {}..{} -- src",
        files.repo.base, files.repo.head
    );
    let lines = vec![read_by(CHANGED[0]), read_by(CHANGED[1])];
    let (code, out, err) = check_review(&files, &scoped(&files, &plain, &lines));
    assert_eq!(code, 0, "{out}\n{err}");
}

/// **With no inventory the whole-range gate stands**: a block that reads two of the
/// three changed files fails naming the third.
#[test]
fn a_review_with_no_inventory_still_demands_the_whole_range() {
    let files = files_repository();
    let lines = vec![read_by(CHANGED[0]), read_by(CHANGED[1])];
    let (code, out, _) = check_review(&files, &review(&files, &lines));
    assert_eq!(code, 1, "{out}");
    assert!(failures(&out)[0].contains(CHANGED[2]), "{out}");
}
