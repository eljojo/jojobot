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

/// 🚨 **A red bar reports every target, not only the ones before the first
/// failure.**
///
/// `cargo test` stops after the first failing target. So a run with an early
/// failure prints a suite count that is a true statement about what RAN and a
/// false impression of what was CHECKED — and nothing on screen says which it
/// is. Three runs on this tree in one day reported 19, 16 and 16 suites ok out
/// of forty-one, each of them looking like a mostly-green bar.
///
/// ⛔️ **A partial red bar is worse than a plain one**, because the reader
/// believes they know which parts are fine. It has already cost a hand-off: an
/// implementer was told three failures were not theirs and to carry on, when
/// twenty-two suites had not run at all.
///
/// **The positive is in the same case.** Asserting only that the flag is there
/// passes against a bar that stopped being the workspace bar, which is a
/// smaller run wearing the same name.
///
/// ⚠️ **It does not reach a target that fails to COMPILE.** Compilation is not
/// a test failure, so cargo still stops — and that run reports no suites at
/// all rather than a plausible-looking count, which is the failure this case is
/// about arriving in a shape a reader cannot misread.
#[test]
fn the_bar_reports_every_target_and_not_only_the_ones_before_a_failure() {
    let makefile =
        std::fs::read_to_string(root().join("Makefile")).expect("the bar is written down");
    let step = makefile
        .lines()
        .skip_while(|l| !l.starts_with("test:"))
        .nth(1)
        .expect("the test target has a recipe");
    assert!(
        step.contains("--no-fail-fast"),
        "the bar stops at the first failing target, so a red run reports a suite count that \
         reads as coverage and is not: {step}",
    );
    assert!(
        step.contains("--workspace"),
        "the bar is no longer the whole workspace, which is a smaller run wearing the same \
         name: {step}",
    );
}
