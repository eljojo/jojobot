use std::collections::BTreeSet;

use super::super::tests::{
    apply, archived, body, claim, clear, created, key, object, record, replay, write,
};
use super::super::{
    ClaimEvent, FieldValue, KindId, MovedRecord, MovedWrite, ObjectBody, ObjectId, State,
};
use super::{Located, claim_number};
use crate::memory::FactStatus;
use crate::memory::numbered::NUMBERS_BY;

fn thing(id: &str, parent: Option<&str>) -> ClaimEvent {
    ClaimEvent::ObjectCreated {
        object: object(id),
        body: ObjectBody {
            parent: parent.map(object),
            ..body("thing", id)
        },
    }
}

fn moved_to(id: &str, parent: Option<&str>, archived: bool) -> ClaimEvent {
    ClaimEvent::ObjectRevised {
        object: object(id),
        body: ObjectBody {
            parent: parent.map(object),
            archived,
            ..body("thing", id)
        },
    }
}

/// A claim homed on `subject` that writes `value` to `k` at ordinal one.
fn holds(record_id: &str, subject: &str, k: &str, value: &str) -> Vec<ClaimEvent> {
    vec![
        created(record_id, claim(&format!("f{record_id}"), subject, "c", 1)),
        write(record_id, subject, k, 1, value),
    ]
}

fn merged(
    folded: &str,
    survivor: &str,
    records: &[(&str, &str)],
    writes: &[(&str, &str, u64, u64)],
) -> ClaimEvent {
    ClaimEvent::ObjectsMerged {
        folded: object(folded),
        survivor: object(survivor),
        records: records
            .iter()
            .map(|(id, label)| MovedRecord {
                record: record(id),
                label: label.to_string(),
            })
            .collect(),
        writes: writes
            .iter()
            .map(|(id, k, from, to)| MovedWrite {
                record: record(id),
                key: key(k),
                from_ordinal: *from,
                ordinal: *to,
            })
            .collect(),
    }
}

fn state(events: &[ClaimEvent]) -> State {
    replay(events)
}

/// Every table row is a name, the events and what the lookups answer after them.
struct Row<T> {
    name: &'static str,
    events: Vec<ClaimEvent>,
    ask: T,
}

#[test]
fn the_next_ordinal_is_one_past_the_highest_in_the_stream() {
    // (object, key, expected)
    let rows = vec![
        Row {
            name: "an empty state starts every stream at one",
            events: vec![],
            ask: vec![("alpha", "mood", 1)],
        },
        Row {
            name: "one write makes the next two",
            events: vec![write("r1", "alpha", "mood", 1, "a")],
            ask: vec![("alpha", "mood", 2)],
        },
        Row {
            name: "the highest decides, not the last to arrive",
            events: vec![
                write("r1", "alpha", "mood", 3, "a"),
                write("r2", "alpha", "mood", 1, "b"),
            ],
            ask: vec![("alpha", "mood", 4)],
        },
        Row {
            name: "a key and an object count apart",
            events: vec![write("r1", "alpha", "mood", 2, "a")],
            ask: vec![("alpha", "tone", 1), ("beta", "mood", 1)],
        },
        Row {
            name: "a clear takes a place",
            events: vec![
                write("r1", "alpha", "mood", 1, "a"),
                clear("r2", "alpha", "mood", 2),
            ],
            ask: vec![("alpha", "mood", 3)],
        },
        Row {
            name: "a claim taken back keeps the places its writes took",
            events: vec![
                created("r1", claim("f1", "alpha", "a", 1)),
                write("r1", "alpha", "mood", 2, "a"),
                archived("r1", "f1", "alpha"),
            ],
            ask: vec![("alpha", "mood", 3)],
        },
        Row {
            name: "a merge puts the folded writes into the survivor's stream after the decided interleave",
            events: vec![
                write("r1", "alpha", "mood", 1, "a"),
                write("r1", "alpha", "mood", 2, "a"),
                write("r2", "beta", "mood", 1, "b"),
                merged(
                    "alpha",
                    "beta",
                    &[],
                    &[("r1", "mood", 1, 3), ("r1", "mood", 2, 4)],
                ),
            ],
            ask: vec![("beta", "mood", 5), ("alpha", "mood", 5)],
        },
        Row {
            name: "a key only the folded object held moves to the survivor",
            events: vec![
                write("r1", "alpha", "tone", 1, "a"),
                merged("alpha", "beta", &[], &[]),
            ],
            ask: vec![("beta", "tone", 2), ("alpha", "tone", 2)],
        },
    ];
    for row in rows {
        let state = state(&row.events);
        for (subject, k, want) in row.ask {
            assert_eq!(
                state.next_ordinal(&object(subject), &key(k)),
                want,
                "{}: {subject} {k}",
                row.name
            );
        }
    }
}

