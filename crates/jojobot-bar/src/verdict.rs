//! **The machine verdict of a test job**: one JSON file that says which suites
//! were red and why, whether the runner or the code is to blame, and when the
//! job has no verdict at all.
//!
//! [`read`] is the one function that turns a test run's text and its exit status
//! into a [`Verdict`]. It lists every red suite, never the first one only. A
//! suite that fails for a reason of the machine is marked with its infra class;
//! a reason it cannot place stays unplaced and is never guessed as either kind.
//!
//! A job the forge cancels runs no code after the cancel, so it writes no file.
//! A reader takes a missing file as "no verdict".

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::{Value, json};

use crate::history::{Verdict as TestOutcome, parse_test_line};
use crate::suite_counts;

/// **What the job as a whole was.** `Infra` means every red thing in the run
/// failed for a reason of the machine; one red suite of the code makes the job
/// `Red`, and the file still names the infra class beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Overall {
    Green,
    Red,
    Infra,
    NoVerdict,
}

impl Overall {
    pub fn word(self) -> &'static str {
        match self {
            Overall::Green => "green",
            Overall::Red => "red",
            Overall::Infra => "infra",
            Overall::NoVerdict => "no-verdict",
        }
    }
}

/// **Why a failure belongs to the machine and not to the code.** Each class is
/// its own value. A reason that matches none of them is not here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Infra {
    DiskFull,
    OutOfMemory,
    Killed,
    NetworkFetch,
}

impl Infra {
    pub fn word(self) -> &'static str {
        match self {
            Infra::DiskFull => "disk-full",
            Infra::OutOfMemory => "out-of-memory",
            Infra::Killed => "killed",
            Infra::NetworkFetch => "network-fetch",
        }
    }
}

/// **The text each class is read from.** The first two are a full disk as Rust
/// prints it: the operating system's words with the error number, and the
/// debug form that `unwrap` prints. The
/// second is the line the allocator prints before a process aborts. The third
/// and fourth are how cargo names a binary that a signal ended. The last two
/// are cargo's lines for a registry it could not reach.
const INFRA_TEXT: [(&str, Infra); 8] = [
    ("No space left on device (os error 28)", Infra::DiskFull),
    ("code: 28, kind: StorageFull", Infra::DiskFull),
    ("memory allocation of ", Infra::OutOfMemory),
    ("(signal: 9, SIGKILL", Infra::Killed),
    ("(signal: 15, SIGTERM", Infra::Killed),
    ("spurious network error", Infra::NetworkFetch),
    ("failed to download from", Infra::NetworkFetch),
    ("Could not resolve host", Infra::NetworkFetch),
];

fn infra_of(line: &str) -> Option<Infra> {
    INFRA_TEXT
        .iter()
        .find(|(text, _)| line.contains(text))
        .map(|(_, class)| *class)
}

/// **One failing test and the first thing it said**: the place of its panic and
/// the line after it, or the first line of its output when it did not panic.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Failure {
    pub test: String,
    pub first_line: String,
    /// The class, when the output of this one failure names a machine reason.
    pub infra: Option<Infra>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Suite {
    pub name: String,
    pub passed: usize,
    pub failed: usize,
    /// The binary ended without a `test result:` line, by a signal or an exit.
    pub crashed: bool,
    /// The class a line outside any failure's output named: a killed binary.
    pub infra: Option<Infra>,
    /// Whether the suite printed a `test result:` line.
    pub reported: bool,
    pub failures: Vec<Failure>,
}

impl Suite {
    fn new(name: &str) -> Self {
        Suite {
            name: name.to_string(),
            ..Suite::default()
        }
    }

    fn red(&self) -> bool {
        self.failed > 0 || self.crashed || !self.failures.is_empty()
    }

    /// **Whether something in this suite is red for a reason the output does
    /// not place on the machine.** One failure without a class is enough, and so
    /// is a failed test that has no name or a crash that no line explains.
    fn code_red(&self) -> bool {
        let unexplained =
            (self.crashed || self.failed > self.failures.len()) && self.infra.is_none();
        unexplained || self.failures.iter().any(|f| f.infra.is_none())
    }

