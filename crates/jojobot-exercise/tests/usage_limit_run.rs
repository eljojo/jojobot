//! **A run that hits the usage limit stops launching sittings.**
//!
//! `stops_the_run` is unit-tested on its own, and that left the `break` in
//! `go()` that reads its answer unwatched: removing it changed no case. This
//! drives the whole of `go()` against a room it spawns, with a stand-in for the
//! agent CLI that prints the CLI's own limit words and exits non-zero at a
//! chosen sitting. Nothing here reaches the network or spends anything.
//!
//! **Paired with the same run where the limit never comes.** A build that
//! always stopped after the first sitting would pass the half that checks
//! later sittings did not launch, and a build that never stopped would pass
//! the half that checks a lock never reads FAILED on a run that did not
//! finish. Both ends move only when the `break` does.

use std::path::PathBuf;

use jojobot_exercise::agent::Agent;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::run::{self, Expectation, Observed, Outcome, Results};
use jojobot_exercise::surface::Seed;

/// A lock that never holds. On a finished run it must read FAILED; on one that
/// ran out of runway it must not.
struct NeverHolds;

#[async_trait::async_trait]
impl Expectation for NeverHolds {
    fn name(&self) -> &str {
        "a lock that fails on its own merits"
    }

    async fn check(&self, _seen: &Observed<'_>) -> Outcome {
        Outcome {
            name: self.name().to_string(),
            held: false,
            applies: true,
            refused: false,
            saying: "the room does not carry it".to_string(),
        }
    }
}

