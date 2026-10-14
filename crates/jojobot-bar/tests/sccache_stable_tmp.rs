//! **The wrapper that runs sccache under a temp directory no run owns.**
//!
//! sccache keeps no temp setting of its own: its server uses the `TMPDIR` of
//! whatever started it, and a bar run starts cargo with a `TMPDIR` of that run's
//! own, which the run removes when it ends. A server that outlives the run then
//! fails every build with "Failed to create temp dir". The dev shell points
//! `RUSTC_WRAPPER` at `scripts/sccache-stable-tmp`, which runs the real sccache
//! with a `TMPDIR` under the user's cache directory. A stand-in sccache records
//! what it was started with, so no real server is touched.

use std::path::{Path, PathBuf};
use std::process::Command;

mod support;

fn wrapper() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/sccache-stable-tmp")
}

fn scratch(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-sccache-wrapper-{tag}-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// **A stand-in sccache** that writes the temp directory it ran with and its
/// arguments, one per line, to the file named by `RECORD`.
fn stand_in(dir: &Path) -> PathBuf {
    let path = dir.join("sccache");
    support::write_executable(
        &path,
        "#!/bin/sh\n{ echo \"$TMPDIR\"; for a in \"$@\"; do echo \"$a\"; done; } > \"$RECORD\"\n",
    );
    path
}

/// Run the wrapper as cargo would: the run's own `TMPDIR` in the environment,
/// the real sccache named in `JOJOBOT_SCCACHE`, and a rustc call after it.
fn run(dir: &Path, cache_home: Option<&Path>, home: &Path) -> std::process::Output {
    let mut command = Command::new(wrapper());
    command
        .args(["rustc", "--crate-name", "example"])
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("TMPDIR", dir.join("run-root"))
        .env("JOJOBOT_SCCACHE", stand_in(dir))
        .env("RECORD", dir.join("record"));
    if let Some(cache_home) = cache_home {
        command.env("XDG_CACHE_HOME", cache_home);
    }
    support::output_fenced(&mut command).expect("the wrapper starts")
}

fn recorded(dir: &Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("record"))
        .expect("the stand-in ran")
        .lines()
        .map(str::to_string)
        .collect()
}

/// **sccache runs with a temp directory under the cache home, not the run's**,
/// the directory exists, and the call's arguments arrive in order. The run's own
/// temp directory is named in the environment, so a wrapper that passed the
/// environment through unchanged would record it instead.
#[test]
fn sccache_runs_with_a_temp_directory_no_run_owns() {
    let dir = scratch("stable");
    let cache = dir.join("cache");

    let out = run(&dir, Some(&cache), &dir.join("home"));

    assert!(out.status.success(), "{out:?}");
    let stable = cache.join("jojobot-sccache-tmp");
    assert_eq!(
        recorded(&dir),
        vec![
            stable.to_string_lossy().into_owned(),
            "rustc".into(),
            "--crate-name".into(),
            "example".into()
        ]
    );
    assert!(stable.is_dir(), "the stable directory was not made");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// **Without a cache home the user's `.cache` is used.**
#[test]
fn without_a_cache_home_the_users_cache_directory_holds_it() {
    let dir = scratch("home");
    let home = dir.join("home");

    let out = run(&dir, None, &home);

    assert!(out.status.success(), "{out:?}");
    assert_eq!(
        recorded(&dir)[0],
        home.join(".cache")
            .join("jojobot-sccache-tmp")
            .to_string_lossy()
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// **A directory that was removed is made again by the next call**, so a server
/// that kept this temp directory finds it there at its next request.
#[test]
fn a_removed_stable_directory_is_made_again_by_the_next_call() {
    let dir = scratch("again");
    let cache = dir.join("cache");
    run(&dir, Some(&cache), &dir.join("home"));
    let stable = cache.join("jojobot-sccache-tmp");
    std::fs::remove_dir_all(&stable).expect("the directory goes");

    let out = run(&dir, Some(&cache), &dir.join("home"));

    assert!(out.status.success(), "{out:?}");
    assert!(stable.is_dir(), "the directory was not made again");
    std::fs::remove_dir_all(&dir).expect("cleanup");
}

/// **With no sccache named, the wrapper fails and says so**, rather than running
/// nothing and reporting success to cargo.
#[test]
fn with_no_sccache_named_the_wrapper_fails_and_says_why() {
    let dir = scratch("unnamed");
    let mut command = Command::new(wrapper());
    command
        .arg("rustc")
        .env_clear()
        .env("PATH", std::env::var("PATH").unwrap_or_default())
        .env("HOME", dir.join("home"));

    let out = support::output_fenced(&mut command).expect("the wrapper starts");

    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("JOJOBOT_SCCACHE"),
        "{out:?}"
    );
    std::fs::remove_dir_all(&dir).expect("cleanup");
}
