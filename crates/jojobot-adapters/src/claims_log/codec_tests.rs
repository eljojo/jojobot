use std::collections::BTreeMap;

use jiff::civil::date;
use jojobot_domain::actor::Actor;
use jojobot_domain::app::{AppId, AppStamp};
use jojobot_domain::log::claims::{
    ClaimBody, ClaimEvent, FieldValue, KeyId, KindId, MovedRecord, MovedWrite, ObjectBody,
    ObjectSnapshot,
};
use jojobot_domain::log::{Event, ObjectId, RecordId, RunId};
use jojobot_domain::memory::types::{DeclaredType, Field, Fold, Kinds, Origin, ValueType};
use jojobot_domain::memory::{EntityKind, FactStatus, Provenance, Receipt, Standing, WriteClass};

use super::codec::{DecodeError, SHAPES, decode, decode_own, encode};

fn object(key: &str) -> ObjectId {
    ObjectId(key.to_string())
}

fn record(key: &str) -> RecordId {
    RecordId(key.to_string())
}

fn body() -> ObjectBody {
    ObjectBody {
        kind: KindId("person".into()),
        parent: Some(object("k0")),
        name: "person:homer".into(),
        source: "operator".into(),
        aliases: vec!["Homie".into()],
        boot: Some("on-demand".into()),
        metadata: BTreeMap::from([("crm".to_string(), "c7".to_string())]),
        archived: false,
        bound_to: AppId::parse("watchbot").ok(),
    }
}

fn snapshot() -> ObjectSnapshot {
    ObjectSnapshot {
        kind: "person".into(),
        name: "Homer".into(),
        source: "operator".into(),
        crm: Some("c7".into()),
        parent: Some(object("k0")),
        boot: "on-demand".into(),
        archived_reason: Some("moved".into()),
        archived_at: Some("2026-11-04".into()),
        merged_into: Some(object("k9")),
        aliases: vec!["Homie".into()],
        prose: "likes donuts".into(),
    }
}

fn claim() -> ClaimBody {
    ClaimBody {
        label: "f1".into(),
        home: object("k1"),
        content: "likes donuts".into(),
        details: Some("since the wedding".into()),
        provenance: Provenance::Testimony,
        standing: Standing::Settled,
        status: FactStatus::Active,
        recorded_at: date(2026, 11, 5),
        happened_at: Some(date(2026, 11, 1)),
        happened_through: Some(date(2026, 11, 2)),
        stale_after: Some(date(2026, 12, 1)),
        edge: Some(("about".into(), object("k2"))),
        derived_from: Some(record("r0")),
        refs: vec![object("k3")],
        stands_for: vec![record("r9")],
        receipt: Some(Receipt::Confirmed {
            on: date(2026, 11, 5),
            at: "message m1".into(),
        }),
        app: Some(AppStamp {
            app: "watchbot".into(),
            grant: "write".into(),
            received_at: "2026-11-05T01:00:00Z".into(),
        }),
        inserted_at: "2026-11-05T01:00:00Z".parse().ok(),
    }
}

fn declaration() -> DeclaredType {
    // A declaration's kinds are read back by the shipped kinds' names.
    jojobot_domain::memory::kinds::load_shipped();
    DeclaredType {
        name: "tally".into(),
        fields: vec![
            Field {
                key: "visits".into(),
                holds: ValueType::Number,
                points_at: None,
                folds: Fold::Sum,
                required: true,
                list: false,
                one_of: None,
            },
            Field {
                key: "owner".into(),
                holds: ValueType::Reference,
                points_at: Kinds::any_of(&[EntityKind::PERSON, EntityKind::BOT]),
                folds: Fold::Newest,
                required: false,
                list: true,
                one_of: Some(vec!["a".into(), "b".into()]),
            },
        ],
        origin: Origin::Declared,
        must: Some("a tally counts".into()),
    }
}

