//! The room's privileged grader never enters a shipped build.

use std::path::Path;
use std::process::Command;

#[test]
fn the_shipped_profiles_do_not_enable_the_room_harness() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let result = Command::new("cargo")
        .current_dir(&root)
        .args([
            "metadata",
            "--offline",
            "--locked",
            "--no-deps",
            "--format-version",
            "1",
        ])
        .output()
        .expect("cargo metadata runs without building");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&result.stdout).expect("metadata JSON");
    let packages = metadata["packages"].as_array().expect("packages");
    let product = packages
        .iter()
        .find(|package| package["name"] == "jojobot")
        .expect("the shipped package");
    assert!(product["features"].get("room-harness").is_some());
    // The shipped derivation builds the workspace with default features.
    // Follow aliases too, so renaming the default entry cannot hide the mode.
    for package in packages {
        let mut pending = vec!["default"];
        let mut visited = std::collections::BTreeSet::new();
        while let Some(feature) = pending.pop() {
            if !visited.insert(feature) {
                continue;
            }
            assert!(
                feature != "room-harness" && !feature.ends_with("/room-harness"),
                "the default workspace build enables the room harness in {}",
                package["name"]
            );
            if let Some(next) = package["features"][feature].as_array() {
                pending.extend(next.iter().filter_map(serde_json::Value::as_str));
            }
        }
        for dependency in package["dependencies"].as_array().expect("dependencies") {
            if dependency["kind"] == "dev" || dependency["optional"] == true {
                continue;
            }
            assert!(
                !dependency["features"]
                    .as_array()
                    .expect("dependency features")
                    .iter()
                    .any(|feature| feature == "room-harness"),
                "{} enables the room harness on a shipped dependency",
                package["name"]
            );
        }
    }
    let flake = std::fs::read_to_string(root.join("flake.nix")).expect("the shipped package build");
    let package = flake
        .split_once("jojobot = craneLib.buildPackage (")
        .expect("the shipped derivation")
        .1
        .split_once("# **The suite,")
        .expect("the separate test derivation")
        .0;
    assert!(
        !package.contains("--all-features") && !package.contains("room-harness"),
        "the shipped Nix package enables the room harness"
    );
}
