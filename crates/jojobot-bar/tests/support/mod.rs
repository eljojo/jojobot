//! **Writing an executable and starting children, without the two overlapping.**
//!
//! Every file here drives the real `bar` binary with a stub `cargo` it writes
//! itself, and its cases run on threads released together. A child started on
//! another thread while a stub is still open for writing carries a copy of that
//! handle until it execs, and an exec of the stub in that window fails with
//! "Text file busy". The stub is not at fault and neither is the bar under
//! test, so the run reads as a bar that stopped after one phase, or never ran
//! one.
//!
//! Writing a stub takes the write side of [`FORK_FENCE`]; starting a child takes
//! the read side from the fork until the exec has happened, and releases it
//! before waiting, so children still run side by side. A rename would not do
//! instead: the copy of the handle belongs to the file, not to its name.
//!
//! A test file includes this with `mod support;` and writes every executable
//! and starts every child through it. The fence is per process, which is the
//! only place the race can happen, so each test file has its own.

#![allow(dead_code)]

use std::io::Write as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::RwLock;

/// Held for writing while an executable is open, and for reading from the fork
/// of a child to its exec.
static FORK_FENCE: RwLock<()> = RwLock::new(());

/// **Write an executable file**, closed and runnable when this returns, with no
/// child able to start in this process while it was open.
pub fn write_executable(path: &Path, script: &str) {
    let _fence = FORK_FENCE.write().unwrap_or_else(|e| e.into_inner());
    let mut file = std::fs::File::create(path).expect("write the executable");
    file.write_all(script.as_bytes())
        .expect("write the executable's body");
    drop(file);
    let mut perms = std::fs::metadata(path)
        .expect("stat the executable")
        .permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).expect("chmod the executable");
}

/// A narrow-loop Cargo world declares one testable binary and its allowed label.
pub fn write_loop_cargo(path: &Path, script: &str) {
    let root = path.parent().unwrap();
    let metadata = serde_json::json!({
        "workspace_root": root,
        "packages": [{"name": "jojobot-bar", "manifest_path": root.join("Cargo.toml"),
            "dependencies": [], "targets": [{"name": "jojobot-bar", "kind": ["bin"], "test": true}]}]
    });
    std::fs::write(root.join("loop-metadata.json"), metadata.to_string()).unwrap();
    let file = root.join("crates/jojobot-bar/build-metrics-binaries.txt");
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(file, "bins\n").unwrap();
    let body = script.strip_prefix("#!/bin/sh\n").unwrap();
    write_executable(
        path,
        &format!(
            r#"#!/bin/sh
if [ "$1" = metadata ]; then
  echo "$*" >> "$(dirname "$0")/argv.log"
  cat "$(dirname "$0")/loop-metadata.json"
  exit $?
fi
{body}"#
        ),
    );
}

/// **Start a child, from the fork to the exec.** Every child a case starts goes
/// through here, so this is the one place that decides how they start.
pub fn fenced_spawn(cmd: &mut Command) -> std::io::Result<Child> {
    let _fence = FORK_FENCE.read().unwrap_or_else(|e| e.into_inner());
    // A scratch repo has its own base and CI context.
    for name in ["SIZE_BASE", "BAR_DEV_BASE", "GITHUB_ACTIONS"] {
        if !cmd.get_envs().any(|(key, _)| key == name) {
            cmd.env_remove(name);
        }
    }
    cmd.spawn()
}

/// **Run a child to its end and take what it printed**, started through the
/// fence. Standard input is closed and both output streams are captured, as
/// `Command::output` does.
pub fn output_fenced(cmd: &mut Command) -> std::io::Result<Output> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    fenced_spawn(cmd)?.wait_with_output()
}

/// **Run a child to its end and take its status**, started through the fence.
/// Standard input and output are inherited, as `Command::status` does.
pub fn status_fenced(cmd: &mut Command) -> std::io::Result<ExitStatus> {
    fenced_spawn(cmd)?.wait()
}

/// **Make a scratch directory a git repository of its own.** The bar's size
/// phase lists the tracked files of the repository it runs in, and a directory
/// that is no repository is a measurement it cannot make, which fails the run.
pub fn git_init(dir: &Path) {
    status_fenced(Command::new("git").args(["init", "-q"]).current_dir(dir))
        .expect("git init in a scratch directory");
}

/// Local-loop gates need a branch base even when a case changes no source file.
pub fn loop_repository(dir: &Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &[
            "-c",
            "user.name=scratch",
            "-c",
            "user.email=scratch@example.com",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "--allow-empty",
            "-q",
            "-m",
            "scratch",
        ][..],
    ] {
        let status = status_fenced(Command::new("git").args(args).current_dir(dir))
            .expect("prepare a local-loop repository");
        assert!(status.success(), "git {args:?} in {dir:?}");
    }
}
