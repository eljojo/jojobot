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
    "0022_type_field_owner",
    "0023_type_field_required",
    "0024_rhythm_kind_takes_its_name",
    "0025_type_field_one_of",
    "0026_session_timezone",
    "0027_fact_inserted_at",
    "0028_fact_stale_after",
    "0029_fact_by_source",
    "0030_session_stated_day",
    "0031_journal_entry_day",
    "0032_entity_badge",
    "0033_entity_merged_into",
    "0034_fact_write",
    "0035_fact_write_backfill",
    "0036_fact_write_moment",
    "0037_fact_happened_at",
    "0038_fact_write_happened_at",
    "0039_fact_recorded_at",
    "0040_fact_write_recorded_at",
    "0041_session_teaching",
    "0042_entity_former_handle",
    "0043_message_sender_mail_waiting",
    "0044_fact_stands_for",
    "0045_entity_former_handle_width",
    "0045_entity_former_handle_ordinal",
    "0045_entity_former_handle_key_drop",
    "0045_entity_former_handle_key_add",
    "0046_fact_status_archived",
    "0047_fact_write_status_archived",
    "0048_session_served_chars",
    "0049_entity_archived",
    "0050_entity_archived_at",
    "0051_entity_write",
    "0052_fact_happened_through",
    "0053_fact_write_happened_through",
    "0054_session_write",
    "0055_message_posted_by_session",
    "0056_session_stated_day",
    "0057_message_quarantined_by",
    "0058_message_quarantined_at",
    "0059_message_quarantine_reason",
    "0060_journal_entry_closing_focus",
    "0061_displaced_type_field",
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

