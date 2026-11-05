use jojobot_domain::log::testing::{Fault, FaultLog, InMemoryLog};
use jojobot_domain::log::{Event, LogStore, Position};
use jojobot_domain::request::Recorded;
use jojobot_domain::request::events::{LegacyRow, Request, RequestKey};

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

async fn ledger(log: &Arc<dyn LogStore>) -> LoggedLedger {
    LoggedLedger::open(log.clone()).await.unwrap()
}

/// **A restart replays to the same ledger**, answered from the log alone: the
/// commit a request id spent, its receipt, its parts and an app attempt are all
/// found again, and the head is where the writes left it.
pub(super) async fn restart_replay_on(log: Arc<dyn LogStore>) {
    let first = ledger(&log).await;
    first
        .append(
            vec![
                LedgerEvent::RequestPartCommitted(part("entity")),
                LedgerEvent::RequestPartCommitted(part("claim")),
            ],
            at(0),
        )
        .await
        .unwrap();
    first
        .append(
            vec![
                LedgerEvent::RequestCompleted {
                    commit: commit(1),
                    required_parts: vec!["entity".into(), "claim".into()],
                },
                LedgerEvent::RequestReceiptRecorded {
                    key: key(1),
                    receipt: "receipt-one".into(),
                },
                LedgerEvent::AppAttemptCommitted(app()),
            ],
            at(1),
        )
        .await
        .unwrap();
    let head = first.head().await;
    assert_eq!(head, Position(5));
    drop(first);

    let second = ledger(&log).await;
    assert_eq!(second.head().await, head);
    let held = second.lookup(&actor(), &id(), at(0)).await.unwrap();
    assert_eq!(held.receipt.as_deref(), Some("receipt-one"));
    assert_eq!(held.request.key.generation, 1);
    let names: Vec<String> = second
        .parts(&actor(), &id(), at(0))
        .await
        .into_iter()
        .map(|held| held.name)
        .collect();
    assert_eq!(names, ["claim", "entity"]);
    assert_eq!(second.app(&app().key).await, Some(app()));
    assert!(second.lookup(&actor(), &id(), at(11)).await.is_none());
    assert!(
        second
            .lookup(&ObjectId("bot:marge".into()), &id(), at(0))
            .await
            .is_none()
    );
}

/// **A batch the state cannot apply never reaches the log.** A completion that
/// names a part nobody committed is refused, and the log holds nothing.
pub(super) async fn an_unappliable_batch_is_refused_before_the_log(log: Arc<dyn LogStore>) {
    let ledger = ledger(&log).await;
    let refused = ledger
        .append(
            vec![LedgerEvent::RequestCompleted {
                commit: commit(1),
                required_parts: vec!["entity".into()],
            }],
            at(0),
        )
        .await;
    assert!(
        matches!(refused, Err(LedgerError::Refused(_))),
        "{refused:?}"
    );
    assert_eq!(ledger.head().await, Position(0));
    assert!(
        log.read_from(Position(0), 10).await.unwrap().is_empty(),
        "the refused batch is not in the log"
    );
    assert!(ledger.lookup(&actor(), &id(), at(0)).await.is_none());
}

/// **A member of another family leaves the replay equal.** The log holds every
/// family's members; the ledger applies its own and skips the rest, whichever
/// side of its own member the foreign one sits.
pub(super) async fn another_family_leaves_the_replay_equal(log: Arc<dyn LogStore>) {
    use jojobot_domain::teaching::events::{RunId, TeachingEvent};
    let foreign = crate::teaching_log::codec::encode(&TeachingEvent::Cleared {
        run: RunId("homer".into()),
    });
    let own = codec::encode(&LedgerEvent::RequestCommitted(commit(1)));
    let batch = SealedBatch::seal(
        BatchId("mixed".into()),
        Position(0),
        at(0),
        REPRESENTATION_VERSION,
        vec![foreign.clone(), own, foreign],
    )
    .unwrap();
    assert!(matches!(
        log.append(Position(0), &batch).await.unwrap(),
        Append::Committed(_)
    ));
    let ledger = ledger(&log).await;
    assert_eq!(ledger.head().await, Position(3));
    assert_eq!(ledger.lookup(&actor(), &id(), at(0)).await, Some(commit(1)));
}

/// **An unknown kind of the ledger's own family fails the replay**, so a build
/// that cannot read what the log holds does not serve a ledger that has it wrong.
pub(super) async fn an_unknown_own_kind_fails_the_replay(log: Arc<dyn LogStore>) {
    let batch = SealedBatch::seal(
        BatchId("future".into()),
        Position(0),
        at(0),
        REPRESENTATION_VERSION,
        vec![stored("LedgerSomethingNew", 1)],
    )
    .unwrap();
    log.append(Position(0), &batch).await.unwrap();
    let opened = LoggedLedger::open(log.clone()).await;
    assert!(
        matches!(opened, Err(LedgerError::Store(_))),
        "{:?}",
        opened.err()
    );
}

#[tokio::test]
async fn a_restart_replays_to_the_same_ledger() {
    restart_replay_on(Arc::new(InMemoryLog::default())).await;
}

#[tokio::test]
async fn an_unappliable_batch_is_refused_before_it_reaches_the_log() {
    an_unappliable_batch_is_refused_before_the_log(Arc::new(InMemoryLog::default())).await;
}

#[tokio::test]
async fn a_member_of_another_family_leaves_the_replay_equal() {
    another_family_leaves_the_replay_equal(Arc::new(InMemoryLog::default())).await;
}

#[tokio::test]
async fn an_unknown_kind_of_the_ledgers_own_family_fails_the_replay() {
    an_unknown_own_kind_fails_the_replay(Arc::new(InMemoryLog::default())).await;
}

/// **A lost acknowledgement is settled by the tail**: the commit landed, the
/// caller is told it does not know, and the held state already holds the batch.
#[tokio::test]
async fn a_lost_acknowledgement_is_settled_by_the_tail_read() {
    let log = InMemoryLog::default();
    let ledger = LoggedLedger::open(Arc::new(log.clone())).await.unwrap();
    log.inject(Fault::LostAcknowledgement).await;
    let appended = ledger
        .append(vec![LedgerEvent::RequestCommitted(commit(1))], at(0))
        .await;
    assert_eq!(appended, Err(LedgerError::Indeterminate));
    assert_eq!(ledger.head().await, Position(1));
    assert_eq!(ledger.lookup(&actor(), &id(), at(0)).await, Some(commit(1)));
}
