//! **Read what `cargo test` printed, and say the short version.**
//!
//! `cargo test --workspace` prints one `test result:` line per suite. That
//! line is the one signal this reads: its presence is what tells a compile
//! failure from a run that compiled and reports zero failures, and its
//! counts are what a verdict sums rather than a caller re-deriving them from
//! thousands of lines. A suite that fails to compile prints no such line at
//! all — `cargo test` aborts before any suite runs, `--no-fail-fast` never
//! gets the chance to matter — so a summary with no `test result:` line
//! anywhere in it is a summary of a workspace that did not compile, not one
//! that compiled and passed everything.

pub mod bundle;
mod calls;
pub mod changes;
pub mod dev;
mod expectation;
mod history;
mod known_reds;
mod levels;
mod reading;
mod recording;
pub mod scratch;
pub mod size;
pub mod sweep;
pub use calls::{CALL_USAGE, Call, parse_call, write_call};
pub use expectation::{BACKGROUND_TIMEOUT_MS, start_lines};
pub use history::{JobRecord, TestRecord, Verdict, parse_job};
pub use known_reds::{KnownRed, KnownReds, load as load_known_reds};
pub use levels::{
    ALWAYS_RUN, Ancestry, DEFECT_WINDOW, Ignored, LEAVE_AT_MS, LeftOut, MIN_RUNS, Plan, Standing,
    contained_from_exit, discounted_ms, is_a_revision, is_slow, median_cost_ms, report_line,
    report_plan, standing,
};
pub use reading::{History, JobRun, RecordedCall, read_history};
pub use recording::{HISTORY_DIR_VARIABLE, Run, RunHeader, history_root, iso_utc, run_id};

/// **What a `cargo test` run said about itself**, summed across every suite
/// it printed a `test result:` line for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TestVerdict {
    /// **At least one suite reported.** `false` means the run never got that
    /// far — a compile error, not a test failure — and `passed`/`failed`
    /// answer nothing when this is `false`: there is nothing to sum.
    pub compiled: bool,
    /// How many `test result:` lines this run printed — one per test binary.
    pub suites: usize,
    pub passed: usize,
    pub failed: usize,
    /// **Every failing test's own name**, in the order `cargo test` printed
    /// them — read off the `test <name> ... FAILED` lines directly, never
    /// re-derived from the suite counts.
    pub failed_names: Vec<String>,
    /// **What each failing test printed about its failure**, by the name
    /// `cargo test` gave it: the text under its `---- name stdout ----` header,
    /// up to the next header or the end of the failures section. A failing test
    /// that printed no such section has no entry.
    pub failures: std::collections::BTreeMap<String, String>,
}

impl TestVerdict {
    /// **Compiled, and nothing failed — going only by what the text said.**
    /// A caller deciding whether the whole phase is fine also needs the
    /// process's own exit status: see [`test_healthy`].
    pub fn ok(&self) -> bool {
        self.compiled && self.failed == 0
    }
}

/// **Whether the test phase, as a whole, is fine.** Two witnesses have to
/// agree: what the text said about itself (`verdict.ok()`) and the process's
/// own exit status.
///
/// They can disagree. A suite that crashes or is killed (`SIGABRT`,
/// `SIGSEGV`, an OOM kill, a killed timeout) exits non-zero and prints no
/// `test result:` line for itself — nothing to sum, nothing to count as
/// failed. Under `--no-fail-fast`, every OTHER suite still runs and still
/// prints its own clean line, so `verdict.ok()` alone reads the crashed
/// suite as if it had never existed rather than as a failure. The exit
/// status is the only witness that saw it: `cargo test` exits non-zero
/// exactly when a suite it spawned did not exit zero, whether or not that
/// suite got the chance to print anything.
pub fn test_healthy(exit_ok: bool, verdict: &TestVerdict) -> bool {
    exit_ok && verdict.ok()
}

/// **Read a `cargo test` run's combined stdout+stderr and sum what it said.**
///
/// Never touches the process's exit code — the text is the only witness this
/// takes, exactly as `scripts/sabotage`'s own classifier does, and for the
/// same reason: an exit code a caller obtained by piping the run is not this
/// function's to trust.
pub fn summarize_test_output(output: &str) -> TestVerdict {
    let mut verdict = TestVerdict::default();
    // The test whose failure section the lines are inside, if any.
    let mut inside: Option<String> = None;
    for line in output.lines() {
        if let Some(name) = line
            .strip_prefix("---- ")
            .and_then(|rest| rest.strip_suffix(" stdout ----"))
        {
            inside = Some(name.to_string());
            verdict.failures.entry(name.to_string()).or_default();
            continue;
        }
        if line == "failures:" || line.starts_with("test result: ") || line.starts_with("---- ") {
            inside = None;
        } else if let Some(name) = &inside {
            let body = verdict.failures.entry(name.clone()).or_default();
            body.push_str(line);
            body.push('\n');
        }
        if let Some(rest) = line.strip_prefix("test result: ") {
            verdict.suites += 1;
            if let Some((passed, failed)) = suite_counts(rest) {
                verdict.passed += passed;
                verdict.failed += failed;
            }
        } else if let Some(name) = line
            .strip_prefix("test ")
            .and_then(|rest| rest.strip_suffix(" ... FAILED"))
        {
            verdict.failed_names.push(name.to_string());
        }
    }
    verdict.compiled = verdict.suites > 0;
    verdict
}

