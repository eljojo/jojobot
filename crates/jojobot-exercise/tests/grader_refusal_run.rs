//! **A grader call the room refused during a phase fails the run, and names the
//! phase.**
//!
//! `go()` reads the room's refusal count before and after each phase. The only
//! case that touched the count filled the result by hand, so replacing
//! `refused_after > refused_before` with `false` changed no case, and a probed
//! grader yielded a passing paid run. This drives the whole of `go()` against a
//! room it spawns, with a stand-in agent that sends one request to the room's
//! harness route with no credential during one chosen sitting.
//!
//! **Paired with the same run where the stand-in sends nothing.** `go()` reads
//! the count with the room's token on every phase, so a build that dropped the
//! token header would fail that control at its first read; a build that counted
//! every phase as refused would fail the pair's second half.

use std::path::PathBuf;

use jojobot_exercise::agent::Agent;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::run::{self, Expectation, Observed, Outcome, Results};
use jojobot_exercise::surface::Seed;

/// A lock that always holds, so that the refusal is the only thing that can
/// make the run read as failed on its locks.
struct AlwaysHolds;

#[async_trait::async_trait]
impl Expectation for AlwaysHolds {
    fn name(&self) -> &str {
        "a lock that holds"
    }

    async fn check(&self, _seen: &Observed<'_>) -> Outcome {
        Outcome {
            name: self.name().to_string(),
            held: true,
            applies: true,
            refused: false,
            saying: "the room carries it".to_string(),
        }
    }
}

const PLAYBOOK: &str = "\
**Server day: 2031-02-03.**

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
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("a clock after 1970")
            .as_nanos();
        let path = std::env::temp_dir().join(jojobot_ports::scratch::name(&format!(
            "exercise-grader-refusal-{nanos}"
        )));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Drive `go()` with an agent that, on its `probe_at`th launch, posts to the
/// room's harness route without the credential. `None` never probes.
async fn run_probing_at(probe_at: Option<usize>) -> Results {
    let scratch = Scratch::new();
    let launches = scratch.0.join("launches");
    let program = scratch.0.join("fake-agent");
    let probe_at = probe_at.map_or(0, |at| at);
    jojobot_exercise::spawn_gate::write_script(
        &program,
        &format!(
            "#!/bin/sh\n\
             echo launched >> '{launches}'\n\
             if [ \"$(wc -l < '{launches}')\" -eq {probe_at} ]; then\n\
             endpoint=$(printf '%s\\n' \"$@\" | grep -o 'http://[^\"]*/mcp' | head -1)\n\
             curl -s -o /dev/null -X POST -H 'content-type: application/json' \
             -d '{{\"operation\":\"refusals\"}}' \"${{endpoint%/mcp}}/_room_harness\"\n\
             fi\n",
            launches = launches.display(),
        ),
    )
    .expect("the stand-in agent is written");

    let playbook = Playbook::parse("grader-refusal", PLAYBOOK).expect("the playbook parses");
    let agent = Agent::new("sonnet").launching(program.to_str().expect("a utf-8 path"));
    run::go(
        &playbook,
        &agent,
        &Seed::new(),
        &[Box::new(AlwaysHolds) as Box<dyn Expectation>],
    )
    .await
    .expect("the run goes through")
}

#[tokio::test]
async fn a_request_the_room_refused_in_the_second_sitting_is_named_with_that_phase() {
    let results = run_probing_at(Some(2)).await;
    assert_eq!(
        results.harness_refusals.len(),
        1,
        "exactly the sitting that probed is named: {:?}",
        results.harness_refusals,
    );
    let (phase, refused) = &results.harness_refusals[0];
    assert!(
        phase.starts_with("Phase 2"),
        "the refusal is named for the sitting that caused it: {phase}",
    );
    assert_eq!(*refused, 1, "one refused request was sent");
    assert!(!results.held(), "a probed grader must never pass a run");
    let rendered = results.rendered(None);
    assert!(
        rendered.contains("[FAILED] grader route — 1 refused calls during Phase 2"),
        "the verdict does not name the refused calls and their phase: {rendered}",
    );
}

#[tokio::test]
async fn the_same_run_with_no_probe_records_no_refusal() {
    let results = run_probing_at(None).await;
    assert!(
        results.harness_refusals.is_empty(),
        "a run whose agent never touched the route recorded refusals: {:?}",
        results.harness_refusals,
    );
    assert!(
        !results.rendered(None).contains("grader route"),
        "the verdict names a grader route nobody probed",
    );
}
