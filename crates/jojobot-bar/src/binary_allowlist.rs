//! The test binaries allowed as build-metrics labels, read without compiling them.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::Summary;

pub const FILE: &str = "crates/jojobot-bar/build-metrics-binaries.txt";

pub fn from_metadata(metadata: &Value) -> Result<String, String> {
    let mut binaries = BTreeSet::new();
    for package in metadata["packages"]
        .as_array()
        .ok_or("metadata has no packages")?
    {
        let name = package["name"].as_str().ok_or("a package has no name")?;
        for target in package["targets"]
            .as_array()
            .ok_or("a package has no targets")?
        {
            if target["test"] != true {
                continue;
            }
            for kind in target["kind"]
                .as_array()
                .ok_or("a target has no kind")?
                .iter()
                .filter_map(Value::as_str)
            {
                match kind {
                    "lib" | "proc-macro" => {
                        binaries.insert(format!("lib-{name}"));
                    }
                    "bin" => {
                        binaries.insert("bins".into());
                    }
                    "test" => {
                        binaries.insert(format!(
                            "test-{}",
                            target["name"].as_str().ok_or("a test has no name")?
                        ));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(binaries.into_iter().collect::<Vec<_>>().join("\n") + "\n")
}

/// Only stdout is metadata; a warning on stderr must never be parsed as JSON.
pub fn phase(summary: &mut Summary) -> Option<Value> {
    let checked = (|| -> Result<_, String> {
        let out = std::process::Command::new(crate::cargo_bin())
            .args(["metadata", "--no-deps", "--format-version", "1", "--locked"])
            .env_remove("BAR_BUNDLE")
            .output()
            .map_err(|error| error.to_string())?;
        if !out.status.success() {
            return Err(format!(
                "cargo metadata failed ({}): {}",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        let metadata: Value =
            serde_json::from_slice(&out.stdout).map_err(|error| error.to_string())?;
        let generated = from_metadata(&metadata)?;
        let held = std::fs::read_to_string(FILE).map_err(|error| format!("{FILE}: {error}"))?;
        if held != generated {
            let wanted: BTreeSet<_> = generated.lines().collect();
            let found: BTreeSet<_> = held.lines().collect();
            return Err(format!(
                "{FILE} differs from cargo metadata; missing {:?}; stale {:?}",
                wanted.difference(&found).collect::<Vec<_>>(),
                found.difference(&wanted).collect::<Vec<_>>()
            ));
        }
        Ok((metadata, generated.lines().count()))
    })();
    match checked {
        Ok((metadata, count)) => {
            summary.phase_ok("metrics-allowlist", &format!("{count} binaries"));
            Some(metadata)
        }
        Err(error) => {
            summary.phase_failed("metrics-allowlist", &error);
            None
        }
    }
}