/// **How many test binaries run at once when nothing says otherwise.** Eight
/// is most of the gain on the measured workspace; more buys little and several
/// bars run on one machine at the same time.
pub const TEST_JOBS_DEFAULT: usize = 8;

/// The environment variable that sets how many test binaries run at once.
pub const TEST_JOBS_VARIABLE: &str = "BAR_JOBS";

/// **The commit a push moved the branch from**, set by the forge's workflow on a
/// push. The report level runs the jobs of the packages that range touched
/// first. Unset, or a commit this checkout does not hold, it falls back to the
/// merge base with `main`.
pub const PUSH_BASE_VARIABLE: &str = "BAR_PUSH_BASE";

/// **How many test binaries to run at once**, from the value of
/// [`TEST_JOBS_VARIABLE`] if it is set. A value that is not a whole number of
/// at least one is an error that names the variable and the value: a limit
/// that fell back to the default would make a timing read later say it ran at
/// a number it did not.
pub fn jobs_limit(value: Option<&str>) -> Result<usize, String> {
    let Some(value) = value else {
        return Ok(TEST_JOBS_DEFAULT);
    };
    match value.trim().parse::<usize>() {
        Ok(n) if n >= 1 => Ok(n),
        _ => Err(format!(
            "{TEST_JOBS_VARIABLE} is {value:?}: it must be a whole number of at least 1"
        )),
    }
}

/// **The `cargo test` runs that, together, are `cargo test --workspace`.**
///
/// Read off `cargo test --workspace --no-run`, which prints one `Executable`
/// line per test binary it built. Each integration-test file is a job of its
/// own, named so a package is not needed: `--workspace --test <name>` selects
/// that file wherever it lives. The unit tests of each library are a job of
/// their own, named for the package (`-p <package> --lib`); the unit tests of
/// every binary and the doc-tests are one job each. Nothing is skipped: a
/// binary the plain run would execute is in exactly one of these.
pub fn test_jobs(listing: &str) -> Vec<Vec<String>> {
    let mut libs: Vec<String> = Vec::new();
    // A library whose package the listing does not name still runs, with the
    // libraries of the others, so that nothing the plain run executes is lost.
    let mut unnamed_lib = false;
    let mut bins = false;
    let mut files: Vec<String> = Vec::new();
    for line in listing.lines() {
        let Some(executable) = line.trim().strip_prefix("Executable ") else {
            continue;
        };
        if let Some(source) = executable.strip_prefix("unittests ") {
            if source.starts_with("src/lib.rs") {
                match package_of(source) {
                    Some(package) if !libs.contains(&package) => libs.push(package),
                    Some(_) => {}
                    None => unnamed_lib = true,
                }
            } else {
                bins = true;
            }
        } else if let Some(file) = executable.strip_prefix("tests/")
            && let Some((name, _)) = file.split_once(".rs")
            && !files.iter().any(|f| f == name)
        {
            files.push(name.to_string());
        }
    }
    let mut jobs: Vec<Vec<String>> = Vec::new();
    jobs.extend(
        libs.into_iter()
            .map(|package| vec!["-p".to_string(), package, "--lib".to_string()]),
    );
    if unnamed_lib {
        jobs.push(vec!["--lib".to_string()]);
    }
    if bins {
        jobs.push(vec!["--bins".to_string()]);
    }
    jobs.extend(files.into_iter().map(|f| vec!["--test".to_string(), f]));
    // The doc-tests print no `Executable` line, so this job does not wait on
    // the listing to name them.
    jobs.push(vec!["--doc".to_string()]);
    jobs
}

/// **The package a library unit-test binary belongs to**, from the path in its
/// listing line: `(/tmp/target/debug/deps/jojobot_domain-1f27…)` is
/// `jojobot-domain`. The binary is named for the package with each hyphen
/// written as an underscore, then a hash.
fn package_of(source: &str) -> Option<String> {
    let path = source.rsplit_once('(')?.1.strip_suffix(')')?;
    let file = path.rsplit('/').next()?;
    let (name, _hash) = file.rsplit_once('-')?;
    (!name.is_empty()).then(|| name.replace('_', "-"))
}

/// **The arguments `cargo test` gets for one job.** A job that names its own
/// package (`-p <package> ...`) gets no scope: `--workspace` beside `-p` would
/// select every package again.
pub fn job_args(scope: &[&str], job: &[String]) -> Vec<String> {
    let own_package = job.first().is_some_and(|a| a == "-p");
    let scope: &[&str] = if own_package { &[] } else { scope };
    scope
        .iter()
        .map(|a| a.to_string())
        .chain(job.iter().cloned())
        .collect()
}

/// **`ok. 58 passed; 0 failed; ...` → `(58, 0)`.** Reads the number sitting
/// immediately before the word it names, so it does not care whether the
/// line opens with `ok.` or `FAILED.` — the two words this looks for are the
/// same either way.
fn suite_counts(rest: &str) -> Option<(usize, usize)> {
    let words: Vec<&str> = rest.split_whitespace().collect();
    Some((
        number_before(&words, "passed")?,
        number_before(&words, "failed")?,
    ))
}

fn number_before(words: &[&str], label: &str) -> Option<usize> {
    let idx = words
        .iter()
        .position(|w| w.trim_end_matches([';', '.']) == label)?;
    idx.checked_sub(1).and_then(|i| words[i].parse().ok())
}

