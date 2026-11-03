use std::collections::{BTreeMap, BTreeSet};

use super::super::tests::{
    apply, archived, body, claim, created, day, object, record, replay, under, write,
};
use super::super::{ClaimBody, ClaimEvent, ObjectBody, State};
use super::{BootFloor, CaptureRequest, ClaimSize, Door, Draft, Measures, Refusal, Route, prepare};
use crate::actor::Actor;
use crate::app::AppId;
use crate::memory::numbered::NUMBERS_BY;
use crate::memory::receipt::Receipt;
use crate::memory::types::{DeclaredType, Field, ValueType};
use crate::memory::{EdgeShape, FactStatus, MemoryError, Provenance, Standing};

fn thing(id: &str) -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object(id),
            body: ObjectBody {
                ..body("thing", id)
            },
        },
        ClaimEvent::AddressBound {
            object: object(id),
            address: id.to_string(),
        },
    ]
}

/// Two homes and a claim on `alpha`; `ref-key` is a declared reference.
fn world() -> (State, Vec<DeclaredType>) {
    let events = [
        thing("alpha"),
        thing("beta"),
        vec![created("r1", claim("f1", "alpha", "first", 1))],
    ]
    .concat();
    let declared = vec![DeclaredType::new(
        "thing",
        vec![Field::new("ref-key", ValueType::Reference)],
    )];
    (replay(&events), declared)
}

fn request(subject: &str) -> CaptureRequest {
    CaptureRequest {
        record: record("new"),
        route: Route::Verb,
        door: Door {
            actor: Actor::Operator,
            owned_keys: BTreeSet::new(),
            owner_revoked: false,
            measures: Measures::default(),
            runs: BTreeMap::new(),
            run: None,
            aged_before: None,
        },
        subject: subject.to_string(),
        draft: Draft {
            content: "a claim".to_string(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: day(3),
            happened_at: None,
            happened_through: None,
            stale_after: None,
            receipt: None,
            app: None,
        },
        edge: None,
        refs: Vec::new(),
        fields: BTreeMap::new(),
        sets: BTreeSet::new(),
        derived: BTreeSet::new(),
        derived_from: None,
        borrow: false,
        drop: None,
    }
}

struct Row {
    name: &'static str,
    request: CaptureRequest,
    refused: Refusal,
}

/// **A request with two faults is told the first.** Each row keeps the first
/// fault in the order and adds the second, so a row fails when the order
/// between the two changes.
#[test]
fn two_faults_are_told_in_the_order() {
    let (state, declared) = world();
    let rows = vec![
        Row {
            name: "a malformed write before a missing subject",
            request: CaptureRequest {
                sets: BTreeSet::from(["absent".to_string()]),
                ..request("nobody")
            },
            refused: Refusal::Malformed(
                "'absent' is named as a setting but carries no value: a setting is a key with a \
                 value, and a key is taken off with clear_fields"
                    .to_string(),
            ),
        },
        Row {
            name: "a missing subject before a missing edge object",
            request: CaptureRequest {
                edge: Some((EdgeShape::About, "ghost".into())),
                ..request("nobody")
            },
            refused: Refusal::UnknownSubject("nobody".into()),
        },
        Row {
            name: "a missing edge object before a missing ref",
            request: CaptureRequest {
                edge: Some((EdgeShape::About, "ghost".into())),
                refs: vec!["phantom".into()],
                ..request("alpha")
            },
            refused: Refusal::UnknownEdgeObject("ghost".into()),
        },
        Row {
            name: "a missing ref before a missing reference field object",
            request: CaptureRequest {
                refs: vec!["phantom".into()],
                fields: BTreeMap::from([("ref-key".into(), "person:moe".into())]),
                ..request("alpha")
            },
            refused: Refusal::UnknownRef("phantom".into()),
        },
        Row {
            name: "a missing reference field object before a missing source",
            request: CaptureRequest {
                fields: BTreeMap::from([("ref-key".into(), "person:moe".into())]),
                derived_from: Some("nobody#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownFieldTarget("person:moe".into()),
        },
        Row {
            name: "a source whose home is unknown",
            request: CaptureRequest {
                derived_from: Some("nobody#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownSourceHome("nobody#f1".into()),
        },
        Row {
            name: "a source whose home holds no such claim",
            request: CaptureRequest {
                derived_from: Some("beta#f1".into()),
                ..request("alpha")
            },
            refused: Refusal::UnknownSourceClaim("beta#f1".into()),
        },
    ];
    for row in rows {
        assert_eq!(
            prepare(&state, &declared, &row.request),
            Err(row.refused),
            "{}",
            row.name
        );
    }
}

/// Refusing changes nothing.
#[test]
fn a_refused_request_leaves_the_state_as_it_was() {
    let (state, declared) = world();
    let before = state.clone();
    assert!(prepare(&state, &declared, &request("nobody")).is_err());
    assert_eq!(state, before);
}

/// **The claim takes the highest held number plus one**, not the count of what
/// is held: a gap does not hand a number out twice.
#[test]
fn the_label_is_the_highest_held_number_plus_one() {
    let (mut state, declared) = world();
    apply(&mut state, &created("r9", claim("f7", "alpha", "gap", 1)));
    let events = prepare(&state, &declared, &request("alpha")).expect("admitted");
    let ClaimEvent::RecordCreated { claim, .. } = &events[0] else {
        panic!("the first event creates the claim: {events:?}");
    };
    assert_eq!(claim.label, "f8");
}

/// **A key takes the place after the highest write in its stream.**
#[test]
fn a_key_takes_the_next_place_in_its_stream() {
    let (mut state, declared) = world();
    apply(&mut state, &write("r1", "alpha", "colour", 4, "red"));
    let events = prepare(
        &state,
        &declared,
        &CaptureRequest {
            fields: BTreeMap::from([
                ("colour".into(), "blue".into()),
                ("size".into(), "9".into()),
            ]),
            ..request("alpha")
        },
    )
    .expect("admitted");
    let places: Vec<(String, u64)> = events
        .iter()
        .filter_map(|event| match event {
            ClaimEvent::FieldWritten { key, ordinal, .. } => Some((key.0.clone(), *ordinal)),
            _ => None,
        })
        .collect();
    assert_eq!(
        places,
        vec![("colour".to_string(), 5), ("size".to_string(), 1)]
    );
}

/// **An own field stays on its claim; a setting and a computed key reach the
/// thing.** Read through the state after the events apply, not off the events.
#[test]
fn own_fields_stay_while_settings_and_computed_keys_reach() {
    let (mut state, declared) = world();
    let events = prepare(
        &state,
        &declared,
        &CaptureRequest {
            fields: BTreeMap::from([
                ("own_key".into(), "o".into()),
                ("set_key".into(), "s".into()),
                ("computed_key".into(), "c".into()),
            ]),
            sets: BTreeSet::from(["set_key".to_string()]),
            derived: BTreeSet::from(["computed_key".to_string()]),
            ..request("alpha")
        },
    )
    .expect("admitted");
    for event in &events {
        apply(&mut state, event);
    }
    let held = state.fields(&object("alpha"), &declared);
    assert!(!held.contains_key("own_key"), "{held:?}");
    assert_eq!(held["set_key"], "s");
    assert_eq!(held["computed_key"], "c");
}

/// A container `coll` numbering by `num`, with a live member holding 1, an
/// archived member holding 2, and a member `m3` holding nothing yet.
fn numbered_world() -> (State, Vec<DeclaredType>) {
    let member = |id: &str, archived: bool| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: ObjectBody {
                    parent: Some(object("coll")),
                    archived,
                    ..body("thing", id)
                },
            },
            ClaimEvent::AddressBound {
                object: object(id),
                address: id.to_string(),
            },
        ]
    };
    let holds = |record_id: &str, subject: &str, k: &str, value: &str| {
        vec![
            created(record_id, claim(&format!("f{record_id}"), subject, "c", 1)),
            write(record_id, subject, k, 1, value),
        ]
    };
    let events = [
        thing("coll"),
        holds("c1", "coll", NUMBERS_BY, "num"),
        member("m1", false),
        holds("n1", "m1", "num", "1"),
        member("m2", true),
        holds("n2", "m2", "num", "2"),
        member("m3", false),
    ]
    .concat();
    (replay(&events), Vec::new())
}

fn numbering(subject: &str, number: &str) -> CaptureRequest {
    CaptureRequest {
        fields: BTreeMap::from([("num".into(), number.into())]),
        sets: BTreeSet::from(["num".to_string()]),
        ..request(subject)
    }
}

/// **No two members of a container hold one number**, and an archived member
/// keeps its own.
#[test]
fn a_member_cannot_take_a_number_another_member_holds() {
    let (state, declared) = numbered_world();
    let held = |number: u64, holder: &str| {
        Err(Refusal::NumberHeld {
            container: object("coll"),
            key: "num".into(),
            number,
            holder: object(holder),
        })
    };
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "1")),
        held(1, "m1"),
        "a live member's number"
    );
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "2")),
        held(2, "m2"),
        "an archived member's number"
    );
    assert!(
        prepare(&state, &declared, &numbering("m3", "3")).is_ok(),
        "a number nobody holds"
    );
    assert!(
        prepare(&state, &declared, &numbering("m1", "1")).is_ok(),
        "a member keeps writing the number it holds"
    );
}