/// 🚨 **The badge column arrives on an `entity` table that already has
/// rows**, which is the only state a real store is ever in when it meets
/// this migration.
///
/// Every other case here migrates a fresh database, so `ADD COLUMN` is only
/// ever asked of an empty table. **A store that has been running is the
/// case that matters and it was the one nothing asked about.**
///
/// **Both halves.** The statement lands rather than being refused, and the
/// rows that were already there come back carrying NULL — which is what the
/// column means by a row written before it existed, and what the fill at
/// startup then reaches.
#[tokio::test]
async fn the_badge_column_lands_on_an_entity_table_that_already_has_rows() {
    let scratch = Scratch::new("migrate-badge-populated");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("badgepopulated")
        .await
        .expect("a database of its own");

    // Everything up to the badge, so the entity table is there without it.
    let upto = MIGRATIONS
        .iter()
        .position(|m| m.version == "0032_entity_badge")
        .expect("the badge migration is in the list");
    apply(&pool, &MIGRATIONS[..upto])
        .await
        .expect("the schema before the badge");
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, boot, prose) \
             VALUES ('person:already-here', 'person', 'Already Here', 'contract-fixture', \
             'on-demand', '')",
    )
    .execute(&pool)
    .await
    .expect("a row from before the column");

    // **Among what applied, rather than the whole list.** What this case is
    // about is `ADD COLUMN` reaching a table that already has rows; which
    // other migrations happen to sit after it is incidental to that. Pinning
    // the whole tail makes every future migration break this case for a
    // reason unrelated to its subject, and a case whose breakage and whose
    // fix are both mechanical teaches people to edit it without reading it.
    //
    // **`ALL_VERSIONS` already pins the list and is what owns that job.**
    let applied = apply(&pool, MIGRATIONS)
        .await
        .expect("the column lands on a table that already has rows");
    assert!(
        applied.contains(&"0032_entity_badge".to_string()),
        "the badge migration did not apply to a populated table: {applied:?}",
    );
    let worn: Option<String> = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind("person:already-here")
        .fetch_one(&pool)
        .await
        .expect("the row is still there");
    assert_eq!(
        worn, None,
        "a row that predates the column came back carrying something, so the fill cannot \
             tell it apart from one it has already reached",
    );
}

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
/// file buys.
///
/// A runner that records a whole set at once leaves the schema half a
/// version with nothing recorded: the tables created before the failing
/// statement are committed anyway, so the retry hits one of them and fails
/// again, for ever, naming a file and saying nothing about the tables
/// underneath it. A person then works out by hand which had landed.
///
/// A failure stops at a file boundary instead. Everything before it is
/// applied AND recorded, the failing one is neither, and clearing the
/// obstruction lets the next run carry on from exactly where it stopped.
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
    let still_begun: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration_begun")
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

        let markers: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration_begun")
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
///
/// **Both interrupted directions ride the same walk, on the same store.**
/// A death can land between the begun-marker and the statement (the
/// change never happened) or between the statement and the ledger row
/// (the change is standing and unrecorded); each migration is put through
/// both before the walk moves on, using the one pool the whole test
/// already pays for.
///
/// **Not-landed** is folded into the round already here: marking a
/// version begun before applying it is exactly the state a death in that
/// window leaves, so the existing `apply` call now runs with the marker
/// already down, and the existing assertions are what proves recovery —
/// a migration whose shape reads "already reached" before its statement
/// ran would see `apply` skip it and return nothing, failing the first
/// assertion below rather than the second.
///
/// **Landed-but-unrecorded** takes the version just applied, removes its
/// ledger row and marks it begun again — the shape now reads true without
/// help, so a correct `apply` records it without touching the store a
/// second time, and a shape that answered "already reached" for the
/// wrong reason would have failed already, above.
///
/// **`NoRows` is the one shape this cannot drive from the schema alone.**
/// Its landed state is "no row violates the condition", which an empty
/// table already satisfies before the statement ever runs — so without a
/// row seeded to violate it first, both directions pass for a reason
/// that has nothing to do with the shape. [`seed_before_recovery_check`]
/// supplies that row by version and panics, naming it, for a `NoRows`
/// migration it does not recognize — a silent skip would read as
/// coverage that is not there.
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

    // Primes the ledger and begun tables with nothing to apply, so
    // `mark_begun` below has a table to write into before the first
    // migration ever runs.
    apply(&pool, &[]).await.expect("the ledger tables exist");

    for (last, migration) in MIGRATIONS.iter().enumerate() {
        seed_before_recovery_check(&pool, migration.version).await;

        // Not landed: a death after the marker committed and before the
        // statement went out.
        mark_begun(&pool, migration.version)
            .await
            .expect("the marker lands");
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

        // Landed but not recorded: a death after the statement committed
        // and before the ledger row went in.
        sqlx::query("DELETE FROM schema_migration WHERE version = ?")
            .bind(migration.version)
            .execute(&pool)
            .await
            .expect("the ledger row goes");
        mark_begun(&pool, migration.version)
            .await
            .expect("the marker lands again");
        assert_eq!(
            apply(&pool, &MIGRATIONS[..=last])
                .await
                .expect("the start completes the schema"),
            Vec::<String>::new(),
            "{} was already landed, so a second start must record it rather than \
                 reissue its statement",
            migration.version,
        );
        let recorded: Option<String> =
            sqlx::query_scalar("SELECT version FROM schema_migration WHERE version = ?")
                .bind(migration.version)
                .fetch_optional(&pool)
                .await
                .expect("the ledger is readable");
        assert_eq!(
            recorded.as_deref(),
            Some(migration.version),
            "{} landed but unrecorded must still end up recorded",
            migration.version,
        );
    }

    store.stop().await;
}

