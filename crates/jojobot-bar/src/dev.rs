//! **Which jobs `bar dev` runs, from what the branch touched.**
//!
//! `bar dev` is a level below the report: the unit tests and the fast test
//! files of the crates a branch touches and the crates that depend on them. It
//! runs no real store, no room and no doc-test, and it names what it left out.
//! It lints the same packages it tests. It is never a boundary run.
//!
//! Everything here is pure. The diff and the workspace are parameters, so the
//! cases need no repository and no cargo.

use std::collections::BTreeSet;

use crate::reading::{History, JobRun};

/// **One integration-test file of a package.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestFile {
    pub name: String,
    /// The file needs a cargo feature to be built, as a room suite does, so a
    /// plain `cargo test -p` does not run it.
    pub needs_feature: bool,
}

/// **One workspace package, as `cargo metadata` describes it.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    /// Its directory, relative to the workspace root, with no trailing slash.
    pub dir: String,
    /// The workspace packages it depends on, in any dependency kind.
    pub depends_on: Vec<String>,
    pub has_lib: bool,
    pub has_bins: bool,
    pub tests: Vec<TestFile>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Workspace {
    pub packages: Vec<Package>,
}

/// **Read `cargo metadata --no-deps --format-version 1`.**
pub fn parse_metadata(text: &str) -> Result<Workspace, String> {
    use serde_json::Value;
    let value: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
    let root = value
        .get("workspace_root")
        .and_then(Value::as_str)
        .ok_or("no workspace_root")?
        .trim_end_matches('/');
    let listed = value
        .get("packages")
        .and_then(Value::as_array)
        .ok_or("no packages")?;
    let members: BTreeSet<&str> = listed
        .iter()
        .filter_map(|p| p.get("name").and_then(Value::as_str))
        .collect();
    let mut packages = Vec::new();
    for package in listed {
        let name = package
            .get("name")
            .and_then(Value::as_str)
            .ok_or("a package has no name")?;
        let manifest = package
            .get("manifest_path")
            .and_then(Value::as_str)
            .ok_or_else(|| format!("{name} has no manifest_path"))?;
        let dir = manifest
            .strip_suffix("/Cargo.toml")
            .and_then(|d| d.strip_prefix(root))
            .map(|d| d.trim_start_matches('/'))
            .ok_or_else(|| format!("{name}: {manifest} is not under {root}"))?;
        // An edge in any kind, a package's own dev-dependency on itself apart.
        let mut depends_on: Vec<String> = package
            .get("dependencies")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|d| d.get("name").and_then(Value::as_str))
            .filter(|d| members.contains(d) && *d != name)
            .map(str::to_string)
            .collect();
        depends_on.sort();
        depends_on.dedup();
        let mut has_lib = false;
        let mut has_bins = false;
        let mut tests = Vec::new();
        for target in package
            .get("targets")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let kinds: Vec<&str> = target
                .get("kind")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect();
            let target_name = target.get("name").and_then(Value::as_str).unwrap_or("");
            if kinds
                .iter()
                .any(|k| matches!(*k, "lib" | "rlib" | "proc-macro"))
            {
                has_lib = true;
            } else if kinds.contains(&"bin") {
                has_bins = true;
            } else if kinds.contains(&"test") {
                let needs_feature = target
                    .get("required-features")
                    .and_then(Value::as_array)
                    .is_some_and(|f| !f.is_empty());
                tests.push(TestFile {
                    name: target_name.to_string(),
                    needs_feature,
                });
            }
        }
        packages.push(Package {
            name: name.to_string(),
            dir: dir.to_string(),
            depends_on,
            has_lib,
            has_bins,
            tests,
        });
    }
    Ok(Workspace { packages })
}

/// **What a diff touches.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touched {
    /// The packages whose directory holds a changed path.
    pub crates: BTreeSet<String>,
    /// A root manifest, lock file or toolchain file changed: every package.
    pub whole_workspace: bool,
    /// Changed paths in no package and not a root file.
    pub outside: Vec<String>,
}

/// **Which packages a list of changed paths touches.**
pub fn touched(changed: &[String], workspace: &Workspace) -> Touched {
    let mut hit = Touched::default();
    for path in changed {
        if matches!(
            path.as_str(),
            "Cargo.toml"
                | "Cargo.lock"
                | "rust-toolchain"
                | "rust-toolchain.toml"
                | "flake.nix"
                | "flake.lock"
        ) {
            hit.whole_workspace = true;
        } else if let Some(package) = workspace.packages.iter().find(|p| {
            path.strip_prefix(&p.dir)
                .is_some_and(|rest| rest.starts_with('/'))
        }) {
            hit.crates.insert(package.name.clone());
        } else {
            hit.outside.push(path.clone());
        }
    }
    hit
}