/// A number is judged after every name the write mentions.
#[test]
fn a_missing_source_is_told_before_a_held_number() {
    let (state, declared) = numbered_world();
    let request = CaptureRequest {
        derived_from: Some("nobody#f1".into()),
        ..numbering("m3", "1")
    };
    assert_eq!(
        prepare(&state, &declared, &request),
        Err(Refusal::UnknownSourceHome("nobody#f1".into()))
    );
}

/// **A number a taken-back claim held is never issued again**, though no member
/// holds it now.
#[test]
fn a_member_cannot_take_a_number_a_taken_back_claim_held() {
    let (mut state, declared) = numbered_world();
    apply(&mut state, &created("t1", claim("f1", "m3", "c", 1)));
    apply(&mut state, &write("t1", "m3", "num", 1, "4"));
    assert!(
        prepare(&state, &declared, &numbering("m3", "4")).is_ok(),
        "a claim that stands holds the number"
    );
    apply(&mut state, &archived("t1", "f1", "m3"));
    let retired = Err(Refusal::NumberRetired {
        container: object("coll"),
        key: "num".into(),
        number: 4,
    });
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "4")),
        retired,
        "the member that held it"
    );
    assert_eq!(
        prepare(&state, &declared, &numbering("m1", "4")),
        retired,
        "another member"
    );
}

/// **A number a taken-back claim held stays retired when its container is
/// merged into another**: the store derives retired numbers from the members'
/// claims, and a container merge changes none of them.
#[test]
fn a_number_stays_retired_after_its_container_is_merged() {
    let (mut state, declared) = numbered_world();
    for event in thing("old") {
        apply(&mut state, &event);
    }
    apply(&mut state, &created("c2", claim("fc2", "old", "c", 1)));
    apply(&mut state, &write("c2", "old", NUMBERS_BY, 1, "num"));
    apply(
        &mut state,
        &ClaimEvent::ObjectCreated {
            object: object("mo"),
            body: ObjectBody {
                parent: Some(object("old")),
                ..body("thing", "mo")
            },
        },
    );
    apply(&mut state, &created("t1", claim("f1", "mo", "c", 1)));
    apply(&mut state, &write("t1", "mo", "num", 1, "5"));
    apply(&mut state, &archived("t1", "f1", "mo"));
    apply(
        &mut state,
        &ClaimEvent::ObjectsMerged {
            folded: object("old"),
            survivor: object("coll"),
            records: Vec::new(),
            writes: Vec::new(),
        },
    );
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "5")),
        Err(Refusal::NumberRetired {
            container: object("coll"),
            key: "num".into(),
            number: 5,
        })
    );
}