/// The source file whose cast the bar reports on.
pub const ROSTER_FILE: &str = "crates/jojobot-domain/tests/fixture_roster.rs";

/// **The handles on the `ROSTER` list of a roster source, in order.** It reads
/// the string literals between `const ROSTER` and the line that closes the list,
/// and nothing else: a comment is skipped, a trailing comment is ignored, and
/// the `ORDINARY` list that follows is a separate constant that is never read.
/// That separation is the whole mechanism — an entry on the other list is not
/// here to be reported.
pub fn roster_entries(source: &str) -> Vec<String> {
    let mut entries = Vec::new();
    let mut inside = false;
    for line in source.lines() {
        let line = line.trim();
        if !inside {
            inside = line.starts_with("const ROSTER");
            continue;
        }
        if line.starts_with("];") {
            break;
        }
        if let Some(rest) = line.strip_prefix('"')
            && let Some(end) = rest.find('"')
        {
            entries.push(rest[..end].to_string());
        }
    }
    entries
}

/// **The entries of `now` that `then` does not hold**, in the order `now` has
/// them and once each. An entry removed since the base is not here: only a name
/// arriving is what a reviewer has to read.
pub fn new_roster_entries(now: &[String], then: &[String]) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    for entry in now {
        if !then.contains(entry) && !found.contains(entry) {
            found.push(entry.clone());
        }
    }
    found
}

/// **The cast names that arrived since the base, rendered.** Report only:
/// nothing here changes `Summary::green`. A name on the other list passes
/// silently, so what this prints is exactly what a person has to read.
pub fn render_roster_report(new: &[String], base: &str) -> String {
    if base.is_empty() {
        return "roster names new since origin/main: unknown — no base ref (origin/main not \
                found)\n"
            .to_string();
    }
    let mut out = format!("roster names new since {base}: {}\n", new.len());
    for entry in new {
        out.push_str(&format!("  {entry}\n"));
    }
    out
}

/// **A wall time as a verdict line says it**: seconds to one decimal, so a
/// phase that takes under a second is not printed as nothing.
pub fn wall(took: std::time::Duration) -> String {
    format!("wall {:.1}s", took.as_secs_f64())
}

/// **The longest a dev loop run may take on a warm build.** A cold target
/// directory may take longer, once.
pub const LOOP_LIMIT: std::time::Duration = std::time::Duration::from_secs(300);

/// **The last line of a `dev` or `narrow` run: its whole wall time.** A run
/// over the loop limit says so in plain words.
pub fn loop_wall_line(took: std::time::Duration) -> String {
    let seconds = took.as_secs();
    let time = format!("{}m{:02}s", seconds / 60, seconds % 60);
    if took > LOOP_LIMIT {
        format!("wall time: {time} — over the five-minute loop: push and let CI run this")
    } else {
        format!("wall time: {time}")
    }
}

/// **A single ten-line-or-fewer verdict a caller reads instead of the raw
/// stream.** Every phase this build runs — `fmt-check`, `test`, `lint` — adds
/// one line; a failing `test` phase adds a few more, capped, never the whole
/// list.
pub struct Summary {
    lines: Vec<String>,
    pub green: bool,
}

impl Summary {
    pub fn new() -> Self {
        Summary {
            lines: Vec::new(),
            green: true,
        }
    }

    /// **A phase that ran clean.** Recorded as ok; nothing about `green`
    /// changes on its own.
    pub fn phase_ok(&mut self, phase: &str, detail: &str) {
        self.lines.push(format!("{phase}: ok — {detail}"));
    }

    /// **A phase that ran clean, with the wall time it took.** The time is the
    /// phase's own, read around the one command it ran.
    pub fn phase_ok_in(&mut self, phase: &str, detail: &str, took: std::time::Duration) {
        self.phase_ok(phase, &format!("{detail} ({})", wall(took)));
    }

    /// **A phase that failed, with the wall time it took.**
    pub fn phase_failed_in(&mut self, phase: &str, detail: &str, took: std::time::Duration) {
        self.phase_failed(phase, &format!("{detail} ({})", wall(took)));
    }

    /// **A phase that did not run at all**, because an earlier one already
    /// failed. Named rather than left off the summary silently — "which
    /// phase ran" is exactly what a caller cannot answer from a list that
    /// just stops.
    pub fn phase_skipped(&mut self, phase: &str) {
        self.lines
            .push(format!("{phase}: not run — an earlier phase failed"));
    }

