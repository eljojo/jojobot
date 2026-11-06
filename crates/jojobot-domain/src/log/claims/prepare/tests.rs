use std::collections::{BTreeMap, BTreeSet};

use super::super::tests::{
    apply, archived, body, claim, created, day, object, record, replay, under, write,
};
use super::super::{ClaimBody, ClaimEvent, ObjectBody, State};
use super::{
    BootFloor, CaptureRequest, ClaimSize, Declined, Door, Draft, Measures, Refusal, Route, prepare,
};
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
    // An edge check on a handle asks the process's kind set, which a case run
    // alone has not loaded.
    crate::memory::kinds::load_shipped();
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

/// What the door measures for a claim that fits: the size of one answer's room.
fn fitting() -> Measures {
    Measures {
        claim: Some(ClaimSize {
            size: 1,
            room: 100,
            row: None,
        }),
        ..Measures::default()
    }
}

fn request(subject: &str) -> CaptureRequest {
    CaptureRequest {
        record: record("new"),
        route: Route::Verb,
        door: Door {
            actor: Actor::Operator,
            owned_keys: BTreeSet::new(),
            owner_revoked: false,
            measures: fitting(),
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
    // The rows name handles (`thing:omega`, `person:moe`), and a handle is a
    // handle only against the kinds this process loaded, so the case loads them
    // itself rather than rely on another case having run first.
    crate::memory::kinds::load_shipped();
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
                edge: Some((EdgeShape::About, "thing:omega".into())),
                ..request("nobody")
            },
            refused: Refusal::UnknownSubject("nobody".into(), Vec::new()),
        },
        Row {
            name: "a missing edge object before a missing ref",
            request: CaptureRequest {
                edge: Some((EdgeShape::About, "thing:omega".into())),
                refs: vec!["phantom".into()],
                ..request("alpha")
            },
            refused: Refusal::UnknownEdgeObject("thing:omega".into(), Vec::new()),
        },
        Row {
            name: "a missing ref before a missing reference field object",
            request: CaptureRequest {
                refs: vec!["phantom".into()],
                fields: BTreeMap::from([("ref-key".into(), "person:moe".into())]),
                ..request("alpha")
            },
            refused: Refusal::UnknownRef("phantom".into(), Vec::new()),
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
            next: 3,
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
        next: 5,
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
            next: 6,
        })
    );
}

/// **An edge the shape cannot point at is told before the object is looked
/// up**: both stores judge the spelling first, so a missing object behind a
/// wrong shape is an invalid edge, not an absent one.
#[test]
fn an_edge_the_shape_cannot_hold_is_told_before_its_object_is_looked_up() {
    let (state, declared) = world();
    let located = |object: &str| CaptureRequest {
        edge: Some((EdgeShape::Location, object.into())),
        ..request("alpha")
    };
    assert!(matches!(
        prepare(&state, &declared, &located("thing:omega")),
        Err(Refusal::Invalid(MemoryError::InvalidEdge(_)))
    ));
    assert!(matches!(
        prepare(&state, &declared, &located("place:omega")),
        Err(Refusal::UnknownEdgeObject(..))
    ));
}

/// A `promise` named `promise:alpha`, holding `promised_by` when it is given
/// one; the kind requires that key.
fn promise_world(promised_by: Option<&str>) -> (State, Vec<DeclaredType>) {
    crate::memory::kinds::load_shipped();
    let mut events = vec![
        ClaimEvent::ObjectCreated {
            object: object("promise:alpha"),
            body: body("promise", "promise:alpha"),
        },
        ClaimEvent::AddressBound {
            object: object("promise:alpha"),
            address: "promise:alpha".into(),
        },
    ];
    if let Some(day) = promised_by {
        events.push(created(
            "p1",
            claim("f1", "promise:alpha", "has to go back", 1),
        ));
        events.push(write("p1", "promise:alpha", "promised_by", 1, day));
    }
    (replay(&events), Vec::new())
}

fn setting(subject: &str, key: &str, value: &str) -> CaptureRequest {
    CaptureRequest {
        fields: BTreeMap::from([(key.into(), value.into())]),
        sets: BTreeSet::from([key.to_string()]),
        ..request(subject)
    }
}

