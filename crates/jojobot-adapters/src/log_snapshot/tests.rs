use super::*;
use std::sync::Mutex;

use jojobot_domain::log::testing::InMemoryLog;
use jojobot_domain::log::{Append, BatchId, ObjectId, RequestId};
use jojobot_domain::request::events::{
    AppAttempt, AppKey, Commit, LedgerEvent, Request, RequestKey,
};
use jojobot_domain::teaching::events::{RunId, TeachingEvent};

fn request() -> Request {
    Request {
        key: RequestKey {
            actor: ObjectId("alpha".into()),
            id: RequestId("beta".into()),
            generation: 2,
        },
        verb: "capture".into(),
        digest: "alpha".into(),
    }
}

async fn append(log: &dyn LogStore, id: &str, members: Vec<Vec<u8>>) {
    let head = log
        .read_from(Position(0), 256)
        .await
        .unwrap()
        .last()
        .map_or(Position(0), |batch| batch.range().last);
    let batch = SealedBatch::seal(
        BatchId(id.into()),
        head,
        jiff::Timestamp::UNIX_EPOCH,
        1,
        members,
    )
    .unwrap();
    assert_eq!(
        log.append(head, &batch).await.unwrap(),
        Append::Committed(batch.range())
    );
}

async fn prefix(log: &dyn LogStore, run: &str) {
    append(
        log,
        "alpha",
        vec![
            teaching::encode(&TeachingEvent::Recorded {
                run: RunId(run.into()),
                domain: "claims".into(),
                taught_at: jiff::Timestamp::UNIX_EPOCH,
            }),
            ledger::encode(&LedgerEvent::RequestCommitted(Commit {
                request: request(),
                landed: "alpha".into(),
                receipt: Some(run.into()),
                written_at: jiff::Timestamp::UNIX_EPOCH,
                expires_at: jiff::Timestamp::from_second(10_000).unwrap(),
            })),
            ledger::encode(&LedgerEvent::AppAttemptCommitted(AppAttempt {
                key: AppKey {
                    app_id: "springfield-watch".into(),
                    event_key: "alpha".into(),
                },
                digest: "alpha".into(),
                landed: run.into(),
                written_at: jiff::Timestamp::UNIX_EPOCH,
            })),
        ],
    )
    .await;
}

async fn tail(log: &dyn LogStore) {
    append(
        log,
        "beta",
        vec![
            teaching::encode(&TeachingEvent::Cleared {
                run: RunId("homer".into()),
            }),
            teaching::encode(&TeachingEvent::Recorded {
                run: RunId("marge".into()),
                domain: "mail".into(),
                taught_at: jiff::Timestamp::from_second(1).unwrap(),
            }),
            ledger::encode(&LedgerEvent::RequestReceiptRecorded {
                key: request().key,
                receipt: "beta".into(),
            }),
        ],
    )
    .await;
}

#[derive(Default)]
struct ReadLog {
    log: InMemoryLog,
    positions: Mutex<Vec<Position>>,
}

#[async_trait::async_trait]
impl LogStore for ReadLog {
    async fn validity(&self) -> Result<Validity, LogError> {
        self.log.validity().await
    }
    async fn append(&self, head: Position, batch: &SealedBatch) -> Result<Append, LogError> {
        self.log.append(head, batch).await
    }
    async fn read_from(
        &self,
        position: Position,
        bound: usize,
    ) -> Result<Vec<SealedBatch>, LogError> {
        self.positions.lock().unwrap().push(position);
        self.log.read_from(position, bound).await
    }
    async fn load_snapshot(&self) -> Result<Option<ValidatedSnapshot>, LogError> {
        self.log.load_snapshot().await
    }
    async fn store_snapshot(&self, snapshot: &ValidatedSnapshot) -> Result<(), LogError> {
        self.log.store_snapshot(snapshot).await
    }
}

fn projection(state: &State) -> Vec<u8> {
    serde_json::to_vec(state).unwrap()
}

