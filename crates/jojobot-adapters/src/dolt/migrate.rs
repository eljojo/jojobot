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
//! # One statement per file, and it is not a style rule
//!
//! **This store does not roll back schema changes.** A file holding several
//! statements can fail with the earlier ones committed, leaving a schema that
//! is half a version — and no retry repairs it, because the retry fails on what
//! already landed. One statement per file makes the unit of a migration the
//! unit of atomicity the store actually offers.
//!
//! **Idempotency was the alternative and this store cannot carry it.**
//! `CREATE TABLE IF NOT EXISTS` works; `ALTER TABLE … ADD COLUMN IF NOT EXISTS`
//! is a syntax error here, and a plain `ADD COLUMN` fails on the second run. So
//! idempotency would hold only while every migration is a creation, and break
//! silently on the first one that adds a column.
//!
//! A test enforces the rule, because one that lived in this comment is one the
//! next author breaks.
//!
//! **The files are compiled in rather than read from disk.** A deployed binary
//! that needs a directory beside it is one that fails on the machine where the
//! directory was not copied, and the failure arrives at start-up on a live host
//! rather than at build time.

use sqlx::{MySql, MySqlPool, Transaction};

/// One migration: what it is called, what it does, and what it leaves behind.
struct Migration {
    /// The name the ledger records and a failure reports.
    version: &'static str,
    /// The statement. One of them — see `every_migration_is_a_single_statement`.
    sql: &'static str,
    /// **The table this statement creates**, which is how a start decides
    /// whether an interrupted migration landed.
    ///
    /// It is written here rather than read out of the SQL on purpose: parsing
    /// the statement would be this file growing the language rule 106 keeps
    /// out of it. Every migration so far creates a table. The first one that
    /// alters a table instead will need a different question asked of the
    /// schema, and nothing here will point that out — a test has to.
    creates: &'static str,
}

/// Every migration, in the order they apply.
///
/// **The order is this list, not the filenames** — a sort is a rule somebody
/// has to know, and a list is one they can read. Adding a migration is a line
/// here and a file beside the others; nothing else.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "0001_session",
        sql: include_str!("../../migrations/0001_session.sql"),
        creates: "session",
    },
    Migration {
        version: "0002_journal_entry",
        sql: include_str!("../../migrations/0002_journal_entry.sql"),
        creates: "journal_entry",
    },
    Migration {
        version: "0003_minted",
        sql: include_str!("../../migrations/0003_minted.sql"),
        creates: "minted",
    },
    Migration {
        version: "0004_mailbox",
        sql: include_str!("../../migrations/0004_mailbox.sql"),
        creates: "mailbox",
    },
    Migration {
        version: "0005_message",
        sql: include_str!("../../migrations/0005_message.sql"),
        creates: "message",
    },
];

/// The table recording what has run. Created by hand rather than by a
/// migration, because it is what says whether a migration has run.
const LEDGER: &str = "CREATE TABLE IF NOT EXISTS schema_migration (
        version    VARCHAR(64) NOT NULL PRIMARY KEY,
        applied_at VARCHAR(48) NOT NULL
    )";

/// The table recording what has been STARTED. Hand-made beside the ledger,
/// for the reason the ledger is.
///
/// **A row here means the runner issued a migration's statement and did not
/// get to record the outcome.** It is written and committed before the
/// statement goes out, and it is removed whichever way that statement ends —
/// with the ledger row on success, on its own on failure. So a row that
/// outlives a process means one thing only: the run died in the window
/// between the change and the record of it.
///
/// A column on the ledger would have been the obvious home. The ledger is
/// created `IF NOT EXISTS`, so a database that already has one would never
/// gain the column, and `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` is a syntax
/// error on this store — the one shape it cannot make idempotent.
const BEGUN: &str = "CREATE TABLE IF NOT EXISTS schema_migration_begun (
        version VARCHAR(64) NOT NULL PRIMARY KEY
    )";

