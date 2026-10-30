//! **The prebuilt test bundle: the test executables one job compiled, and what
//! a later job needs to run them.**
//!
//! `cargo test --no-run` prints one `Executable` line for each test binary it
//! built. The bundle keeps those lines, with the package directory each binary
//! has to run in, so that a job which did not compile anything splits the same
//! jobs from the same text ([`crate::test_jobs`]) and runs the binaries
//! directly. The binaries print what `cargo test` would have printed, so every
//! reader of that output stays as it is.
//!
//! A test binary carries the absolute path of the checkout it was compiled in.
//! The manifest names that path as `root`; a job that runs the bundle from any
//! other directory is refused, because every file the binaries read would be
//! missing.

use serde_json::{Value, json};

/// **One test binary in the bundle.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exe {
    /// The lane that built it: `check`, `rooms` or `year`.
    pub lane: String,
    /// What cargo printed after `Executable`: `unittests src/lib.rs` or
    /// `tests/<name>.rs`.
    pub describes: String,
    /// The binary's path, from `root`.
    pub path: String,
    /// The directory of the package that owns it, from `root`. `cargo test`
    /// runs a binary there, and its tests read files from there.
    pub dir: String,
}

/// **Everything a job needs to run a bundle.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    /// The absolute path of the checkout the binaries were compiled in.
    pub root: String,
    /// The processor architecture they were compiled for. A binary does not run
    /// on another one, and a runner label can name hosts of both.
    pub arch: String,
    pub exes: Vec<Exe>,
}

/// **The `Executable` lines of a `cargo test --no-run` listing**, as
/// `(what it describes, binary path)` pairs.
pub fn executables(listing: &str) -> Vec<(String, String)> {
    listing
        .lines()
        .filter_map(|line| {
            let rest = line.trim().strip_prefix("Executable ")?;
            let (describes, path) = rest.rsplit_once(" (")?;
            Some((describes.to_string(), path.strip_suffix(')')?.to_string()))
        })
        .collect()
}

/// **Make a path relative to `root` when it lies under it.** Cargo prints
/// absolute paths when the target directory is outside the workspace and
/// relative ones otherwise; the manifest keeps one form.
pub fn relative_to(root: &str, path: &str) -> String {
    let prefix = format!("{}/", root.trim_end_matches('/'));
    path.strip_prefix(&prefix).unwrap_or(path).to_string()
}

/// **The package directory a binary runs in.** A binary is named for its
/// target with each hyphen written as an underscore, then a hash. `targets`
/// pairs each target's name, written that way, with its package's directory.
/// A binary no target names, or one that two packages' targets name, is an
/// error: a guess would run a suite in the wrong directory.
pub fn package_dir(path: &str, targets: &[(String, String)]) -> Result<String, String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let name = file.rsplit_once('-').map_or(file, |(name, _)| name);
    let mut dirs: Vec<&str> = targets
        .iter()
        .filter(|(target, _)| target == name)
        .map(|(_, dir)| dir.as_str())
        .collect();
    dirs.sort_unstable();
    dirs.dedup();
    match dirs.as_slice() {
        [one] => Ok((*one).to_string()),
        [] => Err(format!("no target is named for the binary {path}")),
        many => Err(format!(
            "the binary {path} fits targets in {} packages: {many:?}",
            many.len()
        )),
    }
}

/// **The targets of a workspace and its root**, from `cargo metadata
/// --no-deps` output: the workspace root, and each target's name (hyphens as
/// underscores) with the directory of its package, from that root.
pub fn targets_of(metadata: &str) -> Result<(String, Vec<(String, String)>), String> {
    let value: Value = serde_json::from_str(metadata).map_err(|e| e.to_string())?;
    let root = value["workspace_root"]
        .as_str()
        .ok_or("the metadata names no workspace root")?
        .to_string();
    let mut targets = Vec::new();
    for package in value["packages"].as_array().ok_or("no packages")? {
        let manifest = package["manifest_path"]
            .as_str()
            .ok_or("a package names no manifest")?;
        let dir = manifest.rsplit_once('/').map_or(".", |(dir, _)| dir);
        let dir = if dir == root {
            ".".to_string()
        } else {
            relative_to(&root, dir)
        };
        for target in package["targets"].as_array().ok_or("no targets")? {
            let name = target["name"].as_str().ok_or("a target has no name")?;
            targets.push((name.replace('-', "_"), dir.clone()));
        }
    }
    Ok((root, targets))
}

