use std::collections::BTreeSet;

use super::super::tests::{
    apply, archived, body, claim, clear, created, key, object, record, replay, write,
};
use super::super::{ClaimEvent, FieldValue, KindId, MovedRecord, MovedWrite, ObjectBody, State};
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
                        clear("w2", "m1", "num", 2),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None)],
        },
        Row {
            name: "a container that stops numbering holds nothing, and holds it again when it starts",
            events: with(
                collection(),
                [
                    vec![thing("m1", Some("coll"))],
                    holds("w1", "m1", "num", "1"),
                    vec![
                        created("w2", claim("fw2", "coll", "c", 2)),
                        clear("w2", NUMBERS_BY.len().to_string().as_str(), "x", 1),
                        clear("w2", "coll", NUMBERS_BY, 2),
                    ],
                ]
                .concat(),
            ),
            ask: vec![("coll", "num", 1, None)],
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
const SPELLINGS: [&str; 4] = ["s0", "s1", "s2", "s3"];

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
                subject: object(dice.pick(&OBJECTS)),
                key: key(key_name),
                ordinal: 1 + dice.roll(5) as u64,
                class: crate::memory::WriteClass::Sets,
                write: if dice.roll(6) == 0 {
                    FieldValue::Clear
                } else {
                    FieldValue::Set(value)
                },
                targets: Vec::new(),
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
        for held in state.records.values() {
            let address = format!("{name}#{}", held.claim.label);
            let _ = address;
        }
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
            let want: BTreeSet<_> = OBJECTS
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
                .filter(|member| {
                    state
                        .held_value(member, &by)
                        .and_then(|v| crate::memory::numbered::whole_number(&v))
                        == Some(number)
                })
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
    // The claim lookup: a spelling to the object it answers to now or once did.
    for spelling in SPELLINGS {
        let home = state.resolve(spelling).cloned().or_else(|| {
            state
                .retired_addresses
                .get(spelling)
                .map(|object| state.survivor(object).clone())
        });
        for held in state.records.values() {
            let address = format!("{spelling}#{}", held.claim.label);
            let want = home.as_ref().and_then(|home| {
                let mut found = state.records.iter().filter(|(_, other)| {
                    &state.survivor(&other.claim.home).clone() == home
                        && other.claim.label == held.claim.label
                });
                let first = found.next()?;
                found.next().is_none().then(|| Located {
                    record: first.0.clone(),
                    home: home.clone(),
                })
            });
            let got = state.claim_at(&address);
            // Two claims on one label of one home are a state no writer makes.
            if want.is_some() || home.is_none() {
                assert_eq!(got, want, "{context}: {address}");
            }
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
        for step in 0..160 {
            let event = next_event(&mut dice, &state, &mut made);
            if matches!(event, ClaimEvent::ObjectsMerged { .. }) {
                merges += 1;
            }
            apply(&mut state, &event);
            assert_indexes_equal_a_recomputation(
                &state,
                merges,
                &format!("seed {seed} step {step} after {event:?}"),
            );
        }
    }
}
