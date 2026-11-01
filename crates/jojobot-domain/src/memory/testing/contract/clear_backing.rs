//! **A clear is held to the claim that backs the value** (the architect's ruling,
//! 2026-11-01), proven against any [`Memory`] adapter.

use super::support::{capture, edit, ensure, other_caller};
use super::*;
use std::collections::BTreeMap;

fn mood(value: &str) -> BTreeMap<String, String> {
    BTreeMap::from([("mood".to_string(), value.to_string())])
}

/// **A clear sent to a claim whose write of the key is not the one the thing
/// holds is refused, naming the claim that backs the value; a clear sent to that
/// claim takes the key off.** One case, both halves: a store that refused every
/// clear passes the first half alone, and one that took every clear passes the
/// second alone. The older claim still holds the key on its own record (so the
/// clear is a write), and a newer claim holds the value the thing reads.
pub async fn a_clear_on_an_older_claim_is_refused_and_one_on_the_backing_claim_takes_the_key_off<
    M: Memory,
>(
    store: &M,
) {
    let thing = EntityId("thing:contract-clear-older".into());
    ensure(store, &thing).await;
    let older = capture(
        store,
        NewFact {
            fields: mood("content"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(thing.clone(), "felt content", date(2026, 10, 1))
        },
    )
    .await;
    let newer = capture(
        store,
        NewFact {
            fields: mood("queasy"),
            sets: ["mood".to_string()].into(),
            ..NewFact::about(thing.clone(), "felt queasy", date(2026, 10, 2))
        },
    )
    .await;

    let clear = || FactPatch {
        clear_fields: vec!["mood".to_string()],
        ..FactPatch::default()
    };
    let refused = store
        .update_fact(&older.address(), clear(), &other_caller())
        .await;
    match refused {
        Err(MemoryError::ClearsAnOlderWrite { key, backing, .. }) => {
            assert_eq!(key, "mood");
            assert_eq!(
                backing,
                newer.id.as_str(),
                "the refusal names the backing claim"
            );
        }
        other => panic!("a clear on the older claim must be refused, got {other:?}"),
    }
    let held = store.fields(&thing).await.expect("fields read");
    assert_eq!(
        held.get("mood").map(String::as_str),
        Some("queasy"),
        "the newer value survived the refused clear: {held:?}"
    );

    edit(store, &newer.address(), clear()).await;
    let held = store.fields(&thing).await.expect("fields read");
    assert!(
        !held.contains_key("mood"),
        "a clear on the backing claim takes the key off: {held:?}"
    );
}
