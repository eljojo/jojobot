//! **The size report never moves `bar check`'s exit code.** It is print-only,
//! run first and unconditionally, whichever phase after it passes or fails —
//! proven here by driving the real binary against a real git tree, not by
//! reading the library functions' return values in isolation.

use std::fs;
use std::io::Write as _;
use std::process::Command;

/// **A fake `cargo` where every phase succeeds.** Isolates the size report
/// from the rest of `run_check`: whatever this test asserts about the exit
/// code and the printed report is not entangled with a phase failing on its
/// own account.
fn write_fake_cargo_always_green(dir: &std::path::Path) -> std::path::PathBuf {
    let path = dir.join("cargo");
    let mut f = fs::File::create(&path).expect("write fake cargo");
    let script = r#"#!/bin/sh
case " $* " in
  *" --list "*) echo 'some::test::name: test'; exit 0 ;;
esac
case "$1" in
  test) echo 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.01s'; exit 0 ;;
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
        "jojobot-bar-size-test-{tag}-{}-{nanos}",
        std::process::id()
    ));
    fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// A git repo of its own, isolated from this workspace's — `git ls-files`
/// reads whatever repo `bar` is run inside, so the scratch dir has to be one.
fn git_init(dir: &std::path::Path) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("run git")
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "scratch@example.com"]);
    run(&["config", "user.name", "scratch"]);
}

fn git_add(dir: &std::path::Path, file: &str) {
    Command::new("git")
        .args(["add", file])
        .current_dir(dir)
        .status()
        .expect("git add");
}

#[test]
fn an_oversized_tracked_file_is_named_and_the_exit_code_is_unchanged() {
    let dir = scratch_dir("present");
    git_init(&dir);
    let big = "x\n".repeat(2001);
    fs::write(dir.join("big.rs"), big).expect("write big.rs");
    git_add(&dir, "big.rs");
    let fake_cargo = write_fake_cargo_always_green(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .arg("check")
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .output()
        .expect("run bar check");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "an oversized file must not change the exit code: {stdout}"
    );
    assert!(stdout.contains("verdict: GREEN"), "{stdout}");
    assert!(
        stdout.contains("big.rs (2001 lines)"),
        "the specific file must be named, not just a count: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}

/// **The paired negative, in the same shape.** A tree with nothing over the
/// threshold names nothing — the count is zero and the file that IS there
/// never appears in that list.
#[test]
fn a_tree_with_nothing_oversized_names_nothing_and_the_exit_code_is_unchanged() {
    let dir = scratch_dir("absent");
    git_init(&dir);
    fs::write(dir.join("small.rs"), "fn main() {}\n").expect("write small.rs");
    git_add(&dir, "small.rs");
    let fake_cargo = write_fake_cargo_always_green(&dir);

    let output = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .arg("check")
        .current_dir(&dir)
        .env("CARGO", &fake_cargo)
        .output()
        .expect("run bar check");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "a clean tree must exit zero: {stdout}"
    );
    assert!(stdout.contains("verdict: GREEN"), "{stdout}");
    assert!(stdout.contains("files over 2000 lines: 0"), "{stdout}");
    assert!(
        !stdout.contains("small.rs ("),
        "the file that IS there must not be named as oversized: {stdout}"
    );

    let _ = fs::remove_dir_all(&dir);
}
