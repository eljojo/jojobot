//! **Seeded command sequences, over the fake: the fold the writes patched must
//! serve what a fold filled anew from the store serves, and what the store
//! serves.** `tests/fold_agreement_dolt.rs` runs the same draws over the real
//! store.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;

use jojobot_adapters::fold_agreement::{check_seed, landed_enough, run_through_fold};
use jojobot_domain::memory::{LinkRule, set_link_rule};

/// The seeds a default run draws, and the steps each takes.
const SEEDS: std::ops::Range<u64> = 0..60;
const STEPS: usize = 30;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fold_the_writes_patched_serves_what_a_fold_filled_anew_and_the_store_serve() {
    set_link_rule(LinkRule::On);
    let mut failures = Vec::new();
    let mut traces = Vec::new();
    for seed in SEEDS {
        match check_seed(seed, STEPS, Arc::new(InMemoryMemory::booted())).await {
            Ok(trace) => traces.push(trace),
            Err(report) => failures.push(report),
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} seeds disagree; replay one with its seed.\n{}",
        failures.len(),
        SEEDS.end - SEEDS.start,
        failures.first().cloned().unwrap_or_default()
    );
    // **Agreement over writes that did not land agrees about nothing.**
    if let Err(why) = landed_enough(&traces) {
        panic!("the writes did not land through the fold: {why}");
    }
}

/// **The run draws every kind of write, applies it THROUGH the fold, and most of
/// it lands.** A fold that refuses what it is given leaves the agreement above
/// comparing empty claims; this reads the fold's own answers to the writes.
#[tokio::test]
async fn a_default_run_draws_every_kind_of_write_through_the_fold_and_most_of_them_land() {
    set_link_rule(LinkRule::On);
    let mut traces = Vec::new();
    for seed in SEEDS {
        let store = Arc::new(InMemoryMemory::booted());
        let (_, trace) = run_through_fold(seed, STEPS, store).await;
        traces.push(trace);
    }
    if let Err(why) = landed_enough(&traces) {
        panic!("{why}");
    }
}