#[test]
fn the_highest_claim_number_is_per_home_and_survives_a_taken_back_claim() {
    // (home, expected)
    let rows = vec![
        Row {
            name: "a home with no claim holds zero",
            events: vec![],
            ask: vec![("alpha", 0)],
        },
        Row {
            name: "the highest label decides, not the last to arrive",
            events: vec![
                created("r3", claim("f3", "alpha", "c", 1)),
                created("r1", claim("f1", "alpha", "c", 1)),
            ],
            ask: vec![("alpha", 3), ("beta", 0)],
        },
        Row {
            name: "a taken-back claim keeps its number",
            events: vec![
                created("r5", claim("f5", "alpha", "c", 1)),
                archived("r5", "f5", "alpha"),
            ],
            ask: vec![("alpha", 5)],
        },
        Row {
            name: "a label that is not f and a whole number holds none",
            events: vec![
                created("r1", claim("x7", "alpha", "c", 1)),
                created("r2", claim("f", "alpha", "c", 1)),
                created("r3", claim("f0", "alpha", "c", 1)),
                created("r4", claim("f01", "alpha", "c", 1)),
                created("r5", claim("f-2", "alpha", "c", 1)),
            ],
            ask: vec![("alpha", 0)],
        },
        Row {
            name: "a revision to a lower label gives the higher number back",
            events: vec![
                created("r1", claim("f5", "alpha", "c", 1)),
                created("r2", claim("f2", "alpha", "c", 1)),
                ClaimEvent::RecordRevised {
                    record: record("r1"),
                    claim: claim("f1", "alpha", "c", 1),
                },
            ],
            ask: vec![("alpha", 2)],
        },
        Row {
            name: "two claims on one number: one leaving keeps the number",
            events: vec![
                created("r1", claim("f4", "alpha", "c", 1)),
                created("r2", claim("f4", "alpha", "c", 1)),
                ClaimEvent::RecordRevised {
                    record: record("r1"),
                    claim: claim("f1", "alpha", "c", 1),
                },
            ],
            ask: vec![("alpha", 4)],
        },
        Row {
            name: "a claim revised to another home takes its number there",
            events: vec![
                created("r1", claim("f6", "alpha", "c", 1)),
                ClaimEvent::RecordRevised {
                    record: record("r1"),
                    claim: claim("f6", "beta", "c", 1),
                },
            ],
            ask: vec![("alpha", 0), ("beta", 6)],
        },
        Row {
            name: "a merge counts the moved claims under their new labels, whichever name is asked",
            events: vec![
                created("a1", claim("f1", "alpha", "c", 1)),
                created("a2", claim("f2", "alpha", "c", 1)),
                created("b1", claim("f1", "beta", "c", 1)),
                merged("alpha", "beta", &[("a1", "f2"), ("a2", "f3")], &[]),
            ],
            ask: vec![("beta", 3), ("alpha", 3)],
        },
        Row {
            name: "a moved claim relabeled lower gives its old number back",
            events: vec![
                created("a1", claim("f9", "alpha", "c", 1)),
                created("b1", claim("f1", "beta", "c", 1)),
                merged("alpha", "beta", &[("a1", "f2")], &[]),
            ],
            ask: vec![("beta", 2)],
        },
        Row {
            name: "a merge leaves the survivor's own higher number",
            events: vec![
                created("a1", claim("f1", "alpha", "c", 1)),
                created("b1", claim("f7", "beta", "c", 1)),
                merged("alpha", "beta", &[("a1", "f8")], &[]),
            ],
            ask: vec![("beta", 8)],
        },
        Row {
            name: "a claim a merge did not list stays counted under the survivor",
            events: vec![
                created("a1", claim("f9", "alpha", "c", 1)),
                created("b1", claim("f1", "beta", "c", 1)),
                merged("alpha", "beta", &[], &[]),
            ],
            ask: vec![("beta", 9)],
        },
        Row {
            name: "a chain of merges follows to the last survivor",
            events: vec![
                created("a1", claim("f1", "alpha", "c", 1)),
                merged("alpha", "beta", &[("a1", "f2")], &[]),
                created("g1", claim("f1", "gamma", "c", 1)),
                merged("beta", "gamma", &[("a1", "f3")], &[]),
            ],
            ask: vec![("gamma", 3), ("alpha", 3), ("beta", 3)],
        },
    ];
    for row in rows {
        let state = state(&row.events);
        for (home, want) in row.ask {
            assert_eq!(
                state.highest_claim_number(&object(home)),
                want,
                "{}: {home}",
                row.name
            );
        }
    }
}

#[test]
fn the_highest_merge_number_counts_merges_in_the_order_applied() {
    let rows = vec![
        Row {
            name: "no merge is zero",
            events: vec![write("r1", "alpha", "mood", 1, "a")],
            ask: 0,
        },
        Row {
            name: "a merge with nothing to move still counts",
            events: vec![merged("alpha", "beta", &[], &[])],
            ask: 1,
        },
        Row {
            name: "each merge counts",
            events: vec![
                merged("alpha", "beta", &[], &[]),
                merged("gamma", "beta", &[], &[]),
            ],
            ask: 2,
        },
    ];
    for row in rows {
        assert_eq!(
            state(&row.events).highest_merge_number(),
            row.ask,
            "{}",
            row.name
        );
    }
}

fn numbered(state: &State, container: &str, k: &str, number: u64) -> Option<String> {
    state
        .numbered_member(&object(container), k, number)
        .map(|member| member.0.clone())
}

fn collection() -> Vec<ClaimEvent> {
    let mut events = vec![thing("coll", None)];
    events.extend(holds("c", "coll", NUMBERS_BY, "num"));
    events
}

fn with(mut events: Vec<ClaimEvent>, more: Vec<ClaimEvent>) -> Vec<ClaimEvent> {
    events.extend(more);
    events
}

