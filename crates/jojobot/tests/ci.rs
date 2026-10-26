//! **The bar runs without anybody remembering to run it.**
//!
//! This repository is built by disposable implementer sessions that report
//! their own results. Every gate it has — the suites, the lint, the format
//! check, the fixture roster — runs only when a session types the command and
//! reads the exit code honestly. **A session that forgets, or that reads a
//! stale binary's answer, is indistinguishable from a green build.**
//!
//! So the bar is automated, and these cases say so. **Forgejo runs it, and two
//! files guard two moments**: `.forgejo/workflows/pull-request.yml` guards the
//! pull request to `main` (`make check`, the rooms and the year room), and
//! `.forgejo/workflows/branch.yml` guards a line's own working branch
//! (`make report` and the rooms). GitHub runs nothing. Each case asserts a
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

/// One Forgejo workflow by file name, read from the repository root.
fn workflow(file: &str) -> String {
    let path = root().join(".forgejo/workflows").join(file);
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "the bar runs only when somebody remembers to run it: {}: {e}",
            path.display(),
        )
    })
}

/// **GitHub runs no workflow.** The bar runs on Forgejo; a workflow left under
/// `.github` would be a second bar on a host that does not build this
/// repository. The directory may be absent, and when it is there it holds no
/// workflow file.
#[test]
fn github_runs_no_workflow() {
    let dir = root().join(".github/workflows");
    let left: Vec<String> = std::fs::read_dir(&dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".yml") || name.ends_with(".yaml"))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        left.is_empty(),
        "GitHub would run {left:?}, a second bar beside Forgejo's",
    );
}

/// **The automated bar is the documented bar, run through the flake.**
///
/// Two files, two bars, one rule each. `pull-request.yml` runs `make check`,
/// the bar the house rules name, and `branch.yml` runs `make report`, the same
/// bar less the slow jobs its history lets go. Each runs it inside
/// `nix develop`: a workflow that runs `make` outside the flake gets a
/// different toolchain, and not the `dolt` and `make` the dev shell puts on the
/// path, so it fails in ways nobody can reproduce locally. A workflow that runs
/// some other command is a second standard that will drift from this one.
#[test]
fn the_automated_bar_is_the_same_command_the_house_rules_name() {
    let pull_request = workflow("pull-request.yml");
    assert!(
        pull_request.contains("nix develop -c make check"),
        "the pull request runs something other than the documented bar inside the flake, so \
         there are now two standards:\n{pull_request}",
    );
    let branch = workflow("branch.yml");
    assert!(
        branch.contains("nix develop -c make report"),
        "a working branch runs something other than the report bar inside the flake:\n{branch}",
    );
}

/// **Every line's working branch is checked, and every pull request to the
/// mainline.**
///
/// Implementer sessions work on their own branches and the coordinator carries
/// them across. A gate that only watches the mainline reports a break after the
/// carry, which is the one moment the author is already gone. So `branch.yml`
/// triggers on a push to the lines' working branches, and `pull-request.yml` on
/// a pull request to `main`.
///
/// **The positive is in the same case.** Asserting only that a trigger exists
/// passes against one that never names a branch a line pushes.
#[test]
fn the_bar_runs_on_every_working_branch_and_on_every_pull_request_to_the_mainline() {
    let branch = workflow("branch.yml");
    assert!(
        branch.contains("push:") && branch.contains("'dev*-work'"),
        "the branch workflow does not run on a push to the lines' working branches, so a line's \
         break shows only after the carry:\n{branch}",
    );
    let pull_request = workflow("pull-request.yml");
    assert!(
        pull_request.contains("pull_request:") && pull_request.contains("- main"),
        "the pull request workflow does not run on a pull request to main:\n{pull_request}",
    );
}

