//! A claims event is staged only into a log that holds a completed claims
//! import: before it, the seam refuses the write and the log is unchanged.

use jiff::Timestamp;
use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{Event, ObjectId, Position, RequestId, RunId};
use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};

use super::{CLAIMS_IMPORT_PREFIX, Seam};
use crate::dolt::Dolt;
use crate::ledger_log::LedgerError;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn spent() -> LedgerEvent {
    LedgerEvent::RequestCommitted(Commit {
        request: Request {
            key: RequestKey {
                actor: ObjectId("bot:homer".into()),
                id: RequestId("alpha".into()),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest-alpha".into(),
        },
        landed: "{\"address\":\"topic:alpha#f1\"}".into(),
        receipt: None,
        written_at: at(0),
        expires_at: at(10),
    })
}

/// A thing created and given a handle, as two claims events.
fn thing(id: &str, handle: &str) -> Vec<Event<ClaimEvent>> {
    let envelope = |payload| Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(at(0)),
        imported_source_time: None,
        payload,
    };
    vec![
        envelope(ClaimEvent::ObjectCreated {
            object: ObjectId(id.into()),
            body: ObjectBody {
                kind: KindId("person".into()),
                parent: None,
                name: handle.into(),
                source: "operator".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        envelope(ClaimEvent::AddressBound {
            object: ObjectId(id.into()),
            address: handle.into(),
        }),
    ]
}

async fn opened(writer: &Writer, pool: &sql::Pool) -> std::sync::Arc<Seam> {
    let (opener, pool) = (writer.clone(), pool.clone());
    writer
        .run(async move { opener.ensure_seam(&pool).await })
        .await
        .expect("the seam opens")
}

/// Stage in a transaction of its own and commit; the outcome of the staging is
/// what the caller reads.
async fn staged(
    writer: &Writer,
    pool: &sql::Pool,
    seam: &std::sync::Arc<Seam>,
    events: Vec<LedgerEvent>,
    claims: Vec<Event<ClaimEvent>>,
    import: bool,
) -> Result<(), LedgerError> {
    let (seam, pool) = (seam.clone(), pool.clone());
    writer
        .run(async move {
            let mut tx = pool.writer().begin().await.unwrap();
            let pending = match import {
                true => seam.stage_import(&mut tx, claims, at(1)).await,
                false => seam.stage_with(&mut tx, events, claims, at(1)).await,
            }?;
            sql::commit(tx).await.unwrap();
            pending.committed();
            Ok(())
        })
        .await
}

/// Both durable entrances refuse before staging: rolling the transaction back
/// is not what protects the log from a malformed claims batch.
#[tokio::test]
async fn canonical_handle_seam_refuses_duplicates_before_staging_import_or_live_batches() {
    let scratch = Scratch::new("seam-handle-admission");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;
    let mut duplicate = thing("k1", "person:homer");
    duplicate.push(thing("k1", "person:marge").pop().unwrap());
    for import in [true, false] {
        if !import {
            staged(
                &writer,
                &pool,
                &seam,
                vec![],
                thing("k2", "person:lisa"),
                true,
            )
            .await
            .unwrap();
        }
        let log = writer.shared_log(&store);
        let before = log.read_from(Position(0), 100).await.unwrap();
        let (staging, pool, duplicate) = (seam.clone(), pool.clone(), duplicate.clone());
        let result = writer
            .run(async move {
                let mut tx = pool.writer().begin().await.unwrap();
                let result = if import {
                    staging.stage_import(&mut tx, duplicate, at(1)).await
                } else {
                    staging
                        .stage_with(&mut tx, vec![spent()], duplicate, at(1))
                        .await
                };
                // Commit even on refusal: admission itself must have staged nothing.
                sql::commit(tx).await.unwrap();
                result.map(|pending| pending.committed())
            })
            .await;
        assert!(matches!(result, Err(LedgerError::Refused(_))), "{result:?}");
        assert_eq!(log.read_from(Position(0), 100).await.unwrap(), before);
        assert!(seam.read_claims(|state| state.resolve("person:homer").is_none()));
    }
    // A real retire/bind rename is admitted by the same entrances.
    staged(
        &writer,
        &pool,
        &seam,
        vec![],
        thing("k1", "person:homer"),
        false,
    )
    .await
    .unwrap();
    let mut rename = thing("k1", "person:marge");
    rename[0].payload = ClaimEvent::AddressRetired {
        object: ObjectId("k1".into()),
        address: "person:homer".into(),
        day: jiff::civil::date(2026, 11, 5),
    };
    staged(&writer, &pool, &seam, vec![], rename, false)
        .await
        .unwrap();
    assert_eq!(
        seam.read_claims(|state| state.handle(&ObjectId("k1".into())).unwrap().0),
        "person:marge"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A retirement of a spelling the object does not hold is refused before staging**, at the
/// live entrance: the spelling is another object's, or nobody's. The transaction commits even on
/// the refusal, so a refusal that had staged anything first would leave it in the durable log.
#[tokio::test]
async fn canonical_handle_seam_refuses_retirement_of_an_unheld_spelling() {
    let scratch = Scratch::new("seam-unheld-retirement");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;
    staged(
        &writer,
        &pool,
        &seam,
        vec![],
        thing("k1", "person:homer"),
        true,
    )
    .await
    .unwrap();
    let log = writer.shared_log(&store);
    let before = log.read_from(Position(0), 100).await.unwrap();
    for (object, address) in [("k1", "person:marge"), ("k2", "person:homer")] {
        let retire = vec![Event {
            payload: ClaimEvent::AddressRetired {
                object: ObjectId(object.into()),
                address: address.into(),
                day: jiff::civil::date(2026, 11, 5),
            },
            ..thing("k1", "person:homer").remove(0)
        }];
        let (staging, pool, retire) = (seam.clone(), pool.clone(), retire);
        let result = writer
            .run(async move {
                let mut tx = pool.writer().begin().await.unwrap();
                let result = staging
                    .stage_with(&mut tx, vec![spent()], retire, at(1))
                    .await;
                // Commit even on refusal: admission itself must have staged nothing.
                sql::commit(tx).await.unwrap();
                result.map(|pending| pending.committed())
            })
            .await;
        assert!(
            matches!(result, Err(LedgerError::Refused(_))),
            "an unheld retirement of {address} for {object} must be refused: {result:?}"
        );
        assert_eq!(log.read_from(Position(0), 100).await.unwrap(), before);
    }
    writer.drain().await;
    server.stop().await;
}

/// **A restart replays to the same State.** What the first seam held after its live writes, a
/// record and a field write among them, is what a second writer's seam reads back from the log
/// alone; a field the codec dropped would show as a difference here.
#[tokio::test]
async fn a_restarted_seam_holds_the_state_the_first_one_held() {
    use jojobot_domain::log::RecordId;
    use jojobot_domain::log::claims::{ClaimBody, FieldValue, KeyId};
    use jojobot_domain::memory::{FactStatus, Provenance, Standing, WriteClass};
    let scratch = Scratch::new("seam-restart");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let first = Writer::default();
    let seam = opened(&first, &pool).await;
    staged(
        &first,
        &pool,
        &seam,
        vec![],
        thing("k2", "person:lisa"),
        true,
    )
    .await
    .unwrap();
    let envelope = thing("k1", "person:homer").remove(0);
    let mut live = thing("k1", "person:homer");
    live.push(Event {
        payload: ClaimEvent::RecordCreated {
            record: RecordId("r1".into()),
            claim: ClaimBody {
                label: "f1".into(),
                home: ObjectId("k1".into()),
                content: "likes donuts".into(),
                details: None,
                provenance: Provenance::Testimony,
                standing: Standing::Settled,
                status: FactStatus::Active,
                recorded_at: jiff::civil::date(2026, 11, 5),
                happened_at: None,
                happened_through: None,
                stale_after: None,
                edge: None,
                derived_from: None,
                refs: Vec::new(),
                stands_for: Vec::new(),
                receipt: None,
                app: None,
                inserted_at: None,
            },
        },
        ..envelope.clone()
    });
    live.push(Event {
        payload: ClaimEvent::FieldWritten {
            record: RecordId("r1".into()),
            subject: ObjectId("k1".into()),
            key: KeyId("mood".into()),
            ordinal: 1,
            class: WriteClass::Sets,
            write: FieldValue::Set("calm".into()),
            targets: Vec::new(),
        },
        ..envelope
    });
    staged(&first, &pool, &seam, vec![], live, false)
        .await
        .unwrap();
    let held = seam.read_claims(Clone::clone);
    assert!(
        held.resolve("person:homer").is_some() && held.resolve("person:lisa").is_some(),
        "the first seam holds what was written"
    );
    first.drain().await;
    drop(seam);
    drop(first);

    // **An oracle the seam does not hold**: the replay of the durable batches, which applies
    // every claims member through the domain State alone. It reads before the restarted writer
    // opens, because the log admits one writer at a time. A seam that skipped a record or a
    // field write on commit and on replay alike would hold equal, empty States and pass the
    // comparison with the first seam below.
    let oracle = Writer::default();
    let reading = crate::writer_log::WriterLog::open(store.clone(), oracle.clone())
        .await
        .unwrap();
    let independent = crate::claims_log::replayed_from(&reading).await;
    drop(reading);
    oracle.drain().await;
    drop(oracle);

    let second = Writer::default();
    let restarted = opened(&second, &pool).await;
    let restarted_state = restarted.read_claims(Clone::clone);
    assert_eq!(
        restarted_state, held,
        "a restart replays to the State the first seam held"
    );
    assert_eq!(
        restarted_state, independent,
        "the restarted State is the one an independent replay of the log builds"
    );
    let mood_writes = restarted_state.key_trails(
        restarted_state
            .resolve("person:homer")
            .expect("the thing a live write bound resolves"),
    );
    assert_eq!(
        mood_writes.history.get("mood").map(Vec::len),
        Some(1),
        "the field write reaches the replayed State"
    );
    second.drain().await;
    server.stop().await;
}

/// **The admission rule's positive controls.** A check that refused too much would still pass
/// the refusals above, so what it must admit is stated beside them: the same handle bound again,
/// a numbered alias, and a rename (a retirement then a new binding). The State it runs on is
/// applied between checks as the seam applies it.
#[test]
fn canonical_handle_check_admits_the_same_handle_an_alias_and_a_rename() {
    use jojobot_domain::log::claims::State;
    let mut state = State::default();
    let created = thing("k1", "person:homer");
    let bound = |address: &str| Event {
        payload: ClaimEvent::AddressBound {
            object: ObjectId("k1".into()),
            address: address.into(),
        },
        ..created[0].clone()
    };
    let admitted = [
        created[0].clone(),
        created[1].clone(),
        created[1].clone(),
        bound("person:homer/1"),
    ];
    state
        .check(&admitted)
        .expect("the same handle again and a numbered alias are admitted");
    for event in &admitted {
        let _ = state.apply(event);
    }
    let duplicate = bound("person:marge");
    assert!(
        state.check(std::slice::from_ref(&duplicate)).is_err(),
        "a second live handle is still refused"
    );
    let rename = [
        Event {
            payload: ClaimEvent::AddressRetired {
                object: ObjectId("k1".into()),
                address: "person:homer".into(),
                day: jiff::civil::date(2026, 11, 5),
            },
            ..created[0].clone()
        },
        duplicate,
    ];
    state
        .check(&rename)
        .expect("a retirement then a binding is a rename");
}

/// **Before a completed import the seam stages no claims event**, whatever else
/// the write carries, and the log is unchanged. A write of the ledgers alone goes
/// on. The import's own batch is staged, and after it the same write is staged.
#[tokio::test]
async fn a_claims_event_waits_for_a_completed_import() {
    let scratch = Scratch::new("seam-import-gate");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;

    let refused = staged(
        &writer,
        &pool,
        &seam,
        vec![spent()],
        thing("k1", "person:homer"),
        false,
    )
    .await;
    assert!(
        matches!(refused, Err(LedgerError::Refused(_))),
        "{refused:?}"
    );
    assert!(
        writer
            .shared_log(&store)
            .read_from(Position(0), 10)
            .await
            .unwrap()
            .is_empty(),
        "a refused write leaves the log as it was"
    );

    staged(&writer, &pool, &seam, vec![spent()], Vec::new(), false)
        .await
        .expect("a write of the ledgers alone is staged");

    staged(
        &writer,
        &pool,
        &seam,
        Vec::new(),
        thing("k1", "person:homer"),
        true,
    )
    .await
    .expect("the import is staged");
    staged(
        &writer,
        &pool,
        &seam,
        Vec::new(),
        thing("k2", "person:marge"),
        false,
    )
    .await
    .expect("a claims event is staged once the log holds the import");

    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    let imports = batches
        .iter()
        .filter(|batch| batch.header().batch_id.0.starts_with(CLAIMS_IMPORT_PREFIX))
        .count();
    assert_eq!(imports, 1, "the import is one batch of the log");
    writer.drain().await;
    server.stop().await;
}

/// **A seam that opens over a log holding the import stages claims events at
/// once**: the import is read from the log, not remembered by the process that
/// wrote it.
#[tokio::test]
async fn a_seam_opened_over_a_completed_import_stages_claims_events() {
    let scratch = Scratch::new("seam-import-reopened");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    {
        let writer = Writer::default();
        let seam = opened(&writer, &pool).await;
        staged(
            &writer,
            &pool,
            &seam,
            Vec::new(),
            thing("k1", "person:homer"),
            true,
        )
        .await
        .expect("the import is staged");
        writer.drain().await;
    }

    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;
    staged(
        &writer,
        &pool,
        &seam,
        Vec::new(),
        thing("k2", "person:marge"),
        false,
    )
    .await
    .expect("a claims event is staged over a log that holds the import");
    writer.drain().await;
    server.stop().await;
}

fn large_import() -> Vec<Event<ClaimEvent>> {
    let mut events = thing("k1", "person:homer");
    for ordinal in 1..=300 {
        let mut event = events[0].clone();
        event.payload = ClaimEvent::ObjectWriteObserved {
            object: ObjectId("k1".into()),
            filed_under: "person:homer".into(),
            ordinal,
            snapshot: None,
        };
        events.push(event);
    }
    events.push(jojobot_domain::log::claims::import::import_completed(
        events.len(),
    ));
    events
}

/// The import's batches preserve event order and contiguous positions. Neither
/// the log reader nor the seam sees them before their one transaction commits.
#[tokio::test]
async fn a_claims_import_uses_bounded_batches_in_one_transaction() {
    let scratch = Scratch::new("seam-import-batches");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;
    let claims = large_import();
    let expected: Vec<_> = claims
        .iter()
        .map(crate::claims_log::codec::encode)
        .collect();
    let (staging, job_pool, reader) = (seam.clone(), pool.clone(), store.clone());
    writer
        .run(async move {
            let mut tx = job_pool.writer().begin().await.unwrap();
            let pending = staging.stage_import(&mut tx, claims, at(1)).await.unwrap();
            let visible: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM event_log WHERE first_position IS NOT NULL",
            )
            .fetch_one(&reader)
            .await
            .unwrap();
            assert_eq!(visible, 0, "uncommitted import batches are not visible");
            assert!(!staging.lock().claims_import);
            assert!(staging.read_claims(|state| state.resolve("person:homer").is_none()));
            sql::commit(tx).await.unwrap();
            pending.committed();
        })
        .await;
    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 100)
        .await
        .unwrap();
    assert_eq!(batches.len(), 3, "303 events occupy three bounded batches");
    let mut previous = Position(0);
    for (index, batch) in batches.iter().enumerate() {
        assert!(
            batch.members().len() <= 128,
            "one batch exceeds the measured bound"
        );
        assert_eq!(batch.header().expected_previous_position, previous);
        assert_eq!(batch.range().first.0, previous.0 + 1);
        assert_eq!(
            batch.header().batch_id.0,
            format!("{CLAIMS_IMPORT_PREFIX}{}:{index}", previous.0)
        );
        previous = batch.range().last;
    }
    assert_eq!(previous.0, expected.len() as u64);
    assert_eq!(
        batches
            .iter()
            .flat_map(|batch| batch.members().iter())
            .collect::<Vec<_>>(),
        expected.iter().collect::<Vec<_>>()
    );
    assert!(seam.read_claims(|state| state.resolve("person:homer").is_some()));
    let refused = staged(&writer, &pool, &seam, vec![], large_import(), true).await;
    assert!(
        matches!(refused, Err(LedgerError::Refused(_))),
        "{refused:?}"
    );
    assert_eq!(
        writer
            .shared_log(&store)
            .read_from(Position(0), 100)
            .await
            .unwrap(),
        batches
    );
    writer.drain().await;
    drop(seam);
    drop(writer);
    let reopened = Writer::default();
    let seam = opened(&reopened, &pool).await;
    assert!(seam.lock().claims_import);
    assert!(seam.read_claims(|state| state.resolve("person:homer").is_some()));
    reopened.drain().await;
    server.stop().await;
}

/// A duplicate slot makes the second INSERT fail after the first succeeded.
/// Rolling back leaves no claims event, and a retry imports the whole history.
#[tokio::test]
async fn a_claims_import_fault_between_batches_leaves_no_claims_event() {
    let scratch = Scratch::new("seam-import-batch-fault");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = opened(&writer, &pool).await;
    let blocked = format!(
        "b:{}",
        jojobot_domain::request::digest_of("claims-import:128:1")
    );
    sqlx::query("INSERT INTO event_log (slot, first_position, last_position, payload) VALUES (?, NULL, 0, ?)")
        .bind(&blocked).bind(b"[]".as_slice()).execute(&store).await.unwrap();
    let (staging, job_pool) = (seam.clone(), pool.clone());
    writer
        .run(async move {
            let mut tx = job_pool.writer().begin().await.unwrap();
            let result = staging.stage_import(&mut tx, large_import(), at(1)).await;
            assert!(
                result.is_err(),
                "the second batch must meet the duplicate slot"
            );
            let staged: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM event_log WHERE first_position IS NOT NULL",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(staged, 1, "the fault follows a successful first batch");
            tx.rollback().await.unwrap();
        })
        .await;
    assert!(
        writer
            .shared_log(&store)
            .read_from(Position(0), 100)
            .await
            .unwrap()
            .is_empty()
    );
    assert!(!seam.lock().claims_import);
    assert!(seam.read_claims(|state| state.resolve("person:homer").is_none()));
    sqlx::query("DELETE FROM event_log WHERE slot = ?")
        .bind(&blocked)
        .execute(&store)
        .await
        .unwrap();
    staged(&writer, &pool, &seam, vec![], large_import(), true)
        .await
        .unwrap();
    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 100)
        .await
        .unwrap();
    assert_eq!(batches.len(), 3);
    assert_eq!(batches.last().unwrap().range().last.0, 303);
    assert!(seam.read_claims(|state| state.resolve("person:homer").is_some()));
    writer.drain().await;
    server.stop().await;
}

/// **The seam knows the import from its event and not only from the id of its
/// batch.** The log here holds an `ImportCompleted` event in a batch of another
/// id, as a state read from a snapshot that covers the import would; a seam opened
/// over it stages a claims event.
#[tokio::test]
async fn the_seam_knows_the_import_from_its_event() {
    let scratch = Scratch::new("seam-import-event");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let seeding = Writer::default();
    let seam = opened(&seeding, &pool).await;
    {
        let (seam, pool) = (seam.clone(), pool.clone());
        seeding
            .run(async move {
                let mut tx = pool.writer().begin().await.unwrap();
                let head = seam.log.head_in(&mut tx).await.unwrap();
                let marker = jojobot_domain::log::claims::import::import_completed(0);
                let batch = Seam::seal(head, &[], &[marker], at(0), None).unwrap();
                let staged = seam
                    .log
                    .stage(&mut tx, Position(head), &batch)
                    .await
                    .unwrap();
                sql::commit(tx).await.unwrap();
                staged.committed();
            })
            .await;
    }
    // One writer holds the store's fence at a time, so the first lets it go.
    seeding.drain().await;
    drop(seam);
    drop(seeding);
    // A seam of its own replays that log: no batch carries the import's id.
    let reading = Writer::default();
    let reopened = opened(&reading, &pool).await;
    assert!(reopened.read_claims(|state| state.import_completed()));
    let staged_over_it = staged(
        &reading,
        &pool,
        &reopened,
        Vec::new(),
        thing("k1", "person:homer"),
        false,
    )
    .await;
    assert!(
        staged_over_it.is_ok(),
        "a claims event is staged over a log that holds the import's event: {staged_over_it:?}"
    );
    reading.drain().await;
    server.stop().await;
}
