//! **The review stamp is part of its write.**

use super::support::{add, capture};
use super::*;

/// **The review stamp is written in the same act as the rest of its write, so it
/// is never a later change than the write that carries it** (rule 458). A claim
/// carries the stamp beside keys that sort before and after it; the thing's
/// ages then say the thing is reviewed. **Paired**: a plain write after it puts
/// the thing back in the queue, so a store that held every thing reviewed would
/// not pass.
pub async fn a_review_stamp_is_not_a_later_change_than_the_write_that_carries_it<M: Memory>(
    store: &M,
) {
    use crate::memory::review::{DEFRAGGED_AT, is_unreviewed};
    let thing = EntityId("thing:contract-reviewed".into());
    add(
        store,
        NewEntity::new(thing.clone(), "Contract Reviewed", "contract-fixture"),
    )
    .await;
    let named = |keys: &[&str]| -> std::collections::BTreeSet<String> {
        keys.iter().map(|k| k.to_string()).collect()
    };
    capture(
        store,
        NewFact {
            fields: [
                ("alpha_key", "a"),
                (DEFRAGGED_AT, "2026-09-01T00:00:00Z"),
                ("zeta_key", "z"),
            ]
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect(),
            sets: named(&["alpha_key", "zeta_key"]),
            derived: named(&[DEFRAGGED_AT]),
            session: Some("session-reviewed".to_string()),
            ..NewFact::about(thing.clone(), "a write by defrag", date(2026, 9, 1))
        },
    )
    .await;
    let ages = store.field_ages(&thing).await.expect("ages read");
    assert!(
        ages.keys.contains_key(DEFRAGGED_AT),
        "the stamp reached the thing: {ages:?}"
    );
    assert!(
        !is_unreviewed(&ages),
        "the write that carries the stamp is not a later change than itself: {ages:?}"
    );
    capture(
        store,
        NewFact {
            fields: [("alpha_key", "b")]
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            sets: named(&["alpha_key"]),
            session: Some("session-reviewed".to_string()),
            ..NewFact::about(thing.clone(), "a write by somebody else", date(2026, 9, 2))
        },
    )
    .await;
    let after = store.field_ages(&thing).await.expect("ages read");
    assert!(
        is_unreviewed(&after),
        "a later write by anybody else puts the thing back: {after:?}"
    );
}
