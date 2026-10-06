//! **The experiment's text is frozen.**
//!
//! The room documents and the two surface patches are what the four conditions
//! ARE. An edit to any of them after the first paid run would make a row from
//! before and a row from after two different experiments, with nothing in the
//! table to say so. The manifest records each file's sha256, and this fails the
//! moment one drifts, so an edit has to be a deliberate re-freeze: regenerate
//! the manifest in the same commit and say why.

use std::process::Command;

const MANIFEST: &str = "experiments/adoption/frozen.sha256";

/// The four files the four conditions are made of.
const FROZEN: [&str; 4] = [
    "rooms/adoption.md",
    "rooms/adoption-wording.md",
    "experiments/adoption/e2-boot-names-keys.patch",
    "experiments/adoption/e3-receipt-teaches-type.patch",
];

fn crate_dir() -> &'static std::path::Path {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
}

/// **Every frozen file hashes to what the manifest recorded.**
#[test]
fn the_frozen_files_match_their_recorded_hashes() {
    let checked = Command::new("sha256sum")
        .arg("--check")
        .arg(MANIFEST)
        .current_dir(crate_dir())
        .output()
        .expect("sha256sum runs");
    let said = String::from_utf8_lossy(&checked.stdout).to_string()
        + &String::from_utf8_lossy(&checked.stderr);
    assert!(
        checked.status.success(),
        "a frozen file drifted. Re-freeze it deliberately, in the same commit, with the reason:\n{said}",
    );
}

/// **The manifest names exactly those four, so no arm is quietly unfrozen.**
///
/// The check above passes on a manifest that lists one file, or none that
/// matter.
#[test]
fn the_manifest_names_the_four_files_and_no_other() {
    let text = std::fs::read_to_string(crate_dir().join(MANIFEST)).expect("the manifest is there");
    let mut named: Vec<&str> = text
        .lines()
        .filter_map(|line| line.split_whitespace().nth(1))
        .collect();
    named.sort_unstable();
    let mut expected = FROZEN.to_vec();
    expected.sort_unstable();
    assert_eq!(
        named, expected,
        "the manifest freezes a different set: {text}"
    );
}