/// **The binaries one job runs.** The jobs are the ones [`crate::test_jobs`]
/// names: a library's unit tests, every binary's unit tests, one integration
/// test file. The doc-tests have no binary and the answer is empty; the
/// caller runs them through cargo.
pub fn exes_for_job<'a>(job: &[String], exes: &'a [Exe]) -> Vec<&'a Exe> {
    let words: Vec<&str> = job.iter().map(String::as_str).collect();
    let is_lib = |exe: &Exe| exe.describes.starts_with("unittests src/lib.rs");
    match words.as_slice() {
        ["-p", package, "--lib"] => exes
            .iter()
            .filter(|exe| is_lib(exe) && exe_package(exe).as_deref() == Some(package))
            .collect(),
        ["--lib"] => exes
            .iter()
            .filter(|exe| is_lib(exe) && exe_package(exe).is_none())
            .collect(),
        ["--bins"] => exes
            .iter()
            .filter(|exe| exe.describes.starts_with("unittests ") && !is_lib(exe))
            .collect(),
        ["--test", name] => {
            let wanted = format!("tests/{name}.rs");
            exes.iter().filter(|exe| exe.describes == wanted).collect()
        }
        _ => Vec::new(),
    }
}

/// The package a library's unit-test binary belongs to, named as
/// [`crate::test_jobs`] names it.
fn exe_package(exe: &Exe) -> Option<String> {
    let file = exe.path.rsplit('/').next()?;
    let (name, _) = file.rsplit_once('-')?;
    (!name.is_empty()).then(|| name.replace('_', "-"))
}

impl Manifest {
    /// **The listing the lane's binaries came from**, in the form `cargo test
    /// --no-run` prints it, for [`crate::test_jobs`].
    pub fn listing(&self, lane: &str) -> String {
        self.exes
            .iter()
            .filter(|exe| exe.lane == lane)
            .map(|exe| format!("  Executable {} ({})\n", exe.describes, exe.path))
            .collect()
    }

    pub fn to_json(&self) -> String {
        let exes: Vec<Value> = self
            .exes
            .iter()
            .map(|exe| {
                json!({
                    "lane": exe.lane,
                    "describes": exe.describes,
                    "path": exe.path,
                    "dir": exe.dir,
                })
            })
            .collect();
        serde_json::to_string_pretty(&json!({ "root": self.root, "arch": self.arch, "exes": exes }))
            .expect("a value built from strings serializes")
    }