    /// **The `test` phase's own verdict**, rendered with the distinctions the
    /// whole mechanism exists for: a compile failure, a crashed suite and a
    /// clean pass can all show `failed == 0` in the text, and only one of
    /// them is fine — see [`test_healthy`].
    ///
    /// **`known` only annotates.** A failing test on the list is printed with
    /// the card that owns its red, and every failure that is not on the list
    /// is printed ahead of those, so a list of known reds cannot push a new
    /// red below the cap. Neither the verdict nor `green` reads the list.
    pub fn test_phase(&mut self, exit_ok: bool, verdict: &TestVerdict, known: &KnownReds) {
        self.green = self.green && test_healthy(exit_ok, verdict);
        if !verdict.compiled {
            self.lines
                .push("test: DID NOT COMPILE — 0 suites ran".to_string());
            return;
        }
        if verdict.failed == 0 {
            if exit_ok {
                self.lines.push(format!(
                    "test: ok — {} suites, {} passed",
                    verdict.suites, verdict.passed
                ));
            } else {
                self.lines.push(format!(
                    "test: FAILED — cargo exited non-zero over {} suites, {} passed, 0 \
                     failed recorded — a suite crashed or was killed — see log",
                    verdict.suites, verdict.passed
                ));
            }
            return;
        }
        self.lines.push(format!(
            "test: FAILED — {} suites, {} passed, {} failed",
            verdict.suites, verdict.passed, verdict.failed
        ));
        const SHOWN: usize = 5;
        // **A known red that says more than it expects is new**, and is listed
        // with the ones that were never known.
        let (listed, unlisted): (Vec<&String>, Vec<&String>) =
            verdict.failed_names.iter().partition(|name| {
                known.find(name).is_some_and(|red| {
                    red.difference(verdict.failures.get(*name).map_or("", String::as_str))
                        .is_none_or(|difference| difference.beyond.is_empty())
                })
            });
        for name in unlisted.into_iter().chain(listed).take(SHOWN) {
            let Some(red) = known.find(name) else {
                self.lines.push(format!("  FAILED: {name}"));
                continue;
            };
            let said = verdict.failures.get(name).map_or("", String::as_str);
            let difference = red.difference(said).unwrap_or_default();
            if difference.beyond.is_empty() {
                let mut line = format!(
                    "  FAILED: {name} (known red: card {}, {}, since {})",
                    red.card, red.owner, red.since
                );
                if !difference.gone.is_empty() {
                    line.push_str(&format!(
                        " — no longer fails on: {}",
                        difference.gone.join("; ")
                    ));
                }
                self.lines.push(line);
            } else {
                self.lines.push(format!(
                    "  FAILED: {name} (NEW beyond known red card {}: {})",
                    red.card,
                    difference.beyond.join(" | ")
                ));
            }
        }
        if verdict.failed_names.len() > SHOWN {
            self.lines.push(format!(
                "  ...and {} more (see log)",
                verdict.failed_names.len() - SHOWN
            ));
        }
    }

    pub fn phase_failed(&mut self, phase: &str, detail: &str) {
        self.green = false;
        self.lines.push(format!("{phase}: FAILED — {detail}"));
    }

    /// **A line under a phase**, as it is: the reasons a phase failed.
    pub fn note(&mut self, line: &str) {
        self.lines.push(line.to_string());
    }

    pub fn log(&mut self, path: &str, line_count: usize) {
        self.lines.push(format!("log: {path} ({line_count} lines)"));
    }