/// **The touched packages and every package that depends on one of them**,
/// directly or through others.
pub fn with_dependents(touched: &Touched, workspace: &Workspace) -> BTreeSet<String> {
    let mut selected: BTreeSet<String> = if touched.whole_workspace {
        workspace.packages.iter().map(|p| p.name.clone()).collect()
    } else {
        touched.crates.clone()
    };
    // Grow until no package depends on one already selected and is not.
    loop {
        let before = selected.len();
        for package in &workspace.packages {
            if package.depends_on.iter().any(|d| selected.contains(d)) {
                selected.insert(package.name.clone());
            }
        }
        if selected.len() == before {
            return selected;
        }
    }
}

/// **Why a job was left out.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The history's load-discounted median is 30 seconds or more.
    Slow,
    /// The file needs a cargo feature the plain run does not turn on.
    NeedsFeature,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub package: String,
    pub file: String,
    pub why: Why,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DevPlan {
    /// Each job is the arguments after `cargo test`, naming its package.
    pub run: Vec<Vec<String>>,
    pub skipped: Vec<Skipped>,
    /// Test files that ran with no recorded history.
    pub unmeasured: usize,
}

/// **The jobs for the selected packages.**
///
/// A file counts as slow only on the history's word: a file with no recorded
/// runs runs, and the plan counts it, because lack of evidence keeps a job in.
pub fn plan(
    selected: &BTreeSet<String>,
    workspace: &Workspace,
    history: &History,
    slow: &dyn Fn(&[JobRun]) -> bool,
) -> DevPlan {
    let mut plan = DevPlan::default();
    let job = |parts: &[&str]| parts.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    for package in workspace
        .packages
        .iter()
        .filter(|p| selected.contains(&p.name))
    {
        if package.has_lib {
            plan.run.push(job(&["-p", &package.name, "--lib"]));
        }
        if package.has_bins {
            plan.run.push(job(&["-p", &package.name, "--bins"]));
        }
        for file in &package.tests {
            let skipped = |why| Skipped {
                package: package.name.clone(),
                file: file.name.clone(),
                why,
            };
            if file.needs_feature {
                plan.skipped.push(skipped(Why::NeedsFeature));
                continue;
            }
            let runs = history
                .jobs
                .get(&format!("test-{}", file.name))
                .map_or(&[][..], Vec::as_slice);
            if runs.is_empty() {
                plan.unmeasured += 1;
            } else if slow(runs) {
                plan.skipped.push(skipped(Why::Slow));
                continue;
            }
            plan.run
                .push(job(&["-p", &package.name, "--test", &file.name]));
        }
    }
    plan
}

/// **How many changed paths are in no package, and the first few by name**, so
/// that a change to a script a test depends on is visible in the verdict.
pub fn outside_line(outside: &[String]) -> String {
    const NAMED: usize = 5;
    if outside.is_empty() {
        return "0 paths in no package".to_string();
    }
    let first: Vec<&str> = outside.iter().take(NAMED).map(String::as_str).collect();
    format!(
        "{} paths in no package, first {}: {}",
        outside.len(),
        first.len(),
        first.join(", ")
    )
}

/// **The `cargo clippy` arguments for the packages the dev level selected**, the
/// same set the test phase runs, so one selection decides both. `None` for an
/// empty selection: a call with no `-p` lints the package it is run in, which is
/// not what was selected.
pub fn clippy_args(selected: &BTreeSet<String>) -> Option<Vec<String>> {
    if selected.is_empty() {
        return None;
    }
    let mut args = vec!["clippy".to_string()];
    for package in selected {
        args.extend(["-p".to_string(), package.clone()]);
    }
    args.extend(
        [
            "--all-targets",
            "--all-features",
            "--locked",
            "--",
            "-D",
            "warnings",
        ]
        .map(str::to_string),
    );
    Some(args)
}

