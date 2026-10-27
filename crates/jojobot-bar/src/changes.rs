//! **What git says about the branch the bar runs on**: the paths it changed,
//! and the paths of the push being reported.

use std::process::Command;

use crate::{PUSH_BASE_VARIABLE, is_a_revision};

/// One `git` invocation, stdout as text. Empty on any failure — a caller
/// missing `origin/main` or running outside a repo gets nothing to report
/// rather than a crash.
pub fn git_stdout(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
        .unwrap_or_default()
}

/// **The paths the branch changed**, for `bar dev`: everything that differs
/// from the merge base with `main`, committed or not, and the files git does
/// not track yet, with the merge base they were taken from. `None` when there is
/// no merge base to take.
pub fn changed_paths() -> Option<(String, Vec<String>)> {
    // **The ref the branch is measured against.** `main` unless `BAR_DEV_BASE`
    // names another: a branch built on a release line holds that line's own
    // commits, and against `main` they read as the branch's changes.
    let base_ref = std::env::var("BAR_DEV_BASE")
        .ok()
        .filter(|name| !name.is_empty())
        .or_else(|| {
            std::env::var("SIZE_BASE")
                .ok()
                .filter(|name| !name.is_empty())
        })
        .unwrap_or_else(|| "main".to_string());
    let mut base = git_stdout(&["merge-base", &base_ref, "HEAD"]);
    // **The forge's checkout holds `origin/main` and no local `main`**, so the
    // default ref is asked again by its remote-tracking name.
    if base.trim().is_empty() && base_ref == "main" {
        base = git_stdout(&["merge-base", "origin/main", "HEAD"]);
    }
    let base = base.trim();
    if base.is_empty() {
        return None;
    }
    let mut paths: Vec<String> = git_stdout(&["diff", "--name-only", base])
        .lines()
        .chain(git_stdout(&["ls-files", "--others", "--exclude-standard"]).lines())
        .map(str::to_string)
        .collect();
    paths.sort();
    paths.dedup();
    Some((base.to_string(), paths))
}

/// **The paths the push being reported changed.** With the commit the forge says
/// the branch moved from ([`PUSH_BASE_VARIABLE`]) that is the range from it to
/// `HEAD`; a branch's first push has none, and a rewritten branch names a commit
/// this checkout may not hold, so both fall back to the paths `bar dev` takes.
pub fn pushed_paths() -> Option<Vec<String>> {
    if let Ok(before) = std::env::var(PUSH_BASE_VARIABLE) {
        let before = before.trim();
        let none_before = before.chars().all(|c| c == '0');
        if is_a_revision(before)
            && !none_before
            && !git_stdout(&[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("{before}^{{commit}}"),
            ])
            .trim()
            .is_empty()
        {
            return Some(
                git_stdout(&["diff", "--name-only", before, "HEAD"])
                    .lines()
                    .map(str::to_string)
                    .collect(),
            );
        }
    }
    changed_paths().map(|(_, paths)| paths)
}
