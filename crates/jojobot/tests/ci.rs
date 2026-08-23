//! **The bar runs without anybody remembering to run it.**
//!
//! This repository is built by disposable implementer sessions that report
//! their own results. Every gate it has — the suites, the lint, the format
//! check, the fixture roster — runs only when a session types the command and
//! reads the exit code honestly. **A session that forgets, or that reads a
//! stale binary's answer, is indistinguishable from a green build.**
//!
//! So the bar is automated, and this case is what says so. It asserts the
//! workflow exists and runs the SAME command the house rules name, because a
//! workflow that drifts to its own command is a second bar nobody is reading.

use std::path::PathBuf;

/// The repository root, resolved from this crate rather than from wherever a
/// caller happened to stand.
fn root() -> PathBuf {
    PathBuf::new()
        .join(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
}

fn workflow() -> String {
    let path = root().join(".github/workflows/check.yml");
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the bar runs only when somebody remembers to run it: {}: {e}",
            path.display(),
        )
    })
}

/// **The automated bar is the documented bar, run through the flake.**
///
/// Two halves, and they fail differently. A workflow that runs some other
/// command is a second standard that will drift from this one. A workflow that
/// runs `make check` outside the flake gets a different toolchain, and the
/// shellHook that sets the load-bearing environment never runs — so it fails
/// in ways nobody can reproduce locally.
#[test]
fn the_automated_bar_is_the_same_command_the_house_rules_name() {
    let yaml = workflow();
    assert!(
        yaml.contains("make check"),
        "the workflow runs something other than the documented bar, so there are now two \
         standards:\n{yaml}",
    );
    assert!(
        yaml.contains("nix develop"),
        "the workflow runs the bar outside the flake, which is where this project's toolchain \
         lives:\n{yaml}",
    );
}

/// **Every push is checked, not only the ones onto the mainline.**
///
/// Implementer sessions work on their own branches and the coordinator carries
/// them across. A gate that only watches the mainline reports a break after the
/// carry, which is the one moment the author is already gone.
///
/// **The positive is in the same case.** Asserting only that no branch filter
/// exists passes against a file with no trigger at all, which is a workflow
/// that never runs.
#[test]
fn the_bar_runs_on_every_branch_and_not_only_on_the_mainline() {
    let yaml = workflow();
    assert!(
        yaml.contains("push:"),
        "the workflow has no push trigger, so it never runs:\n{yaml}",
    );
    assert!(
        !yaml.contains("branches:"),
        "a branch filter means an implementer's own branch is unchecked until it is carried, \
         which is after the author is gone:\n{yaml}",
    );
}
