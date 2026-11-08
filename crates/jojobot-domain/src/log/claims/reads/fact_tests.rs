use super::super::tests::{body, key, object, record, under};
use super::super::{ClaimEvent, FieldValue, State};
use crate::memory::{EntityId, WriteClass, mention::MARK};

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

fn write(
    id: &str,
    k: &str,
    ordinal: u64,
    class: WriteClass,
    value: FieldValue,
    targets: &[&str],
) -> ClaimEvent {
    ClaimEvent::FieldWritten {
        record: record(id),
        subject: object("k1"),
        key: key(k),
        ordinal,
        class,
        write: value,
        targets: targets.iter().map(|t| object(t)).collect(),
    }
}

fn state_of(events: impl IntoIterator<Item = ClaimEvent>) -> State {
    crate::memory::kinds::load_shipped();
    let mut state = State::default();
    for event in events {
        state.apply(&under(None, None, event));
    }
    state
}

fn base() -> Vec<ClaimEvent> {
    made("k1", "person:alpha")
        .into_iter()
        .chain(made("k2", "person:beta"))
        .chain(made("k3", "person:gamma"))
        .chain([ClaimEvent::RecordCreated {
            record: record("r1"),
            claim: super::super::tests::claim("f1", "k1", "a claim", 1),
        }])
        .collect()
}

/// **A claim serves the things its field names in the order the value names
/// them**, each once, as the store serves them. The write's own targets are kept
/// in the order of their ids, which is another order.
#[test]
fn a_claim_serves_its_links_in_the_order_the_value_names_them() {
    let value = format!("{MARK}k3, {MARK}k2");
    let state = state_of(base().into_iter().chain([write(
        "r1",
        "tags",
        1,
        WriteClass::Stays,
        FieldValue::Set(value),
        &["k2", "k3"],
    )]));
    let fact = state.fact(&record("r1")).expect("the claim");
    assert_eq!(
        fact.field_links["tags"],
        vec![
            EntityId("person:gamma".into()),
            EntityId("person:beta".into())
        ]
    );
}

/// **A clear takes the key out of both bags a claim serves**: the keys it sets on
/// its thing and the keys that reach the thing, as the fold of a record's writes
/// does. A key it set and has not cleared stays in them.
#[test]
fn a_cleared_key_is_in_neither_bag_of_the_claim() {
    let state = state_of(base().into_iter().chain([
        write(
            "r1",
            "status",
            1,
            WriteClass::Sets,
            FieldValue::Set("now".into()),
            &[],
        ),
        write(
            "r1",
            "rank",
            1,
            WriteClass::Sets,
            FieldValue::Set("2".into()),
            &[],
        ),
        write("r1", "status", 2, WriteClass::Sets, FieldValue::Clear, &[]),
    ]));
    let fact = state.fact(&record("r1")).expect("the claim");
    let sets: Vec<&str> = fact.sets.iter().map(String::as_str).collect();
    let reaching: Vec<&str> = fact.reaching.iter().map(String::as_str).collect();
    assert_eq!(sets, ["rank"]);
    assert_eq!(reaching, ["rank"]);
}