/// **What the verdict says was left out**, by name: the files skipped and why,
/// and the packages not selected.
pub fn skipped_line(plan: &DevPlan, selected: &BTreeSet<String>, workspace: &Workspace) -> String {
    let files = |why: Why| -> String {
        let named: Vec<String> = plan
            .skipped
            .iter()
            .filter(|s| s.why == why)
            .map(|s| format!("{}/{}", s.package, s.file))
            .collect();
        if named.is_empty() {
            "none".to_string()
        } else {
            named.join(", ")
        }
    };
    let packages: Vec<&str> = workspace
        .packages
        .iter()
        .map(|p| p.name.as_str())
        .filter(|name| !selected.contains(*name))
        .collect();
    format!(
        "not run at all: doc-tests; slow files: {}; files that need a feature: {}; \
         packages not selected: {}",
        files(Why::Slow),
        files(Why::NeedsFeature),
        if packages.is_empty() {
            "none".to_string()
        } else {
            packages.join(", ")
        }
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const METADATA: &str = include_str!("../tests/fixtures/metadata-no-deps.json");

    fn package(name: &str, depends_on: &[&str], tests: &[(&str, bool)]) -> Package {
        Package {
            name: name.to_string(),
            dir: format!("crates/{name}"),
            depends_on: depends_on.iter().map(|d| d.to_string()).collect(),
            has_lib: true,
            has_bins: false,
            tests: tests
                .iter()
                .map(|(file, needs_feature)| TestFile {
                    name: file.to_string(),
                    needs_feature: *needs_feature,
                })
                .collect(),
        }
    }

    /// `core` is depended on by `store`, `store` by `server`, and `bar` stands
    /// alone: a chain, so that a dependent through another is told from a
    /// direct one.
    fn chain() -> Workspace {
        Workspace {
            packages: vec![
                package("core", &[], &[]),
                package("store", &["core"], &[("store_contract", false)]),
                package(
                    "server",
                    &["store"],
                    &[("fast_one", false), ("slow_one", false), ("room", true)],
                ),
                package("bar", &[], &[]),
            ],
        }
    }

    fn paths(list: &[&str]) -> Vec<String> {
        list.iter().map(|p| p.to_string()).collect()
    }

    fn names(set: &BTreeSet<String>) -> Vec<&str> {
        set.iter().map(String::as_str).collect()
    }

    fn selected(changed: &[&str]) -> BTreeSet<String> {
        with_dependents(&touched(&paths(changed), &chain()), &chain())
    }

    /// **A change in the lowest crate selects it and every crate above it**,
    /// the one that depends on it directly and the one that depends on that.
    #[test]
    fn a_change_in_core_selects_core_and_every_dependent() {
        assert_eq!(
            names(&selected(&["crates/core/src/lib.rs"])),
            ["core", "server", "store"]
        );
    }

    /// **A change in a crate nothing depends on selects that crate only.**
    #[test]
    fn a_change_in_bar_selects_bar_only() {
        assert_eq!(names(&selected(&["crates/bar/src/main.rs"])), ["bar"]);
    }

    /// **A change in the middle selects the middle and what is above it, and
    /// not what is below.**
    #[test]
    fn a_change_in_the_middle_does_not_select_what_it_depends_on() {
        assert_eq!(
            names(&selected(&["crates/store/tests/store_contract.rs"])),
            ["server", "store"]
        );
    }

    /// **A path is in a crate only when the crate's directory holds it.** A
    /// directory whose name starts with another crate's name is not that crate.
    #[test]
    fn a_path_belongs_to_the_directory_that_holds_it_and_to_no_longer_name() {
        let mut ws = chain();
        ws.packages.push(package("core-extra", &[], &[]));
        let hit = touched(&paths(&["crates/core-extra/src/lib.rs"]), &ws);
        assert_eq!(names(&hit.crates), ["core-extra"]);
    }

    /// **A root manifest, the lock file or a toolchain file selects every
    /// crate**, because each of them changes how every crate builds.
    #[test]
    fn a_root_manifest_lock_or_toolchain_selects_every_crate() {
        for file in [
            "Cargo.toml",
            "Cargo.lock",
            "rust-toolchain.toml",
            "flake.nix",
            "flake.lock",
        ] {
            let hit = touched(&paths(&[file]), &chain());
            assert!(hit.whole_workspace, "{file}");
            assert_eq!(
                names(&with_dependents(&hit, &chain())),
                ["bar", "core", "server", "store"],
                "{file}"
            );
        }
    }

    /// **A path in no crate selects nothing and is counted.** The count is what
    /// the verdict says, so a reader sees paths that selected no test.
    #[test]
    fn a_path_in_no_crate_selects_nothing_and_is_counted() {
        let hit = touched(
            &paths(&["Makefile", "scripts/sabotage", "crates/bar/src/lib.rs"]),
            &chain(),
        );
        assert_eq!(names(&hit.crates), ["bar"]);
        assert_eq!(hit.outside, ["Makefile", "scripts/sabotage"]);
        assert!(!hit.whole_workspace);
        let none = touched(&paths(&["Makefile"]), &chain());
        assert!(none.crates.is_empty() && !none.whole_workspace);
        assert!(with_dependents(&none, &chain()).is_empty());
    }

    fn history_with(name: &str, runs: usize) -> History {
        let mut history = History::default();
        let run = JobRun {
            run_id: "r".into(),
            head: None,
            load_average: None,
            cpus: None,
            dirty: None,
            ok: true,
            duration_ms: Some(1),
            failed: Vec::new(),
        };
        history.jobs.insert(name.to_string(), vec![run; runs]);
        history
    }

    fn none_are_slow(_runs: &[JobRun]) -> bool {
        false
    }

    fn set(list: &[&str]) -> BTreeSet<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    /// **Each selected crate runs its library tests and its binaries' tests
    /// when it has them, and a fast test file.** The jobs name their package,
    /// so none depends on the workspace scope.
    #[test]
    fn a_selected_crate_runs_its_unit_tests_and_its_fast_files() {
        let plan = plan(
            &set(&["server"]),
            &chain(),
            &History::default(),
            &none_are_slow,
        );
        assert_eq!(
            plan.run,
            [
                vec!["-p", "server", "--lib"],
                vec!["-p", "server", "--test", "fast_one"],
                vec!["-p", "server", "--test", "slow_one"],
            ]
        );
    }

    /// **A crate with binaries gets a job for their unit tests.**
    #[test]
    fn a_crate_with_binaries_runs_their_unit_tests() {
        let mut ws = chain();
        ws.packages[3].has_bins = true;
        ws.packages[3].has_lib = false;
        let plan = plan(&set(&["bar"]), &ws, &History::default(), &|_| false);
        assert_eq!(plan.run, [vec!["-p", "bar", "--bins"]]);
    }

    /// **A slow file is skipped and named; a file that needs a feature is
    /// skipped and named.** The slow one is the history's word, asked through
    /// the same test the report level uses, on the file's own job name.
    #[test]
    fn a_slow_file_and_a_file_that_needs_a_feature_are_skipped_by_name() {
        let history = history_with("test-slow_one", 5);
        let only_slow_one = |runs: &[JobRun]| runs.len() == 5;
        let plan = plan(&set(&["server"]), &chain(), &history, &only_slow_one);
        assert_eq!(
            plan.run,
            [
                vec!["-p", "server", "--lib"],
                vec!["-p", "server", "--test", "fast_one"],
            ]
        );
        assert_eq!(
            plan.skipped,
            [
                Skipped {
                    package: "server".into(),
                    file: "slow_one".into(),
                    why: Why::Slow
                },
                Skipped {
                    package: "server".into(),
                    file: "room".into(),
                    why: Why::NeedsFeature
                },
            ]
        );
    }

    /// **A file with no recorded history counts as fast, and the plan counts
    /// how many it ran that way.** Lack of evidence keeps a file in, as it does
    /// in the report level; a file nobody measured is not a hole nobody sees.
    #[test]
    fn a_file_with_no_history_runs_and_is_counted() {
        let plan = plan(&set(&["server"]), &chain(), &History::default(), &|_| true);
        assert_eq!(plan.run.len(), 3, "{:?}", plan.run);
        assert_eq!(plan.unmeasured, 2);
        let measured = history_with("test-fast_one", 2);
        let plan = super::plan(&set(&["server"]), &chain(), &measured, &|_| false);
        assert_eq!(plan.unmeasured, 1, "fast_one has a history now");
    }

    /// **The real `cargo metadata` shape reads as the workspace it describes.**
    /// The fixture is a capture of the command on this workspace with the paths
    /// of the machine replaced.
    #[test]
    fn the_real_metadata_reads_as_the_workspace() {
        let ws = parse_metadata(METADATA).expect("the capture reads");
        let mut found: Vec<&str> = ws.packages.iter().map(|p| p.name.as_str()).collect();
        found.sort_unstable();
        assert_eq!(
            found,
            [
                "jojobot",
                "jojobot-adapters",
                "jojobot-bar",
                "jojobot-domain",
                "jojobot-exercise",
                "jojobot-mcp",
                "jojobot-ports"
            ]
        );
        let domain = ws
            .packages
            .iter()
            .find(|p| p.name == "jojobot-domain")
            .expect("domain");
        assert_eq!(domain.dir, "crates/jojobot-domain");
        assert!(domain.has_lib && !domain.has_bins);
        let mcp = ws
            .packages
            .iter()
            .find(|p| p.name == "jojobot-mcp")
            .expect("mcp");
        assert!(
            mcp.depends_on.contains(&"jojobot-domain".to_string()),
            "{:?}",
            mcp.depends_on
        );
        assert!(!mcp.depends_on.contains(&"serde_json".to_string()));
        let adapters = ws
            .packages
            .iter()
            .find(|p| p.name == "jojobot-adapters")
            .expect("adapters");
        assert!(
            !adapters
                .depends_on
                .contains(&"jojobot-adapters".to_string()),
            "a package's own dev-dependency on itself is no edge"
        );
        let jojobot = ws
            .packages
            .iter()
            .find(|p| p.name == "jojobot")
            .expect("root");
        assert!(
            jojobot.depends_on.contains(&"jojobot-exercise".to_string()),
            "a dev-dependency is an edge: {:?}",
            jojobot.depends_on
        );
        let exercise = ws
            .packages
            .iter()
            .find(|p| p.name == "jojobot-exercise")
            .expect("exercise");
        assert!(exercise.has_bins);
        let rooms = exercise.tests.iter().filter(|t| t.needs_feature).count();
        assert_eq!(rooms, 32, "the room files need their feature");
        assert!(
            exercise.tests.iter().any(|t| !t.needs_feature),
            "and some files do not"
        );
    }

    /// **Text that is not metadata is refused with a reason.**
    #[test]
    fn text_that_is_not_metadata_is_refused() {
        assert!(parse_metadata("not json").is_err());
        assert!(parse_metadata("{}").is_err(), "no packages key");
    }

    /// **The verdict names every file it skipped, with the package, and every
    /// package it did not select.** A name left out of this line is a test
    /// nobody knows was not run.
    #[test]
    fn the_skipped_line_names_the_files_and_the_packages_left_out() {
        let history = history_with("test-slow_one", 5);
        let only_slow_one = |runs: &[JobRun]| runs.len() == 5;
        let selected = set(&["server"]);
        let plan = plan(&selected, &chain(), &history, &only_slow_one);
        let line = skipped_line(&plan, &selected, &chain());
        for name in ["server/slow_one", "server/room", "bar", "core", "store"] {
            assert!(line.contains(name), "{name} is missing: {line}");
        }
        assert!(
            !line.contains("fast_one"),
            "a file that ran is not skipped: {line}"
        );
        assert!(!line.contains('\n'), "{line}");
    }

    /// **The paths in no package are counted and the first five are named**, so
    /// the verdict shows a change to a script and does not list a hundred.
    #[test]
    fn the_outside_line_counts_every_path_and_names_the_first_five() {
        let many: Vec<String> = (1..=7).map(|n| format!("docs/page{n}.md")).collect();
        let line = outside_line(&many);
        assert!(line.contains('7'), "{line}");
        for n in 1..=5 {
            assert!(line.contains(&format!("page{n}.md")), "{line}");
        }
        assert!(
            !line.contains("page6.md") && !line.contains("page7.md"),
            "{line}"
        );
        let none = outside_line(&[]);
        assert!(none.contains('0') && !none.contains("docs"), "{none}");
    }

    /// **The lint call names every selected package, in name order, once.**
    #[test]
    fn the_clippy_arguments_name_each_selected_package() {
        let one: BTreeSet<String> = ["alpha".to_string()].into();
        assert_eq!(
            clippy_args(&one).expect("one package").join(" "),
            "clippy -p alpha --all-targets --all-features --locked -- -D warnings"
        );
        let several: BTreeSet<String> = ["beta".to_string(), "alpha".to_string()].into();
        assert_eq!(
            clippy_args(&several).expect("two packages").join(" "),
            "clippy -p alpha -p beta --all-targets --all-features --locked -- -D warnings"
        );
    }

    /// **An empty selection has no lint call**, not a call that lints whatever
    /// package the bar was run in.
    #[test]
    fn an_empty_selection_has_no_clippy_arguments() {
        assert_eq!(clippy_args(&BTreeSet::new()), None);
    }
}