/// **A row that violates a `NoRows` migration's condition, seeded before
/// its recovery is checked.** Every other shape answers "not yet reached"
/// from the schema alone; a backfill or a cleanup leaves the schema
/// exactly as it found it, so only the rows can tell "not yet run" from
/// "ran and found nothing to do" — and an empty table already reads as
/// the second one, for free, which is the answer this seed exists to
/// rule out.
///
/// Every row here is filed under `person:already-here`, the roster
/// handle the badge migration's own test already uses for exactly this
/// "a row from before the change" shape — a fresh handle per migration
/// would need its own roster entry for no reason a reader could see. The
/// walk keeps one growing schema rather than a fresh database per
/// migration, so each row's own id only has to not collide with another
/// seed's in the same table.
///
/// A `NoRows` migration with no arm here panics naming it, rather than
/// silently reading as covered.
async fn seed_before_recovery_check(pool: &MySqlPool, version: &str) {
    match version {
        "0024_rhythm_kind_takes_its_name" => {
            sqlx::query(
                "INSERT INTO type_field (type_name, key_name, ordinal, holds) \
                     VALUES ('rhythm', 'cadence_days', 0, 'int')",
            )
            .execute(pool)
            .await
            .expect("the rhythm type's own row lands");
        }
        "0035_fact_write_backfill" => {
            sqlx::query(
                "INSERT INTO fact (entity, id, content, provenance, status, date) \
                     VALUES ('person:already-here', 's35', 'a claim with no write of its own', \
                     'testimony', 'active', '2026-01-01')",
            )
            .execute(pool)
            .await
            .expect("a claim with no write behind it lands");
        }
        "0046_fact_status_archived" => {
            sqlx::query(
                "INSERT INTO fact (entity, id, content, provenance, status, recorded_at) \
                     VALUES ('person:already-here', 's46', 'a claim overtaken by a later one', \
                     'testimony', 'retracted', '2026-01-01')",
            )
            .execute(pool)
            .await
            .expect("a claim carrying a retired status lands");
        }
        "0047_fact_write_status_archived" => {
            sqlx::query(
                "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, \
                     status, recorded_at) \
                     VALUES ('person:already-here', 's47', 1, 'a claim overtaken by a later one', \
                     'testimony', 'negated', '2026-01-01')",
            )
            .execute(pool)
            .await
            .expect("a write carrying a retired status lands");
        }
        _ => {
            let migration = MIGRATIONS
                .iter()
                .find(|m| m.version == version)
                .expect("the version came from this same list");
            if matches!(migration.leaves, Leaves::NoRows(_, _)) {
                panic!(
                    "{version} is a NoRows migration with no seed registered in \
                         seed_before_recovery_check — without one, its recovery check \
                         cannot tell \"not yet run\" from \"ran and found nothing to do\""
                );
            }
        }
    }
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

/// **The shape `0045_entity_former_handle_key_add` declares is the one
/// that recovers it.**
///
/// The old `0045` was one `ALTER` carrying four clauses — a width change,
/// a new column, and a primary key swap — and its own landed-check asked
/// only about the column. A death after the column landed but before the
/// key swap left the table on the single-column key behind a ledger
/// saying the whole change was in.
///
/// Split into one clause per file, the riskiest pair is the key swap
/// itself: MySQL refuses `ADD PRIMARY KEY` while one already stands, so
/// the drop and the add are two files, and the table carries no primary
/// key between them. This drives the real list, the way `0018`'s own
/// test does. The schema is put back to the state a death between the
/// drop and the add leaves — the key already dropped, the add's ledger
/// row gone, its marker committed — and the start has to finish the swap
/// alone: nothing but the add migration can put a `PRIMARY` index back
/// once the drop has run, so [`Leaves::Index`] answers "not yet reached"
/// here rather than the false "already reached" the old single-column
/// check would have given `Leaves::Column("entity_former_handle",
/// "ordinal")`.
#[tokio::test]
async fn an_interrupted_primary_key_swap_is_recognized_by_whether_the_key_is_there() {
    let scratch = Scratch::new("migrate-interrupted-key-swap");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("keyswappartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the add did NOT take
    // effect: the drop already landed (no primary key stands), the add's
    // ledger row taken away, its marker committed.
    sqlx::raw_sql("ALTER TABLE entity_former_handle DROP PRIMARY KEY")
        .execute(&pool)
        .await
        .expect("the key goes back to dropped");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0045_entity_former_handle_key_add")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0045_entity_former_handle_key_add")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0045_entity_former_handle_key_add".to_string()],
        "the key add that did NOT land is applied on its own, not the whole 0045 replayed",
    );

    // **Read by a route that is not the probe**: the composite key is
    // what the swap is FOR, so the store enforcing it is the evidence. A
    // former handle past the old sixty-four-byte width, written twice at
    // two different ordinals, proves the width and the key together —
    // either one still wrong and this fails: too narrow refuses the
    // insert outright, and a key that never moved off `former_handle`
    // alone refuses the second row as a duplicate.
    let long_handle = format!("person:{}", "a".repeat(100));
    sqlx::query(
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
             VALUES (?, 'badge0001', '2026-01-01', 1)",
    )
    .bind(&long_handle)
    .execute(&pool)
    .await
    .expect("the first event lands at the widened former_handle");
    sqlx::query(
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at, ordinal)
             VALUES (?, 'badge0002', '2026-01-02', 2)",
    )
    .bind(&long_handle)
    .execute(&pool)
    .await
    .expect(
        "…and a second event under the same former handle, at a different ordinal, is kept \
             rather than colliding on a key that never moved",
    );

    store.stop().await;
}