    pub fn render(&self) -> String {
        let verdict = if self.green { "GREEN" } else { "RED" };
        let mut out = format!("verdict: {verdict}\n");
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

impl Default for Summary {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A run inside five minutes prints its time and nothing else; a run
    /// past five says what to do.**
    #[test]
    fn a_loop_run_over_five_minutes_says_so() {
        use std::time::Duration;
        assert_eq!(loop_wall_line(Duration::from_secs(72)), "wall time: 1m12s");
        assert_eq!(loop_wall_line(Duration::from_secs(300)), "wall time: 5m00s");
        let over = loop_wall_line(Duration::from_secs(301));
        assert!(over.starts_with("wall time: 5m01s"), "{over}");
        assert!(
            over.contains("over the five-minute loop: push and let CI run this"),
            "{over}"
        );
    }

    const GREEN: &str = include_str!("../tests/fixtures/green-full-check.txt");
    const ONE_FAILING: &str = include_str!("../tests/fixtures/one-failing-test.txt");
    const COMPILE_ERROR: &str = include_str!("../tests/fixtures/compile-error.txt");
    /// A verbatim `cargo test --workspace --no-run` capture on this workspace.
    const NO_RUN_LISTING: &str = include_str!("../tests/fixtures/no-run-listing.txt");

    /// **Every test binary a real listing names is in exactly one job.** The
    /// fixture is what cargo printed, so this reads the real format. Integration
    /// test files come out one job each, and the three groups (library unit
    /// tests, binary unit tests, doc-tests) one job each, so the jobs together
    /// are the plain workspace run and nothing else.
    #[test]
    fn a_real_listing_becomes_one_job_per_test_file_and_one_per_group() {
        let jobs = test_jobs(NO_RUN_LISTING);
        let files: Vec<&str> = NO_RUN_LISTING
            .lines()
            .filter_map(|l| l.trim().strip_prefix("Executable tests/"))
            .filter_map(|l| l.split(".rs").next())
            .collect();
        assert!(
            files.len() > 20,
            "the fixture lost its test files: {files:?}"
        );
        for file in &files {
            let named: Vec<&Vec<String>> = jobs
                .iter()
                .filter(|j| j == &&vec!["--test".to_string(), file.to_string()])
                .collect();
            assert_eq!(named.len(), 1, "{file} must be exactly one job: {jobs:?}");
        }
        for group in ["--bins", "--doc"] {
            assert!(
                jobs.contains(&vec![group.to_string()]),
                "the {group} job is missing: {jobs:?}"
            );
        }
        let libs = NO_RUN_LISTING
            .lines()
            .filter(|l| l.contains("Executable unittests src/lib.rs"))
            .count();
        assert_eq!(
            jobs.len(),
            files.len() + libs + 2,
            "a job that names no binary in the listing: {jobs:?}"
        );
    }

    /// **Each library's unit tests are a job of their own, named for the
    /// package.** The unit-test binary of a library is named for its package
    /// with the hyphens written as underscores, so the job gives the hyphens
    /// back. A crate whose listing has no library unit-test binary has no job,
    /// and no job runs every library at once.
    #[test]
    fn each_library_with_unit_tests_is_a_job_of_its_own() {
        let jobs = test_jobs(NO_RUN_LISTING);
        let libs: Vec<&str> = jobs
            .iter()
            .filter(|j| j.last().map(String::as_str) == Some("--lib"))
            .map(|j| {
                assert_eq!(j[0], "-p", "a library job names its package: {j:?}");
                j[1].as_str()
            })
            .collect();
        assert_eq!(
            libs,
            [
                "jojobot",
                "jojobot-adapters",
                "jojobot-bar",
                "jojobot-domain",
                "jojobot-exercise",
                "jojobot-mcp",
            ],
            "{jobs:?}"
        );
        assert!(
            !jobs.contains(&vec!["--lib".to_string()]),
            "the workspace-wide library job must be gone: {jobs:?}"
        );
    }

    /// **A library line that names no package still runs**, in a job of its
    /// own for the libraries the listing could not place, so a listing in a
    /// shape this reader does not know drops no test. The line is a real one
    /// with its path taken off.
    #[test]
    fn a_library_line_that_names_no_package_still_runs() {
        let listing = "  Executable unittests src/lib.rs\n\
                       Executable unittests src/lib.rs (/tmp/target/debug/deps/jojobot_mcp-56f677a719b95209)\n";
        assert_eq!(
            test_jobs(listing),
            [
                vec!["-p", "jojobot-mcp", "--lib"],
                vec!["--lib"],
                vec!["--doc"]
            ]
        );
    }

    /// **A job that names its own package runs without the workspace scope**,
    /// because `--workspace` beside `-p` would select every package again. A
    /// job that names none keeps the scope.
    #[test]
    fn a_job_that_names_its_package_drops_the_workspace_scope() {
        let own: Vec<String> = ["-p", "jojobot-domain", "--lib"].map(String::from).to_vec();
        assert_eq!(job_args(&["--workspace"], &own), own);
        let file: Vec<String> = ["--test", "server"].map(String::from).to_vec();
        assert_eq!(
            job_args(&["--workspace"], &file),
            ["--workspace", "--test", "server"]
        );
        let room_scope = ["-p", "jojobot-exercise", "--features", "rooms"];
        assert_eq!(
            job_args(&room_scope, &file),
            [
                "-p",
                "jojobot-exercise",
                "--features",
                "rooms",
                "--test",
                "server"
            ]
        );
    }

    /// **The limit is the default unless the variable says a whole number of
    /// at least one, and anything else is refused by name.**
    #[test]
    fn the_job_limit_is_the_default_or_a_positive_whole_number_and_nothing_else() {
        assert_eq!(jobs_limit(None), Ok(TEST_JOBS_DEFAULT));
        assert_eq!(jobs_limit(Some("3")), Ok(3));
        assert_eq!(jobs_limit(Some(" 12 ")), Ok(12));
        for bad in ["0", "many", "", "-2", "2.5"] {
            let err = jobs_limit(Some(bad)).expect_err(bad);
            assert!(
                err.contains(TEST_JOBS_VARIABLE) && err.contains(&format!("{bad:?}")),
                "the refusal must name the variable and the value: {err}"
            );
        }
    }

    /// **A listing that names no binary still runs the doc-tests and nothing
    /// else.** The doc-tests print no `Executable` line, so their job cannot
    /// depend on the listing naming anything.
    #[test]
    fn an_empty_listing_still_runs_the_doc_tests() {
        assert_eq!(test_jobs(""), vec![vec!["--doc".to_string()]]);
    }

    /// **A real green run reads as compiled, with every suite's counts
    /// summed and nothing failed.** The fixture is a verbatim capture of an
    /// actual `make check` run on this workspace, not an invented shape —
    /// so this proves the parser against the real format, not a guess at it.
    #[test]
    fn a_real_green_run_sums_every_suites_counts_and_fails_nothing() {
        let verdict = summarize_test_output(GREEN);
        assert!(verdict.compiled, "a green run has test result: lines");
        assert_eq!(verdict.suites, 11, "the fixture's own real suite count");
        assert_eq!(verdict.failed, 0);
        assert!(verdict.failed_names.is_empty());
        assert!(verdict.passed > 0, "some suite in the fixture passed tests");
        assert!(verdict.ok());
    }

    /// **A real run with one genuine failure names it, and only it.** The
    /// fixture is a verbatim capture of `cargo test -p jojobot-domain --lib`
    /// with one assertion sabotaged, so the single failing name in it is
    /// real, not constructed to fit the parser.
    #[test]
    fn a_real_single_failure_is_named_and_the_rest_still_counted() {
        let verdict = summarize_test_output(ONE_FAILING);
        assert!(verdict.compiled);
        assert_eq!(verdict.failed, 1);
        assert_eq!(
            verdict.failed_names,
            vec!["attention::tests::a_rhythm_is_overdue_from_the_day_it_falls_due"],
        );
        assert_eq!(
            verdict.passed, 258,
            "the 258 that passed are still counted beside the one that did not",
        );
        assert!(!verdict.ok());
    }

    /// **A real compile error reads as `compiled: false`, never as zero
    /// suites failed.** This is the exact confusion the mechanism exists to
    /// end: the fixture is a verbatim capture of a genuine syntax error, and
    /// it carries no `test result:` line at all — nothing ran.
    #[test]
    fn a_real_compile_error_reads_as_did_not_compile_not_as_a_clean_pass() {
        let verdict = summarize_test_output(COMPILE_ERROR);
        assert!(
            !verdict.compiled,
            "no test result: line exists in this fixture"
        );
        assert_eq!(verdict.suites, 0);
        assert_eq!(
            verdict.failed, 0,
            "there is nothing to have failed — nothing ran"
        );
        assert!(!verdict.ok(), "did-not-compile must never read as ok");
    }

    /// **A crashed or killed suite prints no `test result:` line for itself,
    /// while every other suite under `--no-fail-fast` still prints a clean
    /// one.** The text alone reads this as a pass — nothing named it as
    /// failed, because a corpse cannot print `... FAILED`. Only the process's
    /// own exit status saw it, so `test_healthy` must read `false` here even
    /// though `verdict.ok()` reads `true`.
    #[test]
    fn a_crashed_suite_is_unhealthy_even_though_the_text_alone_reads_clean() {
        let verdict = summarize_test_output(
            "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
             finished in 0.01s\n",
        );
        assert!(verdict.ok(), "the text alone has nothing marked failed");
        assert!(
            !test_healthy(false, &verdict),
            "a non-zero exit must override a clean-looking verdict"
        );
        assert!(
            test_healthy(true, &verdict),
            "a real clean run stays healthy"
        );
    }

    /// **The rendered summary names the crash rather than reading like a
    /// pass or like a counted test failure.** Distinct from both: it is not
    /// `test: ok` (something crashed) and it is not `N failed` (nothing was
    /// counted as failing — there is no name to show).
    #[test]
    fn the_rendered_summary_names_a_crashed_suite_distinctly_from_a_clean_pass() {
        let verdict = summarize_test_output(
            "test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
             finished in 0.01s\n",
        );
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &KnownReds::default());
        let rendered = summary.render();
        assert!(rendered.contains("verdict: RED"), "{rendered}");
        assert!(!rendered.contains("test: ok"), "{rendered}");
        assert!(
            rendered.contains("crashed or was killed"),
            "the reader needs to know this was not a counted failure: {rendered}"
        );
    }

    /// **The rendered summary distinguishes the two states by name**, not
    /// only by a boolean a reader has to already know to check.
    #[test]
    fn the_rendered_summary_names_did_not_compile_distinctly_from_a_clean_pass() {
        let mut clean = Summary::new();
        clean.test_phase(true, &summarize_test_output(GREEN), &KnownReds::default());
        assert!(clean.render().contains("test: ok"));

        let mut broken = Summary::new();
        broken.test_phase(
            false,
            &summarize_test_output(COMPILE_ERROR),
            &KnownReds::default(),
        );
        let rendered = broken.render();
        assert!(
            rendered.contains("DID NOT COMPILE"),
            "a build failure must say so in words, not read like a pass: {rendered}",
        );
        assert!(!rendered.contains("0 failed"), "{rendered}");
    }

    /// **A failing test's name is readable straight from the summary text**,
    /// with no need to open the log — the fact a caller most needs is the one
    /// this mechanism must never make them dig for.
    #[test]
    fn a_failed_tests_name_is_readable_from_the_summary_alone() {
        let mut summary = Summary::new();
        summary.test_phase(
            false,
            &summarize_test_output(ONE_FAILING),
            &KnownReds::default(),
        );
        let rendered = summary.render();
        assert!(
            rendered.contains("attention::tests::a_rhythm_is_overdue_from_the_day_it_falls_due"),
            "{rendered}",
        );
        assert!(rendered.contains("verdict: RED"));
    }

    /// **The summary stays short.** However many tests a suite runs, the
    /// rendered text is a handful of lines — the whole reason this exists.
    #[test]
    fn the_summary_stays_under_ten_lines_even_over_a_real_multi_suite_run() {
        let mut summary = Summary::new();
        summary.phase_ok("fmt-check", "formatted");
        summary.test_phase(true, &summarize_test_output(GREEN), &KnownReds::default());
        summary.phase_ok("lint", "clean");
        summary.log("target/bar/check.log", GREEN.lines().count());
        let rendered = summary.render();
        assert!(
            rendered.lines().count() <= 10,
            "the whole point is a verdict nobody has to scroll: {rendered}",
        );
    }

    const KNOWN_FAILING: &str = "attention::tests::a_rhythm_is_overdue_from_the_day_it_falls_due";

    fn listing(test: &str) -> KnownReds {
        KnownReds::parse(&format!(
            "[[known_red]]\ntest = \"{test}\"\ncard = 2036\nowner = \"dev3\"\n\
             since = \"2026-10-07\"\n"
        ))
        .expect("a well-formed list")
    }

    /// **A failing test on the list is annotated with its card, owner and
    /// date, and the verdict stays red.** The run is the real one-failure
    /// capture, so the name on the list is a name cargo printed.
    #[test]
    fn a_failing_test_on_the_list_is_annotated_and_the_verdict_stays_red() {
        let verdict = summarize_test_output(ONE_FAILING);
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &listing(KNOWN_FAILING));
        let rendered = summary.render();
        assert!(
            rendered.contains(&format!(
                "  FAILED: {KNOWN_FAILING} (known red: card 2036, dev3, since 2026-10-07)"
            )),
            "{rendered}"
        );
        assert!(rendered.contains("verdict: RED"), "{rendered}");
        assert!(rendered.contains("test: FAILED"), "{rendered}");
        assert!(!summary.green, "a known red is information, never a pass");
        // The exit decision is the one it always was.
        assert!(!test_healthy(false, &verdict));
    }

