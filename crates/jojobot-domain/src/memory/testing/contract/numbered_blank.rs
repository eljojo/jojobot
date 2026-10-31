//! **An edit that sets a blank number is refused as a capture is**, proven
//! against any [`Memory`] adapter.
//!
//! The fold reads a blank value as unset, so a write that sets a member's number
//! blank leaves the member holding none. A member that held a number is kept by
//! the keeps-its-number guard; a member that held none is held only by the bad
//! number guard, which needs the write to say it set the key blank.

use super::numbered::ordinances;
use super::support::{add, capture, edit, other_caller, state, state_patch};
use super::*;
use std::collections::BTreeMap;

/// **A claim edited to a blank number on a member with no number is refused,
/// and nothing is written.** Paired with what must still land: a real number on
/// the same claim, and a blank value of an ordinary key, which clears it.
pub async fn an_edit_that_sets_a_blank_number_is_refused_like_a_capture<M: Memory>(store: &M) {
    let o = ordinances(
        store,
        "Hamlet edicts",
        [
            "topic:contract-ordinances-blank",
            "decision:contract-ordinance-blank-curfew",
            "decision:contract-ordinance-blank-parking",
            "decision:contract-ordinance-blank-leaves",
            "decision:contract-ordinance-blank-fireworks",
        ],
    )
    .await;
    let loose = EntityId("decision:contract-ordinance-blank-loose".into());
    add(
        store,
        NewEntity {
            parent: Some(o.collection.clone()),
            ..NewEntity::new(loose.clone(), "Loose edict", "contract-fixture")
        },
    )
    .await;
    let claim = capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("title".to_string(), "Loose".to_string())]),
            ..NewFact::about(loose.clone(), "an edict without a number", date(2026, 5, 3))
        }),
    )
    .await;
    let set = |key: &str, value: &str| {
        state_patch(FactPatch {
            fields: BTreeMap::from([(key.to_string(), value.to_string())]),
            ..FactPatch::default()
        })
    };
    let next = || async {
        store
            .numbering_of(&o.collection)
            .await
            .expect("the collection reads")
            .expect("it numbers its members")
            .next_number
    };
    assert_eq!(next().await, 5, "1, 2 and 4 are held, and 3 is retired");

    for blank in ["", " \t"] {
        let refused = store
            .update_fact(&claim.address(), set("number", blank), &other_caller())
            .await;
        assert!(
            matches!(refused, Err(MemoryError::NotANumber { .. })),
            "{blank:?} on an unnumbered member is a bad number, as a capture says: {refused:?}"
        );
    }
    assert_eq!(next().await, 5, "the refusals wrote nothing");
    assert!(
        !store
            .fields(&loose)
            .await
            .expect("the member reads")
            .contains_key("number"),
        "the member still holds no number"
    );

    // A blank value of a key that is no number clears the key.
    edit(store, &claim.address(), set("title", "")).await;
    assert!(
        !store
            .fields(&loose)
            .await
            .expect("the member reads")
            .contains_key("title"),
        "an ordinary key set blank is cleared"
    );

    // A real number on the same claim lands.
    edit(store, &claim.address(), set("number", "7")).await;
    assert_eq!(next().await, 8, "7 is held by the member now");
}
