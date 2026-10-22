//! **`bar dev` runs the unit tests and the fast files of the packages a branch
//! touches and of those that depend on them.** The real `bar` binary runs in a
//! throwaway git repository against a stub `cargo`, so the assertions are over
//! what the bar spawned. The stub answers `cargo metadata` with a real capture
//! of that command on this workspace; no real cargo runs.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use jojobot_bar::{JobRecord, Run, RunHeader, TestRecord, Verdict};

const METADATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/metadata-no-deps.json"
);

/// A fake `cargo`: it records every call, answers `metadata` with the capture,
/// and passes every other call.
fn write_cargo(dir: &Path) -> PathBuf {
    let path = dir.join("cargo-stub");
    let script = format!(
        r#"#!/bin/sh
echo "$*" >> "$(dirname "$0")/argv.log"
case "$1" in
  fmt) [ -f "$(dirname "$0")/fmt-fails" ] && exit 1; exit 0 ;;
  build) [ -f "$(dirname "$0")/build-fails" ] && exit 1; exit 0 ;;
  metadata)
    [ -f "$(dirname "$0")/metadata-fails" ] && exit 1
    cat "{METADATA}"; exit 0 ;;
  clippy)
    [ -f "$(dirname "$0")/clippy-fails" ] && exit 1
    exit 0 ;;
  test)
    case " $* " in
      *" --no-run "*) exit 0 ;;
    esac
    if [ -f "$(dirname "$0")/tests-fail" ]; then
      echo 'test some::tests::a_red ... FAILED'
      echo 'test result: FAILED. 2 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
      exit 101
    fi
    echo 'test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s'
    exit 0 ;;
  *) exit 0 ;;
esac
"#
    );
    let mut f = fs::File::create(&path).expect("write fake cargo");
    f.write_all(script.as_bytes())
        .expect("write fake cargo body");
    use std::os::unix::fs::PermissionsExt;
    let mut perms = fs::metadata(&path).expect("stat").permissions();
    perms.set_mode(0o755);
    fs::set_permissions(&path, perms).expect("chmod");
    path
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args([
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "-c",
            "commit.gpgsign=false",
        ])
        .args(args)
        .current_dir(dir)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// **A repository with the capture's package directories** on the branch
/// `main`, one commit, so a change can be made on top of it.
fn repository() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "jojobot-bar-dev-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    for package in [
        "jojobot",
        "jojobot-adapters",
        "jojobot-bar",
        "jojobot-domain",
        "jojobot-exercise",
        "jojobot-mcp",
        "jojobot-ports",
    ] {
        let src = dir.join("crates").join(package).join("src");
        fs::create_dir_all(&src).expect("package dir");
        fs::write(src.join("lib.rs"), "// start\n").expect("write");
    }
    fs::write(dir.join("Cargo.toml"), "[workspace]\n").expect("write");
    fs::write(dir.join("Makefile"), "all:\n").expect("write");
    // **The bar writes its log under `target/` in the repository it runs in**, and
    // the real repository ignores that directory. Without the same line here, the
    // log reads as an untracked path in no package on a machine whose git config
    // ignores nothing, such as the build sandbox, and a clean tree is refused.
    fs::write(dir.join(".gitignore"), "/target\n").expect("write");
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["add", "-A"]);
    git(&dir, &["commit", "-q", "-m", "start"]);
    dir
}

struct Ran {
    stdout: String,
    exit: Option<i32>,
    calls: Vec<String>,
}

impl Ran {
    /// The packages a `cargo test -p <package>` call named, once each.
    fn packages_tested(&self) -> Vec<String> {
        let mut found: Vec<String> = self
            .calls
            .iter()
            .filter(|c| c.starts_with("test "))
            .filter_map(|c| c.strip_prefix("test -p "))
            .filter_map(|c| c.split_whitespace().next())
            .map(str::to_string)
            .collect();
        found.sort();
        found.dedup();
        found
    }

    /// The `cargo clippy` calls the bar spawned, whole.
    fn clippy_calls(&self) -> Vec<&str> {
        self.calls
            .iter()
            .filter(|c| c.starts_with("clippy"))
            .map(String::as_str)
            .collect()
    }

