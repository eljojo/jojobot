use super::super::tests::{body, moment, object, record, under};
use super::super::{ClaimEvent, MovedRecord, State};
use crate::log::Event;
use crate::memory::{EntityId, FactId, MergeEvent};

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

fn state_of(events: impl IntoIterator<Item = Event<ClaimEvent>>) -> State {
    let mut state = State::default();
    for event in events {
        state.apply(&event);
    }
    state
}

fn plain(event: ClaimEvent) -> Event<ClaimEvent> {
    under(None, None, event)
}

fn handle(text: &str) -> EntityId {
    EntityId(text.to_string())
}

/// **The merges the State holds, read as the port serves them**: the handle each
/// thing wears itself, the moment and the run of the merge, and each claim that
/// took a new label, old then new, in the order the merges were made. A redirect
/// the store never numbered is no merge event.
#[test]
fn the_merges_of_the_state_are_the_merge_events_of_the_port() {
    let merge = Event {
        written_at: Some(moment("2026-04-02T00:00:00Z")),
        ..under(
            None,
            Some("run-m"),
            ClaimEvent::ObjectsMerged {
                folded: object("k2"),
                survivor: object("k1"),
                records: vec![MovedRecord {
                    record: record("k2/f1"),
                    label: "f4".into(),
                }],
                writes: Vec::new(),
            },
        )
    };
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .map(plain)
            .chain([
                plain(ClaimEvent::RecordCreated {
                    record: record("k2/f1"),
                    claim: super::super::tests::claim("f1", "k2", "claims", 1),
                }),
                merge,
                plain(ClaimEvent::ObjectRedirected {
                    folded: object("k3"),
                    survivor: object("k1"),
                }),
            ]),
    );
    let want = MergeEvent {
        folded: handle("person:beta"),
        survivor: handle("person:alpha"),
        merged_at: moment("2026-04-02T00:00:00Z"),
        session: Some("run-m".into()),
        claims: vec![(FactId("f1".into()), FactId("f4".into()))],
    };
    assert_eq!(state.merge_events(None), vec![want.clone()]);
    assert_eq!(
        state.merge_events(Some(&handle("person:alpha"))),
        vec![want]
    );
    assert!(state.merge_events(Some(&handle("person:gamma"))).is_empty());
}
