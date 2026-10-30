//! **A refusal-count read that fails keeps the sittings already paid for.**
//!
//! `go()` asks the room how many grader requests it refused before and after
//! each phase. A read that failed used to leave `go()` through `?` with the
//! transcript still in a local, so the paid output of the finished phases was
//! never kept. This kills the room's server while the first sitting runs, so
//! the read after that sitting fails, and asks what the run still holds.
//!
//! **Its own file, because it kills a server.** The case finds the one server
//! this process started, so it must be the only case in the process that opens
//! a room.

use std::time::Duration;

use jojobot_exercise::agent::Agent;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::room::{parent_of, room_servers};
use jojobot_exercise::run::{self, Expectation, Observed, Outcome};
use jojobot_exercise::surface::Seed;

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

const PLAYBOOK: &str = "\
**Server day: 2031-02-03.**

## Phase 1 — the first sitting

> Do the first thing.

## Phase 2 — the second sitting

> Do the second thing.
";

/// **The sitting that spends the money also takes the server down**, the way a
/// room that died under a run does. The watcher finds the server this process
/// started and writes its pid where the stand-in agent reads it.
#[tokio::test]
async fn a_failed_refusal_count_read_keeps_the_sitting_already_run() {
    let scratch = std::env::temp_dir().join(jojobot_ports::scratch::name("exercise-refusal-count"));
    std::fs::create_dir_all(&scratch).expect("a scratch directory");
    let pidfile = scratch.join("server-pid");
    let launches = scratch.join("launches");
    let program = scratch.join("fake-agent");
    jojobot_exercise::spawn_gate::write_script(
        &program,
        &format!(
            "#!/bin/sh\n\
             echo launched >> '{launches}'\n\
             i=0\n\
             while [ ! -s '{pidfile}' ] && [ \"$i\" -lt 100 ]; do sleep 0.1; i=$((i+1)); done\n\
             kill -9 \"$(cat '{pidfile}')\"\n\
             echo 'the first sitting said this'\n",
            launches = launches.display(),
            pidfile = pidfile.display(),
        ),
    )
    .expect("the stand-in agent is written");

    let me = std::process::id();
    let watch = pidfile.clone();
    let watcher = tokio::spawn(async move {
        loop {
            // The jojobot server is the parent of the store, and this process is
            // the server's parent.
            let found = room_servers()
                .into_iter()
                .find(|room| parent_of(room.parent) == Some(me));
            if let Some(room) = found {
                std::fs::write(&watch, room.parent.to_string()).expect("the pid is written");
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    });

    let playbook = Playbook::parse("refusal-count", PLAYBOOK).expect("the playbook parses");
    let agent = Agent::new("sonnet").launching(program.to_str().expect("a utf-8 path"));
    let outcome = run::go(
        &playbook,
        &agent,
        &Seed::new(),
        &[Box::new(NeverHolds) as Box<dyn Expectation>],
    )
    .await;
    watcher.abort();
    let _ = std::fs::remove_dir_all(&scratch);

    let results = outcome.expect("a failed read ends the run with its results, never without them");
    assert_eq!(
        results.transcript.len(),
        1,
        "the sitting that ran is kept: {}",
        results.rendered(None)
    );
    assert!(
        results.transcript[0]
            .output
            .contains("the first sitting said this")
    );
    let incomplete = results
        .incomplete
        .as_ref()
        .expect("a run whose read failed says it did not finish");
    assert!(incomplete.failed_read.is_some(), "the failed read is named");
    assert!(!results.held(), "an incomplete run must never hold");
}
