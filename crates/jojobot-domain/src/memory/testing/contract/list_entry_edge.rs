//! **A list entry whose words name exactly one thing draws a connection edge to
//! it**, proven against any [`Memory`] adapter that resolves mentions. The edge
//! is what lets a list show a card's state and lets a walk from the card reach
//! the entry.

use super::support::{capture, ensure};
use super::*;
use crate::memory::{document, entries};

/// A list and two things to name, written the way `capture` writes an entry:
/// the claim carries an order key the server minted.
async fn a_list_and_two_things<M: Memory + ?Sized>(store: &M) -> (EntityId, EntityId, EntityId) {
    let list = EntityId("list:contract-edge-steps".to_string());
    let first = EntityId("thing:contract-edge-alpha".to_string());
    let second = EntityId("thing:contract-edge-beta".to_string());
    for id in [&list, &first, &second] {
        ensure(store, id).await;
    }
    (list, first, second)
}

async fn an_entry<M: Memory + ?Sized>(
    store: &M,
    list: &EntityId,
    words: &str,
    edge: Option<Edge>,
) -> Fact {
    capture(
        store,
        NewFact {
            provenance: Provenance::Testimony,
            fields: [(document::ORDER.to_string(), "m".to_string())]
                .into_iter()
                .collect(),
            edge,
            ..NewFact::about(list.clone(), words, date(2026, 11, 2))
        },
    )
    .await
}

/// The edge a list serves for the entry written with `written`'s id.
async fn edge_of<M: Memory + ?Sized>(store: &M, list: &EntityId, written: &Fact) -> Option<Edge> {
    let facts = store.recall(list).await.expect("the list reads");
    entries::open_entries(&facts)
        .into_iter()
        .find(|entry| entry.id == written.id)
        .expect("the entry is served")
        .edge
        .clone()
}

/// **One mention and no object: the edge is drawn to the mentioned thing.**
pub async fn a_list_entry_naming_one_thing_draws_a_connection_edge<M: Memory + ?Sized>(store: &M) {
    let (list, first, _) = a_list_and_two_things(store).await;
    let written = an_entry(store, &list, &format!("carry @{}", first.as_str()), None).await;
    let edge = edge_of(store, &list, &written)
        .await
        .expect("an entry naming one thing draws an edge");
    assert_eq!(edge.shape, EdgeShape::Connection);
    assert_eq!(edge.object, first);
}

/// **Two mentions and no object: no edge, because either could be the one.**
pub async fn a_list_entry_naming_two_things_draws_no_edge<M: Memory + ?Sized>(store: &M) {
    let (list, first, second) = a_list_and_two_things(store).await;
    let words = format!("carry @{} then @{}", first.as_str(), second.as_str());
    let written = an_entry(store, &list, &words, None).await;
    assert_eq!(edge_of(store, &list, &written).await, None);
}

/// **An object the caller sent wins over the mention.**
pub async fn a_list_entry_with_an_object_keeps_it<M: Memory + ?Sized>(store: &M) {
    let (list, first, second) = a_list_and_two_things(store).await;
    let sent = Edge::new(EdgeShape::About, second.clone());
    let written = an_entry(
        store,
        &list,
        &format!("carry @{}", first.as_str()),
        Some(sent.clone()),
    )
    .await;
    assert_eq!(edge_of(store, &list, &written).await, Some(sent));
}

/// **A claim about a thing that is not a list entry draws no edge from its words.**
pub async fn a_claim_that_is_not_a_list_entry_draws_no_edge_from_its_words<M: Memory + ?Sized>(
    store: &M,
) {
    let (_, first, second) = a_list_and_two_things(store).await;
    let written = capture(
        store,
        NewFact::about(
            second.clone(),
            format!("waits on @{}", first.as_str()),
            date(2026, 11, 2),
        ),
    )
    .await;
    assert_eq!(written.edge, None);
}
