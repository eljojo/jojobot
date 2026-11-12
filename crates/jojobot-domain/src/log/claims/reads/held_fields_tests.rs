//! **What a thing holds is folded once, when a write lands, and a read folds
//! nothing.** The State keeps each thing's folded fields beside its streams: a
//! write folds the thing it lands on, a declaration folds the things that hold a
//! key it names, and every read (`fields`, `keys_holding`, `ordered_first_k`,
//! `unknown_entity`) takes the held fold. The counter is the one the fold runs
//! through, so a read that folded again would count.

use super::super::tests::{body, claim, key, object, record, under};
use super::super::{ClaimEvent, FieldValue, State};
use crate::memory::types::{DeclaredType, Field, ValueType};
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

fn says(id: &str, home: &str) -> ClaimEvent {
    ClaimEvent::RecordCreated {
        record: record(id),
        claim: claim("f1", home, "a claim", 1),
    }
}

fn writes(id: &str, subject: &str, k: &str, ordinal: u64, value: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object(subject),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(value.to_string()),
        targets: Vec::new(),
    }
}

fn apply_all(state: &mut State, events: impl IntoIterator<Item = ClaimEvent>) {
    for event in events {
        state.apply(&under(None, None, event));
    }
}

/// Three things, each holding a `due` and a `mood`.
fn a_state_of_three() -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for (n, handle) in [(1, "person:alpha"), (2, "person:beta"), (3, "person:gamma")] {
        let id = format!("k{n}");
        apply_all(
            &mut state,
            made(&id, handle).into_iter().chain([
                says(&format!("{id}/f1"), &id),
                writes(&format!("{id}/f1"), &id, "due", 1, &format!("2026-09-0{n}")),
                writes(&format!("{id}/f1"), &id, "mood", 2, "calm"),
            ]),
        );
    }
    state
}

#[test]
fn a_read_folds_nothing() {
    let state = a_state_of_three();
    let declared = state.declared_types_shared();
    State::reset_folds_counted();
    let things: Vec<_> = ["k1", "k2", "k3"].iter().map(|id| object(id)).collect();
    for thing in &things {
        assert_eq!(state.fields(thing, &declared).len(), 2);
        assert_eq!(state.fields_served(thing, &declared).len(), 2);
    }
    let held = state.keys_holding(
        "mood",
        &crate::memory::filter::Test::Equals("calm".to_string()),
        None,
    );
    assert_eq!(
        held.len(),
        3,
        "the three things hold the mood: entities {:?}, fields {:?}",
        state.entities().len(),
        state.fields_served(&object("k1"), &declared)
    );
    state.ordered_first_k(
        "due",
        ValueType::Date,
        crate::memory::filter::Direction::Ascending,
        None,
        None,
        2,
    );
    let _ = state.unknown_entity(&EntityId("person:delta".to_string()));
    assert_eq!(State::folds_counted(), 0, "no read folded a thing");
}

#[test]
fn a_write_folds_the_thing_it_lands_on_and_no_other() {
    let mut state = a_state_of_three();
    State::reset_folds_counted();
    apply_all(&mut state, [writes("k2/f1", "k2", "mood", 3, "tired")]);
    assert_eq!(State::folds_counted(), 1, "one write folded one thing");
    let declared = state.declared_types_shared();
    assert_eq!(
        state.fields(&object("k2"), &declared).get("mood"),
        Some(&"tired".to_string()),
        "the read serves what the write folded"
    );
    assert_eq!(
        state.fields(&object("k1"), &declared).get("mood"),
        Some(&"calm".to_string())
    );
}

#[test]
fn a_declaration_folds_only_the_things_that_hold_a_key_it_names() {
    let mut state = a_state_of_three();
    apply_all(
        &mut state,
        made("k4", "person:delta").into_iter().chain([
            says("k4/f1", "k4"),
            writes("k4/f1", "k4", "colour", 1, "red"),
        ]),
    );
    State::reset_folds_counted();
    state.apply(&under(
        None,
        None,
        ClaimEvent::TypeDeclared {
            kind: object("kept"),
            declaration: DeclaredType::new("kept", vec![Field::new("due", ValueType::Date)]),
        },
    ));
    assert_eq!(
        State::folds_counted(),
        3,
        "the three things that hold `due` were folded again, and the one that holds `colour` was not"
    );
}

#[test]
fn what_is_held_is_what_a_fold_makes_after_every_kind_of_event() {
    let mut state = a_state_of_three();
    let check = |state: &State, after: &str| {
        if let Err(differs) = state.held_fields_are_what_a_fold_makes() {
            panic!("after {after}: {differs}");
        }
    };
    check(&state, "three writes");
    apply_all(&mut state, [writes("k1/f1", "k1", "due", 3, "2026-10-01")]);
    check(&state, "a write that replaces a value");
    state.apply(&under(
        None,
        None,
        ClaimEvent::TypeDeclared {
            kind: object("kept"),
            declaration: DeclaredType::new("kept", vec![Field::new("due", ValueType::Date)]),
        },
    ));
    check(&state, "a declaration");
    state.apply(&under(
        None,
        None,
        ClaimEvent::TypeWithdrawn {
            kind: object("kept"),
        },
    ));
    check(&state, "a withdrawal");
    let mut revised = claim("f1", "k2", "a claim", 1);
    revised.status = crate::memory::FactStatus::Archived;
    apply_all(
        &mut state,
        [ClaimEvent::RecordRevised {
            record: record("k2/f1"),
            claim: revised,
        }],
    );
    check(&state, "a claim archived");
    apply_all(
        &mut state,
        [ClaimEvent::ObjectsMerged {
            folded: object("k3"),
            survivor: object("k1"),
            records: Vec::new(),
            writes: Vec::new(),
        }],
    );
    check(&state, "a merge");
}
