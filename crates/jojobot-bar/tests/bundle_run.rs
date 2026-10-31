//! **`bar bundle` builds the room-harness binary last, and a failed harness build
//! fails the bundle.** A stub `cargo` records every call, so the assertions are
//! over what the real `bar` binary spawned.
//!
//! The harness build is `room_server::bundle`, called once between the test
//! compiles and the archive. Compiling the ordinary test lane can replace the
//! profile's binary with the default build, so the harness binary has to be
//! selected after every lane; and a bundle whose room binary did not build must
//! not be archived. Before these cases deleting the call, or moving it before
//! the lanes, left every case green.

use std::fs;
use std::path::Path;
use std::process::Command;

mod support;

/// A stub cargo: metadata names one package, a `--no-run` test call lists one
/// binary, and the harness build fails when the case asks for it.
fn write_cargo(dir: &Path) {
    let metadata = serde_json::json!({
        "workspace_root": dir,
        "packages": [{"name": "pa", "manifest_path": dir.join("Cargo.toml"),
            "dependencies": [], "targets": [{"name": "pa", "kind": ["lib"], "test": true}]}]
    });
    fs::write(dir.join("metadata.json"), metadata.to_string()).unwrap();
    support::write_executable(
        &dir.join("cargo"),
        r#"#!/bin/sh
here="$(dirname "$0")"
echo "$*" >> "$here/argv.log"
case "$1" in
  metadata) cat "$here/metadata.json" ;;
  test) echo "  Executable unittests src/lib.rs (target/debug/deps/pa-1)" ;;
  build)
    case "$*" in
      *room-harness*) [ -f "$here/harness-fails" ] && exit 1 ;;
    esac ;;
esac
exit 0
"#,
    );
}

/// Run `bar bundle` in a scratch workspace and give back its exit code, its
/// output and the calls the stub cargo saw, in order.
fn bundle(tag: &str, harness_fails: bool) -> (Option<i32>, String, Vec<String>) {
    let dir = std::env::temp_dir().join(jojobot_bar::scratch::name(&format!(
        "jojobot-bar-test-{tag}"
    )));
    fs::create_dir_all(dir.join("target/debug/deps")).expect("create scratch dir");
    fs::write(dir.join("target/debug/deps/pa-1"), "binary\n").unwrap();
    support::loop_repository(&dir);
    write_cargo(&dir);
    if harness_fails {
        fs::write(dir.join("harness-fails"), "").unwrap();
    }
    let mut command = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"));
    command
        .arg("bundle")
        .current_dir(&dir)
        .env("TMPDIR", &dir)
        .env("BAR_HISTORY_DIR", dir.join("history"))
        .env("CARGO", dir.join("cargo"))
        .stdout(fs::File::create(dir.join("stdout")).unwrap());
    let status = support::fenced_spawn(&mut command)
        .expect("run bar bundle")
        .wait()
        .unwrap();
    let stdout = fs::read_to_string(dir.join("stdout")).unwrap();
    let calls = fs::read_to_string(dir.join("argv.log"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let _ = fs::remove_dir_all(&dir);
    (status.code(), stdout, calls)
}

const HARNESS_BUILD: &str = "build --workspace --locked --features=jojobot/room-harness";

/// **The harness binary is built after every test compile.** The profile's
/// binary can be replaced by the default build while the test lanes compile, so
/// the room binary has to come last. **Paired**: exactly one harness build is
/// made, so a build that ran it in every lane would not pass.
#[test]
fn the_harness_binary_is_built_once_after_every_test_compile() {
    let (_, stdout, calls) = bundle("order", false);
    let harness: Vec<usize> = calls
        .iter()
        .enumerate()
        .filter(|(_, call)| call.as_str() == HARNESS_BUILD)
        .map(|(at, _)| at)
        .collect();
    assert_eq!(
        harness.len(),
        1,
        "one harness build is made: {calls:?}\n{stdout}"
    );
    let last_compile = calls
        .iter()
        .rposition(|call| call.starts_with("test ") && call.contains("--no-run"))
        .unwrap_or_else(|| panic!("no test compile ran: {calls:?}\n{stdout}"));
    assert!(
        harness[0] > last_compile,
        "the harness binary was built before a test compile: {calls:?}"
    );
}

/// **A harness build that fails fails the bundle, and names its phase.** A
/// bundle holding no room binary would make every room case 404 on its grader
/// route. **Paired** with the same bundle where the build passes, which reaches
/// the harness phase green.
#[test]
fn a_failed_harness_build_fails_the_bundle_and_names_its_phase() {
    let (code, stdout, _) = bundle("fails", true);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(
        stdout.contains("harness-server: FAILED"),
        "the verdict does not name the harness phase: {stdout}"
    );

    let (_, stdout, _) = bundle("passes", false);
    assert!(
        stdout.contains("harness-server: ok"),
        "a harness build that passed did not reach its phase: {stdout}"
    );
}
