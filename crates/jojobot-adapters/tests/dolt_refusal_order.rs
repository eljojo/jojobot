//! **A capture with two missing names is told the first in the decided order, on
//! the real store.** The subject, then the edge's object, then the refs, then the
//! value of a reference-typed field. The cases are the ones the fake runs, and the
//! last is the one the fake does not yet pass.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};
use jojobot_domain::memory::testing::contract;

/// **The real store still names a missing subject before it judges the write.**
/// The decided order refuses a setting with no value first; this pins what the
/// real store does today. The admission that moves in 9.3.3-c2 makes it follow the
/// decided order; that change removes `should_panic` and runs the case like the
/// others.
#[tokio::test]
#[should_panic(expected = "a malformed write is refused before the subject is judged")]
async fn dolt_still_names_a_missing_subject_before_it_judges_a_malformed_write() {
    let scratch = Scratch::new("refusal-order-malformed");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "refusalordermalformed")
        .await
        .expect("a database of the case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let memory = DoltMemory::open(pool);
    contract::a_setting_with_no_value_is_refused_before_a_missing_subject(&memory).await;
    store.stop().await;
}

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
    contract::a_thought_over_its_cap_is_refused_before_a_full_room(&memory).await;
    contract::a_full_room_is_refused_before_a_full_thread(&memory).await;
    contract::a_missing_source_is_refused_before_a_held_role(&memory).await;
    contract::a_write_after_a_merge_is_the_newest_of_its_key(&memory).await;
    store.stop().await;
}