/// **A capture may not drop a thing below its kind's floor, and may not put a
/// key a value it does not hold.** The floor guards a thing that meets it; a
/// thing that fits nothing has nothing to protect, and a key beyond the kind is
/// welcome.
#[test]
fn a_capture_is_held_to_its_subjects_kind() {
    let (held, declared) = promise_world(Some("2026-07-01"));
    assert!(matches!(
        prepare(&held, &declared, &setting("promise:alpha", "promised_by", "")),
        Err(Refusal::BreaksKind(MemoryError::BreaksFit { name, keys }))
            if name == "promise" && keys == ["promised_by"]
    ));
    assert!(matches!(
        prepare(&held, &declared, &setting("promise:alpha", "promised_by", "someday")),
        Err(Refusal::BreaksKind(MemoryError::BreaksType { key, .. })) if key == "promised_by"
    ));
    assert!(
        prepare(
            &held,
            &declared,
            &setting("promise:alpha", "promised_by", "2026-08-01")
        )
        .is_ok(),
        "a day replaces a day"
    );
    assert!(
        prepare(&held, &declared, &setting("promise:alpha", "note", "")).is_ok(),
        "a blank in a key the kind does not require goes freely"
    );
    assert!(
        prepare(&held, &declared, &setting("promise:alpha", "mood", "quiet")).is_ok(),
        "a key beyond the kind is welcome"
    );
    // A promise that holds no day fits nothing: it can be repaired, but a bad
    // value is refused wherever the key is set.
    let (bare, declared) = promise_world(None);
    assert!(
        prepare(&bare, &declared, &setting("promise:alpha", "note", "")).is_ok(),
        "nothing to protect"
    );
    assert!(matches!(
        prepare(
            &bare,
            &declared,
            &setting("promise:alpha", "promised_by", "someday")
        ),
        Err(Refusal::BreaksKind(MemoryError::BreaksType { .. }))
    ));
}

