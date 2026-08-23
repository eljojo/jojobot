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
        served: Vec::new(),
    }
}

/// A recorded stream, as this crate's own material.
fn fixture(name: &str) -> String {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("the fixture is test material: {}: {e}", path.display()))
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

/// 🚨 **What the occupant DID is in the file a person reads, sitting by
/// sitting.**
///
/// The transcript held what the model said at the end. A sitting that reports
/// it wrote a record and a sitting that wrote nothing rendered the same, and
/// that difference is the whole verdict. The stream beside the transcript held
/// the answer and nothing put it where a reader lands.
///
/// **The positive half is the one a dead parser fails**: the verbs, in the
/// order they were called. A case that only asserted an absence would hold
/// against a build that found no calls at all — which is exactly how a silent
/// sitting reads.
#[test]
fn the_calls_a_sitting_made_are_rendered_in_order() {
    let made = run(vec![said(
        "Phase 1 — one",
        "said something",
        &fixture("made-calls.jsonl"),
    )]);
    let text = made.rendered();
    let write = text
        .find("Write")
        .expect("the first verb it called is in the run");
    let read = text
        .find("Read")
        .expect("the second verb it called is in the run");
    assert!(
        write < read,
        "the calls are not in the order they were made, so the log is not a record of what \
         happened: {text}",
    );
}

/// **A sitting that called nothing says so, and does not read like a capture
/// that failed.**
///
/// Both are silence on the page otherwise, and they mean opposite things: one
/// is an occupant that recorded nothing, the other is a run that lost the
/// material.
#[test]
fn a_sitting_that_called_nothing_reads_apart_from_one_that_did_not_run() {
    let quiet = run(vec![said(
        "Phase 1 — one",
        "said something",
        &fixture("called-nothing.jsonl"),
    )]);
    let silent = quiet.rendered();

    let mut broken = said("Phase 1 — one", "said something", "");
    broken.ran = false;
    let failed = run(vec![broken]).rendered();

    assert_ne!(
        silent, failed,
        "a sitting that made no calls renders exactly as a sitting that never ran",
    );
    assert!(
        silent.contains("no calls"),
        "a sitting that called nothing does not say so: {silent}",
    );
}

/// 🚨 **The run states which verbs were called and which never were.**
///
/// A whole simulated year ran and a verb was never called once. That was found
/// by a person noticing an absence; a tally makes it a fact the run states.
///
/// ⛔️ **Counts and names, and nothing else.** No score and no reading of how
/// the run went: the operator judges a run by reading it, and anything that
/// condenses replaces that judgement with a proxy.
///
/// **Both halves.** A verb that was called is counted — which a dead parser
/// fails — and a verb the room serves and nobody called is named.
#[test]
fn the_run_tallies_the_verbs_called_and_the_verbs_never_called() {
    let mut tallied = run(vec![said(
        "Phase 1 — one",
        "said something",
        &fixture("made-calls.jsonl"),
    )]);
    // Read off the room in a real run; named here because this case is the
    // renderer's and not the room's.
    tallied.served = vec!["Write".into(), "Read".into(), "capture".into()];
    let text = tallied.rendered();

    let tally = text.split("verbs").last().expect("the run renders a tally");
    // ⚠️ **The two halves have to be read apart.** Every served verb appears
    // somewhere in this block whatever happened, so asserting a name is present
    // holds identically against a build that found no calls at all and listed
    // every verb as never called. The split is what makes each half mean
    // something.
    let (called, never) = tally
        .split_once("never called:")
        .expect("the tally names what was never called");
    assert!(
        called.contains("Write") && called.contains("Read"),
        "the verbs the occupant called are not counted as called: {tally}",
    );
    assert!(
        never.contains("capture"),
        "a verb the room serves and nobody called is not named, so an absence is still \
         something a person has to notice: {tally}",
    );
    assert!(
        !never.contains("Write"),
        "a verb that was called is listed as never called: {tally}",
    );
}

/// 🚨 **A verb called the way the model calls it is not reported as never
/// called.**
///
/// The model reaches the room through a connector, so every call arrives
/// prefixed with the server it went to — `mcp__<server>__capture`. The room's
/// own list says `capture`. Compared as written, no verb ever matches, and the
/// run reports every verb the room serves as never called **while counting the
/// same verbs beside it.**
///
/// ⚠️ **It fails in the direction that reads as a finding.** *Sixteen verbs a
/// year never touched* is exactly the alarming, actionable output somebody
/// acts on, and it was fabricated.
///
/// **The fixture is a recording**, cut from a paid run's own stream, because
/// the shape is the whole point: a double that uses the same name on both
/// sides proves nothing about production and is how this shipped.
#[test]
fn a_verb_called_as_the_model_calls_it_is_not_reported_as_never_called() {
    let mut reached = run(vec![said(
        "Phase 1 — one",
        "said something",
        &fixture("called-the-room.jsonl"),
    )]);
    reached.served = vec!["start_here".into(), "capture".into()];
    let text = reached.rendered();
    let tally = text.split("verbs").last().expect("the run renders a tally");
    let (called, never) = tally
        .split_once("never called:")
        .expect("the tally names what was never called");

    assert!(
        called.contains("start_here"),
        "the verb the occupant reached the room with is not counted as called: {tally}",
    );
    assert!(
        !never.contains("start_here"),
        "a verb called through the connector is reported as never called, which invents an \
         absence a reader would act on: {tally}",
    );
    // **The other half.** A fix that empties the list reads as everything was
    // used, which is the same defect pointing the other way.
    assert!(
        never.contains("capture"),
        "a verb the room serves and nobody called is no longer named: {tally}",
    );
}

/// **What the occupant reached for that the room does not serve is counted
/// apart from what it does.**
///
/// A run showed twenty-seven calls to a search tool that is not jojobot's at
/// all. Counted among the room's verbs it says the surface was exercised when
/// the work went somewhere else, and dropped it says nothing happened. **Both
/// answer a different question than the tally claims to.**
#[test]
fn tooling_the_room_does_not_serve_is_counted_apart() {
    let mut mixed = run(vec![said(
        "Phase 1 — one",
        "said something",
        &fixture("made-calls.jsonl"),
    )]);
    mixed.served = vec!["capture".into()];
    let text = mixed.rendered();
    let tally = text.split("verbs").last().expect("the run renders a tally");
    let (room, other) = tally
        .split_once("not the room's surface:")
        .expect("the tally keeps the occupant's own tooling apart");
    assert!(
        other.contains("Write") && other.contains("Read"),
        "the occupant's own tooling is not reported at all, so a run that did its work \
         elsewhere reads as a run that did nothing: {tally}",
    );
    assert!(
        !room.contains("Write"),
        "a tool the room does not serve is counted among the room's verbs: {tally}",
    );
}
