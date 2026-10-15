//! **A merge refuses the same things in the same order in every store** (card
//! 2381), proven against any [`Memory`] adapter. A side is judged before the
//! other: a side that is missing or already folded is the refusal, whichever
//! comes first in the order [folded, survivor]. A thing cannot be folded into
//! one of its own descendants. Each refusal is read beside the merge that lands.

use super::support::{add, capture, other_caller};
use super::*;

/// Fold `folded` into `survivor` as a caller who owns neither.
async fn fold<M: Memory>(
    store: &M,
    folded: &EntityId,
    survivor: &EntityId,
) -> Result<Merge, MemoryError> {
    store
        .merge(
            folded,
            survivor,
            None,
            date(2026, 10, 15),
            &other_caller(),
            Some("session-merge-refusals"),
        )
        .await
}

/// **An already-folded side and a missing side are one refusal, and the first
/// side named decides it.** `alpha` was folded into `beta`; folding it into a
/// handle nobody holds is refused for `alpha`, because `alpha` is judged before
/// the survivor is looked for. Folding a handle nobody holds into the live `beta`
/// is the other order, refused for the missing one. **Paired**: the live pair
/// merges, so the refusals are about the sides and not about merging.
pub async fn a_merge_judges_each_side_in_turn_so_an_already_folded_side_is_refused_before_a_missing_one<
    M: Memory,
>(
    store: &M,
) {
    let alpha = EntityId("person:contract-refusal-alpha".into());
    let beta = EntityId("person:contract-refusal-beta".into());
    let gamma = EntityId("person:contract-refusal-gamma".into());
    let omega = EntityId("person:contract-refusal-omega".into());
    for handle in [&alpha, &beta, &gamma] {
        add(
            store,
            NewEntity::new(handle.clone(), handle.slug(), "contract-fixture"),
        )
        .await;
        capture(
            store,
            NewFact::about(handle.clone(), "a claim to carry", date(2026, 10, 15)),
        )
        .await;
    }
    fold(store, &alpha, &beta)
        .await
        .expect("alpha folds into beta");

    // The folded side comes first in the order, so it decides.
    let refused = fold(store, &alpha, &omega)
        .await
        .expect_err("a folded side is refused");
    assert!(
        matches!(&refused, MemoryError::AlreadyMerged { attempted, into }
            if attempted == alpha.as_str() && into == beta.as_str()),
        "the already-folded side is the refusal, not the missing survivor: {refused:?}"
    );
    // And the other order: a missing duplicate into a live survivor.
    let refused = fold(store, &omega, &beta)
        .await
        .expect_err("a missing side is refused");
    assert!(
        matches!(&refused, MemoryError::UnknownEntity { attempted, .. }
            if attempted == omega.as_str()),
        "the missing duplicate is the refusal: {refused:?}"
    );
    // The positive the refusals depend on: two live things merge.
    fold(store, &gamma, &beta)
        .await
        .expect("two live things merge");
}

/// **A parent cannot be folded into its own descendant, however deep.** The
/// served refusal's way forward is read at the surface, in the stories. The folded thing's children are served under
/// the survivor, so a survivor below it would be served as its own parent.
/// **Paired**: the child folds into the parent, and a thing beside the parent
/// takes it, so the refusal is about descent and not about parents.
pub async fn a_parent_cannot_be_folded_into_its_own_descendant<M: Memory>(store: &M) {
    let parent = EntityId("org:contract-parent-one".into());
    let child = EntityId("org:contract-child-one".into());
    let grandchild = EntityId("org:contract-grandchild-one".into());
    let sibling = EntityId("org:contract-sibling-one".into());
    add(
        store,
        NewEntity::new(parent.clone(), parent.slug(), "contract-fixture"),
    )
    .await;
    add(
        store,
        NewEntity {
            parent: Some(parent.clone()),
            ..NewEntity::new(child.clone(), child.slug(), "contract-fixture")
        },
    )
    .await;
    add(
        store,
        NewEntity {
            parent: Some(child.clone()),
            ..NewEntity::new(grandchild.clone(), grandchild.slug(), "contract-fixture")
        },
    )
    .await;
    add(
        store,
        NewEntity::new(sibling.clone(), sibling.slug(), "contract-fixture"),
    )
    .await;

    for below in [&child, &grandchild] {
        let refused = fold(store, &parent, below)
            .await
            .expect_err("a parent does not fold into its own descendant");
        assert!(
            matches!(&refused, MemoryError::MergeIntoDescendant { folded, survivor }
                if folded == parent.as_str() && survivor == below.as_str()),
            "the refusal names both sides and says the survivor sits below: {refused:?}"
        );
    }
    // Nothing moved: neither handle was folded.
    let held = store.list_entities(None).await.expect("lists");
    for side in [&parent, &child, &grandchild] {
        assert!(
            held.iter()
                .any(|e| &e.id == side && e.merged_into.is_none()),
            "{side} was not folded"
        );
    }
    // The positives the refusal depends on: a child folds into its parent, and
    // a thing beside the parent takes it.
    fold(store, &grandchild, &parent)
        .await
        .expect("a descendant folds into its ancestor");
    fold(store, &parent, &sibling)
        .await
        .expect("a parent folds into a thing beside it");
}