/// A numbered alias is judged by its target's kind, not its written prefix.
#[test]
fn a_capture_is_held_to_its_subjects_kind_through_a_numbered_reference() {
    crate::memory::kinds::load_shipped();
    let declared = vec![DeclaredType::new(
        "bot",
        crate::memory::kinds::keys_of("bot"),
    )];
    let mut events = vec![
        ClaimEvent::ObjectCreated {
            object: object("bot:lisa"),
            body: body("bot", "bot:lisa"),
        },
        ClaimEvent::AddressBound {
            object: object("bot:lisa"),
            address: "bot:lisa".into(),
        },
    ];
    for (kind, handle, alias) in [
        ("bot", "bot:marge", "work:alpha-queue/1"),
        ("person", "person:milhouse", "work:alpha-queue/2"),
    ] {
        events.extend([
            ClaimEvent::ObjectCreated {
                object: object(handle),
                body: ObjectBody {
                    aliases: vec![alias.into()],
                    ..body(kind, handle)
                },
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: handle.into(),
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: alias.into(),
            },
        ]);
    }
    let state = replay(&events);
    let admitted = prepare(
        &state,
        &declared,
        &setting("bot:lisa", "reports_to", "work:alpha-queue/1"),
    )
    .expect("the numbered alias names a bot despite its work prefix");
    assert!(admitted.iter().any(|event| matches!(
        event,
        ClaimEvent::FieldWritten { targets, .. } if targets == &vec![object("bot:marge")]
    )));
    assert!(matches!(
        prepare(
            &state,
            &declared,
            &setting("bot:lisa", "reports_to", "work:alpha-queue/2"),
        ),
        Err(Refusal::BreaksKind(MemoryError::BreaksType { name, key, value, .. }))
            if name == "bot" && key == "reports_to" && value == "work:alpha-queue/2"
    ));
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
        Err(Refusal::UnknownSubject("role:alpha".into(), Vec::new())),
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
    // An edge check on a handle asks the process's kind set, which a case run
    // alone has not loaded.
    crate::memory::kinds::load_shipped();
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
            measures: fitting(),
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
        Err(Refusal::UnknownSubject("nobody".into(), Vec::new()))
    );
    let both = CaptureRequest {
        edge: Some((EdgeShape::About, "thing:omega".into())),
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
            measures: fitting(),
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

/// **Taking the key off a collection is judged by the collection as it numbers
/// now.** Every number its members hold would be freed, live and taken back
/// alike, and the lowest handle is named, even when that member is taken back
/// and holds the higher number; a collection whose members hold no number, and
/// one that has no member, may stop numbering.
#[test]
fn a_collection_stops_numbering_only_when_no_member_holds_a_number() {
    let member = |id: &str, parent: &str, archived: bool| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(id),
                body: ObjectBody {
                    parent: Some(object(parent)),
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
        thing("held"),
        holds("c1", "held", NUMBERS_BY, "num"),
        member("b1", "held", false),
        holds("n1", "b1", "num", "1"),
        member("a1", "held", true),
        holds("n2", "a1", "num", "2"),
        thing("quiet"),
        holds("c2", "quiet", NUMBERS_BY, "num"),
        member("q1", "quiet", false),
        thing("empty"),
        holds("c3", "empty", NUMBERS_BY, "num"),
    ]
    .concat();
    let state = replay(&events);
    let off = |subject: &str| CaptureRequest {
        fields: BTreeMap::from([(NUMBERS_BY.into(), String::new())]),
        sets: BTreeSet::from([NUMBERS_BY.to_string()]),
        ..request(subject)
    };
    assert!(matches!(
        prepare(&state, &[], &off("held")),
        Err(Refusal::Numbering(
            crate::memory::MemoryError::NumberFreed { number: 2, holder, .. }
        )) if holder.ends_with("a1")
    ));
    for control in ["quiet", "empty"] {
        assert!(
            prepare(&state, &[], &off(control)).is_ok(),
            "{control}: no member holds a number, so the key may come off"
        );
    }
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

/// **A handle in a field value is stored as the id of the thing it names**, so
/// the value is served as the handle the thing wears when it is read: after a
/// rename the value names the new handle.
#[test]
fn a_handle_in_a_field_value_is_stored_as_the_id_and_follows_a_rename() {
    crate::memory::kinds::load_shipped();
    let mut state = replay(&thing("person:homer"));
    for event in [
        ClaimEvent::ObjectCreated {
            object: object("k9"),
            body: body("person", "person:gamma"),
        },
        ClaimEvent::AddressBound {
            object: object("k9"),
            address: "person:gamma".into(),
        },
    ] {
        apply(&mut state, &event);
    }
    let sent = CaptureRequest {
        fields: BTreeMap::from([("owner".to_string(), "person:gamma".to_string())]),
        ..request("person:homer")
    };
    let events = prepare(&state, &[], &sent).expect("a held handle is admitted");
    let stored = events
        .iter()
        .find_map(|event| match event {
            ClaimEvent::FieldWritten { write, .. } => Some(write.clone()),
            _ => None,
        })
        .expect("the field is written");
    assert_eq!(
        stored,
        crate::log::claims::FieldValue::Set(crate::memory::marked_stored_handle("k9"))
    );
    for event in &events {
        apply(&mut state, event);
    }
    let served = |state: &State| {
        state
            .fact(&record("new"))
            .expect("the State holds the fact")
            .fields
            .get("owner")
            .cloned()
    };
    assert_eq!(served(&state).as_deref(), Some("person:gamma"));
    for event in [
        ClaimEvent::AddressRetired {
            object: object("k9"),
            address: "person:gamma".into(),
            day: day(4),
        },
        ClaimEvent::AddressBound {
            object: object("k9"),
            address: "person:delta".into(),
        },
    ] {
        apply(&mut state, &event);
    }
    assert_eq!(served(&state).as_deref(), Some("person:delta"));
}

/// A world with one bot request and one plain thing.
fn requests_world() -> State {
    crate::memory::kinds::load_shipped();
    let mut events = Vec::new();
    for (id, kind, handle) in [
        ("k1", "bot-request", "bot-request:alpha"),
        ("k3", "thing", "thing:gamma"),
    ] {
        events.push(ClaimEvent::ObjectCreated {
            object: object(id),
            body: body(kind, handle),
        });
        events.push(ClaimEvent::AddressBound {
            object: object(id),
            address: handle.into(),
        });
    }
    replay(&events)
}

fn generation_written(events: &[ClaimEvent]) -> Option<(crate::log::RecordId, String)> {
    events.iter().find_map(|event| match event {
        ClaimEvent::FieldWritten {
            record,
            key,
            write: crate::log::claims::FieldValue::Set(value),
            ..
        } if key.0 == "generation" => Some((record.clone(), value.clone())),
        _ => None,
    })
}

/// **A capture that changes the spec of a bot request writes the generation in
/// its own batch**, under the same record, and one that leaves the spec as it is
/// writes none. The caller sends no number.
#[test]
fn a_capture_of_a_request_spec_writes_its_generation_with_it() {
    let mut state = requests_world();
    let spec = |value: &str| CaptureRequest {
        fields: BTreeMap::from([("desired".to_string(), value.to_string())]),
        sets: BTreeSet::from(["desired".to_string()]),
        ..request("bot-request:alpha")
    };
    let first = prepare(&state, &[], &spec("running")).expect("the spec is admitted");
    assert_eq!(
        generation_written(&first),
        Some((record("new"), "1".to_string()))
    );
    for event in &first {
        apply(&mut state, event);
    }
    let resent = CaptureRequest {
        record: record("resent"),
        ..spec("running")
    };
    let again = prepare(&state, &[], &resent).expect("a resend is admitted");
    assert_eq!(generation_written(&again), None, "a resend makes no number");
    let changed = CaptureRequest {
        record: record("changed"),
        ..spec("stopped")
    };
    let next = prepare(&state, &[], &changed).expect("a change is admitted");
    assert_eq!(
        generation_written(&next),
        Some((record("changed"), "2".to_string()))
    );
}

/// A capture on a thing that is no request writes no generation, whatever it sets.
#[test]
fn a_capture_on_a_plain_thing_writes_no_generation() {
    let state = requests_world();
    let sent = CaptureRequest {
        fields: BTreeMap::from([("desired".to_string(), "running".to_string())]),
        sets: BTreeSet::from(["desired".to_string()]),
        ..request("thing:gamma")
    };
    let events = prepare(&state, &[], &sent).expect("the capture is admitted");
    assert_eq!(generation_written(&events), None);
}

#[test]
fn padded_capture_key_keeps_its_targets_and_setting_class() {
    crate::memory::kinds::load_shipped();
    let (mut state, declared) = world();
    for event in thing("person:gamma") {
        apply(&mut state, &event);
    }
    let sent = CaptureRequest {
        fields: BTreeMap::from([(" owner ".into(), "person:gamma".into())]),
        sets: BTreeSet::from(["owner".into()]),
        ..request("alpha")
    };
    let events = prepare(&state, &declared, &sent).expect("a padded key is admitted");
    let (key, targets, class) = events
        .iter()
        .find_map(|event| match event {
            ClaimEvent::FieldWritten {
                key,
                targets,
                class,
                ..
            } => Some((key, targets, class)),
            _ => None,
        })
        .expect("the field is written");
    assert_eq!(targets, &[object("person:gamma")]);
    assert_eq!(key.0, " owner ");
    assert_eq!(*class, crate::memory::WriteClass::Sets);
    for event in &events {
        apply(&mut state, event);
    }
    let held = state.fields_served(&object("alpha"), &declared);
    assert_eq!(held[" owner "], "person:gamma");
    assert!(!held.contains_key("owner"));
    assert_eq!(
        state.links_from(&object("alpha")),
        BTreeSet::from([object("person:gamma")])
    );
}

#[test]
fn padded_capture_keys_keep_separate_streams_and_targets() {
    crate::memory::kinds::load_shipped();
    let (mut state, declared) = world();
    for event in [thing("person:gamma"), thing("person:delta")].concat() {
        apply(&mut state, &event);
    }
    apply(&mut state, &write("r1", "alpha", " owner ", 7, "before"));
    apply(&mut state, &write("r1", "alpha", "owner", 2, "before"));
    let sent = CaptureRequest {
        fields: BTreeMap::from([
            (" owner ".into(), "person:gamma".into()),
            ("owner".into(), "person:delta".into()),
        ]),
        sets: BTreeSet::from(["owner".into()]),
        ..request("alpha")
    };
    let events = prepare(&state, &declared, &sent).expect("both raw keys are admitted");
    let writes: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            ClaimEvent::FieldWritten {
                key,
                ordinal,
                targets,
                ..
            } => Some((key.0.as_str(), *ordinal, targets.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        writes,
        vec![
            (" owner ", 8, vec![object("person:gamma")]),
            ("owner", 3, vec![object("person:delta")]),
        ]
    );
    for event in &events {
        apply(&mut state, event);
    }
    let held = state.fields_served(&object("alpha"), &declared);
    assert_eq!(held[" owner "], "person:gamma");
    assert_eq!(held["owner"], "person:delta");
    assert_eq!(
        state.links_from(&object("alpha")),
        BTreeSet::from([object("person:gamma"), object("person:delta")])
    );
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
    let mut events = thing("alpha");
    for (kind, handle, alias) in [
        ("person", "person:gamma", "work:alpha-queue/1"),
        ("place", "place:delta", "work:alpha-queue/2"),
    ] {
        events.extend([
            ClaimEvent::ObjectCreated {
                object: object(handle),
                body: ObjectBody {
                    aliases: vec![alias.into()],
                    ..body(kind, handle)
                },
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: handle.into(),
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: alias.into(),
            },
        ]);
    }
    let state = replay(&events);
    let located = |object: &str| CaptureRequest {
        edge: Some((EdgeShape::Location, object.into())),
        ..request("alpha")
    };
    for spelling in ["person:gamma", "work:alpha-queue/1"] {
        assert!(
            matches!(
                prepare(&state, &[], &located(spelling)),
                Err(Refusal::Invalid(MemoryError::InvalidEdge(_)))
            ),
            "a location edge cannot resolve to a person through {spelling}"
        );
    }
    for spelling in ["place:delta", "work:alpha-queue/2"] {
        let admitted = prepare(&state, &[], &located(spelling))
            .expect("a location edge to a place is admitted");
        assert!(admitted.iter().any(|event| matches!(
            event,
            ClaimEvent::RecordCreated { claim, .. }
                if claim.edge == Some(("location".into(), object("place:delta")))
        )));
    }
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

/// **A size nobody measured is refused for a thing the State holds**, as a
/// failure of the store the same call can be sent again after. A thing the State
/// does not hold yet has nothing to measure against, so it is not refused here.
#[test]
fn an_unmeasured_claim_on_a_held_thing_is_refused_for_a_retry() {
    let (state, declared) = world();
    let unmeasured = |subject: &str| CaptureRequest {
        door: measured(|measures| measures.claim = None),
        ..request(subject)
    };
    assert!(matches!(
        prepare(&state, &declared, &unmeasured("alpha")),
        Err(Refusal::Invalid(MemoryError::Store(_)))
    ));
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

/// A competing late fault must not hide the role's held lease.
#[test]
fn late_refusals_tell_the_lease_before_the_body_cap() {
    crate::memory::kinds::load_shipped();
    let state = replay(&held(
        "role",
        "role:alpha",
        &[
            ("r1", "f1", "holder", "sa01"),
            ("r2", "f2", "claimed_at", "2026-04-03T10:00:00Z"),
            ("r3", "f3", "thought_body_cap", "10"),
        ],
    ));
    let mut sent = CaptureRequest {
        route: Route::Engine,
        edge: Some((EdgeShape::Connection, "role:alpha".into())),
        fields: BTreeMap::from([
            ("holder".into(), "sb01".into()),
            ("claimed_at".into(), "2026-04-03T10:10:00Z".into()),
        ]),
        ..request("role:alpha")
    };
    sent.draft.content = "01234567890".into();
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::Invalid(MemoryError::RoleTaken { .. }))
    ));
    sent.fields.insert("holder".into(), "sa01".into());
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::Invalid(MemoryError::ThoughtTooLong {
            len: 11,
            cap: 10,
            ..
        }))
    ));
    sent.draft.content = "0123456789".into();
    assert!(prepare(&state, &[], &sent).is_ok());
}

