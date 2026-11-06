use jiff::civil::date;

use super::super::tests::{body, object, under};
use super::super::{ClaimEvent, State};
use crate::memory::{EntityId, FormerHandle};

fn made(id: &str) -> ClaimEvent {
    ClaimEvent::ObjectCreated {
        object: object(id),
        body: body("person", id),
    }
}

fn bound(id: &str, address: &str) -> ClaimEvent {
    ClaimEvent::AddressBound {
        object: object(id),
        address: address.to_string(),
    }
}

fn retired(id: &str, address: &str, on: jiff::civil::Date) -> ClaimEvent {
    ClaimEvent::AddressRetired {
        object: object(id),
        address: address.to_string(),
        day: on,
    }
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

fn former(handle: &str, badge: &str, on: jiff::civil::Date) -> FormerHandle {
    FormerHandle {
        former: EntityId(handle.to_string()),
        badge: badge.to_string(),
        changed_at: on,
    }
}

/// **The handles things no longer wear, as the port serves them**: each with the
/// badge of the thing that wore it and the day it stopped being current. A handle
/// retired twice, from two things, is two rows, the latest first. A spelling that
/// was never bound, and one retired from a thing that did not hold it, is no row.
#[test]
fn the_retired_spellings_of_the_state_are_the_former_handles_of_the_port() {
    let state = state_of([
        made("k1"),
        made("k2"),
        bound("k1", "person:homer"),
        retired("k1", "person:homer", date(2026, 3, 1)),
        bound("k1", "person:marge"),
        bound("k2", "person:homer"),
        retired("k2", "person:homer", date(2026, 4, 2)),
        retired("k1", "person:lisa", date(2026, 5, 5)),
        retired("k2", "person:marge", date(2026, 5, 6)),
    ]);
    assert_eq!(
        state.former_handles(),
        vec![
            former("person:homer", "k2", date(2026, 4, 2)),
            former("person:homer", "k1", date(2026, 3, 1)),
        ]
    );
}
