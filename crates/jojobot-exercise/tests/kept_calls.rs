//! **A run keeps what the occupant did, through the path a run actually
//! takes.**
//!
//! The unit cases in `calls` prove the rendering. They hold identically on a
//! build where nothing calls it, so they cannot say a run keeps anything. These
//! send a run down [`Results::write_to`] — the one call `main` makes — and read
//! what is on disk afterwards.

use jojobot_exercise::calls;
use jojobot_exercise::run::{Results, Said};

fn said(phase: &str, output: &str, raw: &str) -> Said {
    Said {
        phase: phase.to_string(),
        prompt: format!("what {phase} was asked"),
        output: output.to_string(),
        raw: raw.to_string(),
        ran: true,
        continuing: false,
        read_this: false,
    }
}

fn run(transcript: Vec<Said>) -> Results {
    Results {
        playbook: "rooms/ledger.md".into(),
        model: "some-model".into(),
        outcomes: Vec::new(),
        transcript,
        uncovered: Vec::new(),
        hatches: Vec::new(),
        boundaries: Vec::new(),
        before: "before".into(),
        after: "after".into(),
    }
}

/// A directory of this case's own, named for the case so two cannot collide.
fn kept_under(case: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("jojobot-kept-{}-{case}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// **Writing a run keeps the stream beside it, and does not touch what the
/// operator reads.**
///
/// Both halves in one case. The readable half is the one that matters most:
/// the slice that adds the stream is the slice most likely to spend the
/// transcript to get it, and a check that only looked at the new file would
/// pass on a build that had traded one for the other.
#[test]
fn writing_a_run_keeps_the_raw_stream_beside_the_transcript_a_person_reads() {
    let dir = kept_under("beside");
    let readable = dir.join("ledger-1.md");
    let events = r#"{"type":"assistant","tool":"capture"}"#;

    run(vec![said(
        "Phase 1 — January",
        "what January answered",
        events,
    )])
    .write_to(&readable)
    .expect("a run is written");

    let prose = std::fs::read_to_string(&readable).expect("the readable transcript is on disk");
    assert!(
        prose.contains("what January answered"),
        "the readable transcript lost what the model said:\n{prose}",
    );
    assert!(
        !prose.contains(events),
        "the raw events were rendered into the file the operator reads:\n{prose}",
    );

    let kept = std::fs::read_to_string(calls::beside(&readable))
        .expect("the raw stream is on disk beside it");
    assert!(
        kept.contains(events),
        "the run kept no events for a sitting that made calls:\n{kept}",
    );
    assert!(
        kept.contains("Phase 1 — January"),
        "the kept stream does not say which sitting the events belong to:\n{kept}",
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// **A sitting that made no calls is in the record, saying so.**
///
/// Three sittings of the first paid run produced nothing. A reader has to be
/// able to tell that from a capture that missed them, and the only thing that
/// can carry the difference is the sitting appearing with nothing under it.
///
/// The positive is in the same case, because an absence proves nothing on its
/// own: a build that wrote an empty file passes the second assertion and fails
/// the first.
#[test]
fn a_sitting_that_called_nothing_is_kept_and_reads_apart_from_one_that_made_calls() {
    let dir = kept_under("silent");
    let readable = dir.join("ledger-2.md");
    let events = r#"{"type":"assistant","tool":"capture"}"#;

    run(vec![
        said("Phase 1 — January", "January answered", events),
        said("Phase 2 — February", "February answered", ""),
    ])
    .write_to(&readable)
    .expect("a run is written");

    let kept =
        std::fs::read_to_string(calls::beside(&readable)).expect("the raw stream is on disk");
    assert!(
        kept.contains("Phase 1 — January") && kept.contains(events),
        "the sitting that made calls is not in the record with them:\n{kept}",
    );
    assert!(
        kept.contains("Phase 2 — February"),
        "the silent sitting is missing, so it reads exactly like a capture that failed:\n{kept}",
    );

    let lines: Vec<&str> = kept.lines().collect();
    let silent = lines
        .iter()
        .position(|l| l.contains("Phase 2 — February"))
        .expect("the silent sitting is named, which the assertion above just held");
    assert_eq!(
        &lines[silent + 1..],
        &[] as &[&str],
        "the silent sitting carried events, so nothing distinguishes it:\n{kept}",
    );

    let _ = std::fs::remove_dir_all(&dir);
}
