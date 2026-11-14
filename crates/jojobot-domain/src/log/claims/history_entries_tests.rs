//! The entries a State holds of a thing equal the entries rebuilt from the events at the
//! positions they name.

use super::tests::{body, claim, envelope, key, object, record};
use super::*;
use crate::log::Position;

/// A short life of two things that are later merged: rows written and revised, an
/// observation, a claim written and revised, and two writes of a key.
fn life() -> Vec<(Position, Event<ClaimEvent>)> {
    let written = |position: u64, event: ClaimEvent| (Position(position), envelope(event));
    vec![
        written(
            1,
            ClaimEvent::ObjectCreated {
                object: object("homer"),
                body: body("person", "homer"),
            },
        ),
        written(
            2,
            ClaimEvent::ObjectCreated {
                object: object("homer-b"),
                body: body("person", "homer-b"),
            },
        ),
        written(
            3,
            ClaimEvent::ObjectWriteObserved {
                object: object("homer"),
                filed_under: "person:homer".into(),
                ordinal: 1,
                snapshot: None,
            },
        ),
        written(
            4,
            ClaimEvent::RecordCreated {
                record: record("r1"),
                claim: claim("f1", "homer", "likes donuts", 1),
            },
        ),
        written(
            5,
            ClaimEvent::FieldWritten {
                record: record("r1"),
                subject: object("homer"),
                key: key("mood"),
                ordinal: 1,
                class: WriteClass::Sets,
                write: FieldValue::Set("calm".into()),
                targets: Vec::new(),
            },
        ),
        written(
            6,
            ClaimEvent::RecordRevised {
                record: record("r1"),
                claim: claim("f1", "homer", "likes pink donuts", 2),
            },
        ),
        written(
            7,
            ClaimEvent::FieldWritten {
                record: record("r1"),
                subject: object("homer"),
                key: key("mood"),
                ordinal: 2,
                class: WriteClass::Sets,
                write: FieldValue::Set("cross".into()),
                targets: Vec::new(),
            },
        ),
        written(
            8,
            ClaimEvent::ObjectRevised {
                object: object("homer"),
                body: body("person", "homer simpson"),
            },
        ),
        written(
            9,
            ClaimEvent::ObjectsMerged {
                folded: object("homer-b"),
                survivor: object("homer"),
                records: Vec::new(),
                writes: Vec::new(),
            },
        ),
    ]
}

fn held() -> State {
    let mut state = State::default();
    for (position, event) in life() {
        state.apply_at(&event, position);
    }
    state
}

/// The events at the named positions, as a read of the log by position gives them.
fn fetched(positions: &[Position]) -> Vec<(Position, Event<ClaimEvent>)> {
    life()
        .into_iter()
        .filter(|(position, _)| positions.contains(position))
        .collect()
}

/// **An entry rebuilt from the event at its position is the entry the State holds**, of
/// each kind: the versions of a row (the folded thing's among them), an observation, the
/// writes of a claim and the writes of a key.
#[test]
fn entries_rebuilt_from_the_log_by_position_equal_the_entries_held() {
    let state = held();
    let entries = state.history_entries_of(&object("homer"));
    assert_eq!(entries.versions.len(), 3, "{entries:?}");
    assert_eq!(entries.observed.len(), 1);
    assert_eq!(entries.revisions.len(), 2);
    assert_eq!(entries.writes.len(), 2);

    let events = fetched(&entries.positions());
    let rebuilt = State::history_entries_from(events.iter().map(|(at, event)| (*at, event)));
    assert_eq!(rebuilt, entries);
}

/// **Positions one off give other entries**: the events fetched at positions shifted by
/// one are not the entries the State holds, so a recorded position that is wrong shows.
#[test]
fn entries_rebuilt_at_positions_shifted_by_one_are_not_the_entries_held() {
    let state = held();
    let entries = state.history_entries_of(&object("homer"));
    let events = fetched(&entries.positions());
    let shifted =
        State::history_entries_from(events.iter().map(|(at, event)| (Position(at.0 + 1), event)));
    assert_ne!(shifted, entries);
}

/// **A merge that renumbers the writes of a key changes the entries the State holds, and the
/// entries rebuilt from the log by position are those**: the survivor's writes take their new
/// places and the folded thing's write joins the stream, so the held entries name the merge
/// as well as the writes. Rebuilt from the writes' own events alone, the ordinals would be the
/// ones they were written with.
#[test]
fn entries_rebuilt_by_position_equal_the_entries_held_after_a_merge_renumbers_writes() {
    use super::tests::{created, write};
    use crate::log::claims::{MovedRecord, MovedWrite};

    let events: Vec<(Position, Event<ClaimEvent>)> = [
        created("r1", claim("f1", "alpha", "alpha says", 1)),
        created("r2", claim("f1", "beta", "beta says", 2)),
        created("r3", claim("f2", "alpha", "alpha again", 3)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r3", "alpha", "mood", 2, "steady"),
        write("r2", "beta", "mood", 1, "calm"),
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f3".into(),
            }],
            writes: vec![
                MovedWrite {
                    record: record("r2"),
                    key: key("mood"),
                    from_ordinal: 1,
                    ordinal: 2,
                },
                MovedWrite {
                    record: record("r3"),
                    key: key("mood"),
                    from_ordinal: 2,
                    ordinal: 3,
                },
            ],
        },
    ]
    .into_iter()
    .enumerate()
    .map(|(index, event)| (Position(index as u64 + 1), envelope(event)))
    .collect();
    let mut state = State::default();
    for (position, event) in &events {
        state.apply_at(event, *position);
    }
    let entries = state.history_entries_of(&object("alpha"));
    let ordinals: std::collections::BTreeSet<u64> =
        entries.writes.iter().map(|write| write.ordinal).collect();
    assert_eq!(
        ordinals,
        [1, 2, 3].into(),
        "the merge placed the folded write among the survivor's"
    );

    let wanted = entries.positions();
    let fetched: Vec<_> = events
        .iter()
        .filter(|(position, _)| wanted.contains(position))
        .map(|(position, event)| (*position, event))
        .collect();
    assert_eq!(State::history_entries_from(fetched), entries);
}
