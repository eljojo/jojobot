use std::collections::{BTreeMap, BTreeSet};

use super::super::tests::{
    apply, archived, body, claim, created, day, object, record, replay, write,
};
use super::super::{ClaimEvent, ObjectBody, State};
use super::{CaptureRequest, Door, Draft, Refusal, Route, prepare};
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
