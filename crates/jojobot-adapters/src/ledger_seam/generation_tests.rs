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
