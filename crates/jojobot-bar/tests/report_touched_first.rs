//! **The report level runs the jobs of the packages a push touched first.** The
//! real `bar` binary runs in a scratch repository against a stub `cargo` that
//! lists two packages. The commit the forge says the branch moved from decides
//! which packages the push touched, and without it the merge base with `main`
//! does. No job is dropped: the same jobs run in another order.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

/// **A fake `cargo` for two packages, `pa` and `pb`**, each with a library and
/// one test file. `metadata` describes them under the scratch directory.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo");
    let root = dir.display();
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case "$1" in
  metadata)
    cat <<'JSON'
{{"workspace_root":"{root}","packages":[
 {{"name":"pa","manifest_path":"{root}/crates/pa/Cargo.toml","dependencies":[],
   "targets":[{{"kind":["lib"],"name":"pa"}},{{"kind":["test"],"name":"alpha"}}]}},
 {{"name":"pb","manifest_path":"{root}/crates/pb/Cargo.toml","dependencies":[],
   "targets":[{{"kind":["lib"],"name":"pb"}},{{"kind":["test"],"name":"beta"}}]}}
]}}
JSON
    exit 0 ;;
esac
case " $* " in
  *" --no-run "*)
    echo '  Executable unittests src/lib.rs (/x/deps/pa-1)'
    echo '  Executable unittests src/lib.rs (/x/deps/pb-2)'
    echo '  Executable tests/alpha.rs (/x/deps/alpha-3)'
    echo '  Executable tests/beta.rs (/x/deps/beta-4)'
    exit 0 ;;
esac
case "$1" in
  test)
    echo 'test the_case ... ok'
    echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#
    );
    support::write_executable(&path, &script);
    path
}

fn scratch() -> PathBuf {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name("jojobot-bar-touched-first"));

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

/// **A repository with `main` at the base and a branch of two commits**: the
/// first touches `pa`, the second touches `pb`. Returns the first commit.
fn repository(dir: &Path) -> String {
    git(dir, &["init", "-q", "-b", "main"]);
    for package in ["pa", "pb"] {
        let src = dir.join("crates").join(package).join("src");
        fs::create_dir_all(&src).expect("package dir");
        fs::write(src.join("lib.rs"), "// base\n").expect("write");
    }
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["switch", "-q", "-c", "work"]);
    fs::write(dir.join("crates/pa/src/lib.rs"), "// pa changed\n").expect("write");
    git(dir, &["commit", "-q", "-am", "touch pa"]);
    let first = git(dir, &["rev-parse", "HEAD"]);
    fs::write(dir.join("crates/pb/src/lib.rs"), "// pb changed\n").expect("write");
    git(dir, &["commit", "-q", "-am", "touch pb"]);
    first
}

/// The `cargo test` calls of the run, in the order they started.
fn jobs_run(dir: &Path, level: &str, push_base: Option<&str>) -> (Vec<String>, String) {
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .arg(level)
        .current_dir(dir)
        .env("TMPDIR", dir)
        .env("CARGO", write_cargo(dir))
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env("BAR_JOBS", "1")
        .env_remove("BAR_PUSH_BASE");
    if let Some(base) = push_base {
        command.env("BAR_PUSH_BASE", base);
    }
    let out = support::output_fenced(&mut command).expect("run bar");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}");
    let jobs = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .filter(|call| call.starts_with("test ") && !call.contains("--no-run"))
        .map(|call| {
            call.trim_start_matches("test ")
                .replace("--workspace ", "")
                .replace(" --no-fail-fast --locked", "")
                .replace(" -- -Zunstable-options --report-time", "")
        })
        .collect();
    (jobs, stdout)
}

/// **The commit the push moved from decides the touched packages.** The push
/// touched `pb` only, so `pb`'s jobs run before `pa`'s though the listing names
/// `pa` first. **Paired**: with no such commit the merge base with `main` says
/// both packages are touched and the listed order stands, and the boundary level
/// never reorders.
#[test]
fn the_report_runs_the_jobs_of_the_packages_the_push_touched_first() {
    let dir = scratch();
    let first = repository(&dir);

    let (jobs, stdout) = jobs_run(&dir, "report", Some(&first));
    assert_eq!(
        jobs,
        [
            "-p pb --lib",
            "--test beta",
            "-p pa --lib",
            "--test alpha",
            "--doc"
        ],
        "{stdout}"
    );
    assert!(
        stdout.contains("order:") && stdout.contains("pb"),
        "{stdout}"
    );

    let dir = scratch();
    repository(&dir);
    let (jobs, _) = jobs_run(&dir, "report", None);
    assert_eq!(
        jobs,
        [
            "-p pa --lib",
            "-p pb --lib",
            "--test alpha",
            "--test beta",
            "--doc"
        ],
        "both packages differ from main, so nothing moves"
    );

    let dir = scratch();
    let first = repository(&dir);
    let (jobs, stdout) = jobs_run(&dir, "check", Some(&first));
    assert_eq!(jobs[0], "-p pa --lib", "{stdout}");
    assert!(!stdout.contains("order:"), "{stdout}");
}

