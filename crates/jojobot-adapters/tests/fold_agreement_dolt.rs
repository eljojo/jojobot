//! **The seeded command sequences of `fold_agreement`, over the real store.**
//!
//! Each seed gets a database of its own. After the sequence the fold the writes
//! patched is compared, read by read and in order, with a fold filled anew from
//! the rows and with the store itself. A failure names the seed; the same seed
//! replays it over the fake in `fold_agreement_tests`.

use std::sync::Arc;

use jojobot_adapters::dolt::Dolt;
use jojobot_adapters::dolt::memory::DoltMemory;
use jojobot_adapters::dolt::migrate;
use jojobot_adapters::fold_agreement::check_seed;
use jojobot_adapters::testing::{Scratch, free_port, migrated_database};

/// The seeds a default run over the real store draws, and the steps each takes.
/// Fewer than over the fake: each step is a statement or more.
const SEEDS: std::ops::Range<u64> = 0..12;
const STEPS: usize = 30;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fold_the_writes_patched_serves_what_a_fold_filled_anew_and_the_real_store_serve() {
    let scratch = Scratch::new("agreement");
    let mut server = Dolt::start(&scratch.0, free_port())
        .await
        .expect("the store comes up");
    let mut failures = Vec::new();
    for seed in SEEDS {
        let pool = migrated_database(&server, &format!("agree{seed}"))
            .await
            .expect("a database of its own");
        migrate::run(&pool).await.expect("the schema");
        jojobot_domain::memory::kinds::seed(&DoltMemory::open(pool.clone()))
            .await
            .expect("the kinds are seeded");
        let store = Arc::new(DoltMemory::open(pool));
        if let Err(report) = check_seed(seed, STEPS, store).await {
            failures.push(report);
        }
    }
    server.stop().await;
    assert!(
        failures.is_empty(),
        "{} of {} seeds disagree; replay one with its seed.\n{}",
        failures.len(),
        SEEDS.end - SEEDS.start,
        failures.join("\n\n")
    );
}
