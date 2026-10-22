//! **What a search finds after a merge, over the real store.**
//!
//! A merge writes an event and moves no row, so the search index is read
//! through the event. The case itself is the shared contract's; this file runs
//! it alone against the real store, where the whole contract runs in parts.

use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::search::{IndexedMemory, Retrieval};
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::testing::contract;

use jojobot_adapters::testing::Scratch;

#[tokio::test]
async fn a_claim_of_the_folded_thing_is_found_under_the_survivor_in_the_real_store() {
    let scratch = Scratch(jojobot_adapters::testing::scratch_path(
        "jojobot-search-after-a-merge",
    ));
    std::fs::create_dir_all(&scratch.0).expect("a scratch directory");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "searchaftermerge")
        .await
        .expect("a database of its own");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");
    let indexed = Arc::new(
        IndexedMemory::new(Arc::new(DoltMemory::open(pool.clone())))
            .expect("the search index opens"),
    );
    indexed.rebuild().await.expect("the boot scan reads");
    contract::a_claim_of_the_folded_thing_is_found_under_the_survivor_after_a_fold(
        indexed.as_ref(),
        &Retrieval::new(indexed.index(), vec![indexed.clone()]),
    )
    .await;
    store.stop().await;
}
