//! **A numbered member's address is accepted wherever a write names a thing**,
//! proven against any [`Memory`] adapter.
//!
//! `recall` takes `<collection>/<N>` as a subject and answers the member that
//! holds N. A write that names a thing by something other than its subject (an
//! edge's object, a ref, a field that holds a handle) used to run a literal
//! check on the id and refused the address with name candidates. **Each case
//! sends the address through one such place, reads the record back, and holds
//! that the member itself was stored**, so a rename of the member would follow;
//! a number nobody holds is still refused, beside it.

use super::numbered::{numbered, ordinances};
use super::support::state;
use super::*;
use std::collections::BTreeMap;

/// **The collection every case here uses, built once on a store**: numbered 1, 2,
/// 3 (archived) and 4. The case writes on its first member, so no fixture beyond
/// the collection's own is needed. The handles are ones the roster already holds
/// and the mention cases of the numbered contract use; they are free on a store
/// that does not run those.
async fn world<M: Memory + ?Sized>(store: &M) -> (super::numbered::Ordinances, EntityId) {
    let handles = [
        "topic:contract-ordinances-mention",
        "decision:contract-ordinance-mention-curfew",
        "decision:contract-ordinance-mention-parking",
        "decision:contract-ordinance-mention-leaves",
        "decision:contract-ordinance-mention-fireworks",
    ];
    let [collection, curfew, parking, leaves, fireworks] = handles.map(|h| EntityId(h.to_string()));
    let built = store
        .list_entities(None)
        .await
        .expect("the store lists")
        .iter()
        .any(|e| e.id == collection);
    let o = if built {
        super::numbered::Ordinances {
            collection,
            curfew,
            parking,
            fireworks,
            leaves,
        }
    } else {
        ordinances(store, "Typed ordinances", handles).await
    };
    let author = o.curfew.clone();
    (o, author)
}

/// What a claim's field reads back as, by the claim's words: the stored value,
/// not the echo of what was sent.
async fn held_field<M: Memory + ?Sized>(
    store: &M,
    about: &EntityId,
    content: &str,
    key: &str,
) -> Option<String> {
    let read = store.recall(about).await.expect("the writer reads");
    read.iter()
        .find(|fact| fact.content == content)
        .and_then(|fact| fact.fields.get(key).cloned())
}

/// **An edge's object may be a member's address**, and the edge is stored at the
/// member. The same address for a number nobody holds is blocked.
pub async fn a_numbered_address_is_an_edge_object_and_the_member_is_what_is_stored<M: Memory>(
    store: &M,
) {
    let (o, author) = world(store).await;
    let edge = |object: EntityId| Edge {
        shape: EdgeShape::Connection,
        object,
    };
    let landed = store
        .capture(NewFact {
            edge: Some(edge(numbered(&o.collection, 2))),
            ..NewFact::about(author.clone(), "sees the parking rule", date(2026, 6, 1))
        })
        .await
        .expect("the capture is answered")
        .written()
        .expect("a number a member holds is an edge's object");
    assert_eq!(
        landed.edge.as_ref().map(|e| &e.object),
        Some(&o.parking),
        "the edge is served at the member: {landed:?}"
    );
    let read = store.recall(&author).await.expect("the author reads");
    assert!(
        read.iter()
            .any(|f| f.edge.as_ref().map(|e| &e.object) == Some(&o.parking)),
        "the stored edge points at the member, not at the spelling: {read:?}"
    );

    let blocked = store
        .capture(NewFact {
            edge: Some(edge(numbered(&o.collection, 9))),
            ..NewFact::about(author, "sees a rule nobody holds", date(2026, 6, 2))
        })
        .await
        .expect("the capture is answered");
    assert!(
        matches!(blocked, Guarded::Blocked { .. }),
        "a number nobody holds is still no object: {blocked:?}"
    );
}