    pub fn from_json(text: &str) -> Result<Manifest, String> {
        let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let field = |v: &Value, key: &str| -> Result<String, String> {
            v[key]
                .as_str()
                .map(str::to_string)
                .ok_or_else(|| format!("the manifest has no {key}"))
        };
        let exes = value["exes"]
            .as_array()
            .ok_or("the manifest has no exes")?
            .iter()
            .map(|exe| {
                Ok(Exe {
                    lane: field(exe, "lane")?,
                    describes: field(exe, "describes")?,
                    path: field(exe, "path")?,
                    dir: field(exe, "dir")?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        Ok(Manifest {
            root: field(&value, "root")?,
            arch: field(&value, "arch")?,
            exes,
        })
    }

    /// **Refuse to run these binaries on another architecture.** A binary built
    /// for one fails on the other with an error that names no cause.
    pub fn refuse_unless_arch(&self, here: &str) -> Result<(), String> {
        if self.arch == here {
            return Ok(());
        }
        Err(format!(
            "the bundle was compiled for {} and this host is {here}: its binaries cannot run here",
            self.arch
        ))
    }
}

/// **The cargo calls whose test binaries the bundle holds**, by lane. The rooms lane
/// exists only when a room is named.
pub fn lanes<'a>(rooms: &[&'a str]) -> Vec<(&'static str, Vec<&'a str>)> {
    let mut lanes: Vec<(&'static str, Vec<&'a str>)> = vec![
        (
            "check",
            vec![
                "test",
                "--workspace",
                "--features=jojobot/room-harness",
                "--no-run",
                "--locked",
            ],
        ),
        (
            "year",
            vec![
                "test",
                "-p",
                "jojobot-exercise",
                "--all-features",
                "--test",
                "year_room",
                "--no-run",
                "--locked",
            ],
        ),
    ];
    if !rooms.is_empty() {
        let mut call = vec!["test", "-p", "jojobot-exercise", "--features", "rooms"];
        call.extend(rooms.iter().flat_map(|name| ["--test", *name]));
        call.extend(["--no-run", "--locked"]);
        lanes.insert(1, ("rooms", call));
    }
    lanes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **The check lane compiles the production guard's test with the room-harness
    /// feature.** The bundle ships the binary built with it, and that test expects the
    /// guard to find the harness exactly when its own build has the feature.
    #[test]
    fn the_check_lane_builds_the_room_harness_feature() {
        let lanes = lanes(&[]);
        let (_, check) = lanes
            .iter()
            .find(|(lane, _)| *lane == "check")
            .expect("a check lane");
        assert!(
            check.contains(&"--features=jojobot/room-harness"),
            "{check:?}"
        );
    }

    /// A verbatim `cargo test --workspace --no-run` capture on this workspace.
    const NO_RUN_LISTING: &str = include_str!("../tests/fixtures/no-run-listing.txt");

    fn exe(describes: &str, path: &str) -> Exe {
        Exe {
            lane: "check".to_string(),
            describes: describes.to_string(),
            path: path.to_string(),
            dir: "crates/x".to_string(),
        }
    }

    /// **Every `Executable` line of a real listing is read, and nothing else.**
    #[test]
    fn a_real_listing_gives_one_binary_per_executable_line() {
        let found = executables(NO_RUN_LISTING);
        let lines = NO_RUN_LISTING
            .lines()
            .filter(|l| l.trim().starts_with("Executable "))
            .count();
        assert!(lines > 20, "the fixture lost its executables");
        assert_eq!(found.len(), lines);
        assert_eq!(found[0].0, "unittests src/lib.rs");
        assert!(found[0].1.ends_with("deps/jojobot-a1b1d37cb3ca46e5"));
    }

    /// **The jobs split from a manifest's listing are the jobs split from the
    /// listing it was made from.** The prebuilt run is the plain run, split the
    /// same way.
    #[test]
    fn a_manifest_listing_splits_into_the_same_jobs_as_the_listing_it_came_from() {
        let manifest = Manifest {
            root: "/r".to_string(),
            arch: "x86_64".to_string(),
            exes: executables(NO_RUN_LISTING)
                .into_iter()
                .map(|(describes, path)| exe(&describes, &path))
                .collect(),
        };
        assert_eq!(
            crate::test_jobs(&manifest.listing("check")),
            crate::test_jobs(NO_RUN_LISTING)
        );
        assert_eq!(manifest.listing("rooms"), "");
    }

    /// **Each job maps to the binaries the plain run would run for it.**
    #[test]
    fn a_job_selects_its_own_binaries() {
        let exes = vec![
            exe("unittests src/lib.rs", "/t/debug/deps/jojobot_domain-aa"),
            exe("unittests src/lib.rs", "/t/debug/deps/jojobot_mcp-bb"),
            exe("unittests src/main.rs", "/t/debug/deps/jojobot-cc"),
            exe("unittests src/main.rs", "/t/debug/deps/jojobot_bar-dd"),
            exe("tests/ci.rs", "/t/debug/deps/ci-ee"),
            exe("tests/ci_extra.rs", "/t/debug/deps/ci_extra-ff"),
        ];
        let job = |words: &[&str]| words.iter().map(|w| w.to_string()).collect::<Vec<_>>();
        let paths = |job: Vec<String>| -> Vec<String> {
            exes_for_job(&job, &exes)
                .iter()
                .map(|e| e.path.clone())
                .collect()
        };
        assert_eq!(
            paths(job(&["-p", "jojobot-mcp", "--lib"])),
            ["/t/debug/deps/jojobot_mcp-bb"]
        );
        assert_eq!(
            paths(job(&["--bins"])),
            ["/t/debug/deps/jojobot-cc", "/t/debug/deps/jojobot_bar-dd"]
        );
        assert_eq!(paths(job(&["--test", "ci"])), ["/t/debug/deps/ci-ee"]);
        assert!(paths(job(&["--doc"])).is_empty());
        assert!(paths(job(&["--test", "nothing"])).is_empty());
    }

    /// **A binary runs in the package that owns its target, and a name two
    /// packages share is refused.**
    #[test]
    fn a_binary_finds_one_package_directory_or_is_refused() {
        let targets = vec![
            ("ci".to_string(), "crates/jojobot".to_string()),
            ("shared".to_string(), "crates/a".to_string()),
            ("shared".to_string(), "crates/b".to_string()),
        ];
        assert_eq!(
            package_dir("target/debug/deps/ci-0123abcd", &targets).as_deref(),
            Ok("crates/jojobot")
        );
        let ambiguous = package_dir("target/debug/deps/shared-0123", &targets).unwrap_err();
        assert!(ambiguous.contains("2 packages"), "{ambiguous}");
        let none = package_dir("target/debug/deps/lost-0123", &targets).unwrap_err();
        assert!(none.contains("no target"), "{none}");
    }

    /// **The manifest reads back as it was written.**
    #[test]
    fn a_manifest_reads_back_as_written() {
        let manifest = Manifest {
            root: "/tmp/jojobot-ci/9/repo".to_string(),
            arch: "aarch64".to_string(),
            exes: vec![exe("tests/ci.rs", "target/debug/deps/ci-ee")],
        };
        assert_eq!(Manifest::from_json(&manifest.to_json()), Ok(manifest));
        assert!(Manifest::from_json("{}").is_err());
    }

    /// **A workspace's targets come out of `cargo metadata` with their package
    /// directory from the root, and the root package is `.`.**
    #[test]
    fn metadata_gives_each_target_its_package_directory() {
        let metadata = r#"{"workspace_root":"/w","packages":[
            {"manifest_path":"/w/crates/jojobot-bar/Cargo.toml",
             "targets":[{"name":"jojobot-bar"},{"name":"dev_run"}]},
            {"manifest_path":"/w/Cargo.toml","targets":[{"name":"top"}]}]}"#;
        let (root, targets) = targets_of(metadata).unwrap();
        assert_eq!(root, "/w");
        assert!(targets.contains(&("jojobot_bar".to_string(), "crates/jojobot-bar".to_string())));
        assert!(targets.contains(&("dev_run".to_string(), "crates/jojobot-bar".to_string())));
        assert!(targets.contains(&("top".to_string(), ".".to_string())));
    }

    /// **A bundle runs only on the architecture it was compiled for.**
    #[test]
    fn a_bundle_is_refused_on_another_architecture() {
        let manifest = Manifest {
            root: "/r".to_string(),
            arch: "x86_64".to_string(),
            exes: Vec::new(),
        };
        assert_eq!(manifest.refuse_unless_arch("x86_64"), Ok(()));
        let refusal = manifest.refuse_unless_arch("aarch64").unwrap_err();
        assert!(
            refusal.contains("x86_64") && refusal.contains("aarch64"),
            "{refusal}"
        );
    }

    #[test]
    fn a_path_under_the_root_is_made_relative() {
        assert_eq!(relative_to("/w", "/w/target/x"), "target/x");
        assert_eq!(relative_to("/w/", "/w/target/x"), "target/x");
        assert_eq!(relative_to("/w", "/other/x"), "/other/x");
    }
}
