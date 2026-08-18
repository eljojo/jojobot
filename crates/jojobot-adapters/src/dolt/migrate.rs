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
pub(crate) struct Migration {
    /// The name the ledger records and a failure reports.
    version: &'static str,
    /// The statement. One of them — see `every_migration_is_a_single_statement`.
    sql: &'static str,
    /// **What the schema looks like once this statement has run**, which is how
    /// a start decides whether an interrupted migration landed.
    ///
    /// It is written here rather than read out of the SQL on purpose: parsing
    /// the statement would be this file growing the language rule 106 keeps out
    /// of it. A migration whose shape none of these answers for reaches no
    /// answer at all, and nothing here will point that out — a test has to.
    leaves: Leaves,
}

/// The state a migration leaves the schema in, and the question a start asks to
/// find out whether an interrupted one got there.
#[derive(Clone, Copy)]
enum Leaves {
    /// The table this statement creates.
    Table(&'static str),
    /// The table this statement removes. **The same question, asked the other
    /// way round** — a drop that was interrupted after it committed leaves
    /// nothing behind for a "is it there" check to find, and re-running it
    /// fails on a table that is already gone.
    NoTable(&'static str),
    /// The column this statement adds to a table that was already there. The
    /// table answers "yes" either way, so an ALTER has to be asked about the
    /// thing it actually changes — and it needs the answer more than a
    /// creation does, because `ADD COLUMN` cannot be made idempotent on this
    /// store and a re-issued one fails.
    Column(&'static str, &'static str),
    /// The table this statement leaves with **no row answering this
    /// condition** — the shape of a backfill.
    ///
    /// A backfill changes rows and leaves the schema exactly as it found it,
    /// so every question above answers the same before it and after it. Given
    /// the column shape — the nearest fit — it answers "already reached" from
    /// the moment the migration BEFORE it added that column, and the runner
    /// records a backfill it never ran. That failure is silent and permanent:
    /// unfilled rows behind a ledger saying they were filled.
    ///
    /// So this one reaches the rows. The condition describes what is left to
    /// do — `owner IS NULL`, `kind = ''` — and the migration has landed when
    /// nothing answers it.
    ///
    /// **The condition is SQL, and that is not this file growing a language**
    /// (rule 106). Every migration here already carries its whole statement as
    /// an opaque string in the language the store speaks; this is a second
    /// opaque string in the same language. Nothing parses it, nothing composes
    /// it, and it reaches the store as the tail of one `SELECT COUNT(*)`. The
    /// step past it — a free-form probe query — is the one refused, because a
    /// probe that can ask anything is one nothing constrains to asking about
    /// this migration.
    ///
    /// **No migration has this shape yet**, and the tests are its only users:
    /// a shape and its first user are separate changes, because the user
    /// arrives with the feature that needs one.
    ///
    /// `expect` rather than `allow`, so the first migration to take this shape
    /// makes the attribute itself a warning and the note comes off in the diff
    /// that dates it. `not(test)` because the tests below DO construct it, so
    /// under `cfg(test)` there is nothing to expect.
    #[cfg_attr(not(test), expect(dead_code))]
    NoRows(&'static str, &'static str),
    /// The index this statement puts on the table.
    ///
    /// The table and its columns are all there either way, so the questions
    /// above answer "yes" for an index that was never built — and `CREATE
    /// INDEX` is refused on a name that already exists, exactly as `ADD
    /// COLUMN` is. Uniqueness is what makes an identity column an identity
    /// rather than a suggestion, so this is not a shape a schema can be vague
    /// about.
    ///
    /// No migration has this shape yet either — see [`Leaves::NoRows`].
    #[cfg_attr(not(test), expect(dead_code))]
    Index(&'static str, &'static str),
    /// The type this statement leaves that column declared as — the shape of a
    /// statement that changes a column rather than adding one.
    ///
    /// The table is there and the column is there before it and after it, so
    /// [`Leaves::Column`] answers "already reached" for a change that never
    /// landed. The runner then records the version, the column keeps the type
    /// it had, and every write that needed the new one fails against a ledger
    /// saying the change is in.
    ///
    /// **The type is compared, not the width**, because a width-only question
    /// answers one migration and sends the next change to a column straight
    /// back here. It is the store's own spelling of the declared type —
    /// `varchar(32)` — read from the view [`column_exists`] already uses, and
    /// a test pins the spelling so this does not rest on remembering it.
    ColumnType(&'static str, &'static str, &'static str),
}

impl Leaves {
    /// The table this migration is about, for a log line that names it.
    fn table(self) -> &'static str {
        match self {
            Leaves::Table(t)
            | Leaves::NoTable(t)
            | Leaves::Column(t, _)
            | Leaves::NoRows(t, _)
            | Leaves::Index(t, _)
            | Leaves::ColumnType(t, _, _) => t,
        }
    }

    /// Whether the schema is already in the state this migration produces.
    async fn reached(self, pool: &MySqlPool) -> Result<bool, MigrateError> {
        Ok(match self {
            Leaves::Table(t) => object_exists(pool, t).await?,
            Leaves::NoTable(t) => !object_exists(pool, t).await?,
            Leaves::Column(t, c) => column_exists(pool, t, c).await?,
            Leaves::NoRows(t, condition) => !any_row_answers(pool, t, condition).await?,
            Leaves::Index(t, i) => index_exists(pool, t, i).await?,
            Leaves::ColumnType(t, c, declared) => column_is(pool, t, c, declared).await?,
        })
    }
}

/// Every migration, in the order they apply.
///
/// **The order is this list, not the filenames** — a sort is a rule somebody
/// has to know, and a list is one they can read. Adding a migration is a line
/// here and a file beside the others; nothing else.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        version: "0001_session",
        sql: include_str!("../../migrations/0001_session.sql"),
        leaves: Leaves::Table("session"),
    },
    Migration {
        version: "0002_journal_entry",
        sql: include_str!("../../migrations/0002_journal_entry.sql"),
        leaves: Leaves::Table("journal_entry"),
    },
    Migration {
        version: "0003_minted",
        sql: include_str!("../../migrations/0003_minted.sql"),
        leaves: Leaves::Table("minted"),
    },
    Migration {
        version: "0004_mailbox",
        sql: include_str!("../../migrations/0004_mailbox.sql"),
        leaves: Leaves::Table("mailbox"),
    },
    Migration {
        version: "0005_message",
        sql: include_str!("../../migrations/0005_message.sql"),
        leaves: Leaves::Table("message"),
    },
    Migration {
        version: "0006_handover",
        sql: include_str!("../../migrations/0006_handover.sql"),
        leaves: Leaves::Table("handover"),
    },
    Migration {
        version: "0007_drop_minted",
        sql: include_str!("../../migrations/0007_drop_minted.sql"),
        leaves: Leaves::NoTable("minted"),
    },
    Migration {
        version: "0008_entity",
        sql: include_str!("../../migrations/0008_entity.sql"),
        leaves: Leaves::Table("entity"),
    },
    Migration {
        version: "0009_entity_alias",
        sql: include_str!("../../migrations/0009_entity_alias.sql"),
        leaves: Leaves::Table("entity_alias"),
    },
    Migration {
        version: "0010_fact",
        sql: include_str!("../../migrations/0010_fact.sql"),
        leaves: Leaves::Table("fact"),
    },
    Migration {
        version: "0011_fact_event_metadata",
        sql: include_str!("../../migrations/0011_fact_event_metadata.sql"),
        leaves: Leaves::Table("fact_event_metadata"),
    },
    Migration {
        version: "0012_fact_event_ref",
        sql: include_str!("../../migrations/0012_fact_event_ref.sql"),
        leaves: Leaves::Table("fact_event_ref"),
    },
    Migration {
        version: "0013_type_field",
        sql: include_str!("../../migrations/0013_type_field.sql"),
        leaves: Leaves::Table("type_field"),
    },
    Migration {
        version: "0014_message_delivery",
        sql: include_str!("../../migrations/0014_message_delivery.sql"),
        leaves: Leaves::Table("message_delivery"),
    },
    Migration {
        version: "0015_type_field_origin",
        sql: include_str!("../../migrations/0015_type_field_origin.sql"),
        leaves: Leaves::Column("type_field", "origin"),
    },
    Migration {
        version: "0016_field_write",
        sql: include_str!("../../migrations/0016_field_write.sql"),
        leaves: Leaves::Table("field_write"),
    },
    Migration {
        version: "0017_type_field_folds",
        sql: include_str!("../../migrations/0017_type_field_folds.sql"),
        leaves: Leaves::Column("type_field", "folds"),
    },
    Migration {
        version: "0018_type_field_holds_wider",
        sql: include_str!("../../migrations/0018_type_field_holds_wider.sql"),
        leaves: Leaves::ColumnType("type_field", "holds", "varchar(32)"),
    },
    Migration {
        version: "0019_entity_source_wider",
        sql: include_str!("../../migrations/0019_entity_source_wider.sql"),
        leaves: Leaves::ColumnType("entity", "source", "varchar(255)"),
    },
    Migration {
        version: "0020_entity_crm_wider",
        sql: include_str!("../../migrations/0020_entity_crm_wider.sql"),
        leaves: Leaves::ColumnType("entity", "crm", "varchar(255)"),
    },
    Migration {
        version: "0021_kind",
        sql: include_str!("../../migrations/0021_kind.sql"),
        leaves: Leaves::Table("kind"),
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
    apply(pool, MIGRATIONS).await
}

/// The runner itself, over the list it is given.
///
/// **The list is a parameter so a test can drive this over migrations of its
/// own.** A shape is only proven by a migration that has it, and this file
/// ships the shapes rather than users of them — so without a list of its own a
/// test can reach a new shape only by calling [`Leaves::reached`] directly,
/// which proves the question and not that the runner asks it.
async fn apply(pool: &MySqlPool, migrations: &[Migration]) -> Result<Vec<String>, MigrateError> {
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
    for migration in migrations {
        let version = migration.version;
        if done.iter().any(|seen| seen == version) {
            continue;
        }
        let interrupted = begun.iter().any(|seen| seen == version);

        // Interrupted, and the change is standing: record it and move on.
        // Nothing is applied here, so nothing joins `applied` — the caller
        // asked what this run changed.
        if interrupted && migration.leaves.reached(pool).await? {
            tracing::info!(
                version,
                object = migration.leaves.table(),
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

/// Whether that table already carries this column.
///
/// The same question [`object_exists`] asks, one level down, for the
/// migrations that change a table rather than make one. `ADD COLUMN` is the
/// one statement this store cannot make idempotent, so an interrupted one has
/// to be recognized by what it left or the next start re-issues it and fails.
async fn column_exists(pool: &MySqlPool, table: &str, column: &str) -> Result<bool, MigrateError> {
    let found: i64 = fail_as(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?",
        )
        .bind(table)
        .bind(column)
        .fetch_one(pool)
        .await,
        table,
    )?;
    Ok(found > 0)
}

/// Whether that table's column is declared as this type.
///
/// The question [`column_exists`] cannot ask, for a statement that changes a
/// column instead of adding one: the column is there either way, so its
/// presence says nothing about whether the change landed.
///
/// The comparison is against `column_type`, the store's own rendering of the
/// declared type — `varchar(32)`, `int` — rather than against a width, so one
/// question serves every change a column can undergo.
async fn column_is(
    pool: &MySqlPool,
    table: &str,
    column: &str,
    declared: &str,
) -> Result<bool, MigrateError> {
    let found: i64 = fail_as(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.columns
             WHERE table_schema = DATABASE() AND table_name = ? AND column_name = ?
               AND column_type = ?",
        )
        .bind(table)
        .bind(column)
        .bind(declared)
        .fetch_one(pool)
        .await,
        table,
    )?;
    Ok(found > 0)
}

/// Whether any row of that table still answers the condition.
///
/// The question a start asks about a migration that changed rows rather than
/// the schema, where every question about the schema answers the same before
/// and after.
///
/// **The table and the condition are written into the statement rather than
/// bound to it.** Neither is a value, so neither can be a parameter: a
/// placeholder in a `FROM` or in place of a whole `WHERE` clause is a syntax
/// error, not a slower way of doing the same thing. They come from
/// [`MIGRATIONS`], which is compiled in, so the only writer is somebody editing
/// this file — nothing a caller sends reaches here.
async fn any_row_answers(
    pool: &MySqlPool,
    table: &str,
    condition: &str,
) -> Result<bool, MigrateError> {
    let found: i64 = fail_as(
        sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table} WHERE {condition}"))
            .fetch_one(pool)
            .await,
        table,
    )?;
    Ok(found > 0)
}

/// Whether that table already carries this index.
///
/// The same question [`column_exists`] asks, about the other thing an ALTER can
/// add. The table and its columns are there either way, so nothing above
/// reaches an index — and `CREATE INDEX` on a name that already exists is
/// refused, so an interrupted one has to be recognized by what it left or the
/// next start wedges on it for ever.
async fn index_exists(pool: &MySqlPool, table: &str, index: &str) -> Result<bool, MigrateError> {
    let found: i64 = fail_as(
        sqlx::query_scalar(
            "SELECT COUNT(*) FROM information_schema.statistics
             WHERE table_schema = DATABASE() AND table_name = ? AND index_name = ?",
        )
        .bind(table)
        .bind(index)
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
        "0006_handover",
        "0007_drop_minted",
        "0008_entity",
        "0009_entity_alias",
        "0010_fact",
        "0011_fact_event_metadata",
        "0012_fact_event_ref",
        "0013_type_field",
        "0014_message_delivery",
        "0015_type_field_origin",
        "0016_field_write",
        "0017_type_field_folds",
        "0018_type_field_holds_wider",
        "0019_entity_source_wider",
        "0020_entity_crm_wider",
        "0021_kind",
    ];

    /// **A migration set of this test's own, carrying the shape no shipped
    /// migration has.** A column arrives on a table that was already there,
    /// and a second migration fills it.
    ///
    /// It is written here rather than added to [`MIGRATIONS`] because this
    /// file ships the shapes and not users of them: the first real backfill
    /// arrives with the feature that needs one.
    const BACKFILL: &[Migration] = &[
        Migration {
            version: "t0001_pet",
            sql: "CREATE TABLE pet (name VARCHAR(32) NOT NULL PRIMARY KEY)",
            leaves: Leaves::Table("pet"),
        },
        Migration {
            version: "t0002_pet_owner",
            sql: "ALTER TABLE pet ADD COLUMN owner VARCHAR(32)",
            leaves: Leaves::Column("pet", "owner"),
        },
        Migration {
            version: "t0003_pet_owner_backfill",
            sql: "UPDATE pet SET owner = 'bart' WHERE owner IS NULL",
            leaves: Leaves::NoRows("pet", "owner IS NULL"),
        },
    ];

    /// **An interrupted backfill is recognized by no row being left unfilled.**
    ///
    /// A backfill changes rows and leaves the schema exactly as it found it, so
    /// every question the runner can ask about a schema answers the same before
    /// and after. Given the nearest shape — the column — it answers "already
    /// reached" from the moment the migration BEFORE it added that column. The
    /// runner then records the backfill as landed and skips it, and rows that
    /// were never filled sit behind a ledger saying they were, with nothing
    /// downstream disagreeing.
    ///
    /// So the question has to reach the rows: is any row still unfilled.
    ///
    /// Both directions, because either alone is half a test. Unfilled rows
    /// remaining means the backfill is applied; none remaining means it is
    /// recorded rather than re-issued.
    #[tokio::test]
    async fn an_interrupted_backfill_is_recognized_by_no_row_being_left_unfilled() {
        let scratch = Scratch::new("migrate-interrupted-backfill");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("backfilledpartway")
            .await
            .expect("a database of its own");

        // Everything up to the backfill, applied normally.
        assert_eq!(
            apply(&pool, &BACKFILL[..2])
                .await
                .expect("the table and the column"),
            vec!["t0001_pet".to_string(), "t0002_pet_owner".to_string()],
        );
        // …and a row for the backfill to have work to do on.
        sqlx::query("INSERT INTO pet (name) VALUES ('santas-little-helper')")
            .execute(&pool)
            .await
            .expect("the row lands");

        // The state a death in the window leaves when the statement did NOT
        // take effect: the marker committed, the ledger silent, the rows still
        // unfilled.
        mark_begun(&pool, "t0003_pet_owner_backfill")
            .await
            .expect("the marker lands");

        assert_eq!(
            apply(&pool, BACKFILL)
                .await
                .expect("the start completes the schema"),
            vec!["t0003_pet_owner_backfill".to_string()],
            "an interrupted backfill with rows still unfilled is applied",
        );
        let owners: Vec<Option<String>> = sqlx::query_scalar("SELECT owner FROM pet")
            .fetch_all(&pool)
            .await
            .expect("the table is readable");
        assert_eq!(
            owners,
            vec![Some("bart".to_string())],
            "…and the rows it was for are really filled, which is what the ledger would \
             otherwise be swearing to on its own",
        );

        // The other direction, and it is what makes the first one mean
        // anything. The same marker over rows that ARE filled says the
        // statement landed, so the version is recorded and nothing is applied.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("t0003_pet_owner_backfill")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "t0003_pet_owner_backfill")
            .await
            .expect("the marker lands");

        let applied = apply(&pool, BACKFILL)
            .await
            .expect("the start completes the schema");
        assert!(
            applied.is_empty(),
            "an interrupted backfill that had landed is recorded, not re-issued: {applied:?}"
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.iter().any(|v| v == "t0003_pet_owner_backfill"),
            "…and the ledger says so, so the next start asks nothing: {recorded:?}"
        );

        store.stop().await;
    }

    /// **A second set of this test's own, for the other shape no shipped
    /// migration has**: an index put on a table that was already there.
    ///
    /// **Two tables carry an index of the SAME NAME, and that is the point of
    /// the second one.** An index name is scoped to its table in this store —
    /// `owner_idx` on `kennel` and `owner_idx` on `pet` are both legal, and
    /// this schema already ships `in_order` on two tables — so the table is
    /// the whole of what the question discriminates on. With one table in
    /// this list, every assertion below holds just as well on a build whose
    /// probe never looks at the table it was handed, and the shape would be
    /// answering "some index somewhere has that name".
    ///
    /// The decoy is applied BEFORE the migration under test, so a probe that
    /// ignores the table finds it and reports a change that has not happened.
    const INDEXED: &[Migration] = &[
        Migration {
            version: "t0001_pet",
            sql: "CREATE TABLE pet (name VARCHAR(32) NOT NULL PRIMARY KEY, owner VARCHAR(32))",
            leaves: Leaves::Table("pet"),
        },
        Migration {
            version: "t0002_kennel",
            sql: "CREATE TABLE kennel (name VARCHAR(32) NOT NULL PRIMARY KEY, owner VARCHAR(32))",
            leaves: Leaves::Table("kennel"),
        },
        Migration {
            version: "t0003_kennel_owner_index",
            sql: "CREATE UNIQUE INDEX owner_idx ON kennel (owner)",
            leaves: Leaves::Index("kennel", "owner_idx"),
        },
        Migration {
            version: "t0004_pet_owner_index",
            sql: "CREATE UNIQUE INDEX owner_idx ON pet (owner)",
            leaves: Leaves::Index("pet", "owner_idx"),
        },
    ];

    /// **An interrupted CREATE INDEX is recognized by the index being there.**
    ///
    /// An index changes neither the table nor its columns, so both questions
    /// about the schema answer "yes" from the moment the table exists. A runner
    /// asking either of them about an interrupted index concludes it landed and
    /// records the version — leaving a table with no index and a ledger saying
    /// it has one, which is what makes a unique index an identity rather than a
    /// suggestion.
    ///
    /// The other way round is the permanent wedge the marker exists to end:
    /// `CREATE INDEX` on a name that already exists is refused, exactly as
    /// `ADD COLUMN` is, so a start that re-issues it fails on every start after
    /// it too.
    ///
    /// **A second table carries an index of the same name throughout**, so the
    /// question has to discriminate on the table and not merely on the name.
    /// Without it every assertion here passes on a build whose probe ignores
    /// the table it was handed.
    ///
    /// Both directions, built with the runner's own step.
    #[tokio::test]
    async fn an_interrupted_create_index_is_recognized_by_the_index_being_there() {
        let scratch = Scratch::new("migrate-interrupted-index");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("indexedpartway")
            .await
            .expect("a database of its own");

        // Both tables and the decoy index, applied normally. The decoy is the
        // one that carries the name the migration under test is about.
        assert_eq!(
            apply(&pool, &INDEXED[..3])
                .await
                .expect("the tables and the index beside the one under test"),
            vec![
                "t0001_pet".to_string(),
                "t0002_kennel".to_string(),
                "t0003_kennel_owner_index".to_string(),
            ],
        );

        // The state a death in the window leaves when the statement did NOT
        // take effect: the marker committed, the ledger silent, and `pet`
        // without the index — while `kennel` carries one of that very name.
        mark_begun(&pool, "t0004_pet_owner_index")
            .await
            .expect("the marker lands");

        assert_eq!(
            apply(&pool, INDEXED)
                .await
                .expect("the start completes the schema"),
            vec!["t0004_pet_owner_index".to_string()],
            "an interrupted index that did NOT land is applied, and the same name standing on \
             another table is not it",
        );
        assert!(
            index_exists(&pool, "pet", "owner_idx")
                .await
                .expect("the schema is readable"),
            "…and the index the migration is for is really there afterwards",
        );
        // **Read by a route that is not the function under test.** The
        // assertion above asks the probe whether the probe's own subject is
        // there; this asks the store to enforce what the index is FOR. A
        // unique index that is merely recorded refuses nothing.
        sqlx::query("INSERT INTO pet (name, owner) VALUES ('santas-little-helper', 'bart')")
            .execute(&pool)
            .await
            .expect("the first row lands");
        let doubled = sqlx::query("INSERT INTO pet (name, owner) VALUES ('snowball', 'bart')")
            .execute(&pool)
            .await;
        assert!(
            doubled.is_err(),
            "the unique index really constrains the table it was built on: {doubled:?}"
        );

        // The other direction. The same marker over an index that IS there
        // says the statement landed — and the run completing at all is half
        // the point, because re-issuing `CREATE INDEX` on that name is refused
        // and would wedge every start from here on.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("t0004_pet_owner_index")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "t0004_pet_owner_index")
            .await
            .expect("the marker lands");

        let applied = apply(&pool, INDEXED)
            .await
            .expect("the start completes the schema");
        assert!(
            applied.is_empty(),
            "an interrupted index that had landed is recorded, not re-issued: {applied:?}"
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.iter().any(|v| v == "t0004_pet_owner_index"),
            "…and the ledger says so, so the next start asks nothing: {recorded:?}"
        );

        store.stop().await;
    }

    /// **A third set of this test's own**: a column that changes type rather
    /// than arriving.
    ///
    /// **The other column of the table already has the type the widening
    /// produces**, so the question has to discriminate on the column name and
    /// not merely on that type standing somewhere in the table. Sized the
    /// other way round, every assertion below passes on a build whose probe
    /// never reads the column it was handed.
    const WIDENED: &[Migration] = &[
        Migration {
            version: "t0001_pet",
            sql: "CREATE TABLE pet (name VARCHAR(64) NOT NULL PRIMARY KEY, owner VARCHAR(8))",
            leaves: Leaves::Table("pet"),
        },
        Migration {
            version: "t0002_pet_owner_wider",
            sql: "ALTER TABLE pet MODIFY COLUMN owner VARCHAR(64)",
            leaves: Leaves::ColumnType("pet", "owner", "varchar(64)"),
        },
    ];

    /// **An interrupted widening is recognized by the column's declared type.**
    ///
    /// A statement that changes a column's type leaves the table there and the
    /// column there, so both questions above answer "yes" before it and after
    /// it. The runner asking either about an interrupted widening records a
    /// version whose change never landed, and the column stays the width it
    /// was — behind a ledger saying otherwise, permanently, because no later
    /// start asks again. Every write that needed the new width then fails, and
    /// the schema and the ledger disagree with nothing to reconcile them.
    ///
    /// So the question is the column's declared type rather than its presence.
    ///
    /// Both directions, built with the runner's own step.
    #[tokio::test]
    async fn an_interrupted_widening_is_recognized_by_the_columns_type() {
        let scratch = Scratch::new("migrate-interrupted-widening");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("widenedpartway")
            .await
            .expect("a database of its own");

        // The table, applied normally, with the column at its first width.
        assert_eq!(
            apply(&pool, &WIDENED[..1]).await.expect("the table"),
            vec!["t0001_pet".to_string()],
        );

        // The state a death in the window leaves when the statement did NOT
        // take effect: the marker committed, the ledger silent, the column
        // still narrow — and the table and the column both there, which is all
        // the older questions can see.
        mark_begun(&pool, "t0002_pet_owner_wider")
            .await
            .expect("the marker lands");

        assert_eq!(
            apply(&pool, WIDENED)
                .await
                .expect("the start completes the schema"),
            vec!["t0002_pet_owner_wider".to_string()],
            "an interrupted widening that did NOT land is applied",
        );
        // **Read by a route that is not the probe.** A width the store agrees
        // to is a width that takes a value of that size; a probe agreeing with
        // itself is not evidence.
        sqlx::query("INSERT INTO pet (name, owner) VALUES ('santas-little-helper', ?)")
            .bind("x".repeat(40))
            .execute(&pool)
            .await
            .expect("…and the column really takes a value the old width refused");

        // The other direction: the same marker over a column that IS the new
        // type says the statement landed.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("t0002_pet_owner_wider")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "t0002_pet_owner_wider")
            .await
            .expect("the marker lands");

        let applied = apply(&pool, WIDENED)
            .await
            .expect("the start completes the schema");
        assert!(
            applied.is_empty(),
            "an interrupted widening that had landed is recorded, not re-issued: {applied:?}"
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.iter().any(|v| v == "t0002_pet_owner_wider"),
            "…and the ledger says so, so the next start asks nothing: {recorded:?}"
        );

        store.stop().await;
    }

    /// **Every probe answers about the object it was handed, and about no
    /// other.**
    ///
    /// A probe is a `COUNT(*)` over a schema view, and a view holds every
    /// database on the server and every table in each. So each probe carries
    /// predicates that narrow it — the database, the table, the column, the
    /// name — and **a predicate that goes missing does not make a probe fail;
    /// it makes one answer about somebody else's object.** The runner then
    /// records a migration whose change never landed, which is silent and
    /// permanent because no later start asks again.
    ///
    /// The interruption tests cannot reach this. Each builds the one schema
    /// its own migration is about, so there is nothing else for a widened
    /// probe to find, and every one of them passes on a build whose probe
    /// reads none of what it was handed.
    ///
    /// **So this builds the near misses on purpose.** A second database on the
    /// same server holds the same names; a second table in this database holds
    /// the same column and index names. Every negative here is an object that
    /// exists — just not the one that was asked about — so an assertion goes
    /// red exactly when the predicate that excludes it goes missing.
    #[tokio::test]
    async fn a_probe_answers_about_the_object_it_was_handed() {
        let scratch = Scratch::new("migrate-probe-scope");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let asked = store
            .database("asked")
            .await
            .expect("the database the probes are pointed at");
        let elsewhere = store
            .database("notasked")
            .await
            .expect("a second database on the same server");

        // In the OTHER database: every name this test asks about, so a probe
        // that does not scope to one database finds them all.
        for statement in [
            "CREATE TABLE kennel (name VARCHAR(32) NOT NULL PRIMARY KEY, owner VARCHAR(64))",
            "CREATE UNIQUE INDEX owner_idx ON kennel (owner)",
        ] {
            sqlx::raw_sql(statement)
                .execute(&elsewhere)
                .await
                .expect("the other database's schema lands");
        }
        // In THIS database: the subject, and neighbours built so that every
        // predicate has something to exclude.
        //
        // `bowl` holds neither the column nor the index, so a probe that does
        // not scope to a table answers for the pair — and its primary key is
        // an index of its own, so one that does not scope to an index NAME
        // answers too. `pet` carries a second column of the type the subject
        // is asked about, and `basket` carries a column of the subject's NAME
        // at that same type: between them, a type question that skips the
        // column or skips the table finds a match that is not its own.
        for statement in [
            "CREATE TABLE pet (name VARCHAR(32) NOT NULL PRIMARY KEY, owner VARCHAR(8), \
             sound VARCHAR(64))",
            "CREATE INDEX owner_idx ON pet (owner)",
            "CREATE TABLE bowl (name VARCHAR(32) NOT NULL PRIMARY KEY)",
            "CREATE TABLE basket (name VARCHAR(32) NOT NULL PRIMARY KEY, owner VARCHAR(64))",
        ] {
            sqlx::raw_sql(statement)
                .execute(&asked)
                .await
                .expect("this database's schema lands");
        }

        // **Is this table here** — and a table of that name in another
        // database is not this database's table.
        assert!(
            object_exists(&asked, "pet")
                .await
                .expect("the schema is readable"),
            "the table this database really holds",
        );
        assert!(
            !object_exists(&asked, "kennel")
                .await
                .expect("the schema is readable"),
            "a table of that name in another database is not this one's",
        );

        // **Does this table hold this column** — not another database's table
        // of the same name, and not another table of this database.
        assert!(
            column_exists(&asked, "pet", "owner")
                .await
                .expect("the schema is readable"),
            "the column the table really holds",
        );
        assert!(
            !column_exists(&asked, "kennel", "owner")
                .await
                .expect("the schema is readable"),
            "another database's table holds it, and that is not this question",
        );
        assert!(
            !column_exists(&asked, "bowl", "owner")
                .await
                .expect("the schema is readable"),
            "a neighbouring table of this database holds it, and that is not this question",
        );

        // **Does this table carry this index** — an index name is scoped to
        // its table, so the table is the whole discrimination.
        assert!(
            index_exists(&asked, "pet", "owner_idx")
                .await
                .expect("the schema is readable"),
            "the index the table really carries",
        );
        assert!(
            !index_exists(&asked, "kennel", "owner_idx")
                .await
                .expect("the schema is readable"),
            "another database's table carries that name, and that is not this question",
        );
        assert!(
            !index_exists(&asked, "bowl", "owner_idx")
                .await
                .expect("the schema is readable"),
            "a neighbouring table of this database does not carry it",
        );

        // **Is this column declared as this type** — the type is asked of one
        // column of one table of one database. The other database's column of
        // the same name is the type this one is NOT, which is the near miss a
        // widening actually meets.
        assert!(
            column_is(&asked, "pet", "owner", "varchar(8)")
                .await
                .expect("the schema is readable"),
            "the type the column really has",
        );
        assert!(
            !column_is(&asked, "pet", "owner", "varchar(64)")
                .await
                .expect("the schema is readable"),
            "…and not the type it does not have",
        );
        assert!(
            !column_is(&asked, "kennel", "owner", "varchar(64)")
                .await
                .expect("the schema is readable"),
            "another database's column has that type, and that is not this question",
        );

        store.stop().await;
    }

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

    /// **An interrupted ADD COLUMN is recognized by the column being there.**
    ///
    /// This is the shape the runner could not survive. A creation asks "is the
    /// table there" and an ALTER answers yes whichever way it went, so a start
    /// after an interruption re-issues `ADD COLUMN` against a column that
    /// already exists — and this store refuses that, and refuses it again on
    /// every later start. `ADD COLUMN IF NOT EXISTS` is a syntax error here, so
    /// there is no idempotent form to fall back on: asking about the column is
    /// the only thing that ends the wedge.
    ///
    /// Built with the runner's own step, so this is the state a real
    /// interruption leaves rather than an imagined one.
    #[tokio::test]
    async fn an_interrupted_add_column_is_recognized_by_the_column_being_there() {
        let scratch = Scratch::new("migrate-interrupted-alter");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("alteredpartway")
            .await
            .expect("a database of its own");

        run(&pool).await.expect("the schema");
        // The state a death in the window leaves: the ledger row taken away,
        // the marker committed, the column already there because the statement
        // itself had succeeded.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("0015_type_field_origin")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "0015_type_field_origin")
            .await
            .expect("the marker lands");

        let applied = run(&pool).await.expect("the start completes the schema");
        assert!(
            applied.is_empty(),
            "an interrupted alter that had landed is recorded, not re-issued: {applied:?}"
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.iter().any(|v| v == "0015_type_field_origin"),
            "…and the ledger says so, so the next start asks nothing: {recorded:?}"
        );

        // **The other direction, and it is what makes the first one mean
        // anything.** A death BEFORE the statement took effect leaves the same
        // marker over a table that is still there — so a runner asking about
        // the table answers "it landed" for a change that did not, records the
        // version, and leaves a schema missing a column with a ledger saying it
        // has one. Nothing later repairs that, and every read of the column
        // fails.
        sqlx::raw_sql("ALTER TABLE type_field DROP COLUMN origin")
            .execute(&pool)
            .await
            .expect("the column goes");
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("0015_type_field_origin")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "0015_type_field_origin")
            .await
            .expect("the marker lands");

        assert_eq!(
            run(&pool).await.expect("the start completes the schema"),
            vec!["0015_type_field_origin".to_string()],
            "an interrupted alter that did NOT land is applied",
        );
        assert!(
            column_exists(&pool, "type_field", "origin")
                .await
                .expect("the schema is readable"),
            "…and the column the migration is for is really there afterwards",
        );

        store.stop().await;
    }

    /// **An interrupted DROP is recognized by what it leaves, which is
    /// nothing.**
    ///
    /// Every migration until now created a table, so "did it land" was "is the
    /// table there". A drop reaches the opposite state, and a runner asking the
    /// creation question about it answers `false` for a statement that fully
    /// succeeded — then re-issues `DROP TABLE` against a table that is already
    /// gone, and the store refuses it. That is the same permanent wedge the
    /// begun marker was built to end, arriving through the one shape the marker
    /// did not know about.
    ///
    /// Built with the runner's own step, so this is the state a real
    /// interruption leaves rather than an imagined one.
    #[tokio::test]
    async fn an_interrupted_drop_is_recognized_by_the_table_being_gone() {
        let scratch = Scratch::new("migrate-interrupted-drop");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("droppedpartway")
            .await
            .expect("a database of its own");

        // Everything up to the drop, applied normally.
        run(&pool).await.expect("the schema");
        // Then the drop is put back into the state a death in the window leaves:
        // the ledger row taken away, the marker committed, the table already
        // gone because the statement itself had succeeded.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("0007_drop_minted")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "0007_drop_minted")
            .await
            .expect("the marker lands");

        let applied = run(&pool).await.expect("the start completes the schema");
        assert!(
            applied.is_empty(),
            "an interrupted drop that had landed is recorded, not re-issued: {applied:?}"
        );
        let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
            .fetch_all(&pool)
            .await
            .expect("the ledger is readable");
        assert!(
            recorded.iter().any(|v| v == "0007_drop_minted"),
            "…and the ledger says so, so the next start asks nothing: {recorded:?}"
        );
        assert!(
            !object_exists(&pool, "minted")
                .await
                .expect("the schema is readable"),
            "the table stays gone — a recovery that put it back would be the drop undone"
        );

        store.stop().await;
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

    /// **Every migration's declared shape is one its own statement reaches.**
    ///
    /// A shape is written beside a migration by hand, and nothing until here
    /// compared the two. A shape naming something the statement does not
    /// produce — a type the column is not, a column the ALTER did not add, a
    /// table spelled wrong — goes unnoticed while migrations run in order,
    /// because the question is asked only of a migration that was interrupted.
    /// It then answers `false` for a change that really landed, and the start
    /// re-issues a statement the store has already taken.
    ///
    /// It covers every shape at once and needs nothing remembered: the store's
    /// own rendering of a declared type is compared against the constant
    /// rather than recalled, so the next column that is not a plain `varchar`
    /// is covered by this the day it is added.
    ///
    /// **The question is asked the moment each migration runs, never after the
    /// whole run.** A shape says what ITS statement leaves, and a later
    /// migration may take that away again — `0007` drops the table `0003`
    /// creates, so a check at the end would report a defect on a pair that is
    /// working exactly as written.
    #[tokio::test]
    async fn every_migrations_declared_shape_is_reached_once_it_has_run() {
        let scratch = Scratch::new("migrate-shapes-reached");
        let mut store = crate::dolt::Dolt::start(&scratch.0, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("shapesreached")
            .await
            .expect("a database of its own");

        for (last, migration) in MIGRATIONS.iter().enumerate() {
            assert_eq!(
                apply(&pool, &MIGRATIONS[..=last])
                    .await
                    .expect("the schema moves"),
                vec![migration.version.to_string()],
                "each round applies exactly the one migration it added",
            );
            assert!(
                migration
                    .leaves
                    .reached(&pool)
                    .await
                    .expect("the schema is readable"),
                "{} declares a state its own statement does not leave behind, so an \
                 interruption would re-issue a statement that had already landed",
                migration.version,
            );
        }

        store.stop().await;
    }

    /// **The shape migration 0018 declares is the one that recovers it.**
    ///
    /// The interruption cases for the other shapes drive migration lists this
    /// module writes for itself. Those prove a shape works and NOT that any
    /// shipped migration asks for it: put 0018 back to the column shape this
    /// widening exists to replace and every one of them stays green, because
    /// none of them reads that line.
    ///
    /// So this drives the real list. The widening is put back into the state a
    /// death in the window leaves — the column at its old type, the ledger row
    /// gone, the marker committed — and the start has to apply it again. The
    /// column shape answers "already reached" here, because 0013 created the
    /// column; only the type shape answers for what 0018 does.
    #[tokio::test]
    async fn an_interrupted_shipped_widening_is_recognized_by_the_columns_type() {
        let scratch = Scratch::new("migrate-interrupted-shipped-widening");
        let path = scratch.0.clone();
        std::mem::forget(scratch);
        let mut store = crate::dolt::Dolt::start(&path, free_port())
            .await
            .expect("the store comes up");
        let pool = store
            .database("shippedwidening")
            .await
            .expect("a database of its own");

        run(&pool).await.expect("the schema");

        // The state a death in the window leaves when the statement did NOT
        // take effect: the column back at the type 0013 gave it, the ledger
        // row taken away, the marker committed.
        sqlx::raw_sql("ALTER TABLE type_field MODIFY COLUMN holds VARCHAR(16) NOT NULL")
            .execute(&pool)
            .await
            .expect("the column goes back to its old type");
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind("0018_type_field_holds_wider")
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, "0018_type_field_holds_wider")
            .await
            .expect("the marker lands");

        assert_eq!(
            run(&pool).await.expect("the start completes the schema"),
            vec!["0018_type_field_holds_wider".to_string()],
            "the shipped widening that did NOT land is applied",
        );

        // **Read by a route that is not the probe**: the declaration that
        // overflows the old width is what the widening is FOR, so the store
        // taking it is the evidence. A probe agreeing with itself is not.
        sqlx::query(
            "INSERT INTO type_field (type_name, key_name, ordinal, holds)
             VALUES ('contract-stay', 'part_of', 0, ?)",
        )
        .bind("reference:project")
        .execute(&pool)
        .await
        .expect("…and the cell takes the token the old width refused");

        store.stop().await;
    }
}
