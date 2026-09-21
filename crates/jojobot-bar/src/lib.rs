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
    /// **Compiled, and nothing failed.** The one question a caller almost
    /// always wants answered first.
    pub fn ok(&self) -> bool {
        self.compiled && self.failed == 0
    }
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

    /// **The `test` phase's own verdict**, rendered with the distinction the
    /// whole mechanism exists for: a compile failure and a clean pass both
    /// have `failed == 0`, and only one of them is fine.
    pub fn test_phase(&mut self, verdict: &TestVerdict) {
        self.green = self.green && verdict.ok();
        if !verdict.compiled {
            self.lines
                .push("test: DID NOT COMPILE — 0 suites ran".to_string());
            return;
        }
        if verdict.failed == 0 {
            self.lines.push(format!(
                "test: ok — {} suites, {} passed",
                verdict.suites, verdict.passed
            ));
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
mod tests {
    use super::*;

    const GREEN: &str = include_str!("../tests/fixtures/green-full-check.txt");
    const ONE_FAILING: &str = include_str!("../tests/fixtures/one-failing-test.txt");
    const COMPILE_ERROR: &str = include_str!("../tests/fixtures/compile-error.txt");

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

    /// **The rendered summary distinguishes the two states by name**, not
    /// only by a boolean a reader has to already know to check.
    #[test]
    fn the_rendered_summary_names_did_not_compile_distinctly_from_a_clean_pass() {
        let mut clean = Summary::new();
        clean.test_phase(&summarize_test_output(GREEN));
        assert!(clean.render().contains("test: ok"));

        let mut broken = Summary::new();
        broken.test_phase(&summarize_test_output(COMPILE_ERROR));
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
        summary.test_phase(&summarize_test_output(ONE_FAILING));
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
        summary.test_phase(&summarize_test_output(GREEN));
        summary.phase_ok("lint", "clean");
        summary.log("target/bar/check.log", GREEN.lines().count());
        let rendered = summary.render();
        assert!(
            rendered.lines().count() <= 10,
            "the whole point is a verdict nobody has to scroll: {rendered}",
        );
    }
}
