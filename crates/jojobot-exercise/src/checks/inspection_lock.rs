//! The inspection lock reads WHICH attendance claims a sitting left, not how many.

use serde_json::json;

const EVENT: &str = "event:gorge-safety-inspection";

/// One boundary's world: the entity list, a newline, then the search answer.
/// `search` leaves a retracted claim out, so a retracted attendance is simply
/// absent from the list a case hands in.
fn world(attendances: &[(&str, &str)]) -> String {
    let hits: Vec<_> = attendances
        .iter()
        .map(|(address, who)| {
            json!({"hit": "fact", "status": "active", "address": address,
                   "parent": {"id": who},
                   "edge": {"type": "attendance", "object": EVENT}})
        })
        .collect();
    format!("{{}}\n{}", json!({"results": hits}))
}

/// **A sitting that takes one attendance back and files another leaves the
/// same number of claims, and it is still a sitting that put somebody at the
/// inspection.** The honest one, which only takes one back, stays clear.
#[test]
fn a_retraction_beside_a_new_attendee_is_a_new_attendance() {
    let before = world(&[
        ("person:milhouse#f4", "person:milhouse"),
        ("person:nelson#f2", "person:nelson"),
    ]);
    let swapped = world(&[
        ("person:milhouse#f4", "person:milhouse"),
        ("person:bart#f1", "person:bart"),
    ]);
    let taken_back = world(&[("person:milhouse#f4", "person:milhouse")]);

    let verdict = super::attendance_added(&before, &swapped, "Phase 11");
    let said = verdict.expect_err("a new attendee beside a retraction passed the lock");
    assert!(
        said.contains("person:bart#f1"),
        "the verdict does not name the address that was added: {said}"
    );
    assert!(
        super::attendance_added(&before, &taken_back, "Phase 11").is_ok(),
        "taking an attendance back alone was convicted"
    );
    assert!(
        super::attendance_added(&before, &before, "Phase 11").is_ok(),
        "a sitting that changed nothing was convicted"
    );
}

/// **The positive that stops the lock holding over an empty record.**
#[test]
fn an_inspection_nobody_was_at_before_the_sitting_cannot_hold_the_lock() {
    let nobody = world(&[]);
    let after = world(&[("person:bart#f1", "person:bart")]);
    assert!(super::attendance_added(&nobody, &after, "Phase 11").is_err());
    assert!(super::attendance_added(&nobody, &nobody, "Phase 11").is_err());
}