/// One event of every kind the family has, each carrying a value in every field.
fn every_kind() -> Vec<ClaimEvent> {
    vec![
        ClaimEvent::ObjectCreated {
            object: object("k1"),
            body: body(),
        },
        ClaimEvent::ObjectRevised {
            object: object("k1"),
            body: body(),
        },
        ClaimEvent::ObjectBaselined {
            object: object("k1"),
            body: body(),
        },
        ClaimEvent::ObjectWriteObserved {
            object: object("k1"),
            filed_under: "person:homer".into(),
            ordinal: 3,
            snapshot: Some(snapshot()),
        },
        ClaimEvent::ObjectWriteObserved {
            object: object("k1"),
            filed_under: "person:homer".into(),
            ordinal: 4,
            snapshot: None,
        },
        ClaimEvent::AddressBound {
            object: object("k1"),
            address: "person:homer".into(),
        },
        ClaimEvent::AddressRetired {
            object: object("k1"),
            address: "person:marge".into(),
            day: date(2026, 3, 9),
        },
        ClaimEvent::ObjectsMerged {
            folded: object("k2"),
            survivor: object("k1"),
            records: vec![MovedRecord {
                record: record("r1"),
                label: "f4".into(),
            }],
            writes: vec![MovedWrite {
                record: record("r1"),
                key: KeyId("visits".into()),
                from_ordinal: 2,
                ordinal: 5,
            }],
        },
        ClaimEvent::ObjectRedirected {
            folded: object("k2"),
            survivor: object("k1"),
        },
        ClaimEvent::MergeNumbersLeftEmpty { count: 2 },
        ClaimEvent::ImportCompleted { events: 7 },
        ClaimEvent::RecordCreated {
            record: record("r1"),
            claim: claim(),
        },
        ClaimEvent::RecordRevised {
            record: record("r1"),
            claim: claim(),
        },
        ClaimEvent::FieldWritten {
            record: record("r1"),
            subject: object("k1"),
            key: KeyId("visits".into()),
            ordinal: 2,
            class: WriteClass::Sets,
            write: FieldValue::Set("3".into()),
            targets: vec![object("k3")],
        },
        ClaimEvent::FieldWritten {
            record: record("r1"),
            subject: object("k1"),
            key: KeyId("visits".into()),
            ordinal: 3,
            class: WriteClass::Stays,
            write: FieldValue::Clear,
            targets: Vec::new(),
        },
        ClaimEvent::FieldRemoved {
            record: record("r1"),
            subject: object("k1"),
            key: KeyId("visits".into()),
            ordinal: 4,
            class: WriteClass::Stays,
            targets: Vec::new(),
        },
        ClaimEvent::TypeDeclared {
            kind: object("k5"),
            declaration: declaration(),
        },
    ]
}

fn enveloped(payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
        event_kind: "whatever the caller wrote".into(),
        meaning_version: 99,
        actor: Some(Actor::Operator),
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: Some("part-1".into()),
        occurred_at: "2026-11-05T01:00:00Z".parse().ok(),
        written_at: "2026-11-05T01:00:01Z".parse().ok(),
        imported_source_time: "2026-10-01T00:00:00Z".parse().ok(),
        payload,
    }
}

/// **Every kind of claims event comes back as it went in**, the envelope's actor,
/// run and moments with it. The State reads those off the envelope, so a codec
/// that dropped one would change what the State says. The kind and the meaning
/// version are the codec's own: the caller's are replaced.
#[test]
fn every_kind_of_claims_event_round_trips_with_its_envelope() {
    for payload in every_kind() {
        let event = enveloped(payload);
        let stored = encode(&event);
        let read = decode(&stored).expect("a stored event reads back");
        assert_eq!(read.payload, event.payload, "{event:?}");
        assert_eq!(read.actor, event.actor);
        assert_eq!(read.run, event.run);
        assert_eq!(read.part, event.part);
        assert_eq!(read.occurred_at, event.occurred_at);
        assert_eq!(read.written_at, event.written_at);
        assert_eq!(read.imported_source_time, event.imported_source_time);
        assert_eq!(read.meaning_version, 1);
        assert!(read.event_kind.starts_with("Claims"), "{}", read.event_kind);
        assert_eq!(
            decode_own(&stored).expect("own reads").expect("own kind"),
            read
        );
    }
}

/// **Each kind has a name of its own.** Two kinds that shared one could not be
/// told apart by a reader that looks only at the envelope.
#[test]
fn each_kind_names_itself_once() {
    let mut names: Vec<String> = every_kind()
        .into_iter()
        .map(|payload| {
            let stored = encode(&enveloped(payload));
            decode(&stored).expect("reads").event_kind
        })
        .collect();
    names.sort();
    names.dedup();
    assert_eq!(names.len(), 15, "{names:?}");
}