#[test]
fn a_container_holds_the_numbers_of_the_objects_under_it() {
    // (container, key, number, expected member)
    type Ask = Vec<(&'static str, &'static str, u64, Option<&'static str>)>;
    let rows: Vec<Row<Ask>> = vec![
        Row {
            name: "the empty state holds none",
            events: vec![],
            ask: vec![("coll", "num", 1, None)],
        },
        Row {
            name: "a member holds its number under its container's key",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll")), thing("m2", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    holds("w2", "m2", "num", "2"),
                ]
                .concat(),
            ),
            ask: vec![
                ("coll", "num", 1, Some("m1")),
                ("coll", "num", 2, Some("m2")),
                ("coll", "num", 3, None),
                ("coll", "other", 1, None),
            ],
        },
        Row {
            name: "a member however deep holds a number",
            events: with(
                collection(),
                [
                    vec![thing("mid", Some("coll")), thing("deep", Some("mid"))],
                    holds("w1", "deep", "num", "4"),
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 4, Some("deep"))],
        },
        Row {
            name: "an archived member keeps its number",
            events: with(
                collection(),
                [
                    vec![
                        thing("m1", Some("coll")),
                        moved_to("m1", Some("coll"), true),
                    ],
                    holds("w1", "m1", "num", "1"),
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, Some("m1"))],
        },
        Row {
            name: "only a whole number in its one spelling is a number",
            events: with(
                collection(),
                [
                    vec![
                        thing("a", Some("coll")),
                        thing("b", Some("coll")),
                        thing("c", Some("coll")),
                        thing("d", Some("coll")),
                    ],
                    holds("w1", "a", "num", "007"),
                    holds("w2", "b", "num", "-1"),
                    holds("w3", "c", "num", "x"),
                    holds("w4", "d", "num", "0"),
                ]
                .concat(),
            ),
            ask: vec![
                ("coll", "num", 7, None),
                ("coll", "num", 1, None),
                ("coll", "num", 0, None),
            ],
        },
        Row {
            name: "the container itself holds no seat in itself",
            events: with(collection(), holds("w1", "coll", "num", "5")),
            ask: vec![("coll", "num", 5, None)],
        },
        Row {
            name: "a number the claim no longer stands behind leaves",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![archived("w1", "fw1", "m1")],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None)],
        },
        Row {
            name: "the newest write of the key is the number held",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![
                        created("w2", claim("fw2", "m1", "c", 2)),
                        write("w2", "m1", "num", 2, "2"),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None), ("coll", "num", 2, Some("m1"))],
        },
        Row {
            name: "a clear takes the number off",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![
                        created("w2", claim("fw2", "m1", "c", 2)),
                        clear("w1", "m1", "num", 2),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None)],
        },
        Row {
            name: "a container that stops numbering holds nothing",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![
                        created("w2", claim("fw2", "coll", "c", 2)),
                        clear("c", "coll", NUMBERS_BY, 2),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None)],
        },
        Row {
            name: "a container that numbers again holds the numbers its members still hold",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![
                        created("w2", claim("fw2", "coll", "c", 2)),
                        clear("c", "coll", NUMBERS_BY, 2),
                        created("w3", claim("fw3", "coll", "c", 3)),
                        write("w3", "coll", NUMBERS_BY, 3, "num"),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, Some("m1")), ("coll", "num", 2, None)],
        },
        Row {
            name: "a container that numbers by another key moves its seats to it",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    holds("w2", "m1", "rank", "9"),
                    vec![
                        created("w3", claim("fw3", "coll", "c", 2)),
                        write("w3", "coll", NUMBERS_BY, 2, "rank"),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None), ("coll", "rank", 9, Some("m1"))],
        },
        Row {
            name: "a container numbering after its members already hold numbers",
            events: [
                vec![thing("coll", None), thing("m1", Some("coll"))],
                holds("w1", "m1", "num", "1"),
                holds("c", "coll", NUMBERS_BY, "num"),
            ]
            .concat(),
            ask: vec![("coll", "num", 1, Some("m1"))],
        },
        Row {
            name: "a write that arrives before the object it is on",
            events: [
                holds("w1", "m1", "num", "1"),
                collection(),
                vec![thing("m1", Some("coll"))],
            ]
            .concat(),
            ask: vec![("coll", "num", 1, Some("m1"))],
        },
        Row {
            name: "a member moved out leaves, and moved in arrives",
            events: with(
                collection(),
                [
                    vec![
                        thing("elsewhere", None),
                        thing("m1", Some("coll")),
                        thing("m2", Some("elsewhere")),
                    ],
                    holds("w1", "m1", "num", "1"),
                    holds("w2", "m2", "num", "2"),
                    vec![
                        moved_to("m1", Some("elsewhere"), false),
                        moved_to("m2", Some("coll"), false),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None), ("coll", "num", 2, Some("m2"))],
        },
        Row {
            name: "a subtree moved under a container brings its members",
            events: with(
                collection(),
                [
                    vec![thing("branch", None), thing("m1", Some("branch"))],
                    holds("w1", "m1", "num", "3"),
                    vec![moved_to("branch", Some("coll"), false)],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 3, Some("m1"))],
        },
        Row {
            name: "two containers, one above the other, each number the member",
            events: with(
                collection(),
                [
                    vec![thing("inner", Some("coll")), thing("m1", Some("inner"))],
                    holds("i", "inner", NUMBERS_BY, "num"),
                    holds("w1", "m1", "num", "2"),
                ]
                .concat(),
            ),
            ask: vec![
                ("coll", "num", 2, Some("m1")),
                ("inner", "num", 2, Some("m1")),
            ],
        },
        Row {
            name: "a merge of members leaves the survivor holding the number",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll")), thing("m2", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![merged("m1", "m2", &[], &[("w1", "num", 1, 1)])],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, Some("m2"))],
        },
        Row {
            name: "a merge of containers puts the members under the survivor, asked by either name",
            events: [
                vec![
                    thing("old", None),
                    thing("coll", None),
                    thing("m1", Some("old")),
                ],
                holds("w1", "m1", "num", "1"),
                holds("c", "coll", NUMBERS_BY, "num"),
                vec![merged("old", "coll", &[], &[])],
            ]
            .concat(),
            ask: vec![
                ("coll", "num", 1, Some("m1")),
                ("old", "num", 1, Some("m1")),
            ],
        },
    ];
    for row in rows {
        let state = state(&row.events);
        for (container, k, number, want) in row.ask {
            assert_eq!(
                numbered(&state, container, k, number).as_deref(),
                want,
                "{}: {container} {k} {number}",
                row.name
            );
        }
    }
}

