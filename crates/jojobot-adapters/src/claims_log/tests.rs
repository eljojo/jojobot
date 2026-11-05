use jojobot_domain::log::claims::{ClaimBody, FieldValue, KeyId, KindId, ObjectBody};
use jojobot_domain::log::testing::InMemoryLog;
use jojobot_domain::log::{LogStore, ObjectId, RecordId, RunId};
use jojobot_domain::memory::{FactStatus, Provenance, Standing, WriteClass};

use super::*;

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
}

fn event(payload: ClaimEvent, second: i64) -> Event<ClaimEvent> {
    Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(at(second)),
        imported_source_time: None,
        payload,
    }
}

fn homer() -> ObjectId {
    ObjectId("k1".into())
}

fn object_created() -> Event<ClaimEvent> {
    event(
        ClaimEvent::ObjectCreated {
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
        },
        0,
    )
}

fn address_bound() -> Event<ClaimEvent> {
    event(
        ClaimEvent::AddressBound {
            object: homer(),
            address: "person:homer".into(),
        },
        0,
    )
}

fn record_created() -> Event<ClaimEvent> {
    event(
        ClaimEvent::RecordCreated {
            record: RecordId("r1".into()),
            claim: ClaimBody {
                label: "f1".into(),
                home: homer(),
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
        1,
    )
}

fn field_written() -> Event<ClaimEvent> {
    event(
        ClaimEvent::FieldWritten {
            record: RecordId("r1".into()),
            subject: homer(),
            key: KeyId("mood".into()),
            ordinal: 1,
            class: WriteClass::Sets,
            write: FieldValue::Set("calm".into()),
            targets: Vec::new(),
        },
        1,
    )
}

async fn claims(log: &Arc<dyn LogStore>) -> LoggedClaims {
    LoggedClaims::open(log.clone()).await.unwrap()
}

fn all() -> Vec<Event<ClaimEvent>> {
    vec![
        object_created(),
        address_bound(),
        record_created(),
        field_written(),
    ]
}

/// **A restart replays to the same state**, answered from the log alone: what the
/// first process held is what the second reads back, and the head is where the
/// writes left it. The state is compared with the one the interpreter builds from
/// the same events read as stored, so a field the codec dropped shows here.
pub(super) async fn restart_replay_on(log: Arc<dyn LogStore>) {
    let first = claims(&log).await;
    first
        .append(vec![object_created(), address_bound()], at(0))
        .await
        .unwrap();
    first
        .append(vec![record_created(), field_written()], at(1))
        .await
        .unwrap();
    let head = first.head().await;
    assert_eq!(head, Position(4));
    let held = first.state().await;
    drop(first);

    let second = claims(&log).await;
    assert_eq!(second.head().await, head);
    assert_eq!(second.state().await, held);
    let as_stored: Vec<Event<ClaimEvent>> = all()
        .iter()
        .map(|event| codec::decode(&codec::encode(event)).unwrap())
        .collect();
    assert_eq!(second.state().await, State::replay(&as_stored));
    assert_eq!(
        second.state().await.resolve("person:homer"),
        Some(&homer()),
        "the address a write bound answers after a restart"
    );
}

/// **A member of another family leaves the replay equal.** The log holds every
/// family's members; the claims family applies its own and skips the rest,
/// whichever side of its own member the foreign one sits.
pub(super) async fn another_family_leaves_the_replay_equal(log: Arc<dyn LogStore>) {
    use jojobot_domain::teaching::events::{RunId as TeachingRun, TeachingEvent};
    let foreign = crate::teaching_log::codec::encode(&TeachingEvent::Cleared {
        run: TeachingRun("marge".into()),
    });
    let own = codec::encode(&address_bound());
    let batch = SealedBatch::seal(
        BatchId("mixed".into()),
        Position(0),
        at(0),
        REPRESENTATION_VERSION,
        vec![
            foreign.clone(),
            codec::encode(&object_created()),
            own,
            foreign,
        ],
    )
    .unwrap();
    assert!(matches!(
        log.append(Position(0), &batch).await.unwrap(),
        Append::Committed(_)
    ));
    let claims = claims(&log).await;
    assert_eq!(claims.head().await, Position(4));
    assert_eq!(claims.state().await.resolve("person:homer"), Some(&homer()));
}

/// **An unknown kind of the claims family's own fails the replay**, so a build
/// that cannot read what the log holds does not serve a state that has it wrong.
pub(super) async fn an_unknown_own_kind_fails_the_replay(log: Arc<dyn LogStore>) {
    let future = br#"{"event_kind":"ClaimsSomethingNew","meaning_version":1,"actor":null,"run":null,"session_card":null,"request":null,"part":null,"occurred_at":null,"written_at":null,"imported_source_time":null,"payload":{"shape":"SomethingNew"}}"#;
    let batch = SealedBatch::seal(
        BatchId("future".into()),
        Position(0),
        at(0),
        REPRESENTATION_VERSION,
        vec![future.to_vec()],
    )
    .unwrap();
    log.append(Position(0), &batch).await.unwrap();
    let opened = LoggedClaims::open(log.clone()).await;
    assert!(
        matches!(opened, Err(ClaimsLogError::Store(_))),
        "{:?}",
        opened.err()
    );
}

#[tokio::test]
async fn a_restart_replays_to_the_same_state() {
    restart_replay_on(Arc::new(InMemoryLog::default())).await;
}

#[tokio::test]
async fn a_member_of_another_family_leaves_the_replay_equal() {
    another_family_leaves_the_replay_equal(Arc::new(InMemoryLog::default())).await;
}

#[tokio::test]
async fn an_unknown_kind_of_the_claims_own_family_fails_the_replay() {
    an_unknown_own_kind_fails_the_replay(Arc::new(InMemoryLog::default())).await;
}
