//! **A thing and its first claim are made in one batch, in the real store.**
//!
//! The shared contract case refuses before the batch opens, so it passes on a
//! store that creates the thing in one commit and the claim in a second. This
//! case fails the claim's write from inside the batch, after the thing's row is
//! in it, and asserts the thing is not there afterwards.

use jiff::civil::date;
use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::{EntityId, Memory, NewEntity, NewFact, Provenance};
use sqlx::MySqlPool;

/// **How many rows each table of the database holds**, by table. It reads the
/// rows themselves, not the store's API: a read of a handle that is not on the
/// roster is refused or empty before any claim row is looked at, so it cannot say
/// whether a claim row was left behind.
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

#[tokio::test]
async fn a_first_claim_that_fails_inside_the_write_leaves_no_thing_behind() {
    let scratch = Scratch::new("creation-atomic");
    let server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&server, "creationatomic")
        .await
        .expect("a database of its own");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    let eta = EntityId("work:eta".to_string());
    let claim = |subject: &EntityId| {
        let mut fact = NewFact::about(subject.clone(), "the first claim", date(2026, 10, 7));
        fact.provenance = Provenance::Testimony;
        fact
    };

    // **The claim's table is out of reach while the call runs**, so the read of
    // what the thing already holds (the capacity preflight in `capture_in`, which
    // joins this table) fails after validation has passed and after the thing's
    // own row is in the batch. Nothing the call is asked is wrong; the store
    // fails it.
    let before = row_counts(&pool).await;
    sqlx::query("RENAME TABLE fact_write TO fact_write_away")
        .execute(&pool)
        .await
        .expect("the claim table is moved aside");
    let failed = memory
        .add_entity_with_first_claim(
            NewEntity::new(eta.clone(), "Eta", "contract-fixture"),
            claim(&eta),
        )
        .await;
    sqlx::query("RENAME TABLE fact_write_away TO fact_write")
        .execute(&pool)
        .await
        .expect("the claim table is put back");
    assert!(
        failed.is_err(),
        "the store failed the claim's write, so the call fails: {failed:?}"
    );

    // **Every table holds the rows it held before the call**, counted from the
    // rows. A batch that rolled back the thing's row but left a claim, a key write
    // or a version row keyed to its identity leaves a count higher, behind a
    // handle that no longer reads, which the roster and the read below cannot see.
    assert_eq!(
        row_counts(&pool).await,
        before,
        "a claim that failed inside the batch left rows behind"
    );

    let roster = memory.list_entities(None).await.expect("the roster reads");
    assert!(
        roster.iter().all(|entity| entity.id != eta),
        "a claim that failed inside the batch left {eta} on the roster: {roster:?}"
    );
    assert!(
        memory
            .recall(&eta)
            .await
            .map_or(true, |facts| facts.is_empty()),
        "a read of the handle finds a claim of the thing a failed batch made"
    );

    // **The same call, once the table is back, makes both**, so the miss above
    // is the failure and not a handle this store refuses.
    let (_, made) = memory
        .add_entity_with_first_claim(
            NewEntity::new(eta.clone(), "Eta", "contract-fixture"),
            claim(&eta),
        )
        .await
        .expect("the call is allowed once the store is whole")
        .written()
        .expect("nothing collides with it");
    assert!(
        memory
            .list_entities(None)
            .await
            .expect("the roster reads")
            .iter()
            .any(|entity| entity.id == eta),
        "the thing is made when the claim's write lands"
    );
    // **The claim the call returned is stored**, read back through the store, so
    // a call that commits the thing and answers with a claim it did not persist
    // fails here.
    let stored = memory.recall(&eta).await.expect("the claim reads back");
    assert!(
        stored
            .iter()
            .any(|fact| fact.id == made.id && fact.content == "the first claim"),
        "the first claim the call returned is stored on the new thing: {stored:?}"
    );
}