/// **A role's lease fields belong to the boot door.** The verb is refused them
/// before any object is looked up; the engine's own routes write them.
#[test]
fn a_role_lease_field_is_refused_on_the_verb_and_not_on_the_engine() {
    let (state, declared) = world();
    let leased = |route, subject: &str, extra: Option<&str>| CaptureRequest {
        route,
        fields: BTreeMap::from([("holder".into(), "x".into())]),
        sets: extra.map(|key| key.to_string()).into_iter().collect(),
        ..request(subject)
    };
    let refused = Err(Refusal::RoleFieldGuarded {
        role: "alpha".into(),
        key: "holder".into(),
    });
    assert_eq!(
        prepare(&state, &declared, &leased(Route::Verb, "role:alpha", None)),
        refused,
        "on a role that nobody has made"
    );
    assert_eq!(
        prepare(
            &state,
            &declared,
            &leased(Route::Verb, "role:alpha", Some("absent"))
        ),
        refused,
        "before a malformed write"
    );
    assert_eq!(
        prepare(
            &state,
            &declared,
            &leased(Route::Engine, "role:alpha", None)
        ),
        Err(Refusal::UnknownSubject("role:alpha".into())),
        "the engine's route skips the guard"
    );
    assert!(
        prepare(&state, &declared, &leased(Route::Verb, "alpha", None)).is_ok(),
        "a key named holder on a thing that is no role"
    );
}

fn app(name: &str) -> AppId {
    AppId::parse(name).expect("a valid app name")
}

/// `bound` is bound to the app `bart-ci` and owns the key `state`; `free` is
/// bound to nobody.
fn bound_world() -> (State, Vec<DeclaredType>) {
    let mut events = [thing("bound"), thing("free")].concat();
    events[0] = ClaimEvent::ObjectCreated {
        object: object("bound"),
        body: ObjectBody {
            bound_to: Some(app("bart-ci")),
            ..body("thing", "bound")
        },
    };
    (replay(&events), Vec::new())
}

fn writing(subject: &str, key: &str, actor: Actor, revoked: bool) -> CaptureRequest {
    CaptureRequest {
        door: Door {
            actor,
            owned_keys: BTreeSet::from(["state".to_string()]),
            owner_revoked: revoked,
            measures: Measures::default(),
            runs: BTreeMap::new(),
            run: None,
            aged_before: None,
        },
        fields: BTreeMap::from([(key.into(), "v".into())]),
        ..request(subject)
    }
}

/// **Only the bound app writes the keys it owns**, and only while its grant
/// stands. A key the kind does not mark, and a subject bound to nobody, are
/// open.
#[test]
fn only_the_bound_app_writes_an_owned_key() {
    let (state, declared) = bound_world();
    let owned = |key: &str| Refusal::OwnedKeyWritten {
        subject: "bound".into(),
        key: key.into(),
    };
    let rows = [
        (
            "the bound app",
            writing("bound", "state", Actor::App(app("bart-ci")), false),
            None,
        ),
        (
            "another app",
            writing("bound", "state", Actor::App(app("lisa-ci")), false),
            Some(owned("state")),
        ),
        (
            "a bot",
            writing(
                "bound",
                "state",
                Actor::Bot(crate::memory::EntityId("bot:sigma".into())),
                false,
            ),
            Some(owned("state")),
        ),
        (
            "the operator through this verb",
            writing("bound", "state", Actor::Operator, false),
            Some(owned("state")),
        ),
        (
            "the engine",
            writing("bound", "state", Actor::System("sweep".into()), false),
            Some(owned("state")),
        ),
        (
            "a key the kind does not mark",
            writing("bound", "note", Actor::Operator, false),
            None,
        ),
        (
            "a subject bound to nobody",
            writing("free", "state", Actor::Operator, false),
            None,
        ),
        (
            "a revoked owner, itself included",
            writing("bound", "state", Actor::App(app("bart-ci")), true),
            Some(Refusal::OwnerRevoked {
                subject: "bound".into(),
                key: "state".into(),
            }),
        ),
        (
            "a revoked owner, everyone else too",
            writing("bound", "state", Actor::Operator, true),
            Some(Refusal::OwnerRevoked {
                subject: "bound".into(),
                key: "state".into(),
            }),
        ),
    ];
    for (name, request, refused) in rows {
        let got = prepare(&state, &declared, &request);
        match refused {
            None => assert!(got.is_ok(), "{name}: {got:?}"),
            Some(refusal) => assert_eq!(got, Err(refusal), "{name}"),
        }
    }
}

/// The permission is judged after the subject exists and before the edge's
/// object is looked for.
#[test]
fn a_missing_subject_is_told_before_an_owned_key_and_that_before_a_missing_edge() {
    let (state, declared) = bound_world();
    let missing_subject = writing("nobody", "state", Actor::Operator, false);
    assert_eq!(
        prepare(&state, &declared, &missing_subject),
        Err(Refusal::UnknownSubject("nobody".into()))
    );
    let both = CaptureRequest {
        edge: Some((EdgeShape::About, "ghost".into())),
        ..writing("bound", "state", Actor::Operator, false)
    };
    assert_eq!(
        prepare(&state, &declared, &both),
        Err(Refusal::OwnedKeyWritten {
            subject: "bound".into(),
            key: "state".into(),
        })
    );
}

/// A chart of bots: `homer` heads it, `marge` reports to `boss`, `bart` to `mid`,
/// and `lisa` reports to `homer` too.
fn chart_world() -> (State, Vec<DeclaredType>) {
    // The chart walk asks the process's kind set whether a manager is a handle.
    crate::memory::kinds::load_shipped();
    let mut events = Vec::new();
    for (name, manager) in [
        ("bot:homer", None),
        ("bot:marge", Some("bot:homer")),
        ("bot:bart", Some("bot:marge")),
        ("bot:lisa", Some("bot:homer")),
    ] {
        events.push(ClaimEvent::ObjectCreated {
            object: object(name),
            body: body("bot", name),
        });
        events.push(ClaimEvent::AddressBound {
            object: object(name),
            address: name.to_string(),
        });
        if let Some(manager) = manager {
            events.push(created(
                &format!("r-{name}"),
                claim("f1", name, "reports", 1),
            ));
            events.push(write(&format!("r-{name}"), name, "reports_to", 1, manager));
        }
    }
    (replay(&events), Vec::new())
}

fn by_bot(subject: &str, caller: &str, key: &str, value: &str) -> CaptureRequest {
    CaptureRequest {
        door: Door {
            actor: Actor::Bot(crate::memory::EntityId(caller.to_string())),
            owned_keys: BTreeSet::new(),
            owner_revoked: false,
            measures: Measures::default(),
            runs: BTreeMap::new(),
            run: None,
            aged_before: None,
        },
        fields: BTreeMap::from([(key.into(), value.into())]),
        ..request(subject)
    }
}

