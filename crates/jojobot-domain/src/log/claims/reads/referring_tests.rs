use super::super::tests::{body, key, object, record, under};
use super::super::{ClaimEvent, FieldValue, State};
use crate::memory::{EntityId, WriteClass};

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

/// One claim homed on `home`, written under the label `f1`.
fn claim_on(home: &str, id: &str) -> ClaimEvent {
    ClaimEvent::RecordCreated {
        record: record(id),
        claim: super::super::tests::claim("f1", home, "a claim", 1),
    }
}

/// The record `id` writes `value` on `subject` under `k`, pointing at `targets`.
fn writes(
    id: &str,
    subject: &str,
    k: &str,
    ordinal: u64,
    value: FieldValue,
    targets: &[&str],
) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object(subject),
        key: key(k),
        ordinal,
        class: WriteClass::Stays,
        write: value,
        targets: targets.iter().map(|t| object(t)).collect(),
    }
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

fn holders(state: &State, target: &str) -> Vec<(String, String)> {
    state
        .referring(&EntityId(target.to_string()))
        .iter()
        .map(|fact| (fact.home.0.clone(), fact.id.0.clone()))
        .collect()
}

/// **The records that point at a thing through a field, whatever key they used**:
/// the record whose newest write of a key names the thing, a key that stays on
/// its record included, and none once a later write takes the key off. A thing
/// folded into another is pointed at through its survivor.
#[test]
fn the_records_pointing_at_a_thing_are_the_records_whose_newest_write_names_it() {
    let base = made("k1", "person:alpha")
        .into_iter()
        .chain(made("k2", "person:beta"))
        .chain(made("k3", "person:gamma"))
        .chain([claim_on("k1", "k1/f1"), claim_on("k3", "k3/f1")]);
    let pointing = base.clone().chain([
        writes(
            "k1/f1",
            "k1",
            "spouse",
            1,
            FieldValue::Set("@#k2".into()),
            &["k2"],
        ),
        writes(
            "k3/f1",
            "k3",
            "friend",
            1,
            FieldValue::Set("@#k2".into()),
            &["k2"],
        ),
    ]);
    let state = state_of(pointing.clone());
    assert_eq!(
        holders(&state, "person:beta"),
        vec![
            ("person:alpha".to_string(), "f1".to_string()),
            ("person:gamma".to_string(), "f1".to_string())
        ]
    );
    assert!(holders(&state, "person:alpha").is_empty());

    let cleared = state_of(pointing.clone().chain([writes(
        "k1/f1",
        "k1",
        "spouse",
        2,
        FieldValue::Clear,
        &[],
    )]));
    assert_eq!(
        holders(&cleared, "person:beta"),
        vec![("person:gamma".to_string(), "f1".to_string())]
    );

    let merged = state_of(pointing.chain([ClaimEvent::ObjectsMerged {
        folded: object("k2"),
        survivor: object("k1"),
        records: Vec::new(),
        writes: Vec::new(),
    }]));
    assert_eq!(holders(&merged, "person:alpha").len(), 2);
}

/// **A holder a merge left unmapped has no served address**, and the survivor's
/// own claim under the same label is a different claim: it is not pointing at
/// the thing, and the unmapped one is not served in its place.
#[test]
fn an_unmapped_holder_is_not_served_at_the_address_of_the_survivors_own_claim() {
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .chain([
                claim_on("k1", "k1/f1"),
                claim_on("k3", "k3/f1"),
                writes(
                    "k3/f1",
                    "k3",
                    "friend",
                    1,
                    FieldValue::Set("@#k2".into()),
                    &["k2"],
                ),
                ClaimEvent::ObjectsMerged {
                    folded: object("k3"),
                    survivor: object("k1"),
                    records: Vec::new(),
                    writes: Vec::new(),
                },
            ]),
    );
    assert!(holders(&state, "person:beta").is_empty());
}