    /// The packages a `cargo clippy` call named with `-p`, once each.
    fn packages_linted(&self) -> Vec<String> {
        let mut found: Vec<String> = Vec::new();
        for call in self.clippy_calls() {
            let mut words = call.split_whitespace();
            while let Some(word) = words.next() {
                if word == "-p"
                    && let Some(name) = words.next()
                {
                    found.push(name.to_string());
                }
            }
        }
        found.sort();
        found.dedup();
        found
    }

    fn tested(&self, args: &str) -> bool {
        self.calls
            .iter()
            .any(|c| c.starts_with(&format!("test {args} ")))
    }
}

fn dev(repo: &Path) -> Ran {
    dev_with(repo, &[])
}

/// **A run whose stub `cargo clippy` fails when `clippy_clean` is false.**
fn dev_with_clippy(repo: &Path, clippy_clean: bool) -> Ran {
    dev_with(repo, if clippy_clean { &[] } else { &["clippy-fails"] })
}

/// **A run whose stub `cargo` fails the call each marker names**: `fmt-fails`,
/// `build-fails`, `metadata-fails`, `tests-fail` and `clippy-fails`. The stub
/// answers with a marker file beside it, so the case decides per run.
fn dev_with(repo: &Path, markers: &[&str]) -> Ran {
    dev_run(repo, markers, &[])
}

/// **A run that names the ref its branch is measured against**, as
/// `make dev BASE=<ref>` does.
fn dev_based_on(repo: &Path, base: &str) -> Ran {
    dev_run(repo, &[], &[("BAR_DEV_BASE", base)])
}

fn dev_run(repo: &Path, markers: &[&str], envs: &[(&str, &str)]) -> Ran {
    // The stub and its call log live beside the repository, in a directory of
    // this case's own: the cases run on threads and must not share a log.
    let stub_dir = PathBuf::from(format!("{}-stub", repo.display()));
    fs::create_dir_all(&stub_dir).expect("stub dir");
    for marker in markers {
        fs::write(stub_dir.join(marker), "").expect("mark a call failing");
    }
    let cargo = write_cargo(&stub_dir);
    let calls_file = stub_dir.join("argv.log");
    let out = Command::new(env!("CARGO_BIN_EXE_jojobot-bar"))
        .arg("dev")
        .current_dir(repo)
        .env("TMPDIR", &stub_dir)
        .env("CARGO", &cargo)
        .env("BAR_HISTORY_DIR", repo.join("history-none"))
        .env_remove("BAR_JOBS")
        // A `make dev BASE=...` that runs these cases must not name their base.
        .env_remove("BAR_DEV_BASE")
        .envs(envs.iter().copied())
        .output()
        .expect("run bar");
    let calls = fs::read_to_string(&calls_file)
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect();
    let _ = fs::remove_dir_all(&stub_dir);
    Ran {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        exit: out.status.code(),
        calls,
    }
}