/// **A guarded key is written only by whom its declaration licenses**, judged
/// over the chart the state holds.
#[test]
fn a_guarded_key_is_written_only_by_whom_its_declaration_licenses() {
    let (state, declared) = chart_world();
    // A bot above the thing moves it.
    assert!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:homer", "reports_to", "bot:lisa")
        )
        .is_ok()
    );
    // A bot beside it does not.
    assert!(matches!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:lisa", "reports_to", "bot:lisa")
        ),
        Err(Refusal::Unlicensed(
            crate::memory::MemoryError::KeyNotYours { .. }
        ))
    ));
    // Nobody puts a thing under one of its own reports.
    assert!(matches!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:homer", "bot:homer", "reports_to", "bot:bart")
        ),
        Err(Refusal::Unlicensed(
            crate::memory::MemoryError::ChartCycle { .. }
        ))
    ));
    // A resend of the value the thing holds is not a change.
    assert!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:lisa", "reports_to", "bot:marge")
        )
        .is_ok()
    );
    // A different identity's key: not the bot's own.
    assert!(matches!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:bart", "rule_seats", "3")
        ),
        Err(Refusal::Unlicensed(
            crate::memory::MemoryError::KeyNotYours { .. }
        ))
    ));
    assert!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:marge", "rule_seats", "3")
        )
        .is_ok()
    );
    // An app is judged by its grant before the state sees it, and a key nobody
    // guards is open.
    assert!(
        prepare(
            &state,
            &declared,
            &by_bot("bot:bart", "bot:lisa", "note", "x")
        )
        .is_ok()
    );
}

/// **The keys jojobot writes itself are refused on the verb**, whoever asks, and
/// are the engine's own to write.
#[test]
fn a_key_jojobot_writes_itself_is_refused_on_the_verb_and_not_on_the_engine() {
    let (state, declared) = world();
    for key in [
        "due_on",
        " due_on ",
        "defragged_at",
        "review_mark",
        "settings_record",
        "engine_record",
        "release_notes_told",
        "archive_reason",
    ] {
        let sent = CaptureRequest {
            fields: BTreeMap::from([(key.to_string(), "v".to_string())]),
            ..request("alpha")
        };
        assert_eq!(
            prepare(&state, &declared, &sent),
            Err(Refusal::ServerWritten {
                key: key.trim().to_string()
            }),
            "{key}"
        );
        let engine = CaptureRequest {
            route: Route::Engine,
            ..sent
        };
        assert!(prepare(&state, &declared, &engine).is_ok(), "{key}");
    }
}

/// **A number is one whole number, and a member keeps the one it holds.** The
/// numbering decision of the store, asked over the state.
#[test]
fn a_number_is_a_whole_number_and_a_member_keeps_the_one_it_holds() {
    let (state, declared) = numbered_world();
    for bad in ["two", "0", "-1", "2.5", "04", ""] {
        assert!(
            matches!(
                prepare(&state, &declared, &numbering("m3", bad)),
                Err(Refusal::Numbering(
                    crate::memory::MemoryError::NotANumber { .. }
                ))
            ),
            "{bad:?}"
        );
    }
    // m1 holds 1; giving it 3 would free 1.
    assert!(matches!(
        prepare(&state, &declared, &numbering("m1", "3")),
        Err(Refusal::Numbering(
            crate::memory::MemoryError::NumberFreed { number: 1, .. }
        ))
    ));
}

/// **A collection is founded only over members that already keep its promise.**
#[test]
fn a_collection_is_founded_only_over_members_that_keep_its_promise() {
    let member = |id: &str| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: ObjectBody {
                    parent: Some(object("grp")),
                    ..body("thing", id)
                },
            },
            ClaimEvent::AddressBound {
                object: object(id),
                address: id.to_string(),
            },
            created(&format!("c-{id}"), claim("f1", id, "c", 1)),
            write(&format!("c-{id}"), id, "slot", 1, "7"),
        ]
    };
    let events = [thing("grp"), member("a1"), member("a2")].concat();
    let state = replay(&events);
    let founding = CaptureRequest {
        fields: BTreeMap::from([(NUMBERS_BY.into(), "slot".into())]),
        sets: BTreeSet::from([NUMBERS_BY.to_string()]),
        ..request("grp")
    };
    assert!(matches!(
        prepare(&state, &[], &founding),
        Err(Refusal::Numbering(
            crate::memory::MemoryError::MembersBreakNumbering { .. }
        ))
    ));
}

/// **A field value names a thing only where the link decision says it does.**
/// A handle under any key must exist; loose prose under a reference key names
/// nothing; a lone name under a reference key is refused with the handle to
/// write, unless it is the badge of a thing the state holds.
#[test]
fn a_field_value_names_a_thing_where_the_link_decision_says() {
    // A handle is a handle only against the loaded set of kinds.
    crate::memory::kinds::load_shipped();
    let events = [thing("alpha"), thing("person:gamma"), thing("badge1")].concat();
    let state = replay(&events);
    let declared = vec![DeclaredType::new(
        "thing",
        vec![Field::new("ref-key", ValueType::Reference)],
    )];
    let writing = |key: &str, value: &str| CaptureRequest {
        fields: BTreeMap::from([(key.to_string(), value.to_string())]),
        ..request("alpha")
    };
    assert_eq!(
        prepare(&state, &declared, &writing("free-key", "person:moe")),
        Err(Refusal::UnknownFieldTarget("person:moe".into())),
        "a handle under a key no type declares must exist"
    );
    assert!(
        prepare(&state, &declared, &writing("ref-key", "some loose prose")).is_ok(),
        "prose under a reference key names nothing"
    );
    assert!(matches!(
        prepare(&state, &declared, &writing("ref-key", "ghost")),
        Err(Refusal::Malformed(words)) if words.contains("bare name")
    ));
    let linked = prepare(&state, &declared, &writing("free-key", "person:gamma"))
        .expect("a held handle is admitted");
    assert!(linked.iter().any(|event| matches!(
        event,
        ClaimEvent::FieldWritten { targets, .. } if targets == &vec![object("person:gamma")]
    )));
    let badge = prepare(&state, &declared, &writing("ref-key", "badge1"))
        .expect("a badge a thing wears is admitted");
    assert!(badge.iter().any(|event| matches!(
        event,
        ClaimEvent::FieldWritten { targets, .. } if targets == &vec![object("badge1")]
    )));
}

