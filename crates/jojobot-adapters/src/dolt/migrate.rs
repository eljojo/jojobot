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
