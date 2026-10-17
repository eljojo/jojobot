//! **Where a run's time went**, said once at the end of its report.
//!
//! A paid run takes about twenty minutes, and nothing recorded where they
//! went. This module reads three things the run already holds: its own clock,
//! the model's raw event stream, and the server's request log. It puts them in
//! one block at the end of the report and the kept transcript, so the question
//! "is the server the slow part" has an answer without a hand measurement.
//!
//! **It measures and does not judge.** The share of the wall time the server
//! spent executing is a number beside the others. What it means is the reader's.

use std::time::Duration;

/// What the server spent executing requests, from its own request log.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerTime {
    /// The sum of the time each request was open.
    pub executing: Duration,
    /// How many requests that sum is over.
    pub requests: usize,
}

/// **The sum of the request spans in a server's log.**
///
/// A request span is the `duration_ms` on a `mcp request ended` line of the
/// request log. Two kinds of request are left out. A standing stream (a GET) is
/// open by design for as long as the client listens, and the server words its
/// line "stream", not "request". A `subscriptions/listen` request is held open
/// the same way, so neither is the server working.
pub fn server_time(log: &[String]) -> ServerTime {
    let mut time = ServerTime::default();
    for line in log {
        if !line.contains("jojobot::request_log:")
            || !line.contains("mcp request ended")
            || line.contains(LISTEN)
        {
            continue;
        }
        let Some(millis) = field_after(line, "duration_ms=") else {
            continue;
        };
        time.executing += Duration::from_millis(millis);
        time.requests += 1;
    }
    time
}

/// The MCP method of a request the client holds open on purpose. The server
/// names it the same way in `request_log`.
const LISTEN: &str = "mcp_method=\"subscriptions/listen\"";

/// The whole number that follows `key` in a log line.
fn field_after(line: &str, key: &str) -> Option<u64> {
    let rest = &line[line.find(key)? + key.len()..];
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// **How many answers the model gave**: the distinct message ids of the
/// assistant events in a raw stream. The CLI writes one event per content block
/// of an answer, so counting events would count a thought and a call as two
/// turns.
pub fn model_turns(raw: &str) -> usize {
    let mut answers = std::collections::HashSet::new();
    for event in assistant_events(raw) {
        if let Some(id) = event["message"]["id"].as_str() {
            answers.insert(id.to_string());
        }
    }
    answers.len()
}

/// **How many tool calls the model made**: the `tool_use` blocks in a raw
/// stream, whatever tool they name.
pub fn tool_calls(raw: &str) -> usize {
    let mut calls = std::collections::HashSet::new();
    for event in assistant_events(raw) {
        let Some(blocks) = event["message"]["content"].as_array() else {
            continue;
        };
        for block in blocks.iter().filter(|b| b["type"] == "tool_use") {
            if let Some(id) = block["id"].as_str() {
                calls.insert(id.to_string());
            }
        }
    }
    calls.len()
}

/// The assistant events of a raw stream. A line that is not JSON, and the
/// boundary line between sittings, are not events and are skipped.
fn assistant_events(raw: &str) -> impl Iterator<Item = serde_json::Value> + '_ {
    raw.lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["type"] == "assistant")
}

/// Everything the block says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Timing {
    /// The run's own clock, from the first step to the last.
    pub wall: Duration,
    /// Answers the model gave, or `None` for a run in which no model ran.
    /// That is a replay, and it is not the same fact as a model that said
    /// nothing.
    pub model_turns: Option<usize>,
    /// Calls made: the model's, or the replay's.
    pub tool_calls: usize,
    /// The server's executing time.
    pub server: ServerTime,
}

