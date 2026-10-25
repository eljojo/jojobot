//! Branch-only crate selection. Boundary and prepush callers keep their jobs.

use std::collections::BTreeSet;
use std::process::Command;
use std::sync::OnceLock;

pub(super) struct Selection {
    packages: BTreeSet<String>,
    tests: BTreeSet<String>,
    bins: bool,
    pub allow_docs: bool,
    reason: String,
    scope: Vec<String>,
}

pub(super) fn current() -> Option<&'static Selection> {
    static SELECTION: OnceLock<Option<Selection>> = OnceLock::new();
    SELECTION
        .get_or_init(|| {
            if std::env::var("BAR_SELECT_BY_CRATE").ok().as_deref() != Some("1") {
                return None;
            }
            let value = Command::new("python3")
                .arg("scripts/select-crates")
                .output()
                .ok()
                .filter(|out| out.status.success())
                .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok());
            let names = |key: &str| -> BTreeSet<String> {
                value
                    .as_ref()
                    .and_then(|v| v[key].as_array())
                    .into_iter()
                    .flatten()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            };
            let packages = names("packages");
            let scoped = value.as_ref().is_some_and(|v| v["full"] == false) && !packages.is_empty();
            let scope = if scoped {
                packages
                    .iter()
                    .flat_map(|p| ["-p".to_string(), p.clone()])
                    .collect()
            } else {
                vec!["--workspace".to_string()]
            };
            Some(Selection {
                tests: names("tests"),
                packages,
                scope,
                bins: value.as_ref().is_some_and(|v| v["bins"] == true),
                allow_docs: value.as_ref().is_some_and(|v| v["allow_docs"] == true),
                reason: value
                    .as_ref()
                    .and_then(|v| v["reason"].as_str())
                    .unwrap_or("selection unavailable; full run")
                    .to_string(),
            })
        })
        .as_ref()
}

impl Selection {
    pub fn announce(&self) {
        println!("crate selection: {}", self.reason);
    }
    pub fn scoped(&self) -> bool {
        self.scope.first().is_some_and(|s| s == "-p")
    }
    pub fn scope(&self) -> Vec<&str> {
        self.scope.iter().map(String::as_str).collect()
    }
    pub fn jobs(&self, jobs: Vec<Vec<String>>) -> Vec<Vec<String>> {
        if !self.scoped() {
            return jobs;
        }
        jobs.into_iter()
            .filter(|job| match job.as_slice() {
                [flag, name, ..] if flag == "-p" => self.packages.contains(name),
                [flag, name] if flag == "--test" => self.tests.contains(name),
                [flag] if flag == "--bins" => self.bins,
                _ => true,
            })
            .collect()
    }
}
