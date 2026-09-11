use super::*;

/// Make sure `id` exists, so the write guard's **existence gate** is not
/// what a spec about something else trips over. Idempotent: the suite runs
/// against a shared, pre-populated collection as much as an empty fake.
///
/// The gate itself has its own specs below; everywhere else, provisioning is
/// setup, not the subject under test.
pub(super) async fn ensure<M: Memory + ?Sized>(store: &M, id: &EntityId) {
    let known = store
        .list_entities(None)
        .await
        .expect("list_entities should succeed");
    if known.iter().any(|e| &e.id == id) {
        return;
    }
    // A rhythm is refused without a parent, so provisioning one provisions
    // the thing it is a loop on. The owner is a plain entity of the
    // fixture's own, which keeps the rule the store enforces out of the way
    // of cases that are about something else.
    let parent = if id.kind() == Some(EntityKind::RHYTHM) {
        let owner = EntityId::new(EntityKind::THING, format!("{}-owner", id.slug()));
        if !known.iter().any(|e| e.id == owner) {
            add(
                store,
                NewEntity::new(owner.clone(), owner.slug(), "contract-fixture"),
            )
            .await;
        }
        Some(owner)
    } else {
        None
    };
    add(
        store,
        NewEntity {
            parent,
            ..NewEntity::new(id.clone(), id.slug(), "contract-fixture")
        },
    )
    .await;
}

/// Capture a fact the guard is expected to wave through — provisioning its
/// subject and any edge object first, because every write that names an
/// entity now requires one that exists.
pub(super) async fn capture<M: Memory + ?Sized>(store: &M, fact: NewFact) -> Fact {
    ensure(store, &fact.subject).await;
    if let Some(edge) = &fact.edge {
        ensure(store, &edge.object).await;
    }
    let subject = fact.subject.clone();
    store
        .capture(fact)
        .await
        .expect("capture should succeed")
        .written()
        .unwrap_or_else(|| panic!("the guard must not block {subject}"))
}

/// Add an entity the guard is expected to wave through.
pub(super) async fn add<M: Memory + ?Sized>(store: &M, new: NewEntity) -> Entity {
    let id = new.id.clone();
    store
        .add_entity(new)
        .await
        .expect("add_entity should succeed")
        .written()
        .unwrap_or_else(|| panic!("the guard must not block {id}"))
}

/// Edit a fact the guard is expected to wave through — provisioning any edge
/// object the patch attaches, for the same reason [`capture`] does.
pub(super) async fn edit<M: Memory>(store: &M, address: &FactAddress, patch: FactPatch) -> Fact {
    if let Some(edge) = &patch.edge {
        ensure(store, &edge.object).await;
    }
    store
        .update_fact(address, patch)
        .await
        .expect("update_fact should succeed")
        .written()
        .unwrap_or_else(|| panic!("the guard must not block the edit at {address}"))
}
