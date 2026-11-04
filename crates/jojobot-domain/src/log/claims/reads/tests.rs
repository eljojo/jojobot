use super::super::tests::{archived, body, claim, clear, created, moment, object, stamped, write};
use super::super::{ClaimBody, ClaimEvent, ObjectBody, State};
use crate::memory::types::{DeclaredType, Field, Fold, ValueType};
use crate::memory::{FactId, FactStatus, Provenance, Standing};

fn thing() -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object("work:alpha"),
            body: ObjectBody {
                ..body("work", "Alpha")
            },
        },
        ClaimEvent::AddressBound {
            object: object("work:alpha"),
            address: "work:alpha".to_string(),
        },
    ]
}

fn counter(key: &str) -> DeclaredType {
    DeclaredType::new(
        "tally",
        vec![Field {
            folds: Fold::Sum,
            ..Field::new(key, ValueType::Number)
        }],
    )
}

/// The state after the events, each wrapped in the envelope that carries the
/// moment it names; an event with none is taken in at no moment.
fn state_at(events: Vec<(Option<&str>, ClaimEvent)>) -> State {
    let mut state = State::default();
    for (at, event) in events {
        state.apply(&stamped(at.map(moment), None, event));
    }
    state
}

fn plain(events: &[ClaimEvent]) -> State {
    state_at(events.iter().cloned().map(|event| (None, event)).collect())
}

fn noted(label: &str, content: &str, details: &str) -> ClaimBody {
    ClaimBody {
        details: Some(details.to_string()),
        provenance: Provenance::Observation,
        standing: Standing::Open,
        ..claim(label, "work:alpha", content, 3)
    }
}

/// **The backing of a key is the claim whose write holds the value**: its
/// label, who backs it, how sure anyone was and the note beside it.
#[test]
fn the_backing_of_a_key_is_the_claim_whose_write_holds_it() {
    let events = [
        thing(),
        vec![
            created("r1", claim("f1", "work:alpha", "old", 3)),
            write("r1", "work:alpha", "owner", 1, "bot:alpha"),
            created("r2", noted("f2", "new", "why")),
            write("r2", "work:alpha", "owner", 2, "bot:beta"),
        ],
    ]
    .concat();
    let state = plain(&events);
    let backing = state.backing(&object("work:alpha"), &[]);
    let held = &backing["owner"];
    assert_eq!(held.fact, FactId("f2".to_string()));
    assert_eq!(held.provenance, Provenance::Observation);
    assert_eq!(held.standing, Standing::Open);
    assert_eq!(held.note.as_deref(), Some("why"));
    assert_eq!(backing.len(), 1);
}

/// **A key the fold does not hold has no backing**: one taken off by a clear,
/// one whose only claim was taken back, one that is summed, and written again
/// the key is backed by the write that put it back.
#[test]
fn a_key_that_is_not_held_has_no_backing() {
    let counted = counter("visits");
    let events = [
        thing(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            write("r1", "work:alpha", "owner", 1, "bot:alpha"),
            created("r2", claim("f2", "work:alpha", "b", 3)),
            clear("r2", "work:alpha", "owner", 2),
            created("r3", claim("f3", "work:alpha", "c", 3)),
            write("r3", "work:alpha", "phase", 1, "now"),
            archived("r3", "f3", "work:alpha"),
            created("r4", claim("f4", "work:alpha", "d", 3)),
            write("r4", "work:alpha", "visits", 1, "2"),
        ],
    ]
    .concat();
    let state = plain(&events);
    let home = object("work:alpha");
    assert_eq!(
        state.backing(&home, &[]).into_keys().collect::<Vec<_>>(),
        vec!["visits".to_string()],
        "the cleared key and the taken back key are not here, and the counter is a newest key until a type counts it"
    );
    assert!(
        state.backing(&home, &[counted]).is_empty(),
        "a summed key has no single winning write"
    );
    let again = [
        events,
        vec![write("r1", "work:alpha", "owner", 3, "bot:gamma")],
    ]
    .concat();
    let backing = plain(&again).backing(&home, &[]);
    assert_eq!(backing["owner"].fact, FactId("f1".to_string()));
    assert_eq!(backing.len(), 2, "owner and visits");
}