/// Say that this migration's statement is about to go out, and commit that
/// before it does.
///
/// The order is the whole point: the marker has to be durable BEFORE the
/// change it describes, or the window it exists to describe is still open.
async fn mark_begun(pool: &MySqlPool, version: &str) -> Result<(), MigrateError> {
    fail_as(
        sqlx::query("INSERT INTO schema_migration_begun (version) VALUES (?)")
            .bind(version)
            .execute(pool)
            .await,
        version,
    )?;
    Ok(())
}

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
/// nothing.
///
/// # A change and the record of it cannot commit together
///
/// **This store applies a schema change as it goes and ignores the transaction
/// around it.** `BEGIN; CREATE TABLE t; ROLLBACK;` leaves `t` standing;
/// `BEGIN; INSERT; ROLLBACK;` really does discard the row. So DDL and the
/// ledger row that records it are two separate commits, and no arrangement of
/// this code makes them one.
///
/// What is left is a window: the change lands, the process dies, and the
/// ledger never hears about it. **The begun marker is how that window is
/// survived.** It is committed BEFORE the statement goes out and removed
/// however that statement ends — with the ledger row on success, on its own on
/// failure. A marker that outlives a process therefore means one thing: the
/// run died in the window.
///
/// A start that finds one asks the schema whether the change landed. Present,
/// so it did: the version is recorded and the run carries on. Absent, so it
/// did not: the migration is applied normally. Either way the start completes,
/// which is what a boot that could wedge for ever did not do.
///
/// **A migration nobody marked is never assumed.** A table standing there with
/// no marker beside it is indistinguishable from one somebody else put there,
/// and this refuses it rather than adopting it — the marker is what makes
/// acceptance a fact about this runner instead of a guess about the schema.
pub async fn run(pool: &MySqlPool) -> Result<Vec<String>, MigrateError> {
    fail_as(sqlx::raw_sql(LEDGER).execute(pool).await, "the ledger")?;
    fail_as(sqlx::raw_sql(BEGUN).execute(pool).await, "the ledger")?;

    let done: Vec<String> = fail_as(
        sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(pool)
            .await,
        "the ledger",
    )?;
    let begun: Vec<String> = fail_as(
        sqlx::query_scalar("SELECT version FROM schema_migration_begun")
            .fetch_all(pool)
            .await,
        "the ledger",
    )?;

    let mut applied = Vec::new();
    for migration in MIGRATIONS {
        let version = migration.version;
        if done.iter().any(|seen| seen == version) {
            continue;
        }
        let interrupted = begun.iter().any(|seen| seen == version);

        // Interrupted, and the change is standing: record it and move on.
        // Nothing is applied here, so nothing joins `applied` — the caller
        // asked what this run changed.
        if interrupted && object_exists(pool, migration.creates).await? {
            tracing::info!(
                version,
                object = migration.creates,
                "an interrupted migration had landed, and the start recorded it"
            );
            record(pool, version).await?;
            continue;
        }

        // Otherwise it is applied. A marker already there is the interrupted
        // case where the change did NOT land; re-marking it would collide with
        // itself, and it already says what it needs to say.
        if !interrupted {
            mark_begun(pool, version).await?;
        }
        if let Err(refused) = sqlx::raw_sql(migration.sql).execute(pool).await {
            // The store refused it. That is not an interruption, so the marker
            // must not outlive the attempt saying it was one.
            clear_begun(pool, version).await;
            return Err(failure(version, refused));
        }
        record(pool, version).await?;
        applied.push(version.to_string());
    }
    if !applied.is_empty() {
        tracing::info!(applied = ?applied, "the store's schema moved");
    }
    Ok(applied)
}

/// Whether the schema holds this table.
///
/// The question a start asks instead of guessing, and the whole of what it
/// asks: the table is there or it is not. Its shape is not inspected, because
/// the marker beside it already says this runner issued the statement that
/// makes it.
async fn object_exists(pool: &MySqlPool, table: &str) -> Result<bool, MigrateError> {
    let found: i64 = fail_as(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.tables
             WHERE table_schema = DATABASE() AND table_name = ?",
        )
        .bind(table)
        .fetch_one(pool)
        .await,
        table,
    )?;
    Ok(found > 0)
}

/// Record a version as applied and spend its marker, together.
///
/// **These two are one commit and can be**, both being ordinary rows: the
/// transaction this store ignores for schema changes it honours for these. So
/// a version is never recorded with its marker left standing.
async fn record(pool: &MySqlPool, version: &str) -> Result<(), MigrateError> {
    let mut tx: Transaction<'_, MySql> = fail_as(pool.begin().await, version)?;
    fail_as(
        sqlx::query("INSERT INTO schema_migration (version, applied_at) VALUES (?, ?)")
            .bind(version)
            .bind(jiff::Timestamp::now().to_string())
            .execute(&mut *tx)
            .await,
        version,
    )?;
    fail_as(
        sqlx::query("DELETE FROM schema_migration_begun WHERE version = ?")
            .bind(version)
            .execute(&mut *tx)
            .await,
        version,
    )?;
    fail_as(tx.commit().await, version)?;
    Ok(())
}