/// **A ref may be a member's address**, and the member is what is stored.
pub async fn a_numbered_address_is_a_ref_and_the_member_is_what_is_stored<M: Memory>(store: &M) {
    let (o, author) = world(store).await;
    let landed = store
        .capture(NewFact {
            refs: vec![numbered(&o.collection, 2)],
            ..NewFact::about(author.clone(), "cites the parking rule", date(2026, 6, 1))
        })
        .await
        .expect("the capture is answered")
        .written()
        .expect("a number a member holds is a ref");
    assert_eq!(landed.refs, vec![o.parking.clone()], "{landed:?}");
    let blocked = store
        .capture(NewFact {
            refs: vec![numbered(&o.collection, 9)],
            ..NewFact::about(author, "cites a rule nobody holds", date(2026, 6, 2))
        })
        .await
        .expect("the capture is answered");
    assert!(matches!(blocked, Guarded::Blocked { .. }), "{blocked:?}");
}

/// **A field whose value names a member by its address links the member**, under
/// a key nobody declared and under a key declared to hold a reference.
pub async fn a_numbered_address_in_a_field_links_the_member<M: Memory>(store: &M) {
    let (o, author) = world(store).await;
    let parking_address = numbered(&o.collection, 2).to_string();
    let fields = |key: &str, value: &str| BTreeMap::from([(key.to_string(), value.to_string())]);
    let landed = store
        .capture(state(NewFact {
            fields: fields("governed_by", &parking_address),
            ..NewFact::about(
                author.clone(),
                "is governed by the parking rule",
                date(2026, 6, 1),
            )
        }))
        .await
        .expect("the capture is answered")
        .written()
        .expect("a number a member holds in a field is a link");
    assert_eq!(
        held_field(store, &author, &landed.content, "governed_by").await,
        Some(o.parking.to_string()),
        "the field reads back as the member's handle"
    );
    let blocked = store
        .capture(state(NewFact {
            fields: fields("governed_by", &numbered(&o.collection, 9).to_string()),
            ..NewFact::about(author.clone(), "is governed by nothing", date(2026, 6, 2))
        }))
        .await
        .expect("the capture is answered");
    assert!(matches!(blocked, Guarded::Blocked { .. }), "{blocked:?}");

    store
        .declare_type(
            DeclaredType::new(
                "typed-rule-keeper",
                vec![Field::new("keeps", ValueType::Reference)],
            ),
            None,
        )
        .await
        .expect("the type is declared");
    let declared = store
        .capture(state(NewFact {
            fields: fields("keeps", &parking_address),
            ..NewFact::about(author.clone(), "keeps the parking rule", date(2026, 6, 3))
        }))
        .await
        .expect("the capture is answered")
        .written()
        .expect("a number a member holds under a reference key is a link");
    assert_eq!(
        held_field(store, &author, &declared.content, "keeps").await,
        Some(o.parking.to_string()),
        "the reference field reads back as the member's handle"
    );
    let blocked = store
        .capture(state(NewFact {
            fields: fields("keeps", &numbered(&o.collection, 9).to_string()),
            ..NewFact::about(author, "keeps no rule", date(2026, 6, 4))
        }))
        .await
        .expect("the capture is answered");
    assert!(matches!(blocked, Guarded::Blocked { .. }), "{blocked:?}");
}

/// Every spec above, in one call.
pub async fn run_all_typed_addresses<M: Memory>(store: &M) {
    a_numbered_address_is_an_edge_object_and_the_member_is_what_is_stored(store).await;
    a_numbered_address_is_a_ref_and_the_member_is_what_is_stored(store).await;
    a_numbered_address_in_a_field_links_the_member(store).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn fake_takes_a_numbered_address_as_an_edge_object() {
        a_numbered_address_is_an_edge_object_and_the_member_is_what_is_stored(
            &InMemoryMemory::booted(),
        )
        .await;
    }

    #[tokio::test]
    async fn fake_takes_a_numbered_address_as_a_ref() {
        a_numbered_address_is_a_ref_and_the_member_is_what_is_stored(&InMemoryMemory::booted())
            .await;
    }

    #[tokio::test]
    async fn fake_links_the_member_a_numbered_address_in_a_field_names() {
        a_numbered_address_in_a_field_links_the_member(&InMemoryMemory::booted()).await;
    }
}