/// **The claim's own inputs are judged as the store judges them**, before the
/// write is read for its classes and before any object is looked up.
#[test]
fn the_claims_own_inputs_are_judged_before_any_object_is_looked_up() {
    crate::memory::kinds::load_shipped();
    let (state, declared) = world();
    let draft = |change: &dyn Fn(&mut Draft)| {
        let mut base = request("nobody");
        change(&mut base.draft);
        base
    };
    let invalid = |request: CaptureRequest| match prepare(&state, &declared, &request) {
        Err(Refusal::Invalid(refused)) => refused,
        other => panic!("expected an invalid input, got {other:?}"),
    };
    assert!(matches!(
        invalid(draft(&|d| d.content = "  ".into())),
        MemoryError::InvalidFact(_)
    ));
    assert!(matches!(
        invalid(draft(&|d| d.content = "one\ntwo".into())),
        MemoryError::InvalidFact(_)
    ));
    assert!(matches!(
        invalid(draft(&|d| d.happened_through = Some(day(5)))),
        MemoryError::InvalidFact(_)
    ));
    assert!(matches!(
        invalid(draft(&|d| d.receipt = Some(Receipt::Check(" ".into())))),
        MemoryError::InvalidFact(_)
    ));
    assert_eq!(
        invalid(draft(&|d| {
            d.standing = Standing::Open;
            d.receipt = Some(Receipt::Check("ran it".into()));
        })),
        MemoryError::ReceiptWithoutSettling
    );
    assert_eq!(
        invalid(draft(&|d| d.provenance = Provenance::Observation)),
        MemoryError::UnsourcedObservation
    );
    assert!(matches!(
        invalid(CaptureRequest {
            fields: BTreeMap::from([("k".into(), "@#x".into())]),
            ..request("nobody")
        }),
        MemoryError::InvalidFact(_)
    ));
    assert!(matches!(
        invalid(request("session:alpha")),
        MemoryError::InvalidSubject(_)
    ));
    // A settled claim with a receipt that says something is admitted.
    let settled = CaptureRequest {
        draft: Draft {
            receipt: Some(Receipt::Check(" ran it ".into())),
            ..request("alpha").draft
        },
        ..request("alpha")
    };
    let events = prepare(&state, &declared, &settled).expect("admitted");
    assert!(matches!(
        &events[0],
        ClaimEvent::RecordCreated { claim, .. } if claim.receipt == Some(Receipt::Check("ran it".into()))
    ));
}

/// **An edge fits the kind its shape points at**, judged on the object the
/// spelling resolves to.
#[test]
fn an_edge_points_at_the_kind_its_shape_names() {
    crate::memory::kinds::load_shipped();
    let events = [thing("alpha"), thing("person:gamma")].concat();
    let state = replay(&events);
    let located = |object: &str| CaptureRequest {
        edge: Some((EdgeShape::Location, object.into())),
        ..request("alpha")
    };
    assert!(matches!(
        prepare(&state, &[], &located("person:gamma")),
        Err(Refusal::Invalid(MemoryError::InvalidEdge(_)))
    ));
}

/// A bot `alpha` with `seats` seats and one starred rule, and a thing `beta`.
fn seated_world(seats: &str) -> State {
    let events = [
        vec![
            ClaimEvent::ObjectCreated {
                object: object("bot:alpha"),
                body: body("bot", "Alpha"),
            },
            ClaimEvent::AddressBound {
                object: object("bot:alpha"),
                address: "bot:alpha".to_string(),
            },
            created("r0", claim("f1", "bot:alpha", "settings", 1)),
            write("r0", "bot:alpha", crate::memory::RULE_SEATS, 1, seats),
            created("r1", claim("f2", "bot:alpha", "a rule", 1)),
            write("r1", "bot:alpha", "starred", 1, "true"),
        ],
        thing("beta"),
    ]
    .concat();
    replay(&events)
}

fn starring(subject: &str) -> CaptureRequest {
    CaptureRequest {
        fields: BTreeMap::from([("starred".into(), "true".into())]),
        ..request(subject)
    }
}

/// **A star into full seats is refused, naming the rules that hold them.**
#[test]
fn a_star_into_full_seats_is_refused() {
    crate::memory::kinds::load_shipped();
    let full = seated_world("1");
    match prepare(&full, &[], &starring("bot:alpha")) {
        Err(Refusal::SeatsFull {
            seats,
            starred,
            seated,
            ..
        }) => {
            assert_eq!((seats, starred), (1, 1));
            assert_eq!(seated.len(), 1);
            assert_eq!(seated[0].label, "f2");
        }
        other => panic!("expected full seats, got {other:?}"),
    }
    let room = seated_world("2");
    assert!(prepare(&room, &[], &starring("bot:alpha")).is_ok());
    // A thought takes no seat, and a thing that is no bot has none to fill.
    let thought = CaptureRequest {
        edge: Some((EdgeShape::Connection, "bot:alpha".into())),
        ..starring("bot:alpha")
    };
    assert!(prepare(&full, &[], &thought).is_ok());
    assert!(prepare(&full, &[], &starring("beta")).is_ok());
}

fn measured(change: impl FnOnce(&mut Measures)) -> Door {
    let mut door = request("alpha").door;
    change(&mut door.measures);
    door
}