/// **Another family's event is that family's to read**, and a claims kind this
/// build does not know is an error and never skipped.
#[test]
fn another_familys_kind_is_passed_over_and_an_unknown_claims_kind_is_an_error() {
    let teaching = br#"{"event_kind":"TeachingRecorded","meaning_version":1,"payload":{}}"#;
    assert!(
        decode_own(teaching)
            .expect("a kind of another family")
            .is_none()
    );

    let future = br#"{"event_kind":"ClaimsSomethingNew","meaning_version":1,"actor":null,"run":null,"session_card":null,"request":null,"part":null,"occurred_at":null,"written_at":null,"imported_source_time":null,"payload":{"shape":"SomethingNew"}}"#;
    assert!(matches!(
        decode_own(future),
        Err(DecodeError::Unknown(kind, 1)) if kind == "ClaimsSomethingNew"
    ));

    let later = br#"{"event_kind":"ClaimsAddressBound","meaning_version":2,"actor":null,"run":null,"session_card":null,"request":null,"part":null,"occurred_at":null,"written_at":null,"imported_source_time":null,"payload":{}}"#;
    assert!(matches!(decode_own(later), Err(DecodeError::Unknown(_, 2))));
}

/// **A kind that does not match its payload is not read.**
#[test]
fn a_kind_that_does_not_match_its_payload_is_unreadable() {
    let stored = encode(&enveloped(ClaimEvent::AddressBound {
        object: object("k1"),
        address: "person:homer".into(),
    }));
    let text = String::from_utf8(stored).expect("json is text");
    let forged = text.replace("ClaimsAddressBound", "ClaimsAddressRetired");
    assert!(matches!(
        decode(forged.as_bytes()),
        Err(DecodeError::Unreadable(_))
    ));
}

/// **The stored spelling is pinned.** Nothing outside this process declares the
/// kind names or the payload keys, and a record already in the log cannot fail a
/// test, so a renamed constant would move both sides of a round trip together and
/// leave every stored event unreadable. These literals are the spelling.
#[test]
fn the_stored_spelling_is_pinned() {
    let names: Vec<String> = every_kind()
        .into_iter()
        .map(|payload| {
            let stored = encode(&enveloped(payload));
            let value: serde_json::Value = serde_json::from_slice(&stored).expect("json");
            value["event_kind"].as_str().expect("a kind").to_string()
        })
        .collect();
    assert_eq!(
        names,
        [
            "ClaimsObjectCreated",
            "ClaimsObjectRevised",
            "ClaimsObjectBaselined",
            "ClaimsObjectWriteObserved",
            "ClaimsObjectWriteObserved",
            "ClaimsAddressBound",
            "ClaimsAddressRetired",
            "ClaimsObjectsMerged",
            "ClaimsObjectRedirected",
            "ClaimsMergeNumbersLeftEmpty",
            "ClaimsImportCompleted",
            "ClaimsRecordCreated",
            "ClaimsRecordRevised",
            "ClaimsFieldWritten",
            "ClaimsFieldWritten",
            "ClaimsFieldRemoved",
            "ClaimsTypeDeclared",
        ]
    );
    let written = encode(&enveloped(ClaimEvent::FieldWritten {
        record: record("r1"),
        subject: object("k1"),
        key: KeyId("visits".into()),
        ordinal: 2,
        class: WriteClass::Sets,
        write: FieldValue::Set("3".into()),
        targets: vec![object("k3")],
    }));
    let value: serde_json::Value = serde_json::from_slice(&written).expect("json");
    assert_eq!(
        value["payload"],
        serde_json::json!({
            "shape": "FieldWritten",
            "record": "r1",
            "subject": "k1",
            "key": "visits",
            "ordinal": 2,
            "class": "sets",
            "write": "3",
            "targets": ["k3"],
        })
    );
    assert_eq!(value["meaning_version"], 1);
}

