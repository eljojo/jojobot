//! **A companion record that the store refuses INSIDE the batch leaves nothing
//! behind, in the real store.**
//!
//! A creation writes the thing, its first claim and the engine's companion
//! records in one transaction. A companion is checked for shape before the
//! transaction opens, so a companion that passes that check and fails a rule the
//! transaction reads (here the thing's record ceiling) is refused after the
//! thing's row and the first claim are already in the batch. The plain
//! sequential contract case refuses before the batch and passes on a store that
//! commits in steps. This case does not.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, MemoryError, NewEntity, NewFact, Provenance};
use sqlx::MySqlPool;

/// **How many rows each table of the database holds**, by table. It reads the
/// rows themselves, not the store's API: a read of a handle that is not on the
/// roster is refused before any claim is looked at, so it cannot say whether a
/// claim row was left behind.
async fn row_counts(pool: &MySqlPool) -> Vec<(String, i64)> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT table_name FROM information_schema.tables \
         WHERE table_schema = DATABASE() ORDER BY table_name",
    )
    .fetch_all(pool)
    .await
    .expect("the tables read");
    let mut counts = Vec::new();
    for table in tables {
        let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM `{table}`"
        )))
        .fetch_one(pool)
        .await
        .expect("the rows count");
        counts.push((table, n));
    }
    counts
}

/// The rows of one table that belong to `thing`.
async fn rows_of(pool: &MySqlPool, table: &str, column: &str, thing: &EntityId) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM `{table}` WHERE `{column}` = ?"
    )))
    .bind(thing.as_str())
    .fetch_one(pool)
    .await
    .expect("the rows count")
}

/// A first claim that sets the thing's ceiling to `ceiling`, and one companion.
fn creation(thing: &EntityId, ceiling: &str) -> (NewEntity, NewFact) {
    let mut first = NewFact::about(thing.clone(), "the first claim", date(2026, 10, 7));
    first.provenance = Provenance::Testimony;
    first
        .fields
        .insert("record_capacity".to_string(), ceiling.to_string());
    first.sets.insert("record_capacity".to_string());
    let mut companion = NewFact::about(thing.clone(), "the companion", date(2026, 10, 7));
    companion.provenance = Provenance::Testimony;
    first.also_written.push(companion);
    (
        NewEntity::new(thing.clone(), "Companion", "contract-fixture"),
        first,
    )
}

#[tokio::test]
async fn a_companion_refused_inside_the_transaction_leaves_nothing() {
    let scratch = Scratch::new("companion-atomic");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "companionatomic")
        .await
        .expect("a database of its own");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    // **A ceiling of one holds the first claim and no companion.** The companion
    // is well formed, so only the batch's own read of the thing's records can
    // refuse it, after the thing and the first claim are in the batch.
    let thing = EntityId("work:theta".to_string());
    let (entity, first) = creation(&thing, "1");
    let before = row_counts(&pool).await;
    let refused = memory.add_entity_with_first_claim(entity, first).await;
    assert!(
        matches!(
            &refused,
            Err(MemoryError::OverCapacity { subject, live: 1, limit: 1, adding: 1, .. })
                if subject.as_str() == "work:theta"
        ),
        "the companion goes over the ceiling of one, inside the batch: {refused:?}"
    );
    let roster = memory.list_entities(None).await.expect("the roster reads");
    assert!(
        roster.iter().all(|entity| entity.id != thing),
        "a companion refused inside the batch left {thing} on the roster: {roster:?}"
    );
    // **The claim rows are read from the tables.** The roster check above
    // passes once the thing is gone, whatever happened to its claims.
    assert_eq!(
        row_counts(&pool).await,
        before,
        "a refused batch changed the rows of some table"
    );
    assert_eq!(
        rows_of(&pool, "entity", "id", &thing).await,
        0,
        "the entity table holds a row of {thing} after a refused batch"
    );

    // **The same creation with room for the companion makes all three**, so the
    // miss above is the refusal and not a handle this store turns away.
    let roomy = EntityId("work:iota".to_string());
    let (entity, first) = creation(&roomy, "2");
    memory
        .add_entity_with_first_claim(entity, first)
        .await
        .expect("the creation is allowed")
        .written()
        .expect("nothing collides with it");
    // The same reading sees the rows a creation makes, so the zero above is
    // an absence and not a table this reading cannot see.
    // Claims are keyed by the thing's badge, not its handle.
    let badge: String = sqlx::query_scalar("SELECT badge FROM entity WHERE id = ?")
        .bind(roomy.as_str())
        .fetch_one(&pool)
        .await
        .expect("the thing has a badge");
    for table in ["fact", "fact_write"] {
        let (n,): (i64,) = sqlx::query_as(sqlx::AssertSqlSafe(format!(
            "SELECT COUNT(*) FROM `{table}` WHERE entity = ?"
        )))
        .bind(&badge)
        .fetch_one(&pool)
        .await
        .expect("the rows count");
        assert_eq!(n, 2, "{table} holds the first claim and the companion");
    }
    let after = row_counts(&pool).await;
    for table in ["entity", "fact", "fact_write"] {
        let count = |counts: &[(String, i64)]| {
            counts
                .iter()
                .find(|(name, _)| name == table)
                .map(|(_, n)| *n)
        };
        assert_ne!(
            count(&after),
            count(&before),
            "a creation moves the row count of {table}, so the snapshot sees it"
        );
    }
    assert!(
        memory
            .list_entities(None)
            .await
            .expect("the roster reads")
            .iter()
            .any(|entity| entity.id == roomy),
        "the thing is made when its companion fits"
    );
    let stored = memory.recall(&roomy).await.expect("the claims read back");
    for words in ["the first claim", "the companion"] {
        assert!(
            stored.iter().any(|fact| fact.content == words),
            "{words:?} is stored on the new thing: {stored:?}"
        );
    }

    server.stop().await;
}
