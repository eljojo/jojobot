//! **A branch push runs the rooms only when its change is to a room's own
//! files**: a room's document, a room's test file or the exercise harness, all
//! under `crates/jojobot-exercise`. The real `bar plan rooms` runs in a scratch
//! repository against a stub `cargo` that describes four packages. The answer is
//! the `fast` output the workflow's rooms step reads: `true` leaves the rooms to
//! the pull request and the release gate.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-rooms-off-push"));
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
}

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

/// **The metadata cargo answers for the scratch workspace**, in the shape both
/// the crate selection (`--no-deps`) and the startup plan read.
fn metadata(dir: &Path) -> String {
    let root = dir.display();
    let package = |name: &str, deps: &[&str], targets: &str| {
        let deps: Vec<String> = deps
            .iter()
            .map(|d| format!("{{\"name\":\"{d}\"}}"))
            .collect();
        format!(
            "{{\"id\":\"{name}\",\"name\":\"{name}\",\"manifest_path\":\"{root}/crates/{name}/Cargo.toml\",\
             \"dependencies\":[{}],\"features\":{{}},\"targets\":[{targets}]}}",
            deps.join(",")
        )
    };
    let lib = |name: &str| format!("{{\"kind\":[\"lib\"],\"name\":\"{name}\",\"test\":true}}");
    let packages = [
        package(
            "jojobot-exercise",
            &[],
            &format!(
                "{},{{\"kind\":[\"test\"],\"name\":\"bike_room\",\"test\":true,\"required-features\":[\"rooms\"]}}",
                lib("jojobot-exercise")
            ),
        ),
        package("jojobot", &["jojobot-domain"], &lib("jojobot")),
        package("jojobot-domain", &[], &lib("jojobot-domain")),
        package("jojobot-bar", &[], &lib("jojobot-bar")),
    ];
    let names = [
        "jojobot-exercise",
        "jojobot",
        "jojobot-domain",
        "jojobot-bar",
    ];
    let members: Vec<String> = names.iter().map(|n| format!("\"{n}\"")).collect();
    let nodes: Vec<String> = names
        .iter()
        .map(|n| format!("{{\"id\":\"{n}\",\"features\":[\"rooms\"]}}"))
        .collect();
    format!(
        "{{\"workspace_root\":\"{root}\",\"workspace_members\":[{}],\"packages\":[{}],\
         \"resolve\":{{\"nodes\":[{}]}}}}",
        members.join(","),
        packages.join(","),
        nodes.join(",")
    )
}

/// **A repository with `main` at the base**, the scripts the selection reads and
/// the stub cargo. Returns the directory.
fn repository() -> PathBuf {
    let dir = scratch();
    git(&dir, &["init", "-q", "-b", "main"]);
    for name in [
        "jojobot-exercise",
        "jojobot",
        "jojobot-domain",
        "jojobot-bar",
    ] {
        let src = dir.join("crates").join(name).join("src");
        fs::create_dir_all(&src).expect("package dir");
        fs::write(src.join("lib.rs"), "// base\n").expect("write");
    }
    fs::create_dir_all(dir.join("crates/jojobot-exercise/rooms")).expect("rooms dir");
    fs::write(dir.join("crates/jojobot-exercise/rooms/bike.md"), "base\n").expect("write");
    fs::create_dir_all(dir.join("crates/jojobot-exercise/tests")).expect("tests dir");
    fs::write(
        dir.join("crates/jojobot-exercise/tests/bike_room.rs"),
        "// base\n",
    )
    .expect("write");
    fs::create_dir_all(dir.join("scripts")).expect("scripts dir");
    for name in ["select-crates", "ci-full-paths.txt"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../scripts")
                .join(name),
            dir.join("scripts").join(name),
        )
        .expect("copy a script");
    }
    fs::write(dir.join("scripts/ci-always-tests.txt"), "").expect("write");
    support::write_executable(
        &dir.join("cargo"),
        &format!(
            "#!/bin/sh\nif [ \"$1\" != metadata ]; then exit 1; fi\ncat <<'JSON'\n{}\nJSON\n",
            metadata(&dir)
        ),
    );
    git(&dir, &["add", "."]);
    git(&dir, &["commit", "-q", "-m", "base"]);
    git(&dir, &["switch", "-q", "-c", "work"]);
    dir
}