/// **The shape `0024_rhythm_kind_takes_its_name` declares is the one that
/// recovers it.** The statement is a bare `DELETE`, and the schema is
/// identical before it and after it: nothing but the rows themselves can
/// tell "not yet run" from "ran and found nothing to delete", which is
/// exactly what [`Leaves::NoRows`] asks.
#[tokio::test]
async fn an_interrupted_rhythm_kind_delete_is_recognized_by_the_types_row_being_gone() {
    let scratch = Scratch::new("migrate-interrupted-rhythm-kind");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("rhythmkindpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the delete did NOT
    // take effect: the type's row for `rhythm` back in place, the ledger
    // row taken away, the marker committed. `owner` defaults to `type`,
    // which is exactly the row the delete is for.
    sqlx::query(
        "INSERT INTO type_field (type_name, key_name, ordinal, holds)
             VALUES ('rhythm', 'cadence_days', 0, 'int')",
    )
    .execute(&pool)
    .await
    .expect("the type's row for rhythm goes back in");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0024_rhythm_kind_takes_its_name")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0024_rhythm_kind_takes_its_name")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0024_rhythm_kind_takes_its_name".to_string()],
        "the delete that did NOT land is reissued",
    );

    // **Read by a route that is not the probe**: the type's own row for
    // `rhythm` is gone, while a kind's row of the same name still takes
    // a write — proving the delete took its `owner = 'type'` clause
    // rather than the name alone.
    sqlx::query(
        "INSERT INTO type_field (type_name, key_name, ordinal, holds, owner)
             VALUES ('rhythm', 'name', 0, 'varchar(191)', 'kind')",
    )
    .execute(&pool)
    .await
    .expect("the kind's own row for rhythm still takes a write");

    store.stop().await;
}

/// **The shape `0035_fact_write_backfill` declares is the one that
/// recovers it.** The statement is an `INSERT … SELECT` that leaves the
/// schema exactly as it found it, so nothing but the rows can tell "not
/// yet run" from "ran and found nothing to fill".
///
/// **This one cannot use the full list the way `0018`'s test does.**
/// `0035`'s statement names `fact.date`, and `0039_fact_recorded_at`
/// renames that column three migrations later — reissuing `0035`'s own
/// SQL against the finished schema fails on a column that is no longer
/// there, which is a real ordering fact and not a fault in the test. So
/// this drives `MIGRATIONS` up to the point `0035` itself reaches, the
/// way the badge test above does.
#[tokio::test]
async fn an_interrupted_fact_write_backfill_is_recognized_by_a_claim_with_no_write_of_its_own() {
    let scratch = Scratch::new("migrate-interrupted-fact-write-backfill");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factwritebackfillpartway")
        .await
        .expect("a database of its own");

    let upto = MIGRATIONS
        .iter()
        .position(|m| m.version == "0039_fact_recorded_at")
        .expect("the recorded_at rename is in the list");
    apply(&pool, &MIGRATIONS[..upto])
        .await
        .expect("the schema up to and including the backfill");

    // The state a death in the window leaves when the copy did NOT take
    // effect: a claim with no write of its own, the ledger row taken
    // away, the marker committed.
    sqlx::query(
        "INSERT INTO fact (entity, id, content, provenance, status, date)
             VALUES ('person:already-here', 'f1', 'not yet given a write of its own', \
             'testimony', 'active', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .expect("a claim with no write behind it lands");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0035_fact_write_backfill")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0035_fact_write_backfill")
        .await
        .expect("the marker lands");

    assert_eq!(
        apply(&pool, &MIGRATIONS[..upto])
            .await
            .expect("the start completes the schema"),
        vec!["0035_fact_write_backfill".to_string()],
        "the backfill that did NOT land is reissued",
    );

    // **Read by a route that is not the probe**: the claim's first write
    // now carries its content, which is what the backfill is FOR.
    let content: String = sqlx::query_scalar(
        "SELECT content FROM fact_write WHERE entity = 'person:already-here' \
             AND fact_id = 'f1'",
    )
    .fetch_one(&pool)
    .await
    .expect("the claim's first write landed");
    assert_eq!(content, "not yet given a write of its own");

    store.stop().await;
}

/// **The shape `0046_fact_status_archived` declares is the one that
/// recovers it.** The statement is an `UPDATE` that leaves the schema
/// exactly as it found it, so nothing but the rows can tell whether it
/// ran. Unlike `0035`, nothing downstream renames a column this one
/// touches, so the full list is safe to reissue against.
#[tokio::test]
async fn an_interrupted_fact_status_archive_backfill_is_recognized_by_a_retired_status_left_standing()
 {
    let scratch = Scratch::new("migrate-interrupted-fact-status-archived");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factstatusarchivedpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the rewrite did NOT
    // take effect: a claim still carrying one of the retired tokens, the
    // ledger row taken away, the marker committed.
    sqlx::query(
        "INSERT INTO fact (entity, id, content, provenance, status, recorded_at)
             VALUES ('person:already-here', 'f1', 'a claim overtaken by a later one', \
             'testimony', 'retracted', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .expect("a claim carrying a retired status lands");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0046_fact_status_archived")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0046_fact_status_archived")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0046_fact_status_archived".to_string()],
        "the rewrite that did NOT land is reissued",
    );

    // **Read by a route that is not the probe**: the claim's own status
    // column now reads `archived`.
    let status: String = sqlx::query_scalar(
        "SELECT status FROM fact WHERE entity = 'person:already-here' AND id = 'f1'",
    )
    .fetch_one(&pool)
    .await
    .expect("the claim is readable");
    assert_eq!(status, "archived");

    store.stop().await;
}

/// **The same shape, over `fact_write`.** `0047_fact_write_status_archived`
/// repeats `0046`'s rewrite on the table that carries a copy of the
/// status at each write, and needs the same recovery.
#[tokio::test]
async fn an_interrupted_fact_write_status_archive_backfill_is_recognized_by_a_retired_status_left_standing()
 {
    let scratch = Scratch::new("migrate-interrupted-fact-write-status-archived");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factwritestatusarchivedpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the rewrite did NOT
    // take effect: a write still carrying one of the retired tokens, the
    // ledger row taken away, the marker committed.
    sqlx::query(
        "INSERT INTO fact_write (entity, fact_id, ordinal, content, provenance, status, \
             recorded_at)
             VALUES ('person:already-here', 'f1', 1, 'a claim overtaken by a later one', \
             'testimony', 'negated', '2026-01-01')",
    )
    .execute(&pool)
    .await
    .expect("a write carrying a retired status lands");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0047_fact_write_status_archived")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0047_fact_write_status_archived")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0047_fact_write_status_archived".to_string()],
        "the rewrite that did NOT land is reissued",
    );

    // **Read by a route that is not the probe**: the write's own status
    // column now reads `archived`.
    let status: String = sqlx::query_scalar(
        "SELECT status FROM fact_write WHERE entity = 'person:already-here' \
             AND fact_id = 'f1' AND ordinal = 1",
    )
    .fetch_one(&pool)
    .await
    .expect("the write is readable");
    assert_eq!(status, "archived");

    store.stop().await;
}

