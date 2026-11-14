//! **A State read back from its serialized form is the State the events built.** The snapshot
//! holds this State as bytes, and a boot goes on from it with the tail of the log, so what a
//! restore leaves out, a later event reads wrong. The cases compare the whole State, the fields
//! no equality compares included, and then go on from both with the same events.

use super::tests::{body, claim, clear, created, envelope, key, object, record, write};
use super::*;

/// **An event as the log carries it**: the operator's envelope with a run and a moment of its
/// own, later for each event. A fold's ages and a thing's newest change are worked out from the
/// moments, so events with none would leave them empty and a restore that lost them unseen.
fn stamped(index: usize, payload: ClaimEvent) -> Event<ClaimEvent> {
    let mut event = envelope(payload);
    event.run = Some(RunId("run-a".into()));
    event.written_at =
        Some(Timestamp::from_second(1_700_000_000 + 60 * index as i64).expect("a moment in range"));
    event
}

fn replay_stamped(events: &[ClaimEvent]) -> State {
    let mut state = State::default();
    for (index, event) in events.iter().enumerate() {
        state.apply(&stamped(index, event.clone()));
    }
    state
}

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
        // A child of alpha, with a claim of its own that carries a number under alpha, which
        // numbers its members by that key: a seat held, and one issued.
        ClaimEvent::ObjectCreated {
            object: object("epsilon"),
            body: ObjectBody {
                parent: Some(object("alpha")),
                ..body("thing", "Epsilon")
            },
        },
        bound("epsilon", "thing:epsilon"),
        created("r4", claim("f4", "epsilon", "epsilon says", 4)),
        write("r1", "alpha", memory::numbered::NUMBERS_BY, 6, "number"),
        write("r4", "epsilon", "number", 1, "7"),
        // A link: a write that names another object, so the reference indexes hold it.
        ClaimEvent::FieldWritten {
            record: record("r4"),
            subject: object("epsilon"),
            key: key("friend"),
            ordinal: 1,
            class: WriteClass::Sets,
            write: FieldValue::Set("thing:beta".into()),
            targets: vec![object("beta")],
        },
        // A claim worked out from another, and the record that carries the release notes.
        created(
            "r5",
            ClaimBody {
                derived_from: Some(record("r1")),
                ..claim("f5", "alpha", "worked out", 5)
            },
        ),
        write(
            "r5",
            "alpha",
            memory::capacity::ENGINE_RECORD,
            7,
            memory::capacity::RELEASE_NOTES_RECORD,
        ),
        // The write of an object's own row that an import observed.
        ClaimEvent::ObjectWriteObserved {
            object: object("alpha"),
            filed_under: "alpha".into(),
            ordinal: 1,
            snapshot: Some(ObjectSnapshot {
                kind: "thing".into(),
                name: "Alpha".into(),
                source: "user-named".into(),
                crm: None,
                parent: None,
                boot: String::new(),
                archived_reason: None,
                archived_at: None,
                merged_into: None,
                aliases: vec!["A".into()],
                prose: "the first".into(),
            }),
        },
        ClaimEvent::ImportCompleted { events: 20 },
    ]
}

/// What goes on after the restore: a write the declaration folds, a declaration made again, a
/// write that changes what a thing holds, and a withdrawal.
fn tail() -> Vec<ClaimEvent> {
    vec![
        write("r1", "alpha", "rank", 2, "11"),
        declaring("thing", None),
        write("r1", "alpha", "mood", 9, "late"),
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
    built
        .held_fields_are_what_a_fold_makes()
        .unwrap_or_else(|why| panic!("{what}: the State the events built: {why}"));
    restored
        .held_fields_are_what_a_fold_makes()
        .unwrap_or_else(|why| panic!("{what}: the restored State: {why}"));
}

#[test]
fn a_restored_state_is_the_state_the_events_built_and_goes_on_as_it_does() {
    let all = events();
    let built = replay_stamped(&all);
    assert!(
        !built.declared_types().is_empty(),
        "the fixture declares a type, or the shared declarations are never exercised"
    );
    let mut restored = restored_from(&built);
    same(&restored, &built, "after the restore");

    // The same tail applied to both: a part the restore lost shows as a difference here even
    // when the States looked equal before it.
    let mut built = built;
    let after = all.len();
    for (index, event) in tail().into_iter().enumerate() {
        built.apply(&stamped(after + index, event.clone()));
        restored.apply(&stamped(after + index, event));
        same(&restored, &built, &format!("after tail event {index}"));
    }
}

#[test]
fn a_state_with_no_event_restores_to_the_empty_state() {
    let empty = State::default();
    same(&restored_from(&empty), &empty, "an empty State");
}

/// **An entry that appears twice is refused when a State is read back.** The maps whose keys
/// are not text are stored as lists of entries; reading them into a map would let the later of
/// two entries for one key replace the earlier without a word, so a damaged snapshot would load
/// as a State that differs from any the events built.
#[test]
fn an_entry_that_appears_twice_is_refused_on_load() {
    let built = replay_stamped(&events());
    let json = serde_json::to_value(&built).expect("the State serializes");
    let lists: [&[&str]; 4] = [
        &["streams"],
        &["indexes", "top_ordinal"],
        &["indexes", "labels"],
        &["indexes", "origins"],
    ];
    for path in lists {
        let mut doubled = json.clone();
        let list = path
            .iter()
            .fold(&mut doubled, |value, name| &mut value[*name])
            .as_array_mut()
            .unwrap_or_else(|| panic!("{path:?} is not a list of entries"));
        assert!(
            !list.is_empty(),
            "{path:?}: the fixture holds nothing here, so the case proves nothing"
        );
        let first = list[0].clone();
        list.push(first);
        assert!(
            serde_json::from_value::<State>(doubled).is_err(),
            "{path:?}: an entry that appears twice was read back"
        );
    }
}

/// **The parts of a serialized State the fixture leaves empty**, by path: every list and map
/// under the State and under its indexes, and the folded fields' ages and newest moments. A
/// part the fixture does not populate is a part the restore case cannot catch a loss of.
fn empty_parts(state: &State) -> Vec<String> {
    let json = serde_json::to_value(state).expect("the State serializes");
    let mut empty = Vec::new();
    let mut visit = |prefix: &str, value: &serde_json::Value| {
        if let Some(fields) = value.as_object() {
            for (name, part) in fields {
                let hollow = match part {
                    serde_json::Value::Array(items) => items.is_empty(),
                    serde_json::Value::Object(entries) => entries.is_empty(),
                    _ => false,
                };
                if hollow {
                    empty.push(format!("{prefix}{name}"));
                }
            }
        }
    };
    visit("", &json);
    visit("indexes.", &json["indexes"]);
    visit("held_fields.", &json["held_fields"]);
    let things = json["held_fields"]["things"].as_object();
    let aged = things.is_some_and(|things| {
        things.values().any(|thing| {
            thing["keys"]
                .as_object()
                .is_some_and(|keys| keys.values().any(|key| !key["age"].is_null()))
        })
    });
    if !aged {
        empty.push("held_fields.things.*.keys.*.age (no held key carries an age)".into());
    }
    let dated =
        things.is_some_and(|things| things.values().any(|thing| !thing["newest"].is_null()));
    if !dated {
        empty.push("held_fields.things.*.newest (no thing carries a newest moment)".into());
    }
    empty
}

#[test]
fn the_fixture_populates_every_part_of_the_state() {
    let empty = empty_parts(&replay_stamped(&events()));
    assert!(
        empty.is_empty(),
        "the restore fixture leaves these parts of the State empty, so a restore that lost them \
         would pass: {empty:?}"
    );
}