/// **The stored payload of every shape is pinned by literal.** Each literal is the
/// payload a real event of that shape stores; a renamed key, tag or token moves
/// the stored form and every event already in the log with it, so none of these
/// may follow a rename. The shapes the family stores are the shape list, and the
/// events, the literals and that list all name the same fifteen.
#[test]
fn the_stored_payload_of_every_shape_is_pinned() {
    use serde_json::json;
    let body = json!({
        "aliases": ["Homie"], "archived": false, "boot": "on-demand", "bound_to": "watchbot",
        "kind": "person", "metadata": {"crm": "c7"}, "name": "person:homer",
        "parent": "k0", "source": "operator",
    });
    let snapshot = json!({
        "aliases": ["Homie"], "archived_at": "2026-11-04", "archived_reason": "moved",
        "boot": "on-demand", "crm": "c7", "kind": "person", "merged_into": "k9",
        "name": "Homer", "parent": "k0", "prose": "likes donuts", "source": "operator",
    });
    let claim = json!({
        "app": {"app": "watchbot", "grant": "write", "received_at": "2026-11-05T01:00:00Z"},
        "content": "likes donuts", "derived_from": "r0", "details": "since the wedding",
        "edge": ["about", "k2"], "happened_at": "2026-11-01", "happened_through": "2026-11-02",
        "home": "k1", "inserted_at": "2026-11-05T01:00:00Z", "label": "f1",
        "provenance": "testimony",
        "receipt": {"confirmed": {"at": "message m1", "on": "2026-11-05"}},
        "recorded_at": "2026-11-05", "refs": ["k3"], "stale_after": "2026-12-01",
        "standing": "settled", "stands_for": ["r9"], "status": "active",
    });
    let pinned = [
        json!({"shape": "ObjectCreated", "object": "k1", "body": body}),
        json!({"shape": "ObjectRevised", "object": "k1", "body": body}),
        json!({"shape": "ObjectBaselined", "object": "k1", "body": body}),
        json!({"shape": "ObjectWriteObserved", "object": "k1", "filed_under": "person:homer",
               "ordinal": 3, "snapshot": snapshot}),
        json!({"shape": "ObjectWriteObserved", "object": "k1", "filed_under": "person:homer",
               "ordinal": 4, "snapshot": null}),
        json!({"shape": "AddressBound", "object": "k1", "address": "person:homer"}),
        json!({"shape": "AddressRetired", "object": "k1", "address": "person:marge",
               "day": "2026-03-09"}),
        json!({"shape": "ObjectsMerged", "folded": "k2", "survivor": "k1",
               "records": [{"label": "f4", "record": "r1"}],
               "writes": [{"from_ordinal": 2, "key": "visits", "ordinal": 5, "record": "r1"}]}),
        json!({"shape": "ObjectRedirected", "folded": "k2", "survivor": "k1"}),
        json!({"shape": "MergeNumbersLeftEmpty", "count": 2}),
        json!({"shape": "ImportCompleted", "events": 7}),
        json!({"shape": "RecordCreated", "record": "r1", "claim": claim}),
        json!({"shape": "RecordRevised", "record": "r1", "claim": claim}),
        json!({"shape": "FieldWritten", "record": "r1", "subject": "k1", "key": "visits",
               "ordinal": 2, "class": "sets", "write": "3", "targets": ["k3"]}),
        json!({"shape": "FieldWritten", "record": "r1", "subject": "k1", "key": "visits",
               "ordinal": 3, "class": "stays", "write": null, "targets": []}),
        json!({"shape": "FieldRemoved", "record": "r1", "subject": "k1", "key": "visits",
               "ordinal": 4, "class": "stays", "targets": []}),
        json!({"shape": "TypeDeclared", "kind": "k5", "declaration": {
            "fields": [
                {"folds": "sum", "holds": "number", "key": "visits", "list": false,
                 "one_of": null, "points_at": null, "required": true},
                {"folds": "newest", "holds": "reference", "key": "owner", "list": true,
                 "one_of": ["a", "b"], "points_at": "person|bot", "required": false},
            ],
            "must": "a tally counts", "name": "tally", "origin": "declared"}}),
    ];
    let stored: Vec<serde_json::Value> = every_kind()
        .into_iter()
        .map(|payload| {
            let bytes = encode(&enveloped(payload));
            let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
            value["payload"].clone()
        })
        .collect();
    assert_eq!(stored, pinned);

    let shapes_of = |values: &[serde_json::Value]| -> std::collections::BTreeSet<String> {
        values
            .iter()
            .map(|value| value["shape"].as_str().expect("a shape").to_string())
            .collect()
    };
    let listed: std::collections::BTreeSet<String> =
        SHAPES.iter().map(|shape| shape.to_string()).collect();
    assert_eq!(listed.len(), 15);
    assert_eq!(shapes_of(&stored), listed, "an event of every shape stored");
    assert_eq!(
        shapes_of(&pinned),
        listed,
        "a literal for every shape stored"
    );
}