    /// The class of a red suite whose every red thing is a machine reason.
    pub fn infra_class(&self) -> Option<Infra> {
        if self.code_red() {
            return None;
        }
        self.seen_infra()
    }

    /// The first class any line of this suite named, mixed or not.
    fn seen_infra(&self) -> Option<Infra> {
        self.infra
            .or_else(|| self.failures.iter().find_map(|f| f.infra))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    pub overall: Overall,
    /// The class of the first infra failure in the run, if any.
    pub infra: Option<Infra>,
    pub exit_ok: bool,
    /// At least one suite reported.
    pub compiled: bool,
    /// How many suites reported, red or not.
    pub suites_total: usize,
    pub passed: usize,
    /// Jobs the bar did not run or killed because an earlier job was red.
    pub not_run: usize,
    /// The red suites only, in the order the run printed them.
    pub red: Vec<Suite>,
    /// The phases of the job, other than the tests, that failed: the lint, the
    /// size gate. Set by [`settle`]; a text cannot say it.
    pub failed_phases: Vec<String>,
}

/// **The name of a suite from its `Running` line**: the binary's file name
/// without the hash cargo adds (`jojobot_adapters-4060…` is `jojobot_adapters`),
/// or the text as it is when no path follows.
fn binary_name(path: &str) -> &str {
    let file = path.rsplit('/').next().unwrap_or(path);
    match file.rsplit_once('-') {
        Some((name, hash)) if !hash.is_empty() && hash.chars().all(|c| c.is_ascii_hexdigit()) => {
            name
        }
        _ => file,
    }
}

fn suite_start(line: &str) -> Option<String> {
    if let Some(rest) = line.trim_start().strip_prefix("Doc-tests ") {
        return Some(format!("doc-tests {}", rest.trim()));
    }
    let rest = line.trim_start().strip_prefix("Running ")?;
    Some(match rest.rsplit_once(" (") {
        Some((what, path)) => {
            let bin = binary_name(path.trim_end_matches(')'));
            match what.strip_prefix("unittests ") {
                Some(_) => format!("{bin} (unit tests)"),
                None => bin.to_string(),
            }
        }
        None => binary_name(rest.trim()).to_string(),
    })
}

/// The binary a "process didn't exit successfully" line names.
fn crashed_binary(line: &str) -> Option<&str> {
    let rest = line.split("process didn't exit successfully: `").nth(1)?;
    let path = rest.split('`').next()?.split_whitespace().next()?;
    Some(binary_name(path))
}

/// **One suite as the text printed it**, before anything is decided about it.
#[derive(Default)]
struct Record<'a> {
    name: String,
    /// How many `test result:` lines it printed. None means it never reported.
    results: usize,
    passed: usize,
    failed: usize,
    /// The `test <name> ... FAILED` lines, in print order.
    failing: Vec<String>,
    /// The output under each `---- <name> stdout ----` header, line by line.
    bodies: Vec<(&'a str, Vec<&'a str>)>,
    /// Every other line, except the lines about single tests.
    loose: Vec<&'a str>,
}

/// **A test run's text read once**: the lines before the first suite and the
/// suites. The summary and the verdict are both projections of this.
#[derive(Default)]
pub(crate) struct Parsed<'a> {
    preamble: Vec<&'a str>,
    records: Vec<Record<'a>>,
}

