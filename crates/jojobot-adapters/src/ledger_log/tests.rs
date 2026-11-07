use jiff::Timestamp;
use jojobot_domain::log::{Event, ObjectId, RequestId};
use jojobot_domain::request::Recorded;
use jojobot_domain::request::events::{
    AppAttempt, AppKey, Commit, LedgerEvent, LegacyRow, Part, Request, RequestKey,
};

use super::*;

pub(super) fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn actor() -> ObjectId {
    ObjectId("bot:homer".into())
}

fn id() -> RequestId {
    RequestId("alpha".into())
}

fn key(generation: u64) -> RequestKey {
    RequestKey {
        actor: actor(),
        id: id(),
        generation,
    }
}

fn request(generation: u64) -> Request {
    Request {
        key: key(generation),
        verb: "capture".into(),
        digest: "digest-alpha".into(),
    }
}

fn commit(generation: u64) -> Commit {
    Commit {
        request: request(generation),
        landed: "{\"address\":\"topic:alpha#f1\"}".into(),
        receipt: None,
        written_at: at(0),
        expires_at: at(10),
    }
}

fn part(name: &str) -> Part {
    Part {
        request: request(1),
        name: name.into(),
        recorded: Recorded {
            landed: name.into(),
            note: Some("frozen-plan".into()),
        },
        written_at: at(0),
        expires_at: at(10),
    }
}

fn app() -> AppAttempt {
    AppAttempt {
        key: AppKey {
            app_id: "homer".into(),
            event_key: "alpha".into(),
        },
        digest: "app-digest".into(),
        landed: "app-result".into(),
        written_at: at(0),
    }
}

/// Every event shape of the family, the three legacy rows among them.
fn every_event() -> Vec<LedgerEvent> {
    vec![
        LedgerEvent::RequestPartCommitted(part("entity")),
        LedgerEvent::RequestCommitted(commit(1)),
        LedgerEvent::RequestReceiptRecorded {
            key: key(1),
            receipt: "receipt-one".into(),
        },
        LedgerEvent::RequestCompleted {
            commit: commit(2),
            required_parts: vec!["entity".into(), "claim".into()],
        },
        LedgerEvent::RequestExpired { key: key(1) },
        LedgerEvent::AppAttemptCommitted(app()),
        LedgerEvent::Imported {
            row: LegacyRow::RequestReceipt {
                bot_key: actor(),
                request_id: id(),
                verb: "capture".into(),
                args_digest: "digest-alpha".into(),
                landed: "{}".into(),
                receipt: Some("receipt".into()),
                written_at: at(1),
            },
            generation: 3,
        },
        LedgerEvent::Imported {
            row: LegacyRow::RequestPart {
                bot_key: actor(),
                request_id: id(),
                part: "entity".into(),
                verb: "capture".into(),
                args_digest: "digest-alpha".into(),
                landed: "{}".into(),
                note: None,
                written_at: at(2),
            },
            generation: 3,
        },
        LedgerEvent::Imported {
            row: LegacyRow::AppWrite {
                app_id: "homer".into(),
                event_key: "alpha".into(),
                digest: "app-digest".into(),
                landed: "app-result".into(),
                written_at: at(3),
            },
            generation: 1,
        },
    ]
}

/// **The stored spelling is pinned.** The kinds and the version are written into
/// the log and nothing outside this process declares them, so a rename here
/// leaves every stored record unreadable.
#[test]
fn the_stored_spelling_is_pinned() {
    let kinds: Vec<String> = every_event()
        .iter()
        .map(|event| {
            let stored = String::from_utf8(codec::encode(event)).unwrap();
            assert!(stored.contains("\"meaning_version\":1"), "{stored}");
            stored
        })
        .collect();
    for (stored, kind) in kinds.iter().zip([
        "LedgerRequestPartCommitted",
        "LedgerRequestCommitted",
        "LedgerRequestReceiptRecorded",
        "LedgerRequestCompleted",
        "LedgerRequestExpired",
        "LedgerAppAttemptCommitted",
        "LedgerImported",
        "LedgerImported",
        "LedgerImported",
    ]) {
        assert!(
            stored.contains(&format!("\"event_kind\":\"{kind}\"")),
            "{kind}: {stored}"
        );
    }
}

/// **An event survives its stored encoding**, whichever of the nine shapes it is.
#[test]
fn every_event_survives_its_encoding() {
    for event in every_event() {
        let bytes = codec::encode(&event);
        assert_eq!(codec::decode(&bytes).unwrap(), event);
        assert_eq!(codec::decode_own(&bytes).unwrap(), Some(event));
    }
}

fn stored(kind: &str, version: u32) -> Vec<u8> {
    serde_json::to_vec(&Event {
        event_kind: kind.to_string(),
        meaning_version: version,
        actor: None,
        run: None,
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: None,
        imported_source_time: None,
        payload: serde_json::json!({}),
    })
    .unwrap()
}

