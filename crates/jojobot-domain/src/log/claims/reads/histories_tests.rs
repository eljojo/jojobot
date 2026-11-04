use super::super::tests::{archived, body, claim, created, moment, object, record, stamped, under};
use super::super::{ClaimBody, ClaimEvent, MovedRecord, ObjectBody, State};
use crate::actor::Actor;
use crate::log::Event;
use crate::memory::{FactStatus, Provenance};

fn things() -> Vec<ClaimEvent> {
    ["work:alpha", "work:beta"]
        .into_iter()
        .flat_map(|name| {
            [
                ClaimEvent::ObjectCreated {
                    object: object(name),
                    body: ObjectBody {
                        ..body("work", name)
                    },
                },
                ClaimEvent::AddressBound {
                    object: object(name),
                    address: name.to_string(),
                },
            ]
        })
        .collect()
}

fn revised(id: &str, content: &str, provenance: Provenance) -> ClaimEvent {
    ClaimEvent::RecordRevised {
        record: record(id),
        claim: ClaimBody {
            provenance,
            ..claim("f1", "work:alpha", content, 3)
        },
    }
}

/// **A claim keeps every write of it, oldest first**, each with the claim as
/// that write left it and what the envelope says of the write: the actor, the
/// run, and the moment, which an imported write takes from its source.
#[test]
fn a_claim_keeps_every_write_of_it_oldest_first() {
    let mut state = State::default();
    for event in things() {
        state.apply(&stamped(None, None, event));
    }
    state.apply(&Event {
        written_at: Some(moment("2026-04-01T00:00:00Z")),
        imported_source_time: Some(moment("2026-02-01T00:00:00Z")),
        ..under(
            Some(Actor::Operator),
            Some("run-a"),
            created("r1", claim("f1", "work:alpha", "first", 3)),
        )
    });
    state.apply(&stamped(
        Some(moment("2026-04-02T00:00:00Z")),
        Some(moment("2026-03-01T00:00:00Z")),
        revised("r1", "second", Provenance::Observation),
    ));
    state.apply(&stamped(
        Some(moment("2026-04-03T00:00:00Z")),
        None,
        archived("r1", "f1", "work:alpha"),
    ));
    let chain = state.claim_history(&record("r1"));
    assert_eq!(
        chain
            .iter()
            .map(|w| w.claim.content.as_str())
            .collect::<Vec<_>>(),
        vec!["first", "second", "taken back"]
    );
    assert_eq!(chain[0].actor, Some(Actor::Operator));
    assert_eq!(chain[0].run.as_ref().map(|r| r.0.as_str()), Some("run-a"));
    assert_eq!(
        chain[0].written_at,
        Some(moment("2026-02-01T00:00:00Z")),
        "the first write keeps the moment of its source too"
    );
    assert_eq!(chain[1].claim.provenance, Provenance::Observation);
    assert_eq!(
        chain[1].written_at,
        Some(moment("2026-03-01T00:00:00Z")),
        "an imported write keeps the moment of its source"
    );
    assert_eq!(chain[2].claim.status, FactStatus::Archived);
    assert_eq!(chain[2].written_at, Some(moment("2026-04-03T00:00:00Z")));
    assert_eq!(
        state.record(&record("r1")).map(|r| &r.claim),
        Some(&chain[2].claim),
        "the last write is the claim now"
    );
}

/// **A claim the State does not hold has no writes.**
#[test]
fn a_claim_the_state_does_not_hold_has_no_writes() {
    let state = State::default();
    assert!(state.claim_history(&record("missing")).is_empty());
}

/// **The histories of a thing are the chains of the claims homed on it**, and
/// no other thing's; a merge moves a claim's chain with the claim.
#[test]
fn the_histories_of_a_thing_are_those_of_the_claims_homed_on_it() {
    let mut state = State::default();
    let events = [
        things(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            revised("r1", "a2", Provenance::Testimony),
            created("r2", claim("f1", "work:beta", "b", 3)),
        ],
    ]
    .concat();
    for event in events {
        state.apply(&stamped(None, None, event));
    }
    let ids = |state: &State, name: &str| {
        state
            .claim_histories(&object(name))
            .into_iter()
            .map(|(id, chain)| (id.0.clone(), chain.len()))
            .collect::<Vec<_>>()
    };
    assert_eq!(ids(&state, "work:alpha"), vec![("r1".to_string(), 2)]);
    assert_eq!(ids(&state, "work:beta"), vec![("r2".to_string(), 1)]);
    state.apply(&stamped(
        None,
        None,
        ClaimEvent::ObjectsMerged {
            folded: object("work:beta"),
            survivor: object("work:alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f2".to_string(),
            }],
            writes: Vec::new(),
        },
    ));
    assert_eq!(
        ids(&state, "work:alpha"),
        vec![("r1".to_string(), 2), ("r2".to_string(), 1)]
    );
    assert_eq!(
        ids(&state, "work:beta"),
        ids(&state, "work:alpha"),
        "the folded thing answers as its survivor"
    );
}
