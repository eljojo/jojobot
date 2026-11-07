//! **The ledger seam's recovery from a commit whose answer was lost.** The seam
//! that production runs, over a real store, with a proxy that lets the server
//! commit and drops its answer.

use jiff::Timestamp;
use jojobot_domain::log::{ObjectId, RequestId};
use jojobot_domain::request::events::{Commit, LedgerEvent, Request, RequestKey};
use sqlx::mysql::MySqlPoolOptions;

use crate::dolt::Dolt;
use crate::ledger_seam::Seam;
use crate::testing::{Scratch, free_port, migrated_database};
use crate::writer::{Writer, sql};

fn at(second: i64) -> Timestamp {
    Timestamp::from_second(1_800_000_000 + second).expect("a moment")
}

fn actor() -> ObjectId {
    ObjectId("bot:homer".into())
}

fn commit(id: &str) -> Commit {
    Commit {
        request: Request {
            key: RequestKey {
                actor: actor(),
                id: RequestId(id.into()),
                generation: 1,
            },
            verb: "capture".into(),
            digest: "digest".into(),
        },
        landed: "{}".into(),
        receipt: None,
        written_at: at(0),
        expires_at: at(100_000),
    }
}

/// **A commit whose answer is lost is settled by the next write's stage.** The
/// store commits the first batch and the caller is told it does not know. The seam
/// is then behind the log; the next write reads the tail before it stages, so it
/// lands behind the first batch and the held state holds both.
#[tokio::test]
async fn a_lost_commit_answer_is_settled_by_the_next_stage() {
    let scratch = Scratch::new("ledger-seam-lost-answer");
    let mut server = Dolt::start(&scratch.0, free_port()).await.unwrap();
    let pool = migrated_database(&server, "alpha").await.unwrap();
    let (port, dropped, proxy) =
        crate::dolt_log::tests::lost_commit_reply_proxy(pool.connect_options().get_port()).await;
    let source =
        MySqlPoolOptions::new().connect_lazy_with((*pool.connect_options()).clone().port(port));
    let writer = Writer::default();
    let sql_pool = sql::Pool::from(source);
    let job_writer = writer.clone();
    writer
        .run(async move {
            let seam = Seam::open(&job_writer, &sql_pool).await.unwrap();
            let first = seam
                .append_event_batch(
                    &sql_pool,
                    vec![LedgerEvent::RequestCommitted(commit("one"))],
                    at(1),
                )
                .await;
            assert!(first.is_err(), "the caller is not told the commit landed");
            assert!(seam.behind(), "the seam knows it may be behind the log");

            seam.append_event_batch(
                &sql_pool,
                vec![LedgerEvent::RequestCommitted(commit("two"))],
                at(2),
            )
            .await
            .expect("the next write lands behind the first batch");
            assert!(!seam.behind(), "the stage read the tail");
            let held = seam.read(|state| {
                let held = |id: &str| {
                    state
                        .lookup(&actor(), &RequestId(id.into()), at(3))
                        .is_some()
                };
                (held("one"), held("two"))
            });
            assert_eq!(held, (true, true), "the state holds both batches");
        })
        .await;
    tokio::time::timeout(std::time::Duration::from_secs(5), dropped.notified())
        .await
        .unwrap();
    drop(writer);
    proxy.abort();
    server.stop().await;
}