/// **A change in the bar's own package selects the bar and nothing else**, and
/// the run is one of the stub's: format check, build, then the bar's tests.
#[test]
fn a_change_in_bar_runs_only_bar() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert_eq!(ran.packages_tested(), ["jojobot-bar"], "{:?}", ran.calls);
    assert!(ran.tested("-p jojobot-bar --lib"), "{:?}", ran.calls);
    assert!(
        ran.calls.iter().any(|c| c.starts_with("fmt ")),
        "{:?}",
        ran.calls
    );
    assert!(
        !ran.calls
            .iter()
            .any(|c| c.contains("--workspace") && c.starts_with("test")),
        "a job names its package: {:?}",
        ran.calls
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **The lint phase covers the packages the test phase selected, in one call.**
/// A change in the bar's own package selects the bar alone, so one `-p` names it;
/// a change in the domain selects the domain and what depends on it, and the
/// same packages that were tested are the packages linted.
#[test]
fn the_lint_phase_names_the_packages_the_tests_ran() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert_eq!(
        ran.clippy_calls(),
        ["clippy -p jojobot-bar --all-targets --all-features --locked -- -D warnings"],
        "{:?}",
        ran.calls
    );
    assert!(ran.stdout.contains("lint: ok"), "{}", ran.stdout);
    let _ = fs::remove_dir_all(&repo);

    let repo = repository();
    git(&repo, &["switch", "-q", "-c", "work"]);
    fs::write(
        repo.join("crates/jojobot-domain/src/lib.rs"),
        "// changed\n",
    )
    .expect("edit");
    git(&repo, &["commit", "-q", "-a", "-m", "change"]);
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert_eq!(ran.clippy_calls().len(), 1, "{:?}", ran.calls);
    assert_eq!(
        ran.packages_linted(),
        ran.packages_tested(),
        "{:?}",
        ran.calls
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **A branch whose only fault is a lint fails the dev run, and the verdict
/// names the lint phase.** The tests pass; the run is red all the same.
#[test]
fn a_lint_that_fails_makes_the_run_red_and_names_the_phase() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev_with_clippy(&repo, false);
    assert_eq!(ran.exit, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("lint: FAILED"), "{}", ran.stdout);
    assert!(ran.stdout.contains("test: ok"), "{}", ran.stdout);
    assert!(ran.stdout.contains("verdict: RED"), "{}", ran.stdout);
    let _ = fs::remove_dir_all(&repo);
}

/// **A phase that ran says so, and the left-out line no longer says the lint
/// was left out.** The doc-tests still are.
#[test]
fn the_left_out_line_no_longer_names_the_lint() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev(&repo);
    let line = ran
        .stdout
        .lines()
        .find(|l| l.starts_with("skipped: "))
        .unwrap_or_else(|| panic!("no skipped line: {}", ran.stdout));
    assert!(line.contains("doc-tests"), "{line}");
    assert!(!line.contains("lint"), "{line}");
    let _ = fs::remove_dir_all(&repo);
}

/// **A change on a branch, committed, runs that package's own tests and no
/// others, and the verdict names the packages above it as the branch run's.**
/// The domain is under the adapters and the server crate; the ports and the bar
/// are not above it.
#[test]
fn a_committed_change_in_domain_runs_domain_only_and_names_its_dependents() {
    let repo = repository();
    git(&repo, &["switch", "-q", "-c", "work"]);
    fs::write(
        repo.join("crates/jojobot-domain/src/lib.rs"),
        "// changed\n",
    )
    .expect("edit");
    git(&repo, &["commit", "-q", "-a", "-m", "change"]);
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert_eq!(ran.packages_tested(), ["jojobot-domain"], "{:?}", ran.calls);
    let line = ran
        .stdout
        .lines()
        .find(|line| line.starts_with("dependents: "))
        .unwrap_or_else(|| panic!("no dependents line: {}", ran.stdout));
    for named in ["jojobot-adapters", "jojobot-mcp", "jojobot"] {
        assert!(line.contains(named), "{named}: {line}");
    }
    for unnamed in ["jojobot-bar", "jojobot-ports"] {
        assert!(!line.contains(unnamed), "{unnamed}: {line}");
    }
    assert!(line.contains("wait-run work"), "{line}");
    let _ = fs::remove_dir_all(&repo);
}

