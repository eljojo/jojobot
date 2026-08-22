//! **What the occupant DID, kept as the run's primitive.**
//!
//! The readable transcript holds what the model said at the end of each
//! sitting. It holds nothing about the calls behind that answer, so a sitting
//! that reports it marked a record superseded and a sitting that marked nothing
//! render the same. A person reading the run cannot tell which happened, and
//! that difference is the whole verdict.
//!
//! **The raw event stream is what this module keeps, and everything else is a
//! rendering of it.** A paid run costs money; a parsing bug must never cost
//! one. So the stream goes to disk exactly as the CLI wrote it, beside the
//! readable transcript and never instead of it — if a renderer is wrong, the
//! material is still there and the run can be read again.

/// **Where a run's raw stream goes, given where its readable transcript went.**
///
/// A sibling, derived rather than configured: a run is asked for one path, and
/// a second setting is a second thing to get wrong. The readable file keeps its
/// own name untouched, because the operator reads that one.
pub fn beside(transcript: &std::path::Path) -> std::path::PathBuf {
    transcript.with_extension("jsonl")
}

/// **The line that says which sitting the events after it belong to.**
///
/// It is itself one JSON object, so the file stays valid JSONL from end to end
/// and a reader needs no second format to find the boundaries.
fn boundary(sitting: &str) -> String {
    serde_json::json!({ "jojobot_sitting": sitting }).to_string()
}

/// **Every sitting's stream, in order, with the raw lines untouched.**
///
/// A sitting that produced nothing keeps its boundary line and adds no events.
/// That is the case this shape exists for: **silence and a capture that failed
/// must not read alike**, and a sitting missing from the file altogether is
/// what a failed capture looks like.
pub fn raw_stream(sittings: &[(String, String)]) -> String {
    let mut out = String::new();
    for (sitting, raw) in sittings {
        out.push_str(&boundary(sitting));
        out.push('\n');
        for line in raw.lines() {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// Keep the raw stream beside the readable transcript.
pub fn keep_raw(
    transcript: &std::path::Path,
    sittings: &[(String, String)],
) -> std::io::Result<()> {
    let path = beside(transcript);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, raw_stream(sittings))
}

/// **One call the occupant made, as a reader needs it.**
///
/// Not the whole payload of anything. Some answers in these rooms are tens of
/// kilobytes, and a log nobody can read is the same as no log — so this is the
/// verb, enough of the arguments to say what it acted on, and enough of the
/// answer to tell an answer from a refusal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Call {
    /// The tool as the CLI named it.
    pub verb: String,
    /// What it acted on, read off the arguments.
    pub about: String,
    /// **Whether the room turned it down.** A refusal and an answer are the
    /// difference between the occupant failing and the surface refusing, which
    /// is the whole verdict on a sitting.
    pub refused: bool,
    /// Enough of the answer to tell one from another, and no more.
    pub head: String,
}

/// **How much of an answer is kept.** Enough to tell a refusal from an answer
/// and one answer from another; never enough to bury the log.
const HEAD: usize = 160;

/// The first `HEAD` characters, cut on a character boundary.
fn head_of(text: &str) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(HEAD) {
        Some((at, _)) => format!("{}…", &flat[..at]),
        None => flat,
    }
}