/// **The shape `0019_entity_source_wider` declares is the one that
/// recovers it.** The column is there either way, so only its declared
/// type tells "not yet widened" from "widened", the same question
/// `0018`'s own test drives for `type_field.holds`.
#[tokio::test]
async fn an_interrupted_entity_source_widening_is_recognized_by_the_columns_type() {
    let scratch = Scratch::new("migrate-interrupted-entity-source-widening");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("entitysourcewidepartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the widening did NOT
    // take effect: the column back at the width 0008 gave it, the
    // ledger row taken away, the marker committed.
    sqlx::raw_sql("ALTER TABLE entity MODIFY COLUMN source VARCHAR(191) NOT NULL")
        .execute(&pool)
        .await
        .expect("the column goes back to its old width");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0019_entity_source_wider")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0019_entity_source_wider")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0019_entity_source_wider".to_string()],
        "the widening that did NOT land is applied",
    );

    // **Read by a route that is not the probe**: a source label past the
    // old hundred-and-ninety-one-byte width is what the widening is FOR.
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, boot, prose)
             VALUES ('person:already-here', 'person', 'Already Here', ?, 'on-demand', '')",
    )
    .bind("s".repeat(200))
    .execute(&pool)
    .await
    .expect("…and the row takes the label the old width refused");

    store.stop().await;
}

/// **The shape `0020_entity_crm_wider` declares is the one that recovers
/// it.** The same question as `0019`, over the column beside it.
#[tokio::test]
async fn an_interrupted_entity_crm_widening_is_recognized_by_the_columns_type() {
    let scratch = Scratch::new("migrate-interrupted-entity-crm-widening");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("entitycrmwidepartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the widening did NOT
    // take effect: the column back at the width 0008 gave it, the
    // ledger row taken away, the marker committed.
    sqlx::raw_sql("ALTER TABLE entity MODIFY COLUMN crm VARCHAR(191) NULL")
        .execute(&pool)
        .await
        .expect("the column goes back to its old width");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0020_entity_crm_wider")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0020_entity_crm_wider")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0020_entity_crm_wider".to_string()],
        "the widening that did NOT land is applied",
    );

    // **Read by a route that is not the probe**: a crm label past the
    // old hundred-and-ninety-one-byte width is what the widening is FOR.
    sqlx::query(
        "INSERT INTO entity (id, kind, name, source, crm, boot, prose)
             VALUES ('person:already-here', 'person', 'Already Here', 'contract-fixture', ?, \
             'on-demand', '')",
    )
    .bind("c".repeat(200))
    .execute(&pool)
    .await
    .expect("…and the row takes the label the old width refused");

    store.stop().await;
}

