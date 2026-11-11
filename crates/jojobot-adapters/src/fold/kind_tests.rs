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

/// **The names a fold reads from the claims State are admitted by the store's own
/// kinds too.** The State read is a second way to fill the referring lists, and it
/// asks the same question: a name of a kind the store declares is read, and a name
/// of a kind it does not is skipped. The process set is not told about `sloth`.
#[test]
fn a_read_from_the_claims_state_admits_the_kinds_its_store_declares() {
    use jojobot_domain::memory::kinds::KindSet;
    let kinds = KindSet::new(vec![("sloth".to_string(), Origin::Declared)]);
    let names = vec!["sloth:slow".to_string(), "walrus:tusky".to_string()];
    let read = image::referring_in(
        &jojobot_domain::log::claims::State::default(),
        &kinds,
        &names,
    );
    assert_eq!(
        read.iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["sloth:slow"],
        "the name of a declared kind is read and the other is skipped"
    );
}
