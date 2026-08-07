//! **Schema migrations** — ordered files the server applies on start.
//!
//! The schema has to be able to move without anybody hand-editing a live
//! database, and a first schema with no way to change it makes the next slice a
//! rewrite. So a change is a new file, files run in order, and the database
//! records which have run.
//!
//! **That is the whole mechanism, and keeping it that way is the point** (rule
//! 106). No DSL, no up/down pairs, no generator: this store speaks MySQL, so a
//! migration is SQL. If this file starts growing a language, it has taken the
//! wrong turn.
//!
//! **The files are compiled in rather than read from disk.** A deployed binary
//! that needs a directory beside it is one that fails on the machine where the
//! directory was not copied, and the failure arrives at start-up on a live host
//! rather than at build time.

use sqlx::{MySql, MySqlPool, Transaction};

/// Every migration, in the order they apply.
///
/// **The order is this list, not the filenames** — a sort is a rule somebody
/// has to know, and a list is one they can read. Adding a migration is a line
/// here and a file beside the others; nothing else.
const MIGRATIONS: &[(&str, &str)] = &[
    (
        "0001_sessions",
        include_str!("../../migrations/0001_sessions.sql"),
    ),
    (
        "0002_mailboxes",
        include_str!("../../migrations/0002_mailboxes.sql"),
    ),
];

/// The table recording what has run. Created by hand rather than by a
/// migration, because it is what says whether a migration has run.
const LEDGER: &str = "CREATE TABLE IF NOT EXISTS schema_migration (
        version    VARCHAR(64) NOT NULL PRIMARY KEY,
        applied_at VARCHAR(48) NOT NULL
    )";

/// Why the schema could not be brought to the shape the code expects.
#[derive(Debug, thiserror::Error)]
pub enum MigrateError {
    /// A migration failed to apply. Named, because "the schema is wrong" with
    /// no version in it sends an operator reading every file.
    #[error("migration {version} did not apply: {why}")]
    Failed {
        /// Which one.
        version: String,
        /// The store's account. Logged; it does not cross to a caller.
        why: String,
    },
}

/// Bring the database to the schema this build expects.
///
/// Idempotent: a migration already recorded is skipped, so a restart applies
/// nothing. **Each migration and its ledger row commit together**, so a
/// migration that half-applied cannot be recorded as done — the failure mode
/// that leaves a database nobody can reason about.
pub async fn run(pool: &MySqlPool) -> Result<Vec<String>, MigrateError> {
    fail_as(sqlx::raw_sql(LEDGER).execute(pool).await, "the ledger")?;

    let done: Vec<String> = fail_as(
        sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(pool)
            .await,
        "the ledger",
    )?;

    let mut applied = Vec::new();
    for (version, sql) in MIGRATIONS {
        if done.iter().any(|seen| seen == version) {
            continue;
        }
        let mut tx: Transaction<'_, MySql> = fail_as(pool.begin().await, version)?;
        fail_as(sqlx::raw_sql(sql).execute(&mut *tx).await, version)?;
        fail_as(
            sqlx::query("INSERT INTO schema_migration (version, applied_at) VALUES (?, ?)")
                .bind(version)
                .bind(jiff::Timestamp::now().to_string())
                .execute(&mut *tx)
                .await,
            version,
        )?;
        fail_as(tx.commit().await, version)?;
        applied.push((*version).to_string());
    }
    if !applied.is_empty() {
        tracing::info!(applied = ?applied, "the store's schema moved");
    }
    Ok(applied)
}

/// Name the migration a failure belongs to, and keep the store's own account
/// in the log rather than in the error a caller might see (rule 53).
fn fail_as<T>(outcome: Result<T, sqlx::Error>, version: &str) -> Result<T, MigrateError> {
    outcome.map_err(|e| {
        tracing::error!(version, error = %e, "a migration failed");
        MigrateError::Failed {
            version: version.to_string(),
            why: "the store refused the change".into(),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port};

    /// **A migration that fails is not recorded as done.**
    ///
    /// The ledger row and the migration commit together, so a change that did
    /// not land leaves no claim that it did. Without that, every later start
    /// skips a migration the database never received — a schema and a ledger
    /// that disagree, which nothing detects and no retry repairs.
    ///
    /// The failure is provoked with the real migrations rather than a rigged
    /// one: a database already holding a table the first migration creates
    /// makes that migration fail exactly as a bad change would.
    ///
    /// **What this does NOT claim is that the database is left untouched.** The
    /// tables created before the failing statement are still there, because
    /// this store does not roll back schema changes. Only the ledger is
    /// transactional, and only the ledger is asserted here.
    #[tokio::test]
    async fn a_migration_that_fails_is_not_recorded_as_done() {
        let scratch = Scratch::new("migrate-fail");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("halfway")
            .await
            .expect("a database of its own");

        // The obstruction: a table the first migration will try to create.
        // `raw_sql`, not a prepared statement — this server answers DDL over
        // the prepare path with a protocol error, which is why the migration
        // runner uses raw statements too.
        sqlx::raw_sql("CREATE TABLE session (id VARCHAR(64) NOT NULL PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("the obstruction lands");

        let refused = run(&pool).await;
        let Err(MigrateError::Failed { version, .. }) = &refused else {
            panic!("a migration that cannot apply must fail: {refused:?}");
        };
        assert_eq!(
            version, "0001_sessions",
            "and it names which one, or an operator reads every file"
        );

        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.is_empty(),
            "a migration that did not land is not recorded as done: {recorded:?}"
        );

        // **The positive the verdict rests on**, on a database of its own: the
        // same call on an unobstructed schema records both versions. Without
        // it the assertion above holds on a store that records nothing at all.
        let clean = store
            .database("unobstructed")
            .await
            .expect("a database of its own");
        assert_eq!(
            run(&clean).await.expect("the schema moves"),
            vec!["0001_sessions".to_string(), "0002_mailboxes".to_string()],
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&clean)
            .await
            .expect("the ledger is readable");
        assert_eq!(recorded.len(), 2, "a migration that landed IS recorded");

        store.stop().await;
    }

    /// **A second start applies nothing, and the tables are still there.**
    ///
    /// Both halves. "Applied nothing" alone passes on a run that never applies
    /// anything at all, which is the same database with none of the schema.
    #[tokio::test]
    async fn migrations_run_once_and_the_schema_stays() {
        let scratch = Scratch::new("migrate");
        let mut store = crate::dolt::Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");

        let first = run(store.pool()).await.expect("the schema moves");
        assert_eq!(
            first,
            vec!["0001_sessions".to_string(), "0002_mailboxes".to_string()],
            "the first start applies what is there, in the order the list gives"
        );

        let second = run(store.pool())
            .await
            .expect("the schema is already there");
        assert!(
            second.is_empty(),
            "a second start applies nothing: {second:?}"
        );

        // …and the schema those migrations were for is really present, which
        // is what stops the assertion above from passing over an empty
        // database that also applied nothing.
        sqlx::query("INSERT INTO session (id, sid, bot, focus, started_at, state)
                     VALUES ('1', NULL, 'bot:gamma', 'proving the schema', '2026-01-01T00:00:00Z', 'active')")
            .execute(store.pool())
            .await
            .expect("the session table is there and takes a row");
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES ('gamma', 'bot:gamma')")
            .execute(store.pool())
            .await
            .expect("the mailbox table is there and takes a row");

        store.stop().await;
    }
}
