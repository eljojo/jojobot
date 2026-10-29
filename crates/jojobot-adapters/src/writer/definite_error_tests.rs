//! What a failed statement inside a write says about whether it may have run.

use jojobot_domain::teaching::TeachingError;
use sqlx::{Connection, MySqlConnection};

use super::{Writer, sql};
use crate::dolt::Dolt;
use crate::testing::{Scratch, free_port, migrated_database};

async fn connection_id(pool: &sql::Pool) -> u64 {
    sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&pool.writer())
        .await
        .expect("the dedicated connection answers")
}

/// A server that answers with an error code did not run the statement, so the
/// write is definitely not done: it must not be called indeterminate, and it
/// must not close the writer's connection to fence it.
#[tokio::test]
async fn writer_recovery_definite_class_server_error_is_definite_and_does_not_fence() {
    let scratch = Scratch::new("writer-definite");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let native = migrated_database(&server, "definite_alpha")
        .await
        .expect("native schema");
    let pool: sql::Pool = native.into();
    let writer = Writer::default();
    let (before, indeterminate, definite) = writer
        .run({
            let pool = pool.clone();
            async move {
                let before = connection_id(&pool).await;
                let failed = sqlx::query("INSERT INTO no_such_table (x) VALUES (1)")
                    .execute(&pool)
                    .await
                    .expect_err("the server refuses a missing table");
                assert!(
                    failed.as_database_error().is_some(),
                    "the server named its error: {failed}"
                );
                let answered: TeachingError = sql::store_error(failed);
                (
                    before,
                    matches!(answered, TeachingError::Indeterminate),
                    sql::phase() == sql::Phase::BeforeCommit,
                )
            }
        })
        .await;
    assert!(
        !indeterminate,
        "a statement the server refused was called indeterminate"
    );
    assert!(definite, "the refused statement left the write unsettled");
    let after = writer
        .run({
            let pool = pool.clone();
            async move { connection_id(&pool).await }
        })
        .await;
    assert_eq!(
        before, after,
        "the writer closed its connection to fence a write that did not run"
    );
    server.stop().await;
}

/// A connection lost under a statement may have run it, so the write stays
/// indeterminate and the fence closes the old connection.
#[tokio::test]
async fn writer_recovery_definite_class_lost_connection_stays_indeterminate() {
    let scratch = Scratch::new("writer-lost");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let native = migrated_database(&server, "definite_beta")
        .await
        .expect("native schema");
    let options = native.connect_options();
    let pool: sql::Pool = native.into();
    let writer = Writer::default();
    let indeterminate = writer
        .run(async move {
            let mut conn = pool.acquire().await.expect("the dedicated connection");
            let id: u64 = sqlx::query_scalar("SELECT CONNECTION_ID()")
                .fetch_one(&mut *conn)
                .await
                .expect("its id");
            let mut other = MySqlConnection::connect_with(&options)
                .await
                .expect("a second connection");
            sqlx::raw_sql(sqlx::AssertSqlSafe(format!("KILL CONNECTION {id}")))
                .execute(&mut other)
                .await
                .expect("the old connection is killed");
            let failed = sqlx::query("SELECT 1")
                .execute(&mut *conn)
                .await
                .expect_err("a killed connection fails its next statement");
            assert!(
                failed.as_database_error().is_none(),
                "the server did not answer: {failed}"
            );
            let answered: TeachingError = sql::store_error(failed);
            matches!(answered, TeachingError::Indeterminate)
        })
        .await;
    assert!(
        indeterminate,
        "a lost connection was called a definite failure"
    );
    server.stop().await;
}

/// The same verdict for a statement run on a connection the write acquired: the
/// server's error code settles it as not run, and the write is not fenced.
#[tokio::test]
async fn writer_recovery_definite_class_acquired_connection_error_is_definite() {
    let scratch = Scratch::new("writer-definite-acquired");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("native store");
    let native = migrated_database(&server, "definite_gamma")
        .await
        .expect("native schema");
    let pool: sql::Pool = native.into();
    let writer = Writer::default();
    let (before, indeterminate, definite) = writer
        .run({
            let pool = pool.clone();
            async move {
                let before = connection_id(&pool).await;
                let mut conn = pool.acquire().await.expect("the dedicated connection");
                let failed = sqlx::query("INSERT INTO no_such_table (x) VALUES (1)")
                    .execute(&mut *conn)
                    .await
                    .expect_err("the server refuses a missing table");
                let answered: TeachingError = sql::store_error(failed);
                (
                    before,
                    matches!(answered, TeachingError::Indeterminate),
                    sql::phase() == sql::Phase::BeforeCommit,
                )
            }
        })
        .await;
    assert!(
        !indeterminate,
        "a statement the server refused was called indeterminate"
    );
    assert!(definite, "the refused statement left the write unsettled");
    let after = writer
        .run({
            let pool = pool.clone();
            async move { connection_id(&pool).await }
        })
        .await;
    assert_eq!(
        before, after,
        "the writer closed its connection to fence a write that did not run"
    );
    server.stop().await;
}
