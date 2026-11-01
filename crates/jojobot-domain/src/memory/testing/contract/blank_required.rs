//! **A blank written to a key the kind requires is refused, as taking the key
//! off is.** A blank means unset, so it may not do what a clear may not.
//!
//! The shared contract runs this refusal against both stores.

use super::support::{add, capture, ensure, other_caller, state, state_patch};
use super::*;

/// **A promise that holds its day refuses a blank day, and a blank in a key the
/// kind does not require goes through and clears it.** Paired in one case: a
/// store that refused every blank passes the first half alone, and one that let
/// the day go passes the second alone.
pub async fn a_blank_day_on_a_promise_is_refused_like_a_clear<M: Memory>(store: &M) {
    crate::memory::kinds::seed(store)
        .await
        .expect("the kinds are seeded");
    let owner = EntityId("thing:contract-kettle".into());
    ensure(store, &owner).await;
    let promise = EntityId("promise:contract-borrowed-wrench".into());
    add(
        store,
        NewEntity {
            parent: Some(owner),
            ..NewEntity::new(promise.clone(), "Return The Wrench", "contract-fixture")
        },
    )
    .await;
    let dated = capture(
        store,
        state(NewFact {
            fields: [
                ("promised_by".to_string(), "2026-07-01".to_string()),
                ("note".to_string(), "borrowed it".to_string()),
            ]
            .into_iter()
            .collect(),
            ..NewFact::about(promise, "has to go back", date(2026, 6, 1))
        }),
    )
    .await;
    let blank = |key: &str| {
        state_patch(FactPatch {
            fields: [(key.to_string(), String::new())].into_iter().collect(),
            ..Default::default()
        })
    };
    let before = store.recall(&dated.home).await.unwrap();
    let history = store.history(&dated.home, "promised_by").await.unwrap();
    let day_blanked = store
        .update_fact(&dated.address(), blank("promised_by"), &other_caller())
        .await;
    assert!(
        matches!(day_blanked, Err(MemoryError::BreaksFit { .. })),
        "a promise that holds its day may not lose it to a blank: {day_blanked:?}",
    );
    assert_eq!(store.recall(&dated.home).await.unwrap(), before);
    assert_eq!(
        store.history(&dated.home, "promised_by").await.unwrap(),
        history
    );
    store
        .update_fact(&dated.address(), blank("note"), &other_caller())
        .await
        .expect("a blank in a key the kind does not require goes freely");
}
