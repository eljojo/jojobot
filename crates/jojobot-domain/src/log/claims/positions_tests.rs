//! Each entry the State keeps of a write carries the log position of its event.

use super::tests::{body, claim, envelope, key, object, record};
use super::*;
use crate::log::Position;

fn at(state: &mut State, position: u64, event: ClaimEvent) {
    state.apply_at(&envelope(event), Position(position));
}

fn field(record_name: &str, subject: &str, ordinal: u64, value: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(record_name),
        subject: object(subject),
        key: key("mood"),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(value.into()),
        targets: Vec::new(),
    }
}

/// **A State that kept one entry of each kind**, at positions of their own: a version of an
/// object's row at 2, an observation of it at 3, a claim at 5 revised at 9, and two writes
/// of a key at 7 and 11.
fn lived() -> State {
    let mut state = State::default();
    at(
        &mut state,
        2,
        ClaimEvent::ObjectCreated {
            object: object("homer"),
            body: body("person", "homer"),
        },
    );
    at(
        &mut state,
        3,
        ClaimEvent::ObjectWriteObserved {
            object: object("homer"),
            filed_under: "person:homer".into(),
            ordinal: 1,
            snapshot: None,
        },
    );
    at(
        &mut state,
        5,
        ClaimEvent::RecordCreated {
            record: record("r1"),
            claim: claim("f1", "homer", "likes donuts", 1),
        },
    );
    at(&mut state, 7, field("r1", "homer", 1, "calm"));
    at(
        &mut state,
        9,
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim("f1", "homer", "likes pink donuts", 2),
        },
    );
    at(&mut state, 11, field("r1", "homer", 2, "cross"));
    state
}

/// **A write of each kind the State keeps carries the position of its event**: a version
/// of an object's row, an observation of it, each write of a claim and each write of a
/// key.
#[test]
fn every_kept_write_carries_the_position_of_its_event() {
    let state = lived();
    let versions: Vec<u64> = state
        .entity_versions(&object("homer"))
        .iter()
        .map(|version| version.position.0)
        .collect();
    assert_eq!(versions, vec![2]);
    let observed: Vec<u64> = state
        .observed_writes(&object("homer"))
        .iter()
        .map(|write| write.position.0)
        .collect();
    assert_eq!(observed, vec![3]);
    let revisions: Vec<u64> = state
        .claim_history(&record("r1"))
        .iter()
        .map(|revision| revision.position.0)
        .collect();
    assert_eq!(revisions, vec![5, 9]);
    let writes: Vec<u64> = state
        .write_positions(&object("homer"), &key("mood"))
        .into_iter()
        .map(|position| position.0)
        .collect();
    assert_eq!(writes, vec![7, 11]);
}

/// **The position is in equality and in the serialized State.** The same events at other
/// positions are another State, because once an entry is dropped the position is the only
/// way back to it; and the serialized State carries it.
#[test]
fn the_position_is_part_of_equality_and_of_the_serialized_state() {
    let build = |first: u64| {
        let mut state = State::default();
        at(
            &mut state,
            first,
            ClaimEvent::ObjectCreated {
                object: object("homer"),
                body: body("person", "homer"),
            },
        );
        state
    };
    assert_eq!(build(4), build(4));
    assert_ne!(build(4), build(5));
    let text = serde_json::to_string(&build(4)).unwrap();
    assert!(text.contains("\"position\":4"), "{text}");
}

/// **A merge moves an entry and leaves its position**, so a thing folded into another
/// still names where each of its writes sits.
#[test]
fn a_merge_keeps_the_position_of_what_it_moves() {
    let mut state = State::default();
    for (position, name) in [(1, "homer"), (2, "homer-b")] {
        at(
            &mut state,
            position,
            ClaimEvent::ObjectCreated {
                object: object(name),
                body: body("person", name),
            },
        );
    }
    at(
        &mut state,
        3,
        ClaimEvent::ObjectsMerged {
            folded: object("homer-b"),
            survivor: object("homer"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    );
    let versions: Vec<u64> = state
        .entity_versions(&object("homer"))
        .iter()
        .map(|version| version.position.0)
        .collect();
    assert_eq!(versions, vec![1, 2]);
}

/// **Every kind of entry serializes its position**: a skipped field on any one of them would
/// leave a snapshot that restores a State unable to name where a write sits.
#[test]
fn every_kind_of_entry_serializes_its_position() {
    let text = serde_json::to_string(&lived()).unwrap();
    assert_eq!(
        text.matches("\"position\":").count(),
        6,
        "one for each version, observation, revision and write: {text}"
    );
    for position in [2, 3, 5, 7, 9, 11] {
        assert!(
            text.contains(&format!("\"position\":{position}")),
            "{position} is not serialized: {text}"
        );
    }
}
