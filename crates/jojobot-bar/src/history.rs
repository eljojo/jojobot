//! **What one job's text says about each test in it.**
//!
//! `cargo test` prints one line per test: `test <path> ... ok`, `... FAILED` or
//! `... ignored`. With the harness's report-time flag each line ends in the
//! test's own duration, `<0.013s>`. This reads those lines and nothing else:
//! pure text in, records out, so the file that later stores a run's history
//! holds no parsing of its own.
//!
//! A test's name is unique within one job on this workspace, so the records
//! are not split by test binary.

/// **How a test ended**, as the line says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Ok,
    Failed,
    Ignored,
}

/// **One test's line.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestRecord {
    /// The full path, exactly as `cargo test` printed it.
    pub test: String,
    pub verdict: Verdict,
    /// **How long the test ran, in milliseconds**, when the line says. Stable
    /// `cargo test` prints no such time; the harness prints one when it is run
    /// with `-Zunstable-options --report-time`. `None` is "the line did not
    /// say", never zero.
    pub duration_ms: Option<u64>,
}

/// **What a job's text says**: a record per test line, in the order printed,
/// and how long the job's suites say they took.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct JobRecord {
    pub tests: Vec<TestRecord>,
    /// **The sum of the `finished in` times on the job's `test result:`
    /// lines**, in milliseconds. A job that printed no such line has `None`.
    pub duration_ms: Option<u64>,
}

/// **Read one job's text** (the stdout and stderr of its `cargo test`).
pub fn parse_job(text: &str) -> JobRecord {
    let mut job = JobRecord::default();
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("test result: ") {
            if let Some(ms) = finished_in_ms(rest) {
                job.duration_ms = Some(job.duration_ms.unwrap_or(0) + ms);
            }
        } else if let Some(record) = parse_test_line(line) {
            job.tests.push(record);
        }
    }
    job
}

/// **`test a::b ... ok <0.013s>` → a record.** Any other line, a benchmark
/// result included, is not a test line and gives `None`.
fn parse_test_line(line: &str) -> Option<TestRecord> {
    let (test, outcome) = line.strip_prefix("test ")?.split_once(" ... ")?;
    let (outcome, duration_ms) = match outcome.rsplit_once(" <") {
        Some((before, time)) => match time.strip_suffix("s>") {
            Some(seconds) => (before, seconds_to_ms(seconds)),
            None => (outcome, None),
        },
        None => (outcome, None),
    };
    let verdict = match outcome {
        "ok" => Verdict::Ok,
        "FAILED" => Verdict::Failed,
        other if other.starts_with("ignored") => Verdict::Ignored,
        _ => return None,
    };
    Some(TestRecord {
        test: test.to_string(),
        verdict,
        duration_ms,
    })
}

/// `ok. 346 passed; ...; finished in 12.84s` → 12840.
fn finished_in_ms(result: &str) -> Option<u64> {
    let seconds = result.rsplit_once("finished in ")?.1.strip_suffix('s')?;
    seconds_to_ms(seconds)
}

