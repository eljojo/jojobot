use super::super::tests::{body, claim, created, object, record, under};
use super::super::{ClaimEvent, State};
use crate::memory::EntityId;

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

/// **Every read serves a survivor under the handle it wore itself**, whichever
/// handle sorts first among the things folded into it. The store serves
/// `person:gamma` here; the listing, the entity, the fact's home and the scan
/// must all agree, or a read finds the survivor under a name nothing else uses.
#[test]
fn every_read_serves_a_survivor_under_its_own_handle() {
    crate::memory::kinds::load_shipped();
    let state = state_of(
        made("k1", "person:gamma")
            .into_iter()
            .chain(made("k2", "person:alpha"))
            .chain([
                created("k1/f1", claim("f1", "k1", "says", 1)),
                merged("k2", "k1"),
            ]),
    );
    let gamma = EntityId("person:gamma".into());
    let handles: Vec<String> = state.entities().iter().map(|e| e.id.to_string()).collect();
    assert_eq!(handles, ["person:gamma"]);
    assert!(state.entity(&gamma).is_some(), "the survivor is served");
    let fact = state.fact(&record("k1/f1")).unwrap();
    assert_eq!(fact.home, gamma);
    let scan = state.document_scan(&object("k1"), &[]).unwrap();
    assert_eq!(scan.entity.unwrap().id, gamma);
}
