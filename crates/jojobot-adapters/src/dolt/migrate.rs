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
    /// **Its first user takes a name back**: the keys under `rhythm` belonged
    /// to a declared type and belong to the kind, and the type's rows have to
    /// go before the seed can write the kind's. That is rows changed and a
    /// schema untouched, which is exactly the shape this answers for.
    ///
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
    /// **The lineage walk is what made this shape real**: a claim's pointer at
    /// its source is a column pair the store had to select on, and the index
    /// over that pair is the first migration here that leaves an index rather
    /// than a table or a column.
    Index(&'static str, &'static str),
    /// The index this statement removes. **The same question as [`Index`],
    /// asked the other way round** — the same pairing [`NoTable`] is to
    /// [`Table`].
    ///
    /// A `DROP` before an `ADD` of the same name needs this: `Index` alone
    /// answers "yes" for the old index that is still there when the drop has
    /// not landed, so a start after an interruption cannot tell "not yet
    /// dropped" from "already dropped, and the add is what is missing." This
    /// asks the one the add's own [`Index`] check cannot: whether the name is
    /// gone.
    NoIndex(&'static str, &'static str),
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
            | Leaves::NoIndex(t, _)
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
            Leaves::NoIndex(t, i) => !index_exists(pool, t, i).await?,
            Leaves::ColumnType(t, c, declared) => column_is(pool, t, c, declared).await?,
        })
    }
}