/// **A number issued under a container stays issued under the container it is
/// merged into**, asked by either name, and its taken-back state with it.
#[test]
fn an_issued_number_follows_its_container_into_a_merge() {
    let events = [
        vec![
            thing("old", None),
            thing("coll", None),
            thing("m1", Some("old")),
            // A second member keeps its number held beside the taken-back one.
            thing("m2", Some("old")),
        ],
        holds("o", "old", NUMBERS_BY, "num"),
        holds("c", "coll", NUMBERS_BY, "num"),
        holds("w1", "m1", "num", "5"),
        holds("w2", "m2", "num", "6"),
        vec![
            ClaimEvent::RecordRevised {
                record: record("w1"),
                claim: {
                    let mut taken_back = claim("fw1", "m1", "c", 1);
                    taken_back.status = FactStatus::Archived;
                    taken_back
                },
            },
            merged("old", "coll", &[], &[]),
        ],
    ]
    .concat();
    let state = state(&events);
    for container in ["coll", "old"] {
        assert!(
            state.number_issued(&object(container), "num", 5),
            "{container}: issued"
        );
    }
    assert!(
        state.number_issued(&object("coll"), "num", 6),
        "the held number is issued too, so the unheld set must leave it out"
    );
    assert_eq!(
        state.issued_unheld(&object("coll"), "num"),
        BTreeSet::from([5]),
        "taken back, under the survivor, and the held 6 is not in it"
    );
}

/// **A revision issues no number**: only a write made while the container
/// numbers issues one. A claim revised after the container began to number does
/// not issue the number an earlier, taken-back claim wrote to the same stream
/// before it did, and the number a later member takes stays free.
#[test]
fn a_revision_does_not_issue_a_number_written_before_the_container_numbered() {
    let events = [
        vec![thing("coll", None), thing("m1", Some("coll"))],
        vec![
            created("y", claim("fy", "m1", "c", 1)),
            write("y", "m1", "num", 1, "3"),
            created("x", claim("fx", "m1", "c", 1)),
            write("x", "m1", "num", 2, "5"),
            archived("x", "fx", "m1"),
        ],
        holds("c", "coll", NUMBERS_BY, "num"),
    ]
    .concat();
    let mut state = state(&events);
    assert!(
        !state.number_issued(&object("coll"), "num", 5),
        "written before the container numbered: not issued"
    );
    assert!(state.number_issued(&object("coll"), "num", 3), "held now");
    let mut revised = claim("fy", "m1", "c again", 1);
    revised.details = Some("a note".into());
    apply(
        &mut state,
        &ClaimEvent::RecordRevised {
            record: record("y"),
            claim: revised,
        },
    );
    assert!(
        !state.number_issued(&object("coll"), "num", 5),
        "a revision of another claim issues nothing"
    );
    assert!(state.number_issued(&object("coll"), "num", 3));
}

/// **A write made while the container numbers issues its number for good**,
/// even when the write is not held: one into a claim already taken back holds
/// nothing, and the number is retired, not free.
#[test]
fn a_write_made_while_the_container_numbers_issues_its_number_for_good() {
    let events = [
        vec![thing("coll", None), thing("m1", Some("coll"))],
        holds("c", "coll", NUMBERS_BY, "num"),
        vec![
            created("y", claim("fy", "m1", "c", 1)),
            archived("y", "fy", "m1"),
            write("y", "m1", "num", 1, "7"),
        ],
    ]
    .concat();
    let state = state(&events);
    assert!(
        state.numbered_member(&object("coll"), "num", 7).is_none(),
        "held by nobody"
    );
    assert!(
        state.number_issued(&object("coll"), "num", 7),
        "and still issued"
    );
    assert!(!state.number_issued(&object("coll"), "num", 8));
}

fn bound(id: &str, address: &str) -> ClaimEvent {
    ClaimEvent::AddressBound {
        object: object(id),
        address: address.into(),
    }
}

fn retired(id: &str, address: &str) -> ClaimEvent {
    ClaimEvent::AddressRetired {
        object: object(id),
        address: address.into(),
        day: jiff::civil::date(2026, 4, 1),
    }
}

fn at(record_id: &str, home: &str) -> Option<Located> {
    Some(Located {
        record: record(record_id),
        home: object(home),
    })
}