/// **A fake `cargo` whose library job of `pb` fails.** `metadata` and the
/// listing are the ones of [`write_cargo`]; every `test` call passes except the
/// one for `pb`'s library.
fn write_red_cargo(dir: &Path) -> PathBuf {
    let green = write_cargo(dir);
    let script = fs::read_to_string(&green).expect("read the green stub");
    let red = script.replace(
        "  test)\n",
        "  test)\n    case \" $* \" in *\" -p pb --lib \"*) echo 'test the_case ... FAILED'; echo 'test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'; exit 101 ;; esac\n",
    );
    support::write_executable(&green, &red);
    green
}

fn calls(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

fn selection_fixture() -> (PathBuf, String) {
    let dir = scratch();
    let base = repository(&dir);
    let cargo = write_cargo(&dir);
    let script = fs::read_to_string(&cargo).unwrap();
    let script = script.replace(
        "\"dependencies\":[],\n   \"targets\":[{\"kind\":[\"lib\"],\"name\":\"pb\"}",
        "\"dependencies\":[{\"name\":\"pa\"}],\n   \"targets\":[{\"kind\":[\"bin\"],\"name\":\"pb\"},{\"kind\":[\"lib\"],\"name\":\"pb\"}",
    );
    let root = dir.display();
    let script = script.replace(
        "\n]}",
        &format!("\n,{{\"name\":\"pc\",\"manifest_path\":\"{root}/crates/pc/Cargo.toml\",\"dependencies\":[{{\"name\":\"pb\"}}],\"targets\":[{{\"kind\":[\"lib\"],\"name\":\"pc\"}}]}}\n,{{\"name\":\"pd\",\"manifest_path\":\"{root}/crates/pd/Cargo.toml\",\"dependencies\":[],\"targets\":[{{\"kind\":[\"lib\"],\"name\":\"pd\"}},{{\"kind\":[\"test\"],\"name\":\"delta\"}}]}}\n]}}"),
    );
    let script = script.replace(
        "    echo '  Executable tests/beta.rs (/x/deps/beta-4)'",
        "    echo '  Executable tests/beta.rs (/x/deps/beta-4)'\n    echo '  Executable unittests src/lib.rs (/x/deps/pc-5)'\n    echo '  Executable unittests src/lib.rs (/x/deps/pd-6)'\n    echo '  Executable tests/delta.rs (/x/deps/delta-7)'\n    echo '  Executable unittests src/main.rs (/x/deps/pa-8)'",
    );
    support::write_executable(&cargo, &script);
    fs::create_dir_all(dir.join("scripts")).unwrap();
    for name in ["select-crates", "ci-full-paths.txt"] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts")
            .join(name);
        if source.exists() {
            fs::copy(source, dir.join("scripts").join(name)).unwrap();
        }
    }
    (dir, base)
}

fn crate_selection(dir: &Path, base: &str) -> serde_json::Value {
    let out = support::output_fenced(
        Command::new("python3")
            .arg(dir.join("scripts/select-crates"))
            .current_dir(dir)
            .env("CARGO", dir.join("cargo"))
            .env("BAR_SELECT_BASE", base),
    )
    .unwrap();
    assert!(out.status.success(), "{out:?}");
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn ci_crate_domain_change_selects_transitive_dependents_and_skips_unrelated_tests() {
    let (dir, _) = selection_fixture();
    let selected = crate_selection(&dir, "main");
    assert_eq!(
        selected["packages"],
        serde_json::json!(["pa", "pb", "pc"]),
        "{selected}"
    );
    assert!(
        selected["reason"]
            .as_str()
            .unwrap()
            .contains("pc: depends on pb"),
        "{selected}"
    );
}

#[test]
fn ci_crate_report_executes_only_selected_jobs_but_boundary_runs_everything() {
    for (lane, enabled) in [("report", true), ("check", true), ("report", false)] {
        let (dir, base) = selection_fixture();
        let out = support::output_fenced(
            Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
                .arg(lane)
                .current_dir(&dir)
                .env("CARGO", dir.join("cargo"))
                .env("BAR_HISTORY_DIR", dir.join("history"))
                .env("BAR_SELECT_BY_CRATE", if enabled { "1" } else { "0" })
                .env("BAR_SELECT_BASE", &base)
                .env("BAR_PUSH_BASE", &base)
                .env("BAR_JOBS", "1"),
        )
        .unwrap();
        assert!(out.status.success(), "{out:?}");
        let jobs: Vec<_> = calls(&dir)
            .into_iter()
            .filter(|c| c.starts_with("test ") && !c.contains("--no-run"))
            .collect();
        if lane == "report" && enabled {
            assert!(
                !jobs.iter().any(|c| c.contains("-p pd")
                    || c.contains("--test delta")
                    || c.contains("-p pa")),
                "unrelated tests ran: {jobs:?}"
            );
            assert!(jobs.iter().any(|c| c.contains("-p pb --lib")), "{jobs:?}");
            assert!(jobs.iter().any(|c| c.contains("-p pc --lib")), "{jobs:?}");
            for kind in ["--doc", "--bins", "--test beta"] {
                assert!(
                    jobs.iter().any(|c| c.contains(kind)
                        && c.contains("-p pb -p pc")
                        && !c.contains("--workspace")),
                    "scope lost for {kind}: {jobs:?}"
                );
            }
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("pc: depends on pb"),
                "{out:?}"
            );
        } else {
            assert!(
                jobs.iter().any(|c| c.contains("-p pd --lib")),
                "boundary narrowed: {jobs:?}"
            );
            assert!(jobs.iter().any(|c| c.contains("--test delta")), "{jobs:?}");
        }
    }
}