/// Take the marker back off a migration the store refused.
///
/// **Best effort, and it reports rather than returns.** The caller is already
/// carrying the real failure — the refused migration — and replacing it with
/// the failure to tidy up after it would name the wrong problem. A marker left
/// behind here is the one case that reads as an interruption without being
/// one, so it is logged where an operator will find it.
async fn clear_begun(pool: &MySqlPool, version: &str) {
    if let Err(e) = sqlx::query("DELETE FROM schema_migration_begun WHERE version = ?")
        .bind(version)
        .execute(pool)
        .await
    {
        tracing::error!(
            version,
            error = %e,
            "a refused migration kept its begun marker, so the next start will read it as interrupted"
        );
    }
}

/// Name the migration a failure belongs to, and keep the store's own account
/// in the log rather than in the error a caller might see (rule 53).
fn fail_as<T>(outcome: Result<T, sqlx::Error>, version: &str) -> Result<T, MigrateError> {
    outcome.map_err(|e| failure(version, e))
}

/// The same, for a failure the caller has already taken apart.
fn failure(version: &str, e: sqlx::Error) -> MigrateError {
    tracing::error!(version, error = %e, "a migration failed");
    MigrateError::Failed {
        version: version.to_string(),
        why: "the store refused the change".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dolt::tests::{Scratch, free_port};

    /// **Every migration is exactly one statement, and that is the whole of
    /// the atomicity story.**
    ///
    /// This store does not roll back schema changes, so a file holding several
    /// statements can fail with the earlier ones committed — a schema that is
    /// half a version, which no retry repairs because the retry fails on what
    /// already landed.
    ///
    /// One statement per file makes the unit of a migration the unit of
    /// atomicity the store actually offers: a file either applied or did not,
    /// and the ledger cannot disagree with the schema.
    ///
    /// **Idempotency was the alternative and this store cannot support it.**
    /// `CREATE TABLE IF NOT EXISTS` works, but `ALTER TABLE ... ADD COLUMN IF
    /// NOT EXISTS` is a syntax error here and a plain `ADD COLUMN` fails on the
    /// second run. So idempotency holds only while every migration is a
    /// creation, and breaks silently on the first one that adds a column —
    /// a property maintained by remembering rather than by the mechanism.
    ///
    /// **This test is what makes it a property.** A rule that lives in a
    /// comment is a rule the next author breaks.
    /// Every version, in the order they apply — read from one place, so a new
    /// migration cannot make a test lie by omission.
    const ALL_VERSIONS: &[&str] = &[
        "0001_session",
        "0002_journal_entry",
        "0003_minted",
        "0004_mailbox",
        "0005_message",
    ];

    #[test]
    fn every_migration_is_a_single_statement() {
        for Migration { version, sql, .. } in MIGRATIONS {
            let statements = sql
                .lines()
                .filter(|l| !l.trim_start().starts_with("--"))
                .collect::<Vec<_>>()
                .join("\n")
                .split(';')
                .filter(|s| !s.trim().is_empty())
                .count();
            assert_eq!(
                statements, 1,
                "{version} holds {statements} statements. This store commits DDL as it goes, so a \
                 file with more than one can fail half-applied and never recover — split it."
            );
        }
    }

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
            version, "0001_session",
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
            ALL_VERSIONS
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>(),
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&clean)
            .await
            .expect("the ledger is readable");
        assert_eq!(
            recorded.len(),
            ALL_VERSIONS.len(),
            "a migration that landed IS recorded"
        );

        store.stop().await;
    }

    /// **A set that fails part-way RESUMES.** This is what one statement per
    /// file buys, and it was impossible before.
    ///
    /// A failure used to leave the schema half a version with nothing recorded:
    /// the tables created before the failing statement were committed anyway,
    /// so the retry hit one of them and failed again, for ever, naming a file
    /// and saying nothing about the tables underneath it. A person had to work
    /// out by hand which had landed.
    ///
    /// Now a failure stops at a file boundary. Everything before it is applied
    /// AND recorded, the failing one is neither, and clearing the obstruction
    /// lets the next run carry on from exactly where it stopped.
    #[tokio::test]
    async fn a_set_that_fails_part_way_resumes_where_it_stopped() {
        let scratch = Scratch::new("migrate-resume");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("partway")
            .await
            .expect("a database of its own");

        // Obstruct the third migration, so the first two must land and it must
        // not.
        sqlx::raw_sql("CREATE TABLE minted (a INT NOT NULL PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("the obstruction lands");

        let refused = run(&pool).await;
        let Err(MigrateError::Failed { version, .. }) = &refused else {
            panic!("the obstructed migration must fail: {refused:?}");
        };
        assert_eq!(version, "0003_minted");

        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert_eq!(
            recorded,
            ALL_VERSIONS[..2].to_vec(),
            "everything before the failure is applied and recorded, and nothing after it is"
        );

        // Clear it, and the next run picks up at the file that failed.
        sqlx::raw_sql("DROP TABLE minted")
            .execute(&pool)
            .await
            .expect("the obstruction goes");
        assert_eq!(
            run(&pool).await.expect("the rest applies"),
            ALL_VERSIONS[2..].to_vec(),
            "the run resumes at the file that failed, and does not redo the ones that landed"
        );

        // …and the schema is whole, which is what resuming was for.
        sqlx::raw_sql("INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
                       VALUES ('1', 'gamma', 1, 'b', NULL, 's', '2026-01-01T00:00:00Z', 'new', NULL, NULL)")
            .execute(&pool)
            .await
            .expect("the last table is there and takes a row");

        store.stop().await;
    }

    /// **A start after an interrupted migration completes the schema.**
    ///
    /// This store applies a schema change as it goes and ignores the
    /// transaction around it: `BEGIN; CREATE TABLE t; ROLLBACK;` leaves `t`
    /// standing, while `BEGIN; INSERT; ROLLBACK;` really does discard the row.
    /// So a change and the ledger row recording it CANNOT commit together, and
    /// a process that dies between them leaves the table built and the ledger
    /// silent.
    ///
    /// Nothing recovered from that. The next start re-issued the same bare
    /// `CREATE TABLE`, the store answered "table already exists", and the
    /// failure propagated out of start-up — identically, every time, for ever.
    ///
    /// The begun marker is what makes the state legible. It is committed
    /// before the statement goes out and removed however that statement ends,
    /// so a marker that outlives a process means the run died in exactly this
    /// window. The start then asks the schema whether the change landed
    /// instead of guessing: here it did, so the version is recorded and the
    /// run carries on.
    ///
    /// **The interrupted state is built with the runner's own step**, not with
    /// a hand-written row, so this describes what an interruption really
    /// leaves rather than what it is imagined to leave. The verdict travels
    /// the public surface: `run()` is called, and the ledger and the schema
    /// are read back.
    #[tokio::test]
    async fn a_start_after_an_interrupted_migration_completes_the_schema() {
        let scratch = Scratch::new("migrate-interrupted");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("interrupted")
            .await
            .expect("a database of its own");

        // The state a death in the window leaves, made the way the runner
        // makes it: the marker committed, the statement applied, the ledger
        // row never written.
        sqlx::raw_sql(LEDGER)
            .execute(&pool)
            .await
            .expect("the ledger");
        sqlx::raw_sql(BEGUN)
            .execute(&pool)
            .await
            .expect("the marker table");
        mark_begun(&pool, "0001_session")
            .await
            .expect("the marker lands");
        sqlx::raw_sql(MIGRATIONS[0].sql)
            .execute(&pool)
            .await
            .expect("the interrupted statement applied");

        // The whole point: the next start finishes the job.
        let applied = run(&pool).await.expect("the start completes the schema");
        assert_eq!(
            applied,
            ALL_VERSIONS[1..].to_vec(),
            "the interrupted version is not re-applied, and everything after it is"
        );

        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert_eq!(
            recorded.len(),
            ALL_VERSIONS.len(),
            "the interrupted version is recorded too, so a later start skips it: {recorded:?}"
        );

        // The marker is spent. Left behind, it would claim for ever that a
        // migration is mid-flight.
        let still_begun: Vec<String> =
            sqlx::query_scalar("SELECT version FROM schema_migration_begun")
                .fetch_all(&pool)
                .await
                .expect("the marker table is readable");
        assert!(
            still_begun.is_empty(),
            "a resolved marker is cleared: {still_begun:?}"
        );

        // …and the schema is whole, which is what completing it was for. This
        // is the positive the assertions above rest on: without it they hold
        // just as well on a database that applied nothing.
        sqlx::query("INSERT INTO session (id, sid, bot, focus, started_at, state)
                     VALUES ('1', NULL, 'bot:gamma', 'proving the schema', '2026-01-01T00:00:00Z', 'active')")
            .execute(&pool)
            .await
            .expect("the interrupted table is usable");
        sqlx::query("INSERT INTO mailbox (name, owner) VALUES ('gamma', 'bot:gamma')")
            .execute(&pool)
            .await
            .expect("a later table is there too");

        store.stop().await;
    }

    /// **A migration the store refused is refused again, not adopted.**
    ///
    /// This is the guard that makes the marker honest, and it is the trap the
    /// whole design exists to avoid. A start decides an interrupted migration
    /// landed by finding the table it creates — so if a REFUSED migration left
    /// its marker standing, the next start would find the obstructing table
    /// beside a marker, conclude the change had landed, and record the version
    /// as done. The schema would then be whatever somebody else's table
    /// happens to be, and the ledger would swear it was ours. Silently.
    ///
    /// So the marker comes off whichever way the statement ends, and a refusal
    /// stays a refusal however many times the server starts.
    #[tokio::test]
    async fn a_refused_migration_is_refused_again_and_not_adopted() {
        let scratch = Scratch::new("migrate-refused-twice");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("obstructed")
            .await
            .expect("a database of its own");

        // A `session` table that is not the one the migration builds.
        sqlx::raw_sql("CREATE TABLE session (wrong VARCHAR(8) NOT NULL PRIMARY KEY)")
            .execute(&pool)
            .await
            .expect("the obstruction lands");

        for attempt in ["the first start", "the second start"] {
            let refused = run(&pool).await;
            let Err(MigrateError::Failed { version, .. }) = &refused else {
                panic!("{attempt} must refuse the obstructed migration: {refused:?}");
            };
            assert_eq!(version, "0001_session", "{attempt} names it");

            let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
                .fetch_all(&pool)
                .await
                .expect("the ledger is readable");
            assert!(
                recorded.is_empty(),
                "{attempt} recorded a migration that never applied: {recorded:?}"
            );

            let markers: Vec<String> =
                sqlx::query_scalar("SELECT version FROM schema_migration_begun")
                    .fetch_all(&pool)
                    .await
                    .expect("the marker table is readable");
            assert!(
                markers.is_empty(),
                "{attempt} left a marker on a refused migration, which the next start would read \
                 as an interruption and adopt: {markers:?}"
            );
        }

        // The obstruction is still the obstruction — nothing quietly replaced
        // it with the migration's own table.
        sqlx::query("INSERT INTO session (wrong) VALUES ('x')")
            .execute(&pool)
            .await
            .expect("the obstructing table is untouched");

        store.stop().await;
    }

    /// **An interruption before the change landed applies it normally.**
    ///
    /// The other half of the interrupted case. A marker says a statement was
    /// issued; it does not say the statement arrived. When the table is not
    /// there, the migration simply runs — and the start must not trip over its
    /// own marker while doing it.
    #[tokio::test]
    async fn an_interruption_before_the_change_landed_applies_it() {
        let scratch = Scratch::new("migrate-interrupted-early");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("earlydeath")
            .await
            .expect("a database of its own");

        // The marker lands and the process dies before the statement goes out.
        sqlx::raw_sql(LEDGER)
            .execute(&pool)
            .await
            .expect("the ledger");
        sqlx::raw_sql(BEGUN)
            .execute(&pool)
            .await
            .expect("the marker table");
        mark_begun(&pool, "0001_session")
            .await
            .expect("the marker lands");

        assert_eq!(
            run(&pool).await.expect("the start applies it"),
            ALL_VERSIONS
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>(),
            "the un-landed migration is applied like any other, marker or no marker"
        );

        let markers: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration_begun")
            .fetch_all(&pool)
            .await
            .expect("the marker table is readable");
        assert!(markers.is_empty(), "and its marker is spent: {markers:?}");

        // The schema is real, not merely reported.
        sqlx::query("INSERT INTO session (id, sid, bot, focus, started_at, state)
                     VALUES ('1', NULL, 'bot:gamma', 'proving the schema', '2026-01-01T00:00:00Z', 'active')")
            .execute(&pool)
            .await
            .expect("the table the marker was about is there and takes a row");

        store.stop().await;
    }

    /// **The start itself leaves the marker, and a later start heals from it.**
    ///
    /// The other interrupted tests place the marker themselves, so they prove
    /// what a start does with one and NOT that a start ever writes one. Both
    /// pass unchanged on a build where `run` never marks anything at all —
    /// which is the build where a real interruption leaves no marker and the
    /// old permanent wedge is back.
    ///
    /// So this drives the whole thing through `run` twice, with the window
    /// forced open in between. A ledger the runner did not build refuses the
    /// row that records a version; the ledger is created `IF NOT EXISTS`, so
    /// it survives the start untouched and the recording step fails on it.
    /// That is the production window exactly: the change applied, the record
    /// of it lost. The process death is the only part being stood in for.
    ///
    /// Then the ledger is repaired and the next start finishes the job, from a
    /// marker no test wrote.
    #[tokio::test]
    async fn the_start_marks_the_window_it_can_be_interrupted_in() {
        let scratch = Scratch::new("migrate-marks-window");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("windowed")
            .await
            .expect("a database of its own");

        // A ledger that takes no rows: the recording step will fail on it.
        sqlx::raw_sql(
            "CREATE TABLE schema_migration (
                 version    VARCHAR(64) NOT NULL PRIMARY KEY,
                 applied_at VARCHAR(48) NOT NULL,
                 refuses    INT         NOT NULL
             )",
        )
        .execute(&pool)
        .await
        .expect("the unwritable ledger lands");

        let interrupted = run(&pool).await;
        assert!(
            matches!(&interrupted, Err(MigrateError::Failed { version, .. }) if version == "0001_session"),
            "the recording step must fail on a ledger that refuses the row: {interrupted:?}"
        );

        // The window, as the start left it: the change applied…
        let tables: Vec<String> = sqlx::query_scalar(
            "SELECT table_name FROM information_schema.tables WHERE table_schema = DATABASE()",
        )
        .fetch_all(&pool)
        .await
        .expect("the schema is readable");
        assert!(
            tables.iter().any(|t| t == "session"),
            "the interrupted migration's change applied: {tables:?}"
        );

        // …and the marker THE START wrote still standing over it.
        let markers: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration_begun")
            .fetch_all(&pool)
            .await
            .expect("the marker table is readable");
        assert_eq!(
            markers,
            vec!["0001_session".to_string()],
            "the start marks the window before the change, or nothing can heal it later"
        );

        // Repair the ledger and start again. Nothing here writes a marker —
        // the one the first start left is what the recovery runs on.
        sqlx::raw_sql("DROP TABLE schema_migration")
            .execute(&pool)
            .await
            .expect("the unwritable ledger goes");

        let applied = run(&pool)
            .await
            .expect("the next start completes the schema");
        assert_eq!(
            applied,
            ALL_VERSIONS[1..].to_vec(),
            "the interrupted version is healed rather than re-applied"
        );

        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert_eq!(
            recorded.len(),
            ALL_VERSIONS.len(),
            "and every version is recorded, the healed one included: {recorded:?}"
        );

        // The schema really is whole, which is what healing was for.
        sqlx::query("INSERT INTO session (id, sid, bot, focus, started_at, state)
                     VALUES ('1', NULL, 'bot:gamma', 'proving the schema', '2026-01-01T00:00:00Z', 'active')")
            .execute(&pool)
            .await
            .expect("the interrupted table is usable");
        sqlx::query("INSERT INTO message (id, mailbox, ordinal, body, subject, sender, sent_at, state, notes, in_reply_to)
                     VALUES ('1', 'gamma', 1, 'b', NULL, 's', '2026-01-01T00:00:00Z', 'new', NULL, NULL)")
            .execute(&pool)
            .await
            .expect("the last table is there too");

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
            ALL_VERSIONS
                .iter()
                .map(|v| v.to_string())
                .collect::<Vec<_>>(),
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
