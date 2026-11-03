//! **What a claim may not grow past**: the seats of a bot's rules, the floor of
//! its boot and the one answer a read of the thing serves.
//!
//! The sizes come from the door. Rendering a claim as a read serves it, or a
//! bot's boot, belongs to the served text, so the domain does not render
//! anything twice: it is handed what the server measured and decides "at most
//! the room". A size the door could not measure is not a size known to be over,
//! and it passes.

use super::super::{ObjectId, StarredRule, State};
use super::{CaptureRequest, Refusal};
use crate::memory::{self, MemoryError, RULE_SEATS, seats, types::DeclaredType};

/// **The claim as a read would serve it, and the room one answer has for it.**
/// `row` is the thing's own row and the room one answer has for it, when the row
/// alone does not fit: then no claim on the thing can, and the cause is the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimSize {
    pub size: usize,
    pub room: usize,
    pub row: Option<(usize, usize)>,
}

/// **A bot's boot floor, measured with the claim and without it.** `stamp_margin`
/// is what the measure kept back for the moment the store will stamp on a
/// starred rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BootFloor {
    pub after: usize,
    pub before: usize,
    pub parts: Vec<(String, usize)>,
    pub stamp_margin: usize,
}

/// **What the door measured for the request.** `None` is a size nobody could
/// read.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Measures {
    pub claim: Option<ClaimSize>,
    pub boot_floor: Option<BootFloor>,
}

fn is_a_bot(state: &State, home: &ObjectId) -> bool {
    state.object(home).is_some_and(|body| body.kind.0 == "bot")
}

fn is_starred(request: &CaptureRequest) -> bool {
    request.fields.get("starred").is_some_and(|v| v == "true")
}

/// **A star into full seats is refused.** A thought is no rule and takes no
/// seat, and a bot is never refused for a star it does not write.
pub(super) fn refuses_a_star(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<Refusal> {
    let connection = request
        .edge
        .as_ref()
        .is_some_and(|(shape, _)| *shape == memory::EdgeShape::Connection);
    if !is_a_bot(state, home) || !is_starred(request) || connection {
        return None;
    }
    let held: Vec<StarredRule> = state.starred_rules(home);
    let seats = memory::rule_seats_of(&state.fields(home, declared));
    seats::star_has_no_seat(seats, held.len()).then(|| Refusal::SeatsFull {
        subject: subject_of(state, home, request),
        seats,
        starred: held.len(),
        seated: held,
    })
}

/// **A write that grows a bot's boot past what a boot can carry.** Asked only of
/// a write that could grow the floor: a star, or a seat count. A bot naming its
/// own seat count never gets here: the licence of a guarded key refuses it first.
pub(super) fn refuses_a_boot_floor(
    state: &State,
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<MemoryError> {
    if !is_a_bot(state, home) {
        return None;
    }
    let starred = is_starred(request);
    let seats = request.fields.get(RULE_SEATS);
    if !starred && seats.is_none() {
        return None;
    }
    if let Err(refused) = seats::seat_count_written(seats.map(String::as_str)) {
        return Some(refused);
    }
    let floor = request.door.measures.boot_floor.as_ref()?;
    let budget = crate::text::BOOT_ANSWER.budget;
    (floor.after > budget && floor.after > floor.before).then(|| MemoryError::BootTooHeavy {
        subject: subject_of(state, home, request),
        floor: floor.after,
        budget,
        parts: floor.parts.clone(),
        stamp_margin: if starred { floor.stamp_margin } else { 0 },
        purpose_cap: None,
    })
}

/// **A claim no reader could receive whole** is refused, instead of storing what
/// one answer cannot carry.
pub(super) fn refuses_a_claim_too_big(
    state: &State,
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<MemoryError> {
    let measured = request.door.measures.claim.as_ref()?;
    if measured.size <= measured.room {
        return None;
    }
    let subject = subject_of(state, home, request);
    Some(match measured.row {
        Some((size, room)) => MemoryError::ThingTooLarge {
            subject,
            size,
            room,
        },
        None => MemoryError::ClaimTooLarge {
            subject,
            size: measured.size,
            room: measured.room,
        },
    })
}

fn subject_of(state: &State, home: &ObjectId, request: &CaptureRequest) -> String {
    state
        .handle_of(home)
        .map_or_else(|| request.subject.clone(), |handle| handle.to_string())
}