/// **The step a boot takes after the schema**: write the kinds this build
/// ships, then load what the store answers with, so this process can read a
/// handle at all.
///
/// It sits beside the schema rather than inside a store's `open`, because a
/// load hidden in `open` would make a caller that happens to touch memory look
/// seeded while a caller that touches only the mail rail does not — and no
/// handle's refusal can tell those two apart.
pub async fn seed_kinds(pool: &MySqlPool) -> Result<usize, jojobot_domain::memory::MemoryError> {
    jojobot_domain::memory::kinds::seed(&crate::dolt::memory::DoltMemory::open(pool.clone())).await
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
    Migration {
        version: "0022_type_field_owner",
        sql: include_str!("../../migrations/0022_type_field_owner.sql"),
        leaves: Leaves::Column("type_field", "owner"),
    },
    Migration {
        version: "0023_type_field_required",
        sql: include_str!("../../migrations/0023_type_field_required.sql"),
        leaves: Leaves::Column("type_field", "required"),
    },
    Migration {
        version: "0024_rhythm_kind_takes_its_name",
        sql: include_str!("../../migrations/0024_rhythm_kind_takes_its_name.sql"),
        leaves: Leaves::NoRows("type_field", "type_name = 'rhythm' AND owner = 'type'"),
    },
    Migration {
        version: "0025_type_field_one_of",
        sql: include_str!("../../migrations/0025_type_field_one_of.sql"),
        leaves: Leaves::Column("type_field", "one_of"),
    },
    Migration {
        version: "0026_session_timezone",
        sql: include_str!("../../migrations/0026_session_timezone.sql"),
        leaves: Leaves::Column("session", "timezone"),
    },
    Migration {
        version: "0027_fact_inserted_at",
        sql: include_str!("../../migrations/0027_fact_inserted_at.sql"),
        leaves: Leaves::Column("fact", "inserted_at"),
    },
    Migration {
        version: "0028_fact_stale_after",
        sql: include_str!("../../migrations/0028_fact_stale_after.sql"),
        leaves: Leaves::Column("fact", "stale_after"),
    },
    Migration {
        version: "0029_fact_by_source",
        sql: include_str!("../../migrations/0029_fact_by_source.sql"),
        leaves: Leaves::Index("fact", "by_source"),
    },
    Migration {
        version: "0030_session_stated_day",
        sql: include_str!("../../migrations/0030_session_stated_day.sql"),
        leaves: Leaves::Column("session", "started_on"),
    },
    Migration {
        version: "0031_journal_entry_day",
        sql: include_str!("../../migrations/0031_journal_entry_day.sql"),
        leaves: Leaves::Column("journal_entry", "happened_on"),
    },
    Migration {
        version: "0032_entity_badge",
        sql: include_str!("../../migrations/0032_entity_badge.sql"),
        leaves: Leaves::Column("entity", "badge"),
    },
    Migration {
        version: "0033_entity_merged_into",
        sql: include_str!("../../migrations/0033_entity_merged_into.sql"),
        leaves: Leaves::Column("entity", "merged_into"),
    },
    Migration {
        version: "0034_fact_write",
        sql: include_str!("../../migrations/0034_fact_write.sql"),
        leaves: Leaves::Table("fact_write"),
    },
    Migration {
        version: "0035_fact_write_backfill",
        sql: include_str!("../../migrations/0035_fact_write_backfill.sql"),
        // **The rows, not the schema.** A backfill leaves the schema exactly as
        // it found it, so the nearest shape answers "already reached" from the
        // moment the table existed. This asks whether any claim is still
        // without a write of its own.
        leaves: Leaves::NoRows(
            "fact",
            "NOT EXISTS (SELECT 1 FROM fact_write w WHERE w.entity = fact.entity \
             AND w.fact_id = fact.id)",
        ),
    },
    Migration {
        version: "0036_fact_write_moment",
        sql: include_str!("../../migrations/0036_fact_write_moment.sql"),
        leaves: Leaves::Column("fact_write", "written_at"),
    },
    Migration {
        version: "0037_fact_happened_at",
        sql: include_str!("../../migrations/0037_fact_happened_at.sql"),
        leaves: Leaves::Column("fact", "happened_at"),
    },
    Migration {
        version: "0038_fact_write_happened_at",
        sql: include_str!("../../migrations/0038_fact_write_happened_at.sql"),
        leaves: Leaves::Column("fact_write", "happened_at"),
    },
    Migration {
        version: "0039_fact_recorded_at",
        sql: include_str!("../../migrations/0039_fact_recorded_at.sql"),
        // **The column it leaves, not the one it takes away.** A rename is one
        // statement and the question is the one an ADD asks: is the name this
        // migration produces already there.
        leaves: Leaves::Column("fact", "recorded_at"),
    },
    Migration {
        version: "0040_fact_write_recorded_at",
        sql: include_str!("../../migrations/0040_fact_write_recorded_at.sql"),
        leaves: Leaves::Column("fact_write", "recorded_at"),
    },
    Migration {
        version: "0041_session_teaching",
        sql: include_str!("../../migrations/0041_session_teaching.sql"),
        leaves: Leaves::Table("session_teaching"),
    },
    Migration {
        version: "0042_entity_former_handle",
        sql: include_str!("../../migrations/0042_entity_former_handle.sql"),
        leaves: Leaves::Table("entity_former_handle"),
    },
    Migration {
        version: "0043_message_sender_mail_waiting",
        sql: include_str!("../../migrations/0043_message_sender_mail_waiting.sql"),
        leaves: Leaves::Column("message", "sender_mail_waiting_at_send"),
    },
    Migration {
        version: "0044_fact_stands_for",
        sql: include_str!("../../migrations/0044_fact_stands_for.sql"),
        leaves: Leaves::Table("fact_stands_for"),
    },
    Migration {
        version: "0045_entity_former_handle_width",
        sql: include_str!("../../migrations/0045_entity_former_handle_width.sql"),
        leaves: Leaves::ColumnType("entity_former_handle", "former_handle", "varchar(191)"),
    },
    Migration {
        version: "0045_entity_former_handle_ordinal",
        sql: include_str!("../../migrations/0045_entity_former_handle_ordinal.sql"),
        leaves: Leaves::Column("entity_former_handle", "ordinal"),
    },
    Migration {
        version: "0045_entity_former_handle_key_drop",
        sql: include_str!("../../migrations/0045_entity_former_handle_key_drop.sql"),
        leaves: Leaves::NoIndex("entity_former_handle", "PRIMARY"),
    },
    Migration {
        version: "0045_entity_former_handle_key_add",
        sql: include_str!("../../migrations/0045_entity_former_handle_key_add.sql"),
        leaves: Leaves::Index("entity_former_handle", "PRIMARY"),
    },
    Migration {
        version: "0046_fact_status_archived",
        sql: include_str!("../../migrations/0046_fact_status_archived.sql"),
        leaves: Leaves::NoRows("fact", "status IN ('superseded', 'retracted', 'negated')"),
    },
    Migration {
        version: "0047_fact_write_status_archived",
        sql: include_str!("../../migrations/0047_fact_write_status_archived.sql"),
        leaves: Leaves::NoRows(
            "fact_write",
            "status IN ('superseded', 'retracted', 'negated')",
        ),
    },
    Migration {
        version: "0048_session_served_chars",
        sql: include_str!("../../migrations/0048_session_served_chars.sql"),
        leaves: Leaves::Column("session", "served_chars"),
    },
    Migration {
        version: "0049_entity_archived",
        sql: include_str!("../../migrations/0049_entity_archived.sql"),
        leaves: Leaves::Column("entity", "archived_reason"),
    },
    Migration {
        version: "0050_entity_archived_at",
        sql: include_str!("../../migrations/0050_entity_archived_at.sql"),
        leaves: Leaves::Column("entity", "archived_at"),
    },
    Migration {
        version: "0051_entity_write",
        sql: include_str!("../../migrations/0051_entity_write.sql"),
        leaves: Leaves::Table("entity_write"),
    },
    Migration {
        version: "0052_fact_happened_through",
        sql: include_str!("../../migrations/0052_fact_happened_through.sql"),
        leaves: Leaves::Column("fact", "happened_through"),
    },
    Migration {
        version: "0053_fact_write_happened_through",
        sql: include_str!("../../migrations/0053_fact_write_happened_through.sql"),
        leaves: Leaves::Column("fact_write", "happened_through"),
    },
    Migration {
        version: "0054_session_write",
        sql: include_str!("../../migrations/0054_session_write.sql"),
        leaves: Leaves::Table("session_write"),
    },
    Migration {
        version: "0055_message_posted_by_session",
        sql: include_str!("../../migrations/0055_message_posted_by_session.sql"),
        leaves: Leaves::Column("message", "posted_by_session"),
    },
    Migration {
        version: "0056_session_stated_day",
        sql: include_str!("../../migrations/0056_session_stated_day.sql"),
        leaves: Leaves::Column("session", "stated_day"),
    },
    Migration {
        version: "0057_message_quarantined_by",
        sql: include_str!("../../migrations/0057_message_quarantined_by.sql"),
        leaves: Leaves::Column("message", "quarantined_by"),
    },
    Migration {
        version: "0058_message_quarantined_at",
        sql: include_str!("../../migrations/0058_message_quarantined_at.sql"),
        leaves: Leaves::Column("message", "quarantined_at"),
    },
    Migration {
        version: "0059_message_quarantine_reason",
        sql: include_str!("../../migrations/0059_message_quarantine_reason.sql"),
        leaves: Leaves::Column("message", "quarantine_reason"),
    },
    Migration {
        version: "0060_journal_entry_closing_focus",
        sql: include_str!("../../migrations/0060_journal_entry_closing_focus.sql"),
        leaves: Leaves::Column("journal_entry", "closing_focus"),
    },
    Migration {
        version: "0061_displaced_type_field",
        sql: include_str!("../../migrations/0061_displaced_type_field.sql"),
        leaves: Leaves::Table("displaced_type_field"),
    },
    Migration {
        version: "0062_message_by_sender",
        sql: include_str!("../../migrations/0062_message_by_sender.sql"),
        leaves: Leaves::Index("message", "by_sender"),
    },
    Migration {
        version: "0063_field_link",
        sql: include_str!("../../migrations/0063_field_link.sql"),
        leaves: Leaves::Table("field_link"),
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
mod tests;
