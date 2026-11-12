use super::super::import::supply_identity;
use super::super::tests::{body, object, under};
use super::super::{ClaimEvent, State};

fn made(id: &str, kind: &str, name: &str, handle: &str) -> [ClaimEvent; 2] {
    [
        ClaimEvent::ObjectCreated {
            object: object(id),
            body: body(kind, name),
        },
        ClaimEvent::AddressBound {
            object: object(id),
            address: handle.to_string(),
        },
    ]
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    // A thing is served as an entity of the kind its handle names, and the kind
    // set is the process's.
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

fn handles(rows: &[crate::memory::Entity]) -> Vec<&str> {
    rows.iter().map(|row| row.id.0.as_str()).collect()
}

/// **The stored listing leaves out what the build supplies.** The whole listing
/// holds every object, a supplied record under its supply identity too, because
/// the near-miss screen reads it; the stored listing holds the things somebody
/// stored, so a layer that lays the shipped record on top finds no second copy.
#[test]
fn the_stored_listing_leaves_out_an_object_under_a_supply_identity() {
    let supply = supply_identity("person:milhouse");
    let state = state_of(
        made("k1", "person", "Alpha", "person:alpha")
            .into_iter()
            .chain(made(&supply.0, "person", "Milhouse", "person:milhouse")),
    );
    assert_eq!(
        handles(&state.listing()),
        vec!["person:alpha", "person:milhouse"]
    );
    assert_eq!(handles(&state.stored_listing()), vec!["person:alpha"]);
}

/// **A stored row at a supplied handle is listed as the stored row** (rule 508):
/// the supply identity beside it is left out, the row the operator made stays.
#[test]
fn a_stored_row_at_a_supplied_handle_stays_in_the_stored_listing() {
    let supply = supply_identity("person:milhouse");
    let state = state_of(
        made("k1", "person", "Milly", "person:milhouse")
            .into_iter()
            .chain([ClaimEvent::ObjectCreated {
                object: supply,
                body: body("person", "Milhouse"),
            }]),
    );
    let stored = state.stored_listing();
    assert_eq!(handles(&stored), vec!["person:milhouse"]);
    assert_eq!(stored[0].name, "Milly");
}
