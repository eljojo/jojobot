//! **A capture with two missing names is told the first in the decided order, on
//! the real store.** The subject, then the edge's object, then the refs, then the
//! value of a reference-typed field. The cases are the ones the fake runs, and the
//! last is the one the fake does not yet pass.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract;

#[tokio::test]
async fn dolt_judges_two_missing_names_in_the_decided_order() {
    let scratch = Scratch::new("refusal-order");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "refusalorder")
        .await
        .expect("a database of the case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool);
    contract::a_missing_subject_is_refused_before_a_missing_edge_object(&memory).await;
    contract::a_missing_edge_object_is_refused_before_a_missing_ref(&memory).await;
    contract::a_missing_ref_is_refused_before_a_missing_reference_field(&memory).await;
    store.stop().await;
}
