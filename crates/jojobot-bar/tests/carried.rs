//! **`make carried` says what a carry would move, from the repository alone.**
//!
//! The tool prints three sections about the checkout it is run in: what the
//! current branch holds that `main` does not, what `main` holds that
//! `origin/main` does not, and whether `main` has moved on since the branch left
//! it. Every case here runs it against a scratch repository whose history is
//! built by the case, so the expected lines are known before the tool runs, and
//! reads the sections by their headers rather than by position.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The tool, resolved from this crate rather than from wherever a caller
/// happened to stand.
fn tool() -> PathBuf {
    PathBuf::new()
        .join(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/carried")
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-carried-{tag}-{}-{nanos}",
        std::process::id()
    ));
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

fn commit(dir: &Path, subject: &str) {
    git(dir, &["commit", "--allow-empty", "-m", subject]);
}

/// A repository with a known history:
///
/// ```text
/// main:    first on main - second on main - third on main
/// work:                     \- work one - work two
/// ```
///
/// `origin/main` is at the first commit, and `work` left `main` at the second,
/// so `main` has moved on by one commit and has two that `origin/main` lacks.
/// The remote `origin` points at a path that does not exist: a tool that
/// fetched would fail, and the case would show it.
fn a_repository_with_a_known_history(tag: &str) -> PathBuf {
    let dir = scratch(tag);
    git(&dir, &["init", "-b", "main"]);
    commit(&dir, "first on main");
    let first = git(&dir, &["rev-parse", "HEAD"]);
    git(&dir, &["update-ref", "refs/remotes/origin/main", &first]);
    git(
        &dir,
        &[
            "remote",
            "add",
            "origin",
            dir.join("nowhere").to_str().expect("a utf-8 path"),
        ],
    );
    commit(&dir, "second on main");
    git(&dir, &["switch", "-c", "work"]);
    commit(&dir, "work one");
    commit(&dir, "work two");
    git(&dir, &["switch", "main"]);
    commit(&dir, "third on main");
    git(&dir, &["switch", "work"]);
    dir
}

/// Run the tool in `dir` and hand back what it printed.
fn carried(dir: &Path) -> String {
    let ran = Command::new(tool())
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .expect("the tool runs");
    assert!(
        ran.status.success(),
        "the tool failed: {}{}",
        String::from_utf8_lossy(&ran.stdout),
        String::from_utf8_lossy(&ran.stderr)
    );
    String::from_utf8_lossy(&ran.stdout).into_owned()
}

/// The text of section `n`: from its `[n]` header to the next header.
fn section(output: &str, n: usize) -> String {
    let mut taken = Vec::new();
    let mut inside = false;
    for line in output.lines() {
        let header = line
            .strip_prefix('[')
            .and_then(|rest| rest.split_once(']'))
            .and_then(|(number, _)| number.parse::<usize>().ok());
        match header {
            Some(found) => inside = found == n,
            None if inside => {}
            None => continue,
        }
        if inside {
            taken.push(line);
        }
    }
    assert!(
        !taken.is_empty(),
        "the output has no section [{n}]: {output}"
    );
    taken.join("\n")
}

fn position(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle:?} is not in: {text}"))
}

/// **Section one: what this branch holds that `main` does not, oldest first,
/// one line each.** The commits `main` holds are not the branch's to carry, so
/// none of them is listed; with them in the list the section would name the
/// whole history.
#[test]
fn section_one_lists_what_the_branch_holds_that_main_does_not_oldest_first() {
    let dir = a_repository_with_a_known_history("one");
    let output = carried(&dir);
    let held = section(&output, 1);
    assert!(
        position(&held, "work one") < position(&held, "work two"),
        "the branch's commits are not oldest first: {held}"
    );
    for main_only in ["first on main", "second on main", "third on main"] {
        assert!(
            !held.contains(main_only),
            "{main_only:?} is main's, and the branch does not hold it: {held}"
        );
    }
    assert_eq!(
        held.lines()
            .filter(|line| line.starts_with("  ") && line.contains("work "))
            .count(),
        2,
        "one line for each commit: {held}"
    );
}

/// **Section two: what `main` holds that `origin/main` does not**, which is
/// what the next push would carry, oldest first. The commit `origin/main` has
/// is not listed, and neither is the branch's own work.
#[test]
fn section_two_lists_what_main_holds_that_origin_main_does_not_oldest_first() {
    let dir = a_repository_with_a_known_history("two");
    let output = carried(&dir);
    let pushed = section(&output, 2);
    assert!(
        position(&pushed, "second on main") < position(&pushed, "third on main"),
        "main's unpushed commits are not oldest first: {pushed}"
    );
    for not_carried in ["first on main", "work one", "work two"] {
        assert!(
            !pushed.contains(not_carried),
            "{not_carried:?} is not part of the next push: {pushed}"
        );
    }
}

/// **Section three: whether `main` has moved on since the branch left it.** The
/// branch in the known history left `main` at its second commit, and `main` has
/// one more. A branch started from `main`'s head says it is based there. Both
/// halves, because a section that always said one thing would pass either alone.
#[test]
fn section_three_says_whether_main_has_moved_on_since_the_branch_left_it() {
    let behind = a_repository_with_a_known_history("three-behind");
    let said = section(&carried(&behind), 3);
    assert!(
        said.contains("moved on") && said.contains("1 commit"),
        "a branch behind main by one commit did not say main has moved on by one: {said}"
    );

    let based = a_repository_with_a_known_history("three-based");
    git(&based, &["switch", "-c", "fresh", "main"]);
    commit(&based, "fresh work");
    let said = section(&carried(&based), 3);
    assert!(
        said.contains("main's head") && !said.contains("moved on"),
        "a branch started from main's head did not say it is based there: {said}"
    );
}

/// **The tool reads and never writes.** The remote in the scratch repository
/// points at a path that does not exist, so a fetch would fail and the case
/// with it; and the references, which a fetch or a pull would move, are the
/// same afterwards.
#[test]
fn the_tool_reads_the_repository_and_moves_nothing() {
    let dir = a_repository_with_a_known_history("reads-only");
    let before = git(&dir, &["for-each-ref", "--format=%(refname) %(objectname)"]);
    carried(&dir);
    let after = git(&dir, &["for-each-ref", "--format=%(refname) %(objectname)"]);
    assert_eq!(before, after, "the tool moved a reference");
    assert_eq!(
        git(&dir, &["symbolic-ref", "--short", "HEAD"]),
        "work",
        "the tool moved HEAD"
    );
}

/// **A checkout with no `origin/main` says so instead of failing**, since the
/// tool is for a repository that may never have fetched. The other two
/// sections still answer.
#[test]
fn a_repository_with_no_origin_main_says_so_and_still_answers_the_rest() {
    let dir = a_repository_with_a_known_history("no-origin");
    git(&dir, &["update-ref", "-d", "refs/remotes/origin/main"]);
    let output = carried(&dir);
    assert!(
        section(&output, 2).contains("origin/main"),
        "section two does not say origin/main is missing: {output}"
    );
    assert!(
        section(&output, 1).contains("work one"),
        "section one stopped answering without origin/main: {output}"
    );
}
