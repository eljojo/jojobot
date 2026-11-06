use super::super::tests::{body, claim, object, record, under};
use super::super::{ClaimBody, ClaimEvent, MovedRecord, State};
use crate::memory::{EntityId, FactAddress, FactId};

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

/// The claim `id`, labelled `label` under `home`, worked out from `from`.
fn created(id: &str, label: &str, home: &str, from: Option<&str>) -> ClaimEvent {
    ClaimEvent::RecordCreated {
        record: record(id),
        claim: ClaimBody {
            derived_from: from.map(record),
            ..claim(label, home, "a claim", 1)
        },
    }
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

fn standing_on(state: &State, home: &str, local: &str) -> Vec<(String, String)> {
    state
        .built_on(&FactAddress::new(
            EntityId(home.to_string()),
            FactId(local.to_string()),
        ))
        .iter()
        .map(|fact| (fact.home.0.clone(), fact.id.0.clone()))
        .collect()
}

/// **The claims worked out from a claim**, of every status, in the order of their
/// addresses. A claim names its source by identity, so a source that a merge
/// moved under another handle and label is still found at its new address.
#[test]
fn the_claims_built_on_a_claim_are_the_claims_that_name_it_as_their_source() {
    let base = made("k1", "person:alpha")
        .into_iter()
        .chain(made("k2", "person:beta"))
        .chain([
            created("k1/f1", "f1", "k1", None),
            created("k2/f1", "f1", "k2", Some("k1/f1")),
            created("k1/f2", "f2", "k1", Some("k1/f1")),
            created("k2/f2", "f2", "k2", Some("k2/f1")),
        ]);
    let state = state_of(base.clone());
    assert_eq!(
        standing_on(&state, "person:alpha", "f1"),
        vec![
            ("person:alpha".to_string(), "f2".to_string()),
            ("person:beta".to_string(), "f1".to_string()),
        ]
    );
    assert_eq!(
        standing_on(&state, "person:beta", "f1"),
        vec![("person:beta".to_string(), "f2".to_string())]
    );

    // The source's claim moves to the survivor under a new label, and what was
    // built on it is found at the new address.
    let moved = state_of(base.chain([ClaimEvent::ObjectsMerged {
        folded: object("k2"),
        survivor: object("k1"),
        records: vec![MovedRecord {
            record: record("k2/f1"),
            label: "f7".into(),
        }],
        writes: Vec::new(),
    }]));
    assert_eq!(
        standing_on(&moved, "person:alpha", "f7"),
        vec![("person:alpha".to_string(), "f2".to_string())]
    );
}
