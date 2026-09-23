//! **Every shipped room has a needle-hygiene case, named.**
//!
//! `no_lock_here_rests_on_a_needle_that_matches_somewhere_else` is a live
//! check: it works a room to completion and asks every one of its own
//! query-based locks against the room as it actually answers, rather than
//! classifying the locks' text alone the way `standing_gate.rs` does. That
//! liveness is exactly why it cannot be a driver over
//! `expectations::shipped_rooms()` the way the static gates are: driving a
//! room to completion is bespoke per room — bike, loop, year, vault and
//! handover each furnish and work their own document by a different call
//! sequence, and nothing here plays a room the way a real occupant would
//! for free (`run::go` does, but it drives a real paid model).
//!
//! **So this is a named allowlist, not a driver** — the shape
//! `standing_gate.rs` already uses for the same reason: a room added to
//! `expectations::ROOMS` without its own case here is a gap nobody wrote a
//! sentence about, and this is the gate that catches it mechanically rather
//! than trusting the next person to remember.

/// **Every room this build ships with its own
/// `no_lock_here_rests_on_a_needle_that_matches_somewhere_else` case,
/// named.** Add a room's case first; add its name here only once that case
/// is actually green — this list is a receipt, never a promise.
const COVERED: &[&str] = &[
    jojobot_exercise::expectations::BIKE_ROOM,
    jojobot_exercise::expectations::LOOP_ROOM,
    jojobot_exercise::expectations::YEAR_ROOM,
    jojobot_exercise::expectations::HANDOVER_ROOM,
    jojobot_exercise::expectations::DECISIONS_ROOM,
    jojobot_exercise::expectations::VAULT_ROOM,
];

/// **A room named here on purpose, without a green case, and why.** Not a
/// second `COVERED` — a room moves here only when its case is written and
/// red for a real, understood reason, never as a quiet way to skip writing
/// one at all. An entry with no reason is a defect in this list, the same
/// floor `standing_gate.rs`'s own allowlist holds.
struct Pending<'a> {
    room: &'a str,
    reason: &'a str,
}

/// **Empty on purpose.** `VAULT_ROOM` moved to `COVERED` once seven of its
/// nine ambiguous needles were scoped to their own sitting; the two that
/// cannot be — `overdue_excluded` and `archived_excluded`, both envelope
/// matches — are named as accepted exceptions inside `vault_room.rs`'s own
/// case rather than kept here, because they are not gaps in coverage: the
/// case is written, green, and watching for anything new.
const PENDING: &[Pending<'static>] = &[];

/// **A shipped room with no named entry, covered or pending, is a room
/// nobody watched for needle hygiene at all.** This does not run the
/// missing case — it cannot, since driving a room is bespoke — it says
/// plainly that one is owed, or that its gap is at least named.
#[test]
fn every_shipped_room_has_needle_hygiene_coverage_named() {
    for room in jojobot_exercise::expectations::shipped_rooms() {
        let pending = PENDING.iter().any(|p| p.room == room);
        assert!(
            COVERED.contains(&room) || pending,
            "{room} ships with no needle-hygiene coverage named here, covered or pending. Add \
             its own no_lock_here_rests_on_a_needle_that_matches_somewhere_else case in its own \
             test file, then list {room} in COVERED — or in PENDING with a real reason if it is \
             written and not yet green.",
        );
    }
}

/// **A pending entry with no real reason is the allowlist's own failure
/// mode**, the same one `standing_gate.rs` guards against: a shrug is easy
/// to write and easy to miss reading over.
#[test]
fn every_pending_entry_has_a_substantive_reason() {
    for entry in PENDING {
        assert!(
            entry.reason.split_whitespace().count() > 4,
            "{}: a pending room's reason must say something, not stand in for one",
            entry.room,
        );
    }
}

/// **The other direction, so the list cannot rot quietly.** A name that is
/// not a shipped room is either a typo or a room the build stopped
/// shipping — either way, nothing was actually watching what this list
/// claims to.
#[test]
fn every_named_entry_is_a_room_this_build_still_ships() {
    let shipped: Vec<&str> = jojobot_exercise::expectations::shipped_rooms().collect();
    for name in COVERED {
        assert!(
            shipped.contains(name),
            "{name} is listed as covered but is not a shipped room — a typo, or a room this \
             build no longer ships",
        );
    }
}