/// **An unknown kind or version of the ledger's own family is refused rather
/// than skipped**, a kind of another family is left to its reader, and bytes
/// that are no envelope are unreadable.
#[test]
fn an_unknown_own_kind_or_version_is_refused_and_a_foreign_kind_is_left_alone() {
    assert!(matches!(
        codec::decode_own(&stored("LedgerSomethingNew", 1)),
        Err(codec::DecodeError::Unknown(kind, 1)) if kind == "LedgerSomethingNew"
    ));
    assert!(matches!(
        codec::decode_own(&stored("LedgerRequestCommitted", 2)),
        Err(codec::DecodeError::Unknown(kind, 2)) if kind == "LedgerRequestCommitted"
    ));
    assert_eq!(
        codec::decode_own(&stored("TeachingCleared", 1)).unwrap(),
        None
    );
    assert!(matches!(
        codec::decode_own(b"not an envelope"),
        Err(codec::DecodeError::Unreadable(_))
    ));
}

/// **The stored payload spelling is pinned, key by key.** The keys of a payload,
/// the `shape` tags of an imported row and the envelope's request, part and
/// source time are stored, and nothing outside this process declares them. Encode
/// then decode passes under any consistent rename, so each shape is asserted
/// against a literal written out here, not one the codec made.
#[test]
fn the_stored_payload_spelling_is_pinned() {
    use serde_json::{Value, json};
    let request = |generation: u64| {
        json!({
            "key": {"actor": "bot:homer", "id": "alpha", "generation": generation},
            "verb": "capture",
            "digest": "digest-alpha",
        })
    };
    let landed = "{\"address\":\"topic:alpha#f1\"}";
    let (t0, t10) = ("2023-11-14T22:13:20Z", "2023-11-14T22:13:30Z");
    let expected = [
        (
            Some("alpha"),
            Some("entity"),
            None,
            json!({
                "request": request(1), "name": "entity", "landed": "entity",
                "note": "frozen-plan", "written_at": t0, "expires_at": t10,
            }),
        ),
        (
            Some("alpha"),
            None,
            None,
            json!({
                "request": request(1), "landed": landed, "receipt": null,
                "written_at": t0, "expires_at": t10,
            }),
        ),
        (
            Some("alpha"),
            None,
            None,
            json!({
                "key": {"actor": "bot:homer", "id": "alpha", "generation": 1},
                "receipt": "receipt-one",
            }),
        ),
        (
            Some("alpha"),
            None,
            None,
            json!({
                "commit": {
                    "request": request(2), "landed": landed, "receipt": null,
                    "written_at": t0, "expires_at": t10,
                },
                "required_parts": ["entity", "claim"],
            }),
        ),
        (
            Some("alpha"),
            None,
            None,
            json!({"key": {"actor": "bot:homer", "id": "alpha", "generation": 1}}),
        ),
        (
            None,
            None,
            None,
            json!({
                "app_id": "homer", "event_key": "alpha", "digest": "app-digest",
                "landed": "app-result", "written_at": t0,
            }),
        ),
        (
            None,
            None,
            Some("2023-11-14T22:13:21Z"),
            json!({
                "row": {
                    "shape": "RequestReceipt", "bot_key": "bot:homer",
                    "request_id": "alpha", "verb": "capture",
                    "args_digest": "digest-alpha", "landed": "{}",
                    "receipt": "receipt", "written_at": "2023-11-14T22:13:21Z",
                },
                "generation": 3,
            }),
        ),
        (
            None,
            None,
            Some("2023-11-14T22:13:22Z"),
            json!({
                "row": {
                    "shape": "RequestPart", "bot_key": "bot:homer",
                    "request_id": "alpha", "part": "entity", "verb": "capture",
                    "args_digest": "digest-alpha", "landed": "{}", "note": null,
                    "written_at": "2023-11-14T22:13:22Z",
                },
                "generation": 3,
            }),
        ),
        (
            None,
            None,
            Some("2023-11-14T22:13:23Z"),
            json!({
                "row": {
                    "shape": "AppWrite", "app_id": "homer", "event_key": "alpha",
                    "digest": "app-digest", "landed": "app-result",
                    "written_at": "2023-11-14T22:13:23Z",
                },
                "generation": 1,
            }),
        ),
    ];
    assert_eq!(
        every_event().len(),
        expected.len(),
        "every shape needs its literal"
    );
    for (event, (request, part, source, payload)) in every_event().iter().zip(expected) {
        let stored: Value = serde_json::from_slice(&codec::encode(event)).unwrap();
        assert_eq!(stored["payload"], payload, "{event:?}");
        assert_eq!(stored["request"], json!(request), "{event:?}");
        assert_eq!(stored["part"], json!(part), "{event:?}");
        assert_eq!(stored["imported_source_time"], json!(source), "{event:?}");
    }
}