/// The body cap wins even when the thought has no room to land.
#[test]
fn late_refusals_tell_the_body_cap_before_the_room() {
    crate::memory::kinds::load_shipped();
    let now = "2026-04-03T10:00:00Z".parse().unwrap();
    let own = Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    let mut state = roomy(2, own, now);
    apply(
        &mut state,
        &write("r0", "bot:alpha", "thought_body_cap", 1, "10"),
    );
    let mut sent = thinking();
    sent.draft.content = "01234567890".into();
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::Invalid(MemoryError::ThoughtTooLong {
            len: 11,
            cap: 10,
            ..
        }))
    ));
    sent.draft.content = "0123456789".into();
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::RoomFull {
            live: 2,
            capacity: 2,
            ..
        })
    ));
    sent.drop = Some(("bot:alpha#f2".into(), "no longer earns it".into()));
    assert!(prepare(&state, &[], &sent).is_ok());
}

/// Quiet thoughts free room but still count against the record ceiling.
#[test]
fn late_refusals_tell_the_room_before_the_record_ceiling() {
    crate::memory::kinds::load_shipped();
    let now = "2026-04-03T10:00:00Z".parse().unwrap();
    let own = Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    let mut state = roomy(2, own, now);
    apply(
        &mut state,
        &write("r0", "bot:alpha", "record_capacity", 1, "3"),
    );
    let mut sent = thinking();
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::RoomFull {
            live: 2,
            capacity: 2,
            ..
        })
    ));
    sent.door.aged_before = Some("2026-04-03T11:00:00Z".parse().unwrap());
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::OverCapacity { limit: 3, adding: 1, records, .. }) if records.len() == 3
    ));
    sent.borrow = true;
    assert!(prepare(&state, &[], &sent).is_ok());
}

