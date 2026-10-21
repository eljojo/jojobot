//! **A claim made on a thing after a merge gets an id nobody holds**, on the
//! real store. A merge moves no stored row and reserves ids for the claims it
//! folds, so the survivor's stored rows are no longer a run from one, and a new
//! claim takes the id past the highest, never the count of the stored rows.
//!
//! The shared contract case runs in the whole-contract run on this store; it
//! runs here alone so the break "mint the id by count" is seen on its own.

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::testing::{free_port, migrated_database};
use jojobot_domain::memory::testing::contract as memory;

/// A directory of this run's own, removed when it is done.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(what: &str) -> Self {
        let path = jojobot_adapters::testing::scratch_path(&format!("jojobot-contract-{what}"));
        std::fs::create_dir_all(&path).expect("a scratch directory");
        Scratch(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn dolt_gives_a_claim_made_after_a_merge_an_id_nobody_holds() {
    let scratch = Scratch::new("claim-ids-after-a-merge");
    let mut store = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let pool = migrated_database(&store, "memory")
        .await
        .expect("a database of this case's own");
    migrate::run(&pool).await.expect("the schema");
    jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
        .await
        .expect("the kinds are seeded");

    memory::a_claim_made_after_a_merge_gets_an_id_nobody_holds(&DoltMemory::open(pool.clone()))
        .await;

    store.stop().await;
}
