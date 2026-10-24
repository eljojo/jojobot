//! **A paid run opens its server on the day the room's document states, and
//! never on the wall clock.**
//!
//! A server stamps its own clock day on the moments it writes and on the
//! `as_of` of every answer, and a lock matches a needle as text in an answer. A
//! run on the real clock can pass or fail on the calendar. These drive the whole
//! of `go()` against a room it spawns, with a stand-in for the agent CLI that
//! does nothing, and read the day the server reports from inside the run.
//! Nothing here reaches the network or spends anything.
//!
//! **The days are in the 2030s**, so no wall clock a test could run on reports
//! one of them: a server on the real clock fails every case here on every day.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use jojobot_exercise::agent::Agent;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::run::{self, Expectation, Observed, Outcome};
use jojobot_exercise::surface::Seed;

/// A lock that holds, and writes down the day the room's server reports while
/// the run is still going.
struct ReadsTheDay(Arc<Mutex<Vec<Option<String>>>>);

#[async_trait::async_trait]
impl Expectation for ReadsTheDay {
    fn name(&self) -> &str {
        "the server's day is read from inside the run"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let day = seen.room.stated_day().await;
        self.0.lock().expect("the day log").push(day);
        Outcome {
            name: self.name().to_string(),
            held: true,
            applies: true,
            refused: false,
            saying: String::new(),
        }
    }
}

/// A directory this case owns alone, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let path = std::env::temp_dir().join(jojobot_ports::scratch::name("exercise-server-day"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A one-sitting document, stating `day` as its server day when given one.
fn document(day: Option<&str>) -> String {
    let stated = day
        .map(|day| format!("**Server day: {day}.**\n\n"))
        .unwrap_or_default();
    format!("# A room\n\n{stated}## Phase 1 — the only sitting\n\n> Do the thing.\n")
}

/// Run `go()` over `text` with an agent that only records that it started, and
/// say what day the server reported and how many times the agent started.
async fn run_over(text: &str) -> (anyhow::Result<()>, Vec<Option<String>>, usize) {
    let scratch = Scratch::new();
    let launches = scratch.0.join("launches");
    let program = scratch.0.join("fake-agent");
    jojobot_exercise::spawn_gate::write_script(
        &program,
        &format!(
            "#!/bin/sh\necho launched >> '{launches}'\n",
            launches = launches.display(),
        ),
    )
    .expect("the stand-in agent is written");
    let agent = Agent::new("sonnet").launching(program.to_str().expect("a utf-8 path"));

    let days = Arc::new(Mutex::new(Vec::new()));
    let playbook = Playbook::parse("rooms/stand-in.md", text).expect("the document parses");
    let held: Vec<Box<dyn Expectation>> = vec![Box::new(ReadsTheDay(days.clone()))];
    let outcome = run::go(&playbook, &agent, &Seed::new(), &held)
        .await
        .map(|_| ());
    let started = std::fs::read_to_string(&launches)
        .unwrap_or_default()
        .lines()
        .count();
    let seen = days.lock().expect("the day log").clone();
    (outcome, seen, started)
}

/// **The run's server reports the day the document states, and it moves when
/// the document does.** Two documents differing only in that line, so a run
/// that carried a constant of its own would pass one and fail the other.
#[tokio::test]
async fn a_paid_runs_server_reports_the_day_its_document_states() {
    for day in ["2031-02-03", "2032-07-19"] {
        let (outcome, seen, started) = run_over(&document(Some(day))).await;
        outcome.expect("a document that states its day runs");
        assert_eq!(started, 1, "the agent started once");
        assert_eq!(
            seen,
            vec![Some(day.to_string())],
            "the server did not act out the day its document states",
        );
    }
}

/// **A document with no stated day is refused before the agent starts.**
/// Paired with the case above: the same document, given a day, runs.
#[tokio::test]
async fn a_document_that_states_no_server_day_is_refused_before_the_agent_starts() {
    let (outcome, seen, started) = run_over(&document(None)).await;
    let refused = outcome.expect_err("a run with no stated day must not start");
    assert!(
        refused.to_string().contains("rooms/stand-in.md"),
        "the refusal does not name the room: {refused}",
    );
    assert_eq!(started, 0, "the agent started on a room with no stated day");
    assert!(seen.is_empty(), "a lock ran on a room that was refused");
}
