//! The capture job on a real store: what it admits is sealed with the commit
//! and answered by the claims State, what it refuses leaves nothing, and a
//! measure is taken on the State the job finds, not on one before it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_domain::actor::Actor;
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::job::{CaptureJob, within};
use jojobot_domain::log::claims::{
    CaptureRequest, ClaimEvent, ClaimSize, Declined, Door, Draft, KindId, Measures, ObjectBody,
    Route,
};
use jojobot_domain::log::{Event, ObjectId, Position, RecordId};
use jojobot_domain::memory::{
    EntityId, FactStatus, Guarded, Memory, NewFact, Provenance, Standing,
};

use super::{Admission, admit};
use crate::dolt::Dolt;
use crate::dolt::requests;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn at() -> Timestamp {
    Timestamp::from_second(1_700_000_000).expect("a moment")
}

fn envelope(payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: None,
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(at()),
        imported_source_time: None,
        payload,
    }
}

fn homer() -> ObjectId {
    ObjectId("k1".into())
}

fn seeded() -> Vec<Event<ClaimEvent>> {
    vec![
        envelope(ClaimEvent::ObjectCreated {
            object: homer(),
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
        }),
        envelope(ClaimEvent::AddressBound {
            object: homer(),
            address: "person:homer".into(),
        }),
    ]
}

