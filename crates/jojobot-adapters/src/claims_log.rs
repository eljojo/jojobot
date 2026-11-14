//! **The claims family's events, as the log stores them.** Claim, field and identity events are
//! the log envelope with a typed payload ([`codec`]). The ledger seam holds the claims
//! [`State`], replays these events into it and stages new ones; this module holds only what the
//! seam calls: the codec, [`apply`] (which warns when a durable log repairs a duplicate handle)
//! and [`report_replay`]. A read is answered from the seam's State and touches no store
//! (decision log 292).

use jojobot_domain::log::claims::{ClaimEvent, State};
use jojobot_domain::log::{Event, Position};

pub mod codec;

#[cfg(test)]
mod codec_tests;

/// The batches one tail read returns, in the independent replay a test builds.
#[cfg(test)]
const READ_BOUND: usize = 256;

/// **Apply an event that sits at `position` in the log.**
pub(crate) fn apply(state: &mut State, event: &Event<ClaimEvent>, position: Position) {
    let applied = state.apply_at(event, position);
    if let Some(former) = applied.replaced_handle
        && let ClaimEvent::AddressBound { object, address } = &event.payload
    {
        tracing::warn!(
            object = %object.0,
            former,
            current = %address,
            duplicate_handle_bindings = state.duplicate_handle_bindings(),
            replacing_event_at = ?state.last_duplicate_handle_binding_at(),
            "a durable claims binding replaced a live handle"
        );
    }
}

/// **The claims State an independent replay of the durable batches builds**: each claims member
/// decoded and applied to an empty State, in log order. It shares the codec and the domain
/// State's apply with the seam, and nothing else: not the seam's staging, its adapter apply, its
/// commit bookkeeping or its recovery, so a State the seam holds wrongly through any of those
/// cannot also be what a test compares it with.
#[cfg(test)]
pub(crate) async fn replayed_from(log: &dyn jojobot_domain::log::LogStore) -> State {
    let mut state = State::default();
    let mut head = Position(0);
    loop {
        let batches = log
            .read_from(head, READ_BOUND)
            .await
            .expect("the durable batches read");
        if batches.is_empty() {
            return state;
        }
        for batch in batches {
            for (index, member) in batch.members().iter().enumerate() {
                if let Some(event) =
                    codec::decode_own(member).expect("a stored claims member reads")
                {
                    let position = Position(batch.range().first.0 + index as u64);
                    let _ = state.apply_at(&event, position);
                }
            }
            head = batch.range().last;
        }
    }
}

pub(crate) fn report_replay(state: &State) {
    tracing::info!(
        duplicate_handle_bindings = state.duplicate_handle_bindings(),
        last_duplicate_handle_binding_at = ?state.last_duplicate_handle_binding_at(),
        "claims replay complete"
    );
}
