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

/// **Holders come in the order of the key their records are filed under**, which
/// a rename does not change, and not in the order of the handles they wear now.
#[test]
fn holders_are_ordered_by_their_key_and_not_by_their_handle() {
    let state = state_of(
        made("k1", "person:omega")
            .into_iter()
            .chain(made("k2", "person:alpha"))
            .chain(made("k3", "person:gamma"))
            .chain([
                claim_on("k1", "k1/f1"),
                claim_on("k2", "k2/f1"),
                writes(
                    "k1/f1",
                    "k1",
                    "friend",
                    1,
                    FieldValue::Set("@#k3".into()),
                    &["k3"],
                ),
                writes(
                    "k2/f1",
                    "k2",
                    "friend",
                    1,
                    FieldValue::Set("@#k3".into()),
                    &["k3"],
                ),
            ]),
    );
    assert_eq!(
        holders(&state, "person:gamma"),
        vec![
            ("person:omega".to_string(), "f1".to_string()),
            ("person:alpha".to_string(), "f1".to_string())
        ]
    );
}

/// **Holders are listed by the key their records are served under**, which is the
/// survivor's for a claim a merge moved, and a holder's own records by the number
/// in their labels. The moved claim was made under `k9`, a key that sorts after
/// `k3`; served under `k1` it comes first. **Paired**: before the merge the same
/// claim comes after `k3`, so the order moved with the merge and not by chance.
#[test]
fn holders_follow_the_key_they_are_served_under_and_their_label_numbers() {
    let base = made("k1", "person:alpha")
        .into_iter()
        .chain(made("k9", "person:beta"))
        .chain(made("k3", "person:gamma"))
        .chain(made("k4", "person:delta"))
        .chain([
            claim_on("k9", "k9/f1"),
            claim_on("k3", "k3/f1"),
            writes(
                "k9/f1",
                "k9",
                "friend",
                1,
                FieldValue::Set("@#k4".into()),
                &["k4"],
            ),
            writes(
                "k3/f1",
                "k3",
                "friend",
                1,
                FieldValue::Set("@#k4".into()),
                &["k4"],
            ),
        ]);
    let before = state_of(base.clone());
    assert_eq!(
        holders(&before, "person:delta"),
        vec![
            ("person:gamma".to_string(), "f1".to_string()),
            ("person:beta".to_string(), "f1".to_string()),
        ],
        "made under k9, the claim is served after k3's"
    );
    let merged = state_of(base.chain([ClaimEvent::ObjectsMerged {
        folded: object("k9"),
        survivor: object("k1"),
        records: vec![super::super::MovedRecord {
            record: record("k9/f1"),
            label: "f1".to_string(),
        }],
        writes: Vec::new(),
    }]));
    assert_eq!(
        holders(&merged, "person:delta"),
        vec![
            ("person:alpha".to_string(), "f1".to_string()),
            ("person:gamma".to_string(), "f1".to_string()),
        ],
        "served under the survivor k1, the moved claim comes before k3's"
    );
}

/// A holder's own records are listed by the number in their labels: the tenth
/// follows the ninth.
#[test]
fn a_holders_records_are_listed_by_the_number_in_their_labels() {
    let mut events: Vec<ClaimEvent> = made("k1", "person:alpha")
        .into_iter()
        .chain(made("k2", "person:beta"))
        .collect();
    for number in [10_u64, 2, 11] {
        let id = format!("k1/f{number}");
        events.push(ClaimEvent::RecordCreated {
            record: record(&id),
            claim: super::super::tests::claim(&format!("f{number}"), "k1", "a claim", 1),
        });
        events.push(writes(
            &id,
            "k1",
            "friend",
            number,
            FieldValue::Set("@#k2".into()),
            &["k2"],
        ));
    }
    let state = state_of(events);
    let labels: Vec<String> = holders(&state, "person:beta")
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert_eq!(labels, ["f2", "f10", "f11"]);
}

/// **A spelling a merge folded away is pointed at by no holder.** What a holder
/// wrote before the merge names the survivor now: the folded spelling answers
/// for none of it, and the survivor's spelling answers for all of it.
#[test]
fn a_folded_spelling_is_pointed_at_by_no_holder_and_its_survivor_by_all() {
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .chain([
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
                    folded: object("k2"),
                    survivor: object("k1"),
                    records: Vec::new(),
                    writes: Vec::new(),
                },
            ]),
    );
    assert_eq!(
        holders(&state, "person:alpha"),
        vec![("person:gamma".to_string(), "f1".to_string())]
    );
    assert!(holders(&state, "person:beta").is_empty());
}
