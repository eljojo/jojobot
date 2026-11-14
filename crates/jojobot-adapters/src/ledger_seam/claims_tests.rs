//! The seam holds the claims state beside the ledgers', filled by the one replay.

use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{BatchId, Event, LogStore, ObjectId, Position, RunId, SealedBatch};

use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, imported_database};
use crate::writer::Writer;
use crate::writer_log::WriterLog;

fn event(payload: ClaimEvent) -> Event<ClaimEvent> {
    Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(jiff::Timestamp::from_second(1_700_000_000).expect("a moment")),
        imported_source_time: None,
        payload,
    }
}

fn at(second: i64) -> jiff::Timestamp {
    jiff::Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn homer() -> ObjectId {
    ObjectId("k1".into())
}

fn claims_events() -> Vec<Event<ClaimEvent>> {
    vec![
        event(ClaimEvent::ObjectCreated {
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
        event(ClaimEvent::AddressBound {
            object: homer(),
            address: "person:homer".into(),
        }),
    ]
}

/// **One replay fills the claims state the seam holds.** The log holds a batch of
/// claims events with a member of another family on each side of it; the seam
/// opens over it and answers the address the batch bound, and its head is where
/// the batch left the log. The state is the one an independent replay of the durable batches
/// builds, which shares nothing with the seam.
#[tokio::test]
async fn opening_the_seam_replays_the_claims_members_of_the_log() {
    use jojobot_domain::teaching::events::{RunId as TeachingRun, TeachingEvent};
    let scratch = Scratch::new("seam-claims");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();

    // **The log is seeded by a writer of its own.** A writer opens its seam at its
    // first transaction, and a seam does not see a batch appended after it by
    // another holder; so the seam under test belongs to a second writer, which
    // opens over the seeded log as a restarted process would.
    let seeding = Writer::default();
    // Seed the log with a batch of claims events, then put a foreign member in a batch
    // of its own after it: the family test is by the kind of each member.
    {
        let log = WriterLog::open(store.clone(), seeding.clone())
            .await
            .unwrap();
        // Opening the log imported the old ledger tables, so the log is not empty: the batch
        // is admitted against the head the log holds.
        let head = log
            .read_from(Position(0), 256)
            .await
            .unwrap()
            .last()
            .map_or(Position(0), |batch| batch.range().last);
        let claims = SealedBatch::seal(
            BatchId("claims:seed".into()),
            head,
            jiff::Timestamp::now(),
            1,
            claims_events()
                .iter()
                .map(crate::claims_log::codec::encode)
                .collect(),
        )
        .unwrap();
        log.append(head, &claims).await.unwrap();
        let foreign = crate::teaching_log::codec::encode(&TeachingEvent::Cleared {
            run: TeachingRun("marge".into()),
        });
        let batch = SealedBatch::seal(
            BatchId("foreign".into()),
            claims.range().last,
            jiff::Timestamp::now(),
            1,
            vec![foreign],
        )
        .unwrap();
        log.append(claims.range().last, &batch).await.unwrap();
    }
    // The replay of the durable batches, which holds the import of the old tables the open made as
    // well as the fixture's batch.
    let expected = {
        let log = WriterLog::open(store.clone(), seeding.clone())
            .await
            .unwrap();
        crate::claims_log::replayed_from(&log).await
    };
    seeding.drain().await;
    drop(seeding);

    let writer = Writer::default();
    let pool = crate::writer::sql::Pool::from(store.clone());
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens over a log that holds claims events")
    };
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        Some(homer()),
        "the address the claims batch bound answers from the seam"
    );
    assert_eq!(
        seam.read_claims(Clone::clone),
        expected,
        "the seam reads the log to the state an independent replay of it builds"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A restarted process over a log**: the batches are sealed in order into the log of `store`
/// through a writer of its own, then the seam of a second writer opens over it. A writer opens its
/// seam at its first transaction and a seam does not see a batch appended after it by another
/// holder, so the seam under test cannot belong to the writer that seeded the log.
async fn seam_after_seeding(
    store: &sqlx::MySqlPool,
    batches: Vec<(&str, Vec<Vec<u8>>)>,
) -> (Writer, std::sync::Arc<super::Seam>) {
    let seeding = Writer::default();
    {
        let log = WriterLog::open(store.clone(), seeding.clone())
            .await
            .unwrap();
        for (id, members) in batches {
            let head = log
                .read_from(Position(0), 256)
                .await
                .unwrap()
                .last()
                .map_or(Position(0), |batch| batch.range().last);
            let batch =
                SealedBatch::seal(BatchId(id.into()), head, jiff::Timestamp::now(), 1, members)
                    .unwrap();
            log.append(head, &batch).await.unwrap();
        }
    }
    seeding.drain().await;
    drop(seeding);
    let writer = Writer::default();
    let pool = crate::writer::sql::Pool::from(store.clone());
    let seam = {
        let (opener, pool) = (writer.clone(), pool);
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens over the seeded log")
    };
    (writer, seam)
}

fn encoded(events: &[Event<ClaimEvent>]) -> Vec<Vec<u8>> {
    events
        .iter()
        .map(crate::claims_log::codec::encode)
        .collect()
}

/// **A member of another family leaves the replay equal**, whichever side of the claims
/// members the foreign one sits: the log holds every family's members, and the claims family
/// applies its own and skips the rest.
#[tokio::test]
async fn a_member_of_another_family_on_each_side_leaves_the_seams_claims_equal() {
    use jojobot_domain::teaching::events::{RunId as TeachingRun, TeachingEvent};
    let scratch = Scratch::new("seam-claims-foreign");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    let foreign = crate::teaching_log::codec::encode(&TeachingEvent::Cleared {
        run: TeachingRun("marge".into()),
    });
    let mut members = vec![foreign.clone()];
    members.extend(encoded(&claims_events()));
    members.push(foreign);
    let (writer, seam) = seam_after_seeding(&store, vec![("mixed", members)]).await;
    assert_eq!(
        seam.read_claims(|state| state.resolve("person:homer").cloned()),
        Some(homer()),
        "the claims members apply with a foreign member on each side"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A kind of the claims family's own that this build cannot read stops the claims, not the
/// ledgers.** A build that cannot read what the log holds must not serve a claims State that
/// has it wrong, so the seam refuses to lend its claims; the ledgers go on.
#[tokio::test]
async fn an_unknown_own_kind_makes_the_seam_refuse_to_lend_its_claims() {
    use jojobot_domain::log::claims::ClaimsLender;
    let scratch = Scratch::new("seam-claims-unknown-kind");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    let future = br#"{"event_kind":"ClaimsSomethingNew","meaning_version":1,"actor":null,"run":null,"session_card":null,"request":null,"part":null,"occurred_at":null,"written_at":null,"imported_source_time":null,"payload":{"shape":"SomethingNew"}}"#;
    let (writer, seam) = seam_after_seeding(&store, vec![("future", vec![future.to_vec()])]).await;
    let mut ran = false;
    let lent = seam.lend(&mut |_| ran = true);
    assert!(
        !lent && !ran,
        "a seam that could not read an own member lent its claims anyway"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A claims batch that holds the same handle twice replays safely, and says so once.** A log
/// written before the admission rule can hold two live bindings of one handle; the replay keeps
/// the later one, resolves the earlier as a former handle, counts the repair and logs it.
#[tokio::test]
async fn a_durable_duplicate_handle_replays_safely_as_a_former_handle_and_warns_once() {
    let sink = crate::log_capture::log_sink();
    let scratch = Scratch::new("seam-claims-duplicate");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = imported_database(&server, "alpha").await.unwrap();
    let bound = |address: &str, second: i64| Event {
        written_at: Some(at(second)),
        ..event(ClaimEvent::AddressBound {
            object: homer(),
            address: address.into(),
        })
    };
    let second = bound("person:lisa", 1);
    let mut repeated = second.clone();
    repeated.written_at = Some(at(2));
    let members = encoded(&[
        claims_events().remove(0),
        claims_events().remove(1),
        second,
        repeated,
    ]);
    let (writer, seam) = seam_after_seeding(&store, vec![("duplicate-handles", members)]).await;
    seam.read_claims(|state| {
        assert_eq!(state.handle(&homer()).unwrap().0, "person:lisa");
        let former = state
            .canonical_handle("person:homer")
            .expect("the former handle resolves");
        assert_eq!(former.0, "person:lisa");
        assert_eq!(state.resolve(&former.0), Some(&homer()));
        assert!(
            state.former_handles().is_empty(),
            "no event stated a retirement day"
        );
        assert_eq!(state.duplicate_handle_bindings(), 1);
        assert_eq!(state.last_duplicate_handle_binding_at(), Some(at(1)));
    });
    let logged = sink.text();
    let warnings: Vec<_> = logged
        .lines()
        .filter(|line| {
            line.contains("WARN")
                && line.contains("object=k1")
                && line.contains("person:homer")
                && line.contains("person:lisa")
        })
        .collect();
    assert_eq!(warnings.len(), 1, "each repaired duplicate is logged once");
    assert!(warnings[0].contains("duplicate_handle_bindings=1"));
    assert!(warnings[0].contains(&at(1).to_string()));
    writer.drain().await;
    server.stop().await;
}

/// **A seam opened before the import knows what the import appended.** The mail
/// rail opens the seam in the startup passes, and the claims import runs after them.
/// The import appends through the shared log and not through the seam, so the seam
/// has to be told to read what the import wrote.
#[tokio::test]
async fn a_seam_opened_before_the_import_knows_the_things_the_import_appended() {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};
    let scratch = Scratch::new("seam-follows-import");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = crate::testing::migrated_database(&server, "alpha")
        .await
        .unwrap();
    jojobot_domain::memory::kinds::seed(&crate::dolt::memory::DoltMemory::open(store.clone()))
        .await
        .expect("the kinds are seeded");
    let pool = crate::writer::sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    // A row of the old tables, written by a bare store: it emits no event.
    crate::dolt::memory::DoltMemory::open(store.clone())
        .add_entity(NewEntity::new(
            EntityId("person:homer".into()),
            "Homer",
            "fixture",
        ))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    crate::claims_import::import_claims_once(
        &writer,
        &store,
        &jojobot_domain::memory::owned::Provisions::default(),
        jiff::Timestamp::from_second(1_700_000_000).expect("a moment"),
    )
    .await
    .expect("the import runs");
    assert!(
        seam.read_claims(|state| state.resolve("person:homer").is_some()),
        "the seam reads the thing the import appended"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A seam opened before the supply reconcile knows what the reconcile appended.**
/// The reconcile appends through the shared log like the import, so the seam has to
/// be told to read its batch. The build ships a type the log does not hold yet, and
/// the reconcile declares it.
#[tokio::test]
async fn a_seam_opened_before_the_reconcile_knows_the_declaration_it_appended() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    let scratch = Scratch::new("seam-follows-reconcile");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = crate::testing::migrated_database(&server, "alpha")
        .await
        .unwrap();
    jojobot_domain::memory::kinds::seed(&crate::dolt::memory::DoltMemory::open(store.clone()))
        .await
        .expect("the kinds are seeded");
    let pool = crate::writer::sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    let at = jiff::Timestamp::from_second(1_700_000_000).expect("a moment");
    let provisions = jojobot_domain::memory::owned::Provisions::default();
    crate::claims_import::import_claims_once(&writer, &store, &provisions, at)
        .await
        .expect("the import runs");
    let shipped = [DeclaredType::new(
        "pledge",
        vec![Field::new("witness", ValueType::Text)],
    )];
    let events = crate::claims_import::reconcile_supply_list_once(
        &writer,
        &store,
        &provisions,
        &shipped,
        &["pledge".to_string()],
        at,
        jiff::civil::date(2023, 11, 14),
    )
    .await
    .expect("the reconcile runs");
    assert!(events > 0, "the reconcile declared the shipped type");
    assert!(
        seam.read_claims(|state| state.declared(&ObjectId("pledge".into())).is_some()),
        "the seam reads the declaration the reconcile appended"
    );
    writer.drain().await;
    server.stop().await;
}