    /// **A failing test that is not on the list prints exactly as it always
    /// did.** The paired negative: a list naming some other test annotates
    /// nothing here.
    #[test]
    fn a_failing_test_not_on_the_list_prints_without_an_annotation() {
        let verdict = summarize_test_output(ONE_FAILING);
        let mut summary = Summary::new();
        summary.test_phase(
            false,
            &verdict,
            &listing("elsewhere::tests::some_other_test"),
        );
        let rendered = summary.render();
        assert!(
            rendered
                .lines()
                .any(|l| l == format!("  FAILED: {KNOWN_FAILING}")),
            "{rendered}"
        );
        assert!(!rendered.contains("known red"), "{rendered}");
        assert!(rendered.contains("verdict: RED"), "{rendered}");
    }

    /// **A run whose only failures are all known is still red.** Annotating
    /// every failure must not read as an excuse.
    #[test]
    fn a_run_of_only_known_reds_is_still_red() {
        let verdict = summarize_test_output(ONE_FAILING);
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &listing(KNOWN_FAILING));
        assert!(!summary.green);
        assert!(summary.render().contains("verdict: RED"));
    }

    /// **A failure that is not known is listed before the known ones**, so
    /// a list of known reds can never push a new red below the cap.
    #[test]
    fn an_unknown_failure_is_listed_ahead_of_known_ones() {
        let known: Vec<String> = (0..6).map(|n| format!("m::tests::known_{n}")).collect();
        let text: String = known
            .iter()
            .map(|name| {
                format!(
                    "[[known_red]]\ntest = \"{name}\"\ncard = 1\nowner = \"dev\"\n\
                     since = \"2026-10-07\"\n"
                )
            })
            .collect();
        let verdict = TestVerdict {
            compiled: true,
            suites: 1,
            passed: 0,
            failed: 7,
            failed_names: known
                .iter()
                .cloned()
                .chain(["m::tests::brand_new".to_string()])
                .collect(),
            ..TestVerdict::default()
        };
        let mut summary = Summary::new();
        summary.test_phase(
            false,
            &verdict,
            &KnownReds::parse(&text).expect("a well-formed list"),
        );
        let rendered = summary.render();
        let new = rendered.find("m::tests::brand_new").expect(&rendered);
        let first_known = rendered.find("m::tests::known_0").expect(&rendered);
        assert!(new < first_known, "{rendered}");
        assert!(rendered.contains("...and 2 more"), "{rendered}");
    }

    /// A run with one failing test, as `cargo test` prints it: the line, then the
    /// failures section with the test's own output.
    fn one_failure_saying(items: &[&str]) -> String {
        let mut text = String::from(
            "running 2 tests\ntest m::tests::fine ... ok\n\
             test m::tests::the_roster_one ... FAILED\n\nfailures:\n\n\
             ---- m::tests::the_roster_one stdout ----\n\n\
             thread 'm::tests::the_roster_one' (31) panicked at a.rs:9:5:\n\
             handles outside the roster, add them:\n",
        );
        for item in items {
            text.push_str(item);
            text.push('\n');
        }
        text.push_str(
            "note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace\n\n\n\
             failures:\n    m::tests::the_roster_one\n\n\
             test result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; \
             finished in 0.01s\n",
        );
        text
    }

    fn expecting(items: &str) -> KnownReds {
        KnownReds::parse(&format!(
            "[[known_red]]\ntest = \"m::tests::the_roster_one\"\ncard = 2036\n\
             owner = \"dev3\"\nsince = \"2026-10-07\"\nexpects = [{items}]\n"
        ))
        .expect("a well-formed list")
    }

    const THE_ROSTER_ITEMS: &str = "\"handles outside the roster\", \"person:homer in \"";

    /// **What a failing test printed is kept with its name**, from the run's own
    /// text: the output between its header and the next section.
    #[test]
    fn a_failing_tests_own_output_is_kept_under_its_name() {
        let verdict = summarize_test_output(&one_failure_saying(&["person:homer in /x/A.md"]));
        let said = verdict
            .failures
            .get("m::tests::the_roster_one")
            .expect("the failure section was read");
        assert!(said.contains("person:homer in /x/A.md"), "{said}");
        assert!(!said.contains("test result"), "the section ended: {said}");
        assert!(!said.contains("failures:"), "the section ended: {said}");
    }

    /// **A known red that fails on exactly what it expects stays a known red.**
    #[test]
    fn a_red_that_fails_on_what_it_expects_is_annotated() {
        let verdict = summarize_test_output(&one_failure_saying(&["person:homer in /x/A.md"]));
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &expecting(THE_ROSTER_ITEMS));
        let rendered = summary.render();
        assert!(
            rendered.contains(
                "  FAILED: m::tests::the_roster_one (known red: card 2036, dev3, since 2026-10-07)\n"
            ),
            "{rendered}"
        );
        assert!(!rendered.contains("NEW"), "{rendered}");
    }

    /// **A known red that fails on more than it expects is a new red.** The
    /// extra line is named, the known annotation is not given, and the failure
    /// is listed ahead of the ones that are only known.
    #[test]
    fn a_red_that_fails_on_more_than_it_expects_is_new() {
        let verdict = summarize_test_output(&one_failure_saying(&[
            "person:homer in /x/A.md",
            "person:lisa in /x/B.md",
        ]));
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &expecting(THE_ROSTER_ITEMS));
        let rendered = summary.render();
        assert!(
            rendered.contains("NEW beyond known red card 2036: person:lisa in /x/B.md"),
            "{rendered}"
        );
        assert!(!rendered.contains("(known red:"), "{rendered}");
        assert!(rendered.contains("verdict: RED"), "{rendered}");
    }

    /// **An expected item that is no longer there is said**, beside the known
    /// annotation: the entry can be tightened.
    #[test]
    fn an_expected_item_that_is_gone_is_said() {
        let verdict = summarize_test_output(&one_failure_saying(&[]));
        let mut summary = Summary::new();
        summary.test_phase(false, &verdict, &expecting(THE_ROSTER_ITEMS));
        let rendered = summary.render();
        assert!(
            rendered.contains("(known red: card 2036, dev3, since 2026-10-07) — no longer fails on: person:homer in "),
            "{rendered}"
        );
    }

    /// **A known red that says more than it expects is listed ahead of the
    /// reds that are only known**, so a cap on the list cannot hide it.
    #[test]
    fn a_red_with_more_than_it_expects_is_listed_ahead_of_known_ones() {
        let text = format!(
            "test m::tests::plain_known ... FAILED\n{}",
            one_failure_saying(&["person:homer in /x/A.md", "person:lisa in /x/B.md"])
        );
        let verdict = summarize_test_output(&text);
        let mut list = String::from(
            "[[known_red]]\ntest = \"m::tests::plain_known\"\ncard = 1\nowner = \"dev\"\n\
             since = \"2026-10-07\"\n",
        );
        list.push_str(&format!(
            "[[known_red]]\ntest = \"m::tests::the_roster_one\"\ncard = 2036\n\
             owner = \"dev3\"\nsince = \"2026-10-07\"\nexpects = [{THE_ROSTER_ITEMS}]\n"
        ));
        let mut summary = Summary::new();
        summary.test_phase(
            false,
            &verdict,
            &KnownReds::parse(&list).expect("a well-formed list"),
        );
        let rendered = summary.render();
        let new = rendered.find("NEW beyond known red").expect(&rendered);
        let known = rendered.find("m::tests::plain_known").expect(&rendered);
        assert!(new < known, "{rendered}");
    }
}

