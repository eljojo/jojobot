//! **A paid run ends with where its time went, and the numbers are the run's
//! own.**
//!
//! These drive the whole of `go()` against a room they spawn, with a stand-in
//! for the agent CLI that prints a recorded stream. Nothing here reaches the
//! network or spends anything. The stream is the `made-calls` fixture, so the
//! model's turns and calls have a known answer, and the wall time and the
//! server's time come from the clock and the request log of this run.

use std::path::PathBuf;

use jojobot_exercise::agent::Agent;
use jojobot_exercise::playbook::Playbook;
use jojobot_exercise::run::{self, Expectation, Observed, Outcome};
use jojobot_exercise::surface::Seed;

/// A lock that holds, and asks the room one question, so the server has
/// something to do inside the run.
struct AsksTheRoom;

#[async_trait::async_trait]
impl Expectation for AsksTheRoom {
    fn name(&self) -> &str {
        "the room is asked something"
    }

    async fn check(&self, seen: &Observed<'_>) -> Outcome {
        let _ = seen.room.stated_day().await;
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
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "jojobot-exercise-run-timing-{}-{}",
            std::process::id(),
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

/// **The text of a run whose agent printed `stream`**, whole.
async fn rendered_run_of(stream: &str) -> String {
    let scratch = Scratch::new();
    let program = scratch.0.join("fake-agent");
    let recorded = scratch.0.join("stream.jsonl");
    std::fs::write(&recorded, stream).expect("the recorded stream is written");
    jojobot_exercise::spawn_gate::write_script(
        &program,
        &format!("#!/bin/sh\ncat '{}'\n", recorded.display()),
    )
    .expect("the stand-in agent is written");
    let agent = Agent::new("sonnet").launching(program.to_str().expect("a utf-8 path"));
    let playbook = Playbook::parse(
        "rooms/stand-in.md",
        "# A room\n\n**Server day: 2031-02-03.**\n\n## Phase 1 — the only sitting\n\n> Do the thing.\n",
    )
    .expect("the document parses");
    let held: Vec<Box<dyn Expectation>> = vec![Box::new(AsksTheRoom)];
    let results = run::go(&playbook, &agent, &Seed::new(), &held)
        .await
        .expect("the run finishes");
    results.rendered(None)
}

/// The text after `key` on the block's line, which is the last such line.
fn said(text: &str, key: &str) -> String {
    text.lines()
        .rev()
        .find_map(|line| line.strip_prefix(key))
        .unwrap_or_else(|| panic!("no line starts with {key:?}: {text}"))
        .to_string()
}

/// **The block states the stream's turns and calls, a wall time above zero, and
/// a server share below the whole.** The turns and calls are read out of the
/// stream the stand-in printed; the others could only come from this run.
#[tokio::test]
async fn a_paid_run_ends_with_its_own_wall_time_turns_calls_and_server_share() {
    let text = rendered_run_of(include_str!("fixtures/made-calls.jsonl")).await;
    assert!(
        text.contains("\ntiming\n"),
        "the run carries no timing block: {text}"
    );
    assert_eq!(said(&text, "model turns: "), "2", "{text}");
    assert_eq!(said(&text, "tool calls: "), "2", "{text}");

    let wall: f64 = said(&text, "wall time: ")
        .trim_end_matches(" s")
        .parse()
        .expect("a number of seconds");
    assert!(wall > 0.0, "a run that opened a room took no time: {text}");

    // "17.2 s in 118 requests, 2.4% of the wall time"
    let server = said(&text, "server executing: ");
    let requests: usize = server
        .split(" in ")
        .nth(1)
        .and_then(|rest| rest.split(' ').next())
        .and_then(|count| count.parse().ok())
        .unwrap_or_else(|| panic!("no request count in {server:?}"));
    assert!(requests > 0, "the server logged no request: {text}");
    let share: f64 = server
        .split(", ")
        .last()
        .and_then(|part| part.split('%').next())
        .and_then(|number| number.parse().ok())
        .unwrap_or_else(|| panic!("no share in {server:?}"));
    assert!(
        (0.0..100.0).contains(&share),
        "the server's share is not below the whole: {server}"
    );
}

/// **A stream that moves, moves the block.** Paired with the case above: a
/// stream with one answer and no call gives one turn and no calls, so a block
/// printing constants passes one case and fails the other.
#[tokio::test]
async fn a_different_stream_gives_a_different_block() {
    let text = rendered_run_of(include_str!("fixtures/called-nothing.jsonl")).await;
    assert_eq!(said(&text, "model turns: "), "1", "{text}");
    assert_eq!(said(&text, "tool calls: "), "0", "{text}");
}