impl Timing {
    /// **The block a report ends with.** One fact to a line, so a reader and a
    /// search both find it.
    pub fn block(&self) -> String {
        let wall = self.wall.as_secs_f64();
        let share = if wall > 0.0 {
            format!(
                "{:.1}% of the wall time",
                self.server.executing.as_secs_f64() / wall * 100.0
            )
        } else {
            "no share, the run took no time".to_string()
        };
        let turns = match self.model_turns {
            Some(turns) => turns.to_string(),
            None => "none, no model ran".to_string(),
        };
        format!(
            "timing\nwall time: {wall:.1} s\nmodel turns: {turns}\ntool calls: {}\n\
             server executing: {:.1} s in {} requests, {share}\n",
            self.tool_calls,
            self.server.executing.as_secs_f64(),
            self.server.requests,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The fixtures are the shapes a real stream has, cut down; see
    /// `calls::tests::fixture` for what comes out of them.
    const MADE_CALLS: &str = include_str!("../tests/fixtures/made-calls.jsonl");
    const CALLED_NOTHING: &str = include_str!("../tests/fixtures/called-nothing.jsonl");

    /// A request line as the server wrote it, with the time and level the log
    /// layer puts in front.
    fn ended(http: &str, mcp: &str, ms: u64) -> String {
        format!(
            "2026-10-17T01:41:10.628037Z  INFO jojobot::request_log: mcp request ended \
             http_method={http} mcp_method=\"{mcp}\" status=200 duration_ms={ms} outcome=\"answered\""
        )
    }

    #[test]
    fn the_server_time_is_the_sum_of_the_request_spans() {
        let log = vec![
            "2026-10-17T01:41:10.100000Z  INFO jojobot: serving 127.0.0.1:1".to_string(),
            ended("POST", "initialize", 8),
            ended("POST", "tools/call", 120),
            ended("POST", "tools/list", 5),
        ];
        let time = server_time(&log);
        assert_eq!(time.executing, Duration::from_millis(133), "{time:?}");
        assert_eq!(time.requests, 3, "{time:?}");
    }

    #[test]
    fn a_stream_held_open_on_purpose_is_not_the_server_working() {
        let log = vec![
            ended("POST", "tools/call", 40),
            ended("POST", "subscriptions/listen", 600_000),
            "2026-10-17T01:41:10.932861Z  INFO jojobot::request_log: mcp stream closed by the \
             client or the proxy http_method=GET status=200 duration_ms=301 outcome=\"cancelled\""
                .to_string(),
        ];
        let time = server_time(&log);
        assert_eq!(time.executing, Duration::from_millis(40), "{time:?}");
        assert_eq!(time.requests, 1, "{time:?}");
    }

    #[test]
    fn a_log_with_no_request_line_sums_to_nothing() {
        let time = server_time(&["something else".to_string()]);
        assert_eq!(time, ServerTime::default());
    }

    #[test]
    fn a_turn_is_an_answer_and_not_an_event() {
        // Three assistant events share one answer (a thought and two calls);
        // a second answer follows. Two turns, three calls' worth of events.
        assert_eq!(model_turns(MADE_CALLS), 2);
        assert_eq!(model_turns(CALLED_NOTHING), 1);
        assert_eq!(model_turns(""), 0);
    }

    #[test]
    fn a_tool_call_is_a_tool_use_block_whatever_the_tool() {
        assert_eq!(tool_calls(MADE_CALLS), 2);
        assert_eq!(tool_calls(CALLED_NOTHING), 0);
        assert_eq!(
            tool_calls("not json at all\n{\"jojobot_sitting\":\"one\"}"),
            0
        );
    }

    #[test]
    fn the_block_is_one_fact_to_a_line_and_has_no_double_space() {
        let timing = Timing {
            wall: Duration::from_millis(724_300),
            model_turns: Some(41),
            tool_calls: 96,
            server: ServerTime {
                executing: Duration::from_millis(17_200),
                requests: 118,
            },
        };
        let block = timing.block();
        let lines: Vec<&str> = block.lines().collect();
        assert_eq!(lines.len(), 5, "{block}");
        assert_eq!(lines[0], "timing");
        for (line, key) in lines[1..].iter().zip([
            "wall time: ",
            "model turns: ",
            "tool calls: ",
            "server executing: ",
        ]) {
            assert!(line.starts_with(key), "{line:?} should start with {key:?}");
        }
        assert!(!block.contains("  "), "a double space in {block:?}");
        assert!(lines[1].contains("724.3"), "{block}");
        assert!(lines[2].ends_with("41"), "{block}");
        assert!(lines[3].ends_with("96"), "{block}");
        assert!(lines[4].contains("17.2"), "{block}");
        assert!(lines[4].contains("118 requests"), "{block}");
        assert!(lines[4].contains("2.4%"), "{block}");
    }

    #[test]
    fn a_replay_says_no_model_ran_and_does_not_say_zero() {
        let block = Timing {
            wall: Duration::from_secs(3),
            model_turns: None,
            tool_calls: 4,
            ..Timing::default()
        }
        .block();
        assert!(block.contains("model turns: none, no model ran"), "{block}");
        assert!(block.contains("tool calls: 4"), "{block}");
    }

    #[test]
    fn a_run_with_no_wall_time_names_no_share_rather_than_dividing_by_zero() {
        let block = Timing::default().block();
        assert!(!block.contains("NaN") && !block.contains("inf"), "{block}");
        assert!(block.contains("no share"), "{block}");
    }
}