/// Actual snapshots from two different prefixes make accepting a bad snapshot
/// observably wrong. Every refusal row must replay the whole log, while the
/// valid row must skip its prefix and preserve both families and their tail.
#[tokio::test]
async fn snapshot_boot_agrees_with_full_replay_and_discards_each_bad_snapshot() {
    let log = InMemoryLog::default();
    prefix(&log, "homer").await;
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let good = log.load_snapshot().await.unwrap().unwrap();
    let original = log.validity().await.unwrap();
    tail(&log).await;
    log.replace_snapshot(None);
    let no_snapshot = ReadLog {
        log: log.clone(),
        ..ReadLog::default()
    };
    let full = boot(&no_snapshot).await.unwrap();
    assert_eq!(
        no_snapshot.positions.lock().unwrap().first(),
        Some(&Position(0))
    );
    assert!(
        !full
            .state
            .teaching
            .has_contact(&RunId("homer".into()), "claims")
    );
    assert!(
        full.state
            .teaching
            .has_contact(&RunId("marge".into()), "mail")
    );
    assert_eq!(
        full.state
            .ledger
            .request(&request().key)
            .unwrap()
            .committed
            .as_ref()
            .unwrap()
            .receipt
            .as_deref(),
        Some("beta")
    );
    let expected = projection(&full.state);
    let wire: serde_json::Value = serde_json::from_slice(&good.blob).unwrap();
    assert!(
        wire["ledger"].get("history").is_none(),
        "the ledger's old event copy is not a projection"
    );

    log.replace_snapshot(Some(good.clone()));
    let read = ReadLog {
        log: log.clone(),
        ..ReadLog::default()
    };
    let from_snapshot = boot(&read).await.unwrap();
    assert_eq!(projection(&from_snapshot.state), expected);
    assert_eq!(from_snapshot.position, full.position);
    assert_eq!(
        read.positions.lock().unwrap()[..2],
        [Position(good.covered_position.0 - 1), good.covered_position]
    );
    assert!(
        !read.positions.lock().unwrap().contains(&Position(0)),
        "a valid snapshot skips the prefix"
    );

    let alternate = InMemoryLog::default();
    prefix(&alternate, "bart").await;
    take_snapshot(&alternate, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let alternate = alternate.load_snapshot().await.unwrap().unwrap();
    let mut different = good.clone();
    different.blob = alternate.blob;
    different.serialized_length = different.blob.len() as u64;
    different.checksum = Sha256::digest(&different.blob).into();

    for row in [
        "format",
        "projection",
        "identity",
        "representation",
        "erasure",
        "future_erasure",
        "checksum",
        "length",
        "boundary",
        "batch",
        "decode",
    ] {
        let mut bad = different.clone();
        let mut validity = original.clone();
        match row {
            "format" => bad.snapshot_format_version += 1,
            "projection" => bad.projection_version = bad.projection_version.wrapping_add(1),
            "identity" => bad.log_identity = "another-log".into(),
            "representation" => bad.representation_epoch += 1,
            "erasure" => {
                validity.erasure_epoch += 1;
                validity.earliest_affected_position = Some(good.covered_position);
            }
            "future_erasure" => bad.erasure_epoch += 1,
            "checksum" => bad.checksum[0] ^= 1,
            "length" => bad.serialized_length += 1,
            "boundary" => bad.covered_position.0 -= 1,
            "batch" => bad.batch_checksum[0] ^= 1,
            "decode" => {
                bad.blob = b"{".to_vec();
                bad.serialized_length = 1;
                bad.checksum = Sha256::digest(&bad.blob).into();
            }
            _ => unreachable!(),
        }
        log.set_validity(validity);
        log.replace_snapshot(Some(bad));
        let read = ReadLog {
            log: log.clone(),
            ..ReadLog::default()
        };
        let restored = boot(&read).await.unwrap();
        assert_eq!(projection(&restored.state), expected, "discard row {row}");
        assert!(
            read.positions.lock().unwrap().contains(&Position(0)),
            "discard row {row} must replay"
        );
    }

    // A snapshot strictly before the erasure's first affected position survives.
    let mut after = original.clone();
    after.erasure_epoch = 1;
    after.earliest_affected_position = Some(Position(good.covered_position.0 + 1));
    log.set_validity(after);
    log.replace_snapshot(Some(good.clone()));
    let read = ReadLog {
        log: log.clone(),
        ..ReadLog::default()
    };
    assert_eq!(projection(&boot(&read).await.unwrap().state), expected);
    assert!(!read.positions.lock().unwrap().contains(&Position(0)));
    assert_eq!(
        log.store_snapshot(&good).await,
        Err(LogError::Conflict),
        "an older serializer cannot publish"
    );
}

#[tokio::test]
async fn snapshot_bytes_are_deterministic_and_an_empty_log_round_trips() {
    let log = InMemoryLog::default();
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    assert_eq!(boot(&log).await.unwrap().position, Position(0));
    prefix(&log, "homer").await;
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let first = log.load_snapshot().await.unwrap().unwrap();
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let second = log.load_snapshot().await.unwrap().unwrap();
    assert_eq!(first, second);

    let mut projections = Vec::new();
    for reverse in [false, true] {
        let mut state = State::default();
        let mut keys: Vec<_> = (0..16).collect();
        if reverse {
            keys.reverse();
        }
        for key in keys {
            let mut request = request();
            request.key.id = RequestId(format!("item-{key}"));
            state
                .apply(&Member::Ledger(LedgerEvent::RequestCommitted(Commit {
                    request,
                    landed: "alpha".into(),
                    receipt: None,
                    written_at: jiff::Timestamp::UNIX_EPOCH,
                    expires_at: jiff::Timestamp::from_second(10_000).unwrap(),
                })))
                .unwrap();
        }
        projections.push(projection(&state));
    }
    assert_eq!(
        projections[0], projections[1],
        "map insertion and hash iteration do not change the bytes"
    );
}

/// The same snapshot path stores and restores through the real writer/log port.
#[tokio::test]
async fn snapshot_boot_restores_both_families_from_the_real_store() {
    use crate::dolt::Dolt;
    use crate::testing::{Scratch, free_port, migrated_database};
    use crate::writer::Writer;
    use crate::writer_log::WriterLog;

    let scratch = Scratch::new("snapshot-boot");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let writer = Writer::default();
    let log = WriterLog::open(pool.clone(), writer.clone()).await.unwrap();
    prefix(&log, "homer").await;
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    tail(&log).await;
    let reopened = WriterLog::open(pool, writer.clone()).await.unwrap();
    let restored = boot(&reopened).await.unwrap();
    let mut full = State::default();
    for batch in reopened.read_from(Position(0), 256).await.unwrap() {
        apply(&mut full, &batch).unwrap();
    }
    assert_eq!(projection(&restored.state), projection(&full));
    assert!(
        restored
            .state
            .teaching
            .has_contact(&RunId("marge".into()), "mail")
    );
    assert_eq!(
        restored
            .state
            .ledger
            .request(&request().key)
            .unwrap()
            .committed
            .as_ref()
            .unwrap()
            .receipt
            .as_deref(),
        Some("beta")
    );
    drop(reopened);
    drop(log);
    writer.drain().await;
    server.stop().await;
}