/// **Read a `cargo test` run's combined output into its suites.** It never
/// looks at an exit status. A suite's lines with no `Running` line before them
/// are still a suite, so a red is not dropped for want of a header.
pub(crate) fn parse(text: &str) -> Parsed<'_> {
    let mut out = Parsed::default();
    // The failure whose output the lines are inside: a record and a body.
    let mut inside: Option<(usize, usize)> = None;
    for line in text.lines() {
        if let Some(name) = suite_start(line) {
            if let Some((r, b)) = inside {
                out.records[r].bodies[b].1.push(line);
            }
            out.records.push(Record {
                name,
                ..Record::default()
            });
            continue;
        }
        let starts_a_suite = ["test ", "---- ", "failures:"]
            .iter()
            .any(|p| line.starts_with(p));
        if out.records.is_empty() {
            if !starts_a_suite {
                out.preamble.push(line);
                continue;
            }
            out.records.push(Record {
                name: "(unnamed)".to_string(),
                ..Record::default()
            });
        }
        let current = out.records.len() - 1;
        if let Some(name) = line
            .strip_prefix("---- ")
            .and_then(|rest| rest.strip_suffix(" stdout ----"))
        {
            out.records[current].bodies.push((name, Vec::new()));
            inside = Some((current, out.records[current].bodies.len() - 1));
            continue;
        }
        let mut in_body = false;
        if line == "failures:" || line.starts_with("test result: ") || line.starts_with("---- ") {
            inside = None;
        } else if let Some((r, b)) = inside {
            out.records[r].bodies[b].1.push(line);
            in_body = true;
        }
        let record = &mut out.records[current];
        if let Some(rest) = line.strip_prefix("test result: ") {
            record.results += 1;
            if let Some((passed, failed)) = suite_counts(rest) {
                record.passed += passed;
                record.failed += failed;
            }
        } else if let Some(test) = parse_test_line(line) {
            if test.verdict == TestOutcome::Failed {
                record.failing.push(test.test);
            }
        } else if !in_body {
            record.loose.push(line);
        }
    }
    out
}

impl Parsed<'_> {
    /// **The summary of the run**: the counts and the failures the text states.
    pub(crate) fn test_verdict(&self) -> crate::TestVerdict {
        let mut verdict = crate::TestVerdict::default();
        for record in &self.records {
            verdict.suites += record.results;
            verdict.passed += record.passed;
            verdict.failed += record.failed;
            verdict.failed_names.extend(record.failing.iter().cloned());
            for (name, lines) in &record.bodies {
                let body = verdict.failures.entry((*name).to_string()).or_default();
                for line in lines {
                    body.push_str(line);
                    body.push('\n');
                }
            }
        }
        verdict.compiled = verdict.suites > 0;
        verdict
    }
}

/// **The first thing a failing test said**: the place of its panic and the line
/// under it, or its first non-blank line when it did not panic.
fn first_line(lines: &[&str]) -> String {
    let mut first = String::new();
    let mut awaiting: Option<String> = None;
    for line in lines {
        if let Some(place) = line.split(" panicked at ").nth(1) {
            awaiting = Some(place.trim().to_string());
            first = place.trim().to_string();
        } else if let Some(place) = awaiting.take_if(|_| !line.trim().is_empty()) {
            first = format!("{place} {}", line.trim());
        } else if first.is_empty() && !line.trim().is_empty() {
            first = line.trim().to_string();
        }
    }
    first
}

impl Suite {
    /// The suite as the verdict holds it. A name that appears in a `FAILED` line
    /// and under a header is one failure.
    fn of(record: &Record<'_>) -> Self {
        let mut suite = Suite::new(&record.name);
        suite.passed = record.passed;
        suite.failed = record.failed;
        suite.reported = record.results > 0;
        let named = record
            .failing
            .iter()
            .map(String::as_str)
            .chain(record.bodies.iter().map(|(name, _)| *name));
        for name in named {
            if suite.failures.iter().any(|f| f.test == name) {
                continue;
            }
            let lines: Vec<&str> = record
                .bodies
                .iter()
                .filter(|(body, _)| *body == name)
                .flat_map(|(_, lines)| lines.iter().copied())
                .collect();
            // A line about a single test is not a reason.
            let infra = lines
                .iter()
                .filter(|line| parse_test_line(line).is_none())
                .find_map(|line| infra_of(line));
            suite.failures.push(Failure {
                test: name.to_string(),
                first_line: first_line(&lines),
                infra,
            });
        }
        suite
    }
}