/// **Every call in one sitting's stream, in the order it was made.**
///
/// Reads the events and nothing else: no judgement about whether a call was
/// the right one, because that is a person's reading and this is deterministic
/// software.
pub fn calls_in(stream: &str) -> Vec<Call> {
    let events: Vec<serde_json::Value> = stream
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect();
    // **The answer arrives in a later event than the call**, so the results are
    // gathered first and each call then finds its own by id.
    let mut answers: std::collections::HashMap<String, (bool, String)> =
        std::collections::HashMap::new();
    for block in events.iter().flat_map(blocks) {
        if block["type"] == "tool_result" {
            let Some(id) = block["tool_use_id"].as_str() else {
                continue;
            };
            // **`is_error` is absent on a call that worked.** Reading a missing
            // key as anything but "not an error" marks every good call refused.
            let refused = block["is_error"].as_bool().unwrap_or(false);
            answers.insert(
                id.to_string(),
                (refused, head_of(&flatten(&block["content"]))),
            );
        }
    }
    events
        .iter()
        .flat_map(blocks)
        .filter(|block| block["type"] == "tool_use")
        .map(|block| {
            let id = block["id"].as_str().unwrap_or_default();
            let (refused, head) = answers.get(id).cloned().unwrap_or((
                false,
                // **A call whose answer never arrived is a real shape.** The
                // run can be stopped mid-sitting, and a log that invented an
                // answer would hide exactly that.
                "[no answer in the stream]".to_string(),
            ));
            Call {
                verb: block["name"].as_str().unwrap_or("[unnamed]").to_string(),
                about: head_of(&flatten(&block["input"])),
                refused,
                head,
            }
        })
        .collect()
}

/// **What the occupant said at the end, which is what a person reads.**
///
/// The CLI reports it on the `result` event. `None` is a sitting that never
/// reached one — stopped, or cut off — and that is a fact rather than an empty
/// string.
pub fn final_text(stream: &str) -> Option<String> {
    stream
        .lines()
        .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
        .filter(|event| event["type"] == "result")
        .find_map(|event| event["result"].as_str().map(str::to_string))
}