#[test]
fn ci_crate_test_only_change_selects_its_crate() {
    let (dir, base) = selection_fixture();
    // The base already contains pa's edit; only pb's test differs now.
    fs::write(dir.join("crates/pb/src/lib.rs"), "// base\n").unwrap();
    fs::create_dir_all(dir.join("crates/pb/tests")).unwrap();
    fs::write(dir.join("crates/pb/tests/beta.rs"), "// case\n").unwrap();
    git(&dir, &["add", "crates/pb"]);
    git(&dir, &["commit", "-q", "-m", "test only"]);
    let selected = crate_selection(&dir, &base);
    assert_eq!(
        selected["packages"],
        serde_json::json!(["pb", "pc"]),
        "{selected}"
    );
    assert!(
        selected["reason"].as_str().unwrap().contains("pb: changed"),
        "{selected}"
    );
}

#[test]
fn ci_crate_non_code_behaviour_paths_force_full_tests() {
    for path in [
        "crates/pa/served-texts/read.txt",
        "crates/pa/migrations/001.sql",
        "crates/pa/tests/fixtures/input.json",
        "crates/pa/tests/snapshots/read.snap",
        "size-ratchet.toml",
        ".forgejo/workflows/branch.yml",
        "Cargo.lock",
        "flake.nix",
        "flake.lock",
        "Cargo.toml",
        "scripts/select-crates",
    ] {
        let (dir, base) = selection_fixture();
        let target = dir.join(path);
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, "changed\n").unwrap();
        git(&dir, &["add", path]);
        git(&dir, &["commit", "-q", "-m", "behaviour changed"]);
        // Keep the executable selector separate from the changed fixture path.
        let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/select-crates");
        let out = support::output_fenced(
            Command::new("python3")
                .arg(source)
                .current_dir(&dir)
                .env("CARGO", dir.join("cargo"))
                .env("BAR_SELECT_BASE", &base),
        )
        .unwrap();
        let selected: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(selected["full"], true, "{path}: {selected}");
        assert!(
            selected["reason"].as_str().unwrap().contains(path),
            "{path}: {selected}"
        );
    }
}

#[test]
fn ci_crate_unreadable_inputs_keep_full_and_plain_docs_can_reuse_proof() {
    let (dir, _) = selection_fixture();
    for base in ["missing-carry-ref", "HEAD"] {
        let selected = crate_selection(&dir, base);
        assert_eq!(selected["full"], true, "{base}: {selected}");
        assert_eq!(selected["allow_docs"], false, "{selected}");
    }
    let head = git(&dir, &["rev-parse", "HEAD"]);
    fs::write(dir.join("README.md"), "documentation\n").unwrap();
    git(&dir, &["add", "README.md"]);
    git(&dir, &["commit", "-q", "-m", "docs"]);
    let selected = crate_selection(&dir, &head);
    assert_eq!(selected["full"], true, "{selected}");
    assert_eq!(selected["allow_docs"], true, "{selected}");
    support::write_executable(&dir.join("cargo"), "#!/bin/sh\necho 'not JSON'\n");
    let selected = crate_selection(&dir, &head);
    assert_eq!(selected["full"], true, "{selected}");
    assert_eq!(selected["allow_docs"], false, "{selected}");
}