/// **The checkout carries the whole history, because a test of the bar reads it.**
///
/// The fixture roster test reads the commit messages since a fixed boundary and
/// fails when it cannot read the range. The checkout action takes one commit by
/// default, so a workflow that leaves the depth alone is red on every push for a
/// reason no change to the code fixes. Every workflow runs that test (the pre-push
/// one through `make prepush`), so each carries the depth, **on every one of their checkouts**: a count of the depths
/// equal to the count of the checkouts, because a job that kept the default is
/// red while a sibling job that has the depth is green.
///
/// **The positive is in the same case.** Asserting that no depth of one is set
/// passes against a file with no checkout at all.
#[test]
fn the_checkout_carries_the_whole_history() {
    for file in ["pull-request.yml", "branch.yml", "prepush.yml"] {
        let yaml = workflow(file);
        assert!(
            yaml.contains("/checkout@"),
            "{file} has no checkout, so it builds nothing:\n{yaml}",
        );
        assert_eq!(
            yaml.matches("/checkout@").count(),
            yaml.matches("fetch-depth: 0").count(),
            "a checkout in {file} takes one commit, so the commit range the roster test reads is \
             missing and that job is red on every push:\n{yaml}",
        );
    }
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
        rooms.contains("jojobot-bar -- rooms") && !rooms.contains("year"),
        "`rooms` must run the room suites through the bar and leave the year room out: {rooms}",
    );

    let year = dry_run(&["room", "ROOM=year"]);
    assert!(
        year.contains("--test year_room") && year.contains("--all-features"),
        "`room ROOM=year` must run the year room's own test file with every feature on: {year}",
    );

    let prepush = dry_run(&["prepush"]);
    for step in [
        "jojobot-bar -- check",
        "jojobot-bar -- rooms",
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

/// **The build the paid tier depends on is the locked workspace build.**
///
/// The deploy package builds from `Cargo.lock` and can neither rewrite it nor
/// reach the network, and the bar asks the same question with `--locked`. A
/// build step without the flag repairs a manifest that disagrees with the lock
/// silently, so the binary a paid run drives is not the one the package makes.
///
/// **The positive is in the same case**: the line is the workspace build, so a
/// recipe that gained the flag by building one crate does not pass.
#[test]
fn the_paid_tiers_build_is_the_locked_workspace_build() {
    let planned = paid_dry_run(None);
    let build = planned
        .lines()
        .find(|line| line.contains("build"))
        .unwrap_or_else(|| panic!("the paid target plans no build step: {planned}"));
    assert!(
        build.contains("--locked"),
        "the build the paid tier depends on can rewrite the lock: {build}",
    );
    assert!(
        build.contains("--workspace"),
        "the build the paid tier depends on is not the workspace build: {build}",
    );
}

/// A Forgejo workflow file, by its name without the extension.
fn forgejo_workflow(name: &str) -> String {
    let path = root().join(format!(".forgejo/workflows/{name}.yml"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// The lines of `yaml` under the line that is exactly `header` (indented by
/// `indent` spaces), down to the next line indented that much or less. Comment
/// and blank lines are left out, so a word in a comment is never read as a key.
fn lines_under<'a>(yaml: &'a str, header: &str, indent: usize) -> Vec<&'a str> {
    let wanted = format!("{}{header}", " ".repeat(indent));
    let mut inside = false;
    let mut found = Vec::new();
    for line in yaml.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        let depth = line.len() - line.trim_start().len();
        if inside && depth <= indent {
            break;
        }
        if inside {
            found.push(line);
        } else if line == wanted {
            inside = true;
        }
    }
    found
}

/// **A pull request to a deploy branch gets the pull-request bar.**
///
/// A carry into a `deploy-*` branch is landed by a pull request, and a trigger
/// that names only `main` leaves that carry with no bar at all. **The positive
/// is in the same case**: `main` is still named, so a trigger that swapped one
/// branch for the other does not pass.
#[test]
fn the_pull_request_bar_covers_the_mainline_and_the_deploy_branches() {
    let yaml = forgejo_workflow("pull-request");
    let on = lines_under(&yaml, "on:", 0).join("\n");
    assert!(
        on.contains("pull_request:"),
        "no pull_request trigger:\n{on}"
    );
    let branches: Vec<String> = lines_under(&on, "pull_request:", 2)
        .into_iter()
        .filter_map(|line| line.trim().strip_prefix("- "))
        .map(|name| name.trim_matches(|c| c == '\'' || c == '"').to_string())
        .collect();
    for wanted in ["main", "deploy-*"] {
        assert!(
            branches.iter().any(|name| name == wanted),
            "the pull request trigger does not name {wanted}: {branches:?}",
        );
    }
}

/// Whether a `[[test]]` entry of the manifest named `suite` carries
/// `required-features`. White space is left out of the entry first, so
/// `name="x"` and `name = "x"` are one spelling.
fn is_feature_gated(manifest: &str, suite: &str) -> bool {
    manifest.split("[[test]]").skip(1).any(|entry| {
        let entry = entry.split("[[").next().unwrap_or_default();
        let entry: String = entry.split_whitespace().collect();
        entry.contains(&format!("name=\"{suite}\"")) && entry.contains("required-features")
    })
}

/// **The manifest half of the guard below reads a gated entry in either spelling
/// and passes an ungated one.** No entry of the real manifest is gated today, so
/// the real manifest cannot show the check refusing anything; this feeds it text
/// that does.
#[test]
fn a_feature_gated_test_entry_is_found_with_or_without_spaces() {
    let gated = "[[test]]\nname = \"dolt_store\"\nrequired-features = [\"real\"]\n";
    let tight = "[[test]]\nname=\"dolt_store\"\nrequired-features=[\"real\"]\n";
    let other = "[[test]]\nname = \"other\"\nrequired-features = [\"real\"]\n\
                 [[test]]\nname = \"dolt_store\"\n";
    assert!(is_feature_gated(gated, "dolt_store"));
    assert!(is_feature_gated(tight, "dolt_store"));
    assert!(!is_feature_gated(other, "dolt_store"));
}

/// **Each real-store suite runs once in a pull request's checks.** `make check`
/// runs every test binary of the workspace, and the two real-store suites are
/// ordinary test binaries of the adapters crate: a file under `tests/` and no
/// `required-features` in the manifest. A pull-request job that ran them again
/// would hold a runner for an hour to repeat a verdict. **The positive is in the
/// same case**: the `check` job exists and runs the bar, so a workflow that lost
/// both jobs does not pass with nothing running twice.
#[test]
fn a_pull_request_runs_each_real_store_suite_once() {
    let manifest = std::fs::read_to_string(root().join("crates/jojobot-adapters/Cargo.toml"))
        .expect("the adapters manifest");
    for suite in ["dolt_store", "dolt_without_a_home"] {
        assert!(
            root()
                .join(format!("crates/jojobot-adapters/tests/{suite}.rs"))
                .exists(),
            "{suite} is not a test file of the adapters crate, so `make check` may not run it"
        );
        assert!(
            !is_feature_gated(&manifest, suite),
            "{suite} is gated behind a feature that `make check` may leave off"
        );
    }
    let yaml = forgejo_workflow("pull-request");
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
    // **The bar is `make check` as a word**: the step that wraps it for a carry
    // writes more after it, and `make check-x` would not be the bar.
    assert!(
        jobs.lines().any(
            |line| line
                .match_indices("make check")
                .any(|(at, word)| line[at + word.len()..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace))
        ),
        "the pull request does not run the bar:\n{jobs}"
    );
    // **Every line of every job is read**, so a `run: |` block counts as much as
    // a one-line `run:`. Comment lines were left out by `lines_under`.
    for needle in ["make integration", "dolt_store", "dolt_without_a_home"] {
        let at = jobs.lines().find(|line| line.contains(needle));
        assert!(
            at.is_none(),
            "a pull-request job runs a real-store suite that `make check` already ran: {at:?}"
        );
    }
}

/// **The pre-push run keeps the bar's logs, after the bar and whatever its verdict.**
///
/// The job log holds only the bar's summary, so without the upload the output of
/// a red case is lost. The upload step follows the step that runs `make prepush`,
/// runs when the job failed (`!cancelled()`), never changes the verdict
/// (`continue-on-error`), and names the artifact and the logs it keeps. **The
/// steps are read as steps**: comment lines are left out, so a word in a comment
/// is never read as a key.
#[test]
fn the_prepush_run_uploads_the_bar_logs_after_the_bar_whatever_its_verdict() {
    let yaml = forgejo_workflow("prepush");
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
    let job = lines_under(&jobs, "prepush:", 2).join("\n");
    let mut steps: Vec<Vec<&str>> = Vec::new();
    for line in lines_under(&job, "steps:", 4) {
        if line.starts_with("      - ") {
            steps.push(Vec::new());
        }
        steps
            .last_mut()
            .unwrap_or_else(|| panic!("a line before the first step:\n{job}"))
            .push(line.trim_start().trim_start_matches("- "));
    }
    let position = |wanted: &str| {
        steps
            .iter()
            .position(|step| step.contains(&wanted))
            .unwrap_or_else(|| panic!("no step has the line `{wanted}`:\n{job}"))
    };
    let bar = position("run: nix develop -c make prepush");
    let upload = position(
        "uses: ${{ forgejo.server_url }}/${{ forgejo.repository_owner }}/upload-artifact@ff15f0306b3f739f7b6fd43fb5d26cd321bd4de5",
    );
    assert!(
        bar < upload,
        "the logs are uploaded before the bar has run, so there is nothing to keep:\n{job}"
    );
    for wanted in [
        "if: ${{ !cancelled() }}",
        "continue-on-error: true",
        "name: bar-logs-prepush",
        "path: target/bar/*.log",
    ] {
        assert!(
            steps[upload].contains(&wanted),
            "the upload step does not hold `{wanted}`: {:?}",
            steps[upload]
        );
    }
}

/// **One job of a branch run can be run alone.**
///
/// The forge has no endpoint to re-run a job, so the branch workflow takes a
/// dispatch with a `job` input, and each job runs for a push, for a dispatch
/// that names no job, and for a dispatch that names it. **The positive is in the
/// same case**: both jobs are found and each is gated, so a gate that fits one
/// job and a file with a job missing do not pass. A dispatch has a concurrency
/// group of its own, because a dispatch in the group of the push would cancel the
/// push's run, and the job beside it.
#[test]
fn each_branch_job_can_be_dispatched_alone() {
    let yaml = forgejo_workflow("branch");
    let on = lines_under(&yaml, "on:", 0).join("\n");
    assert!(
        on.contains("workflow_dispatch:"),
        "no dispatch trigger:\n{on}"
    );
    assert!(
        lines_under(&on, "workflow_dispatch:", 2)
            .iter()
            .any(|line| line.trim() == "job:"),
        "the dispatch takes no `job` input:\n{on}",
    );
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
    for job in ["report", "rooms"] {
        let gate = lines_under(&jobs, &format!("{job}:"), 2)
            .into_iter()
            .find(|line| line.trim_start().starts_with("if:") && line.starts_with("    if:"))
            .unwrap_or_else(|| panic!("the {job} job has no gate of its own:\n{jobs}"));
        for part in [
            "forgejo.event_name != 'workflow_dispatch'",
            "inputs.job == ''",
            &format!("inputs.job == '{job}'"),
        ] {
            assert!(gate.contains(part), "the {job} gate lacks {part}: {gate}");
        }
    }
    let group = lines_under(&yaml, "concurrency:", 0)
        .into_iter()
        .find(|line| line.trim_start().starts_with("group:"))
        .expect("the branch workflow names a concurrency group");
    assert!(
        group.contains("forgejo.event_name") && group.contains("inputs.job"),
        "a dispatch shares the group of a push, and cancels its run: {group}",
    );
}

/// **A newer push cancels the older run of the same branch or pull request, and
/// nothing else.** Each workflow that runs on a push or a pull request names a
/// concurrency group keyed on what the run is about, the ref for a working
/// branch and the number for a pull request, and cancels in progress: a group
/// keyed on anything wider cancels another line's run, and a group that does not
/// cancel lets two runs of one head take the runners. **The protected refs are
/// excluded by the triggers**: no push trigger names `main` or a deploy branch,
/// and release QA starts by dispatch only, so a push to either starts no run and
/// cancels none. The positive is in the same case: every group is found.
#[test]
fn a_newer_push_cancels_the_same_branchs_older_run_and_no_protected_ref_is_pushed_to_run() {
    for (workflow, keyed_on) in [
        ("branch", "${{ forgejo.ref }}"),
        ("pull-request", "${{ forgejo.event.pull_request.number }}"),
    ] {
        let yaml = forgejo_workflow(workflow);
        let concurrency = lines_under(&yaml, "concurrency:", 0);
        let group = concurrency
            .iter()
            .find(|line| line.trim_start().starts_with("group:"))
            .unwrap_or_else(|| panic!("{workflow} names no concurrency group"));
        assert!(
            group.contains(keyed_on),
            "{workflow}'s group is not keyed on {keyed_on}: {group}"
        );
        assert!(
            concurrency
                .iter()
                .any(|line| line.trim() == "cancel-in-progress: true"),
            "{workflow} does not cancel a superseded run: {concurrency:?}"
        );
    }
    for workflow in ["branch", "pull-request", "prepush"] {
        let yaml = forgejo_workflow(workflow);
        let on = lines_under(&yaml, "on:", 0).join("\n");
        let pushed: Vec<&str> = lines_under(&on, "push:", 2)
            .into_iter()
            .map(str::trim)
            .collect();
        // **The positive beside the negative**: the branch workflow is the one that
        // runs on a push, and it names the working branches it runs on. A file that
        // lost its `push:` would pass the check below over an empty list.
        if workflow == "branch" {
            assert!(
                pushed.iter().any(|line| line.contains("dev")),
                "branch runs on pushes to the working branches: {pushed:?}"
            );
        }
        for protected in ["main", "deploy-"] {
            assert!(
                !pushed.iter().any(|line| line.contains(protected)),
                "{workflow} runs on a push to {protected}: {pushed:?}"
            );
        }
    }
    let prepush = forgejo_workflow("prepush");
    let on = lines_under(&prepush, "on:", 0);
    assert_eq!(
        on.iter().map(|line| line.trim()).collect::<Vec<_>>(),
        ["workflow_dispatch:"],
        "release QA starts by dispatch only"
    );
}

/// **A job that stops printing fails; it does not sit.** Each job of the branch
/// workflow and the release QA job carries a bound well past its usual run, so a
/// hung job is a red run inside the runner's own cap and not a slot held for
/// hours. **The positive is in the same case**: each job is found, so a file that
/// lost a job does not pass with nothing to bound.
#[test]
fn each_long_job_carries_a_time_bound_of_its_own() {
    for (workflow, job, minutes) in [
        ("branch", "report", 60),
        ("branch", "rooms", 60),
        ("prepush", "prepush", 130),
    ] {
        let yaml = forgejo_workflow(workflow);
        let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
        let block = lines_under(&jobs, &format!("{job}:"), 2);
        assert!(
            !block.is_empty(),
            "{workflow}.yml has no {job} job:\n{jobs}"
        );
        let bound = block
            .iter()
            .find(|line| line.starts_with("    timeout-minutes:"))
            .unwrap_or_else(|| panic!("the {workflow} {job} job has no timeout-minutes:\n{jobs}"));
        assert_eq!(
            bound.trim(),
            format!("timeout-minutes: {minutes}"),
            "{workflow} {job}",
        );
    }
}

/// **A flake check that prints nothing for half an hour fails.** A build that
/// hangs silently held a runner for forty-six minutes in the release QA run, with
/// nothing in its log to say why. `-L` prints the build log as it goes, and the
/// silence limit turns a build that prints nothing into a failure.
#[test]
fn the_flake_check_fails_after_half_an_hour_of_silence() {
    let planned = dry_run(&["flake-check"]);
    let line = planned
        .lines()
        .find(|line| line.contains("nix flake check"))
        .unwrap_or_else(|| panic!("the flake check target plans no flake check: {planned}"));
    assert!(
        line.split_whitespace().any(|word| word == "-L"),
        "the flake check does not print the build log as it runs: {line}",
    );
    assert!(
        line.contains("--max-silent-time 1800"),
        "the flake check has no limit on silence: {line}",
    );
}

/// **A run fetches no action from `data.forgejo.org`.** That host can be slow, and
/// a run then dies at checkout or at upload. Every `uses:` names the mirror of the
/// action on the forge that runs the workflow, through the forge's own server
/// address and the owner of the repository (`forgejo.server_url` and
/// `forgejo.repository_owner`), at the full commit the mirror holds for the
/// tag the workflow used before. **The positive is in the
/// same case**: each workflow uses at least one action, and every action it uses
/// is one of the three mirrored here, so a fourth action is a decision and not a
/// default.
#[test]
fn every_action_is_fetched_from_the_mirror_at_a_pinned_commit() {
    const MIRROR: &str = "${{ forgejo.server_url }}/${{ forgejo.repository_owner }}";
    let pinned = [
        ("checkout", "11d5960a326750d5838078e36cf38b85af677262"),
        (
            "upload-artifact",
            "ff15f0306b3f739f7b6fd43fb5d26cd321bd4de5",
        ),
        (
            "download-artifact",
            "9bc31d5ccc31df68ecc42ccf4149144866c47d8a",
        ),
    ];
    let dir = root().join(".forgejo/workflows");
    let mut seen = 0;
    for entry in std::fs::read_dir(&dir).expect("the workflows directory is readable") {
        let path = entry.expect("a directory entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("yml") {
            continue;
        }
        let yaml = std::fs::read_to_string(&path).expect("a workflow is readable");
        for line in yaml.lines() {
            let Some((_, used)) = line.split_once("uses:") else {
                continue;
            };
            if line.trim_start().starts_with('#') {
                continue;
            }
            seen += 1;
            let used = used.trim();
            let allowed: Vec<String> = pinned
                .iter()
                .map(|(action, commit)| format!("{MIRROR}/{action}@{commit}"))
                .collect();
            assert!(
                allowed.iter().any(|ok| ok == used),
                "{} uses `{used}`, which is not one of {allowed:?}",
                path.display(),
            );
        }
    }
    assert!(
        seen > 0,
        "no workflow uses an action, so nothing was checked"
    );
}

/// **The pull request compiles its test binaries once, and the three bars run
/// them side by side.**
///
/// `build` is the one job that compiles (`make bundle`), after `static`.
/// `check`, `rooms` and `year` each need `static` and `build`, so none waits
/// for another bar, and each unpacks the bundle and runs its own target with
/// `BAR_BUNDLE` set. Every job that runs a bar names `CI_ROOT`: a binary reads
/// its files by the path it was compiled in, and `CI_ROOT` is what puts the
/// job's checkout at that path. A job that left either out would compile the
/// tests again, or fail to find its files.
///
/// **The positives are in the same case**: all three bars are present, and the
/// `integration` job is gone, because `check` runs the suites it repeated.
#[test]
fn the_pull_request_builds_once_and_runs_the_three_bars_side_by_side() {
    let yaml = forgejo_workflow("pull-request");
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");

    let build = lines_under(&jobs, "build:", 2).join("\n");
    assert!(
        build.contains("make bundle CI_ROOT="),
        "the build job does not compile the bundle at the shared path:\n{build}"
    );
    assert_eq!(
        build
            .lines()
            .filter(|line| line.trim_start().starts_with("needs:"))
            .map(str::trim)
            .collect::<Vec<_>>(),
        ["needs: static"],
        "the build job must wait for static and for nothing else:\n{build}"
    );

    for (job, target) in [("check", "check"), ("rooms", "rooms"), ("year", "year")] {
        let block = lines_under(&jobs, &format!("{job}:"), 2).join("\n");
        assert!(!block.is_empty(), "the pull request has no {job} job");
        assert_eq!(
            block
                .lines()
                .filter(|line| line.trim_start().starts_with("needs:"))
                .map(str::trim)
                .collect::<Vec<_>>(),
            ["needs: [static, build]"],
            "{job} must wait for static and the build, never another bar:\n{block}"
        );
        assert!(
            block.contains(&format!("nix develop -c make {target} CI_ROOT=")),
            "{job} does not run `make {target}` at the shared path:\n{block}"
        );
        assert!(
            block.contains("BAR_BUNDLE: target/bundle/manifest.json"),
            "{job} does not run the bundle's binaries:\n{block}"
        );
        assert!(
            block.contains("/download-artifact@") && block.contains("test-bundle"),
            "{job} does not download the bundle:\n{block}"
        );
    }
    assert!(
        !jobs.contains("\n  integration:"),
        "the integration job repeats suites `make check` runs:\n{jobs}"
    );
}

/// **A job that holds the bundle runs the bundled bar, and a job with none
/// builds the bar.** The bundle carries the bar built from the same commit, so
/// a `cargo run` in a job that built nothing would compile the bar and every
/// crate under it before the first test. `CI_ROOT` puts the job at the root the
/// path is relative to. **The positive is in the same case**: without
/// `BAR_BUNDLE` the same targets still run `cargo run`.
#[test]
fn a_job_with_a_bundle_runs_the_bundled_bar_and_one_without_builds_it() {
    for target in ["check", "rooms", "year"] {
        let with = dry_run(&[
            target,
            "CI_ROOT=/tmp/jojobot-ci/1",
            "BAR_BUNDLE=target/bundle/manifest.json",
        ]);
        assert!(
            with.contains("scripts/ci-root /tmp/jojobot-ci/1 cargo-target/debug/jojobot-bar"),
            "`make {target}` with a bundle does not run the bundled bar at the shared path: {with}"
        );
        assert!(
            !with.contains("cargo run"),
            "`make {target}` with a bundle still builds the bar: {with}"
        );
        let without = dry_run(&[target]);
        assert!(
            without.contains("cargo run -q --locked -p jojobot-bar"),
            "`make {target}` with no bundle does not build the bar: {without}"
        );
    }
}

/// **Each job's sccache server answers on a socket inside that job's own
/// checkout.** A computed port can be taken by another job or another
/// program; the checkout is not shared, so a socket in it is the job's alone.
/// The runner's own socket name is replaced, because that server keeps the
/// mount namespace of the first job that started it.
#[test]
fn each_job_runs_an_sccache_server_on_a_socket_in_its_own_checkout() {
    let script = std::fs::read_to_string(root().join("scripts/ci-root"))
        .expect("scripts/ci-root is on disk");
    let code: String = script
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        code.contains("export SCCACHE_SERVER_UDS=\"$JOJOBOT_CI_ROOT/"),
        "ci-root does not give the job a socket under its own root:\n{code}"
    );
    assert!(
        !code.contains("SCCACHE_SERVER_PORT"),
        "ci-root still computes a port for the sccache server:\n{code}"
    );
}

