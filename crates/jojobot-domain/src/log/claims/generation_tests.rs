//! The generation a batch makes on a bot request: one past the highest ever
//! written, only when the folded spec differs after the batch.

use super::generation::Generation;
use super::tests::{apply, archived, body, claim, created, object, record, replay, write};
use super::{ClaimEvent, State};

/// A request and a plain thing, each with its handle.
fn world() -> Vec<ClaimEvent> {
    crate::memory::kinds::load_shipped();
    let mut events = Vec::new();
    for (id, kind, handle) in [
        ("k1", "bot-request", "bot-request:alpha"),
        ("k2", "bot-request", "bot-request:beta"),
        ("k3", "thing", "thing:gamma"),
    ] {
        events.push(ClaimEvent::ObjectCreated {
            object: object(id),
            body: body(kind, handle),
        });
        events.push(ClaimEvent::AddressBound {
            object: object(id),
            address: handle.into(),
        });
    }
    events
}

/// A request that holds `desired` under generation `number`, written by the
/// claim `label` of record `id`.
fn holding(id: &str, label: &str, home: &str, desired: &str, number: &str) -> Vec<ClaimEvent> {
    vec![
        created(id, claim(label, home, "a spec", 1)),
        write(id, home, "desired", 1, desired),
        write(id, home, "generation", 1, number),
    ]
}

fn generations(state: &State, pending: &[ClaimEvent]) -> Vec<Generation> {
    state.generations_after(pending, &[])
}

fn made(object_id: &str, record_id: &str, number: u64) -> Generation {
    Generation {
        object: object(object_id),
        record: record(record_id),
        number,
    }
}

#[test]
fn the_first_spec_written_makes_generation_one() {
    let state = replay(&world());
    let pending = [
        created("r1", claim("f1", "k1", "a spec", 1)),
        write("r1", "k1", "desired", 1, "running"),
    ];
    assert_eq!(generations(&state, &pending), vec![made("k1", "r1", 1)]);
}

#[test]
fn a_resend_of_the_spec_makes_none() {
    let mut events = world();
    events.extend(holding("r1", "f1", "k1", "running", "1"));
    let state = replay(&events);
    let pending = [
        created("r2", claim("f2", "k1", "again", 2)),
        write("r2", "k1", "desired", 2, "running"),
    ];
    assert_eq!(generations(&state, &pending), Vec::new());
}

#[test]
fn a_changed_spec_makes_one_past_the_highest_ever() {
    let mut events = world();
    events.extend(holding("r1", "f1", "k1", "running", "1"));
    events.extend([
        created("r2", claim("f2", "k1", "stop", 2)),
        write("r2", "k1", "desired", 2, "stopped"),
        write("r2", "k1", "generation", 2, "2"),
        // A number once issued names a spec for good, whatever became of its claim.
        archived("r2", "f2", "k1"),
    ]);
    let state = replay(&events);
    let pending = [
        created("r3", claim("f3", "k1", "run", 3)),
        write("r3", "k1", "desired", 3, "stopped"),
    ];
    assert_eq!(generations(&state, &pending), vec![made("k1", "r3", 3)]);
}

#[test]
fn a_batch_that_touches_two_requests_makes_one_for_each() {
    let state = replay(&world());
    let pending = [
        created("r1", claim("f1", "k1", "a spec", 1)),
        write("r1", "k1", "desired", 1, "running"),
        created("r2", claim("f1", "k2", "a spec", 1)),
        write("r2", "k2", "desired", 1, "running"),
        write("r1", "k1", "model", 1, "alpha-model"),
    ];
    assert_eq!(
        generations(&state, &pending),
        vec![made("k1", "r1", 1), made("k2", "r2", 1)]
    );
}

#[test]
fn a_batch_that_touches_no_spec_key_makes_none() {
    let state = replay(&world());
    let pending = [
        created("r1", claim("f1", "k1", "a note", 1)),
        write("r1", "k1", "remark", 1, "hello"),
    ];
    assert_eq!(generations(&state, &pending), Vec::new());
}

#[test]
fn a_thing_that_is_no_request_makes_none() {
    let state = replay(&world());
    let pending = [
        created("r1", claim("f1", "k3", "a spec", 1)),
        write("r1", "k3", "desired", 1, "running"),
    ];
    assert_eq!(generations(&state, &pending), Vec::new());
}

#[test]
fn taking_back_the_claim_that_holds_the_spec_makes_the_next_number() {
    let mut events = world();
    events.extend(holding("r1", "f1", "k1", "running", "1"));
    let state = replay(&events);
    let pending = [archived("r1", "f1", "k1")];
    assert_eq!(generations(&state, &pending), vec![made("k1", "r1", 2)]);
}

#[test]
fn a_batch_not_applied_changes_nothing_in_the_state() {
    let mut events = world();
    events.extend(holding("r1", "f1", "k1", "running", "1"));
    let mut state = replay(&events);
    let before = state.clone();
    let pending = [archived("r1", "f1", "k1")];
    let _ = generations(&state, &pending);
    assert_eq!(
        state, before,
        "the question reads the State and writes nothing"
    );
    for event in &pending {
        apply(&mut state, event);
    }
    assert_ne!(state, before);
}