/// The content blocks of one event, which is where calls and answers live.
fn blocks(event: &serde_json::Value) -> Vec<serde_json::Value> {
    event["message"]["content"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

/// Text out of a value that may be a string, a list of blocks, or an object.
fn flatten(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(text) => text.clone(),
        serde_json::Value::Array(items) => items.iter().map(flatten).collect::<Vec<_>>().join(" "),
        serde_json::Value::Object(fields) => fields
            .iter()
            .filter(|(key, _)| key.as_str() != "type")
            .map(|(key, held)| format!("{key}={}", flatten(held)))
            .collect::<Vec<_>>()
            .join(" "),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sitting(name: &str, raw: &str) -> (String, String) {
        (name.to_string(), raw.to_string())
    }

    /// **The raw stream lands beside the readable transcript, not on top of
    /// it.** The operator reads the readable one; a capture that overwrote it
    /// would trade the run's most-read output for its most-machine-readable.
    #[test]
    fn the_raw_stream_is_a_sibling_and_the_readable_transcript_keeps_its_name() {
        let readable = std::path::Path::new("transcripts/year-1787418998.md");
        let raw = beside(readable);
        assert_ne!(
            raw, readable,
            "the raw stream took the readable file's name"
        );
        assert_eq!(
            raw.parent(),
            readable.parent(),
            "a run's two outputs belong in one place: {}",
            raw.display(),
        );
        assert_eq!(
            raw.file_name().map(|n| n.to_string_lossy().to_string()),
            Some("year-1787418998.jsonl".to_string()),
            "the sibling is named for the same run: {}",
            raw.display(),
        );
    }

    /// **The raw lines survive byte for byte**, because they are the primitive
    /// every later reading is made from. A capture that reshaped them would put
    /// a parser between the run and its own evidence.
    #[test]
    fn every_event_line_survives_exactly_as_the_cli_wrote_it() {
        let odd = r#"{"type":"assistant","text":"a quote \" and a backslash \\ and a brace }"}"#;
        let kept = raw_stream(&[sitting("Phase 1", odd)]);
        assert!(
            kept.contains(odd),
            "the raw line was rewritten on the way to disk:\n{kept}",
        );
    }

    /// **A sitting that made no calls is not a sitting that went missing.**
    ///
    /// Three sittings of the first paid run produced nothing. If silence and a
    /// capture that failed render the same, the instrument cannot answer the
    /// question it was built for — so the empty sitting keeps its boundary and
    /// the absent one has none.
    ///
    /// **Both halves in one case.** The positive is what the negative rests on:
    /// without it, a build that wrote an empty file would pass the absence
    /// check and fail nothing.
    #[test]
    fn a_sitting_that_produced_nothing_is_named_and_one_that_never_ran_is_not() {
        let kept = raw_stream(&[
            sitting("Phase 1 — January", r#"{"type":"result"}"#),
            sitting("Phase 2 — February", ""),
        ]);
        assert!(
            kept.contains("Phase 2 — February"),
            "a sitting that produced nothing is missing from the record, so it reads exactly like \
             a capture that failed:\n{kept}",
        );
        assert!(
            !kept.contains("Phase 3"),
            "a sitting that never ran was named anyway, so the record invents one:\n{kept}",
        );
        // **By line, not by substring.** The sitting's name sits inside a JSON
        // object, so splitting on the name alone lands mid-line and leaves the
        // rest of the boundary looking like an event.
        let lines: Vec<&str> = kept.lines().collect();
        let at = lines
            .iter()
            .position(|l| l.contains("Phase 2 — February"))
            .expect("the boundary is there, which the assertion above just held");
        assert_eq!(
            &lines[at + 1..],
            &[] as &[&str],
            "the empty sitting carried events, so the boundaries do not say who owns what:\n{kept}",
        );
    }

    /// **Every sitting is separated by a line that is itself JSON**, so the
    /// whole file parses as JSONL and a reader needs no second format to walk
    /// it.
    #[test]
    fn the_file_is_valid_jsonl_from_end_to_end_including_the_boundaries() {
        let kept = raw_stream(&[
            sitting("Phase 1", "{\"type\":\"system\"}\n{\"type\":\"result\"}"),
            sitting("Phase 2", "{\"type\":\"result\"}"),
        ]);
        let lines: Vec<&str> = kept.lines().collect();
        assert_eq!(
            lines.len(),
            5,
            "two boundaries and three events: {lines:#?}",
        );
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line)
                .unwrap_or_else(|e| panic!("a line the reader cannot parse: {line}\n{e}"));
        }
        assert_eq!(
            lines
                .iter()
                .filter(|l| l.contains("jojobot_sitting"))
                .count(),
            2,
            "one boundary per sitting: {lines:#?}",
        );
    }

    /// The stream reaches the disk at the path the sibling names.
    #[test]
    fn the_kept_stream_is_readable_after_the_process_that_wrote_it_is_gone() {
        let dir = std::env::temp_dir().join(format!("jojobot-raw-{}", std::process::id()));
        let readable = dir.join("ledger-1.md");
        keep_raw(&readable, &[sitting("Phase 1", r#"{"type":"result"}"#)]).expect("kept");
        let back = std::fs::read_to_string(beside(&readable)).expect("the sibling is on disk");
        assert!(
            back.contains("Phase 1") && back.contains(r#"{"type":"result"}"#),
            "what came back is not what was written: {back}",
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The two captures, kept as they came off the CLI and scrubbed of ids,
    /// paths and cost. **They are the primary source for every expectation
    /// below** — the shapes here were read out of a real stream rather than
    /// written from what the format ought to do.
    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("the fixture is test material: {}: {e}", path.display()))
    }

    /// **Every call the occupant made, in order, with what it acted on.**
    ///
    /// This is the whole point of the slice: the sitting below reported its
    /// answer and nothing about the two calls behind it.
    #[test]
    fn a_sitting_reports_the_calls_it_made_in_the_order_it_made_them() {
        let made = calls_in(&fixture("made-calls.jsonl"));
        assert_eq!(
            made.iter().map(|c| c.verb.as_str()).collect::<Vec<_>>(),
            vec!["Write", "Read"],
            "the calls, in order: {made:#?}",
        );
        assert!(
            made[0].about.contains("roster.txt"),
            "a call that does not say what it acted on: {:?}",
            made[0],
        );
        assert!(
            made.iter().all(|c| !c.refused),
            "a call the room answered is logged as refused: {made:#?}",
        );
    }

    /// **A sitting that called nothing is not a sitting that was cut off.**
    ///
    /// The real capture settles what nothing else could: the silent stream is
    /// NOT shorter or truncated — it carries the same framing throughout, and
    /// what makes it silent is that no call is in it. So the positive and the
    /// negative are one case: no calls, AND the sitting plainly reached its
    /// end.
    #[test]
    fn a_sitting_that_called_nothing_still_reached_its_end() {
        let silent = fixture("called-nothing.jsonl");
        assert!(
            calls_in(&silent).is_empty(),
            "calls were found in the sitting that made none: {:#?}",
            calls_in(&silent),
        );
        assert!(
            final_text(&silent).is_some(),
            "the silent sitting reads as cut off, which is what a failed capture looks like",
        );
        assert!(
            silent.lines().count() > 10,
            "the silent stream is not a short one, and a check that assumed it was would pass              here for the wrong reason: {} lines",
            silent.lines().count(),
        );
    }

    /// ⚠️ **The stream does not end at `result`.**
    ///
    /// A `system` event arrives after it in a real capture, so a reader that
    /// stops at the result loses whatever follows. **The fixture is asserted to
    /// still have that shape**, because the day it is regenerated without it,
    /// this case would start passing for a reason that has nothing to do with
    /// the parser.
    #[test]
    fn the_final_text_is_found_even_though_more_events_follow_it() {
        let made = fixture("made-calls.jsonl");
        let types: Vec<String> = made
            .lines()
            .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
            .map(|e| e["type"].as_str().unwrap_or_default().to_string())
            .collect();
        let at = types
            .iter()
            .position(|t| t == "result")
            .expect("the capture reached a result");
        assert!(
            at < types.len() - 1,
            "the fixture no longer carries the shape this case exists for: {types:?}",
        );
        assert_eq!(
            final_text(&made).as_deref(),
            Some("Milhouse"),
            "the answer a person reads was not recovered from the stream",
        );
    }

    /// **An answer is cut down, never carried whole.** Some answers in these
    /// rooms are tens of kilobytes, and a log nobody can read is no log.
    #[test]
    fn an_answer_is_kept_only_as_far_as_it_tells_one_call_from_another() {
        let long = "x".repeat(4_000);
        let stream = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"tool_use","id":"id-1","name":"recall","input":{{"handle":"person:milhouse"}}}}]}}}}
{{"type":"user","message":{{"content":[{{"type":"tool_result","tool_use_id":"id-1","content":"{long}"}}]}}}}"#
        );
        let made = calls_in(&stream);
        assert_eq!(made.len(), 1, "one call: {made:#?}");
        assert!(
            made[0].head.chars().count() < 400,
            "the whole answer was kept, so a run's log buries its own story: {} characters",
            made[0].head.chars().count(),
        );
        assert!(
            made[0].about.contains("person:milhouse"),
            "the call no longer says what it acted on: {:?}",
            made[0],
        );
    }

    /// **A refusal is logged as one, and an answer is not.**
    ///
    /// `is_error` is absent on a call that worked — read from the capture, not
    /// assumed — so a reader that treats a missing key as anything but success
    /// marks every good call refused. Both directions in one case.
    #[test]
    fn a_refused_call_reads_apart_from_one_the_room_answered() {
        let stream = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"id-1","name":"capture","input":{"subject":"person:milhouse"}},{"type":"tool_use","id":"id-2","name":"recall","input":{"handle":"person:nelson"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"id-1","content":"blocked: no such subject","is_error":true}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"id-2","content":"one entity"}]}}"#;
        let made = calls_in(stream);
        assert_eq!(made.len(), 2, "both calls: {made:#?}");
        assert!(made[0].refused, "the refusal is not marked: {:?}", made[0]);
        assert!(
            !made[1].refused,
            "an answered call is marked refused, which happens when a missing is_error is read \
             as anything but success: {:?}",
            made[1],
        );
    }
}