/// **A branch built on another line than `main` is measured against that
/// line.** Without `BAR_DEV_BASE` the merge base is `main`, so the commits the
/// branch's own base holds count as touched and the run selects their packages
/// too; with it, only what the branch changed is selected.
#[test]
fn a_branch_is_measured_against_the_base_it_names() {
    let repo = repository();
    git(&repo, &["switch", "-q", "-c", "line"]);
    fs::write(
        repo.join("crates/jojobot-domain/src/lib.rs"),
        "// the line's own change\n",
    )
    .expect("edit");
    git(&repo, &["commit", "-q", "-a", "-m", "line"]);
    git(&repo, &["switch", "-q", "-c", "work"]);
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    git(&repo, &["commit", "-q", "-a", "-m", "work"]);

    let against_main = dev(&repo);
    assert_eq!(
        against_main.packages_tested(),
        ["jojobot-bar", "jojobot-domain"],
        "{:?}",
        against_main.calls
    );
    let against_line = dev_based_on(&repo, "line");
    assert_eq!(against_line.exit, Some(0), "{}", against_line.stdout);
    assert_eq!(
        against_line.packages_tested(),
        ["jojobot-bar"],
        "{:?}",
        against_line.calls
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **A dev run ends with its whole wall time, on the last line**, and a run
/// inside the limit does not say it is over.
#[test]
fn a_dev_run_ends_with_its_wall_time() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    let last = ran.stdout.lines().last().expect("output");
    assert!(last.starts_with("wall time: "), "{last}");
    assert!(!last.contains("over the five-minute loop"), "{last}");
    let _ = fs::remove_dir_all(&repo);
}

/// **A file the branch added and git does not track yet still selects its
/// package.**
#[test]
fn a_new_file_selects_its_package() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-ports/src/new.rs"), "// new\n").expect("add");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert!(
        ran.packages_tested().contains(&"jojobot-ports".to_string()),
        "{:?}",
        ran.calls
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **Red tests do not stop the lint**: it runs, says clean, and the verdict is
/// red for the tests alone. The control for the cases below, where the lint is
/// not run because nothing before it passed.
#[test]
fn red_tests_still_run_the_lint_and_it_says_clean() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev_with(&repo, &["tests-fail"]);
    assert_eq!(ran.exit, Some(1), "{}", ran.stdout);
    assert!(ran.stdout.contains("test: FAILED"), "{}", ran.stdout);
    assert!(ran.stdout.contains("lint: ok — clean"), "{}", ran.stdout);
    assert_eq!(ran.packages_linted(), ["jojobot-bar"], "{:?}", ran.calls);
    let _ = fs::remove_dir_all(&repo);
}

/// **A phase before the tests that fails ends the run, and the lint says it did
/// not run.** One case per phase that can end a dev run early: the format check,
/// the package read, the build and the empty selection. Each names its own
/// failed phase, spawns no clippy, and ends red.
#[test]
fn a_phase_that_ends_the_run_early_leaves_the_lint_not_run() {
    type Break = (&'static [&'static str], &'static str, bool);
    let breaks: [Break; 3] = [
        (&["fmt-fails"], "fmt-check: FAILED", false),
        (&["metadata-fails"], "scope: FAILED", false),
        (&["build-fails"], "build: FAILED", true),
    ];
    for (markers, said, built) in breaks {
        let repo = repository();
        fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
        let ran = dev_with(&repo, markers);
        assert_eq!(ran.exit, Some(1), "{markers:?}: {}", ran.stdout);
        assert!(ran.stdout.contains(said), "{markers:?}: {}", ran.stdout);
        assert!(
            ran.stdout.contains("lint: not run"),
            "{markers:?}: {}",
            ran.stdout
        );
        assert!(
            ran.clippy_calls().is_empty(),
            "{markers:?}: {:?}",
            ran.calls
        );
        assert!(
            ran.packages_tested().is_empty(),
            "{markers:?}: {:?}",
            ran.calls
        );
        assert_eq!(
            ran.calls.iter().any(|c| c.starts_with("build")),
            built,
            "{markers:?}: {:?}",
            ran.calls
        );
        let _ = fs::remove_dir_all(&repo);
    }
}

