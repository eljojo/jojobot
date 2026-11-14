use super::super::tests::{body, object, stamped};
use super::super::{ClaimEvent, ObjectBody, State};
use crate::memory::EntityId;

fn state_of(things: &[(&str, &str)], merges: &[(&str, &str)]) -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    let mut events = Vec::new();
    for (kind, name) in things {
        events.push(ClaimEvent::ObjectCreated {
            object: object(name),
            body: ObjectBody { ..body(kind, name) },
        });
        events.push(ClaimEvent::AddressBound {
            object: object(name),
            address: (*name).to_string(),
        });
    }
    for (folded, survivor) in merges {
        events.push(ClaimEvent::ObjectsMerged {
            folded: object(folded),
            survivor: object(survivor),
            records: Vec::new(),
            writes: Vec::new(),
        });
    }
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    state
}

fn ids(names: &[&str]) -> Vec<EntityId> {
    names.iter().map(|name| EntityId((*name).into())).collect()
}

/// **A chain of merges leaves one thing a browse offers, and it is declined**: A
/// folded into B and B into C, so only C is offered, and C was merged into. The
/// folded things are no thing a holder query is asked about.
#[test]
fn a_chain_of_merges_declines_its_last_survivor() {
    let state = state_of(
        &[
            ("work", "work:alpha"),
            ("work", "work:beta"),
            ("work", "work:delta"),
        ],
        &[("work:alpha", "work:beta"), ("work:beta", "work:delta")],
    );
    assert_eq!(state.declined_among(None), ids(&["work:delta"]));
}

/// **A role merged twice into one thing declines the survivor, and no role**: the
/// roles are folded away, so the survivor is the one thing the store may hold a
/// renewed lease moment for.
#[test]
fn a_role_merged_twice_declines_the_survivor() {
    let state = state_of(
        &[
            ("role", "role:gamma"),
            ("role", "role:delta"),
            ("person", "person:homer"),
            ("person", "person:marge"),
        ],
        &[
            ("role:gamma", "person:homer"),
            ("role:delta", "person:homer"),
        ],
    );
    assert_eq!(state.declined_among(None), ids(&["person:homer"]));
}

/// **A role folded into a role declines the surviving role**, by either rule: it
/// holds a lease and a merge touched it. A role no merge touched is declined for its
/// lease alone, and a thing no merge touched is not declined.
#[test]
fn a_role_merged_into_a_role_declines_the_survivor() {
    let state = state_of(
        &[
            ("role", "role:gamma"),
            ("role", "role:delta"),
            ("role", "role:epsilon"),
            ("person", "person:homer"),
        ],
        &[("role:gamma", "role:delta")],
    );
    assert_eq!(
        state.declined_among(None),
        ids(&["role:delta", "role:epsilon"])
    );
}
