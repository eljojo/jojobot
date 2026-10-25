//! **A nested numbered address resolves to its member wherever a write names a
//! thing**, proven against any [`Memory`] adapter. `outer/2/5` is the member
//! numbered 5 in the collection that member 2 of `outer` is: the prefix is
//! resolved first and the number is read in its scope. Each case sends the
//! address through one place and holds that the final member is what is stored,
//! with a missing nested member refused beside it.

use super::numbered::numbered;
use super::support::{add, capture, state};
use super::*;
use std::collections::BTreeMap;

/// An outer collection that numbers its members; member 2 is itself a
/// collection that numbers; its member 5 is the final member. A thing outside it
/// to write on.
pub(super) struct Nested {
    pub(super) outer: EntityId,
    pub(super) inner: EntityId,
    pub(super) leaf: EntityId,
    pub(super) writer: EntityId,
}

async fn world<M: Memory + ?Sized>(store: &M) -> Nested {
    let id = |handle: &str| EntityId(handle.to_string());
    let world = Nested {
        outer: id("topic:contract-ordinances-nested"),
        inner: id("topic:contract-wards"),
        leaf: id("decision:contract-ward-eight"),
        writer: id("person:contract-nested-writer"),
    };
    let known = store.list_entities(None).await.expect("the store lists");
    if known.iter().any(|e| e.id == world.outer) {
        return world;
    }
    add(
        store,
        NewEntity::new(world.outer.clone(), "Nested ordinances", "contract-fixture"),
    )
    .await;
    capture(
        store,
        state(NewFact {
            fields: BTreeMap::from([("numbers_by".to_string(), "number".to_string())]),
            ..NewFact::about(world.outer.clone(), "numbers its members", date(2026, 5, 1))
        }),
    )
    .await;
    for (member, parent, name, number, numbers) in [
        (&world.inner, &world.outer, "Wards", "2", true),
        (&world.leaf, &world.inner, "Ward eight", "5", false),
    ] {
        add(
            store,
            NewEntity {
                parent: Some(parent.clone()),
                ..NewEntity::new(member.clone(), name, "contract-fixture")
            },
        )
        .await;
        let mut fields = BTreeMap::from([("number".to_string(), number.to_string())]);
        if numbers {
            fields.insert("numbers_by".to_string(), "number".to_string());
        }
        capture(
            store,
            state(NewFact {
                fields,
                ..NewFact::about(
                    member.clone(),
                    format!("{name} is {number}"),
                    date(2026, 5, 2),
                )
            }),
        )
        .await;
    }
    add(
        store,
        NewEntity::new(world.writer.clone(), "Nested writer", "contract-fixture"),
    )
    .await;
    world
}

/// **Every place a write names a thing, sent the nested address of the final
/// member**: an edge, a ref, a field, a creation under it, a move under it, an
/// edit, an archive and a restore. The final member is what each stores or
/// changes, and the address of a member nobody holds is refused in the same
/// places.
pub async fn a_nested_numbered_address_is_the_final_member_everywhere<M: Memory>(store: &M) {
    let w = world(store).await;
    let address = numbered(&numbered(&w.outer, 2), 5);
    let missing = numbered(&numbered(&w.outer, 2), 9);

    let landed = store
        .capture(state(NewFact {
            edge: Some(Edge {
                shape: EdgeShape::Connection,
                object: address.clone(),
            }),
            refs: vec![address.clone()],
            fields: BTreeMap::from([("governed_by".to_string(), address.to_string())]),
            ..NewFact::about(w.writer.clone(), "is bound to the ward", date(2026, 6, 1))
        }))
        .await
        .expect("the capture is answered")
        .written()
        .expect("a nested address names the final member");
    assert_eq!(landed.edge.as_ref().map(|e| &e.object), Some(&w.leaf));
    assert_eq!(landed.refs, vec![w.leaf.clone()], "{landed:?}");
    let read = store.recall(&w.writer).await.expect("the writer reads");
    let held = read.iter().find(|f| f.content == landed.content);
    assert_eq!(
        held.and_then(|f| f.fields.get("governed_by")),
        Some(&w.leaf.to_string()),
        "{read:?}"
    );

    let refused = store
        .capture(NewFact {
            edge: Some(Edge {
                shape: EdgeShape::Connection,
                object: missing.clone(),
            }),
            ..NewFact::about(w.writer.clone(), "is bound to no ward", date(2026, 6, 2))
        })
        .await
        .expect("the capture is answered");
    assert!(matches!(refused, Guarded::Blocked { .. }), "{refused:?}");

    let child = EntityId("thing:contract-nested-child".to_string());
    let created = store
        .add_entity(NewEntity {
            parent: Some(address.clone()),
            ..NewEntity::new(child.clone(), "Nested child", "contract-fixture")
        })
        .await
        .expect("the creation is answered")
        .written()
        .expect("a nested address is a parent");
    assert_eq!(created.parent.as_ref(), Some(&w.leaf), "{created:?}");
    let blocked = store
        .add_entity(NewEntity {
            parent: Some(missing.clone()),
            ..NewEntity::new(
                EntityId("thing:contract-nested-orphan".to_string()),
                "Nested orphan",
                "contract-fixture",
            )
        })
        .await
        .expect("the creation is answered");
    assert!(matches!(blocked, Guarded::Blocked { .. }), "{blocked:?}");

    let moved = store
        .rename_entity(
            &w.writer,
            &w.writer,
            Some(address.clone()),
            date(2026, 6, 3),
            crate::memory::Override::none(),
            None,
        )
        .await
        .expect("the move is answered")
        .written()
        .expect("a nested address is a parent for a move");
    assert_eq!(moved.parent.as_ref(), Some(&w.leaf), "{moved:?}");

    let edited = store
        .update_entity(
            &address,
            EntityPatch {
                aliases: Some(vec!["ward eight again".to_string()]),
                ..Default::default()
            },
        )
        .await
        .expect("the edit is answered")
        .written()
        .expect("a nested address selects the final member");
    assert_eq!(edited.id, w.leaf, "{edited:?}");
    let missed = store.update_entity(&missing, EntityPatch::default()).await;
    assert!(
        matches!(missed, Err(MemoryError::UnknownEntity { .. })),
        "{missed:?}"
    );

    let archived = store
        .archive_entity(&address, "closed", None)
        .await
        .expect("a nested address selects the final member");
    assert_eq!(archived.id, w.leaf, "{archived:?}");
    let (restored, _) = store
        .restore_entity(&address, None)
        .await
        .expect("a nested address selects the final member");
    assert_eq!(restored.id, w.leaf, "{restored:?}");
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::testing::InMemoryMemory;

    #[tokio::test]
    async fn fake_resolves_a_nested_numbered_address_everywhere() {
        a_nested_numbered_address_is_the_final_member_everywhere(&InMemoryMemory::booted()).await;
    }
}