/// **A boot over its ceiling that the write made bigger is refused**, and a
/// boot already over stays repairable by every write that does not grow it.
#[test]
fn a_write_that_grows_a_boot_past_its_ceiling_is_refused() {
    crate::memory::kinds::load_shipped();
    let state = seated_world("3");
    let budget = crate::text::BOOT_ANSWER.budget;
    let floor = |before: usize, after: usize| {
        Some(BootFloor {
            after,
            before,
            parts: vec![("charter".into(), after)],
            stamp_margin: 7,
        })
    };
    let asking = |floor: Option<BootFloor>, request: CaptureRequest| CaptureRequest {
        door: measured(|measures| measures.boot_floor = floor),
        ..request
    };
    match prepare(
        &state,
        &[],
        &asking(floor(budget, budget + 1), starring("bot:alpha")),
    ) {
        Err(Refusal::Invalid(MemoryError::BootTooHeavy {
            floor,
            budget: held,
            stamp_margin,
            ..
        })) => assert_eq!((floor, held, stamp_margin), (budget + 1, budget, 7)),
        other => panic!("expected a heavy boot, got {other:?}"),
    }
    // Not grown, not over, not measured, or a write that grows no floor.
    for (name, request) in [
        (
            "already over",
            asking(floor(budget + 1, budget + 1), starring("bot:alpha")),
        ),
        ("within", asking(floor(0, budget), starring("bot:alpha"))),
        ("unmeasured", asking(None, starring("bot:alpha"))),
        (
            "no star and no seats",
            asking(
                floor(0, budget + 9),
                CaptureRequest {
                    fields: BTreeMap::from([("note".into(), "x".into())]),
                    ..request("bot:alpha")
                },
            ),
        ),
        ("not a bot", asking(floor(0, budget + 9), starring("beta"))),
    ] {
        assert!(prepare(&state, &[], &request).is_ok(), "{name}");
    }
    let seats = |value: &str| CaptureRequest {
        fields: BTreeMap::from([(crate::memory::RULE_SEATS.into(), value.into())]),
        ..request("bot:alpha")
    };
    assert!(matches!(
        prepare(&state, &[], &asking(floor(0, budget + 9), seats("4"))),
        Err(Refusal::Invalid(MemoryError::BootTooHeavy {
            stamp_margin: 0,
            ..
        }))
    ));
    assert!(matches!(
        prepare(&state, &[], &seats("many")),
        Err(Refusal::Invalid(MemoryError::InvalidFact(_)))
    ));
}

/// **A claim no answer could carry is refused**, and the thing's row is named
/// when the row is the cause.
#[test]
fn a_claim_too_big_for_one_answer_is_refused() {
    let (state, declared) = world();
    let sized = |size: usize, row: Option<(usize, usize)>| CaptureRequest {
        door: measured(|measures| {
            measures.claim = Some(ClaimSize {
                size,
                room: 100,
                row,
            })
        }),
        ..request("alpha")
    };
    assert!(prepare(&state, &declared, &sized(100, None)).is_ok());
    assert_eq!(
        prepare(&state, &declared, &sized(101, None)),
        Err(Refusal::Invalid(MemoryError::ClaimTooLarge {
            subject: "alpha".into(),
            size: 101,
            room: 100
        }))
    );
    assert_eq!(
        prepare(&state, &declared, &sized(101, Some((150, 100)))),
        Err(Refusal::Invalid(MemoryError::ThingTooLarge {
            subject: "alpha".into(),
            size: 150,
            room: 100
        }))
    );
}

/// **The three limits are told in the order the verb asks them.**
#[test]
fn the_limits_are_told_in_order() {
    crate::memory::kinds::load_shipped();
    let state = seated_world("1");
    let budget = crate::text::BOOT_ANSWER.budget;
    let both = |floor: bool, claim: bool| CaptureRequest {
        door: measured(|measures| {
            measures.boot_floor = floor.then(|| BootFloor {
                after: budget + 1,
                before: 0,
                parts: Vec::new(),
                stamp_margin: 0,
            });
            measures.claim = claim.then_some(ClaimSize {
                size: 2,
                room: 1,
                row: None,
            });
        }),
        ..starring("bot:alpha")
    };
    assert!(matches!(
        prepare(&state, &[], &both(true, true)),
        Err(Refusal::Invalid(MemoryError::BootTooHeavy { .. }))
    ));
    assert!(matches!(
        prepare(&state, &[], &both(false, true)),
        Err(Refusal::SeatsFull { .. })
    ));
    let roomy = seated_world("2");
    assert!(matches!(
        prepare(&roomy, &[], &both(false, true)),
        Err(Refusal::Invalid(MemoryError::ClaimTooLarge { .. }))
    ));
}

/// **A merged survivor is the same identity under either spelling**, so it
/// cannot write what only a different identity may write about itself.
#[test]
fn a_merged_survivor_cannot_write_its_own_rule_seats() {
    let events = [
        ["bot:alpha", "bot:beta"]
            .iter()
            .flat_map(|name| {
                vec![
                    ClaimEvent::ObjectCreated {
                        object: object(name),
                        body: body("bot", name),
                    },
                    ClaimEvent::AddressBound {
                        object: object(name),
                        address: name.to_string(),
                    },
                ]
            })
            .collect::<Vec<_>>(),
        vec![ClaimEvent::ObjectsMerged {
            folded: object("bot:alpha"),
            survivor: object("bot:beta"),
            records: Vec::new(),
            writes: Vec::new(),
        }],
    ]
    .concat();
    let state = replay(&events);
    assert!(matches!(
        prepare(
            &state,
            &[],
            &by_bot("bot:beta", "bot:beta", "rule_seats", "3")
        ),
        Err(Refusal::Unlicensed(
            crate::memory::MemoryError::KeyNotYours { .. }
        ))
    ));
    // Reached by the folded spelling, it is the same identity too.
    assert!(matches!(
        prepare(
            &state,
            &[],
            &by_bot("bot:alpha", "bot:beta", "rule_seats", "3")
        ),
        Err(Refusal::Unlicensed(
            crate::memory::MemoryError::KeyNotYours { .. }
        ))
    ));
}

/// **A filler known only by its run still counts among the writers of a thread**,
/// so it cannot raise the ceiling of the thread it fills. A run no bot is known
/// for names nobody, and neither does an archived bot.
#[test]
fn a_thread_filler_known_only_by_its_run_cannot_raise_its_ceiling() {
    crate::memory::kinds::load_shipped();
    let filled = |archived_bot: bool| {
        let mut state = State::default();
        let mut bot_body = body("bot", "lisa");
        bot_body.archived = archived_bot;
        for event in [
            ClaimEvent::ObjectCreated {
                object: object("bot:lisa"),
                body: bot_body,
            },
            ClaimEvent::AddressBound {
                object: object("bot:lisa"),
                address: "bot:lisa".to_string(),
            },
            ClaimEvent::ObjectCreated {
                object: object("thread:alpha"),
                body: body("thread", "alpha"),
            },
            ClaimEvent::AddressBound {
                object: object("thread:alpha"),
                address: "thread:alpha".to_string(),
            },
            created("r1", claim("f1", "thread:alpha", "a fill", 1)),
        ] {
            state.apply(&under(None, Some("sa01"), event));
        }
        state
    };
    let raising = || by_bot("thread:alpha", "bot:lisa", "record_capacity", "9");
    let known = |state: &State, runs: &[(&str, &str)]| {
        let mut request = raising();
        request.door.runs = runs
            .iter()
            .map(|(run, bot)| {
                (
                    super::super::super::RunId(run.to_string()),
                    crate::memory::EntityId(bot.to_string()),
                )
            })
            .collect();
        prepare(state, &[], &request)
    };
    assert!(matches!(
        known(&filled(false), &[("sa01", "bot:lisa")]),
        Err(Refusal::Unlicensed(_))
    ));
    assert!(known(&filled(false), &[]).is_ok(), "a run nobody owns");
    assert!(
        known(&filled(true), &[("sa01", "bot:lisa")]).is_ok(),
        "an archived bot writes nobody's thread"
    );
}

