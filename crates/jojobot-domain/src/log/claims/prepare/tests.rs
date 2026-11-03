use std::collections::{BTreeMap, BTreeSet};

use super::super::tests::{apply, body, claim, created, day, object, record, replay, write};
use super::super::{ClaimEvent, ObjectBody, State};
use super::{CaptureRequest, Draft, Refusal, prepare};
use crate::memory::numbered::NUMBERS_BY;
use crate::memory::types::{DeclaredType, Field, ValueType};
use crate::memory::{FactStatus, Provenance, Standing};

fn thing(id: &str) -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object(id),
            body: ObjectBody {
                ..body("thing", id)
            },
        },
        ClaimEvent::AddressBound {
            object: object(id),
            address: id.to_string(),
        },
    ]
}

/// Two homes and a claim on `alpha`; `ref-key` is a declared reference.
fn world() -> (State, Vec<DeclaredType>) {
    let events = [
        thing("alpha"),
        thing("beta"),
        vec![created("r1", claim("f1", "alpha", "first", 1))],
    ]
    .concat();
    let declared = vec![DeclaredType::new(
        "thing",
        vec![Field::new("ref-key", ValueType::Reference)],
    )];
    (replay(&events), declared)
}

fn request(subject: &str) -> CaptureRequest {
    CaptureRequest {
        record: record("new"),
        subject: subject.to_string(),
        draft: Draft {
            content: "a claim".to_string(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: day(3),
            happened_at: None,
            happened_through: None,
            stale_after: None,
            receipt: None,
            app: None,
        },
        edge: None,
        refs: Vec::new(),
        fields: BTreeMap::new(),
        sets: BTreeSet::new(),
        derived: BTreeSet::new(),
        derived_from: None,
    }
}

struct Row {
    name: &'static str,
    request: CaptureRequest,
    refused: Refusal,
}

/// **A request with two faults is told the first.** Each row keeps the first
/// fault in the order and adds the second, so a row fails when the order
/// between the two changes.
#[test]
fn two_faults_are_told_in_the_order() {
    let (state, declared) = world();
    let rows = vec![
        Row {
            name: "a malformed write before a missing subject",
            request: CaptureRequest {
                sets: BTreeSet::from(["absent".to_string()]),
                ..request("nobody")
            },
            refused: Refusal::Malformed(
                "'absent' is named as a setting but carries no value: a setting is a key with a \
                 value, and a key is taken off with clear_fields"
                    .to_string(),
            ),
        },
        Row {
            name: "a missing subject before a missing edge object",
            request: CaptureRequest {
                edge: Some(("about".into(), "ghost".into())),
                ..request("nobody")
            },
            refused: Refusal::UnknownSubject("nobody".into()),
        },
        Row {
            name: "a missing edge object before a missing ref",
            request: CaptureRequest {
                edge: Some(("about".into(), "ghost".into())),
                refs: vec!["phantom".into()],
                ..request("alpha")
            },
            refused: Refusal::UnknownEdgeObject("ghost".into()),
        },
        Row {
            name: "a missing ref before a missing reference field object",
            request: CaptureRequest {
                refs: vec!["phantom".into()],
                fields: BTreeMap::from([("ref-key".into(), "person:moe".into())]),
                ..request("alpha")
            },
            refused: Refusal::UnknownRef("phantom".into()),
        },
        Row {
            name: "a missing reference field object before a missing source",
            request: CaptureRequest {
                fields: BTreeMap::from([("ref-key".into(), "person:moe".into())]),
                derived_from: Some("nobody#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownFieldTarget("person:moe".into()),
        },
        Row {
            name: "a source whose home is unknown",
            request: CaptureRequest {
                derived_from: Some("nobody#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownSourceHome("nobody#f1".into()),
        },
        Row {
            name: "a source whose home holds no such claim",
            request: CaptureRequest {
                derived_from: Some("beta#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownSourceClaim("beta#f1".into()),
        },
    ];
    for row in rows {
        assert_eq!(
            prepare(&state, &declared, &row.request),
            Err(row.refused),
            "{}",
            row.name
        );
    }
}

/// Refusing changes nothing.
#[test]
fn a_refused_request_leaves_the_state_as_it_was() {
    let (state, declared) = world();
    let before = state.clone();
    assert!(prepare(&state, &declared, &request("nobody")).is_err());
    assert_eq!(state, before);
}

/// **The claim takes the highest held number plus one**, not the count of what
/// is held: a gap does not hand a number out twice.
#[test]
fn the_label_is_the_highest_held_number_plus_one() {
    let (mut state, declared) = world();
    apply(&mut state, &created("r9", claim("f7", "alpha", "gap", 1)));
    let events = prepare(&state, &declared, &request("alpha")).expect("admitted");
    let ClaimEvent::RecordCreated { claim, .. } = &events[0] else {
        panic!("the first event creates the claim: {events:?}");
    };
    assert_eq!(claim.label, "f8");
}

/// **A key takes the place after the highest write in its stream.**
#[test]
fn a_key_takes_the_next_place_in_its_stream() {
    let (mut state, declared) = world();
    apply(&mut state, &write("r1", "alpha", "colour", 4, "red"));
    let events = prepare(
        &state,
        &declared,
        &CaptureRequest {
            fields: BTreeMap::from([
                ("colour".into(), "blue".into()),
                ("size".into(), "9".into()),
            ]),
            ..request("alpha")
        },
    )
    .expect("admitted");
    let places: Vec<(String, u64)> = events
        .iter()
        .filter_map(|event| match event {
            ClaimEvent::FieldWritten { key, ordinal, .. } => Some((key.0.clone(), *ordinal)),
            _ => None,
        })
        .collect();
    assert_eq!(
        places,
        vec![("colour".to_string(), 5), ("size".to_string(), 1)]
    );
}

/// A container `coll` numbering by `num`, with a live member holding 1, an
/// archived member holding 2, and a member `m3` holding nothing yet.
fn numbered_world() -> (State, Vec<DeclaredType>) {
    let member = |id: &str, archived: bool| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: ObjectBody {
                    parent: Some(object("coll")),
                    archived,
                    ..body("thing", id)
                },
            },
            ClaimEvent::AddressBound {
                object: object(id),
                address: id.to_string(),
            },
        ]
    };
    let holds = |record_id: &str, subject: &str, k: &str, value: &str| {
        vec![
            created(record_id, claim(&format!("f{record_id}"), subject, "c", 1)),
            write(record_id, subject, k, 1, value),
        ]
    };
    let events = [
        thing("coll"),
        holds("c1", "coll", NUMBERS_BY, "num"),
        member("m1", false),
        holds("n1", "m1", "num", "1"),
        member("m2", true),
        holds("n2", "m2", "num", "2"),
        member("m3", false),
    ]
    .concat();
    (replay(&events), Vec::new())
}

fn numbering(subject: &str, number: &str) -> CaptureRequest {
    CaptureRequest {
        fields: BTreeMap::from([("num".into(), number.into())]),
        ..request(subject)
    }
}

/// **No two members of a container hold one number**, and an archived member
/// keeps its own.
#[test]
fn a_member_cannot_take_a_number_another_member_holds() {
    let (state, declared) = numbered_world();
    let held = |number: u64, holder: &str| {
        Err(Refusal::NumberHeld {
            container: object("coll"),
            key: "num".into(),
            number,
            holder: object(holder),
        })
    };
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "1")),
        held(1, "m1"),
        "a live member's number"
    );
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "2")),
        held(2, "m2"),
        "an archived member's number"
    );
    assert!(
        prepare(&state, &declared, &numbering("m3", "3")).is_ok(),
        "a number nobody holds"
    );
    assert!(
        prepare(&state, &declared, &numbering("m1", "1")).is_ok(),
        "a member keeps writing the number it holds"
    );
}

/// A number is judged after every name the write mentions.
#[test]
fn a_missing_source_is_told_before_a_held_number() {
    let (state, declared) = numbered_world();
    let request = CaptureRequest {
        derived_from: Some("nobody#f1".into()),
        ..numbering("m3", "1")
    };
    assert_eq!(
        prepare(&state, &declared, &request),
        Err(Refusal::UnknownSourceHome("nobody#f1".into()))
    );
}