/// A kind fault is told only after the record has a slot.
#[test]
fn late_refusals_tell_the_record_ceiling_before_the_kind() {
    crate::memory::kinds::load_shipped();
    let state = replay(&held(
        "view",
        "view:alpha",
        &[("r0", "f1", "record_capacity", "1")],
    ));
    let mut sent = CaptureRequest {
        fields: BTreeMap::from([("asks".into(), "wrong".into())]),
        ..request("view:alpha")
    };
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::OverCapacity { limit: 1, adding: 1, records, .. }) if records.len() == 1
    ));
    sent.borrow = true;
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::BreaksKind(MemoryError::BreaksType { key, value, .. }))
            if key == "asks" && value == "wrong"
    ));
    sent.fields.insert("asks".into(), "ready".into());
    assert!(prepare(&state, &[], &sent).is_ok());
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
            ("r1", "f10", "note", "x"),
            ("r9", "f2", "note", "x"),
            ("r2", "f9", "note", "x"),
            ("review", "f11", "review_mark", "x"),
            ("engine", "f12", "engine_record", "x"),
        ],
    ));
    match prepare(&marked, &[], &request("beta")) {
        Err(Refusal::OverCapacity { records, .. }) => assert_eq!(
            records
                .iter()
                .map(|held| (held.record.clone(), held.label.as_str()))
                .collect::<Vec<_>>(),
            vec![
                (record("r0"), "f1"),
                (record("r9"), "f2"),
                (record("r2"), "f9"),
                (record("r1"), "f10"),
            ],
            "the refusal lists numeric labels, excluding records that take no slot"
        ),
        other => panic!("expected a full thing, got {other:?}"),
    }
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

