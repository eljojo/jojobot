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

/// `make -n paid <vars>`, dry run: what the recipe would run, never run.
///
/// `-n` prints the commands a target would execute without executing them —
/// including the guard line and the `cargo run` beneath it — so this asks the
/// real thing the paid target does without ever spending the money it costs.
fn paid_dry_run(playbook: Option<&str>) -> String {
    let mut command = std::process::Command::new("make");
    command
        .current_dir(root())
        .arg("-n")
        .arg("paid")
        .env_remove("PLAYBOOK");
    if let Some(playbook) = playbook {
        command.env("PLAYBOOK", playbook);
    }
    let done = command.output().expect("make runs");
    format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr),
    )
}

/// **An unset room must not reach the binary as an empty flag, and a named one
/// must still reach it.**
///
/// Decision log 299 put the default room in the binary; the target wrapping it
/// must let a caller reach that default rather than gating it behind a
/// variable the binary no longer needs. The positive half is in the same
/// case: a target that dropped `--playbook` unconditionally would pass the
/// first half by never sending the flag at all.
#[test]
fn an_unset_playbook_reaches_the_binary_with_no_flag_and_a_set_one_still_passes_through() {
    let bare = paid_dry_run(None);
    assert!(
        !bare.contains("--playbook"),
        "an unset PLAYBOOK reached the binary as a flag with nothing after it, which the \
         binary would refuse as a missing argument rather than resolve to its own default: \
         {bare}",
    );

    let named = paid_dry_run(Some("rooms/loop.md"));
    assert!(
        named.contains("--playbook rooms/loop.md"),
        "a named PLAYBOOK must still reach the binary: {named}",
    );
}

/// `make -n <args>`, dry run: what the targets would run, never run.
///
/// **A dry run reads the recipe lines, so a line that calls `$(MAKE)` still
/// runs — and passes `-n` down to the sub-make.** The pre-push steps are
/// targets of their own for that reason; a step written beside a `$(MAKE)`
/// call on one line would execute under `-n`.
fn dry_run(args: &[&str]) -> String {
    let done = std::process::Command::new("make")
        .current_dir(root())
        .arg("-n")
        .args(args)
        .output()
        .expect("make runs");
    format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr),
    )
}

/// **The year room runs in the pre-push target and nowhere else by default.**
/// `rooms` turns on the `rooms` feature alone, so it leaves the year room out;
/// `room ROOM=year` names its test file and turns every feature on; and the
/// pre-push target runs the bar, the real store, `rooms`, the year room, the
/// package build and the flake check. Losing any one of those steps loses
/// coverage nobody else runs.
#[test]
fn the_pre_push_target_runs_the_rooms_the_year_room_and_release_qa() {
    let rooms = dry_run(&["rooms"]);
    assert!(
        rooms.contains("--features rooms") && !rooms.contains("year"),
        "`rooms` must run the room suites and leave the year room out: {rooms}",
    );

    let year = dry_run(&["room", "ROOM=year"]);
    assert!(
        year.contains("--test year_room") && year.contains("--all-features"),
        "`room ROOM=year` must run the year room's own test file with every feature on: {year}",
    );

    let prepush = dry_run(&["prepush"]);
    for step in [
        "jojobot-bar -- check",
        "--test dolt_store",
        "--features rooms",
        "make room ROOM=year",
        "nix build",
        "nix flake check",
    ] {
        assert!(
            prepush.contains(step),
            "the pre-push target no longer runs `{step}`: {prepush}",
        );
    }
}

/// **`make rooms` runs exactly the room suites the manifest declares, and
/// nothing else.** `cargo test -p jojobot-exercise --features rooms` alone also
/// runs the crate's unit tests and every test file that is not a room suite,
/// all of which `make check` already runs. The target therefore names each room
/// target with `--test`, and the names come from the manifest so the two
/// cannot drift apart. The year room is not among them.
#[test]
fn make_rooms_names_the_manifests_room_targets_and_no_others() {
    let manifest = std::fs::read_to_string(root().join("crates/jojobot-exercise/Cargo.toml"))
        .expect("the exercise manifest is on disk");
    let mut declared: Vec<String> = manifest
        .split("[[test]]")
        .skip(1)
        .filter(|block| block.contains("required-features = [\"rooms\"]"))
        .map(|block| {
            block
                .lines()
                .find_map(|l| l.strip_prefix("name = \""))
                .and_then(|l| l.strip_suffix('"'))
                .expect("a test target names itself")
                .to_string()
        })
        .collect();
    declared.sort();
    assert!(
        declared.len() > 20 && !declared.contains(&"year_room".to_string()),
        "the manifest no longer declares the room suites it did: {declared:?}",
    );

    let rooms = dry_run(&["rooms"]);
    let mut named: Vec<String> = rooms
        .split_whitespace()
        .zip(rooms.split_whitespace().skip(1))
        .filter(|(flag, _)| *flag == "--test")
        .map(|(_, name)| name.to_string())
        .collect();
    named.sort();
    assert_eq!(
        named, declared,
        "`make rooms` must run each declared room target and no other: {rooms}",
    );
}

/// **The year room is reachable only through a feature the pre-push target
/// turns on, and `rooms` does not imply it.** A manifest edit that made
/// `rooms` pull in `year`, or dropped the year room's gate, would put the
/// slowest suite back in `make rooms` or in `make check`.
#[test]
fn the_year_room_is_gated_apart_from_the_other_rooms() {
    let manifest = std::fs::read_to_string(root().join("crates/jojobot-exercise/Cargo.toml"))
        .expect("the exercise manifest is on disk");
    let block = manifest
        .split("[[test]]")
        .find(|b| b.contains("name = \"year_room\""))
        .expect("the year room is declared as a test target");
    assert!(
        block.contains("required-features = [\"year\"]"),
        "the year room must need the `year` feature and no other: {block}",
    );
    assert!(
        manifest.contains("rooms = []") && manifest.contains("year = []"),
        "`rooms` and `year` must be independent features: {manifest}",
    );
}
