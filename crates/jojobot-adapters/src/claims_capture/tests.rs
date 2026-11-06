//! The capture job on a real store: what it admits is sealed with the commit
//! and answered by the claims State, what it refuses leaves nothing, and a
//! measure is taken on the State the job finds, not on one before it.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_domain::actor::Actor;
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::job::{CaptureJob, within};
use jojobot_domain::log::claims::{
    BootFloor, CaptureRequest, ClaimBody, ClaimEvent, ClaimSize, Declined, Door, Draft, KindId,
    Measures, ObjectBody, Route,
};
use jojobot_domain::log::{Event, ObjectId, Position, RecordId, RunId};
use jojobot_domain::memory::{
    EntityId, FactStatus, Guarded, Memory, MemoryError, NewFact, Provenance, Standing,
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
        envelope(ClaimEvent::ObjectCreated {
            object: ObjectId("k2".into()),
            body: ObjectBody {
                kind: KindId("bot".into()),
                parent: None,
                name: "bot:alpha".into(),
                source: "operator".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        envelope(ClaimEvent::AddressBound {
            object: ObjectId("k2".into()),
            address: "bot:alpha".into(),
        }),
        // A record the build supplies, held under an identity of its own: the import
        // creates and binds it, and the overlay on it is a capture.
        envelope(ClaimEvent::ObjectCreated {
            object: ObjectId("supply-view-loops".into()),
            body: ObjectBody {
                kind: KindId("view".into()),
                parent: None,
                name: "The loops view".into(),
                source: "jojobot".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        envelope(ClaimEvent::AddressBound {
            object: ObjectId("supply-view-loops".into()),
            address: "view:loops".into(),
        }),
        // A claim on the bot that a run first wrote: the door asks whose run it is.
        Event {
            run: Some(RunId("run-x".into())),
            ..envelope(ClaimEvent::RecordCreated {
                record: RecordId("seed-record".into()),
                claim: ClaimBody {
                    label: "f1".into(),
                    home: ObjectId("k2".into()),
                    content: "an earlier thought".into(),
                    details: None,
                    provenance: Provenance::Testimony,
                    standing: Standing::Settled,
                    status: FactStatus::Active,
                    recorded_at: date(2026, 4, 1),
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
            })
        },
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
                None,
                None,
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
        .read_from(Position(7), 10)
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

/// **The fact the State serves for an admitted capture says when it was inserted**:
/// the moment of the write that admitted it.
#[tokio::test]
async fn an_admitted_capture_is_served_with_the_moment_it_was_inserted() {
    let (_scratch, mut server, _store, pool, writer) = world("capture-job-inserted").await;
    let saw = Arc::new(Mutex::new(Vec::new()));
    capture_once(&writer, &pool, "person:homer", "r1", saw)
        .await
        .expect("the capture is admitted");
    let seam = writer.seam().expect("the seam is open");
    let inserted = seam.read_claims(|state| {
        state
            .fact(&RecordId("r1".into()))
            .expect("the State holds the fact")
            .inserted_at
    });
    assert_eq!(inserted, Some(at()));
    writer.drain().await;
    server.stop().await;
}

/// **A seam opened over the log serves the moment a claim was inserted**, the one
/// the admitting job stamped: it is read back from the log, not remembered.
#[tokio::test]
async fn a_reopened_seam_serves_the_moment_a_claim_was_inserted() {
    let (_scratch, mut server, _store, pool, writer) = world("capture-job-inserted-replay").await;
    let saw = Arc::new(Mutex::new(Vec::new()));
    capture_once(&writer, &pool, "person:homer", "r1", saw)
        .await
        .expect("the capture is admitted");
    writer.drain().await;

    let reopened = Writer::default();
    let seam = {
        let (opener, pool) = (reopened.clone(), pool.clone());
        reopened
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    let inserted = seam.read_claims(|state| {
        state
            .fact(&RecordId("r1".into()))
            .expect("the replayed State holds the fact")
            .inserted_at
    });
    assert_eq!(inserted, Some(at()));
    reopened.drain().await;
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
            .read_from(Position(7), 10)
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
        measure: Arc::new(|_, _, _| fits()),
        boot_floor: Arc::new(|_, _| Box::pin(async { None })),
        run_bots: Arc::new(|_| Box::pin(async { BTreeMap::new() })),
    }
}

/// A job whose boot render answers `floor`, counting how often it is asked.
fn job_with_boot(floor: Option<BootFloor>, asked: Arc<AtomicUsize>) -> CaptureJob {
    CaptureJob {
        boot_floor: Arc::new(move |_, _| {
            asked.fetch_add(1, Ordering::SeqCst);
            let floor = floor.clone();
            Box::pin(async move { floor })
        }),
        ..job()
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

fn starred(subject: &str) -> NewFact {
    NewFact {
        fields: [("starred".to_string(), "true".to_string())].into(),
        sets: ["starred".to_string()].into(),
        ..NewFact::about(EntityId(subject.into()), "a rule", date(2026, 4, 19))
    }
}

/// **A star that makes the boot too heavy is refused, and nothing is written.**
/// The boot render is handed to the job as a callback, and the job asks it for
/// a write that stars a rule: without that, the refusal would drop silently.
/// **A capture that changes the spec of a bot request is numbered once, by the
/// writer, in the batch of the act.** The store is asked with a job, and the log
/// holds one write of the generation for each spec the request had.
#[tokio::test]
async fn a_capture_of_a_request_spec_through_the_port_is_numbered_once() {
    let (_scratch, mut server, store, pool, writer) = world("capture-job-generation").await;
    {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move {
                let seam = opener.ensure_seam(&pool).await.unwrap();
                let mut tx = pool.writer().begin().await.unwrap();
                let request = vec![
                    envelope(ClaimEvent::ObjectCreated {
                        object: ObjectId("k9".into()),
                        body: ObjectBody {
                            kind: KindId("bot-request".into()),
                            parent: None,
                            name: "bot-request:alpha".into(),
                            source: "operator".into(),
                            aliases: Vec::new(),
                            boot: None,
                            metadata: Default::default(),
                            archived: false,
                            bound_to: None,
                        },
                    }),
                    envelope(ClaimEvent::AddressBound {
                        object: ObjectId("k9".into()),
                        address: "bot-request:alpha".into(),
                    }),
                ];
                let pending = seam
                    .stage_with(&mut tx, Vec::new(), request, at())
                    .await
                    .unwrap();
                sql::commit(tx).await.unwrap();
                pending.committed();
            })
            .await;
    }
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    for desired in ["running", "stopped", "stopped"] {
        let memory = Arc::clone(&memory);
        let mut fact = NewFact::about(
            EntityId("bot-request:alpha".into()),
            "a spec",
            date(2026, 4, 19),
        );
        fact.fields = BTreeMap::from([("desired".to_string(), desired.to_string())]);
        fact.sets = BTreeSet::from(["desired".to_string()]);
        let written = within(job(), writer.run(async move { memory.capture(fact).await })).await;
        assert!(
            matches!(written, Ok(Guarded::Written(_))),
            "the capture is admitted: {written:?}"
        );
    }
    let seam = writer.seam().expect("the seam is open");
    let held = seam.read_claims(|state| state.fields(&ObjectId("k9".into()), &[]));
    assert_eq!(held.get("generation").map(String::as_str), Some("2"));
    let numbers: Vec<String> = writer
        .shared_log(&store)
        .read_from(Position(0), 100)
        .await
        .unwrap()
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter_map(|event| match event.payload {
            ClaimEvent::FieldWritten {
                key,
                write: jojobot_domain::log::claims::FieldValue::Set(value),
                ..
            } if key.0 == "generation" => Some(value),
            _ => None,
        })
        .collect();
    assert_eq!(
        numbers,
        ["1", "2"],
        "one number for each spec, none for the resend"
    );
    writer.drain().await;
    server.stop().await;
}

#[tokio::test]
async fn a_star_that_makes_the_boot_too_heavy_is_refused_and_writes_nothing() {
    let (_scratch, mut server, store, _pool, writer) = world("capture-job-boot-heavy").await;
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    let asked = Arc::new(AtomicUsize::new(0));
    let heavy = BootFloor {
        after: jojobot_domain::text::BOOT_ANSWER.budget + 1,
        before: 0,
        parts: Vec::new(),
        stamp_margin: 0,
    };
    let refused = {
        let memory = Arc::clone(&memory);
        within(
            job_with_boot(Some(heavy), Arc::clone(&asked)),
            writer.run(async move { memory.capture(starred("bot:alpha")).await }),
        )
        .await
    };
    assert!(
        matches!(refused, Err(MemoryError::BootTooHeavy { .. })),
        "an over-budget star is refused: {refused:?}"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    assert!(
        writer
            .shared_log(&store)
            .read_from(Position(7), 10)
            .await
            .unwrap()
            .is_empty(),
        "the refused star wrote nothing"
    );
    writer.drain().await;
    server.stop().await;
}

/// **The boot is asked only of a write that could grow it**, and a boot that
/// cannot be read passes.
#[tokio::test]
async fn the_boot_is_asked_only_of_a_write_that_could_grow_it() {
    let (_scratch, mut server, store, _pool, writer) = world("capture-job-boot-asked").await;
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    let asked = Arc::new(AtomicUsize::new(0));
    let capture = |fact: NewFact| {
        let memory = Arc::clone(&memory);
        let job = job_with_boot(None, Arc::clone(&asked));
        within(job, writer.run(async move { memory.capture(fact).await }))
    };
    let plain = NewFact::about(EntityId("bot:alpha".into()), "a note", date(2026, 4, 19));
    assert!(matches!(capture(plain).await, Ok(Guarded::Written(_))));
    assert_eq!(
        asked.load(Ordering::SeqCst),
        0,
        "a plain write is not asked"
    );
    assert!(
        matches!(capture(starred("bot:alpha")).await, Ok(Guarded::Written(_))),
        "a boot that cannot be read passes"
    );
    assert_eq!(asked.load(Ordering::SeqCst), 1, "a star is asked");
    writer.drain().await;
    server.stop().await;
}

/// **Which bot a run belongs to is asked of a write that could be a thought**,
/// with the runs that first wrote the claims on its subject; a write that draws
/// no `connection` edge asks nothing.
#[tokio::test]
async fn the_runs_of_a_subject_are_asked_of_a_write_that_could_be_a_thought() {
    let (_scratch, mut server, store, _pool, writer) = world("capture-job-run-bots").await;
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    let asked: Arc<Mutex<Vec<Vec<RunId>>>> = Arc::default();
    let job = {
        let asked = Arc::clone(&asked);
        CaptureJob {
            run_bots: Arc::new(move |runs| {
                asked.lock().unwrap().push(runs);
                Box::pin(async { BTreeMap::new() })
            }),
            ..job()
        }
    };
    let capture = |fact: NewFact| {
        let memory = Arc::clone(&memory);
        within(
            job.clone(),
            writer.run(async move { memory.capture(fact).await }),
        )
    };
    let plain = NewFact::about(EntityId("bot:alpha".into()), "a note", date(2026, 4, 19));
    assert!(matches!(capture(plain).await, Ok(Guarded::Written(_))));
    assert!(
        asked.lock().unwrap().is_empty(),
        "a plain write asks nothing"
    );
    let thought = NewFact {
        edge: Some(jojobot_domain::memory::Edge {
            shape: jojobot_domain::memory::EdgeShape::Connection,
            object: EntityId("person:homer".into()),
        }),
        ..NewFact::about(EntityId("bot:alpha".into()), "a thought", date(2026, 4, 19))
    };
    assert!(matches!(capture(thought).await, Ok(Guarded::Written(_))));
    assert_eq!(
        *asked.lock().unwrap(),
        [vec![RunId("run-x".into())]],
        "the run that first wrote a claim on the subject is asked about"
    );
    writer.drain().await;
    server.stop().await;
}

/// **The claim that carries an overlay on a supplied record is a capture the
/// State answers.** The build supplies `view:loops` and the import holds it in the
/// State under an identity of its own; the claim the overlay writes (the operator's
/// name and aliases, in the overlay's keys) is decided by `prepare` like any other,
/// and the State serves it and the keys on the record.
///
/// The overlay layer's own reads before it writes (the fields and the listing of the
/// store beneath) still go to the store's rows, which hold nothing for a supplied
/// record: they move to the State with the fold's re-read, and this case holds the
/// write.
#[tokio::test]
async fn the_claim_carrying_an_overlay_on_a_supplied_record_is_a_capture_the_state_answers() {
    use jojobot_domain::memory::owned::{is_overlay_key, overlay_fields};

    let (_scratch, mut server, store, _pool, writer) = world("capture-job-overlay").await;
    let memory = Arc::new(crate::dolt::memory::DoltMemory::open(store.clone()));
    let new = overlay_fields(Some("Weekly rounds"), Some(&["Rounds".to_string()]));
    let claim = NewFact {
        sets: new.keys().cloned().collect(),
        fields: new,
        ..NewFact::about(
            EntityId("view:loops".into()),
            "Set the name and aliases of this record.",
            date(2026, 4, 19),
        )
    };
    let written = {
        let memory = Arc::clone(&memory);
        within(
            job(),
            writer.run(async move { memory.capture(claim).await }),
        )
        .await
    };
    let Ok(Guarded::Written(fact)) = written else {
        panic!("the claim on a supplied record is admitted: {written:?}");
    };
    assert_eq!(
        fact.home.0, "view:loops",
        "it is homed on the supplied record"
    );
    assert!(
        fact.sets.iter().any(|key| is_overlay_key(key)),
        "it carries the overlay's keys as settings: {:?}",
        fact.sets
    );
    let seam = writer.seam().expect("the seam is open");
    let (fields, held) = seam
        .read_claims(|state| {
            state
                .thing_as_read("view:loops", &state.declared_types())
                .map(|(_, fields, held)| (fields, held))
        })
        .expect("the State holds the supplied record");
    assert_eq!(held, 1, "the overlay is one claim on the record");
    assert!(
        fields.keys().any(|key| is_overlay_key(key)),
        "the overlay's keys reach the record in the State: {fields:?}"
    );
    writer.drain().await;
    server.stop().await;
}

/// **The bot a callback names for a run is the door's**: the map the job is handed
/// is the map `prepare` decides with, not only a question that was asked.
#[tokio::test]
async fn the_bots_a_callback_names_for_runs_reach_the_door() {
    let (_scratch, mut server, _store, pool, writer) = world("capture-job-runs-door").await;
    let admitted = {
        let pool = pool.clone();
        writer
            .run(async move {
                let tx = pool.begin().await.unwrap();
                let seam = crate::writer::Writer::current()
                    .and_then(|writer| writer.seam())
                    .expect("begin opened the seam");
                let thought = CaptureRequest {
                    edge: Some((
                        jojobot_domain::memory::EdgeShape::Connection,
                        "person:homer".into(),
                    )),
                    ..request("bot:alpha", "r1")
                };
                let run_bots: Arc<jojobot_domain::log::claims::job::RunBotsMeasure> =
                    Arc::new(|runs| {
                        Box::pin(async move {
                            runs.into_iter()
                                .map(|run| (run, EntityId("bot:alpha".into())))
                                .collect()
                        })
                    });
                let decided = admit(
                    &seam,
                    Box::new(move |_, _| Ok(thought)),
                    Box::new(|_, _| fits()),
                    None,
                    Some(run_bots),
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
    };
    let Ok(admission) = admitted else {
        panic!("the thought is admitted");
    };
    assert_eq!(
        admission.request.door.runs,
        BTreeMap::from([(RunId("run-x".into()), EntityId("bot:alpha".into()))]),
        "the door carries the bot the callback named for the run"
    );
    writer.drain().await;
    server.stop().await;
}
