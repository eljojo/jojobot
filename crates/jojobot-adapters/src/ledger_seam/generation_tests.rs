//! The seam numbers the batch of every writer: a bot request whose folded spec the
//! batch changes carries its next generation in the same batch, and a batch that
//! leaves the spec as it was carries none.

use jiff::Timestamp;
use jiff::civil::date;
use jojobot_domain::log::claims::{ClaimBody, ClaimEvent, FieldValue, KeyId, KindId, ObjectBody};
use jojobot_domain::log::{Event, ObjectId, Position, RecordId};
use jojobot_domain::memory::{FactStatus, Provenance, Standing, WriteClass};

use super::Seam;
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_700_000_000 + second).expect("a moment")
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
        written_at: Some(at(0)),
        imported_source_time: None,
        payload,
    }
}

/// Two bot requests, each with its handle.
fn requests() -> Vec<Event<ClaimEvent>> {
    let mut events = Vec::new();
    for (id, handle) in [("k1", "bot-request:alpha"), ("k2", "bot-request:beta")] {
        events.push(envelope(ClaimEvent::ObjectCreated {
            object: ObjectId(id.into()),
            body: ObjectBody {
                kind: KindId("bot-request".into()),
                parent: None,
                name: handle.into(),
                source: "operator".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }));
        events.push(envelope(ClaimEvent::AddressBound {
            object: ObjectId(id.into()),
            address: handle.into(),
        }));
    }
    events
}

/// The record `record` taken back: its claim as the revision leaves it.
fn archive(record: &str, label: &str, home: &str) -> Vec<Event<ClaimEvent>> {
    let mut revised = spec(record, label, home, 1, "running");
    revised.truncate(1);
    revised
        .into_iter()
        .map(|event| match event.payload {
            ClaimEvent::RecordCreated { record, mut claim } => {
                claim.status = FactStatus::Archived;
                Event {
                    payload: ClaimEvent::RecordRevised { record, claim },
                    ..event
                }
            }
            _ => event,
        })
        .collect()
}

/// A claim of record `record` on `home` that sets `desired`.
fn spec(
    record: &str,
    label: &str,
    home: &str,
    ordinal: u64,
    desired: &str,
) -> Vec<Event<ClaimEvent>> {
    vec![
        envelope(ClaimEvent::RecordCreated {
            record: RecordId(record.into()),
            claim: ClaimBody {
                label: label.into(),
                home: ObjectId(home.into()),
                content: "a spec".into(),
                details: None,
                provenance: Provenance::Testimony,
                standing: Standing::Settled,
                status: FactStatus::Active,
                recorded_at: date(2026, 4, 19),
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
        }),
        envelope(ClaimEvent::FieldWritten {
            record: RecordId(record.into()),
            subject: ObjectId(home.into()),
            key: KeyId("desired".into()),
            ordinal,
            class: WriteClass::Sets,
            write: FieldValue::Set(desired.into()),
            targets: Vec::new(),
        }),
    ]
}

async fn stage(
    writer: &Writer,
    pool: &sql::Pool,
    seam: &std::sync::Arc<Seam>,
    claims: Vec<Event<ClaimEvent>>,
) {
    let (seam, pool) = (seam.clone(), pool.clone());
    writer
        .run(async move {
            let mut tx = pool.writer().begin().await.unwrap();
            let pending = seam
                .stage_with(&mut tx, Vec::new(), claims, at(1))
                .await
                .unwrap();
            sql::commit(tx).await.unwrap();
            pending.committed();
        })
        .await;
}

/// The generation writes of the log, as (subject, value), in log order.
async fn numbers(writer: &Writer, store: &sqlx::MySqlPool) -> Vec<(String, String)> {
    writer
        .shared_log(store)
        .read_from(Position(0), 100)
        .await
        .unwrap()
        .iter()
        .flat_map(|batch| batch.members().to_vec())
        .filter_map(|member| crate::claims_log::codec::decode_own(&member).ok().flatten())
        .filter_map(|event| match event.payload {
            ClaimEvent::FieldWritten {
                subject,
                key,
                write: FieldValue::Set(value),
                ..
            } if key.0 == "generation" => Some((subject.0, value)),
            _ => None,
        })
        .collect()
}

/// **Taking back the claim that holds a spec makes the next number, and the
/// number stays in the fold.** It rides on a live claim written in the same batch,
/// never on the archived revision: the keys of an archived claim leave the thing
/// with it.
#[tokio::test]
async fn a_withdrawal_numbers_the_spec_on_a_live_claim() {
    let scratch = Scratch::new("seam-generation-withdrawal");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    stage(&writer, &pool, &seam, requests()).await;
    stage(&writer, &pool, &seam, spec("r1", "f1", "k1", 1, "running")).await;
    stage(&writer, &pool, &seam, archive("r1", "f1", "k1")).await;

    let held = seam.read_claims(|state| state.fields(&ObjectId("k1".into()), &[]));
    assert_eq!(
        held.get("generation").map(String::as_str),
        Some("2"),
        "the second number is in the fold: {held:?}"
    );
    assert_eq!(held.get("desired"), None, "the spec left with its claim");
    writer.drain().await;
    server.stop().await;
}

/// **The claim that carries a withdrawal's number is dated by the server's moment
/// of the batch**, the one the seam is staged at, and never by the wall clock: an
/// instance acting out a day dates what it writes by that day.
#[tokio::test]
async fn the_companion_claim_of_a_withdrawal_is_dated_by_the_stated_day() {
    let scratch = Scratch::new("seam-generation-stated-day");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    stage(&writer, &pool, &seam, requests()).await;
    stage(&writer, &pool, &seam, spec("r1", "f1", "k1", 1, "running")).await;
    // The withdrawal carries no moment of its own: the day comes from the batch.
    let mut withdrawal = archive("r1", "f1", "k1");
    for event in &mut withdrawal {
        event.written_at = None;
    }
    stage(&writer, &pool, &seam, withdrawal).await;

    let dated = seam.read_claims(|state| {
        let located = state.claim_at("bot-request:alpha#f2")?;
        state.fact(&located.record).map(|fact| fact.recorded_at)
    });
    // `at(1)` of these cases is a day in November 2023.
    assert_eq!(dated, Some(date(2023, 11, 14)));
    writer.drain().await;
    server.stop().await;
}

/// **A seam opened before the import knows what the import appended.** The mail
/// rail opens the seam in the startup passes, and the import runs after them.
#[tokio::test]
async fn a_seam_opened_before_the_import_knows_the_things_the_import_appended() {
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};
    let scratch = Scratch::new("seam-follows-import");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    jojobot_domain::memory::kinds::seed(&crate::dolt::memory::DoltMemory::open(store.clone()))
        .await
        .expect("the kinds are seeded");
    let pool = sql::Pool::from(store.clone());
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
        at(0),
    )
    .await
    .expect("the import runs");
    let known = seam.read_claims(|state| state.canonical_handle("person:homer"));
    assert_eq!(known, Some(EntityId("person:homer".into())));
    writer.drain().await;
    server.stop().await;
}

