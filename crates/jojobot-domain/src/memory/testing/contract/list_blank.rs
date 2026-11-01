//! **An emptied list key keeps its key, and a single-valued key does not**
//! (rule 751), proven against any [`Memory`] adapter.

use super::support::{capture, edit, ensure};
use super::*;
use std::collections::BTreeMap;

fn both(guests: &str, venue: &str) -> BTreeMap<String, String> {
    BTreeMap::from([
        ("guests".to_string(), guests.to_string()),
        ("venue".to_string(), venue.to_string()),
    ])
}

/// **A blank written to a list key leaves the key holding an empty list; the
/// same blank written to a single-valued key takes the key off.** Paired in one
/// case: a store that dropped every blank passes the second half alone, and one
/// that kept every blank passes the first alone.
pub async fn a_blank_keeps_a_list_key_and_takes_a_single_valued_key_off<M: Memory>(store: &M) {
    store
        .declare_type(
            DeclaredType::new(
                "contract-outing",
                vec![
                    Field::listing("guests", ValueType::Text),
                    Field::new("venue", ValueType::Text),
                ],
            ),
            None,
        )
        .await
        .expect("the type is declared");
    let thing = EntityId("thing:contract-outing-list".into());
    ensure(store, &thing).await;
    let claim = capture(
        store,
        NewFact {
            fields: both("alpha,beta", "the rink"),
            sets: ["guests".to_string(), "venue".to_string()].into(),
            ..NewFact::about(thing.clone(), "the outing", date(2026, 10, 1))
        },
    )
    .await;
    let held = store.fields(&thing).await.expect("fields read");
    assert_eq!(held.get("guests").map(String::as_str), Some("alpha,beta"));
    assert_eq!(held.get("venue").map(String::as_str), Some("the rink"));

    edit(
        store,
        &claim.address(),
        FactPatch {
            fields: both("", ""),
            sets: ["guests".to_string(), "venue".to_string()].into(),
            ..FactPatch::default()
        },
    )
    .await;
    let held = store.fields(&thing).await.expect("fields read");
    assert_eq!(
        held.get("guests").map(String::as_str),
        Some(""),
        "an emptied list keeps its key: {held:?}"
    );
    assert!(
        !held.contains_key("venue"),
        "a blank single-valued key is off the thing: {held:?}"
    );
}
