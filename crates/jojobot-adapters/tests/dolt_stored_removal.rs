//! **The SQL key reader tells a stored removal from a live clear.** Before the
//! refusal of a clear by a claim that did not hold the key, any clear took the key
//! off the thing (rule 677). A row the store kept then carries no mark, and its
//! meaning is the column's default. A clear written now is one claim's write, and
//! the row says so. Both are NULL values; the reader must not read them alike.

use jiff::civil::date;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::shared_database;
use jojobot_domain::memory::{EntityId, FactPatch, Memory, NewEntity, NewFact};

async fn a_store(label: &str) -> (sqlx::MySqlPool, DoltMemory) {
    let pool = shared_database(label).await.expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");
    (pool, memory)
}

/// A thing whose mood two claims wrote, the older `calm` and the newer `queasy`.
async fn two_claims_on_one_key(
    memory: &DoltMemory,
    handle: &str,
) -> (EntityId, jojobot_domain::memory::Fact) {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), "Kettle", "fixture"))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    let mut older = None;
    for (words, mood) in [("felt calm", "calm"), ("felt queasy", "queasy")] {
        let mut fact = NewFact::about(id.clone(), words, date(2026, 10, 9));
        fact.fields.insert("mood".into(), mood.into());
        fact.sets.insert("mood".into());
        let written = memory
            .capture(fact)
            .await
            .expect("capture")
            .written()
            .expect("the claim lands");
        older.get_or_insert(written);
    }
    (id, older.expect("the older claim"))
}

/// **A NULL row a store kept before the refusal takes the key off the thing**,
/// whichever claim held the value: the older claim's clear is stored beside the
/// newer claim's value, and the reader serves no mood.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_historical_clear_by_a_claim_that_did_not_hold_the_key_takes_it_off() {
    let (pool, memory) = a_store("stored-removal-historical").await;
    let (thing, older) = two_claims_on_one_key(&memory, "thing:kettle").await;
    let entity: String = sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
        .bind(thing.as_str())
        .fetch_one(&pool)
        .await
        .expect("the thing has a key");
    // The row the deployed build wrote for a clear: no value, no mark.
    sqlx::query(
        "INSERT INTO field_write (entity, `key`, ordinal, value, fact_id, written_at,
                                  write_class, fact_write_ordinal)
         VALUES (?, 'mood', 3, NULL, ?, '2026-10-09T10:00:00Z', 'sets', 1)",
    )
    .bind(&entity)
    .bind(older.id.as_str())
    .execute(&pool)
    .await
    .expect("the stored clear lands");
    let fields = memory.fields(&thing).await.expect("the fields read");
    assert!(
        !fields.contains_key("mood"),
        "the old clear took the key off the thing: {fields:?}"
    );
}

/// **A clear written now by a claim that does not hold the key withdraws only its
/// own write**, and the reader keeps the newer claim's value.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_live_clear_by_a_claim_that_does_not_hold_the_key_leaves_the_value() {
    let (pool, memory) = a_store("stored-removal-live").await;
    let (thing, older) = two_claims_on_one_key(&memory, "thing:kettle").await;
    memory
        .add_entity(NewEntity::new(
            EntityId("bot:beta".into()),
            "Beta",
            "fixture",
        ))
        .await
        .expect("add")
        .written()
        .expect("not blocked");
    memory
        .update_fact(
            &older.address(),
            FactPatch {
                clear_fields: vec!["mood".into()],
                ..Default::default()
            },
            &EntityId("bot:beta".into()),
        )
        .await
        .expect("edit")
        .written()
        .expect("the clear lands");
    let fields = memory.fields(&thing).await.expect("the fields read");
    assert_eq!(
        fields.get("mood").map(String::as_str),
        Some("queasy"),
        "the newer claim still holds the key: {fields:?}"
    );
    // **What the previous release's reader sees**: it ignores the new column and
    // reads a row of no value as a clear, as it read every clear. The live clear
    // is such a row, and carries the mark only in the column.
    let rows: Vec<(Option<String>, String, bool)> = sqlx::query_as(
        "SELECT value, write_class, live_clear FROM field_write
         WHERE `key` = 'mood' AND value IS NULL",
    )
    .fetch_all(&pool)
    .await
    .expect("the rows read");
    assert_eq!(
        rows,
        vec![(None, "sets".to_string(), true)],
        "one row of no value, in the class of the key it clears, marked live"
    );
}