/// **A number is issued only by a write made while the member stood under a
/// container that numbered by that key.** A number a claim wrote before the
/// collection existed, or before the member joined it, was never issued.
#[test]
fn a_number_written_before_it_was_numbered_is_not_issued() {
    let member = |id: &str, parent: Option<&str>| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: ObjectBody {
                    parent: parent.map(object),
                    ..body("thing", id)
                },
            },
            ClaimEvent::AddressBound {
                object: object(id),
                address: id.to_string(),
            },
        ]
    };
    let taken_back = |record_id: &str, subject: &str, value: &str| {
        vec![
            created(record_id, claim(&format!("f{record_id}"), subject, "c", 1)),
            write(record_id, subject, "num", 1, value),
            archived(record_id, &format!("f{record_id}"), subject),
        ]
    };
    let events = [
        thing("coll"),
        // before the collection numbers anything
        member("m1", Some("coll")),
        taken_back("n1", "m1", "5"),
        // before the member joins
        member("m2", None),
        taken_back("n2", "m2", "6"),
        vec![
            created("c1", claim("f1", "coll", "c", 1)),
            write("c1", "coll", NUMBERS_BY, 1, "num"),
        ],
        vec![ClaimEvent::ObjectRevised {
            object: object("m2"),
            body: ObjectBody {
                parent: Some(object("coll")),
                ..body("thing", "m2")
            },
        }],
        member("m3", Some("coll")),
        // after: a taken-back number is retired for good
        taken_back("n3", "m3", "7"),
    ]
    .concat();
    let state = replay(&events);
    assert!(
        prepare(&state, &[], &numbering("m3", "5")).is_ok(),
        "written before the collection numbered"
    );
    assert!(
        prepare(&state, &[], &numbering("m3", "6")).is_ok(),
        "written before the member joined"
    );
    assert!(
        matches!(
            prepare(&state, &[], &numbering("m3", "7")),
            Err(Refusal::NumberRetired { .. })
        ),
        "written and taken back since"
    );
}

/// A bare object of `kind` that answers to `handle`, with claims that each
/// write one key: `(record, label, key, value)`.
fn held(kind: &str, handle: &str, writes: &[(&str, &str, &str, &str)]) -> Vec<ClaimEvent> {
    let mut events = vec![
        ClaimEvent::ObjectCreated {
            object: object(handle),
            body: body(kind, handle),
        },
        ClaimEvent::AddressBound {
            object: object(handle),
            address: handle.to_string(),
        },
    ];
    for (record, label, key, value) in writes {
        events.push(created(record, claim(label, handle, "c", 1)));
        events.push(write(record, handle, key, 1, value));
    }
    events
}

/// **A write to a role object is judged against the lease the role holds**, on
/// the engine's route: the verb never writes a lease field.
#[test]
fn a_role_is_taken_only_when_its_lease_has_lapsed_or_is_the_claimants() {
    crate::memory::kinds::load_shipped();
    let state = replay(&held(
        "role",
        "role:alpha",
        &[
            ("r1", "f1", "holder", "sa01"),
            ("r2", "f2", "claimed_at", "2026-04-03T10:00:00Z"),
        ],
    ));
    let claim_at = |holder: &str, at: &str| CaptureRequest {
        route: Route::Engine,
        fields: BTreeMap::from([
            ("holder".into(), holder.into()),
            ("claimed_at".into(), at.into()),
        ]),
        ..request("role:alpha")
    };
    assert!(matches!(
        prepare(&state, &[], &claim_at("sb01", "2026-04-03T10:10:00Z")),
        Err(Refusal::Invalid(MemoryError::RoleTaken { .. }))
    ));
    assert!(prepare(&state, &[], &claim_at("sa01", "2026-04-03T10:10:00Z")).is_ok());
    assert!(prepare(&state, &[], &claim_at("sb01", "2026-04-03T12:00:00Z")).is_ok());
}

/// **A thought is capped by its container's own body cap**, and another bot's
/// claim on the container is no thought of its.
#[test]
fn a_thought_over_its_containers_body_cap_is_refused() {
    crate::memory::kinds::load_shipped();
    let state = replay(
        &[
            held(
                "bot",
                "bot:alpha",
                &[
                    ("r1", "f1", "thought_capacity", "3"),
                    ("r2", "f2", "thought_body_cap", "10"),
                ],
            ),
            held("bot", "bot:beta", &[]),
        ]
        .concat(),
    );
    let thought = |content: &str, actor: Actor| {
        let mut request = CaptureRequest {
            edge: Some((EdgeShape::Connection, "bot:alpha".into())),
            ..request("bot:alpha")
        };
        request.draft.content = content.to_string();
        request.door.actor = actor;
        request
    };
    let own = || Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    assert!(prepare(&state, &[], &thought("0123456789", own())).is_ok());
    assert!(matches!(
        prepare(&state, &[], &thought("01234567890", own())),
        Err(Refusal::Invalid(MemoryError::ThoughtTooLong {
            len: 11,
            cap: 10,
            ..
        }))
    ));
    assert!(matches!(
        prepare(&state, &[], &thought("01234567890", Actor::Operator)),
        Err(Refusal::Invalid(MemoryError::ThoughtTooLong { .. }))
    ));
    let another = Actor::Bot(crate::memory::EntityId("bot:beta".into()));
    assert!(prepare(&state, &[], &thought("01234567890", another)).is_ok());
    // A run that belongs to another bot writes another's claim too.
    let mut by_run = thought("01234567890", Actor::Operator);
    by_run.door.run = Some(super::super::super::RunId("sb01".into()));
    by_run.door.runs = BTreeMap::from([(
        super::super::super::RunId("sb01".into()),
        crate::memory::EntityId("bot:beta".into()),
    )]);
    assert!(prepare(&state, &[], &by_run).is_ok());
}