/// **Turn a test run's text and its exit status into the verdict.** `not_run`
/// is how many jobs the bar left out because an earlier one was red.
pub fn read(text: &str, exit_ok: bool, not_run: usize) -> Verdict {
    let parsed = parse(text);
    let counted = parsed.test_verdict();
    let mut suites: Vec<Suite> = parsed.records.iter().map(Suite::of).collect();
    // A line outside any failure's output may name a machine reason for its
    // suite, or the binary a signal ended.
    for (i, record) in parsed.records.iter().enumerate() {
        for line in &record.loose {
            let about = match crashed_binary(line) {
                Some(bin) => {
                    let at = suites
                        .iter()
                        .position(|s| s.name == bin || s.name == format!("{bin} (unit tests)"))
                        .unwrap_or_else(|| {
                            suites.push(Suite::new(bin));
                            suites.len() - 1
                        });
                    suites[at].crashed = true;
                    at
                }
                None => i,
            };
            if suites[about].infra.is_none() {
                suites[about].infra = infra_of(line);
            }
        }
    }
    // A binary that started and never reported, in a run that failed, ended
    // without a line of its own: a signal or an exit that cargo's wrapper did
    // not describe. The bundled binaries are run directly and land here.
    if !exit_ok {
        for suite in suites.iter_mut().filter(|s| !s.reported) {
            suite.crashed = true;
        }
    }
    // What the lines before the first suite said: a build that ran out of disk.
    let preamble = parsed.preamble.iter().find_map(|line| infra_of(line));
    let red: Vec<Suite> = suites.into_iter().filter(Suite::red).collect();
    let infra = red.iter().find_map(Suite::seen_infra).or(preamble);
    let code_red = red.iter().any(Suite::code_red);
    let overall = if text.trim().is_empty() && red.is_empty() {
        Overall::NoVerdict
    } else if code_red {
        Overall::Red
    } else if infra.is_some() && (!red.is_empty() || !exit_ok) {
        Overall::Infra
    } else if !exit_ok {
        Overall::Red
    } else if counted.suites == 0 {
        Overall::NoVerdict
    } else {
        Overall::Green
    };
    Verdict {
        overall,
        infra: match overall {
            Overall::Red | Overall::Infra => infra,
            _ => None,
        },
        exit_ok,
        compiled: counted.compiled,
        suites_total: counted.suites,
        passed: counted.passed,
        not_run,
        red,
        failed_phases: Vec::new(),
    }
}