/// Two thoughts of `bot:alpha` as a capture makes them, an edge and no fields:
/// created at `created_at`, then each given a field write at that moment and
/// edited at `edited_at` when it names one.
fn bare_room(created_at: jiff::Timestamp, edited_at: Option<jiff::Timestamp>) -> State {
    let mut state = roomy(0, Actor::Operator, created_at);
    let at = |moment: jiff::Timestamp, payload: ClaimEvent| {
        let mut event = under(
            Some(Actor::Bot(crate::memory::EntityId("bot:alpha".into()))),
            None,
            payload,
        );
        event.written_at = Some(moment);
        event
    };
    for n in 0..2 {
        let thought = ClaimBody {
            edge: Some((EdgeShape::Connection.to_string(), object("bot:alpha"))),
            ..claim(&format!("f{}", n + 2), "bot:alpha", "a thought", 1)
        };
        state.apply(&at(created_at, created(&format!("t{n}"), thought.clone())));
        if let Some(edited_at) = edited_at {
            // The edited thought also holds an older field write: the one shape
            // where a field-only moment reads it as quiet.
            state.apply(&at(
                created_at,
                write(&format!("t{n}"), "bot:alpha", "note", 1, "x"),
            ));
            state.apply(&at(
                edited_at,
                ClaimEvent::RecordRevised {
                    record: record(&format!("t{n}")),
                    claim: ClaimBody {
                        content: "a thought, kept".into(),
                        ..thought
                    },
                },
            ));
        }
    }
    state
}

