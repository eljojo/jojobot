//! **A claim a newer claim has displaced as the backing of a key is not
//! rewritten.** The revision of a claim counts the writes of that claim only, so
//! a second claim that set the same key leaves the first at revision 1; an edit
//! that names the keys the claim must still back is refused when another claim
//! backs one of them.
//!
//! The case is run by one test of each store (the fake, and the real store in the
//! adapters' tests), not listed with the contract, so the contract's file keeps its size.

use super::support::{capture, edit, other_caller, state, state_patch};
use super::*;
use std::collections::{BTreeMap, BTreeSet};

fn setting(key: &str, value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([(key.to_string(), value.to_string())])
}

/// **The edit lands while its claim backs the key, and is refused once another
/// claim does.** Paired in one case: a store that refused every edit naming
/// `must_back` passes the second half alone, and one that ignored it passes the
/// first alone.
pub async fn a_rewrite_of_a_claim_a_newer_claim_displaced_is_refused<M: Memory>(store: &M) {
    let thing = EntityId("thing:contract-must-back-oak".into());
    super::support::ensure(store, &thing).await;
    let older = capture(
        store,
        state(NewFact {
            fields: setting("agent", "a1"),
            ..NewFact::about(thing.clone(), "the first claim", date(2026, 9, 1))
        }),
    )
    .await;
    let address = older.address();
    let must_back: BTreeSet<String> = BTreeSet::from(["agent".to_string()]);

    // Nothing newer: the claim backs the key and the rewrite lands.
    edit(
        store,
        &address,
        state_patch(FactPatch {
            fields: setting("agent", "a2"),
            expected_revision: Some(1),
            must_back: must_back.clone(),
            ..Default::default()
        }),
    )
    .await;

    // A second claim sets the key: the first claim stays at revision 2, and a
    // rewrite that read it there must be refused, with nothing written.
    let newer = capture(
        store,
        state(NewFact {
            fields: setting("agent", "a3"),
            ..NewFact::about(thing.clone(), "the second claim", date(2026, 9, 2))
        }),
    )
    .await;
    let refused = store
        .update_fact(
            &address,
            state_patch(FactPatch {
                fields: setting("agent", "a4"),
                expected_revision: Some(2),
                must_back,
                ..Default::default()
            }),
            &other_caller(),
        )
        .await;
    assert!(
        matches!(refused, Err(MemoryError::StaleRevision { .. })),
        "a claim another claim displaced was rewritten: {refused:?}"
    );
    let held = store.fields(&thing).await.expect("the fields read");
    assert_eq!(
        held.get("agent").map(String::as_str),
        Some("a3"),
        "{held:?}"
    );
    let writes = store
        .claim_history(&address)
        .await
        .expect("the history reads")
        .len();
    assert_eq!(writes, 2, "the refused edit appended no write");
    assert_eq!(
        store
            .claim_history(&newer.address())
            .await
            .expect("the history reads")
            .len(),
        1
    );
}
