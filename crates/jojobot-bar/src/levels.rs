//! **Which jobs may leave the report level, from what the history says.**
//!
//! The protecting half comes first: a job leaves only with evidence, and the
//! lack of evidence keeps it. A job with too few recorded runs, or with a red in
//! its recent runs that nobody has called a flake, stays in every level. A red
//! counts only when `main` contains the head its run recorded; a head that is
//! missing or cannot be told counts too. Nothing
//! here switches a job off; the report level is a subset of the boundary level
//! and the boundary level runs everything.

use crate::reading::{History, JobRun, RecordedCall};
use crate::recording::job_name;

/// **How many recorded runs a job needs before it can leave the report level.**
pub const MIN_RUNS: usize = 5;

/// **How many of a job's newest runs are searched for a red that holds it.**
pub const DEFECT_WINDOW: usize = 20;

/// **Where a job stands on its history alone**, before its cost is looked at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Standing {
    /// Enough runs and nothing in the newest runs holds it.
    Eligible,
    /// Fewer than [`MIN_RUNS`] recorded runs.
    TooFewRuns { have: usize },
    /// A red in the newest runs is called a defect.
    DefectCalled,
    /// A red in the newest runs is not called a flake: nobody has judged it, so
    /// it may be a defect.
    RedNotCalledFlake,
}

/// **The call that applies to one test's red in one run**: the newest call
/// naming the test and either that run or, when the call names no run, that
/// run's commit.
fn call_for<'a>(run: &JobRun, test: &str, calls: &'a [RecordedCall]) -> Option<&'a RecordedCall> {
    calls
        .iter()
        .filter(|call| call.test == test)
        .filter(|call| match &call.run_id {
            Some(id) => id == &run.run_id,
            None => call.head.is_some() && call.head == run.head,
        })
        .max_by(|a, b| a.at.cmp(&b.at))
}

/// **Whether a run's red counts against its job.** A red on a head `main` does
/// not contain is work in progress on another line, and it says nothing about
/// whether `main` is safe to pass. A run that recorded no head, or whose head
/// cannot be told, counts: the bar errs toward running more.
fn counts(run: &JobRun, ancestry: Ancestry) -> bool {
    match run.head.as_deref() {
        None => true,
        Some(head) => ancestry(head) != Some(false),
    }
}

/// **How many red runs in a job's newest window were ignored** because their
/// head is not in `main`.
pub fn ignored_reds(runs: &[JobRun], ancestry: Ancestry) -> usize {
    runs[runs.len().saturating_sub(DEFECT_WINDOW)..]
        .iter()
        .filter(|run| !run.ok || !run.failed.is_empty())
        .filter(|run| !counts(run, ancestry))
        .count()
}

/// **Where a job stands**, given its recorded runs oldest first and every call.
pub fn standing(runs: &[JobRun], calls: &[RecordedCall], ancestry: Ancestry) -> Standing {
    if runs.len() < MIN_RUNS {
        return Standing::TooFewRuns { have: runs.len() };
    }
    let mut uncalled = false;
    for run in &runs[runs.len().saturating_sub(DEFECT_WINDOW)..] {
        // A job that failed without naming a test (a crashed binary) has a red
        // that no call can name, so it is never called a flake.
        let reds: Vec<Option<&str>> = if !run.ok && run.failed.is_empty() {
            vec![None]
        } else {
            run.failed.iter().map(|test| Some(test.as_str())).collect()
        };
        if !reds.is_empty() && !counts(run, ancestry) {
            continue;
        }
        for red in reds {
            match red.and_then(|test| call_for(run, test, calls)) {
                Some(call) if call.call == "defect" => return Standing::DefectCalled,
                Some(call) if call.call == "flake" => {}
                _ => uncalled = true,
            }
        }
    }
    if uncalled {
        Standing::RedNotCalledFlake
    } else {
        Standing::Eligible
    }
}

/// **How costly a job's runs must be, at the median, to leave the report
/// level**: thirty seconds, load-discounted. At the threshold counts as costly.
pub const LEAVE_AT_MS: f64 = 30_000.0;

