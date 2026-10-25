//! The writer of a write, read from the real store.

use super::*;
use crate::dolt::tests::free_port;
use crate::dolt::{Dolt, migrate};
use crate::testing::{Scratch, migrated_database};

/// **A bot folded into another reads as the survivor in a claim's history**,
/// on the real store: the write holds the folded bot's id, and the reader
/// follows the fold to the survivor.
#[tokio::test]
async fn a_bot_folded_into_another_reads_as_the_survivor_in_the_real_stores_history() {
    let scratch = Scratch::new("folded-writer");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "folded_writer")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    jojobot_domain::memory::testing::contract::a_bot_folded_into_another_reads_as_the_survivor_in_a_claims_history(
        &memory,
    )
    .await;
    store.stop().await;
}

/// **A record filed by a bot that was folded into another makes the survivor a
/// writer of the thread**, on the real store.
#[tokio::test]
async fn a_record_filed_by_a_folded_bot_makes_its_survivor_a_writer_in_the_real_store() {
    let scratch = Scratch::new("folded-filler");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "folded_filler")
        .await
        .expect("a database of its own");
    migrate::run(&pool).await.expect("the schema");
    let memory = DoltMemory::open(pool.clone());
    jojobot_domain::memory::kinds::seed(&memory)
        .await
        .expect("the kinds are seeded");

    jojobot_domain::memory::testing::contract::a_record_filed_by_a_folded_bot_makes_its_survivor_a_writer(
        &memory,
    )
    .await;
    store.stop().await;
}
