use super::super::tests::{body, object, under};
use super::super::{ClaimEvent, State};

fn made(id: &str, handle: &str) -> [ClaimEvent; 2] {
    [
        ClaimEvent::ObjectCreated {
            object: object(id),
            body: body("person", handle),
        },
        ClaimEvent::AddressBound {
            object: object(id),
            address: handle.to_string(),
        },
    ]
}

fn merged(folded: &str, survivor: &str) -> ClaimEvent {
    ClaimEvent::ObjectsMerged {
        folded: object(folded),
        survivor: object(survivor),
        records: Vec::new(),
        writes: Vec::new(),
    }
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

/// **A survivor keeps the handle it wore, and a folded thing keeps its own.**
/// The folded thing's spellings answer to the survivor after a merge, so a read
/// that served the first spelling of them all would serve a survivor under the
/// handle of the thing folded into it whenever that sorts first.
#[test]
fn a_merge_leaves_each_thing_its_own_handle() {
    let state = state_of(
        made("k1", "person:gamma")
            .into_iter()
            .chain(made("k2", "person:alpha"))
            .chain(made("k3", "person:beta"))
            .chain([merged("k2", "k1"), merged("k1", "k3")]),
    );
    let handle = |id: &str| state.own_handle_of(&object(id)).map(|h| h.0);
    assert_eq!(handle("k2").as_deref(), Some("person:alpha"));
    assert_eq!(handle("k1").as_deref(), Some("person:gamma"));
    assert_eq!(handle("k3").as_deref(), Some("person:beta"));
}