/// **Every job that makes or runs the bundle runs on an x86_64 host.** The
/// bundle holds binaries of one architecture and the label `nix` also names an
/// aarch64 host, so a job on that label can build for one architecture and run
/// on the other. **The positive is in the same case**: all four jobs are found,
/// so a workflow that lost one does not pass with nothing to check.
#[test]
fn every_job_that_makes_or_runs_the_bundle_runs_on_an_x86_64_host() {
    let yaml = forgejo_workflow("pull-request");
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
    for job in ["build", "check", "rooms", "year"] {
        let block = lines_under(&jobs, &format!("{job}:"), 2);
        assert!(!block.is_empty(), "the pull request has no {job} job");
        let labels: Vec<&str> = block
            .iter()
            .filter_map(|line| line.trim().strip_prefix("runs-on:"))
            .map(str::trim)
            .collect();
        assert_eq!(
            labels,
            ["nix-x86_64"],
            "{job} does not run on an x86_64 host only"
        );
    }
}

/// **Every step of the prepush bar arm is a job of the pull-request workflow.**
/// A pull-request proof vouches only for a green job of each name on
/// `BAR_ARM`, so a step with no job of that name would keep the arm from ever
/// being skipped. **The positive is in the same case**: the arm is not empty.
#[test]
fn every_step_of_the_prepush_arm_is_a_pull_request_job() {
    let makefile = std::fs::read_to_string(root().join("Makefile")).expect("the Makefile");
    let arm: Vec<String> = makefile
        .lines()
        .find_map(|line| line.strip_prefix("BAR_ARM"))
        .map(|rest| {
            rest.trim_start_matches([' ', '=', ':', '?'])
                .split_whitespace()
                .map(str::to_string)
                .collect()
        })
        .expect("a BAR_ARM line");
    assert!(!arm.is_empty(), "the arm names no step");
    let yaml = forgejo_workflow("pull-request");
    let jobs = lines_under(&yaml, "jobs:", 0).join("\n");
    for step in &arm {
        assert!(
            !lines_under(&jobs, &format!("{step}:"), 2).is_empty(),
            "the arm step {step} is no job of the pull-request workflow:\n{jobs}"
        );
    }
}
