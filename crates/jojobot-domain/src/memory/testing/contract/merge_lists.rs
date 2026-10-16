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
async fn a_list_of<M: Memory>(
    store: &M,
    list: &EntityId,
    name: &str,
    texts: [&str; 3],
) -> Vec<Fact> {
    add(
        store,
        NewEntity::new(list.clone(), name, "contract-fixture"),
    )
    .await;
    let keys = crate::memory::order::spread(None, None, texts.len()).expect("room");
    let mut written = Vec::new();
    for (text, key) in texts.into_iter().zip(keys) {
        written.push(
            capture(
                store,
                NewFact {
                    provenance: Provenance::Testimony,
                    fields: [(document::ORDER.to_string(), key)].into_iter().collect(),
                    session: Some("session-list".to_string()),
                    ..NewFact::about(list.clone(), text, date(2026, 10, 16))
                },
            )
            .await,
        );
    }
    written
}

/// One tick on a list: a record beside the entries naming one entry and an act.
async fn a_tick<M: Memory>(store: &M, list: &EntityId, entry: &Fact) {
    capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            fields: [
                (entries::TICK_ENTRY.to_string(), entry.id.0.clone()),
                (entries::TICK_ACT.to_string(), "done".to_string()),
            ]
            .into_iter()
            .collect(),
            session: Some("session-tick".to_string()),
            ..NewFact::about(list.clone(), "done it", date(2026, 10, 16))
        },
    )
    .await;
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

/// **Two lists merge and every tick still marks the entry it was made for.** A
/// folded entry takes a new id on the survivor, where the id it had may name an
/// entry of the survivor's own, so a tick that kept its old entry id would mark
/// the wrong entry. Each list ticks one entry done before the merge, and the
/// survivor reads those same entries done after it.
pub async fn a_merged_lists_ticks_follow_their_entries<M: Memory>(store: &M) {
    let into = EntityId("list:contract-ticks-into".into());
    let away = EntityId("list:contract-ticks-away".into());
    let kept = a_list_of(
        store,
        &into,
        "The Ticked Survivor",
        ["tent", "stove", "lantern"],
    )
    .await;
    let folded = a_list_of(
        store,
        &away,
        "The Ticked Duplicate",
        ["letter", "bread", "bins"],
    )
    .await;
    a_tick(store, &into, &kept[1]).await;
    a_tick(store, &away, &folded[0]).await;
    a_tick(store, &away, &folded[2]).await;

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

    let facts = store.recall(&into).await.expect("the survivor reads after");
    let view = entries::view(&facts, entries::Setting::Sink);
    let read: Vec<(&str, &str)> = view
        .entries
        .iter()
        .map(|shown| (shown.fact.content.as_str(), shown.state))
        .collect();
    assert_eq!(
        read,
        [
            ("tent", "open"),
            ("lantern", "open"),
            ("bread", "open"),
            ("stove", "done"),
            ("letter", "done"),
            ("bins", "done"),
        ],
        "each tick marks the entry it was made for"
    );
}