/// **A request made in the batch that sets its spec is numbered by its handle**,
/// the spelling the batch binds, and not by the display name it is created under.
#[tokio::test]
async fn a_request_made_in_the_same_batch_is_known_by_its_handle() {
    let scratch = Scratch::new("seam-generation-same-batch");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    let mut batch = vec![
        envelope(ClaimEvent::ObjectCreated {
            object: ObjectId("k3".into()),
            body: ObjectBody {
                kind: KindId("bot-request".into()),
                parent: None,
                name: "Gamma, a request".into(),
                source: "operator".into(),
                aliases: Vec::new(),
                boot: None,
                metadata: Default::default(),
                archived: false,
                bound_to: None,
            },
        }),
        envelope(ClaimEvent::AddressBound {
            object: ObjectId("k3".into()),
            address: "bot-request:gamma".into(),
        }),
    ];
    batch.extend(spec("r1", "f1", "k3", 1, "running"));
    stage(&writer, &pool, &seam, batch).await;
    assert_eq!(
        numbers(&writer, &store).await,
        [("k3".to_string(), "1".to_string())]
    );
    writer.drain().await;
    server.stop().await;
}

/// **A batch that changes the spec of a request carries its generation, one that
/// repeats the spec carries none, and a batch that changes two requests carries a
/// number for each.** The writer that built the batch sent no number.
#[tokio::test]
async fn the_seam_numbers_the_requests_a_batch_changes() {
    let scratch = Scratch::new("seam-generation");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    let pool = sql::Pool::from(store.clone());
    let writer = Writer::default();
    let seam = {
        let (opener, pool) = (writer.clone(), pool.clone());
        writer
            .run(async move { opener.ensure_seam(&pool).await })
            .await
            .expect("the seam opens")
    };
    stage(&writer, &pool, &seam, requests()).await;
    assert_eq!(numbers(&writer, &store).await, Vec::new());

    stage(&writer, &pool, &seam, spec("r1", "f1", "k1", 1, "running")).await;
    stage(&writer, &pool, &seam, spec("r2", "f2", "k1", 2, "running")).await;
    let mut both = spec("r3", "f3", "k1", 3, "stopped");
    both.extend(spec("r4", "f1", "k2", 1, "running"));
    stage(&writer, &pool, &seam, both).await;

    assert_eq!(
        numbers(&writer, &store).await,
        [
            ("k1".to_string(), "1".to_string()),
            ("k1".to_string(), "2".to_string()),
            ("k2".to_string(), "1".to_string()),
        ],
        "one for the first spec, none for the resend, one for each request in the last batch"
    );
    writer.drain().await;
    server.stop().await;
}