/// **One run's duration, discounted for the load it ran under.** A run that
/// shared a machine with more work than it had CPUs for ran slower than it
/// costs: the duration is scaled by the share of the machine it had,
/// `min(1, cpus / max(load, 1))`, with the CPU count that run recorded. A run
/// that recorded no CPU count, or no load, is not discounted. `None` when the
/// run recorded no duration.
pub fn discounted_ms(run: &JobRun) -> Option<f64> {
    let duration = run.duration_ms? as f64;
    Some(match (run.cpus, run.load_average) {
        (Some(cpus), Some(load)) => duration * (cpus as f64 / load.max(1.0)).min(1.0),
        _ => duration,
    })
}

/// **The median of a job's discounted durations**, over the runs that recorded
/// one; the mean of the middle two when there is an even number. `None` when no
/// run recorded a duration.
pub fn median_cost_ms(runs: &[JobRun]) -> Option<f64> {
    let mut costs: Vec<f64> = runs.iter().filter_map(discounted_ms).collect();
    costs.sort_by(f64::total_cmp);
    let middle = costs.len() / 2;
    match costs.len() {
        0 => None,
        n if n % 2 == 1 => Some(costs[middle]),
        _ => Some((costs[middle - 1] + costs[middle]) / 2.0),
    }
}

/// **Whether a job's runs cost enough to leave the report level.** A job with
/// no recorded duration has no evidence of cost and is not slow.
pub fn is_slow(runs: &[JobRun]) -> bool {
    median_cost_ms(runs).is_some_and(|ms| ms >= LEAVE_AT_MS)
}

/// **The jobs that run at every level whatever their history**, by job name.
/// The upgrade gate meets a deployed store with new code, and no run of a job
/// that did not meet it says anything about the next one.
pub const ALWAYS_RUN: &[&str] = &["test-upgrade_gate"];

/// **A job the report level leaves out, and the standing that allowed it.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LeftOut {
    pub job: String,
}

/// **What the report level runs and what it leaves to the boundary.** The jobs
/// that run keep the order they were listed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub run: Vec<Vec<String>>,
    pub left_out: Vec<LeftOut>,
    /// The red job runs in the newest windows that were ignored because their
    /// head is not in `main`.
    pub ignored: usize,
}

/// **Whether a recorded head is contained in `main`**: `Some(true)` when it is,
/// `Some(false)` when `main` does not contain it, `None` when it cannot be told.
pub type Ancestry<'a> = &'a dyn Fn(&str) -> Option<bool>;

/// **What `git merge-base --is-ancestor <head> main` answered**, from its exit
/// status: zero is contained, one is not, and anything else (an unknown commit
/// exits 128) is a head that cannot be told. A process that ended without a
/// status cannot be told either.
pub fn contained_from_exit(code: Option<i32>) -> Option<bool> {
    match code {
        Some(0) => Some(true),
        Some(1) => Some(false),
        _ => None,
    }
}

/// **Whether a recorded head is safe to hand to git as a revision.** A head
/// comes from a file in the history directory, and one that starts with a dash
/// would be read as an option.
pub fn is_a_revision(head: &str) -> bool {
    !head.is_empty() && !head.starts_with('-')
}

/// **The sentence the report's level line ends with.**
pub fn report_line(plan: &Plan, total: usize) -> String {
    let left: Vec<&str> = plan.left_out.iter().map(|l| l.job.as_str()).collect();
    format!(
        "report, {} of {total} jobs, left to the boundary: {}; ignored {} reds on heads main \
         does not contain",
        plan.run.len(),
        if left.is_empty() {
            "none".to_string()
        } else {
            left.join(", ")
        },
        plan.ignored,
    )
}