#[test]
fn an_address_names_a_claim_and_its_home() {
    // (address, expected)
    type Ask = Vec<(&'static str, Option<Located>)>;
    let rows: Vec<Row<Ask>> = vec![
        Row {
            name: "the empty state names nothing",
            events: vec![],
            ask: vec![("thing:alpha#f1", None), ("nothing", None)],
        },
        Row {
            name: "a spelling the object answers to, and a label",
            events: vec![
                bound("alpha", "thing:alpha"),
                created("r1", claim("f1", "alpha", "c", 1)),
                created("r2", claim("f2", "alpha", "c", 1)),
            ],
            ask: vec![
                ("thing:alpha#f1", at("r1", "alpha")),
                ("thing:alpha#f2", at("r2", "alpha")),
                ("thing:alpha#f3", None),
                ("thing:alpha", None),
                ("thing:beta#f1", None),
            ],
        },
        Row {
            name: "the same label on two homes names two claims",
            events: vec![
                bound("alpha", "thing:alpha"),
                bound("beta", "thing:beta"),
                created("r1", claim("f1", "alpha", "c", 1)),
                created("r2", claim("f1", "beta", "c", 1)),
            ],
            ask: vec![
                ("thing:alpha#f1", at("r1", "alpha")),
                ("thing:beta#f1", at("r2", "beta")),
            ],
        },
        Row {
            name: "a spelling the object gave up still names it",
            events: vec![
                bound("alpha", "thing:gamma"),
                created("r1", claim("f1", "alpha", "c", 1)),
                bound("alpha", "thing:epsilon"),
                retired("alpha", "thing:gamma"),
            ],
            ask: vec![
                ("thing:gamma#f1", at("r1", "alpha")),
                ("thing:epsilon#f1", at("r1", "alpha")),
            ],
        },
        Row {
            name: "a spelling bound again to another object names that one",
            events: vec![
                bound("alpha", "thing:delta"),
                created("r1", claim("f1", "alpha", "c", 1)),
                created("r2", claim("f1", "beta", "c", 1)),
                retired("alpha", "thing:delta"),
                bound("beta", "thing:delta"),
            ],
            ask: vec![("thing:delta#f1", at("r2", "beta"))],
        },
        Row {
            name: "a spelling retired by an object that does not hold it changes nothing",
            events: vec![
                bound("alpha", "thing:alpha"),
                created("r1", claim("f1", "alpha", "c", 1)),
                retired("beta", "thing:alpha"),
            ],
            ask: vec![("thing:alpha#f1", at("r1", "alpha"))],
        },
        Row {
            name: "a revised label is served under the new label only",
            events: vec![
                bound("alpha", "thing:alpha"),
                created("r1", claim("f1", "alpha", "c", 1)),
                ClaimEvent::RecordRevised {
                    record: record("r1"),
                    claim: claim("f4", "alpha", "c", 1),
                },
            ],
            ask: vec![
                ("thing:alpha#f1", None),
                ("thing:alpha#f4", at("r1", "alpha")),
            ],
        },
        Row {
            name: "a merge serves the moved claim at its new label, under either object's spelling",
            events: vec![
                bound("alpha", "thing:alpha"),
                bound("beta", "thing:beta"),
                created("a1", claim("f1", "alpha", "c", 1)),
                created("b1", claim("f1", "beta", "c", 1)),
                merged("alpha", "beta", &[("a1", "f2")], &[]),
            ],
            ask: vec![
                ("thing:beta#f1", at("b1", "beta")),
                ("thing:beta#f2", at("a1", "beta")),
                ("thing:alpha#f2", at("a1", "beta")),
                ("thing:alpha#f1", at("b1", "beta")),
            ],
        },
        Row {
            name: "a merged object's retired spelling follows it to the survivor",
            events: vec![
                bound("alpha", "thing:gamma"),
                retired("alpha", "thing:gamma"),
                created("a1", claim("f1", "alpha", "c", 1)),
                merged("alpha", "beta", &[("a1", "f1")], &[]),
            ],
            ask: vec![("thing:gamma#f1", at("a1", "beta"))],
        },
        Row {
            name: "a claim a merge did not list keeps its label under the survivor",
            events: vec![
                bound("alpha", "thing:alpha"),
                created("a1", claim("f5", "alpha", "c", 1)),
                merged("alpha", "beta", &[], &[]),
            ],
            ask: vec![("thing:alpha#f5", at("a1", "beta"))],
        },
    ];
    for row in rows {
        let state = state(&row.events);
        for (address, want) in row.ask {
            assert_eq!(state.claim_at(address), want, "{}: {address}", row.name);
        }
    }
}

#[test]
fn a_label_is_a_claim_number_only_as_f_and_a_whole_number() {
    assert_eq!(claim_number("f12"), Some(12));
    for label in ["", "f", "f0", "f007", "x1", "F1", "f+1", "f1x", "1"] {
        assert_eq!(claim_number(label), None, "{label:?}");
    }
}

/// A small deterministic generator: the property needs repeatable runs, not
/// randomness.
struct Dice(u64);

impl Dice {
    fn roll(&mut self, below: usize) -> usize {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 33) as usize) % below
    }

    fn pick<T: Copy>(&mut self, from: &[T]) -> T {
        from[self.roll(from.len())]
    }
}

const OBJECTS: [&str; 6] = ["o0", "o1", "o2", "o3", "o4", "o5"];
const KEYS: [&str; 3] = [NUMBERS_BY, "num", "mood"];
const SPELLINGS: [&str; 4] = ["s0", "work:eta", "work:iota", "work:kappa"];

