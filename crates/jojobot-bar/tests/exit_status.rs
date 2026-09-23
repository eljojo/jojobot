//! **The bar's own exit status must survive the whole path**, from cargo's
//! exit code to the process that ran `bar`. Testing `test_healthy` alone
//! proves the library function; it says nothing about whether `run_check`
//! actually calls it with the real exit status rather than a discarded one.
//! A stub `cargo` drives the real `bar` binary end to end to prove that.

use std::fs;
use std::io::Write as _;
use std::process::Command;

/// **A fake `cargo` that mimics a crashed sibling suite.** `fmt`, `build`
/// and `clippy` succeed silently; `test` prints one clean `test result:`
/// line — as a real sibling suite under `--no-fail-fast` would — and then
/// exits non-zero, exactly as the crashed suite that printed nothing could
/// not stop it from doing.
fn write_fake_cargo(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let mut f = fs::File::create(&path).expect("write fake cargo");
    let script = r#"#!/bin/sh
case "$1" in
  test) echo 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 1 ;;
  *) exit 0 ;;
esac
"#;
    f.write_all(script.as_bytes())
        .expect("write fake cargo body");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).expect("stat fake cargo").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod fake cargo");
    path
}

fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// **A crashed sibling suite must red the whole `bar check` run**, not read
/// as a clean pass because the one suite that printed a `test result:` line
/// printed a clean one. This is the exact hazard `run_check` used to miss:
/// it decided the test phase from the text alone, discarding cargo's own
/// exit status.
#[test]
fn a_crashed_sibling_suite_reds_the_whole_check_run() {
    let dir = scratch_dir("check");
    let fake_cargo = write_fake_cargo(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .arg("check")
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .output()
        .expect("run bar check");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !output.status.success(),
        "a crashed sibling suite must not exit zero: {stdout}"
    );
    assert!(
        stdout.contains("verdict: RED"),
        "a crashed sibling suite must not read GREEN: {stdout}"
    );
    assert!(
        stdout.contains("crashed or was killed"),
        "the reader needs to know this was not a counted test failure: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}
