//! **A boot that never serves says how its process ended and what it said on
//! stderr.** Four processes stand in for a server: one killed from outside before
//! its serving line, one that exits, one that never prints and never stops, and
//! one that serves. Each failure is read for the signal or status and the words.

use std::process::Stdio;
use std::time::Duration;

use jojobot_adapters::testing::{STDERR_TAIL_LINES, wait_for_serving_line};

/// A shell script run as the child, with stdout and stderr piped.
fn child_running(script: &str) -> tokio::process::Child {
    tokio::process::Command::new("sh")
        .arg("-c")
        .arg(script)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .expect("sh runs")
}

#[tokio::test]
async fn a_child_killed_from_outside_is_named_by_its_signal_and_its_stderr_is_quoted() {
    let mut child = child_running("echo 'out of memory soon' >&2; kill -9 $$");
    let failure = wait_for_serving_line(&mut child, Duration::from_secs(10), "serving http://")
        .await
        .err()
        .expect("a killed child never serves");
    assert!(failure.ended.contains("signal 9"), "{failure}");
    assert!(
        failure
            .stderr_tail
            .iter()
            .any(|line| line.contains("out of memory soon")),
        "{failure}"
    );
}

#[tokio::test]
async fn a_child_that_exits_is_named_by_its_status() {
    let mut child = child_running("echo 'bad store' >&2; exit 3");
    let failure = wait_for_serving_line(&mut child, Duration::from_secs(10), "serving http://")
        .await
        .err()
        .expect("an exited child never serves");
    assert!(failure.ended.contains("status 3"), "{failure}");
    assert!(failure.to_string().contains("bad store"), "{failure}");
}

#[tokio::test]
async fn a_child_that_never_prints_is_killed_when_the_wait_ends_and_says_so() {
    let mut child = child_running("sleep 30");
    let failure = wait_for_serving_line(&mut child, Duration::from_millis(300), "serving http://")
        .await
        .err()
        .expect("a silent child never serves");
    assert!(failure.ended.contains("still running"), "{failure}");
}

#[tokio::test]
async fn only_the_last_lines_of_stderr_are_kept_and_a_serving_child_is_let_through() {
    let noisy = (1..=30)
        .map(|n| format!("echo 'noisy-{n:02}' >&2;"))
        .collect::<String>();
    let mut failing = child_running(&format!("{noisy} exit 1"));
    let failure = wait_for_serving_line(&mut failing, Duration::from_secs(10), "serving http://")
        .await
        .err()
        .expect("an exited child never serves");
    assert_eq!(failure.stderr_tail.len(), STDERR_TAIL_LINES);
    assert!(
        failure
            .stderr_tail
            .first()
            .is_some_and(|line| line == "noisy-11")
            && failure
                .stderr_tail
                .last()
                .is_some_and(|line| line == "noisy-30"),
        "{failure}"
    );

    let mut serving = child_running("echo booting; echo 'serving http://127.0.0.1:1'; sleep 1");
    let served = wait_for_serving_line(&mut serving, Duration::from_secs(10), "serving http://")
        .await
        .expect("a child that prints its serving line is let through");
    assert_eq!(served.seen.len(), 2, "{:?}", served.seen);
}
