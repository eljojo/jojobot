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
    // A thing is served as an entity of the kind its handle names, and the kind
    // set is the process's.
    crate::memory::kinds::load_shipped();
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

/// **The listing holds every thing, folded rows too**, in the order of their
/// handles. A folded row names the thing it was folded into, and that thing
/// answers to the names of what was folded into it.
#[test]
fn the_listing_holds_the_folded_rows_and_names_what_was_folded_into_a_survivor() {
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .map(plain)
            .chain([
                plain(ClaimEvent::ObjectsMerged {
                    folded: object("k2"),
                    survivor: object("k1"),
                    records: Vec::new(),
                    writes: Vec::new(),
                }),
                plain(ClaimEvent::ObjectRedirected {
                    folded: object("k3"),
                    survivor: object("k1"),
                }),
            ]),
    );
    let rows = state.listing();
    let seen: Vec<(&str, Option<&str>)> = rows
        .iter()
        .map(|row| {
            (
                row.id.0.as_str(),
                row.merged_into.as_ref().map(|into| into.0.as_str()),
            )
        })
        .collect();
    assert_eq!(
        seen,
        vec![
            ("person:alpha", None),
            ("person:beta", Some("person:alpha")),
            ("person:gamma", Some("person:alpha")),
        ]
    );
    assert_eq!(
        rows[0].folded_names,
        vec!["person:beta".to_string(), "person:gamma".to_string()]
    );
}

/// **A claim a merge left unmapped names the merge that left it**, by the number
/// the merge holds: the order the numbered merges were made in. A redirect the
/// store never numbered takes no number.
#[test]
fn an_unmapped_claim_names_the_merge_that_left_it() {
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .chain(made("k4", "person:delta"))
            .map(plain)
            .chain([
                plain(ClaimEvent::ObjectRedirected {
                    folded: object("k4"),
                    survivor: object("k1"),
                }),
                plain(ClaimEvent::RecordCreated {
                    record: record("k2/f1"),
                    claim: super::super::tests::claim("f1", "k2", "left behind", 1),
                }),
                plain(ClaimEvent::ObjectsMerged {
                    folded: object("k3"),
                    survivor: object("k1"),
                    records: Vec::new(),
                    writes: Vec::new(),
                }),
                plain(ClaimEvent::ObjectsMerged {
                    folded: object("k2"),
                    survivor: object("k1"),
                    records: Vec::new(),
                    writes: Vec::new(),
                }),
            ]),
    );
    let scan = state
        .document_scan(&object("k1"), &[])
        .expect("the survivor");
    let left: Vec<(String, u64)> = scan
        .unmapped
        .iter()
        .map(|claim| (claim.address.to_string(), claim.merge_no))
        .collect();
    assert_eq!(left, vec![("person:beta#f1".to_string(), 2)]);
}

/// **A merge event names the survivor its event named**, not the end of the chain
/// the survivor was folded into since. The import orders marks by entity row, so
/// the mark that folds B into C can arrive before the mark that folds A into B.
/// The store holds A's merge under B, and the State serves it under B.
#[test]
fn a_merge_event_names_the_survivor_its_event_named() {
    let merged = |folded: &str, survivor: &str| {
        plain(ClaimEvent::ObjectsMerged {
            folded: object(folded),
            survivor: object(survivor),
            records: Vec::new(),
            writes: Vec::new(),
        })
    };
    let state = state_of(
        made("k1", "person:alpha")
            .into_iter()
            .chain(made("k2", "person:beta"))
            .chain(made("k3", "person:gamma"))
            .map(plain)
            .chain([merged("k2", "k3"), merged("k1", "k2")]),
    );
    let pairs = |events: Vec<MergeEvent>| -> Vec<(String, String)> {
        events
            .into_iter()
            .map(|event| (event.folded.0, event.survivor.0))
            .collect()
    };
    assert_eq!(
        pairs(state.merge_events(None)),
        vec![
            ("person:beta".to_string(), "person:gamma".to_string()),
            ("person:alpha".to_string(), "person:beta".to_string()),
        ]
    );
    assert_eq!(
        pairs(state.merge_events(Some(&handle("person:beta")))).len(),
        2,
        "beta is a side of both merges"
    );
}