/// **The shape `0045_entity_former_handle_width` declares is the one
/// that recovers it.** The old `0045` was one `ALTER` carrying four
/// clauses; split apart (see `an_interrupted_primary_key_swap_…` above
/// for the rest of that history), this clause's own landed-check has to
/// answer for it alone, the way `0018`'s test drives the real list
/// rather than the mechanism's own synthetic fixture.
#[tokio::test]
async fn an_interrupted_former_handle_widening_is_recognized_by_the_columns_type() {
    let scratch = Scratch::new("migrate-interrupted-former-handle-widening");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("formerhandlewidepartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");

    // The state a death in the window leaves when the widening did NOT
    // take effect: the column back at the width 0042 gave it, the
    // ledger row taken away, the marker committed.
    sqlx::raw_sql(
        "ALTER TABLE entity_former_handle MODIFY COLUMN former_handle VARCHAR(64) NOT NULL",
    )
    .execute(&pool)
    .await
    .expect("the column goes back to its old width");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0045_entity_former_handle_width")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0045_entity_former_handle_width")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0045_entity_former_handle_width".to_string()],
        "the width clause that did NOT land is applied on its own, not the whole 0045 \
             replayed",
    );

    // **Read by a route that is not the probe**: a former handle past
    // the old sixty-four-byte width is what the widening is FOR.
    sqlx::query(
        "INSERT INTO entity_former_handle (former_handle, badge, changed_at)
             VALUES (?, 'badge0001', '2026-01-01')",
    )
    .bind(format!("thing:{}", "h".repeat(90)))
    .execute(&pool)
    .await
    .expect("…and the row takes the handle the old width refused");

    store.stop().await;
}

/// **The shape `0017_type_field_folds` declares is the one that recovers
/// it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_type_field_folds_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-type-field-folds");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("typefieldfoldspartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0017_type_field_folds")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0017_type_field_folds")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0017_type_field_folds"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE type_field DROP COLUMN folds")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0017_type_field_folds")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0017_type_field_folds")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0017_type_field_folds".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "type_field", "folds")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0022_type_field_owner` declares is the one that recovers
/// it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_type_field_owner_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-type-field-owner");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("typefieldownerpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0022_type_field_owner")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0022_type_field_owner")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0022_type_field_owner"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE type_field DROP COLUMN owner")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0022_type_field_owner")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0022_type_field_owner")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0022_type_field_owner".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "type_field", "owner")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0023_type_field_required` declares is the one that
/// recovers it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_type_field_required_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-type-field-required");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("typefieldrequiredpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0023_type_field_required")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0023_type_field_required")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0023_type_field_required"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE type_field DROP COLUMN required")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0023_type_field_required")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0023_type_field_required")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0023_type_field_required".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "type_field", "required")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0025_type_field_one_of` declares is the one that
/// recovers it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_type_field_one_of_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-type-field-one-of");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("typefieldoneofpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0025_type_field_one_of")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0025_type_field_one_of")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0025_type_field_one_of"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE type_field DROP COLUMN one_of")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0025_type_field_one_of")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0025_type_field_one_of")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0025_type_field_one_of".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "type_field", "one_of")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0026_session_timezone` declares is the one that recovers
/// it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_session_timezone_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-session-timezone");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("sessiontimezonepartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0026_session_timezone")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0026_session_timezone")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0026_session_timezone"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE session DROP COLUMN timezone")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0026_session_timezone")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0026_session_timezone")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0026_session_timezone".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "session", "timezone")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0027_fact_inserted_at` declares is the one that recovers
/// it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_fact_inserted_at_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-fact-inserted-at");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factinsertedatpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0027_fact_inserted_at")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0027_fact_inserted_at")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0027_fact_inserted_at"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE fact DROP COLUMN inserted_at")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0027_fact_inserted_at")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0027_fact_inserted_at")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0027_fact_inserted_at".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "fact", "inserted_at")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0028_fact_stale_after` declares is the one that recovers
/// it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_fact_stale_after_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-fact-stale-after");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("factstaleafterpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0028_fact_stale_after")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0028_fact_stale_after")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0028_fact_stale_after"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE fact DROP COLUMN stale_after")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0028_fact_stale_after")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0028_fact_stale_after")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0028_fact_stale_after".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "fact", "stale_after")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **The shape `0030_session_stated_day` declares is the one that
/// recovers it.** The same question `0015`'s own test drives for
/// `type_field.origin`.
#[tokio::test]
async fn an_interrupted_session_started_on_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-session-started-on");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("sessionstartedonpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0030_session_stated_day")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0030_session_stated_day")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded.iter().any(|v| v == "0030_session_stated_day"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE session DROP COLUMN started_on")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0030_session_stated_day")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0030_session_stated_day")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0030_session_stated_day".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "session", "started_on")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}

