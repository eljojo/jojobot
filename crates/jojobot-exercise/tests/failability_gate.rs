//! **Every shipped lock is named, before alternation reaches the format.**
//!
//! `jojobot_exercise::failability` is the registry; nothing walked it against
//! the shipped rooms until this file. A registry nobody calls is a list
//! somebody wrote by hand once; a gate is a standing case that walks
//! `expectations::shipped_rooms()` the same way `reads_the_rooms.rs` and
//! `standing_gate.rs` do, and fails the moment a lock ships that neither list
//! accounts for.
//!
//! **This is a backlog gate, not a risk-acceptance one.** `standing_gate.rs`'s
//! `ALLOWED` names exposures this build is choosing to live with.
//! `failability::PENDING` names locks nobody has proven failable yet — owed
//! work, not a judgement call — so it carries no per-entry justification the
//! way `ALLOWED` does: the reason every entry is there is the same reason,
//! stated once in the module doc, and repeating it 128 times would be the
//! shrug that check exists to catch, not the sentence it wants.

use jojobot_exercise::failability;

/// **Every lock in every shipped room is named** — by a registered negative
/// control, or by the pending backlog admitting none exists yet. A lock
/// ships in neither list only when nobody has decided anything about
/// whether it can fail, which is the gap this gate exists to close.
#[test]
fn every_shipped_lock_is_named() {
    for (room, name) in failability::shipped_locks() {
        assert!(
            failability::is_named(
                &room,
                &name,
                failability::NEGATIVE_CONTROLS,
                failability::PENDING,
            ),
            "{room}: {name:?} is named by neither a registered negative control nor the pending \
             backlog. Write the control and register it, or add this lock to \
             failability::PENDING.",
        );
    }
}

/// **A pending entry has to still name a real, shipped lock.** An entry
/// naming a lock that was renamed or removed since is a snoozed alarm for a
/// gap that closed on its own — the same rot `standing_gate.rs`'s
/// `every_named_exposure_still_reproduces` holds against its own list,
/// checked here because `PENDING` is the one list in this file that is not
/// empty yet.
#[test]
fn every_pending_entry_still_names_a_shipped_lock() {
    let shipped = failability::shipped_locks();
    for pending in failability::PENDING {
        assert!(
            shipped
                .iter()
                .any(|(room, name)| room == pending.room && name == pending.lock),
            "{}: {:?} is on the pending backlog but is not a lock any shipped room carries any \
             more — remove the stale entry.",
            pending.room,
            pending.lock,
        );
    }
}

/// **A registered control's proof has to still name something real.** An
/// entry naming a file or a function that was renamed or removed since is a
/// dangling pointer that rots in silence — the same class of rot the check
/// above holds against `PENDING`, checked here for `NEGATIVE_CONTROLS`. This
/// checks only that the file and the function still exist; it never runs
/// the function — see `failability`'s own module doc for why.
#[test]
fn every_registered_controls_proof_still_exists() {
    for control in failability::NEGATIVE_CONTROLS {
        assert!(
            failability::proof_exists(control).is_ok(),
            "{}/{:?}: {}",
            control.room,
            control.lock,
            failability::proof_exists(control).unwrap_err(),
        );
    }
}

/// **The registry's two populations, reported apart — never as one
/// number.** A blanket proof (the lock notices an unworked room) and a
/// discriminating one (the lock notices one specific wrong act, with
/// everything else right) answer different questions. Collapsing them into
/// a single count would say "N locks are proven" and leave a reader with
/// no way to tell which bar was actually cleared — which is exactly the
/// number a `NEGATIVE_CONTROLS.len()` would report if this pinned that
/// instead.
#[test]
fn the_registry_reports_blanket_and_discriminating_counts_apart() {
    let (blanket, discriminating) = failability::tally(failability::NEGATIVE_CONTROLS);
    assert_eq!(
        blanket, 62,
        "the blanket population drifted without this test noticing",
    );
    assert_eq!(
        discriminating, 4,
        "the discriminating population drifted without this test noticing",
    );
}
