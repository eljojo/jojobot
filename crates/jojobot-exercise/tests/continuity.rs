//! **A phase that lost its memory must fail the run, not read as fine.**
//!
//! This is the failure shape the tier is least able to survive: phases that
//! measure what a later session finds are meaningless if the agent silently
//! started over, and a transcript from such a run is perfectly readable. So the
//! detection is structural — the invocation's own status, and a run appearing
//! on the board that a continuing phase had no reason to start — and never a
//! reading of what the agent said.

use jojobot_exercise::run::{Boundary, Results, Said};

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
        ran,
        continuing,
    }
}

fn run(transcript: Vec<Said>, offered: &[usize]) -> Results {
    Results {
        playbook: "trivial".into(),
        model: "sonnet".into(),
        outcomes: Vec::new(),
        transcript,
        uncovered: Vec::new(),
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
