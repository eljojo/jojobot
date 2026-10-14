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
    "write_summary",
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
        last_signal: Default::default(),
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
        last_signal: Default::default(),
    };
    walk_every_kind_of_write(&probe).await;
}

/// **A boot declares the types the build ships, every time, and a store that
/// already holds them has nothing to fold again.** A declaration the image
/// already holds, word for word, changes how no thing folds, so it must not read
/// the whole store again: with the image full, six such declarations at boot read
/// it six times over and the server took minutes to start. Paired with the
/// positive: a declaration that differs still reads the image again and changes
/// what a counter key folds to.
#[tokio::test]
async fn declaring_a_type_the_image_already_holds_reads_nothing_again() {
    use jojobot_domain::memory::types::{DeclaredType, Field, ValueType};
    use jojobot_domain::memory::{EntityId, Memory, NewEntity};

    let store = Arc::new(InMemoryMemory::booted());
    let counted = Arc::new(Counted::new(store.clone()));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    let thing = EntityId("thing:delta".into());
    folded
        .add_entity(NewEntity::new(thing.clone(), "Delta Station", "fold-image"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");

    let ranked = DeclaredType::new(
        "fold-ranked",
        vec![Field::required("rank", ValueType::Number)],
    );
    folded
        .declare_type(ranked.clone(), None)
        .await
        .expect("declared");

    counted.reset();
    folded
        .declare_type(ranked, None)
        .await
        .expect("declared again");
    assert_eq!(
        counted.seen(),
        std::collections::BTreeMap::from([("declare_type", 1)]),
        "declaring what the image already holds reached the store for more than the declaration"
    );

    counted.reset();
    folded
        .declare_type(
            DeclaredType::new(
                "fold-ranked",
                vec![Field::required("rank", ValueType::Text)],
            ),
            None,
        )
        .await
        .expect("declared differently");
    assert!(
        counted.calls() > 1,
        "a declaration that differs must read the image again: {:?}",
        counted.seen()
    );
}
