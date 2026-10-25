//! **A rewrite of a claim a newer claim displaced is refused, on the real
//! store.** The case is the one the fake runs; the store decides it in the
//! transaction that would append the edit.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract;

#[tokio::test]
async fn dolt_refuses_a_rewrite_of_a_claim_a_newer_claim_displaced() {
    let scratch = Scratch::new("must-back");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "mustback")
        .await
        .expect("a database of the case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool);
    contract::a_rewrite_of_a_claim_a_newer_claim_displaced_is_refused(&memory).await;
    contract::a_rewrite_naming_the_keys_it_must_back_is_refused_without_a_revision(&memory).await;
    store.stop().await;
}
