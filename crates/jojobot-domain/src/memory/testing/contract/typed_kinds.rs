//! A numbered value has its member's kind, including under a narrowed reference.

use super::numbered::numbered;
use super::support::{add, capture, state, state_patch};
use super::*;
use std::collections::BTreeMap;

struct World {
    holder: EntityId,
    decision: EntityId,
    decision_number: EntityId,
    venue: EntityId,
    venue_number: EntityId,
    guest: EntityId,
    guest_number: EntityId,
}

async fn world<M: Memory>(store: &M) -> World {
    let (o, _) = super::typed_addresses::world(store).await;
    let id = |s: &str| EntityId(s.to_string());
    let w = World {
        holder: id("thing:contract-typed-holder"),
        decision: o.parking.clone(),
        decision_number: numbered(&o.collection, 2),
        venue: id("place:contract-typed-venue"),
        venue_number: id("topic:contract-typed-venues/2"),
        guest: id("person:contract-typed-guest"),
        guest_number: id("place:contract-typed-guests/2"),
    };
    if store
        .list_entities(None)
        .await
        .unwrap()
        .iter()
        .any(|e| e.id == w.holder)
    {
        return w;
    }
    store
        .declare_kind(
            "thing",
            Origin::Shipped,
            vec![Field::pointing_at("keeps", EntityKind::DECISION)],
        )
        .await
        .expect("a kind-narrowed reference declaration");
    for (collection, member, title) in [
        (id("topic:contract-typed-venues"), &w.venue, "Venue"),
        (id("place:contract-typed-guests"), &w.guest, "Guest"),
    ] {
        add(
            store,
            NewEntity::new(
                collection.clone(),
                format!("{title} collection"),
                "contract-fixture",
            ),
        )
        .await;
        capture(
            store,
            state(NewFact {
                fields: BTreeMap::from([("numbers_by".into(), "number".into())]),
                ..NewFact::about(collection.clone(), "numbers its members", date(2026, 5, 1))
            }),
        )
        .await;
        add(
            store,
            NewEntity {
                parent: Some(collection),
                ..NewEntity::new(member.clone(), title, "contract-fixture")
            },
        )
        .await;
        capture(
            store,
            state(NewFact {
                fields: BTreeMap::from([("number".into(), "2".into())]),
                ..NewFact::about(member.clone(), "holds number two", date(2026, 5, 2))
            }),
        )
        .await;
    }
    add(
        store,
        NewEntity::new(w.holder.clone(), "Typed holder", "contract-fixture"),
    )
    .await;
    w
}

fn visit(w: &World, object: &EntityId) -> NewFact {
    state(NewFact {
        edge: Some(Edge::new(EdgeShape::Location, object.clone())),
        fields: BTreeMap::from([("keeps".into(), w.decision.to_string())]),
        ..NewFact::about(w.holder.clone(), "visits the venue", date(2026, 6, 1))
    })
}

async fn location<M: Memory>(store: &M, edit: bool, inverse: bool) {
    let w = world(store).await;
    let control = capture(store, visit(&w, &w.venue)).await;
    assert_eq!(control.edge.as_ref().unwrap().object, w.venue);
    let before = store.recall(&w.holder).await.unwrap();
    let object = if inverse {
        &w.guest_number
    } else {
        &w.venue_number
    };
    let result = if edit {
        store
            .update_fact(
                &control.address(),
                FactPatch {
                    edge: Some(Edge::new(EdgeShape::Location, object.clone())),
                    ..Default::default()
                },
                &w.holder,
            )
            .await
    } else {
        store.capture(visit(&w, object)).await
    };
    if inverse {
        assert!(
            matches!(result, Err(MemoryError::InvalidEdge(_))),
            "a place-prefixed collection cannot make its person member a place: {result:?}"
        );
        assert_eq!(
            store.recall(&w.holder).await.unwrap(),
            before,
            "refusal changes no claim"
        );
    } else {
        let written = result
            .expect("a numbered location is answered")
            .written()
            .expect("the member is a place");
        assert_eq!(written.edge.as_ref().unwrap().object, w.venue);
        let held = store.recall(&w.holder).await.unwrap();
        assert_eq!(
            held.iter()
                .find(|f| f.id == written.id)
                .unwrap()
                .edge
                .as_ref()
                .unwrap()
                .object,
            w.venue
        );
    }
}

