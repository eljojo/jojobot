//! **The in-memory image (decision log 292), held to the store it mirrors.**
//!
//! The walk in [`crate::fold_battery`] writes every kind of write through the
//! fold and, after each, asks every read of the fold and of the store. These
//! cases run it over the fake; `tests/fold_image_dolt.rs` runs the same walk
//! over the real store.

use std::sync::Arc;

use jojobot_domain::memory::testing::InMemoryMemory;

use crate::counted::Counted;
use crate::fold::Folded;
use crate::fold_battery::{Probe, walk_every_kind_of_write};

/// The reads served from memory once the fold has loaded. Each read is added
/// here in the commit that switches it.
const SWITCHED: &[&str] = &[
    "list_entities",
    "former_handles",
    "declared_types",
    "declared_kinds",
    "recall",
    "fields",
    "fields_versioned",
    "field_ages",
    "claim_histories",
    "claim_history",
    "scan_entity",
    "scan",
    "history",
    "keys_kept_off_the_thing",
    "key_trails",
    "backing",
    "built_on",
    "referring_to",
    "children",
];

/// **The instrument first.** With nothing switched the walk compares a fold
/// that forwards everything with the store, so it can only pass; a failure here
/// is the battery disagreeing with itself and says nothing about the image.
#[tokio::test]
async fn the_battery_agrees_with_a_fold_that_serves_nothing_from_memory() {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: &[],
        kept_off_misses_are_empty: false,
    };
    walk_every_kind_of_write(&probe).await;
}

/// **After startup the switched reads make no call to the store, and answer
/// as the store does, after every kind of write.**
#[tokio::test]
async fn switched_reads_answer_from_memory_exactly_as_the_store_does() {
    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let probe = Probe {
        folded: &folded,
        direct: &*store,
        counted: &counted,
        switched: SWITCHED,
        kept_off_misses_are_empty: false,
    };
    walk_every_kind_of_write(&probe).await;
}