/// **The report level's jobs**, from the listed jobs and the history. A job
/// leaves only when it is not in [`ALWAYS_RUN`], its history makes it
/// [`Standing::Eligible`], and `slow` says its runs are costly enough to be
/// worth leaving. With no history nothing is eligible, so the report level is
/// the boundary level.
pub fn report_plan(
    jobs: &[Vec<String>],
    history: &History,
    slow: &dyn Fn(&[JobRun]) -> bool,
    ancestry: Ancestry,
) -> Plan {
    let mut plan = Plan {
        run: Vec::new(),
        left_out: Vec::new(),
        ignored: 0,
    };
    for job in jobs {
        let name = job_name(job);
        let runs = history.jobs.get(&name).map_or(&[][..], Vec::as_slice);
        plan.ignored += ignored_reds(runs, ancestry);
        let leaves = !ALWAYS_RUN.contains(&name.as_str())
            && standing(runs, &history.calls, ancestry) == Standing::Eligible
            && slow(runs);
        if leaves {
            plan.left_out.push(LeftOut { job: name });
        } else {
            plan.run.push(job.clone());
        }
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn everything_in_main(_: &str) -> Option<bool> {
        Some(true)
    }

    /// An ancestry answer from a list of heads; a head not on the list cannot
    /// be resolved.
    fn ancestry_of<'a>(known: &'a [(&'a str, bool)]) -> impl Fn(&str) -> Option<bool> + 'a {
        move |head| {
            known
                .iter()
                .find(|(named, _)| *named == head)
                .map(|(_, contained)| *contained)
        }
    }

    fn run(n: usize, failed: &[&str]) -> JobRun {
        JobRun {
            run_id: format!("r{n:03}"),
            head: Some(format!("h{n:03}")),
            load_average: None,
            cpus: None,
            ok: failed.is_empty(),
            duration_ms: None,
            failed: failed.iter().map(|test| test.to_string()).collect(),
        }
    }

    fn clean(count: usize) -> Vec<JobRun> {
        (0..count).map(|n| run(n, &[])).collect()
    }

    fn call(
        test: &str,
        word: &str,
        run: Option<&str>,
        head: Option<&str>,
        at: &str,
    ) -> RecordedCall {
        RecordedCall {
            test: test.into(),
            call: word.into(),
            run_id: run.map(str::to_string),
            head: head.map(str::to_string),
            at: at.into(),
        }
    }

    /// **A job needs five recorded runs before it can leave**, and with fewer it
    /// says how many it has. Four clean runs are not enough; five are.
    #[test]
    fn a_job_needs_five_recorded_runs() {
        assert_eq!(
            standing(&[], &[], &everything_in_main),
            Standing::TooFewRuns { have: 0 }
        );
        assert_eq!(
            standing(&clean(4), &[], &everything_in_main),
            Standing::TooFewRuns { have: 4 }
        );
        assert_eq!(
            standing(&clean(5), &[], &everything_in_main),
            Standing::Eligible
        );
    }

    /// **A red called a defect holds the job for the newest twenty runs and no
    /// longer.** The same red is out of the window when twenty more runs follow.
    #[test]
    fn a_defect_holds_a_job_for_twenty_runs() {
        let calls = [call(
            "a::b",
            "defect",
            Some("r000"),
            None,
            "2026-10-08T00:00:00Z",
        )];
        let mut runs = vec![run(0, &["a::b"])];
        runs.extend((1..20).map(|n| run(n, &[])));
        assert_eq!(runs.len(), 20);
        assert_eq!(
            standing(&runs, &calls, &everything_in_main),
            Standing::DefectCalled,
            "the red is the oldest of twenty"
        );
        runs.push(run(20, &[]));
        assert_eq!(
            standing(&runs, &calls, &everything_in_main),
            Standing::Eligible,
            "twenty-one runs push it out"
        );
    }

    /// **A red nobody has called holds the job, and a red called a flake does
    /// not.** The call can name the run or, without a run, the commit.
    #[test]
    fn an_uncalled_red_holds_a_job_and_a_flake_does_not() {
        let mut runs = clean(6);
        runs[3] = run(3, &["a::b"]);
        assert_eq!(
            standing(&runs, &[], &everything_in_main),
            Standing::RedNotCalledFlake
        );
        let by_run = [call(
            "a::b",
            "flake",
            Some("r003"),
            None,
            "2026-10-08T00:00:00Z",
        )];
        assert_eq!(
            standing(&runs, &by_run, &everything_in_main),
            Standing::Eligible
        );
        let by_head = [call(
            "a::b",
            "flake",
            None,
            Some("h003"),
            "2026-10-08T00:00:00Z",
        )];
        assert_eq!(
            standing(&runs, &by_head, &everything_in_main),
            Standing::Eligible
        );
        let other_test = [call(
            "a::c",
            "flake",
            Some("r003"),
            None,
            "2026-10-08T00:00:00Z",
        )];
        assert_eq!(
            standing(&runs, &other_test, &everything_in_main),
            Standing::RedNotCalledFlake,
            "a call is about one test"
        );
        let other_run = [call(
            "a::b",
            "flake",
            Some("r004"),
            None,
            "2026-10-08T00:00:00Z",
        )];
        assert_eq!(
            standing(&runs, &other_run, &everything_in_main),
            Standing::RedNotCalledFlake,
            "and about one run"
        );
    }

    /// **A call that names neither a run nor a commit is about nothing**, even
    /// for a run that recorded no commit.
    #[test]
    fn a_call_naming_neither_run_nor_commit_applies_to_no_run() {
        let mut runs = clean(6);
        runs[2] = JobRun {
            head: None,
            ..run(2, &["a::b"])
        };
        let unplaced = [call("a::b", "flake", None, None, "2026-10-08T00:00:00Z")];
        assert_eq!(
            standing(&runs, &unplaced, &everything_in_main),
            Standing::RedNotCalledFlake
        );
    }

    /// **The newest call on a red decides.** A flake that is later called a
    /// defect holds the job; a defect later called a flake does not.
    #[test]
    fn the_newest_call_on_a_red_decides() {
        let mut runs = clean(6);
        runs[2] = run(2, &["a::b"]);
        let flake_then_defect = [
            call("a::b", "flake", Some("r002"), None, "2026-10-08T01:00:00Z"),
            call("a::b", "defect", Some("r002"), None, "2026-10-08T02:00:00Z"),
        ];
        assert_eq!(
            standing(&runs, &flake_then_defect, &everything_in_main),
            Standing::DefectCalled
        );
        let defect_then_flake = [
            call("a::b", "defect", Some("r002"), None, "2026-10-08T01:00:00Z"),
            call("a::b", "flake", Some("r002"), None, "2026-10-08T02:00:00Z"),
        ];
        assert_eq!(
            standing(&runs, &defect_then_flake, &everything_in_main),
            Standing::Eligible
        );
    }

    /// **A job that failed without naming a test holds itself.** A crashed
    /// binary prints no failing name, so no call can name its red.
    #[test]
    fn a_crash_with_no_test_name_holds_the_job() {
        let mut runs = clean(6);
        runs[1] = JobRun {
            ok: false,
            ..run(1, &[])
        };
        assert_eq!(
            standing(&runs, &[], &everything_in_main),
            Standing::RedNotCalledFlake
        );
    }

    fn job(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|part| part.to_string()).collect()
    }

    fn listed() -> Vec<Vec<String>> {
        vec![
            job(&["--lib"]),
            job(&["--test", "alpha"]),
            job(&["--test", "beta"]),
            job(&["--test", "upgrade_gate"]),
            job(&["--doc"]),
        ]
    }

    fn history_of(names: &[(&str, usize)]) -> History {
        let mut history = History::default();
        for (name, count) in names {
            history.jobs.insert(name.to_string(), clean(*count));
        }
        history
    }

    /// **With no history the report level is the boundary level**, whatever the
    /// cost rule says: nothing has the runs to leave with. The protecting half.
    #[test]
    fn with_no_history_the_report_level_runs_every_job() {
        let plan = report_plan(
            &listed(),
            &History::default(),
            &|_| true,
            &everything_in_main,
        );
        assert_eq!(plan.run, listed());
        assert!(plan.left_out.is_empty());
    }

    /// **A seeded history moves exactly the jobs the rule names**: the one with
    /// five clean runs and a cost leaves, the one with four does not, the one
    /// with a recent uncalled red does not, and the one that is not slow does
    /// not. The paired positive is that the job named does leave.
    #[test]
    fn a_seeded_history_moves_exactly_the_jobs_the_rule_names() {
        let mut history = history_of(&[("test-alpha", 5), ("lib", 4), ("doc", 9)]);
        let mut red = clean(7);
        red[5] = run(5, &["a::b"]);
        history.jobs.insert("test-beta".into(), red);
        let slow_only_alpha = |runs: &[JobRun]| runs.len() == 5;
        let plan = report_plan(&listed(), &history, &slow_only_alpha, &everything_in_main);
        let left: Vec<&str> = plan.left_out.iter().map(|l| l.job.as_str()).collect();
        assert_eq!(
            left,
            ["test-alpha"],
            "doc has the runs and no red but is not slow"
        );
        assert_eq!(
            plan.run,
            vec![
                job(&["--lib"]),
                job(&["--test", "beta"]),
                job(&["--test", "upgrade_gate"]),
                job(&["--doc"]),
            ],
            "the others run, in the order they were listed"
        );
    }

    /// **The upgrade gate runs at every level whatever its history.** It has the
    /// runs, no red and a cost, and still stays.
    #[test]
    fn the_upgrade_gate_stays_whatever_its_history() {
        let history = history_of(&[("test-upgrade_gate", 30)]);
        let plan = report_plan(&listed(), &history, &|_| true, &everything_in_main);
        assert!(plan.run.contains(&job(&["--test", "upgrade_gate"])));
        assert!(plan.left_out.is_empty());
    }

    fn timed(n: usize, ms: u64, cpus: Option<u64>, load: Option<f64>) -> JobRun {
        JobRun {
            duration_ms: Some(ms),
            cpus,
            load_average: load,
            ..run(n, &[])
        }
    }

    fn all_taking(count: usize, ms: u64) -> Vec<JobRun> {
        (0..count).map(|n| timed(n, ms, None, None)).collect()
    }

    /// **The threshold is thirty seconds and the edge is costly.** A median of
    /// exactly 30 s leaves; a millisecond under does not.
    #[test]
    fn the_threshold_is_thirty_seconds_and_the_edge_is_costly() {
        assert!(is_slow(&all_taking(5, 30_000)));
        assert!(!is_slow(&all_taking(5, 29_999)));
    }

    /// **A run under more load than it had CPUs for is discounted by its own
    /// share.** The same raw duration is slow on a machine with room and not on
    /// a loaded one: 100 s at a load of 100 on 16 CPUs is 16 s.
    #[test]
    fn a_run_is_discounted_by_the_share_of_the_machine_it_had() {
        let loaded = timed(0, 100_000, Some(16), Some(100.0));
        assert_eq!(discounted_ms(&loaded), Some(16_000.0));
        let roomy = timed(0, 100_000, Some(16), Some(8.0));
        assert_eq!(
            discounted_ms(&roomy),
            Some(100_000.0),
            "load under the CPUs is no discount"
        );
        let idle = timed(0, 100_000, Some(16), Some(0.2));
        assert_eq!(
            discounted_ms(&idle),
            Some(100_000.0),
            "load under one is read as one"
        );
        let five: Vec<JobRun> = (0..5)
            .map(|n| timed(n, 100_000, Some(16), Some(100.0)))
            .collect();
        assert!(!is_slow(&five), "100 s raw, 16 s discounted");
    }

    /// **A run recorded before the CPU count existed is not discounted.** Nor is
    /// one that recorded no load.
    #[test]
    fn a_run_without_a_cpu_count_or_a_load_is_not_discounted() {
        assert_eq!(
            discounted_ms(&timed(0, 100_000, None, Some(100.0))),
            Some(100_000.0)
        );
        assert_eq!(
            discounted_ms(&timed(0, 100_000, Some(16), None)),
            Some(100_000.0)
        );
        assert_eq!(discounted_ms(&run(0, &[])), None, "no duration, no cost");
    }

    /// **The median, not the mean, and the middle two averaged when even.** Two
    /// slow runs among five do not make a job slow; two of four at the edge do.
    #[test]
    fn the_cost_is_the_median_of_the_discounted_durations() {
        let mut runs = all_taking(3, 10_000);
        runs.extend((3..5).map(|n| timed(n, 100_000, None, None)));
        assert_eq!(median_cost_ms(&runs), Some(10_000.0));
        let even = [timed(0, 20_000, None, None), timed(1, 40_000, None, None)];
        assert_eq!(median_cost_ms(&even), Some(30_000.0));
        assert_eq!(median_cost_ms(&clean(5)), None, "no duration was recorded");
        assert!(!is_slow(&clean(5)));
    }

    /// **A red nobody called a flake keeps a costly job in the report level, and
    /// calling it a flake lets the job go.** Costly and with enough runs in both.
    #[test]
    fn an_uncalled_red_keeps_a_costly_job_in_until_it_is_called_a_flake() {
        let mut runs = all_taking(6, 40_000);
        runs[2] = JobRun {
            ok: false,
            failed: vec!["a::b".into()],
            ..runs[2].clone()
        };
        let mut history = History::default();
        history.jobs.insert("test-alpha".into(), runs);
        let jobs = vec![job(&["--test", "alpha"])];
        let held = report_plan(&jobs, &history, &is_slow, &everything_in_main);
        assert_eq!(held.run, jobs, "an uncalled red holds it");
        history.calls.push(call(
            "a::b",
            "flake",
            Some("r002"),
            None,
            "2026-10-08T00:00:00Z",
        ));
        let freed = report_plan(&jobs, &history, &is_slow, &everything_in_main);
        assert!(freed.run.is_empty());
        assert_eq!(freed.left_out.len(), 1);
    }

    fn six_runs_with_a_red_at_three() -> Vec<JobRun> {
        let mut runs = clean(6);
        runs[3] = run(3, &["a::b"]);
        runs
    }

    /// **A red on a head `main` contains counts against the job.**
    #[test]
    fn a_red_on_a_head_main_contains_holds_the_job() {
        let known = [("h003", true)];
        assert_eq!(
            standing(&six_runs_with_a_red_at_three(), &[], &ancestry_of(&known)),
            Standing::RedNotCalledFlake
        );
    }

    /// **A red on a head `main` does not contain is ignored for leaving**, so the
    /// job is eligible, and a plan with a cost lets it go and counts the red it
    /// ignored.
    #[test]
    fn a_red_on_a_head_main_lacks_does_not_hold_the_job() {
        let known = [("h003", false)];
        let runs = six_runs_with_a_red_at_three();
        assert_eq!(
            standing(&runs, &[], &ancestry_of(&known)),
            Standing::Eligible
        );

        let mut history = History::default();
        history.jobs.insert("test-alpha".into(), runs);
        let jobs = vec![job(&["--test", "alpha"])];
        let plan = report_plan(&jobs, &history, &|_| true, &ancestry_of(&known));
        assert_eq!(plan.left_out.len(), 1);
        assert_eq!(plan.ignored, 1);
        assert!(plan.run.is_empty());
    }

    /// **The exit status of the ancestry question maps to contained, not
    /// contained, or cannot be told**, and a head that would be read as an option
    /// is never handed to git.
    #[test]
    fn git_exit_statuses_and_unsafe_heads_are_read_for_what_they_are() {
        assert_eq!(contained_from_exit(Some(0)), Some(true));
        assert_eq!(contained_from_exit(Some(1)), Some(false));
        assert_eq!(contained_from_exit(Some(128)), None, "an unknown commit");
        assert_eq!(contained_from_exit(None), None, "no status");
        assert!(is_a_revision("0adc2869"));
        assert!(!is_a_revision(""));
        assert!(!is_a_revision("--output=/tmp/x"));
    }

    /// **A head that cannot be resolved counts, and so does a run that recorded
    /// no head.** The bar errs toward running more. The run with no head is not
    /// looked up at all.
    #[test]
    fn an_unknown_head_and_a_run_with_no_head_hold_the_job() {
        let nothing_known: [(&str, bool); 0] = [];
        assert_eq!(
            standing(
                &six_runs_with_a_red_at_three(),
                &[],
                &ancestry_of(&nothing_known)
            ),
            Standing::RedNotCalledFlake,
            "git could not place the head"
        );

        let mut runs = clean(6);
        runs[3] = JobRun {
            head: None,
            ..run(3, &["a::b"])
        };
        let never_asked =
            |_: &str| -> Option<bool> { panic!("a run with no head is not looked up") };
        assert_eq!(
            standing(&runs, &[], &never_asked),
            Standing::RedNotCalledFlake
        );
    }

    /// **A defect called on a head `main` lacks is ignored too**: the red is not
    /// on the line of work the report level protects.
    #[test]
    fn a_defect_called_on_a_head_main_lacks_is_ignored() {
        let known = [("h003", false)];
        let calls = [call(
            "a::b",
            "defect",
            Some("r003"),
            None,
            "2026-10-08T00:00:00Z",
        )];
        assert_eq!(
            standing(
                &six_runs_with_a_red_at_three(),
                &calls,
                &ancestry_of(&known)
            ),
            Standing::Eligible
        );
    }

    /// **The plan counts the reds it ignored across the jobs, and the level line
    /// says how many.** A red that counts is not in the number.
    #[test]
    fn the_level_line_says_how_many_reds_were_ignored() {
        let mut history = History::default();
        let mut alpha = clean(8);
        alpha[3] = run(3, &["a::b"]);
        alpha[4] = run(4, &["a::b"]);
        history.jobs.insert("test-alpha".into(), alpha);
        let mut beta = clean(8);
        beta[2] = run(2, &["c::d"]);
        history.jobs.insert("test-beta".into(), beta);
        let known = [("h003", false), ("h004", false), ("h002", true)];

        let plan = report_plan(&listed(), &history, &|_| true, &ancestry_of(&known));

        assert_eq!(plan.ignored, 2, "alpha's two reds, not beta's");
        let line = report_line(&plan, listed().len());
        assert!(line.contains("ignored 2 reds"), "{line}");
    }
}
