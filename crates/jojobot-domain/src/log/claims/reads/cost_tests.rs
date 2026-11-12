//! **What each read costs at the size the calibration store holds**, counted and not
//! timed: a read that folded, built a document or looked at a thing it did not need
//! would show here as a count, whatever machine runs it. Three thousand things, forty
//! bots that wrote into one thread, and every read the fold serves from the State.
//!
//! The counts are the cost model the architect asked for: a read of one thing costs
//! one lookup or one scan; a browse looks at every thing once and folds none; the
//! writers of a thread build a document for each bot and for no other thing.

use super::super::tests::{body, claim, key, object, record, under};
use super::super::{ClaimEvent, FieldValue, State};
use crate::memory::filter::{Direction, Test};
use crate::memory::types::ValueType;
use crate::memory::{EntityId, EntityKind, WriteClass};

const THINGS: usize = 3000;
const BOTS: usize = 40;

/// The letters a number is written in, so a handle holds no digit.
fn letters(mut n: usize) -> String {
    let mut out = String::new();
    loop {
        out.push((b'a' + (n % 26) as u8) as char);
        n /= 26;
        if n == 0 {
            return out;
        }
    }
}

fn apply_all(state: &mut State, events: impl IntoIterator<Item = ClaimEvent>) {
    for event in events {
        state.apply(&under(None, None, event));
    }
}

/// **A State of the calibration store's size**: three thousand things that each
/// hold a `due` and a `mood`, a thread, and forty bots that each wrote ten thoughts
/// into it.
fn a_state_of_the_calibration_size() -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for n in 0..THINGS {
        let (id, handle) = (format!("t{n}"), format!("thing:{}", letters(n)));
        apply_all(
            &mut state,
            [
                ClaimEvent::ObjectCreated {
                    object: object(&id),
                    body: body("thing", &handle),
                },
                ClaimEvent::AddressBound {
                    object: object(&id),
                    address: handle.clone(),
                },
                ClaimEvent::RecordCreated {
                    record: record(&format!("{id}/f1")),
                    claim: claim("f1", &id, "a claim", 1),
                },
                field_write(&id, "due", 1, &format!("2026-09-{:02}", 1 + n % 28)),
                field_write(&id, "mood", 2, if n % 2 == 0 { "calm" } else { "tired" }),
            ],
        );
    }
    apply_all(
        &mut state,
        [
            ClaimEvent::ObjectCreated {
                object: object("thread"),
                body: body("thread", "a thread"),
            },
            ClaimEvent::AddressBound {
                object: object("thread"),
                address: "thread:alpha".to_string(),
            },
        ],
    );
    for bot in 0..BOTS {
        let (id, handle) = (format!("b{bot}"), format!("bot:{}", letters(bot)));
        apply_all(
            &mut state,
            [
                ClaimEvent::ObjectCreated {
                    object: object(&id),
                    body: body("bot", &handle),
                },
                ClaimEvent::AddressBound {
                    object: object(&id),
                    address: handle,
                },
            ],
        );
        for thought in 0..10 {
            let mut thinking = claim(&format!("f{}", thought + 1), &id, "a thought", 1);
            thinking.edge = Some(("connection".to_string(), object("thread")));
            apply_all(
                &mut state,
                [ClaimEvent::RecordCreated {
                    record: record(&format!("{id}/f{}", thought + 1)),
                    claim: thinking,
                }],
            );
        }
    }
    state
}

fn field_write(home: &str, k: &str, ordinal: u64, value: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(&format!("{home}/f1")),
        subject: object(home),
        key: key(k),
        ordinal,
        class: WriteClass::Sets,
        write: FieldValue::Set(value.to_string()),
        targets: Vec::new(),
    }
}

#[test]
fn a_read_of_one_thing_costs_one_lookup_or_one_scan_and_folds_nothing() {
    let state = a_state_of_the_calibration_size();
    let declared = state.declared_types_shared();
    let thing = object("t1500");

    State::reset_folds_counted();
    assert_eq!(state.fields(&thing, &declared).len(), 2);
    assert_eq!(
        (
            State::folds_counted(),
            State::scans_counted(),
            State::visits_counted()
        ),
        (0, 0, 0),
        "fields is one lookup of the held fold"
    );

    State::reset_folds_counted();
    assert_eq!(
        state
            .document_scan(&thing, &declared)
            .expect("held")
            .facts
            .len(),
        1
    );
    assert_eq!(
        (
            State::folds_counted(),
            State::scans_counted(),
            State::visits_counted()
        ),
        (0, 1, 0),
        "recall and scan_entity build the scan of the one thing"
    );
}

#[test]
fn a_browse_looks_at_every_thing_once_and_folds_none() {
    let state = a_state_of_the_calibration_size();

    State::reset_folds_counted();
    let calm = state.keys_holding("mood", &Test::Equals("calm".to_string()), None);
    assert_eq!(calm.len(), THINGS / 2, "half the things are calm");
    assert_eq!(
        (
            State::folds_counted(),
            State::scans_counted(),
            State::visits_counted()
        ),
        (0, 0, THINGS + BOTS + 1),
        "keys_holding looks at every thing once and folds none"
    );

    State::reset_folds_counted();
    let first = state.ordered_first_k(
        "due",
        ValueType::Date,
        Direction::Ascending,
        Some(EntityKind::THING),
        None,
        5,
    );
    assert!(first.head.len() >= 5, "the front of the order is placed");
    assert_eq!(
        (
            State::folds_counted(),
            State::scans_counted(),
            State::visits_counted()
        ),
        (0, 0, THINGS + BOTS + 1),
        "ordered_first_k looks at every thing once and folds none"
    );
}

#[test]
fn the_writers_of_a_thread_build_a_document_for_each_bot_and_no_other_thing() {
    let state = a_state_of_the_calibration_size();
    let declared = state.declared_types_shared();

    State::reset_folds_counted();
    let writers = state.thought_writers(&EntityId("thread:alpha".to_string()), &declared);
    assert_eq!(writers.len(), BOTS, "each bot wrote into the thread");
    assert_eq!(
        (
            State::folds_counted(),
            State::scans_counted(),
            State::visits_counted()
        ),
        (0, BOTS, 0),
        "a document is built for each bot, none for the three thousand things"
    );
}
