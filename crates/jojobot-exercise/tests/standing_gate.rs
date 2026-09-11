//! **The classifier as a gate, not a survey.**
//!
//! `lock::standing_findings` classifies every needle in every shipped lock
//! without asking a room anything — but nothing ran it over the shipped
//! rooms. A classifier nobody calls is a survey somebody ran by hand once; a
//! gate is a standing case that walks `expectations::shipped_rooms()` the
//! same way `reads_the_rooms.rs` does, and fails when a needle nobody has
//! named appears.
//!
//! **The known exposures are a named allowlist, never a count.** A count
//! says *eleven is fine*; a named list says *these eleven, for these
//! reasons* — so a twelfth cannot pass without somebody writing a sentence,
//! and that sentence is the review moment a pinned count would have snoozed
//! through. An entry with no real reason is a defect in the allowlist, not
//! an exemption — `every_allowlist_entry_has_a_substantive_reason` holds
//! that floor mechanically, because a shrug is easy to write and easy to
//! miss reading over eleven of them.
//!
//! **The list also has to stay honest the other direction.** An entry
//! nobody's finding matches any more is a snoozed alarm for a lock that was
//! fixed — the same rot a pinned count invites, just facing the other way —
//! so `every_named_exposure_still_reproduces` holds that a listed entry is
//! never allowed to go quiet on its own.

use jojobot_exercise::lock::{self, Risk, StandingFinding};

/// One needle this build already knows is exposed, and why it still is.
struct Allowed<'a> {
    room: &'a str,
    lock: &'a str,
    needle: &'a str,
    risk: Risk,
    reason: &'a str,
}

/// **Every exposure this build ships, named.** Add a lock's own companion
/// instead of an entry here whenever that is the real fix — an entry is for
/// the exposures that are staying, with the sentence that says why.
const ALLOWED: &[Allowed<'static>] = &[Allowed {
    room: "rooms/year.md",
    lock: "Phase 9 — nothing on the pump currently carries the day it came back, so a \
               reader is left with no day to find — whether it was never recorded, or a later, \
               legitimate correction cleared the only trace of it",
    needle: "person:ralph",
    risk: Risk::Retraction,
    reason: "the historical instance the classifier was built to catch. October corrects \
                 this record in place rather than retracting it, so the honest play never trips \
                 it — but the lock still carries a bare person:ralph with no companion, so a \
                 future sitting that retracted Ralph's account instead of correcting it would \
                 satisfy this lock on dead text exactly as the original bug did",
}];

/// The findings a room's shipped locks classify to.
fn found_in(room: &str) -> Vec<StandingFinding> {
    lock::standing_findings(&lock::locks_of(room))
}

/// **Whether a finding is named, against a given allowlist.** Taking the
/// list as a parameter rather than reading the constant is what lets the
/// mechanism be proven on its own, synthetic data — see the last test below.
fn is_allowed(room: &str, finding: &StandingFinding, allowed: &[Allowed<'_>]) -> bool {
    allowed.iter().any(|a| {
        a.room == room
            && a.lock == finding.lock
            && a.needle == finding.needle
            && a.risk == finding.risk
    })
}

/// **Every needle the classifier flags in a shipped room must be named.** A
/// finding nobody listed is a lock that got more satisfiable without anybody
/// writing a sentence about it — the gate this file exists to be.
#[test]
fn every_standing_exposure_in_a_shipped_room_is_named() {
    for room in jojobot_exercise::expectations::shipped_rooms() {
        for finding in found_in(room) {
            assert!(
                is_allowed(room, &finding, ALLOWED),
                "{room}: a standing exposure is not on the allowlist — {:?} / {:?} ({:?}). \
                 Give the lock a status companion to close it, or add a named entry here with a \
                 real reason.",
                finding.lock,
                finding.needle,
                finding.risk,
            );
        }
    }
}

/// **Every named entry must still reproduce.** An entry nobody's finding
/// matches any more is a snoozed alarm for a lock that was fixed — the
/// allowlist rotting the other direction from the one above.
#[test]
fn every_named_exposure_still_reproduces() {
    for allowed in ALLOWED {
        let found = found_in(allowed.room);
        assert!(
            found.iter().any(|f| f.lock == allowed.lock
                && f.needle == allowed.needle
                && f.risk == allowed.risk),
            "{}: the allowlist names an exposure that no longer reproduces — {:?} / {:?} \
             ({:?}). Remove the stale entry.",
            allowed.room,
            allowed.lock,
            allowed.needle,
            allowed.risk,
        );
    }
}

/// **A named entry needs a real reason, not a shrug.** The allowlist's whole
/// point is that a caller has to write a sentence to add one; a threshold on
/// length is a cheap floor under that, not a judge of quality.
#[test]
fn every_allowlist_entry_has_a_substantive_reason() {
    for allowed in ALLOWED {
        assert!(
            allowed.reason.split_whitespace().count() >= 12,
            "{}/{}: the reason is too thin to be a reason: {:?}",
            allowed.lock,
            allowed.needle,
            allowed.reason,
        );
    }
}

/// **The gate itself, proven on a lock nothing shipped writes.** A bare
/// handle with no matching allowlist entry must fail, and the identical
/// finding with a matching entry must clear — proven against a synthetic
/// lock and a synthetic allowlist rather than by editing a shipped room, so
/// the mechanism is what is under test rather than today's eleven.
#[test]
fn an_unnamed_exposure_fails_the_gate_and_naming_it_clears_it() {
    let locks = lock::read(
        "```locks\n\
         recall {\"subject\": \"person:homer\", \"facts\": true}\n\
         carries person:homer\n\
         say     homer is not on the roster\n\
         ```\n",
    )
    .expect("the lock reads");
    let found = lock::standing_findings(&locks);
    assert_eq!(
        found.len(),
        1,
        "the synthetic lock must classify to exactly one finding"
    );

    let planted_room = "not-a-shipped-room";
    assert!(
        !is_allowed(planted_room, &found[0], &[]),
        "an exposure checked against an empty allowlist read as named — the gate would pass \
         anything",
    );

    let named = [Allowed {
        room: planted_room,
        lock: &found[0].lock,
        needle: &found[0].needle,
        risk: found[0].risk,
        reason: "planted for the gate's own proof, not a real exposure",
    }];
    assert!(
        is_allowed(planted_room, &found[0], &named),
        "naming the exact finding did not clear it against its own allowlist",
    );
}
