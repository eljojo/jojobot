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
use crate::actor::Actor;
use crate::memory::{
    self, EntityKind, MemoryError, RULE_SEATS, THOUGHT_BODY_CAP, THOUGHT_CAPACITY, capacity, seats,
    types::DeclaredType,
};

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

/// **A write to a role object is judged against the lease the role holds.** The
/// lease is read off the role's own fields, so the renewal itself stays a
/// heartbeat off the log and this only reads it.
pub(super) fn refuses_a_role_write(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<MemoryError> {
    let role = memory::role_named_by(&state.handle_of(home)?)?;
    let lease = crate::session::role_state(Some(&state.fields(home, declared)));
    memory::refuses_role_write(&role, None, &request.fields, &lease)
}

/// **Whether another bot wrote the claim**: the bot the actor is, else the bot
/// the writing run belongs to, when that bot is not the thing it writes on.
/// A claim no bot is known to have written is the thing's own.
fn written_by_another(state: &State, home: &ObjectId, request: &CaptureRequest) -> bool {
    let writer = match &request.door.actor {
        Actor::Bot(bot) => Some(bot),
        _ => request
            .door
            .run
            .as_ref()
            .and_then(|run| request.door.runs.get(run)),
    };
    writer
        .and_then(|bot| state.resolve(bot.as_str()))
        .is_some_and(|bot| bot != state.survivor(home))
}

/// **A thought is a claim the thing keeps of itself**: it draws a `connection`
/// edge and no other bot wrote it.
fn is_a_thought(state: &State, home: &ObjectId, request: &CaptureRequest) -> bool {
    request
        .edge
        .as_ref()
        .is_some_and(|(shape, _)| *shape == memory::EdgeShape::Connection)
        && !written_by_another(state, home, request)
}

fn whole(held: &std::collections::BTreeMap<String, String>, key: &str) -> Option<usize> {
    held.get(key).and_then(|value| value.trim().parse().ok())
}

/// **A thought over its container's own body cap is refused**, whether or not
/// the room has space.
pub(super) fn refuses_a_thought_over_cap(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<MemoryError> {
    if !is_a_thought(state, home, request) {
        return None;
    }
    let held = state.fields(home, declared);
    memory::refuses_thought_over_cap(
        &state.handle_of(home)?,
        &memory::normalize_content(&request.draft.content),
        whole(&held, THOUGHT_BODY_CAP),
        whole(&held, THOUGHT_CAPACITY),
    )
}

/// **The record ceiling of a thing**, decided over the live records it holds. A
/// write that adds none (the review mark, a record about an act of jojobot's own)
/// is exempt, and a thing already over its ceiling admits nothing that adds.
pub(super) fn refuses_over_the_ceiling(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<Refusal> {
    let kind = state
        .object(home)
        .and_then(|body| EntityKind::from_token(&body.kind.0));
    let held = state.fields(home, declared);
    let ceiling = capacity::ceiling_of(kind, &held)?;
    let adding = capacity::slots_taken(&request.fields);
    if adding == 0 {
        return None;
    }
    let live = state.live_records(home);
    match capacity::fits(ceiling.limit, live.len(), adding, request.borrow) {
        capacity::Fits::Yes | capacity::Fits::OnTheReserve => None,
        capacity::Fits::No => Some(Refusal::OverCapacity {
            subject: subject_of(state, home, request),
            limit: ceiling.limit,
            adding,
            from_setting: ceiling.from == capacity::CeilingFrom::Setting,
            records: live,
        }),
    }
}