/// **The touched crates' library cases run before anything is built for the
/// workspace.** A case a push broke there is red before the build and before the
/// test binaries of the other crates are compiled. **Paired**: a green run runs
/// those cases once and not a second time with the other jobs.
#[test]
fn the_touched_crates_library_cases_run_before_the_workspace_is_built() {
    let dir = scratch();
    let first = repository(&dir);
    write_red_cargo(&dir);
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("report")
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", dir.join("cargo"))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("BAR_JOBS", "1")
            .env("BAR_PUSH_BASE", &first),
    )
    .expect("run bar");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(!out.status.success(), "{stdout}");
    assert!(stdout.contains("fast: FAILED"), "{stdout}");
    let seen = calls(&dir);
    assert!(
        seen.iter().any(|call| call.contains("-p pb --lib")),
        "{seen:?}"
    );
    assert!(
        !seen.iter().any(|call| call.starts_with("build ")),
        "the workspace was built after the fast cases were red: {seen:?}"
    );
    assert!(
        !seen.iter().any(|call| call.contains("--no-run")),
        "the test binaries were built after the fast cases were red: {seen:?}"
    );

    let dir = scratch();
    let first = repository(&dir);
    let (jobs, stdout) = jobs_run(&dir, "report", Some(&first));
    assert!(stdout.contains("fast: ok"), "{stdout}");
    assert_eq!(
        jobs.iter()
            .filter(|job| job.contains("-p pb --lib"))
            .count(),
        1,
        "the cases that ran first ran again: {jobs:?}"
    );
    let built = calls(&dir)
        .iter()
        .position(|call| call.starts_with("build "))
        .expect("a green run builds");
    let fast = calls(&dir)
        .iter()
        .position(|call| call.contains("-p pb --lib"))
        .expect("the fast cases ran");
    assert!(fast < built, "the fast cases ran after the build");
}

/// **The library cases of `jojobot-exercise` wait for the build.** They open
/// rooms that spawn the built `jojobot` binary, so run first on a runner whose
/// target holds none, they are red for no change of the code. The push touched
/// only that package, so no case runs ahead of the build; the same stub names
/// `pb` instead in [`the_touched_crates_library_cases_run_before_the_workspace_is_built`],
/// where they do.
#[test]
fn the_library_cases_that_spawn_the_binary_run_after_the_build() {
    let dir = scratch();
    let first = repository(&dir);
    let script = fs::read_to_string(write_cargo(&dir)).expect("read the stub");
    support::write_executable(
        &dir.join("cargo"),
        &script
            .replace("\"name\":\"pb\"", "\"name\":\"jojobot-exercise\"")
            .replace("deps/pb-2", "deps/jojobot_exercise-2"),
    );
    let out = support::output_fenced(
        Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
            .arg("report")
            .current_dir(&dir)
            .env("TMPDIR", &dir)
            .env("CARGO", dir.join("cargo"))
            .env("BAR_HISTORY_DIR", dir.join("history"))
            .env("BAR_JOBS", "1")
            .env("BAR_PUSH_BASE", &first),
    )
    .expect("run bar");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(out.status.success(), "{stdout}");
    assert!(!stdout.contains("fast:"), "{stdout}");
    let seen = calls(&dir);
    let built = seen
        .iter()
        .position(|call| call.starts_with("build "))
        .expect("a green run builds");
    let cases = seen
        .iter()
        .position(|call| call.starts_with("test ") && call.contains("-p jojobot-exercise --lib"))
        .expect("the cases still run");
    assert!(built < cases, "the cases ran before the build: {seen:?}");
}

/// **A repository the way the forge's checkout is: `origin/main` and no local
/// `main`.** The branch has one commit, which touches `pb`.
fn repository_with_only_the_remote_main(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    for package in ["pa", "pb"] {
        let src = dir.join("crates").join(package).join("src");
        fs::create_dir_all(&src).expect("package dir");
        fs::write(src.join("lib.rs"), "// base\n").expect("write");
    }
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "base"]);
    git(dir, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(dir, &["switch", "-q", "-c", "work"]);
    fs::write(dir.join("crates/pb/src/lib.rs"), "// pb changed\n").expect("write");
    git(dir, &["commit", "-q", "-am", "touch pb"]);
    git(dir, &["branch", "-q", "-D", "main"]);
}

/// **With no commit the push moved from and no local `main`, the merge base with
/// `origin/main` says what the branch touched.** The forge's checkout holds only
/// the remote-tracking ref, so a fallback that asked for `main` alone found
/// nothing, and a branch's first push, a dispatch or a rewritten branch ran with
/// the jobs in cargo's order.
#[test]
fn the_fallback_range_is_the_merge_base_with_origin_main_when_there_is_no_local_main() {
    let dir = scratch();
    repository_with_only_the_remote_main(&dir);
    let (jobs, stdout) = jobs_run(&dir, "report", None);
    assert_eq!(
        jobs,
        [
            "-p pb --lib",
            "--test beta",
            "-p pa --lib",
            "--test alpha",
            "--doc"
        ],
        "{stdout}"
    );
    assert!(
        stdout.contains("order:") && stdout.contains("pb"),
        "{stdout}"
    );
}