/// **A schema fingerprint** — every table, its columns (name, declared
/// type, nullability) and its indexes, read from `information_schema`
/// and rendered in a fixed order. Two databases fingerprint identically
/// only when their schemas actually agree; this is what a comparison
/// between a copied store and a freshly migrated one is built on.
async fn schema_fingerprint(pool: &MySqlPool) -> String {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = DATABASE() ORDER BY table_name",
    )
    .fetch_all(pool)
    .await
    .expect("tables readable");
    let mut out = String::new();
    for table in &tables {
        out.push_str(table);
        out.push('\n');
        let columns: Vec<(String, String, String)> = sqlx::query_as(
            "SELECT column_name, column_type, is_nullable FROM information_schema.columns \
                 WHERE table_schema = DATABASE() AND table_name = ? ORDER BY column_name",
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("columns readable");
        for (name, ty, nullable) in columns {
            out.push_str(&format!("  {name} {ty} {nullable}\n"));
        }
        let indexes: Vec<(String, String, i64)> = sqlx::query_as(
            "SELECT index_name, column_name, non_unique FROM information_schema.statistics \
                 WHERE table_schema = DATABASE() AND table_name = ? \
                 ORDER BY index_name, seq_in_index",
        )
        .bind(table)
        .fetch_all(pool)
        .await
        .expect("indexes readable");
        for (name, col, unique) in indexes {
            out.push_str(&format!("  idx {name} {col} {unique}\n"));
        }
    }
    out
}