/// A lock named for the phase it belongs to, as a lock read out of a room
/// document is (`Phase N — the sentence`), that never holds.
struct NeverHoldsIn(&'static str);

#[async_trait::async_trait]
impl Expectation for NeverHoldsIn {
    fn name(&self) -> &str {
        self.0
    }

    async fn check(&self, _seen: &Observed<'_>) -> Outcome {
        Outcome {
            name: self.0.to_string(),
            held: false,
            applies: true,
            refused: false,
            saying: "the room does not carry it".to_string(),
        }
    }
}

const PLAYBOOK: &str = "\
## Phase 1 — the first sitting

> Do the first thing.

## Phase 2 — the second sitting

> Do the second thing.

## Phase 3 — the third sitting

> Do the third thing.
";

/// A directory this case owns alone, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        Scratch::at(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("a clock after 1970")
                .as_nanos(),
        )
    }

    /// The directory for a case that starts at this instant. The counter is
    /// what makes the name unique: the process and the clock repeat when two
    /// cases of this process start together.
    fn at(nanos: u128) -> Scratch {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "jojobot-exercise-usage-limit-{}-{}-{}",
            std::process::id(),
            nanos,
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **Two cases that start at the same instant get two directories.** The cases
/// run side by side in one process, so a name built from the process and the
/// clock alone can repeat. Both then share one launch count and one script,
/// and whichever finishes first removes the directory under the other: a
/// missing script in a later sitting, or the other case's stderr read as this
/// case's own.
#[test]
fn two_scratch_directories_made_at_one_instant_are_distinct() {
    let first = Scratch::at(7);
    let second = Scratch::at(7);
    assert_ne!(first.0, second.0, "two cases were given one directory");
}

/// The CLI's own words when the usage limit stops it.
const LIMIT_WORDS: &str = "Claude usage limit reached. Your limit resets 3pm";

/// Drive `go()` with an agent that prints `stderr` and exits 1 on its
/// `fail_at`th launch, and say how many times it was launched.
async fn run_failing_at(fail_at: usize, stderr: &str) -> (Results, usize) {
    run_failing_at_judged_by(fail_at, stderr, vec![Box::new(NeverHolds)]).await
}

/// The same drive, with the locks the run is judged by chosen by the case.
async fn run_failing_at_judged_by(
    fail_at: usize,
    stderr: &str,
    expectations: Vec<Box<dyn Expectation>>,
) -> (Results, usize) {
    let scratch = Scratch::new();
    let launches = scratch.0.join("launches");
    let program = scratch.0.join("fake-agent");
    jojobot_exercise::spawn_gate::write_script(
        &program,
        &format!(
            "#!/bin/sh\n\
             echo launched >> '{launches}'\n\
             if [ \"$(wc -l < '{launches}')\" -eq {fail_at} ]; then\n\
             echo '{stderr}' >&2\n\
             exit 1\n\
             fi\n",
            launches = launches.display(),
        ),
    )
    .expect("the stand-in agent is written");

    let playbook = Playbook::parse("usage-limit", PLAYBOOK).expect("the playbook parses");
    let agent = Agent::new("sonnet").launching(program.to_str().expect("a utf-8 path"));
    let results = run::go(&playbook, &agent, &Seed::new(), &expectations)
        .await
        .expect("the run goes through");
    let launched = std::fs::read_to_string(&launches)
        .unwrap_or_default()
        .lines()
        .count();
    (results, launched)
}

/// The line of a rendered verdict that says the run is incomplete.
fn incomplete_line(rendered: &str) -> String {
    rendered
        .lines()
        .find(|line| line.contains("INCOMPLETE"))
        .unwrap_or_else(|| panic!("the verdict has no INCOMPLETE line: {rendered}"))
        .to_string()
}

#[tokio::test]
async fn a_limit_in_the_second_sitting_stops_the_third_from_launching() {
    let (results, launched) = run_failing_at(2, LIMIT_WORDS).await;
    assert_eq!(
        launched, 2,
        "the sitting after the limit was launched anyway"
    );
    let incomplete = results
        .incomplete
        .as_ref()
        .expect("a run that hit the limit says it did not finish");
    assert!(
        incomplete.phase.starts_with("Phase 2"),
        "the run names the sitting that hit the limit: {}",
        incomplete.phase,
    );
    assert_eq!(incomplete.reset.as_deref(), Some("3pm"));
    assert!(!results.held(), "an incomplete run must never hold");
    let rendered = results.rendered(None);
    assert!(
        rendered.contains("INCOMPLETE"),
        "the verdict does not say the run is incomplete: {rendered}"
    );
    // The agent's own words are echoed in the transcript above, so the cause
    // is asked of the verdict line itself.
    let verdict = incomplete_line(&rendered);
    assert!(
        verdict.contains("usage limit"),
        "the verdict line does not name the cause: {verdict}"
    );
    assert!(
        rendered.contains("[not run] a lock that fails on its own merits"),
        "a lock on an incomplete run must read as not run: {rendered}"
    );
    assert!(
        !rendered.contains("[FAILED] a lock that fails on its own merits"),
        "a lock the run never gave a fair chance read as FAILED: {rendered}"
    );
}

#[tokio::test]
async fn the_same_run_with_no_limit_launches_every_sitting_and_the_lock_fails() {
    let (results, launched) = run_failing_at(99, LIMIT_WORDS).await;
    assert_eq!(launched, 3, "every sitting launches when no limit comes");
    assert!(
        results.incomplete.is_none(),
        "a run that never hit a limit is not incomplete"
    );
    let rendered = results.rendered(None);
    assert!(
        rendered.contains("[FAILED] a lock that fails on its own merits"),
        "a lock that really fails on a finished run must read FAILED: {rendered}"
    );
}

#[tokio::test]
async fn an_ordinary_failure_in_the_second_sitting_is_not_a_limit() {
    let (results, launched) = run_failing_at(2, "error: the model could not be reached").await;
    assert_eq!(
        launched, 3,
        "a failure that is not a usage limit stopped the run"
    );
    assert!(
        results.incomplete.is_none(),
        "an ordinary non-zero exit was read as a usage limit"
    );
    let rendered = results.rendered(None);
    assert!(
        rendered.contains("[FAILED] a lock that fails on its own merits"),
        "a lock on a run that finished must read FAILED: {rendered}"
    );
    assert!(
        !rendered.contains("[not run]"),
        "a lock on a run that finished read as not run: {rendered}"
    );
}

#[tokio::test]
async fn a_session_limit_is_named_as_the_agents_own_words_said_it() {
    let (results, _) =
        run_failing_at(2, "Claude session limit reached. Your limit resets 3pm").await;
    let verdict = incomplete_line(&results.rendered(None));
    assert!(
        verdict.contains("session limit"),
        "the verdict does not name the session limit the agent reported: {verdict}"
    );
    assert!(
        !verdict.contains("usage limit"),
        "the verdict names a usage limit the agent never reported: {verdict}"
    );
}

#[tokio::test]
async fn a_lock_from_before_the_limit_still_reads_failed_and_later_ones_read_not_run() {
    let expectations: Vec<Box<dyn Expectation>> = vec![
        Box::new(NeverHoldsIn(
            "Phase 1 \u{2014} a lock that failed on its own merits",
        )),
        Box::new(NeverHoldsIn(
            "Phase 2 \u{2014} a lock in the sitting that hit the limit",
        )),
        Box::new(NeverHoldsIn(
            "Phase 3 \u{2014} a lock in a sitting that never ran",
        )),
    ];
    let (results, _) = run_failing_at_judged_by(2, LIMIT_WORDS, expectations).await;
    assert!(
        results.incomplete.is_some(),
        "the limit must stop the run, or the case measures nothing"
    );
    let rendered = results.rendered(None);
    assert!(
        rendered.contains("[FAILED] Phase 1 \u{2014} a lock that failed on its own merits"),
        "a lock from before the limit failed on its own and must read FAILED: {rendered}"
    );
    assert!(
        rendered.contains("[not run] Phase 2 \u{2014} a lock in the sitting that hit the limit"),
        "a lock in the sitting that hit the limit must read not run: {rendered}"
    );
    assert!(
        rendered.contains("[not run] Phase 3 \u{2014} a lock in a sitting that never ran"),
        "a lock after the limit must read not run: {rendered}"
    );
}
