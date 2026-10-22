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
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-touched-first-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
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
