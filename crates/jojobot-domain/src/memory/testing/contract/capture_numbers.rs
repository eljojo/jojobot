//! **The numbers a capture takes, proven against any [`Memory`] adapter.** A
//! write is numbered past every write the thing holds under its key, so a thing
//! that took in the writes of another by a merge still reads its newest write as
//! the newest.
//!
//! Run by one test of each store, not listed with the contract, so the contract's
//! file keeps its size.

use super::role_claims::folded_fields_of;
use super::support::{add, capture, ensure, other_caller, state};
use super::*;

/// **A write made after a merge is the newest of its key.** Both things wrote the
/// key, one of them twice; the merge folds one into the other; a new write of the
/// key on the survivor is read back, and so is the one after it. A number taken
/// by counting the survivor's own writes would collide with a folded write and
/// leave an older value standing.
pub async fn a_write_after_a_merge_is_the_newest_of_its_key<M: Memory>(store: &M) {
    let survivor = EntityId("thing:contract-numbers-survivor".into());
    let folded = EntityId("thing:contract-numbers-folded".into());
    for handle in [&survivor, &folded] {
        add(
            store,
            NewEntity::new(handle.clone(), handle.as_str(), "contract-fixture"),
        )
        .await;
    }
    let write = |home: &EntityId, value: &str, day: i8| {
        state(NewFact {
            fields: [("numbers_key".to_string(), value.to_string())]
                .into_iter()
                .collect(),
            ..NewFact::about(home.clone(), "a setting", date(2026, 7, day))
        })
    };
    capture(store, write(&survivor, "survivor-first", 1)).await;
    capture(store, write(&folded, "folded-first", 2)).await;
    capture(store, write(&folded, "folded-second", 3)).await;
    store
        .merge(
            &folded,
            &survivor,
            Some("one thing, filed twice"),
            date(2026, 7, 4),
            &other_caller(),
            None,
        )
        .await
        .expect("the fold lands");
    ensure(store, &survivor).await;
    for value in ["after-the-merge", "and-again"] {
        capture(store, write(&survivor, value, 5)).await;
        let fields = folded_fields_of(store, &survivor).await;
        assert_eq!(
            fields.get("numbers_key").map(String::as_str),
            Some(value),
            "a write after a merge reads back as the newest: {fields:?}"
        );
    }
    // **The key's history lists the writes in the order they were numbered**, so
    // a number that collided with a folded write would show here as an order that
    // is not the order of writing.
    let found = graph::walk(
        store,
        &[],
        &graph::GraphQuery {
            select: graph::Selection {
                subject: Some(survivor.clone()),
                ..graph::Selection::default()
            },
            include: graph::Include {
                facts: false,
                prose: false,
                stood_for: false,
                counted: false,
                status: None,
            },
            follow: None,
            history: Some(graph::History::of("numbers_key")),
            masked: Default::default(),
        },
    )
    .await
    .expect("a handle is a selection")
    .objects;
    let history = found[0]
        .history
        .as_ref()
        .expect("the query named a key, so the object carries its writes");
    assert_eq!(
        history
            .writes
            .iter()
            .map(|w| w.value.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some("survivor-first"),
            Some("folded-first"),
            Some("folded-second"),
            Some("after-the-merge"),
            Some("and-again"),
        ],
        "the writes of the key read in the order they were made"
    );
}
