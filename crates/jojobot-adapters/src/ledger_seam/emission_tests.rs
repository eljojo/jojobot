//! A writer adds its claims events to the transaction's one batch through the
//! writer's job context, and the commit seals them with the request's ledger
//! event. The call sites of the commit do not change.

use jiff::Timestamp;
use jojobot_domain::clock::Clock;
use jojobot_domain::log::claims::{ClaimEvent, KindId, ObjectBody};
use jojobot_domain::log::{Event, ObjectId, Position, RunId};
use jojobot_domain::memory::EntityId;
use jojobot_domain::request::{self, Request, RequestId, Scope};

use crate::dolt::Dolt;
use crate::dolt::requests;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn homer() -> ObjectId {
    ObjectId("person:homer".into())
}

fn created() -> Vec<Event<ClaimEvent>> {
    vec![Event {
        event_kind: String::new(),
        meaning_version: 1,
        actor: None,
        run: Some(RunId("run-a".into())),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: Some(Timestamp::from_second(1_700_000_000).expect("a moment")),
        imported_source_time: None,
        payload: ClaimEvent::ObjectCreated {
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
    }]
}

fn scope() -> std::sync::Arc<Scope> {
    Scope::new(Request {
        bot: EntityId("person:homer".into()),
        id: RequestId::parse("alpha").expect("a valid request id"),
        verb: "capture".into(),
        digest: "digest-alpha".into(),
    })
}

async fn store_with_homer(name: &str) -> (Scratch, Dolt, sqlx::MySqlPool, sql::Pool, Writer) {
    let scratch = Scratch::new(name);
    let server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let store = migrated_database(&server, "alpha").await.unwrap();
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, boot, prose) \
         VALUES ('person:homer', 'person', 'Homer', 'fixture', 'on-demand', '')",
    )
    .execute(&store)
    .await
    .expect("the caller's entity");
    let pool = sql::Pool::from(store.clone());
    (scratch, server, store, pool, Writer::default())
}

/// **One transaction, one batch**: the request's ledger event and the emitted
/// claims events are sealed together at consecutive positions, and the seam
/// applies both.
#[tokio::test]
async fn a_commit_seals_the_emitted_claims_with_the_requests_ledger_event() {
    let (_scratch, mut server, store, pool, writer) = store_with_homer("emission-one-batch").await;

    {
        let pool = pool.clone();
        writer
            .run(async move {
                request::within(
                    scope(),
                    request::armed(async {
                        let tx = pool.begin().await.unwrap();
                        sql::emit(created()).unwrap();
                        requests::commit(tx, &Clock::default(), || "{}".into())
                            .await
                            .unwrap();
                    }),
                )
                .await;
            })
            .await;
    }

    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 1, "one batch for one transaction");
    assert_eq!(
        batches[0].members().len(),
        2,
        "the ledger member and the emitted claims event"
    );
    assert_eq!(batches[0].range().last, Position(2));
    let seam = writer.seam().expect("the seam opened with the first begin");
    assert!(
        seam.read_claims(|state| state.object(&homer()).is_some()),
        "the claims State answers the object the commit emitted"
    );
    writer.drain().await;
    server.stop().await;
}

/// **A call that carries no request id still seals what a writer emitted**, as a
/// batch of claims alone.
#[tokio::test]
async fn a_commit_with_no_request_id_seals_the_claims_alone() {
    let (_scratch, mut server, store, pool, writer) =
        store_with_homer("emission-claims-only").await;

    {
        let pool = pool.clone();
        writer
            .run(async move {
                let tx = pool.begin().await.unwrap();
                sql::emit(created()).unwrap();
                requests::commit(tx, &Clock::default(), || "{}".into())
                    .await
                    .unwrap();
            })
            .await;
    }

    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(batches[0].members().len(), 1);
    writer.drain().await;
    server.stop().await;
}

/// **A transaction that rolls back leaves its emitted events behind it**: the
/// next transaction of the same job seals only its own.
#[tokio::test]
async fn events_emitted_in_a_rolled_back_transaction_are_not_sealed_by_the_next() {
    let (_scratch, mut server, store, pool, writer) =
        store_with_homer("emission-rolled-back").await;

    {
        let pool = pool.clone();
        writer
            .run(async move {
                let tx = pool.begin().await.unwrap();
                sql::emit(created()).unwrap();
                tx.rollback().await.unwrap();
                request::within(
                    scope(),
                    request::armed(async {
                        let tx = pool.begin().await.unwrap();
                        requests::commit(tx, &Clock::default(), || "{}".into())
                            .await
                            .unwrap();
                    }),
                )
                .await;
            })
            .await;
    }

    let batches = writer
        .shared_log(&store)
        .read_from(Position(0), 10)
        .await
        .unwrap();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0].members().len(),
        1,
        "the ledger member alone: the rolled-back claims are gone"
    );
    writer.drain().await;
    server.stop().await;
}

/// **An emission outside a writer job fails**, and nothing is kept for a later
/// job to seal.
#[tokio::test]
async fn an_emission_outside_a_writer_job_is_refused() {
    assert!(sql::emit(created()).is_err());
}
