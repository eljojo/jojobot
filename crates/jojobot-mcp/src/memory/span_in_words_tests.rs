use super::*;
use crate::harness::*;
use crate::memory::span_in_words::names_a_span;
use crate::memory::testing::*;

const SPAN_WORDS: &str = "Started the course: six Tuesday evenings, 6 October to 10 November 2026";

async fn capture_on_project(jojobot: &Jojobot, args: CaptureArgs) -> serde_json::Value {
    ensure(jojobot, "project:backyard-shed").await;
    json_of(&jojobot.capture(Parameters(args)).await.expect("capture ok"))
}

fn taught(receipt: &serde_json::Value) -> bool {
    receipt["teaching"].as_array().is_some_and(|lines| {
        lines.iter().any(|line| {
            line.as_str()
                .is_some_and(|t| t.contains("happened_through"))
        })
    })
}

/// **A span written as text is told where a span goes**, on the project write the
/// vault run made. The project-work offer beside it names neither key.
#[tokio::test]
async fn a_span_written_as_text_is_taught_the_span_keys() {
    let jojobot = handler();
    let receipt =
        capture_on_project(&jojobot, capture_args("project:backyard-shed", SPAN_WORDS)).await;
    assert!(taught(&receipt), "{receipt}");
}

/// **A single date is not a span**, and neither are two days joined by "and".
#[tokio::test]
async fn a_single_date_or_two_days_joined_by_and_are_not_taught() {
    let jojobot = handler();
    let single = capture_on_project(
        &jojobot,
        capture_args(
            "project:backyard-shed",
            "The inspection is on 6 October 2026",
        ),
    )
    .await;
    assert!(!taught(&single), "{single}");
    let two_days = capture_on_project(
        &jojobot,
        capture_args(
            "project:backyard-shed",
            "Visits are on 6 October and again on 13 October",
        ),
    )
    .await;
    assert!(!taught(&two_days), "{two_days}");
}

/// **A write that sent the span keys is not taught them.**
#[tokio::test]
async fn a_write_that_sent_the_span_keys_is_not_taught() {
    let jojobot = handler();
    let receipt = capture_on_project(
        &jojobot,
        CaptureArgs {
            happened_at: Some("2026-10-06".into()),
            happened_through: Some("2026-11-10".into()),
            ..capture_args("project:backyard-shed", SPAN_WORDS)
        },
    )
    .await;
    assert!(!taught(&receipt), "{receipt}");
}

/// **The line is said once a session**, and a numeric span is read too.
#[tokio::test]
async fn the_line_is_said_once_and_a_numeric_span_is_read() {
    let jojobot = handler();
    let first = capture_on_project(
        &jojobot,
        capture_args("project:backyard-shed", "Open 2026-10-06 to 2026-11-10"),
    )
    .await;
    assert!(taught(&first), "{first}");
    let second =
        capture_on_project(&jojobot, capture_args("project:backyard-shed", SPAN_WORDS)).await;
    assert!(!taught(&second), "{second}");
}

#[test]
fn the_reader_takes_the_ways_a_span_is_written() {
    for yes in [
        "6 October to 10 November",
        "October 6 through November 10",
        "6th October - 10th November 2026",
        "2026-10-06 until 2026-11-10",
        "6 October 2026 to 10 November 2026",
        "6 Oct \u{2013} 10 Nov",
    ] {
        assert!(names_a_span(yes), "{yes}");
    }
    for no in [
        "6 October 2026",
        "6 October and 10 November",
        "From 6 October, then 10 November",
        "Room 6 to 10 is free",
        "in October 2026",
    ] {
        assert!(!names_a_span(no), "{no}");
    }
}