#[cfg(test)]
mod roster_report_tests {
    use super::*;

    const SOURCE: &str = r#"
use std::fs;

/// The cast.
const ROSTER: &[&str] = &[
    "bot:assistant",
    // Bob's Burgers.
    "person:gayle",
    "person:stan", // made up: "Stan" the second
];

/// Handles that name nobody.
const ORDINARY: &[&str] = &[
    "thing:gravel-bike",
];
"#;

    fn names(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_string()).collect()
    }

    /// **Only the cast list is read**, a comment is skipped, and a quotation in a
    /// trailing comment is not an entry.
    #[test]
    fn only_the_roster_list_is_read() {
        assert_eq!(
            roster_entries(SOURCE),
            names(&["bot:assistant", "person:gayle", "person:stan"])
        );
    }

    /// **A name that arrives on the cast list is reported, and one that arrives on
    /// the ordinary list is not** — the two cases side by side, because a report
    /// that named everything would pass the first alone.
    #[test]
    fn a_new_cast_name_is_reported_and_a_new_ordinary_one_is_not() {
        let before = roster_entries(SOURCE);
        let with_both = SOURCE
            .replace(
                "    \"person:stan\", // made up: \"Stan\" the second\n",
                "    \"person:stan\", // made up: \"Stan\" the second\n    \"person:kyle\",\n",
            )
            .replace(
                "    \"thing:gravel-bike\",\n",
                "    \"thing:gravel-bike\",\n    \"thing:folding-chairs\",\n",
            );
        let arrived = new_roster_entries(&roster_entries(&with_both), &before);
        assert_eq!(arrived, names(&["person:kyle"]));
        let rendered = render_roster_report(&arrived, "abc1234");
        assert!(rendered.contains("person:kyle"), "{rendered}");
        assert!(!rendered.contains("folding-chairs"), "{rendered}");
        assert!(rendered.contains("new since abc1234: 1"), "{rendered}");
    }

    /// An entry that left the list is not new, one that stays is not new, and a
    /// name written twice is reported once.
    #[test]
    fn removed_and_repeated_entries_are_not_reported_twice() {
        let then = names(&["person:stan", "person:kyle"]);
        let now = names(&["person:stan", "person:wendy", "person:wendy"]);
        assert_eq!(new_roster_entries(&now, &then), names(&["person:wendy"]));
    }

    /// With no base to compare to the report says so rather than naming every
    /// entry as new.
    #[test]
    fn no_base_ref_says_unknown() {
        let rendered = render_roster_report(&names(&["person:kyle"]), "");
        assert!(rendered.contains("unknown"), "{rendered}");
        assert!(!rendered.contains("person:kyle"), "{rendered}");
    }
}