/// **A thought with no fields ages from the write that created it**, as the
/// store ages it, and does not stay in the room for good.
#[test]
fn a_thought_with_no_fields_ages_out_of_the_room() {
    crate::memory::kinds::load_shipped();
    let created_at: jiff::Timestamp = "2026-04-01T10:00:00Z".parse().unwrap();
    let room = bare_room(created_at, None);
    let later = CaptureRequest {
        door: Door {
            aged_before: Some("2026-04-02T00:00:00Z".parse().unwrap()),
            ..thinking().door
        },
        ..thinking()
    };
    assert!(prepare(&room, &[], &later).is_ok(), "both are quiet");
    let sooner = CaptureRequest {
        door: Door {
            aged_before: Some("2026-04-01T09:00:00Z".parse().unwrap()),
            ..thinking().door
        },
        ..thinking()
    };
    assert!(
        matches!(
            prepare(&room, &[], &sooner),
            Err(Refusal::RoomFull { live: 2, .. })
        ),
        "neither is quiet yet"
    );
}

/// **An edit of a thought is a touch**, whatever fields it wrote: an edited
/// thought stays in the room while the edit is newer than the age line.
#[test]
fn an_edited_thought_stays_in_the_room() {
    crate::memory::kinds::load_shipped();
    let created_at: jiff::Timestamp = "2026-04-01T10:00:00Z".parse().unwrap();
    let edited_at: jiff::Timestamp = "2026-04-03T10:00:00Z".parse().unwrap();
    let room = bare_room(created_at, Some(edited_at));
    let between = CaptureRequest {
        door: Door {
            aged_before: Some("2026-04-02T00:00:00Z".parse().unwrap()),
            ..thinking().door
        },
        ..thinking()
    };
    assert!(matches!(
        prepare(&room, &[], &between),
        Err(Refusal::RoomFull {
            live: 2,
            aged_out: 0,
            ..
        })
    ));
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
    let mut guarded = full.clone();
    apply(
        &mut guarded,
        &write("t0", "bot:alpha", "rule_seats", 1, "2"),
    );
    assert!(matches!(
        prepare(&guarded, &[], &dropping("bot:alpha#f2")),
        Err(Refusal::RoomFull { live: 2, .. })
    ));
    let events = prepare(&guarded, &[], &dropping("bot:alpha#f3"))
        .expect("the ordinary thought can still be dropped");
    assert!(matches!(
        &events[0],
        ClaimEvent::RecordRevised { record, claim }
            if record == &super::super::tests::record("t1")
                && claim.status == FactStatus::Archived
    ));
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

/// A stored thought's first run identifies its writer when no actor was kept.
#[test]
fn a_stored_thoughts_first_run_identifies_its_writer() {
    crate::memory::kinds::load_shipped();
    let now = "2026-04-03T10:00:00Z".parse().unwrap();
    let own = Actor::Bot(crate::memory::EntityId("bot:alpha".into()));
    let mut state = roomy(1, own, now);
    for event in held("bot", "bot:beta", &[]) {
        apply(&mut state, &event);
    }
    let thought = ClaimBody {
        edge: Some((EdgeShape::Connection.to_string(), object("bot:alpha"))),
        ..claim("f3", "bot:alpha", "a thought", 1)
    };
    for payload in [
        created("t1", thought),
        write("t1", "bot:alpha", "note", 1, "x"),
    ] {
        let mut event = under(None, Some("sb01"), payload);
        event.written_at = Some(now);
        state.apply(&event);
    }
    let run = super::super::super::RunId("sb01".into());
    let mut sent = thinking();
    sent.door
        .runs
        .insert(run.clone(), crate::memory::EntityId("bot:beta".into()));
    assert!(
        prepare(&state, &[], &sent).is_ok(),
        "another bot's run leaves a slot beside the own thought"
    );
    sent.door
        .runs
        .insert(run, crate::memory::EntityId("bot:alpha".into()));
    assert!(matches!(
        prepare(&state, &[], &sent),
        Err(Refusal::RoomFull { live: 2, capacity: 2, room, .. }) if room.len() == 2
    ));
    // The answer names the room as the facts the State serves.
    let Err(refusal) = prepare(&state, &[], &sent) else {
        panic!("a full room refuses");
    };
    let Declined::Refused(MemoryError::RoomFull { room, .. }) = refusal.declined(&state) else {
        panic!("a full room is answered as a full room");
    };
    assert_eq!(
        room.len(),
        2,
        "each thought of the room is served as a fact"
    );
}

/// **A refusal is answered once, from the State that refused**: a missing name
/// is a block, a malformed write is its words, and a number held names the
/// number the next member takes.
#[test]
fn a_refusal_is_answered_from_the_state_that_refused() {
    let (state, declared) = world();
    assert_eq!(
        prepare(&state, &declared, &request("nobody"))
            .unwrap_err()
            .declined(&state),
        Declined::Blocked {
            attempted: crate::memory::EntityId("nobody".into()),
            candidates: Vec::new(),
        }
    );
    let malformed = CaptureRequest {
        sets: BTreeSet::from(["absent".to_string()]),
        ..request("alpha")
    };
    assert!(matches!(
        prepare(&state, &declared, &malformed)
            .unwrap_err()
            .declined(&state),
        Declined::Refused(MemoryError::InvalidFact(_))
    ));
    let (state, declared) = numbered_world();
    assert_eq!(
        prepare(&state, &declared, &numbering("m3", "1"))
            .unwrap_err()
            .declined(&state),
        Declined::Refused(MemoryError::NumberTaken {
            collection: "coll".into(),
            key: "num".into(),
            number: 1,
            holder: "m1".into(),
            next: 3,
        })
    );
}

/// **A name nothing holds is refused with the names it resembles**, read from
/// the State the refusal was decided on: the subject, an edge's object and a
/// ref alike. A name nothing resembles carries none.
#[test]
fn an_unknown_name_is_refused_with_the_names_it_resembles() {
    crate::memory::kinds::load_shipped();
    // Each thing answers to a roster name that is not its handle, so a handle
    // nobody holds can resemble it by the name alone.
    let named = |handle: &str, name: &str| {
        vec![
            ClaimEvent::ObjectCreated {
                object: object(handle),
                body: body("thing", name),
            },
            ClaimEvent::AddressBound {
                object: object(handle),
                address: handle.to_string(),
            },
        ]
    };
    let state = replay(
        &[
            named("thing:alpha", "delta"),
            named("thing:omega", "epsilon"),
        ]
        .concat(),
    );
    let handles = |matches: &[crate::memory::guard::EntityMatch]| -> Vec<String> {
        matches.iter().map(|m| m.handle.to_string()).collect()
    };

    let Err(Refusal::UnknownSubject(attempted, near)) =
        prepare(&state, &[], &request("thing:delta"))
    else {
        panic!("a subject nobody holds is refused");
    };
    assert_eq!(attempted, "thing:delta");
    assert_eq!(handles(&near), vec!["thing:alpha"]);

    let Err(Refusal::UnknownEdgeObject(_, near)) = prepare(
        &state,
        &[],
        &CaptureRequest {
            edge: Some((EdgeShape::Connection, "thing:epsilon".into())),
            ..request("thing:alpha")
        },
    ) else {
        panic!("an edge object nobody holds is refused");
    };
    assert_eq!(handles(&near), vec!["thing:omega"]);

    let Err(Refusal::UnknownRef(_, near)) = prepare(
        &state,
        &[],
        &CaptureRequest {
            refs: vec!["thing:delta".into()],
            ..request("thing:alpha")
        },
    ) else {
        panic!("a ref nobody holds is refused");
    };
    assert_eq!(handles(&near), vec!["thing:alpha"]);

    let Err(Refusal::UnknownSubject(_, near)) = prepare(&state, &[], &request("thing:sigma"))
    else {
        panic!("a subject nobody holds is refused");
    };
    assert!(near.is_empty(), "nothing resembles it: {near:?}");
}
