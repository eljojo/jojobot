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