/// **A diff that touches no package runs nothing and is refused**, as a
/// selection that matched no test is.
#[test]
fn a_diff_that_touches_no_package_is_refused() {
    let repo = repository();
    fs::write(repo.join("Makefile"), "all:\n\t@true\n").expect("edit");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(2), "{}", ran.stdout);
    assert!(ran.packages_tested().is_empty(), "{:?}", ran.calls);
    assert!(ran.stdout.contains("lint: not run"), "{}", ran.stdout);
    assert!(ran.clippy_calls().is_empty(), "{:?}", ran.calls);
    assert!(
        ran.stdout.contains("Makefile"),
        "the refusal names the path that selected nothing: {}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **A branch that changed nothing has nothing to run, and says so.** The loop
/// is run from habit, on main as often as on a branch, and a refusal there reads
/// as a failure. Nothing is built or tested and the exit is zero; the line names
/// the commit it compared with.
///
/// Paired with the case above it: a diff that touches only paths in no package is
/// still refused, because that one is a question for the person.
#[test]
fn a_clean_tree_has_nothing_to_run_and_is_not_refused() {
    let repo = repository();
    let base = String::from_utf8(
        Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo)
            .output()
            .expect("run git")
            .stdout,
    )
    .expect("utf8");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert!(
        ran.stdout.contains(&format!(
            "scope: ok — nothing changed since {}; nothing to run",
            &base.trim()[..10]
        )),
        "{}",
        ran.stdout
    );
    assert!(ran.stdout.contains("verdict: GREEN"), "{}", ran.stdout);
    assert!(
        !ran.calls
            .iter()
            .any(|c| c.starts_with("build") || c.starts_with("test")),
        "nothing is built or tested on a clean tree: {:?}",
        ran.calls
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **Every phase a dev run ran says how long it took.** The figure is the
/// phase's own wall time, in the line beside its verdict.
#[test]
fn a_dev_run_says_how_long_each_phase_took() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    for phase in ["fmt-check", "build", "jobs"] {
        let line = ran
            .stdout
            .lines()
            .find(|line| line.starts_with(&format!("{phase}: ")))
            .unwrap_or_else(|| panic!("no {phase} line: {}", ran.stdout));
        assert!(
            line.contains("wall "),
            "{phase} carries its wall time: {line}"
        );
    }
    let _ = fs::remove_dir_all(&repo);
}

/// **With no `main` to take a merge base from, every package is selected**, and
/// the verdict names the packages and says it is not a boundary run.
#[test]
fn with_no_main_every_package_runs() {
    let repo = repository();
    git(&repo, &["branch", "-q", "-m", "elsewhere"]);
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert_eq!(ran.packages_tested().len(), 7, "{:?}", ran.calls);
    assert!(ran.stdout.contains("boundary"), "{}", ran.stdout);
    // The words the EM asked the line to carry, so that nobody reads dev as
    // covering the store.
    assert!(
        ran.stdout.contains("no real store guaranteed absent"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&repo);
}

/// **A file the history calls slow is skipped and named; a file it calls fast
/// runs.** The history is written the way the bar writes it.
#[test]
fn a_file_the_history_calls_slow_is_skipped_and_named() {
    let repo = repository();
    fs::write(repo.join("crates/jojobot-bar/src/lib.rs"), "// changed\n").expect("edit");
    let history = repo.join("history-none");
    let job = |ms| JobRecord {
        tests: vec![TestRecord {
            test: "t::passes".into(),
            verdict: Verdict::Ok,
            duration_ms: None,
        }],
        duration_ms: Some(ms),
    };
    for n in 0..5 {
        let run = Run::begin(
            &history,
            RunHeader {
                run_id: format!("2026100{n}000000-c-{n}"),
                level: "check".into(),
                checkout: "c".into(),
                head: None,
                dirty: None,
                started_at: "2026-10-01T00:00:00Z".into(),
                bar_jobs: 8,
                load_average: None,
                cpus: None,
            },
        )
        .expect("begin");
        run.record_job(
            &["--test".into(), "report_level".into()],
            true,
            &job(60_000),
        )
        .expect("slow");
        run.record_job(&["--test".into(), "orphan_sweep".into()], true, &job(1_000))
            .expect("fast");
    }
    let ran = dev(&repo);
    assert_eq!(ran.exit, Some(0), "{}", ran.stdout);
    assert!(
        !ran.tested("-p jojobot-bar --test report_level"),
        "{:?}",
        ran.calls
    );
    assert!(
        ran.tested("-p jojobot-bar --test orphan_sweep"),
        "{:?}",
        ran.calls
    );
    assert!(
        ran.stdout.contains("jojobot-bar/report_level"),
        "{}",
        ran.stdout
    );
    let _ = fs::remove_dir_all(&repo);
}