/// **The file's content**, for the full head sha and the job's name.
pub fn to_json(verdict: &Verdict, head_sha: &str, job: &str) -> Value {
    json!({
        "head_sha": head_sha,
        "job": job,
        "overall": verdict.overall.word(),
        "infra": verdict.infra.map(Infra::word),
        "exit_ok": verdict.exit_ok,
        "compiled": verdict.compiled,
        "suites_total": verdict.suites_total,
        "passed": verdict.passed,
        "not_run": verdict.not_run,
        "failed_phases": verdict.failed_phases,
        "red_suites": verdict.red.iter().map(|suite| json!({
            "name": suite.name,
            "passed": suite.passed,
            "failed": suite.failed,
            "crashed": suite.crashed,
            "infra": suite.infra_class().map(Infra::word),
            "failing": suite.failures.iter().map(|f| json!({
                "test": f.test,
                "first_line": f.first_line,
                "infra": f.infra.map(Infra::word),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
    })
}

/// **Where a job's file is written**, under the directory the logs are in.
pub fn path_of(job: &str) -> String {
    format!("target/bar/verdict-{job}.json")
}

/// **Write the job's verdict file and return the one line the log ends with.**
/// The head is the full sha of the checkout, empty when git cannot say.
pub fn write(
    job: &str,
    text: &str,
    exit_ok: bool,
    not_run: usize,
    failed_phases: Vec<String>,
) -> std::io::Result<String> {
    let mut verdict = read(text, exit_ok, not_run);
    verdict.overall = overall_with(verdict.overall, &failed_phases);
    verdict.failed_phases = failed_phases;
    let head = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map(|out| String::from_utf8_lossy(&out.stdout).trim().to_string())
        .unwrap_or_default();
    let path = path_of(job);
    if let Some(dir) = Path::new(&path).parent() {
        fs::create_dir_all(dir)?;
    }
    let body = serde_json::to_string_pretty(&to_json(&verdict, &head, job))
        .map_err(std::io::Error::other)?;
    fs::write(&path, body + "\n")?;
    Ok(format!(
        "verdict file: {path} — {}, {} red suites{}",
        verdict.overall.word(),
        verdict.red.len(),
        verdict
            .infra
            .map(|class| format!(", infra: {}", class.word()))
            .unwrap_or_default()
    ))
}

/// **The phases whose failure the text of the tests already tells.** A phase
/// outside this list (the lint, the size gate) fails for a reason the text
/// cannot place on the machine.
const SPOKEN_FOR: [&str; 5] = ["build", "test-compile", "test", "doc-tests", "fast"];

/// **The job's overall outcome once its other phases are known.** Green or infra
/// tests in a job whose lint failed are a red job: the infra reason stays on the
/// file, and a code failure is not hidden behind it.
fn overall_with(overall: Overall, failed_phases: &[String]) -> Overall {
    let outside = failed_phases
        .iter()
        .any(|phase| !SPOKEN_FOR.contains(&phase.as_str()));
    match overall {
        Overall::Green | Overall::Infra if outside => Overall::Red,
        other => other,
    }
}

/// **What a job's test phase read, kept until the job ends.** The file says the
/// job's outcome, and a lint or a size gate that fails after the tests is part of
/// it, so the file is written when the verdict is printed, not when the tests end.
pub struct Input {
    job: String,
    text: String,
    exit_ok: bool,
    not_run: usize,
}

impl crate::Summary {
    /// **Keep the text a job's tests printed, to be settled at the job's end.**
    /// A later call replaces an earlier one.
    pub fn verdict_of(&mut self, job: &str, text: &str, exit_ok: bool, not_run: usize) {
        self.pending_verdict = Some(Input {
            job: job.to_string(),
            text: text.to_string(),
            exit_ok,
            not_run,
        });
    }
}

/// **Write the file of a job that kept its text, and put its line on the
/// verdict.** A file that cannot be written is said on the verdict and does not
/// change the job's own result.
pub fn settle(summary: &mut crate::Summary) {
    let Some(input) = summary.pending_verdict.take() else {
        return;
    };
    let phases = summary.failed_phases();
    match write(
        &input.job,
        &input.text,
        input.exit_ok,
        input.not_run,
        phases,
    ) {
        Ok(line) => summary.note(&line),
        Err(e) => summary.note(&format!("verdict file: not written — {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `verdict-three-reds.txt` is three job texts as CI run 1929 and the runs
    // before it printed them, less the passing-test lines. The other `verdict-`
    // files are built from the same shapes: cargo's and Rust's own wording for
    // each infra class, which no kept CI log holds.
    const THREE_REDS: &str = include_str!("../tests/fixtures/verdict-three-reds.txt");
    const GREEN: &str = include_str!("../tests/fixtures/green-full-check.txt");
    const COMPILE_ERROR: &str = include_str!("../tests/fixtures/compile-error.txt");
    const DISK_FULL: &str = include_str!("../tests/fixtures/verdict-disk-full.txt");
    const DISK_FULL_BUILD: &str = include_str!("../tests/fixtures/verdict-disk-full-build.txt");
    const KILLED: &str = include_str!("../tests/fixtures/verdict-killed-binary.txt");
    const MIXED: &str = include_str!("../tests/fixtures/verdict-mixed-suite.txt");
    const BUNDLE_KILLED: &str = include_str!("../tests/fixtures/verdict-bundle-killed.txt");
    const TERMINATED: &str = include_str!("../tests/fixtures/verdict-terminated-binary.txt");
    const OUT_OF_MEMORY: &str = include_str!("../tests/fixtures/verdict-out-of-memory.txt");
    const NETWORK: &str = include_str!("../tests/fixtures/verdict-network-fetch.txt");

    fn names(verdict: &Verdict) -> Vec<&str> {
        verdict.red.iter().map(|s| s.name.as_str()).collect()
    }

    /// **Every red suite is named, not the first.** The capture holds three red
    /// binaries; the first has 22 failing tests.
    #[test]
    fn a_run_with_three_red_suites_names_all_three_and_all_their_failures() {
        let verdict = read(THREE_REDS, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(
            names(&verdict),
            [
                "jojobot_adapters (unit tests)",
                "fixture_roster",
                "report_touched_first"
            ]
        );
        assert_eq!(verdict.red[0].failures.len(), 22, "{:?}", verdict.red[0]);
        assert_eq!(verdict.red[1].failures.len(), 1);
        assert_eq!(verdict.red[2].failures.len(), 2);
        assert!(verdict.red.iter().all(|s| s.infra.is_none()));
        assert_eq!(verdict.infra, None);
    }

    /// **Each failure carries the first line it said**: where it panicked and
    /// the message under that.
    #[test]
    fn a_failure_carries_its_panic_place_and_the_line_under_it() {
        let verdict = read(THREE_REDS, false, 0);
        let roster = &verdict.red[1].failures[0];
        assert_eq!(
            roster.test,
            "ordinary_carries_no_name_the_workspace_has_stopped_using"
        );
        assert_eq!(
            roster.first_line,
            "crates/jojobot-domain/tests/fixture_roster.rs:748:5: ordinary exceptions nothing in the workspace uses — delete them; only reviewed cast names may be kept ready for future fixtures:"
        );
        // Output that quotes a bar run, as this suite's own test does, is not
        // read as a second suite or a second verdict.
        assert!(
            verdict.red[2].failures[0]
                .first_line
                .starts_with("crates/jojobot-bar/tests/")
        );
    }

    /// **The counts of the suites that were green are not lost.**
    #[test]
    fn the_totals_count_every_suite_and_the_passed_tests_in_it() {
        let verdict = read(THREE_REDS, false, 0);
        assert_eq!(verdict.suites_total, 3);
        assert_eq!(verdict.passed, 435 + 24 + 2);
    }

    /// **A full disk in one suite beside two red suites of the code**: all three
    /// are listed, the disk one is infra, the job is red because two are not.
    #[test]
    fn a_full_disk_is_infra_beside_two_reds_of_the_code() {
        let verdict = read(DISK_FULL, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(verdict.infra, Some(Infra::DiskFull));
        let by_name: Vec<(&str, Option<Infra>)> = verdict
            .red
            .iter()
            .map(|s| (s.name.as_str(), s.infra_class()))
            .collect();
        assert_eq!(
            by_name,
            [
                ("fixture_roster", None),
                ("report_touched_first", None),
                ("user_stories", Some(Infra::DiskFull)),
            ]
        );
    }

    /// **Only infra is red: the job is infra.**
    #[test]
    fn a_run_whose_only_red_suite_ran_out_of_disk_is_infra() {
        let only_disk = &DISK_FULL[DISK_FULL.find("     Running tests/user_stories").unwrap()..];
        let verdict = read(only_disk, false, 0);
        assert_eq!(names(&verdict), ["user_stories"], "{only_disk}");
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::DiskFull));
    }

    /// **One suite, one failure of the code and one of the machine**: the job is
    /// red, the infra reason stays on the file, and the suite is not infra.
    #[test]
    fn an_infra_failure_does_not_hide_a_code_failure_in_the_same_suite() {
        let verdict = read(MIXED, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(verdict.infra, Some(Infra::DiskFull));
        assert_eq!(names(&verdict), ["user_stories"]);
        let suite = &verdict.red[0];
        assert_eq!(suite.infra_class(), None);
        let by_test: Vec<(&str, Option<Infra>)> = suite
            .failures
            .iter()
            .map(|f| (f.test.as_str(), f.infra))
            .collect();
        assert_eq!(
            by_test,
            [
                ("a_story_that_orders_a_list", None),
                ("a_story_that_writes_a_document", Some(Infra::DiskFull)),
            ]
        );
    }

    /// **A bundled binary is run directly**, so nothing but the harness's own
    /// `Running` line names it. One that stopped before its result is a red suite.
    #[test]
    fn a_binary_that_stopped_before_its_result_is_a_crashed_suite() {
        let verdict = read(BUNDLE_KILLED, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(verdict.infra, None);
        assert_eq!(names(&verdict), ["user_stories"]);
        assert!(verdict.red[0].crashed);
        // With a zero exit status the same text names nothing red.
        assert_ne!(read(BUNDLE_KILLED, true, 0).overall, Overall::Red);
    }

    /// **A binary that started and never reported has not compiled a suite.**
    /// The summary and the verdict both say so; the verdict still names the
    /// crashed suite.
    #[test]
    fn a_started_binary_with_no_result_line_is_not_a_reported_suite() {
        let counted = crate::summarize_test_output(KILLED);
        assert_eq!((counted.compiled, counted.suites), (false, 0));
        let verdict = read(KILLED, false, 0);
        assert_eq!((verdict.compiled, verdict.suites_total), (false, 0));
        assert!(verdict.red[0].crashed);
    }

    /// **A failed phase the text cannot place on the machine makes the job red,
    /// whatever the tests were.**
    #[test]
    fn a_failed_lint_makes_green_and_infra_tests_a_red_job() {
        let lint = vec!["lint".to_string()];
        let test_only = vec!["test".to_string()];
        assert_eq!(overall_with(Overall::Green, &lint), Overall::Red);
        assert_eq!(overall_with(Overall::Infra, &lint), Overall::Red);
        assert_eq!(overall_with(Overall::Infra, &test_only), Overall::Infra);
        assert_eq!(overall_with(Overall::NoVerdict, &lint), Overall::NoVerdict);
    }

    /// **The counts are the summary's own**: both read one text one way.
    #[test]
    fn the_totals_agree_with_the_summary_on_every_capture() {
        for text in [
            THREE_REDS,
            KILLED,
            GREEN,
            COMPILE_ERROR,
            MIXED,
            BUNDLE_KILLED,
        ] {
            let counted = crate::summarize_test_output(text);
            let verdict = read(text, false, 0);
            assert_eq!(
                (verdict.compiled, verdict.suites_total, verdict.passed),
                (counted.compiled, counted.suites, counted.passed)
            );
        }
    }

    /// **A binary a signal ended is red and has no `test result:` line.**
    #[test]
    fn a_killed_binary_is_a_red_suite_of_the_class_killed() {
        let verdict = read(KILLED, false, 0);
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::Killed));
        assert_eq!(names(&verdict), ["user_stories"]);
        assert!(verdict.red[0].crashed);
    }

    /// **A build that ran out of disk has no suite at all** and is still infra.
    #[test]
    fn a_build_that_ran_out_of_disk_is_infra_with_no_suite() {
        let verdict = read(DISK_FULL_BUILD, false, 0);
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::DiskFull));
        assert!(verdict.red.is_empty() && !verdict.compiled);
    }

    /// **A binary the runner terminated is killed, as one that was killed.**
    #[test]
    fn a_terminated_binary_is_a_red_suite_of_the_class_killed() {
        let verdict = read(TERMINATED, false, 0);
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::Killed));
        assert_eq!(names(&verdict), ["user_stories"]);
        assert!(verdict.red[0].crashed);
    }

    #[test]
    fn an_allocation_that_failed_is_out_of_memory() {
        let verdict = read(OUT_OF_MEMORY, false, 0);
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::OutOfMemory));
    }

    #[test]
    fn a_registry_that_cannot_be_reached_is_a_network_failure() {
        let verdict = read(NETWORK, false, 0);
        assert_eq!(verdict.overall, Overall::Infra);
        assert_eq!(verdict.infra, Some(Infra::NetworkFetch));
        assert!(verdict.red.is_empty() && !verdict.compiled);
    }

    /// **Each class has its own value**, never one for all of them.
    #[test]
    fn the_four_classes_are_four_values() {
        let classes = [DISK_FULL, KILLED, OUT_OF_MEMORY, NETWORK].map(|t| read(t, false, 0).infra);
        assert_eq!(
            classes,
            [
                Some(Infra::DiskFull),
                Some(Infra::Killed),
                Some(Infra::OutOfMemory),
                Some(Infra::NetworkFetch)
            ]
        );
    }

    /// **A clean run is green, and a warning about the network in a run that
    /// went on to pass is not a failure.**
    #[test]
    fn a_clean_run_is_green_and_names_no_red_suite() {
        let verdict = read(GREEN, true, 0);
        assert_eq!(verdict.overall, Overall::Green);
        assert_eq!(verdict.infra, None);
        assert!(verdict.red.is_empty());
        let with_warning = format!(
            "warning: spurious network error (2 tries remaining): [6] Could not resolve host: index.crates.io\n{GREEN}"
        );
        let verdict = read(&with_warning, true, 0);
        assert_eq!(verdict.overall, Overall::Green);
        assert_eq!(verdict.infra, None);
    }

    /// **A red whose suite printed no `Running` line is still listed.** The
    /// capture is a `cargo test` run as the bar's own summary tests keep it.
    #[test]
    fn a_red_with_no_running_line_before_it_is_listed_and_not_dropped() {
        let text = include_str!("../tests/fixtures/timed-failures.txt");
        let verdict = read(text, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(verdict.red.len(), 1, "{:?}", verdict.red);
        assert_eq!(verdict.red[0].failed, 13);
        assert_eq!(verdict.red[0].failures.len(), 13);
    }

    /// **Every job that runs tests uploads the file the bar writes.** The path
    /// the bar writes comes from `path_of`; the workflows spell it as a glob.
    #[test]
    fn the_test_jobs_of_both_workflows_upload_the_verdict_files() {
        assert!(path_of("report").starts_with("target/bar/verdict-"));
        assert!(path_of("report").ends_with(".json"));
        for (workflow, uploads) in [
            (
                include_str!("../../../.forgejo/workflows/branch.yml"),
                ["bar-logs-report", "bar-logs-rooms"].as_slice(),
            ),
            (
                include_str!("../../../.forgejo/workflows/pull-request.yml"),
                ["bar-logs-check", "bar-logs-rooms", "bar-logs-year"].as_slice(),
            ),
        ] {
            for upload in uploads {
                let step = workflow
                    .split(&format!("name: {upload}\n"))
                    .nth(1)
                    .unwrap_or_else(|| panic!("no upload named {upload}"));
                let path = step.split("retention-days").next().unwrap_or_default();
                assert!(
                    path.contains("target/bar/verdict-*.json"),
                    "{upload}: {path}"
                );
            }
        }
    }

    /// **A log with nothing in it proves nothing.**
    #[test]
    fn an_empty_log_is_no_verdict_whatever_the_exit_status() {
        assert_eq!(read("", true, 0).overall, Overall::NoVerdict);
        assert_eq!(read("", false, 0).overall, Overall::NoVerdict);
        assert_eq!(read("\n\n", true, 0).overall, Overall::NoVerdict);
    }

    /// **A build that did not compile is red, and not infra.**
    #[test]
    fn a_compile_error_is_red_with_no_suite_and_no_class() {
        let verdict = read(COMPILE_ERROR, false, 0);
        assert_eq!(verdict.overall, Overall::Red);
        assert_eq!(verdict.infra, None);
        assert!(!verdict.compiled && verdict.red.is_empty());
    }

    /// **An exit status the text does not explain is red, not green.**
    #[test]
    fn a_non_zero_exit_with_every_suite_green_is_red() {
        assert_eq!(read(GREEN, false, 0).overall, Overall::Red);
    }

    /// **A test that quotes an infra phrase while it passes is not infra.**
    #[test]
    fn the_name_of_a_passing_test_is_not_a_reason() {
        let text = "     Running tests/a.rs (/x/deps/a-0123abcd)\n\
                    test a_boot_killed_by_signal: 9 (signal: 9, SIGKILL) ... ok <0.1s>\n\
                    test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.1s\n";
        assert_eq!(read(text, true, 0).overall, Overall::Green);
    }

    /// **The file says the full sha, the job and what the run left out.**
    #[test]
    fn the_file_holds_the_head_the_job_and_the_jobs_not_run() {
        let mut verdict = read(THREE_REDS, false, 76);
        verdict.failed_phases = vec!["lint".to_string()];
        let value = to_json(&verdict, "0123456789abcdef", "report");
        assert_eq!(value["failed_phases"][0], "lint");
        assert_eq!(value["head_sha"], "0123456789abcdef");
        assert_eq!(value["job"], "report");
        assert_eq!(value["overall"], "red");
        assert_eq!(value["not_run"], 76);
        assert_eq!(value["red_suites"].as_array().map(Vec::len), Some(3));
        assert_eq!(value["infra"], Value::Null);
    }
}