fn seconds_to_ms(seconds: &str) -> Option<u64> {
    let seconds: f64 = seconds.trim().parse().ok()?;
    (seconds >= 0.0).then(|| (seconds * 1000.0).round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::summarize_test_output;

    /// Verbatim captures of real runs of this workspace's bar.
    const GREEN: &str = include_str!("../tests/fixtures/green-full-check.txt");
    const ONE_FAILING: &str = include_str!("../tests/fixtures/one-failing-test.txt");
    /// `cargo test -p jojobot-domain --lib` with the harness's report-time flag;
    /// the run has one ignored test, and its line carries a reason.
    const WITH_TIMES: &str = include_str!("../tests/fixtures/report-time-with-ignored.txt");
    /// A run of this crate's own tests with report-time, twelve of them failing.
    const FAILING_WITH_TIMES: &str =
        include_str!("../tests/fixtures/report-time-with-failures.txt");

    fn find<'a>(job: &'a JobRecord, test: &str) -> &'a TestRecord {
        job.tests
            .iter()
            .find(|record| record.test == test)
            .unwrap_or_else(|| panic!("{test} is not among the records"))
    }

    /// **A run on stable has a record per test the suite counted, none of them
    /// timed.** The count is the one the `test result:` line carries, read by
    /// the bar's other parser, so the two readers must agree. The fixture is
    /// cut short after its first suites, so only the first suite, which keeps
    /// its own result line, is compared.
    #[test]
    fn a_stable_run_has_a_record_per_counted_test_and_no_times() {
        let result = GREEN.find("test result:").expect("a suite result");
        let end = result + GREEN[result..].find('\n').expect("the line ends");
        let first_suite = &GREEN[..end];
        let job = parse_job(first_suite);
        let counted = summarize_test_output(first_suite);
        assert_eq!(counted.passed, 58, "the suite this case is about");
        assert_eq!(
            job.tests.len(),
            counted.passed,
            "{} records",
            job.tests.len()
        );
        assert!(job.tests.iter().all(|r| r.verdict == Verdict::Ok));
        assert!(job.tests.iter().all(|r| r.duration_ms.is_none()));
        assert_eq!(job.duration_ms, Some(26_270));
    }

    /// **A failing test is recorded as failed and the rest as ok**, in the
    /// order the run printed them.
    #[test]
    fn a_failing_test_is_recorded_as_failed_and_only_it() {
        let job = parse_job(ONE_FAILING);
        let name = "attention::tests::a_rhythm_is_overdue_from_the_day_it_falls_due";
        let failed: Vec<&TestRecord> = job
            .tests
            .iter()
            .filter(|r| r.verdict == Verdict::Failed)
            .collect();
        assert_eq!(failed.len(), 1, "{failed:?}");
        assert_eq!(failed[0].test, name);
        assert_eq!(job.tests.len(), 259, "258 ok beside the one that failed");
    }

    /// **A timed line gives its milliseconds, an ignored test keeps its name
    /// and carries no time.** The ignored line in this capture has a reason
    /// after the word, and the reason is not part of the name.
    #[test]
    fn a_timed_line_gives_its_milliseconds_and_an_ignored_test_none() {
        let job = parse_job(WITH_TIMES);
        assert_eq!(job.tests.len(), 347, "346 ok and the ignored one");
        assert_eq!(
            find(
                &job,
                "attention::tests::a_consuming_check_in_refuses_a_snooze_day"
            ),
            &TestRecord {
                test: "attention::tests::a_consuming_check_in_refuses_a_snooze_day".into(),
                verdict: Verdict::Ok,
                duration_ms: Some(2),
            }
        );
        assert_eq!(
            find(&job, "attention::tests::a_blank_snooze_day_is_no_snooze").duration_ms,
            Some(5)
        );
        assert_eq!(
            find(
                &job,
                "memory::tests::fake_does_not_owe_a_person_for_a_deadline_on_one_claim"
            ),
            &TestRecord {
                test: "memory::tests::fake_does_not_owe_a_person_for_a_deadline_on_one_claim"
                    .into(),
                verdict: Verdict::Ignored,
                duration_ms: None,
            }
        );
        assert_eq!(job.duration_ms, Some(12_840));
    }

    /// **A failing line carries its time too**, and the line is a failed test
    /// rather than an unreadable one: the bar's own `... FAILED` reader does
    /// not see this shape, and this one must.
    #[test]
    fn a_failing_timed_line_is_a_failed_test_with_its_time() {
        let job = parse_job(FAILING_WITH_TIMES);
        let failed = job
            .tests
            .iter()
            .filter(|r| r.verdict == Verdict::Failed)
            .count();
        assert_eq!(failed, 12, "the suite line says twelve failed");
        assert_eq!(
            find(
                &job,
                "tests::a_failed_tests_name_is_readable_from_the_summary_alone"
            ),
            &TestRecord {
                test: "tests::a_failed_tests_name_is_readable_from_the_summary_alone".into(),
                verdict: Verdict::Failed,
                duration_ms: Some(1),
            }
        );
        assert_eq!(job.tests.len(), 37, "twelve failed and twenty-five ok");
    }

    /// **Lines that are not a test's line make no record**: the suite line,
    /// the list of failures a failing run prints after the tests, and a
    /// benchmark.
    #[test]
    fn a_line_that_is_not_a_test_result_makes_no_record() {
        let text = "running 1 test\n\
                    test a::b ... bench:        1,000 ns/iter (+/- 10)\n\
                    test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; \
                    finished in 0.00s\n\
                    failures:\n    a::b\n";
        let job = parse_job(text);
        assert!(job.tests.is_empty(), "{job:?}");
        assert_eq!(job.duration_ms, Some(0));
        assert_eq!(parse_job("").duration_ms, None, "no suite line, no time");
    }
}