/// The next event of a run, read off the state the earlier ones made so that a
/// merge moves the claims and writes that are really there.
fn next_event(dice: &mut Dice, state: &State, made: &mut usize) -> ClaimEvent {
    *made += 1;
    let n = *made;
    let live: Vec<&str> = OBJECTS
        .iter()
        .copied()
        .filter(|name| !state.redirects.contains_key(&object(name)))
        .collect();
    let existing: Vec<&crate::log::RecordId> = state.records.keys().collect();
    match dice.roll(12) {
        0..=1 => {
            let id = dice.pick(&OBJECTS);
            let parent = dice.pick(&["o0", "o1", "o2", "o3", "o4", "o5", "-", "-"]);
            let parent = (parent != "-" && parent != id).then_some(parent);
            let body = ObjectBody {
                parent: parent.map(object),
                archived: dice.roll(4) == 0,
                kind: KindId("thing".into()),
                ..body("thing", id)
            };
            if state.objects.contains_key(&object(id)) {
                ClaimEvent::ObjectRevised {
                    object: object(id),
                    body,
                }
            } else {
                ClaimEvent::ObjectCreated {
                    object: object(id),
                    body,
                }
            }
        }
        2 => {
            let spelling = dice.pick(&SPELLINGS);
            let who = dice.pick(&OBJECTS);
            if dice.roll(3) == 0 {
                retired(who, spelling)
            } else {
                bound(who, spelling)
            }
        }
        3..=4 => {
            let label = if dice.roll(6) == 0 {
                format!("x{n}")
            } else {
                format!("f{n}")
            };
            created(&format!("r{n}"), claim(&label, dice.pick(&OBJECTS), "c", 1))
        }
        5 if !existing.is_empty() => {
            let record_id = dice.pick(&existing).clone();
            let held = &state.records[&record_id].claim;
            let mut revised = held.clone();
            match dice.roll(3) {
                0 => {
                    revised.status = if held.status == FactStatus::Active {
                        FactStatus::Archived
                    } else {
                        FactStatus::Active
                    }
                }
                1 => revised.label = format!("f{n}"),
                _ => revised.home = object(dice.pick(&OBJECTS)),
            }
            ClaimEvent::RecordRevised {
                record: record_id,
                claim: revised,
            }
        }
        6..=9 if !existing.is_empty() => {
            let key_name = dice.pick(&KEYS);
            let value = if key_name == NUMBERS_BY {
                "num".to_string()
            } else {
                ["1", "2", "3", "x", "007"][dice.roll(5)].to_string()
            };
            ClaimEvent::FieldWritten {
                record: dice.pick(&existing).clone(),
                // A write names the object a handle resolves to, never one
                // folded into another.
                subject: state.survivor(&object(dice.pick(&OBJECTS))).clone(),
                key: key(key_name),
                ordinal: 1 + dice.roll(5) as u64,
                class: crate::memory::WriteClass::Sets,
                write: if dice.roll(6) == 0 {
                    FieldValue::Clear
                } else {
                    FieldValue::Set(value)
                },
                targets: if dice.roll(2) == 0 {
                    vec![object(dice.pick(&OBJECTS)), object(dice.pick(&OBJECTS))]
                } else {
                    Vec::new()
                },
            }
        }
        10 | 11 if live.len() > 1 => {
            let folded = dice.pick(&live);
            let survivor = loop {
                let candidate = dice.pick(&live);
                if candidate != folded {
                    break candidate;
                }
            };
            let records: Vec<MovedRecord> = state
                .records
                .iter()
                .filter(|(_, held)| held.claim.home == object(folded) && dice.roll(4) != 0)
                .map(|(id, _)| MovedRecord {
                    record: id.clone(),
                    label: format!("f{}", 1000 + n),
                })
                .enumerate()
                .map(|(at, moved)| MovedRecord {
                    label: format!("{}{at}", moved.label),
                    ..moved
                })
                .collect();
            let writes: Vec<MovedWrite> = state
                .streams
                .iter()
                .filter(|((subject, _), _)| *subject == object(folded))
                .flat_map(|((_, k), stream)| {
                    stream.iter().map(|w| MovedWrite {
                        record: w.record.clone(),
                        key: k.clone(),
                        from_ordinal: w.ordinal,
                        ordinal: w.ordinal + 100,
                    })
                })
                .collect();
            ClaimEvent::ObjectsMerged {
                folded: object(folded),
                survivor: object(survivor),
                records,
                writes,
            }
        }
        _ => created(
            &format!("r{n}"),
            claim(&format!("f{n}"), dice.pick(&OBJECTS), "c", 1),
        ),
    }
}

/// A write of `owner` on `s` by `record_id`, pointing at `target`.
fn points(record_id: &str, ordinal: u64, target: &str) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(record_id),
        subject: object("o0"),
        key: key("owner"),
        ordinal,
        class: crate::memory::WriteClass::Sets,
        write: FieldValue::Set(target.to_string()),
        targets: vec![object(target)],
    }
}

/// **A clear takes the links off the thing only when the clearing claim holds
/// the key**, as the fold of the fields decides it. Two claims write `owner`, the
/// older naming `o1` and the newer `o2`; the older then clears. The newer still
/// holds the key, so the thing still points at `o2` and nothing resurrects `o1`.
/// Control: the newer claim clearing takes the key, and the link, off the thing.
#[test]
fn a_clear_by_a_claim_that_does_not_hold_the_key_keeps_the_links() {
    let setup = || {
        vec![
            thing("o0", None),
            thing("o1", None),
            thing("o2", None),
            created("r1", claim("f1", "o0", "c", 1)),
            points("r1", 1, "o1"),
            created("r2", claim("f2", "o0", "c", 1)),
            points("r2", 2, "o2"),
        ]
    };
    let mut older_clears = setup();
    older_clears.push(clear("r1", "o0", "owner", 3));
    let kept = state(&older_clears);
    assert_eq!(
        kept.links_from(&object("o0")),
        BTreeSet::from([object("o2")]),
        "the newer claim still holds the key"
    );
    assert_eq!(
        kept.links_into(&object("o2")),
        BTreeSet::from([object("o0")])
    );
    assert!(kept.links_into(&object("o1")).is_empty(), "o1 is not back");
    assert_eq!(
        links_from_by_scan(&kept, &object("o0")),
        kept.links_from(&object("o0"))
    );

    let mut holder_clears = setup();
    holder_clears.push(clear("r2", "o0", "owner", 3));
    let taken = state(&holder_clears);
    assert!(taken.links_from(&object("o0")).is_empty());
    assert!(taken.links_into(&object("o2")).is_empty());
    assert!(
        taken.links_into(&object("o1")).is_empty(),
        "no resurrection"
    );
}

/// **A stored removal takes the key off the thing whichever claim held it**
/// (rule 677), and the value, the backing and the links agree on it. Two claims
/// write `owner`; the older then has a removal stored. Control: a live clear by
/// the same older claim leaves the newer claim holding all three.
#[test]
fn a_stored_removal_by_a_claim_that_does_not_hold_the_key_takes_all_three_off() {
    let setup = || {
        vec![
            thing("o0", None),
            thing("o1", None),
            thing("o2", None),
            created("r1", claim("f1", "o0", "c", 1)),
            points("r1", 1, "o1"),
            created("r2", claim("f2", "o0", "c", 1)),
            points("r2", 2, "o2"),
        ]
    };
    let removal = ClaimEvent::FieldRemoved {
        record: record("r1"),
        subject: object("o0"),
        key: key("owner"),
        ordinal: 3,
        class: crate::memory::WriteClass::Sets,
        targets: Vec::new(),
    };
    let mut stored = setup();
    stored.push(removal);
    let removed = state(&stored);
    assert!(
        !removed.fields(&object("o0"), &[]).contains_key("owner"),
        "the value is off the thing"
    );
    assert!(
        !removed.backing(&object("o0"), &[]).contains_key("owner"),
        "no claim backs a key the thing does not hold"
    );
    assert!(removed.links_from(&object("o0")).is_empty());
    assert!(removed.links_into(&object("o2")).is_empty());

    let mut live = setup();
    live.push(clear("r1", "o0", "owner", 3));
    let kept = state(&live);
    assert_eq!(
        kept.fields(&object("o0"), &[])
            .get("owner")
            .map(String::as_str),
        Some("o2")
    );
    assert!(kept.backing(&object("o0"), &[]).contains_key("owner"));
    assert_eq!(
        kept.links_from(&object("o0")),
        BTreeSet::from([object("o2")])
    );
}

