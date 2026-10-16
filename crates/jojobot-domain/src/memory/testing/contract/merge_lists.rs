//! **Merging two lists leaves one list holding both, each in its own order**
//! (bug 2468), proven against any [`Memory`] adapter. The folded list's entries
//! follow the survivor's last entry in their own order, every entry keeps its
//! words and the provenance it was written with, and nothing the survivor's
//! people ordered is shuffled.

use super::support::{add, capture, other_caller};
use super::*;
use crate::memory::{document, entries};

/// One list with three entries, written the way `capture` writes them: each
/// entry a claim on the list carrying an order key the server minted, here from
/// the same spread a capture's placement mints from.
async fn a_list_of<M: Memory>(store: &M, list: &EntityId, name: &str, texts: [&str; 3]) {
    add(
        store,
        NewEntity::new(list.clone(), name, "contract-fixture"),
    )
    .await;
    let keys = crate::memory::order::spread(None, None, texts.len()).expect("room");
    for (text, key) in texts.into_iter().zip(keys) {
        capture(
            store,
            NewFact {
                provenance: Provenance::Testimony,
                fields: [(document::ORDER.to_string(), key)].into_iter().collect(),
                session: Some("session-list".to_string()),
                ..NewFact::about(list.clone(), text, date(2026, 10, 16))
            },
        )
        .await;
    }
}

/// The entries a list serves, in order, as `(words, provenance)`.
fn served(facts: &[Fact]) -> Vec<(String, Provenance)> {
    entries::open_entries(facts)
        .into_iter()
        .map(|fact| (fact.content.clone(), fact.provenance))
        .collect()
}

/// **Two lists of three entries merge into one of six.** The survivor's entries
/// come first in their order, then the folded list's in theirs, and each entry is
/// still the operator's testimony.
pub async fn two_lists_merged_keep_each_lists_order<M: Memory>(store: &M) {
    let into = EntityId("list:contract-merged-into".into());
    let away = EntityId("list:contract-merged-away".into());
    // Named by hand, for the reason the document case names its own: the guard
    // takes two lists whose names share a word.
    a_list_of(
        store,
        &into,
        "The Survivor List",
        ["tent", "stove", "lantern"],
    )
    .await;
    a_list_of(store, &away, "The Folded List", ["letter", "bread", "bins"]).await;

    let before = served(&store.recall(&into).await.expect("the survivor reads"));
    assert_eq!(before.len(), 3, "{before:?}");

    store
        .merge(
            &away,
            &into,
            Some("one list under two names"),
            date(2026, 10, 16),
            &other_caller(),
            Some("session-merge"),
        )
        .await
        .expect("the lists merge");

    let facts_after = store.recall(&into).await.expect("the survivor reads after");
    let after = served(&facts_after);
    let words: Vec<&str> = after.iter().map(|(words, _)| words.as_str()).collect();
    assert_eq!(
        words,
        ["tent", "stove", "lantern", "letter", "bread", "bins"],
        "the survivor's entries, then the folded list's in their own order"
    );
    assert!(
        after
            .iter()
            .all(|(_, provenance)| *provenance == Provenance::Testimony),
        "every entry kept the provenance it was written with: {after:?}"
    );
}
