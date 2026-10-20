//! **No two tests that start a real store can share a scratch directory.**
//!
//! A directory named for the process alone is the same directory for every test
//! in that process. Two real stores on one data directory fail only when both
//! run, which reads as load and costs a re-run each time. Every scratch path now
//! comes from [`jojobot_adapters::testing::scratch_path`], which draws a number
//! per call, and this case fails any real-store test that builds its own.
//!
//! The rule is read off the sources, because a test that bypasses the helper
//! passes every other check. **A file is a real-store test when it starts a
//! store or a server that does**, and it must not call `temp_dir` itself.

use std::path::{Path, PathBuf};

/// What marks a file as one that starts a real store, or the server that starts
/// one. Each spelling is one a test in this workspace uses today.
const STARTS_A_STORE: &[&str] = &[
    "Dolt::start",
    "start_unhurried",
    "start_acquiring",
    "migrated_database",
    "STATE_DIRECTORY",
    "JOJOBOT_STORE_PORT",
    "free_port(",
];

/// The call a test makes to build a scratch path of its own.
const BUILDS_ITS_OWN: &str = "temp_dir()";

/// **The lines of one file that build a scratch path where the file starts a
/// real store**, as 1-based line numbers. Empty for a file that starts none.
fn bypasses(source: &str) -> Vec<usize> {
    if !STARTS_A_STORE.iter().any(|mark| source.contains(mark)) {
        return Vec::new();
    }
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains(BUILDS_ITS_OWN))
        .map(|(at, _)| at + 1)
        .collect()
}

/// Every `.rs` file under the crates, skipping build output.
fn sources(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("a directory of the workspace reads") {
        let path = entry.expect("an entry reads").path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            sources(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            found.push(path);
        }
    }
}

/// **Where the rule does not apply, and why.** The helper is the one place that
/// asks for the temp directory. The rooms of the exercise crate draw their
/// directory from their own helper, which a case there holds to one directory
/// per room made at one instant.
fn is_exempt(path: &str) -> bool {
    path.ends_with("crates/jojobot-adapters/src/testing.rs")
        || path.contains("crates/jojobot-exercise/")
        || path.ends_with("crates/jojobot-adapters/tests/real_store_scratch.rs")
}

/// **The scan, over the real tree.** Counted first, so a walk that reaches
/// nothing cannot pass.
#[test]
fn no_real_store_test_builds_its_own_scratch_path() {
    let crates = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .expect("the crates directory resolves");
    let mut files = Vec::new();
    sources(&crates, &mut files);
    let mut real_store_files = 0;
    let mut offenders = Vec::new();
    for file in &files {
        let shown = file.to_string_lossy().into_owned();
        if is_exempt(&shown) {
            continue;
        }
        let source = std::fs::read_to_string(file).expect("a source reads");
        if STARTS_A_STORE.iter().any(|mark| source.contains(mark)) {
            real_store_files += 1;
        }
        for at in bypasses(&source) {
            offenders.push(format!("{shown}:{at}"));
        }
    }
    assert!(
        real_store_files >= 30,
        "the scan reached only {real_store_files} real-store files, so it asserts nothing"
    );
    assert!(
        offenders.is_empty(),
        "these lines build their own scratch path in a test that starts a real store; \
         use jojobot_adapters::testing::scratch_path: {offenders:?}"
    );
}

/// **The rule tells the three shapes apart.** A test that starts a store and
/// asks for the temp directory itself is flagged. One that goes through the
/// helper is not. A file that starts no store may use the temp directory as it
/// likes.
#[test]
fn the_scan_flags_a_bypass_and_leaves_the_helper_and_other_files_alone() {
    let bypass = "let port = free_port();\n\
                  let dir = std::env::temp_dir().join(format!(\"jojobot-x-{}\", std::process::id()));\n";
    assert_eq!(bypasses(bypass), [2]);

    let through_the_helper = "let port = free_port();\n\
                              let dir = jojobot_adapters::testing::scratch_path(\"jojobot-x\");\n";
    assert!(bypasses(through_the_helper).is_empty());

    let no_store = "let dir = std::env::temp_dir().join(\"jojobot-bar-case\");\n";
    assert!(bypasses(no_store).is_empty());
}
