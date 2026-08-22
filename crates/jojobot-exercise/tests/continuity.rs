//! **A phase that lost its memory must fail the run, not read as fine.**
//!
//! This is the failure shape the tier is least able to survive: phases that
//! measure what a later session finds are meaningless if the agent silently
//! started over, and a transcript from such a run is perfectly readable. So the
//! detection is structural — the invocation's own status, and a run appearing
//! on the board that a continuing phase had no reason to start — and never a
//! reading of what the agent said.

use jojobot_exercise::run::{Boundary, Outcome, Results, Said};

fn boundary(runs: usize) -> Boundary {
    Boundary {
        before: String::new(),
        mail: String::new(),
        world: String::new(),
        board: String::new(),
        runs_offered: runs,
    }
}

fn said(phase: &str, ran: bool, continuing: bool) -> Said {
    Said {
        phase: phase.to_string(),
        prompt: "say something".into(),
        output: "the agent answered, and it reads perfectly well".into(),
        raw: String::new(),
        ran,
        continuing,
        read_this: false,
    }
}

fn run(transcript: Vec<Said>, offered: &[usize]) -> Results {
    run_with(transcript, offered, Vec::new())
}

/// **A run carrying the outcomes it was given.**
///
/// A case about a HARNESS failure needs the expectations to have HELD, or the
/// verdict is false for a different reason — an empty outcome list is itself a
/// failure — and the assertion proves nothing.
fn run_with(transcript: Vec<Said>, offered: &[usize], outcomes: Vec<Outcome>) -> Results {
    Results {
        playbook: "trivial".into(),
        model: "sonnet".into(),
        outcomes,
        transcript,
        uncovered: Vec::new(),
        hatches: Vec::new(),
        boundaries: offered.iter().map(|n| boundary(*n)).collect(),
        before: "before".into(),
        after: "after".into(),
    }
}

/// The invocation itself came back non-zero on a phase told to carry on.
#[test]
fn a_resume_the_agent_could_not_make_fails_the_phase() {
    let lost = run(
        vec![said("Phase 1", true, false), said("Phase 2", false, true)],
        &[1, 1, 1],
    );
    assert_eq!(
        lost.lost_continuity().len(),
        1,
        "a refused resume was not reported: {:?}",
        lost.lost_continuity(),
    );
    assert!(
        !lost.held(),
        "a run that lost a phase's memory reported a pass"
    );
}

/// **The silent one.** Everything succeeded, the transcript reads well, and a
/// phase that was told to continue started a run of its own — which is what a
/// fresh conversation does when it meets jojobot with no memory.
#[test]
fn a_continuing_phase_that_starts_its_own_run_fails_the_phase() {
    let lost = run(
        vec![said("Phase 1", true, false), said("Phase 2", true, true)],
        &[1, 1, 2],
    );
    assert_eq!(
        lost.lost_continuity().len(),
        1,
        "a phase that quietly started over was not reported: {:?}",
        lost.lost_continuity(),
    );
    assert!(!lost.held());
}

/// The positive both rest on, and it has to be here: a run where every phase
/// carried on reports nothing, and a FRESH phase starting its own run is what
/// a fresh phase is for.
#[test]
fn a_run_that_kept_its_continuity_reports_none() {
    let kept = run(
        vec![
            said("Phase 1", true, false),
            said("Phase 2", true, true),
            said("Phase 3", true, false),
        ],
        &[1, 1, 1, 2],
    );
    assert!(
        kept.lost_continuity().is_empty(),
        "a run that kept its memory was reported as having lost it: {:?}",
        kept.lost_continuity(),
    );
}

/// **A phase that answered one step of six is a harness failure, not a pass.**
///
/// The run that paid for this wrote "Summary 48–53: PASS" after answering step
/// 53 alone. Five steps went unmeasured and the results read clean, because
/// nothing required an answer per step — and the phase's own check failed for
/// a reason nobody could settle from the transcript.
///
/// **Structural: it asks whether the answer names the step**, never what it
/// says about it. So a rephrasing passes and a silence does not.
///
/// Paired with the run that answered every step, which must stay clean —
/// without it this passes against a check that calls every run a failure.
#[test]
fn a_phase_that_skipped_its_steps_is_a_harness_failure() {
    let asked = "48. Add the thing.\n49. Record what it is.\n50. Add another.\n51. Check it in.\n52. Ask what is quiet.\n53. Ask again for another day.";

    let held_outcome = || Outcome {
        name: "a check that held".into(),
        held: true,
        saying: "it held".into(),
    };
    let skipped = run_with(
        vec![Said {
            phase: "Phase 9 — the loop that has gone quiet".into(),
            prompt: asked.into(),
            // The shape the paid run produced: one step answered, the rest
            // swept into a range nobody wrote out.
            output: "53. neither came back. Summary 48-53: PASS".into(),
            raw: String::new(),
            ran: true,
            continuing: false,
            read_this: false,
        }],
        &[1, 1],
        vec![held_outcome()],
    );
    assert_eq!(
        skipped.steps_unanswered().len(),
        1,
        "a phase that answered one step of six reported clean: {:?}",
        skipped.steps_unanswered(),
    );
    assert!(
        !skipped.held(),
        "the run passed while five sixths of a phase went unmeasured",
    );

    let answered = run_with(
        vec![Said {
            phase: "Phase 9 — the loop that has gone quiet".into(),
            prompt: asked.into(),
            output: "48. added it. 49. recorded it. 50. added the second. 51. checked it in. 52. asked. 53. asked again."
                .into(),
            raw: String::new(),
            ran: true,
            continuing: false,
            read_this: false,
        }],
        &[1, 1],
        vec![held_outcome()],
    );
    // **The positive the verdict above rests on**: the same run with every step
    // answered passes, so that failure turned on the unanswered steps rather
    // than on a fixture that could never pass anything.
    assert!(
        answered.held(),
        "a run that answered every step and held every check did not pass",
    );
    assert!(
        answered.steps_unanswered().is_empty(),
        "a phase that answered every step was called a harness failure: {:?}",
        answered.steps_unanswered(),
    );

    // **The neighbouring number must not answer for it.** `5` is not named by
    // `53`, which is the shape the paid run actually produced.
    let neighbour = run(
        vec![Said {
            phase: "Phase 2 — the identity you arrive holding".into(),
            prompt: "5. Say what the charter told you.".into(),
            output: "53. the loop with a cadence came back".into(),
            raw: String::new(),
            ran: true,
            continuing: false,
            read_this: false,
        }],
        &[1, 1],
    );
    assert_eq!(
        neighbour.steps_unanswered().len(),
        1,
        "a longer number answered for the step it contains: {:?}",
        neighbour.steps_unanswered(),
    );
}
