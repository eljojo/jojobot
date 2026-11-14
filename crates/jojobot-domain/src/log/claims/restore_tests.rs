//! **A State read back from its serialized form is the State the events built.** The snapshot
//! holds this State as bytes, and a boot goes on from it with the tail of the log, so what a
//! restore leaves out, a later event reads wrong. The cases compare the whole State, the fields
//! no equality compares included, and then go on from both with the same events.

use super::tests::{body, claim, clear, created, envelope, key, object, record, replay, write};
use super::*;

fn bound(name: &str, address: &str) -> ClaimEvent {
    ClaimEvent::AddressBound {
        object: object(name),
        address: address.into(),
    }
}

/// A declaration that names one key, so a fold of the kind reads it.
fn declaring(kind: &str, must: Option<&str>) -> ClaimEvent {
    ClaimEvent::TypeDeclared {
        kind: object(kind),
        declaration: memory::types::DeclaredType {
            name: kind.to_string(),
            fields: vec![memory::types::Field::new(
                "rank",
                memory::types::ValueType::Number,
            )],
            origin: memory::types::Origin::Declared,
            must: must.map(str::to_string),
        },
    }
}

/// **Every part of the State, each at least once**: things and their spellings (one given up),
/// records, key writes of both classes, a clear, a taken-back claim, a declaration made twice,
/// a merge with moved writes, a redirect and the completed import.
fn events() -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object("alpha"),
            body: body("thing", "Alpha"),
        },
        ClaimEvent::ObjectCreated {
            object: object("beta"),
            body: body("thing", "Beta"),
        },
        ClaimEvent::ObjectCreated {
            object: object("gamma"),
            body: body("thing", "Gamma"),
        },
        bound("alpha", "thing:alpha"),
        bound("beta", "thing:beta"),
        bound("gamma", "thing:gamma"),
        ClaimEvent::AddressRetired {
            object: object("alpha"),
            address: "thing:alpha".into(),
            day: jiff::civil::date(2026, 4, 3),
        },
        bound("alpha", "thing:delta"),
        declaring("thing", None),
        created("r1", claim("f1", "alpha", "alpha says", 1)),
        created("r2", claim("f1", "beta", "beta says", 2)),
        created("r3", claim("f2", "alpha", "alpha again", 3)),
        write("r1", "alpha", "mood", 1, "tense"),
        write("r3", "alpha", "mood", 3, "steady"),
        write("r1", "alpha", "rank", 1, "4"),
        write("r2", "beta", "mood", 5, "calm"),
        write("r2", "beta", "rank", 2, "9"),
        clear("r3", "alpha", "mood", 4),
        super::tests::archived("r3", "f2", "alpha"),
        declaring("thing", Some("rank")),
        ClaimEvent::ObjectsMerged {
            folded: object("beta"),
            survivor: object("alpha"),
            records: vec![MovedRecord {
                record: record("r2"),
                label: "f3".into(),
            }],
            writes: vec![MovedWrite {
                record: record("r2"),
                key: key("mood"),
                from_ordinal: 5,
                ordinal: 2,
            }],
        },
        ClaimEvent::ObjectRedirected {
            folded: object("gamma"),
            survivor: object("alpha"),
        },
        ClaimEvent::ImportCompleted { events: 20 },
    ]
}

/// What goes on after the restore: a write the declaration folds, a declaration made again, and
/// a write to a thing the merge folded away.
fn tail() -> Vec<ClaimEvent> {
    vec![
        write("r1", "alpha", "rank", 2, "11"),
        declaring("thing", None),
        write("r1", "beta", "mood", 7, "late"),
        ClaimEvent::TypeWithdrawn {
            kind: object("thing"),
        },
    ]
}

fn restored_from(state: &State) -> State {
    serde_json::from_slice::<State>(&serde_json::to_vec(state).expect("the State serializes"))
        .expect("the serialized State reads back")
        .restored()
}

/// **The comparison no equality makes**: `State`'s equality leaves the folded fields out, and
/// the shared declarations are a copy of them, so the whole State is compared as it prints.
fn same(restored: &State, built: &State, what: &str) {
    assert_eq!(restored, built, "{what}: the States differ");
    assert_eq!(
        format!("{restored:?}"),
        format!("{built:?}"),
        "{what}: a part no equality compares differs"
    );
    restored
        .held_fields_are_what_a_fold_makes()
        .unwrap_or_else(|why| panic!("{what}: {why}"));
}

#[test]
fn a_restored_state_is_the_state_the_events_built_and_goes_on_as_it_does() {
    let all = events();
    let built = replay(&all);
    assert!(
        !built.declared_types().is_empty(),
        "the fixture declares a type, or the shared declarations are never exercised"
    );
    let mut restored = restored_from(&built);
    same(&restored, &built, "after the restore");

    // The same tail applied to both: a part the restore lost shows as a difference here even
    // when the States looked equal before it.
    let mut built = built;
    for event in tail() {
        built.apply(&envelope(event.clone()));
        restored.apply(&envelope(event));
        same(&restored, &built, "after a tail event");
    }
}

#[test]
fn a_state_with_no_event_restores_to_the_empty_state() {
    let empty = State::default();
    same(&restored_from(&empty), &empty, "an empty State");
}
