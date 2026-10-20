//! **An ungrouped aggregate answers exactly one row, whatever it matched.**
//!
//! A write picks its next ordinal by asking the store for the newest one under a
//! key prefix: `SELECT MAX(ordinal) FROM <table> WHERE <key prefix> = ?`, read as
//! one row. A thing with no writes yet matches no row, and SQL says that answer
//! is one row holding NULL. A store that answers NO row there makes the first
//! write to every new thing fail, so the store is held to the standard answer
//! here, on the shapes the code reads, whichever version of it the toolchain
//! carries.
//!
//! The cases need a temporary directory and the `dolt` binary the toolchain
//! already holds, so they run in an ordinary `cargo test`.

use std::path::PathBuf;

use jojobot_adapters::testing::{free_port, migrated_database, start_unhurried};
use sqlx::MySqlPool;

/// A directory of this run's own, removed when it is done.
struct Scratch(PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = jojobot_adapters::testing::scratch_path(&format!("jojobot-contract-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// **Every row an aggregate query answers**, so a missing row and a NULL one are
/// told apart. `fetch_one` would fold both into a failure.
async fn answers(pool: &MySqlPool, sql: &str, binds: &[&str]) -> Vec<Option<i64>> {
    let mut query = sqlx::query_scalar::<_, Option<i64>>(sqlx::AssertSqlSafe(sql));
    for bind in binds {
        query = query.bind(*bind);
    }
    query
        .fetch_all(pool)
        .await
        .unwrap_or_else(|why| panic!("the store answers {sql}: {why}"))
}

/// The ordinal reads the code made, each over a table of the real schema and the
/// key prefix the code binds. Spelled as they were in `dolt/memory.rs`,
/// `dolt/mailboxes.rs` and `dolt/sessions.rs`, before `dolt/ordinal.rs` took them
/// over, because what the store is held to is the shape, not a wrapper around it.
const PREFIX_READS: &[(&str, &[&str])] = &[
    (
        "SELECT MAX(ordinal) FROM field_write WHERE entity = ? AND `key` = ?",
        &["person:alpha", "donuts"],
    ),
    (
        "SELECT MAX(ordinal) FROM fact_write WHERE entity = ? AND fact_id = ?",
        &["person:alpha", "f1"],
    ),
    (
        "SELECT MAX(ordinal) FROM entity_write WHERE entity = ?",
        &["person:alpha"],
    ),
    (
        "SELECT MAX(ordinal) FROM entity_former_handle WHERE former_handle = ?",
        &["person:alpha"],
    ),
    (
        "SELECT MAX(ordinal) FROM push_attempt WHERE message_id = ?",
        &["m1"],
    ),
    (
        "SELECT MAX(ordinal) FROM charter_change_write WHERE bot = ?",
        &["bot:marge"],
    ),
    (
        "SELECT MAX(ordinal) FROM type_write WHERE name = ?",
        &["stall"],
    ),
];

/// **A key prefix that matches no row, over a table that holds rows, answers one
/// empty row.** The table is a probe with the primary-key shape those tables
/// have, `(a, b, ordinal)`, holding one thing's writes; the prefix asked is
/// another thing's. The positive is the same query for the thing it holds.
#[tokio::test]
async fn an_ungrouped_max_over_a_key_prefix_with_no_matching_row_answers_one_empty_row() {
    let scratch = Scratch::new("max-prefix");
    let mut store = start_unhurried(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = store.database("maxprefix").await.expect("a database");
    sqlx::raw_sql(
        "CREATE TABLE probe (a VARCHAR(64) NOT NULL, b VARCHAR(64) NOT NULL, \
         ordinal INT NOT NULL, PRIMARY KEY (a, b, ordinal))",
    )
    .execute(&pool)
    .await
    .expect("the probe table");
    sqlx::raw_sql("INSERT INTO probe VALUES ('held', 'key', 1), ('held', 'key', 3)")
        .execute(&pool)
        .await
        .expect("a thing with writes");

    let full = "SELECT MAX(ordinal) FROM probe WHERE a = ? AND b = ?";
    assert_eq!(
        answers(&pool, full, &["held", "key"]).await,
        vec![Some(3)],
        "the control: the prefix that matches answers its newest ordinal, so the cases \
         below are not passing on a table the query cannot read"
    );
    assert_eq!(
        answers(&pool, full, &["held", "other"]).await,
        vec![None],
        "a full prefix that matches no row answers one row holding NULL"
    );
    assert_eq!(
        answers(&pool, full, &["nobody", "key"]).await,
        vec![None],
        "…and so does a first part that matches no row"
    );
    assert_eq!(
        answers(
            &pool,
            "SELECT MAX(ordinal) FROM probe WHERE a = ?",
            &["nobody"]
        )
        .await,
        vec![None],
        "…and so does a prefix shorter than the key"
    );
    store.stop().await;
}

/// **The reads the code makes answer one empty row on the real schema**, over
/// tables that hold nothing: no matching row and an empty table at once.
#[tokio::test]
async fn an_ungrouped_max_over_an_empty_table_answers_one_empty_row() {
    let scratch = Scratch::new("max-empty");
    let mut store = start_unhurried(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "maxempty")
        .await
        .expect("a migrated database");

    for (sql, binds) in PREFIX_READS {
        assert_eq!(
            answers(&pool, sql, binds).await,
            vec![None],
            "the read the code makes answers one row holding NULL: {sql}"
        );
    }
    assert_eq!(
        answers(&pool, "SELECT MAX(ordinal) FROM entity_write", &[]).await,
        vec![None],
        "an ungrouped MAX over an empty table answers one row holding NULL"
    );
    assert_eq!(
        answers(
            &pool,
            "SELECT COALESCE(MAX(ordinal), 0) + 1 FROM entity_write",
            &[]
        )
        .await,
        vec![Some(1)],
        "…and the next ordinal counted from it is the first"
    );
    store.stop().await;
}