/// Copy a directory tree, the way a template room copies its store —
/// used here to prove the copy is schema-safe, before anything in
/// `jojobot-exercise` relies on it being so.
fn copy_dir_all(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

/// 🚨 **The assertion standing between the template-copy optimisation and
/// a suite that lies quietly.** A room built from a copied, pre-migrated
/// store must read the same schema as a room migrated fresh — this is
/// what jojobot-exercise's template mechanism rests on, proven at the
/// layer where it is actually true rather than assumed from the layer
/// above.
///
/// **Both halves.** A copy of the whole chain fingerprints identically to
/// a fresh run of the whole chain — the positive a template copy needs.
/// A copy of a store built from a SHORT chain fingerprints differently —
/// the negative that proves the comparison is not vacuously true: a
/// stale or truncated template would be caught here, not discovered
/// later as a suite quietly testing the wrong schema.
#[tokio::test]
async fn a_copied_migrated_store_fingerprints_identically_to_a_fresh_one() {
    let scratch_whole = Scratch::new("migrate-copy-whole");
    let path_whole = scratch_whole.0.clone();
    std::mem::forget(scratch_whole);
    let mut store_whole = crate::dolt::Dolt::start(&path_whole, free_port())
        .await
        .expect("the whole-chain store comes up");
    let pool_whole = store_whole
        .database("copywhole")
        .await
        .expect("a database of its own");
    apply(&pool_whole, MIGRATIONS)
        .await
        .expect("the whole chain applies");
    let print_whole = schema_fingerprint(&pool_whole).await;
    store_whole.stop().await;

    // The copy: a fresh directory, populated from the whole-chain
    // store's own files rather than migrated — what a template room
    // does, at the layer where it actually happens.
    let scratch_copy = Scratch::new("migrate-copy-of-whole");
    let path_copy = scratch_copy.0.clone();
    std::mem::forget(scratch_copy);
    copy_dir_all(&path_whole, &path_copy).expect("the store directory copies");
    let mut store_copy = crate::dolt::Dolt::start(&path_copy, free_port())
        .await
        .expect("the copy comes up without re-migrating");
    let pool_copy = store_copy
        .database("copywhole")
        .await
        .expect("the copy already has its database");
    let print_copy = schema_fingerprint(&pool_copy).await;
    assert_eq!(
        print_whole, print_copy,
        "a copy of a fully migrated store must fingerprint identically to the store it was \
             copied from, or a template room silently serves a different schema than a fresh one",
    );
    store_copy.stop().await;

    // The negative: a store built from a chain short its last
    // migration, copied the same way. Its fingerprint must differ from
    // the whole chain's, or this comparison cannot catch a stale or
    // truncated template.
    let scratch_short = Scratch::new("migrate-copy-short");
    let path_short = scratch_short.0.clone();
    std::mem::forget(scratch_short);
    let mut store_short = crate::dolt::Dolt::start(&path_short, free_port())
        .await
        .expect("the short-chain store comes up");
    let pool_short = store_short
        .database("copyshort")
        .await
        .expect("a database of its own");
    apply(&pool_short, &MIGRATIONS[..MIGRATIONS.len() - 1])
        .await
        .expect("the short chain applies");
    store_short.stop().await;

    let scratch_copy_short = Scratch::new("migrate-copy-of-short");
    let path_copy_short = scratch_copy_short.0.clone();
    std::mem::forget(scratch_copy_short);
    copy_dir_all(&path_short, &path_copy_short).expect("the short store directory copies");
    let mut store_copy_short = crate::dolt::Dolt::start(&path_copy_short, free_port())
        .await
        .expect("the copy of the short store comes up");
    let pool_copy_short = store_copy_short
        .database("copyshort")
        .await
        .expect("the copy already has its database");
    let print_copy_short = schema_fingerprint(&pool_copy_short).await;
    assert_ne!(
        print_whole, print_copy_short,
        "a copy of a store built from a short migration chain fingerprinted identically to \
             the whole chain, so this comparison cannot actually catch a stale template",
    );
    store_copy_short.stop().await;
}

/// **The shape `0055_message_posted_by_session` declares is the one that
/// recovers it.** The same question `0030`'s own test drives for
/// `session.started_on`.
#[tokio::test]
async fn an_interrupted_message_posted_by_session_add_is_recognized_by_the_column_being_there() {
    let scratch = Scratch::new("migrate-interrupted-message-posted-by-session");
    let path = scratch.0.clone();
    std::mem::forget(scratch);
    let mut store = crate::dolt::Dolt::start(&path, free_port())
        .await
        .expect("the store comes up");
    let pool = store
        .database("messagepostedbysessionpartway")
        .await
        .expect("a database of its own");

    run(&pool).await.expect("the schema");
    // The state a death in the window leaves: the ledger row taken away,
    // the marker committed, the column already there because the
    // statement itself had succeeded.
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0055_message_posted_by_session")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0055_message_posted_by_session")
        .await
        .expect("the marker lands");

    let applied = run(&pool).await.expect("the start completes the schema");
    assert!(
        applied.is_empty(),
        "an interrupted add-column that had landed is recorded, not re-issued: {applied:?}"
    );
    let recorded: Vec<String> = sqlx::query_scalar("SELECT version FROM schema_migration")
        .fetch_all(&pool)
        .await
        .expect("the ledger is readable");
    assert!(
        recorded
            .iter()
            .any(|v| v == "0055_message_posted_by_session"),
        "…and the ledger says so, so the next start asks nothing: {recorded:?}"
    );

    // **The other direction, and it is what makes the first one mean
    // anything.** A death BEFORE the statement took effect leaves the
    // same marker over a table that is still there.
    sqlx::raw_sql("ALTER TABLE message DROP COLUMN posted_by_session")
        .execute(&pool)
        .await
        .expect("the column goes");
    sqlx::query("DELETE FROM schema_migration WHERE version = ?")
        .bind("0055_message_posted_by_session")
        .execute(&pool)
        .await
        .expect("the ledger row goes");
    mark_begun(&pool, "0055_message_posted_by_session")
        .await
        .expect("the marker lands");

    assert_eq!(
        run(&pool).await.expect("the start completes the schema"),
        vec!["0055_message_posted_by_session".to_string()],
        "an interrupted add-column that did NOT land is applied",
    );
    assert!(
        column_exists(&pool, "message", "posted_by_session")
            .await
            .expect("the schema is readable"),
        "…and the column the migration is for is really there afterwards",
    );

    store.stop().await;
}
