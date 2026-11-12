//! **A fold admits the kinds the store under it declares**, not the set this
//! process parses against.

use super::*;
use jojobot_domain::memory::testing::InMemoryMemory;
use jojobot_domain::memory::types::Origin;

/// A store that declares `sloth`, and a fold filled from it. The process set is
/// not told about `sloth`, so a fold that still asked it would refuse the read.
#[tokio::test]
async fn a_fold_reads_a_kind_its_store_declares_and_refuses_one_it_does_not() {
    let store = Arc::new(InMemoryMemory::new());
    store
        .declare_kind("sloth", Origin::Declared, Vec::new())
        .await
        .expect("the store declares its kind");
    let folded = Folded::new(store);
    folded.rebuild().await.expect("the fold loads");

    let admitted = folded
        .referring_to(&EntityId("sloth:slow".to_string()))
        .await;
    assert!(
        admitted.is_ok(),
        "the fold refused a kind its store declares: {admitted:?}"
    );
    let refused = folded
        .referring_to(&EntityId("walrus:tusky".to_string()))
        .await;
    assert!(
        matches!(&refused, Err(MemoryError::InvalidSubject(why)) if why.contains("no kind is named")),
        "the fold admitted a kind its store does not declare: {refused:?}"
    );
}

/// **A kind declaration refreshes the kinds the fold holds and reads no thing
/// again.** The kinds are the one read the fold keeps beside the claims State: about
/// two dozen rows, read again when a kind is written. A refill of every thing, as a
/// declaration made before the State served the reads, would show as a read of the
/// listing or of a document.
#[tokio::test]
async fn a_kind_declaration_refreshes_the_kinds_and_reads_no_thing_again() {
    let counted = Arc::new(crate::counted::Counted::new(Arc::new(
        InMemoryMemory::booted(),
    )));
    let folded = Folded::new(counted.clone());
    folded.rebuild().await.expect("the fold loads");
    counted.reset();
    let changed = folded
        .declare_kind("sloth", Origin::Declared, Vec::new())
        .await
        .expect("the kind is declared");
    assert!(changed, "the store had no such kind");
    let seen = counted.seen();
    for read in ["list_entities", "scan", "scan_entity", "recall", "fields"] {
        assert_eq!(
            seen.get(read).copied().unwrap_or(0),
            0,
            "a kind declaration read {read} again: {seen:?}"
        );
    }
    let held = folded.declared_kinds().await.expect("the kinds are read");
    assert!(
        held.iter().any(|(token, _)| token == "sloth"),
        "the fold holds the kind it just declared: {held:?}"
    );
}
