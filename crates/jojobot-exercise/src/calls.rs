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
}
