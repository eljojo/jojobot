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
pub(super) async fn world<M: Memory + ?Sized>(
    store: &M,
) -> (super::numbered::Ordinances, EntityId) {
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

/// **A member's address selects the row an edit or an archive changes.** The
/// edit lands on the member and answers the member; a number nobody holds is an
/// unknown entity, as it is for a read.
pub async fn a_numbered_address_edits_and_archives_the_member<M: Memory>(store: &M) {
    let (o, _) = world(store).await;
    let edited = store
        .update_entity(
            &numbered(&o.collection, 2),
            EntityPatch {
                aliases: Some(vec!["the parking rule".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("the edit is answered")
        .written()
        .expect("a number a member holds selects the member");
    assert_eq!(edited.id, o.parking, "{edited:?}");
    let read = store.list_entities(None).await.expect("the store lists");
    let parking = read.iter().find(|e| e.id == o.parking).expect("the member");
    assert_eq!(parking.aliases, ["the parking rule"], "{parking:?}");

    let missed = store
        .update_entity(
            &numbered(&o.collection, 9),
            EntityPatch {
                aliases: Some(vec!["nobody".to_string()]),
                ..Default::default()
            },
        )
        .await;
    assert!(
        matches!(missed, Err(MemoryError::UnknownEntity { .. })),
        "a number nobody holds is no row: {missed:?}"
    );

    let archived = store
        .archive_entity(&numbered(&o.collection, 4), "superseded", None)
        .await
        .expect("a number a member holds selects the member");
    assert_eq!(archived.id, o.fireworks, "{archived:?}");
    assert!(archived.archived.is_some(), "{archived:?}");
    let missed = store
        .archive_entity(&numbered(&o.collection, 9), "superseded", None)
        .await;
    assert!(
        matches!(missed, Err(MemoryError::UnknownEntity { .. })),
        "{missed:?}"
    );

    let (restored, _) = store
        .restore_entity(&numbered(&o.collection, 4), None)
        .await
        .expect("a number a member holds selects the member");
    assert_eq!(restored.id, o.fireworks, "{restored:?}");
    assert!(restored.archived.is_none(), "{restored:?}");
}

/// **An edit of a claim may replace its edge or a handle-valued field with a
/// member's address**, and the member is what is stored; a number nobody holds
/// is blocked and leaves the claim as it was.
pub async fn a_numbered_address_replaces_an_edge_and_a_field_by_an_edit<M: Memory>(store: &M) {
    let (o, author) = world(store).await;
    let made = store
        .capture(NewFact {
            edge: Some(Edge {
                shape: EdgeShape::Connection,
                object: o.curfew.clone(),
            }),
            ..NewFact::about(author.clone(), "will be re-pointed", date(2026, 6, 8))
        })
        .await
        .expect("the capture is answered")
        .written()
        .expect("the claim lands");
    // The key is declared to hold a reference, as the field case declares it; a
    // store that ran that case already holds the declaration.
    let _ = store
        .declare_type(
            DeclaredType::new(
                "typed-rule-keeper",
                vec![Field::new("keeps", ValueType::Reference)],
            ),
            None,
        )
        .await;
    let to = |edge: Option<u64>, keeps: Option<u64>| FactPatch {
        edge: edge.map(|n| Edge {
            shape: EdgeShape::Connection,
            object: numbered(&o.collection, n),
        }),
        fields: keeps
            .map(|n| {
                BTreeMap::from([("keeps".to_string(), numbered(&o.collection, n).to_string())])
            })
            .unwrap_or_default(),
        ..Default::default()
    };
    // Each refusal leaves the claim as it was: the edge it began with, no field.
    for (what, patch) in [
        ("an unknown edge alone", to(Some(9), None)),
        ("an unknown field alone", to(None, Some(9))),
        ("a valid edge beside an unknown field", to(Some(2), Some(9))),
        ("a valid field beside an unknown edge", to(Some(9), Some(2))),
    ] {
        let blocked = store
            .update_fact(&made.address(), patch, &author)
            .await
            .expect("the edit is answered");
        assert!(
            matches!(blocked, Guarded::Blocked { .. }),
            "{what}: {blocked:?}"
        );
        let now = claim_now(store, &author).await;
        assert_eq!(
            now.edge.as_ref().map(|e| &e.object),
            Some(&o.curfew),
            "{what} left the edge as it was: {now:?}"
        );
        assert!(!now.fields.contains_key("keeps"), "{what}: {now:?}");
    }

    let edge = store
        .update_fact(&made.address(), to(Some(2), None), &author)
        .await
        .expect("the edit is answered")
        .written()
        .expect("a number a member holds replaces an edge");
    assert_eq!(edge.edge.as_ref().map(|e| &e.object), Some(&o.parking));
    store
        .update_fact(&made.address(), to(None, Some(2)), &author)
        .await
        .expect("the edit is answered")
        .written()
        .expect("a number a member holds replaces a reference field");
    assert_eq!(
        held_field(store, &author, "will be re-pointed", "keeps").await,
        Some(o.parking.to_string()),
        "the field reads back as the member's handle"
    );
}

/// The claim the edit cases work on, as it reads now.
async fn claim_now<M: Memory + ?Sized>(store: &M, author: &EntityId) -> Fact {
    let read = store.recall(author).await.expect("the author reads");
    let claim = read.iter().find(|f| f.content == "will be re-pointed");
    claim.expect("the claim the case wrote").clone()
}

/// **A member's address is a parent**, for a thing created under it and for a
/// thing moved under it, and the member is what is stored. The member cannot be
/// its own parent through its address, and a number nobody holds is blocked.
pub async fn a_numbered_address_is_a_parent_and_the_member_is_what_is_stored<M: Memory>(store: &M) {
    let (o, _) = world(store).await;
    let child = EntityId("thing:contract-ordinance-typed-child".to_string());
    let created = store
        .add_entity(NewEntity {
            parent: Some(numbered(&o.collection, 2)),
            ..NewEntity::new(child.clone(), "Typed child", "contract-fixture")
        })
        .await
        .expect("the creation is answered")
        .written()
        .expect("a number a member holds is a parent");
    assert_eq!(created.parent.as_ref(), Some(&o.parking), "{created:?}");
    let listed = store.list_entities(None).await.expect("the store lists");
    let stored = listed.iter().find(|e| e.id == child).expect("the child");
    assert_eq!(stored.parent.as_ref(), Some(&o.parking), "{stored:?}");

    let blocked = store
        .add_entity(NewEntity {
            parent: Some(numbered(&o.collection, 9)),
            ..NewEntity::new(
                EntityId("thing:contract-sapling".to_string()),
                "Typed sapling",
                "contract-fixture",
            )
        })
        .await
        .expect("the creation is answered");
    assert!(matches!(blocked, Guarded::Blocked { .. }), "{blocked:?}");

    let sapling = EntityId("thing:contract-sapling".to_string());
    store
        .add_entity(NewEntity::new(
            sapling.clone(),
            "Typed sapling",
            "contract-fixture",
        ))
        .await
        .expect("the creation is answered")
        .written()
        .expect("a thing with no parent lands");
    let moved = store
        .rename_entity(
            &sapling,
            &sapling,
            Some(numbered(&o.collection, 4)),
            date(2026, 6, 6),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the move is answered")
        .written()
        .expect("a number a member holds is a parent for a move");
    assert_eq!(moved.parent.as_ref(), Some(&o.fireworks), "{moved:?}");

    let own = store
        .rename_entity(
            &o.parking,
            &o.parking,
            Some(numbered(&o.collection, 2)),
            date(2026, 6, 7),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the move is answered");
    let Guarded::Blocked { candidates, .. } = own else {
        panic!("the member cannot be its own parent: {own:?}");
    };
    assert!(
        candidates
            .iter()
            .any(|c| c.reason == crate::memory::guard::MatchReason::SelfParent),
        "refused as its own parent, not as a name: {candidates:?}"
    );
}

/// **A parent named by a member's address is the same parent as its handle, for
/// the screen that lets siblings share a numbered series.** Two things in one
/// series under one parent are siblings whichever way the parent is spelled; a
/// third under another parent is not, and is refused as resembling them.
pub async fn a_numbered_spelling_of_the_parent_keeps_siblings_siblings<M: Memory>(store: &M) {
    let (o, _) = world(store).await;
    let series = |n: u32| EntityId(format!("thing:contract-series-{n}"));
    let under = |n: u32, parent: EntityId| NewEntity {
        parent: Some(parent),
        ..NewEntity::new(series(n), format!("Series {n}"), "contract-fixture")
    };
    store
        .add_entity(under(1, o.parking.clone()))
        .await
        .expect("the creation is answered")
        .written()
        .expect("the first of the series lands");
    let second = store
        .add_entity(under(2, numbered(&o.collection, 2)))
        .await
        .expect("the creation is answered")
        .written()
        .expect("the same parent by its number is a sibling");
    assert_eq!(second.parent.as_ref(), Some(&o.parking), "{second:?}");

    let stranger = store
        .add_entity(under(3, o.curfew.clone()))
        .await
        .expect("the creation is answered");
    assert!(
        matches!(stranger, Guarded::Blocked { .. }),
        "a series under another parent resembles the first: {stranger:?}"
    );
}

/// **What a member's address stored follows the member's rename**: the edge, the
/// ref and the field written by the cases above name the member by its permanent
/// id, so they read as the new handle. Run after them.
pub async fn what_a_numbered_address_stored_follows_the_members_rename<M: Memory>(store: &M) {
    let (o, author) = world(store).await;
    let renamed = EntityId("decision:contract-ordinance-typed-moved".to_string());
    store
        .rename_entity(
            &o.parking,
            &renamed,
            None,
            date(2026, 6, 5),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the rename is answered")
        .written()
        .expect("the rename lands");
    let read = store.recall(&author).await.expect("the author reads");
    let edge = read
        .iter()
        .find(|f| f.content == "sees the parking rule")
        .expect("the edge case wrote this claim");
    assert_eq!(edge.edge.as_ref().map(|e| &e.object), Some(&renamed));
    let cited = read
        .iter()
        .find(|f| f.content == "cites the parking rule")
        .expect("the ref case wrote this claim");
    assert_eq!(cited.refs, vec![renamed.clone()]);
    assert_eq!(
        held_field(
            store,
            &author,
            "is governed by the parking rule",
            "governed_by"
        )
        .await,
        Some(renamed.to_string())
    );
}

/// Numbered references pass the same gate that refuses a lone unknown name
/// and a former handle used as a whole target, on capture and edit alike.
pub async fn a_numbered_reference_keeps_both_existence_gates<M: Memory>(store: &M) {
    let (o, author) = world(store).await;
    store
        .declare_type(
            DeclaredType::new(
                "typed-rule-keeper",
                vec![Field::new("keeps", ValueType::Reference)],
            ),
            None,
        )
        .await
        .unwrap();
    let mut addresses = Vec::new();
    for sets in [false, true] {
        let make = |value: String| {
            let fact = NewFact {
                fields: BTreeMap::from([("keeps".into(), value)]),
                ..NewFact::about(
                    author.clone(),
                    "keeps a numbered reference",
                    date(2026, 6, 8),
                )
            };
            if sets { state(fact) } else { fact }
        };
        let patch = |value: String| {
            let patch = FactPatch {
                fields: BTreeMap::from([("keeps".into(), value)]),
                ..Default::default()
            };
            if sets {
                super::support::state_patch(patch)
            } else {
                patch
            }
        };
        let written = store
            .capture(make(numbered(&o.collection, 2).to_string()))
            .await
            .unwrap()
            .written()
            .expect("a numbered reference passes the existence gate");
        let address = written.address();
        let read = store.recall(&author).await.unwrap();
        let captured = read.iter().find(|fact| fact.address() == address).unwrap();
        assert_eq!(captured.fields["keeps"], o.parking.to_string());
        assert_eq!(captured.field_handles(), vec![o.parking.clone()]);
        assert_eq!(captured.sets.contains("keeps"), sets);
        store
            .update_fact(
                &address,
                patch(numbered(&o.collection, 4).to_string()),
                &author,
            )
            .await
            .unwrap()
            .written()
            .expect("an edit accepts a numbered reference too");
        let before = store.recall(&author).await.unwrap();
        let fields = store.fields(&author).await.unwrap();
        let edited = before
            .iter()
            .find(|fact| fact.address() == address)
            .unwrap();
        assert_eq!(edited.fields["keeps"], o.fireworks.to_string());
        assert_eq!(edited.field_handles(), vec![o.fireworks.clone()]);
        assert_eq!(edited.sets.contains("keeps"), sets);
        let captured = store.capture(make("dev7".into())).await;
        assert!(
            matches!(&captured, Err(MemoryError::InvalidFact(_))),
            "the numbered gate must not admit a lone unknown name: {captured:?}"
        );
        let edited = store
            .update_fact(&address, patch("dev7".into()), &author)
            .await;
        assert!(
            matches!(&edited, Err(MemoryError::InvalidFact(_))),
            "an edit refuses the same lone unknown name: {edited:?}"
        );
        assert_eq!(store.recall(&author).await.unwrap(), before);
        assert_eq!(store.fields(&author).await.unwrap(), fields);
        addresses.push((sets, address));
    }
    let renamed = EntityId("decision:contract-ordinance-typed-moved".into());
    store
        .rename_entity(
            &o.parking,
            &renamed,
            None,
            date(2026, 6, 9),
            crate::memory::Override::none(),
            None,
        )
        .await
        .unwrap()
        .written()
        .unwrap();
    let before = store.recall(&author).await.unwrap();
    let fields = store.fields(&author).await.unwrap();
    for (sets, address) in addresses {
        let fact = NewFact {
            fields: BTreeMap::from([("keeps".into(), o.parking.to_string())]),
            ..NewFact::about(
                author.clone(),
                "names a former handle whole",
                date(2026, 6, 10),
            )
        };
        let captured = store
            .capture(if sets { state(fact) } else { fact })
            .await
            .unwrap();
        assert!(
            matches!(&captured, Guarded::Blocked { attempted, .. } if attempted == &o.parking),
            "a former handle as the whole reference stays refused: {captured:?}"
        );
        let patch = FactPatch {
            fields: BTreeMap::from([("keeps".into(), o.parking.to_string())]),
            ..Default::default()
        };
        let patch = if sets {
            super::support::state_patch(patch)
        } else {
            patch
        };
        let edited = store.update_fact(&address, patch, &author).await.unwrap();
        assert!(
            matches!(&edited, Guarded::Blocked { attempted, .. } if attempted == &o.parking),
            "an edit refuses the former handle whole too: {edited:?}"
        );
        assert_eq!(store.recall(&author).await.unwrap(), before);
        assert_eq!(store.fields(&author).await.unwrap(), fields);
    }
}

/// Every spec above, in one call.
pub async fn run_all_typed_addresses<M: Memory>(store: &M) {
    super::typed_kinds::run_all_typed_kinds(store).await;
    a_numbered_address_is_an_edge_object_and_the_member_is_what_is_stored(store).await;
    a_numbered_address_is_a_ref_and_the_member_is_what_is_stored(store).await;
    a_numbered_address_in_a_field_links_the_member(store).await;
    a_numbered_address_replaces_an_edge_and_a_field_by_an_edit(store).await;
    a_numbered_address_is_a_parent_and_the_member_is_what_is_stored(store).await;
    a_numbered_spelling_of_the_parent_keeps_siblings_siblings(store).await;
    a_numbered_address_edits_and_archives_the_member(store).await;
    what_a_numbered_address_stored_follows_the_members_rename(store).await;
    super::nested_addresses::a_nested_numbered_address_is_the_final_member_everywhere(store).await;
    super::nested_addresses::a_renamed_collection_answers_for_its_numbers_by_its_old_handle(store)
        .await;
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
    async fn fake_replaces_an_edge_and_a_field_by_an_edit_with_a_numbered_address() {
        a_numbered_address_replaces_an_edge_and_a_field_by_an_edit(&InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn fake_keeps_siblings_siblings_under_a_numbered_spelling_of_the_parent() {
        a_numbered_spelling_of_the_parent_keeps_siblings_siblings(&InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn fake_takes_a_numbered_address_as_a_parent() {
        a_numbered_address_is_a_parent_and_the_member_is_what_is_stored(&InMemoryMemory::booted())
            .await;
    }

    #[tokio::test]
    async fn fake_edits_and_archives_the_member_a_numbered_address_names() {
        a_numbered_address_edits_and_archives_the_member(&InMemoryMemory::booted()).await;
    }

    #[tokio::test]
    async fn fake_follows_the_members_rename_in_what_a_numbered_address_stored() {
        let store = InMemoryMemory::booted();
        a_numbered_address_is_an_edge_object_and_the_member_is_what_is_stored(&store).await;
        a_numbered_address_is_a_ref_and_the_member_is_what_is_stored(&store).await;
        a_numbered_address_in_a_field_links_the_member(&store).await;
        what_a_numbered_address_stored_follows_the_members_rename(&store).await;
    }

    #[tokio::test]
    async fn fake_links_the_member_a_numbered_address_in_a_field_names() {
        a_numbered_address_in_a_field_links_the_member(&InMemoryMemory::booted()).await;
    }
}
