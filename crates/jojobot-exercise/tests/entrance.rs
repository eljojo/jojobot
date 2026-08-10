//! **The entrance, walked through rather than assumed.**
//!
//! Everything under the paid target is tested; the target itself was not, and a
//! tier whose entrance nobody has walked through is the shape this project has
//! already paid for once — a suite that existed and was never run.
//!
//! These run the real binary and cost nothing, because every case here is one
//! that must stop BEFORE a room is built or anything is billed. That is the
//! property being asserted: the refusals are cheap, and they are refusals.

use std::process::Command;

fn run(args: &[&str]) -> (bool, String) {
    let done = Command::new(env!("CARGO_BIN_EXE_jojobot-exercise"))
        .args(args)
        .output()
        .expect("the binary runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&done.stdout),
        String::from_utf8_lossy(&done.stderr),
    );
    (done.status.success(), said)
}

/// **A playbook nobody has written expectations for stops at the door.**
///
/// The case the tier's honesty rests on: a run that asserts nothing must not
/// happen at all, rather than happening and reporting a pass over an empty
/// list. It exits non-zero, and it says what is missing.
#[test]
fn a_playbook_with_no_expectations_refuses_before_anything_is_billed() {
    let playbook = std::env::temp_dir().join("jojobot-entrance-playbook.md");
    std::fs::write(
        &playbook,
        "## Phase 1 — a thing\n\n**Session: fresh.**\n\n> Say something.\n",
    )
    .expect("a playbook on disk");

    let (ok, said) = run(&["--playbook", playbook.to_str().expect("a path")]);
    assert!(!ok, "a run with nothing to assert reported success: {said}");
    assert!(
        said.contains("no expectations are registered"),
        "the refusal must say what is missing: {said}",
    );
    // The positive the refusal rests on: it stopped at the door rather than
    // after doing the expensive half. A room prints its server's log, so a
    // refusal that had built one would carry it.
    assert!(
        !said.contains("listening on"),
        "a room was built before the refusal: {said}",
    );
    let _ = std::fs::remove_file(&playbook);
}

/// The two ways a caller gets the arguments wrong, both non-zero and both
/// naming what to do — paired with the positive that the binary runs at all.
#[test]
fn the_arguments_are_refused_by_name() {
    let (ok, missing) = run(&[]);
    assert!(!ok, "a run with no playbook reported success: {missing}");
    assert!(
        missing.contains("--playbook"),
        "the refusal names the argument it needs: {missing}",
    );

    let (ok, unknown) = run(&["--playbook", "/dev/null", "--nonsense", "x"]);
    assert!(!ok, "an unknown argument was accepted: {unknown}");
    assert!(
        unknown.contains("--nonsense"),
        "the refusal names what it did not understand: {unknown}",
    );
}