/// **What `bar plan <lane>` says** on the branch as it stands: the `fast` output
/// and the printed line. `selecting` is the workflow's `BAR_SELECT_BY_CRATE`,
/// which a dispatch sets to `0`; `base` is `BAR_SELECT_BASE`, unset when `None`.
fn plan_lane(dir: &Path, lane: &str, selecting: &str, base: Option<&str>) -> (String, String) {
    let output = dir.join("github-output");
    let _ = fs::remove_file(&output);
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .args(["plan", lane])
        .current_dir(dir)
        .env("CARGO", dir.join("cargo"))
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env("BAR_SELECT_BY_CRATE", selecting)
        .env_remove("BAR_SELECT_BASE")
        .env_remove("BAR_CI_BASE")
        .env("GITHUB_OUTPUT", &output);
    if let Some(base) = base {
        command.env("BAR_SELECT_BASE", base);
    }
    let out = support::output_fenced(&mut command).expect("run the plan");
    assert!(out.status.success(), "{out:?}");
    let written = fs::read_to_string(&output).expect("the plan writes its outputs");
    assert_eq!(
        written.matches("fast=").count(),
        1,
        "one answer, not two: {written:?}"
    );
    (
        written.trim().to_string(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// **What `bar plan rooms` says for a branch whose one change is `path`.**
fn plan_rooms(dir: &Path, path: &str, selecting: &str, base: &str) -> (String, String) {
    commit_change(dir, path);
    plan_lane(dir, "rooms", selecting, Some(base))
}

fn commit_change(dir: &Path, path: &str) {
    fs::create_dir_all(dir.join(path).parent().expect("a parent")).expect("parent dir");
    fs::write(dir.join(path), "// changed\n").expect("edit");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "change"]);
}

#[test]
fn a_change_no_room_reaches_leaves_the_rooms_to_the_pull_request() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-bar/src/lib.rs", "1", "main");
    assert_eq!(fast, "fast=true", "{said}");
    assert!(said.contains("pull request"), "{said}");
    assert!(
        said.contains("room document"),
        "the line says which files would have selected the rooms: {said}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// **A server-crate change selects no room**, the domain included: a defect it
/// causes that a room would catch is caught at the landing's pull request.
#[test]
fn a_domain_only_change_leaves_the_rooms_to_the_pull_request() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-domain/src/lib.rs", "1", "main");
    assert_eq!(fast, "fast=true", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

/// **A path the crate selection runs everything for is still no room file.** The
/// selector answers a full run for a manifest, a migration, a fixture, a workflow
/// file; none of them is under the exercise crate, so none selects the rooms.
#[test]
fn a_global_path_outside_the_exercise_crate_leaves_the_rooms_to_the_pull_request() {
    for path in [
        "crates/jojobot-domain/Cargo.toml",
        "crates/jojobot-adapters/migrations/0001_add.sql",
        "crates/jojobot-domain/tests/fixtures/sample.json",
        ".forgejo/workflows/branch.yml",
    ] {
        let dir = repository();
        let (fast, said) = plan_rooms(&dir, path, "1", "main");
        assert_eq!(fast, "fast=true", "{path}: {said}");
        let _ = fs::remove_dir_all(&dir);
    }
}

#[test]
fn a_change_to_a_room_test_file_runs_the_rooms() {
    let dir = repository();
    let (fast, said) = plan_rooms(
        &dir,
        "crates/jojobot-exercise/tests/bike_room.rs",
        "1",
        "main",
    );
    assert_eq!(fast, "fast=false", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_change_to_the_exercise_harness_runs_the_rooms() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-exercise/src/lib.rs", "1", "main");
    assert_eq!(fast, "fast=false", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_room_document_runs_the_rooms() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-exercise/rooms/bike.md", "1", "main");
    assert_eq!(fast, "fast=false", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_missing_selection_base_runs_the_rooms() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-bar/src/lib.rs", "1", "origin/none");
    assert_eq!(fast, "fast=false", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

/// **Paired with the first case**: the same change under a dispatch, which does
/// not select by crate, still runs the rooms.
#[test]
fn a_dispatch_runs_the_rooms_whatever_the_change() {
    let dir = repository();
    let (fast, said) = plan_rooms(&dir, "crates/jojobot-bar/src/lib.rs", "0", "main");
    assert_eq!(fast, "fast=false", "{said}");
    let _ = fs::remove_dir_all(&dir);
}

/// **The skip is the rooms' alone.** `plan` writes the same `fast` output for
/// every lane, and the report job skips its step when it is true: a bar-only push
/// that leaves the rooms to the pull request must not skip the report, the check,
/// the build or the year. Paired with the first case: the rooms lane skips the
/// same push.
#[test]
fn a_change_no_room_reaches_skips_no_other_lane() {
    let dir = repository();
    commit_change(&dir, "crates/jojobot-bar/src/lib.rs");
    let (rooms, said) = plan_lane(&dir, "rooms", "1", Some("main"));
    assert_eq!(rooms, "fast=true", "{said}");
    for lane in ["report", "check", "build", "year"] {
        let (fast, said) = plan_lane(&dir, lane, "1", Some("main"));
        assert_eq!(fast, "fast=false", "{lane}: {said}");
    }
    let _ = fs::remove_dir_all(&dir);
}

/// **Nothing to read means run the rooms.** An unset base, and a branch with no
/// change since its base, answer like a missing base does.
#[test]
fn an_unset_base_and_an_empty_diff_run_the_rooms() {
    let dir = repository();
    commit_change(&dir, "crates/jojobot-bar/src/lib.rs");
    let (fast, said) = plan_lane(&dir, "rooms", "1", None);
    assert_eq!(fast, "fast=false", "an unset base: {said}");
    let _ = fs::remove_dir_all(&dir);
    let dir = repository();
    let (fast, said) = plan_lane(&dir, "rooms", "1", Some("main"));
    assert_eq!(fast, "fast=false", "an empty diff: {said}");
    let _ = fs::remove_dir_all(&dir);
}
