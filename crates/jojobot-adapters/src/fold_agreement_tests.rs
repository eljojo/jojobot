//! **Seeded command sequences, over the fake: the fold the writes patched must
//! serve what a fold filled anew from the store serves, and what the store
//! serves.** `tests/fold_agreement_dolt.rs` runs the same draws over the real
//! store.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;

use crate::fold::Folded;
use crate::fold_agreement::{check_seed, run_sequence};

/// The seeds a default run draws, and the steps each takes.
const SEEDS: std::ops::Range<u64> = 0..60;
const STEPS: usize = 30;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_fold_the_writes_patched_serves_what_a_fold_filled_anew_and_the_store_serve() {
    let mut failures = Vec::new();
    for seed in SEEDS {
        if let Err(report) = check_seed(seed, STEPS, Arc::new(InMemoryMemory::booted())).await {
            failures.push(report);
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} seeds disagree; replay one with its seed.\n{}",
        failures.len(),
        SEEDS.end - SEEDS.start,
        failures.first().cloned().unwrap_or_default()
    );
}

/// A run that does nothing agrees with anything: the draw must write, merge and
/// edit.
#[tokio::test]
async fn a_default_run_draws_every_kind_of_write_and_most_of_them_land() {
    let store = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(store.clone());
    folded.rebuild().await.expect("the fold loads");
    let mut steps = Vec::new();
    let mut refused = 0;
    for seed in SEEDS {
        let trace = run_sequence(seed, STEPS, &InMemoryMemory::booted()).await;
        refused += trace.refused;
        steps.extend(trace.steps);
    }
    for verb in [
        "add ",
        "capture on",
        "edit ",
        "retract ",
        "merge ",
        "rename ",
        "archive ",
    ] {
        assert!(
            steps
                .iter()
                .any(|s| s.contains(verb) && !s.contains("refused")),
            "no {verb:?} landed in a default run"
        );
    }
    assert!(
        refused * 3 < steps.len(),
        "{refused} of {} steps refused",
        steps.len()
    );
}
