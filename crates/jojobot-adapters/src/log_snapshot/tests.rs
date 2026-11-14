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
    let _counting = FINGERPRINT_SKIPS.lock().await;
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
    let mut full = Boot::empty();
    for batch in reopened.read_from(Position(0), 256).await.unwrap() {
        apply(&mut full, &batch).unwrap();
    }
    assert_eq!(projection(&restored.state), projection(&full.state));
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

/// What `restored` says of the snapshot the log holds now: none when it is used.
async fn reason_of(log: &InMemoryLog) -> Option<Skip> {
    let validity = log.validity().await.unwrap();
    restored(log, &validity).await.unwrap().err()
}

/// **One defect, one named reason.** The control is a good snapshot and a good
/// validity; each row breaks exactly one thing, and the function that decides says
/// which. A row that is not a defect (an erasure after the snapshot's last
/// position) names nothing.
#[tokio::test]
async fn each_defect_of_a_snapshot_has_its_own_named_reason() {
    let log = InMemoryLog::default();
    prefix(&log, "homer").await;
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let good = log.load_snapshot().await.unwrap().unwrap();
    let validity = log.validity().await.unwrap();
    assert_eq!(refusal(&good, &validity), None, "the control is usable");

    type Break = fn(&mut ValidatedSnapshot, &mut Validity);
    //   (the row, what it breaks, the reason named)
    let table: [(&str, Break, Option<Skip>); 8] = [
        (
            "format",
            |snapshot, _| snapshot.snapshot_format_version += 1,
            Some(Skip::Format),
        ),
        (
            "fingerprint",
            |snapshot, _| snapshot.projection_version ^= 1,
            Some(Skip::Fingerprint),
        ),
        (
            "identity",
            |snapshot, _| snapshot.log_identity.push('x'),
            Some(Skip::Identity),
        ),
        (
            "epoch",
            |_, validity| validity.representation_epoch += 1,
            Some(Skip::Epoch),
        ),
        (
            "erasure over the snapshot",
            |_, validity| {
                validity.erasure_epoch += 1;
                validity.earliest_affected_position = Some(Position(0));
            },
            Some(Skip::Erasure),
        ),
        (
            "erasure after the snapshot",
            |snapshot, validity| {
                validity.erasure_epoch += 1;
                validity.earliest_affected_position =
                    Some(Position(snapshot.covered_position.0 + 1));
            },
            None,
        ),
        (
            "length",
            |snapshot, _| snapshot.serialized_length += 1,
            Some(Skip::Length),
        ),
        (
            "checksum",
            |snapshot, _| snapshot.blob[0] ^= 1,
            Some(Skip::Checksum),
        ),
    ];
    for (row, broken, reason) in table {
        let (mut snapshot, mut validity) = (good.clone(), validity.clone());
        broken(&mut snapshot, &mut validity);
        assert_eq!(refusal(&snapshot, &validity), reason, "{row}");
    }

    // The three reasons that need the log: none held, a snapshot not tied to a
    // batch, and a blob that is not this build's State.
    log.replace_snapshot(None);
    assert_eq!(reason_of(&log).await, Some(Skip::NoneHeld));
    let mut untied = good.clone();
    untied.batch_checksum[0] ^= 1;
    log.replace_snapshot(Some(untied));
    assert_eq!(reason_of(&log).await, Some(Skip::Boundary));
    let mut unreadable = good.clone();
    unreadable.blob = b"not a state".to_vec();
    unreadable.serialized_length = unreadable.blob.len() as u64;
    unreadable.checksum = Sha256::digest(&unreadable.blob).into();
    log.replace_snapshot(Some(unreadable));
    assert_eq!(reason_of(&log).await, Some(Skip::Unreadable));
    // A blob that parses as this build's State but keeps the position zero, which no batch
    // holds, is unreadable too.
    fn zero_first(value: &mut serde_json::Value) -> bool {
        match value {
            serde_json::Value::Object(fields) => {
                if fields
                    .get("position")
                    .is_some_and(serde_json::Value::is_u64)
                {
                    fields.insert("position".into(), serde_json::json!(0));
                    return true;
                }
                fields.values_mut().any(zero_first)
            }
            serde_json::Value::Array(items) => items.iter_mut().any(zero_first),
            _ => false,
        }
    }
    use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
    let claiming = InMemoryLog::default();
    append(
        &claiming,
        "claims",
        vec![crate::claims_log::codec::encode(
            &jojobot_domain::log::Event {
                event_kind: String::new(),
                meaning_version: 1,
                actor: None,
                run: None,
                session_card: None,
                request: None,
                part: None,
                occurred_at: None,
                written_at: None,
                imported_source_time: None,
                payload: ClaimEvent::ObjectCreated {
                    object: ObjectId("k1".into()),
                    body: ObjectBody {
                        kind: KindId("person".into()),
                        parent: None,
                        name: "person:homer".into(),
                        source: "operator".into(),
                        aliases: Vec::new(),
                        boot: None,
                        metadata: Default::default(),
                        archived: false,
                        bound_to: None,
                    },
                },
            },
        )],
    )
    .await;
    take_snapshot(&claiming, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let good = claiming.load_snapshot().await.unwrap().unwrap();
    let mut parsed: serde_json::Value = serde_json::from_slice(&good.blob).unwrap();
    assert!(
        zero_first(&mut parsed["claims"]),
        "the control State keeps a claims position to zero"
    );
    let mut zeroed = good.clone();
    zeroed.blob = serde_json::to_vec(&parsed).unwrap();
    zeroed.serialized_length = zeroed.blob.len() as u64;
    zeroed.checksum = Sha256::digest(&zeroed.blob).into();
    claiming.replace_snapshot(Some(zeroed));
    assert_eq!(reason_of(&claiming).await, Some(Skip::Unreadable));
}

/// **A boot that skips a snapshot counts the skip under its reason.** The count is
/// the one /metrics serves, so a deploy that replays from zero is seen.
#[tokio::test]
async fn a_boot_that_skips_a_snapshot_counts_it_by_reason() {
    let _counting = FINGERPRINT_SKIPS.lock().await;
    let count = |name: &str| {
        skip_counts()
            .into_iter()
            .find(|(each, _)| *each == name)
            .map(|(_, count)| count)
            .expect("every reason has a count")
    };
    let log = InMemoryLog::default();
    prefix(&log, "homer").await;
    take_snapshot(&log, jiff::Timestamp::UNIX_EPOCH)
        .await
        .unwrap();
    let mut stale = log.load_snapshot().await.unwrap().unwrap();
    stale.projection_version ^= 1;
    log.replace_snapshot(Some(stale));
    let before = count("fingerprint");
    boot(&log).await.unwrap();
    assert_eq!(
        count("fingerprint"),
        before + 1,
        "a fingerprint skip is counted once"
    );
}

/// **The ten reasons are served under these names.** The boot line and the
/// `reason` label of `jojobot_snapshot_skips` carry them, and a scraper, a
/// dashboard or an alert reads them from outside this process, so the spellings
/// are written out here and not taken from the list they are checked against.
#[test]
fn the_skip_reasons_are_served_under_these_ten_names() {
    let served: Vec<&str> = Skip::ALL.iter().map(|skip| skip.name()).collect();
    assert_eq!(
        served,
        [
            "none_held",
            "format",
            "fingerprint",
            "identity",
            "epoch",
            "erasure",
            "length",
            "checksum",
            "boundary",
            "unreadable",
        ]
    );
}

/// **Each reason counts at its own place in the list**, so the count a scrape
/// serves under a name is the count of that reason and of no other.
#[test]
fn every_reason_counts_at_its_own_place_in_the_list() {
    for (at, skip) in Skip::ALL.iter().enumerate() {
        assert_eq!(skip.index(), at, "{}", skip.name());
    }
}