pub async fn b5_numbered_location_capture<M: Memory>(store: &M) {
    location(store, false, false).await;
}

pub async fn b5_numbered_location_capture_refuses_a_person<M: Memory>(store: &M) {
    location(store, false, true).await;
}

pub async fn b5_numbered_location_edit<M: Memory>(store: &M) {
    location(store, true, false).await;
}

pub async fn b5_numbered_location_edit_refuses_a_person<M: Memory>(store: &M) {
    location(store, true, true).await;
}

async fn reference<M: Memory>(store: &M, edit: bool) {
    let w = world(store).await;
    let fields = |id: &EntityId| BTreeMap::from([("keeps".into(), id.to_string())]);
    let control = capture(
        store,
        state(NewFact {
            fields: fields(&w.decision),
            ..NewFact::about(w.holder.clone(), "keeps the parking rule", date(2026, 6, 2))
        }),
    )
    .await;
    let write = |value: &EntityId| {
        state(NewFact {
            fields: fields(value),
            ..NewFact::about(w.holder.clone(), "keeps a numbered rule", date(2026, 6, 3))
        })
    };
    let result = if edit {
        store
            .update_fact(
                &control.address(),
                state_patch(FactPatch {
                    fields: fields(&w.decision_number),
                    ..Default::default()
                }),
                &w.holder,
            )
            .await
    } else {
        store.capture(write(&w.decision_number)).await
    };
    let written = result
        .expect("the narrowed reference is answered")
        .written()
        .expect("the member is a decision");
    let before = store.recall(&w.holder).await.unwrap();
    assert_eq!(
        before.iter().find(|f| f.id == written.id).unwrap().fields["keeps"],
        w.decision.to_string()
    );
    let refused = if edit {
        store
            .update_fact(
                &control.address(),
                state_patch(FactPatch {
                    fields: fields(&w.guest_number),
                    ..Default::default()
                }),
                &w.holder,
            )
            .await
    } else {
        store.capture(write(&w.guest_number)).await
    };
    assert!(
        matches!(refused, Err(MemoryError::BreaksType { .. })),
        "a person member cannot satisfy reference:decision: {refused:?}"
    );
    assert_eq!(
        store.recall(&w.holder).await.unwrap(),
        before,
        "a wrong kind changes no claim"
    );
    assert_ne!(w.guest.kind(), w.decision.kind());
}

pub async fn b5_numbered_reference_capture<M: Memory>(store: &M) {
    reference(store, false).await;
}

pub async fn b5_numbered_reference_edit<M: Memory>(store: &M) {
    reference(store, true).await;
}

pub async fn run_all_typed_kinds<M: Memory>(store: &M) {
    b5_numbered_location_capture(store).await;
    b5_numbered_location_capture_refuses_a_person(store).await;
    b5_numbered_location_edit(store).await;
    b5_numbered_location_edit_refuses_a_person(store).await;
    b5_numbered_reference_capture(store).await;
    b5_numbered_reference_edit(store).await;
}

#[cfg(test)]
mod tests {
    macro_rules! cases {
        ($($case:ident),+ $(,)?) => {$(
            #[tokio::test]
            async fn $case() {
                super::$case(&crate::memory::testing::InMemoryMemory::booted()).await;
            }
        )+};
    }

    cases!(
        b5_numbered_location_capture,
        b5_numbered_location_capture_refuses_a_person,
        b5_numbered_location_edit,
        b5_numbered_location_edit_refuses_a_person,
        b5_numbered_reference_capture,
        b5_numbered_reference_edit,
    );
}