fn request(subject: &str, record: &str) -> CaptureRequest {
    CaptureRequest {
        record: RecordId(record.into()),
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
        subject: subject.into(),
        draft: Draft {
            content: "a claim".into(),
            details: None,
            provenance: Provenance::Testimony,
            standing: Standing::Settled,
            status: FactStatus::Active,
            recorded_at: date(2026, 4, 19),
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

fn fits() -> Measures {
    Measures {
        claim: Some(ClaimSize {
            size: 1,
            room: 100,
            row: None,
        }),
        ..Measures::default()
    }
}

/// A store whose log holds homer, read by a writer that opens after the seed.
async fn world(name: &str) -> (Scratch, Dolt, sqlx::MySqlPool, sql::Pool, Writer) {
    jojobot_domain::memory::kinds::load_shipped();
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let seeding = Writer::default();
    {
        let (opener, pool) = (seeding.clone(), pool.clone());
        seeding
            .run(async move {
                let seam = opener.ensure_seam(&pool).await.unwrap();
                let mut tx = pool.writer().begin().await.unwrap();
                let pending = seam
                    .stage_with(&mut tx, Vec::new(), seeded(), at())
                    .await
                    .unwrap();
                sql::commit(tx).await.unwrap();
                pending.committed();
            })
            .await;
    }
    seeding.drain().await;
    drop(seeding);
    (scratch, server, store, pool, Writer::default())
}

async fn capture_once(
    writer: &Writer,
    pool: &sql::Pool,
    subject: &'static str,
    record: &'static str,
    saw: Arc<Mutex<Vec<bool>>>,
) -> Result<Admission, Declined> {
    let pool = pool.clone();
    writer
        .run(async move {
            let tx = pool.begin().await.unwrap();
            let seam = crate::writer::Writer::current()
                .and_then(|writer| writer.seam())
                .expect("begin opened the seam");
            let decided = admit(
                &seam,
                Box::new(move |_, _| Ok(request(subject, record))),
                Box::new(move |state, _| {
                    saw.lock()
                        .unwrap()
                        .push(state.record(&RecordId("r1".into())).is_some());
                    fits()
                }),
                at(),
            )
            .await
            .unwrap();
            requests::commit(tx, &Clock::default(), || "{}".into())
                .await
                .unwrap();
            decided
        })
        .await
}

/// **An admitted capture is sealed with the commit and the State answers it.**
#[tokio::test]
async fn an_admitted_capture_is_sealed_and_the_state_answers_it() {
    let (_scratch, mut server, store, pool, writer) = world("capture-job-admitted").await;
    let saw = Arc::new(Mutex::new(Vec::new()));
    let admitted = capture_once(&writer, &pool, "person:homer", "r1", saw)
        .await
        .expect("the capture is admitted");
    assert!(!admitted.events.is_empty());

    let batches = writer
        .shared_log(&store)
        .read_from(Position(2), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 1, "one batch for the one transaction");
    assert_eq!(batches[0].members().len(), admitted.events.len());
    let seam = writer.seam().expect("the seam is open");
    assert!(
        seam.read_claims(|state| state.record(&RecordId("r1".into())).is_some()),
        "the claims State answers the record the commit sealed"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A refused capture leaves nothing**: no event in the log, no record held.
#[tokio::test]
async fn a_refused_capture_leaves_nothing() {
    let (_scratch, mut server, store, pool, writer) = world("capture-job-refused").await;
    let saw = Arc::new(Mutex::new(Vec::new()));
    let refused = capture_once(&writer, &pool, "person:marge", "r1", saw)
        .await
        .err()
        .expect("an unknown subject is refused");
    assert!(matches!(refused, Declined::Blocked { .. }));

    assert!(
        writer
            .shared_log(&store)
            .read_from(Position(2), 10)
            .await
            .unwrap()
            .is_empty(),
        "the log holds only the seed"
    );
    writer.drain().await;
    server.stop().await;
}

/// **The measure is taken on the State the job finds.** Two captures submitted
/// back to back: the second one's measure sees the first one's record.
#[tokio::test]
async fn a_measure_sees_the_state_the_earlier_write_left() {
    let (_scratch, mut server, _store, pool, writer) = world("capture-job-measured").await;
    let saw = Arc::new(Mutex::new(Vec::new()));
    capture_once(&writer, &pool, "person:homer", "r1", saw.clone())
        .await
        .expect("the first capture is admitted");
    capture_once(&writer, &pool, "person:homer", "r2", saw.clone())
        .await
        .expect("the second capture is admitted");
    assert_eq!(
        *saw.lock().unwrap(),
        [false, true],
        "the second measure saw the record the first left"
    );
    writer.drain().await;
    server.stop().await;
}

fn job() -> CaptureJob {
    CaptureJob {
        door: Arc::new(|_, _, _| {
            Ok(Door {
                actor: Actor::Operator,
                owned_keys: BTreeSet::new(),
                owner_revoked: false,
                measures: Measures::default(),
                runs: BTreeMap::new(),
                run: None,
                aged_before: None,
            })
        }),
        measure: Arc::new(|_, _| fits()),
    }
}

/// **The store's capture, called with a job, is decided over the State.** The
/// fact it returns is the State's own read of the record; a name nothing holds
/// is a block; neither the old tables nor a second route is written.
#[tokio::test]
async fn the_ports_capture_with_a_job_is_decided_over_the_state() {
    let (_scratch, mut server, store, _pool, writer) = world("capture-job-port").await;
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    let written = {
        let memory = Arc::clone(&memory);
        let fact = NewFact::about(
            EntityId("person:homer".into()),
            "a claim",
            date(2026, 4, 19),
        );
        within(job(), writer.run(async move { memory.capture(fact).await })).await
    };
    let Ok(Guarded::Written(fact)) = written else {
        panic!("the capture is decided over the State: {written:?}");
    };
    assert_eq!(fact.id.0, "f1");
    assert_eq!(fact.home.0, "person:homer");
    assert_eq!(fact.content, "a claim");
    let seam = writer.seam().expect("the seam is open");
    // The record is named first: the seam's lock is not reentrant, so a read of
    // the State cannot ask the seam again inside itself.
    let record = RecordId(fact_record(&seam, "f1"));
    assert_eq!(
        seam.read_claims(|state| state.fact(&record)),
        Some(fact),
        "the State serves the fact the port returned"
    );

    let blocked = {
        let memory = Arc::clone(&memory);
        let fact = NewFact::about(
            EntityId("person:marge".into()),
            "a claim",
            date(2026, 4, 19),
        );
        within(job(), writer.run(async move { memory.capture(fact).await })).await
    };
    assert!(
        matches!(blocked, Ok(Guarded::Blocked { .. })),
        "a name nothing holds is a block: {blocked:?}"
    );
    writer.drain().await;
    server.stop().await;
}

/// The record that carries `label`, as the State holds it.
fn fact_record(seam: &Arc<crate::ledger_seam::Seam>, label: &str) -> String {
    seam.read_claims(|state| {
        let home = state.resolve("person:homer").cloned().expect("homer");
        state
            .live_records(&home)
            .into_iter()
            .find(|held| held.label == label)
            .map(|held| held.record.0)
            .expect("the record is held")
    })
}
