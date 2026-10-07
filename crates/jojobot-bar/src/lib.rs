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
    for line in output.lines() {
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
/// that file wherever it lives. The unit tests of every library, the unit tests
/// of every binary and the doc-tests are one job each. Nothing is skipped: a
/// binary the plain run would execute is in exactly one of these.
pub fn test_jobs(listing: &str) -> Vec<Vec<String>> {
    let mut libs = false;
    let mut bins = false;
    let mut files: Vec<String> = Vec::new();
    for line in listing.lines() {
        let Some(executable) = line.trim().strip_prefix("Executable ") else {
            continue;
        };
        if let Some(source) = executable.strip_prefix("unittests ") {
            if source.starts_with("src/lib.rs") {
                libs = true;
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
    if libs {
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

/// **A tracked source file and how many lines it holds now.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileSize {
    pub path: String,
    pub lines: usize,
}

/// **A tracked source file and how many lines it gained since the base
/// ref.** Negative when a file shrank; only positive deltas are reported.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileGrowth {
    pub path: String,
    pub delta: i64,
}

/// A file over this many lines is reported, decision log 273 (the
/// operator's ruling on repo scalability). Overturnable — this is a report,
/// not a gate.
pub const OVERSIZED_LINES: usize = 2000;

/// A file that grew by more than this many lines in one batch is reported.
pub const GROWTH_LINES: i64 = 200;

/// **Every file over [`OVERSIZED_LINES`], largest first.** A tree with none
/// over the threshold returns an empty list — this names files, never a
/// count of zero dressed up as a name.
pub fn oversized_files(files: &[FileSize]) -> Vec<&FileSize> {
    let mut over: Vec<&FileSize> = files.iter().filter(|f| f.lines > OVERSIZED_LINES).collect();
    over.sort_by(|a, b| b.lines.cmp(&a.lines).then_with(|| a.path.cmp(&b.path)));
    over
}

/// **Every file that grew by more than [`GROWTH_LINES`], largest delta
/// first.** A shrunk or unchanged file is never in this list — only growth
/// is what a size tripwire exists to catch.
pub fn grown_files(files: &[FileGrowth]) -> Vec<&FileGrowth> {
    let mut grown: Vec<&FileGrowth> = files.iter().filter(|f| f.delta > GROWTH_LINES).collect();
    grown.sort_by(|a, b| b.delta.cmp(&a.delta).then_with(|| a.path.cmp(&b.path)));
    grown
}

/// **The two lists, rendered.** Report only: nothing here is a verdict, and
/// nothing here ever changes `Summary::green`.
pub fn render_size_report(oversized: &[&FileSize], grown: &[&FileGrowth], base: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "files over {OVERSIZED_LINES} lines: {}\n",
        oversized.len()
    ));
    for f in oversized {
        out.push_str(&format!("  {} ({} lines)\n", f.path, f.lines));
    }
    if base.is_empty() {
        out.push_str(&format!(
            "files grown over {GROWTH_LINES} lines: unknown — no base ref (origin/main not \
             found)\n"
        ));
    } else {
        out.push_str(&format!(
            "files grown over {GROWTH_LINES} lines against {base}: {}\n",
            grown.len()
        ));
        for f in grown {
            out.push_str(&format!("  {} (+{} lines)\n", f.path, f.delta));
        }
    }
    out
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
    pub fn test_phase(&mut self, exit_ok: bool, verdict: &TestVerdict) {
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
        for name in verdict.failed_names.iter().take(SHOWN) {
            self.lines.push(format!("  FAILED: {name}"));
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
mod size_report_tests {
    use super::*;

    fn size(path: &str, lines: usize) -> FileSize {
        FileSize {
            path: path.to_string(),
            lines,
        }
    }

    fn growth(path: &str, delta: i64) -> FileGrowth {
        FileGrowth {
            path: path.to_string(),
            delta,
        }
    }

    /// **A tree with an oversized file names it, largest first.**
    #[test]
    fn an_oversized_file_is_named_largest_first() {
        let files = [
            size("crates/a/src/lib.rs", 500),
            size("crates/b/src/lib.rs", 2001),
        ];
        let over = oversized_files(&files);
        assert_eq!(
            over.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["crates/b/src/lib.rs"],
            "the file at the threshold's boundary must not appear: {over:?}",
        );
    }

    /// **The paired negative: a tree with nothing over the threshold names
    /// nothing.** Not a count of zero dressed up as a name — an empty list.
    #[test]
    fn a_tree_with_nothing_oversized_names_nothing() {
        let files = [
            size("crates/a/src/lib.rs", 500),
            size("crates/b/src/lib.rs", OVERSIZED_LINES),
        ];
        assert!(
            oversized_files(&files).is_empty(),
            "a file exactly AT the threshold is not OVER it",
        );
    }

    /// **A tree with a file that grew past the threshold names it.**
    #[test]
    fn a_grown_file_is_named_largest_delta_first() {
        let files = [
            growth("crates/a/src/lib.rs", 50),
            growth("crates/b/src/lib.rs", 201),
        ];
        let grown = grown_files(&files);
        assert_eq!(
            grown.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
            vec!["crates/b/src/lib.rs"],
        );
    }

    /// **The paired negative: nothing grew past the threshold, nothing is
    /// named** — a file that shrank is not growth, and a delta exactly at
    /// the threshold is not past it.
    #[test]
    fn a_tree_with_nothing_grown_names_nothing() {
        let files = [
            growth("crates/a/src/lib.rs", -300),
            growth("crates/b/src/lib.rs", GROWTH_LINES),
        ];
        assert!(grown_files(&files).is_empty());
    }

    /// **The rendered report names the file, not only a count.**
    #[test]
    fn the_rendered_report_names_an_oversized_and_a_grown_file() {
        let big = size("crates/b/src/lib.rs", 2500);
        let over = vec![&big];
        let moved = growth("crates/c/src/lib.rs", 300);
        let up = vec![&moved];
        let rendered = render_size_report(&over, &up, "abc1234");
        assert!(
            rendered.contains("crates/b/src/lib.rs (2500 lines)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("crates/c/src/lib.rs (+300 lines)"),
            "{rendered}"
        );
        assert!(rendered.contains("abc1234"), "{rendered}");
    }

    /// **No base ref says so, rather than reporting an empty list that reads
    /// like a clean tree.**
    #[test]
    fn no_base_ref_is_named_rather_than_read_as_nothing_grown() {
        let rendered = render_size_report(&[], &[], "");
        assert!(rendered.contains("unknown"), "{rendered}");
        assert!(!rendered.contains("against"), "{rendered}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        for group in ["--lib", "--bins", "--doc"] {
            assert!(
                jobs.contains(&vec![group.to_string()]),
                "the {group} job is missing: {jobs:?}"
            );
        }
        assert_eq!(
            jobs.len(),
            files.len() + 3,
            "a job that names no binary in the listing: {jobs:?}"
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
        summary.test_phase(false, &verdict);
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
        clean.test_phase(true, &summarize_test_output(GREEN));
        assert!(clean.render().contains("test: ok"));

        let mut broken = Summary::new();
        broken.test_phase(false, &summarize_test_output(COMPILE_ERROR));
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
        summary.test_phase(false, &summarize_test_output(ONE_FAILING));
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
        summary.test_phase(true, &summarize_test_output(GREEN));
        summary.phase_ok("lint", "clean");
        summary.log("target/bar/check.log", GREEN.lines().count());
        let rendered = summary.render();
        assert!(
            rendered.lines().count() <= 10,
            "the whole point is a verdict nobody has to scroll: {rendered}",
        );
    }
}