/// **The moment of a key is the moment of the write that holds it**, not of the
/// newest write of the thing, and a holding write with no moment leaves the key
/// out.
#[test]
fn a_key_is_as_old_as_the_write_that_holds_it() {
    let state = state_at(
        thing()
            .into_iter()
            .map(|event| (None, event))
            .chain([
                (None, created("r1", claim("f1", "work:alpha", "a", 3))),
                (
                    Some("2026-04-01T00:00:00Z"),
                    write("r1", "work:alpha", "owner", 1, "bot:alpha"),
                ),
                (
                    Some("2026-04-05T00:00:00Z"),
                    write("r1", "work:alpha", "phase", 1, "now"),
                ),
                (
                    Some("2026-04-02T00:00:00Z"),
                    write("r1", "work:alpha", "owner", 2, "bot:beta"),
                ),
                (None, write("r1", "work:alpha", "unstamped", 1, "x")),
            ])
            .collect(),
    );
    let ages = state.held_since(&object("work:alpha"), &[]);
    assert_eq!(ages["owner"], moment("2026-04-02T00:00:00Z"));
    assert_eq!(ages["phase"], moment("2026-04-05T00:00:00Z"));
    assert!(!ages.contains_key("unstamped"));
    assert_eq!(ages.len(), 2);
}

/// **A fact taken back is passed over by both reads**, and the status is read
/// from the claim now, not from when the write was made.
#[test]
fn a_write_of_a_taken_back_claim_backs_and_ages_nothing() {
    let events = [
        thing(),
        vec![
            created("r1", claim("f1", "work:alpha", "a", 3)),
            write("r1", "work:alpha", "owner", 1, "bot:alpha"),
            created("r2", claim("f2", "work:alpha", "b", 3)),
            write("r2", "work:alpha", "owner", 2, "bot:beta"),
            archived("r2", "f2", "work:alpha"),
        ],
    ]
    .concat();
    let state = state_at(
        events
            .into_iter()
            .map(|event| (Some("2026-04-03T00:00:00Z"), event))
            .collect(),
    );
    let home = object("work:alpha");
    assert_eq!(
        state.backing(&home, &[])["owner"].fact,
        FactId("f1".to_string())
    );
    assert_eq!(
        state
            .record(&super::super::tests::record("r2"))
            .map(|r| r.claim.status),
        Some(FactStatus::Archived)
    );
    assert!(state.held_since(&home, &[]).contains_key("owner"));
}

/// **A read costs the streams of its own thing**, however many other things the
/// State holds: the streams are looked up by subject and never scanned.
#[test]
fn a_read_looks_only_at_the_streams_of_its_own_thing() {
    let mut events = thing();
    events.extend([
        created("r0", claim("f1", "work:alpha", "a", 3)),
        write("r0", "work:alpha", "owner", 1, "bot:alpha"),
        write("r0", "work:alpha", "phase", 1, "now"),
    ]);
    for (n, name) in [
        "delta", "epsilon", "zeta", "eta", "theta", "iota", "kappa", "lambda", "mu", "nu",
    ]
    .into_iter()
    .enumerate()
    {
        let id = format!("work:{name}");
        events.extend([
            ClaimEvent::ObjectCreated {
                object: object(&id),
                body: body("work", "Other"),
            },
            created(&format!("o{n}"), claim("f1", &id, "x", 3)),
            write(&format!("o{n}"), &id, "owner", 1, "bot:beta"),
        ]);
    }
    let state = plain(&events);
    // The first thing in the map, and one in the middle of it, so a read that
    // starts from the front of the map is seen as well as one that never stops.
    // A stream past the thing's own is looked at once, to see it is another's.
    for (name, own_streams) in [("work:alpha", 2), ("work:lambda", 1)] {
        let home = object(name);
        for read in 0..2 {
            super::STREAMS_EXAMINED.with(|n| n.set(0));
            match read {
                0 => drop(state.backing(&home, &[])),
                _ => drop(state.held_since(&home, &[])),
            };
            let examined = super::STREAMS_EXAMINED.with(|n| n.get());
            assert!(
                (own_streams..=own_streams + 1).contains(&examined),
                "read {read} of {name} looked at {examined} streams, its own are {own_streams}"
            );
        }
    }
}