/// **A stored removal is no empty value.** A view declaration key such as
/// `selects` holds a blank as a value (rule 751), so a removal that arrived as a
/// blank would leave the key on the thing and backed. The removal takes it off;
/// an explicit empty write of the same key stays held.
#[test]
fn a_stored_removal_of_a_key_that_holds_blanks_takes_it_off_and_an_empty_write_does_not() {
    let setup = || {
        vec![
            thing("o0", None),
            created("r1", claim("f1", "o0", "c", 1)),
            write("r1", "o0", "selects", 1, "kind"),
        ]
    };
    let mut removed = setup();
    removed.push(ClaimEvent::FieldRemoved {
        record: record("r1"),
        subject: object("o0"),
        key: key("selects"),
        ordinal: 2,
        class: crate::memory::WriteClass::Sets,
        targets: Vec::new(),
    });
    let gone = state(&removed);
    assert!(!gone.fields(&object("o0"), &[]).contains_key("selects"));
    assert!(!gone.backing(&object("o0"), &[]).contains_key("selects"));

    let mut emptied = setup();
    emptied.push(write("r1", "o0", "selects", 2, ""));
    let held = state(&emptied);
    assert_eq!(
        held.fields(&object("o0"), &[])
            .get("selects")
            .map(String::as_str),
        Some(""),
        "an explicit empty write is a value"
    );
    assert!(held.backing(&object("o0"), &[]).contains_key("selects"));
}

/// **What a thing points at, read by scanning every stream of the state**: the
/// frozen targets of the newest write of each key, among the writes that reach
/// the thing and sit in an active claim, a cleared key pointing at none.
fn links_from_by_scan(state: &State, subject: &ObjectId) -> BTreeSet<ObjectId> {
    let subject = state.survivor(subject);
    let mut out = BTreeSet::new();
    for ((held, _), stream) in &state.streams {
        if held != subject {
            continue;
        }
        // The fold of the fields: a clear takes the key off only when the
        // clearing claim holds it.
        let mut ordered: Vec<_> = stream
            .iter()
            .filter(|write| {
                crate::memory::reaches_the_thing(write.class)
                    && state
                        .records
                        .get(&write.record)
                        .is_some_and(|record| record.claim.status == FactStatus::Active)
            })
            .collect();
        ordered.sort_by_key(|write| write.ordinal);
        let mut newest = None;
        for write in ordered {
            match write.write {
                FieldValue::Set(_) => newest = Some(write),
                FieldValue::Removed => newest = None,
                FieldValue::Clear => {
                    if newest
                        .is_some_and(|holding: &super::super::Write| holding.record == write.record)
                    {
                        newest = None;
                    }
                }
            }
        }
        if let Some(write) = newest {
            out.extend(
                write
                    .targets
                    .iter()
                    .map(|target| state.survivor(target).clone()),
            );
        }
    }
    out
}