/// **A thing holds no more live records than its ceiling**, a write that adds
/// none is exempt, and the reserve is one record, once.
#[test]
fn a_thing_holds_no_more_live_records_than_its_ceiling() {
    let writes = |records: &[(&'static str, &'static str)]| {
        let mut all = vec![("r0", "f1", "record_capacity", "2")];
        all.extend(records.iter().map(|(id, label)| (*id, *label, "note", "x")));
        replay(&held("thing", "beta", &all))
    };
    // A review mark and a record of jojobot's own act hold no slot.
    let marked = replay(&held(
        "thing",
        "beta",
        &[
            ("r0", "f1", "record_capacity", "2"),
            ("r1", "f2", "note", "x"),
            ("r8", "f8", "review_mark", "x"),
            ("r9", "f9", "engine_record", "x"),
        ],
    ));
    assert!(matches!(
        prepare(&marked, &[], &request("beta")),
        Err(Refusal::OverCapacity { records, .. }) if records.len() == 2
    ));
    let full = writes(&[("r1", "f2")]);
    let over = writes(&[("r1", "f2"), ("r2", "f3")]);
    let adding = |borrow: bool| CaptureRequest {
        borrow,
        ..request("beta")
    };
    assert!(prepare(&writes(&[]), &[], &adding(false)).is_ok());
    match prepare(&full, &[], &adding(false)) {
        Err(Refusal::OverCapacity {
            limit,
            adding,
            from_setting,
            records,
            ..
        }) => {
            assert_eq!((limit, adding, from_setting), (2, 1, true));
            assert_eq!(records.len(), 2);
        }
        other => panic!("expected a full thing, got {other:?}"),
    }
    // The reserve: exactly full, one record, once.
    assert!(prepare(&full, &[], &adding(true)).is_ok());
    assert!(matches!(
        prepare(&over, &[], &adding(true)),
        Err(Refusal::OverCapacity { .. })
    ));
    // A record that takes no slot is exempt, even on a thing over its ceiling.
    let marking = CaptureRequest {
        route: Route::Engine,
        fields: BTreeMap::from([("review_mark".into(), "x".into())]),
        ..request("beta")
    };
    assert!(prepare(&over, &[], &marking).is_ok());
}

/// A bot `alpha` with room for two thoughts, and `kept` thoughts of its own
/// written by `by`, each touched at `touched`.
fn roomy(kept: usize, by: Actor, touched: jiff::Timestamp) -> State {
    let mut state = State::default();
    let at = |actor: Actor, payload: ClaimEvent| {
        let mut event = under(Some(actor), None, payload);
        event.written_at = Some(touched);
        event
    };
    for payload in held("bot", "bot:alpha", &[("r0", "f1", "thought_capacity", "2")]) {
        state.apply(&at(Actor::Operator, payload));
    }
    for n in 0..kept {
        let id = format!("t{n}");
        let thought = ClaimBody {
            edge: Some((EdgeShape::Connection.to_string(), object("bot:alpha"))),
            ..claim(&format!("f{}", n + 2), "bot:alpha", "a thought", 1)
        };
        for payload in [
            created(&id, thought),
            write(&id, "bot:alpha", "note", 1, "x"),
        ] {
            state.apply(&at(by.clone(), payload));
        }
    }
    state
}

fn thinking() -> CaptureRequest {
    let mut thought = CaptureRequest {
        edge: Some((EdgeShape::Connection, "bot:alpha".into())),
        ..request("bot:alpha")
    };
    thought.door.actor = Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    thought
}

/// **A thought joining a full room gives up one the write names, spends the
/// reserve once, or is refused.** Thoughts gone quiet and claims another bot
/// wrote are not in the room.
#[test]
fn a_thought_joining_a_full_room_gives_up_one_or_is_refused() {
    crate::memory::kinds::load_shipped();
    let own = || Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    let now: jiff::Timestamp = "2026-04-03T10:00:00Z".parse().unwrap();
    let full = roomy(2, own(), now);
    assert!(prepare(&roomy(1, own(), now), &[], &thinking()).is_ok());
    match prepare(&full, &[], &thinking()) {
        Err(Refusal::RoomFull {
            live,
            capacity,
            aged_out,
            room,
            ..
        }) => assert_eq!((live, capacity, aged_out, room.len()), (2, 2, 0, 2)),
        other => panic!("expected a full room, got {other:?}"),
    }
    // The reserve: once, and not over a room already over.
    let borrowing = CaptureRequest {
        borrow: true,
        ..thinking()
    };
    assert!(prepare(&full, &[], &borrowing).is_ok());
    assert!(matches!(
        prepare(&roomy(3, own(), now), &[], &borrowing),
        Err(Refusal::RoomFull { .. })
    ));
    // A drop archives the thought in the same act, ahead of the new claim.
    let dropping = |address: &str| CaptureRequest {
        drop: Some((address.into(), "no longer earns it".into())),
        ..thinking()
    };
    let events = prepare(&full, &[], &dropping("bot:alpha#f2")).expect("admitted");
    assert!(matches!(
        &events[0],
        ClaimEvent::RecordRevised { record, claim }
            if record == &super::super::tests::record("t0")
                && claim.status == FactStatus::Archived
                && claim.details.as_deref() == Some("no longer earns it")
    ));
    assert!(matches!(&events[1], ClaimEvent::RecordCreated { .. }));
    // Only a live thought of this room can be dropped.
    assert!(matches!(
        prepare(&full, &[], &dropping("bot:alpha#f1")),
        Err(Refusal::RoomFull { .. })
    ));
    assert!(matches!(
        prepare(&full, &[], &dropping("bot:alpha#f9")),
        Err(Refusal::RoomFull { .. })
    ));
    // Gone quiet: still there, outside the count.
    let quiet = CaptureRequest {
        door: Door {
            aged_before: Some("2026-04-03T11:00:00Z".parse().unwrap()),
            ..thinking().door
        },
        ..thinking()
    };
    assert!(prepare(&full, &[], &quiet).is_ok());
    // Another bot's claims on the bot take no seat.
    let other = Actor::Bot(crate::memory::EntityId("bot:beta".into()));
    let mut mixed = roomy(2, other, now);
    for event in held("bot", "bot:beta", &[]) {
        apply(&mut mixed, &event);
    }
    assert!(prepare(&mixed, &[], &thinking()).is_ok());
}