/// **Every index, recomputed from the final state alone**, compared with what
/// the maps kept by `apply` answer.
fn assert_indexes_equal_a_recomputation(state: &State, merges: u64, context: &str) {
    assert_eq!(state.highest_merge_number(), merges, "{context}: merges");
    let survivor = |name: &str| state.survivor(&object(name)).clone();
    for name in OBJECTS {
        for k in KEYS {
            let stream = state.streams.get(&(survivor(name), key(k)));
            let want = stream
                .and_then(|stream| stream.iter().map(|w| w.ordinal).max())
                .map_or(1, |top| top + 1);
            assert_eq!(
                state.next_ordinal(&object(name), &key(k)),
                want,
                "{context}: next ordinal {name} {k}"
            );
        }
        let want = state
            .records
            .values()
            .filter(|held| state.survivor(&held.claim.home) == &survivor(name))
            .filter_map(|held| claim_number(&held.claim.label))
            .max()
            .unwrap_or(0);
        assert_eq!(
            state.highest_claim_number(&object(name)),
            want,
            "{context}: highest claim {name}"
        );
    }
    // The numbering: a member is under a container when the walk up by parent
    // meets it, and holds a number when the fold says so.
    for container in OBJECTS {
        let container = survivor(container);
        let Some(by) = state
            .held_value(&container, NUMBERS_BY)
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty())
        else {
            for number in 0..=8 {
                assert_eq!(
                    state.numbered_member(&container, "num", number),
                    None,
                    "{context}: {container:?} numbers nothing, yet holds {number}"
                );
            }
            continue;
        };
        for number in 0..=8u64 {
            let under: Vec<ObjectId> = OBJECTS
                .iter()
                .map(|name| survivor(name))
                .filter(|member| member != &container)
                .filter(|member| {
                    let mut at = member.clone();
                    let mut hops = 0;
                    while let Some(parent) = state
                        .objects
                        .get(&at)
                        .and_then(|body| body.parent.as_ref())
                        .map(|parent| state.survivor(parent).clone())
                    {
                        if parent == container {
                            return true;
                        }
                        at = parent;
                        hops += 1;
                        if hops > OBJECTS.len() {
                            break;
                        }
                    }
                    false
                })
                .collect();
            // A number a member under the container holds is issued. The converse
            // is not a function of the final state: a write made before the
            // container numbered, or before the member joined it, issued nothing.
            // So an issued number is only required to have been written by some
            // claim, and a number held now is required to be issued.
            let written_by_anyone = state.streams.iter().any(|((_, written), stream)| {
                written == &key(&by)
                    && stream.iter().any(|write| {
                        matches!(&write.write, FieldValue::Set(v)
                            if crate::memory::numbered::whole_number(v) == Some(number))
                    })
            });
            if state.number_issued(&container, &by, number) {
                assert!(
                    written_by_anyone,
                    "{context}: {container:?} {by} {number}: issued, yet no claim wrote it"
                );
            }
            if under.iter().any(|member| {
                state
                    .held_value(member, &by)
                    .and_then(|v| crate::memory::numbered::whole_number(&v))
                    == Some(number)
            }) {
                assert!(
                    state.number_issued(&container, &by, number),
                    "{context}: {container:?} {by} {number}: held, yet not issued"
                );
            }
            let want: BTreeSet<_> = under
                .iter()
                .filter(|member| {
                    state
                        .held_value(member, &by)
                        .and_then(|v| crate::memory::numbered::whole_number(&v))
                        == Some(number)
                })
                .cloned()
                .collect();
            let got = state.numbered_member(&container, &by, number);
            match got {
                None => assert!(
                    want.is_empty(),
                    "{context}: {container:?} {by} {number}: expected one of {want:?}"
                ),
                Some(member) => assert_eq!(
                    Some(member),
                    want.iter().next(),
                    "{context}: {container:?} {by} {number}"
                ),
            }
        }
    }
    // The handle an object answers to: the first `kind:slug` spelling bound to
    // it or to anything folded into it, as a scan of every address finds it.
    for name in OBJECTS {
        let want = state
            .addresses
            .iter()
            .find(|(address, bound)| {
                state.survivor(bound) == &survivor(name)
                    && address.contains(':')
                    && !address.contains('#')
            })
            .map(|(address, _)| crate::memory::EntityId(address.clone()));
        assert_eq!(
            state.handle_of(&object(name)),
            want,
            "{context}: handle of {name}"
        );
    }
    // The homes: every claim homed on a thing, whatever label it holds.
    for name in OBJECTS {
        let want: BTreeSet<&crate::log::RecordId> = state
            .records
            .iter()
            .filter(|(_, held)| state.survivor(&held.claim.home) == &survivor(name))
            .map(|(id, _)| id)
            .collect();
        let got: BTreeSet<&crate::log::RecordId> =
            state.records_homed_on(&object(name)).into_iter().collect();
        assert_eq!(got, want, "{context}: claims homed on {name}");
    }
    // The links: what a thing points at, and who points at a thing, as the scan
    // of every stream of the state reads them.
    for name in OBJECTS {
        let want_from = links_from_by_scan(state, &object(name));
        assert_eq!(
            state.links_from(&object(name)),
            want_from,
            "{context}: links from {name}"
        );
        let target = survivor(name);
        let subjects: BTreeSet<ObjectId> = state
            .streams
            .keys()
            .map(|(subject, _)| subject.clone())
            .collect();
        let want_into: BTreeSet<ObjectId> = subjects
            .into_iter()
            .filter(|subject| links_from_by_scan(state, subject).contains(&target))
            .collect();
        assert_eq!(
            state.links_into(&object(name)),
            want_into,
            "{context}: links into {name}"
        );
    }
    // The claim lookup: a spelling to the object it answers to now or once did.
    for spelling in SPELLINGS {
        let home = state.resolve(spelling).cloned().or_else(|| {
            state
                .retired_addresses
                .get(spelling)
                .map(|object| state.survivor(object).clone())
        });
        // A label no claim holds names nothing, whatever the spelling.
        let labels: BTreeSet<&str> = state
            .records
            .values()
            .map(|held| held.claim.label.as_str())
            .chain(["f9999"])
            .collect();
        for label in labels {
            let address = format!("{spelling}#{label}");
            let mut holders: Vec<Located> = home
                .iter()
                .flat_map(|home| {
                    state
                        .records
                        .iter()
                        .filter(move |(_, other)| {
                            &state.survivor(&other.claim.home).clone() == home
                                && other.claim.label == label
                        })
                        .map(move |(id, _)| Located {
                            record: id.clone(),
                            home: home.clone(),
                        })
                })
                .collect();
            // Two claims on one label of one home are a state no writer makes.
            if holders.len() > 1 {
                continue;
            }
            let want = holders.pop();
            assert_eq!(state.claim_at(&address), want, "{context}: {address}");
        }
    }
}

#[test]
fn the_indexes_equal_a_recomputation_after_any_run_of_events() {
    for seed in 0..60u64 {
        let mut dice = Dice(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
        let mut state = State::default();
        let mut made = 0;
        let mut merges = 0;
        // Every number seen issued under a container, by the name it was asked
        // by: a number issued once is issued under its survivor for good.
        let mut issued_seen: BTreeSet<(&str, u64)> = BTreeSet::new();
        for step in 0..160 {
            let event = next_event(&mut dice, &state, &mut made);
            if matches!(event, ClaimEvent::ObjectsMerged { .. }) {
                merges += 1;
            }
            apply(&mut state, &event);
            for (name, number) in &issued_seen {
                assert!(
                    state.number_issued(&object(name), "num", *number),
                    "seed {seed} step {step} after {event:?}: {name} num {number}: issued once, lost"
                );
            }
            for name in OBJECTS {
                for number in 0..=8 {
                    if state.number_issued(&object(name), "num", number) {
                        issued_seen.insert((name, number));
                    }
                }
            }
            assert_indexes_equal_a_recomputation(
                &state,
                merges,
                &format!("seed {seed} step {step} after {event:?}"),
            );
        }
    }
}
