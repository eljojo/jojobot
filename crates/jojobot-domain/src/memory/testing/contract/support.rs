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

/// **How an existing thing for a guard case comes to exist** — written
/// fresh for the ordinary run, or already sitting in the store as a
/// build-supplied record for the doubled one (rule 234). One shape, so a
/// guard case that asks "is this handle already taken" cannot tell which
/// kind of existing answered it, and the same assertions hold for both.
#[async_trait::async_trait]
pub(super) trait Backing<M: Memory + ?Sized>: Send + Sync {
    /// A handle that already exists by the time this returns, and the
    /// source label its own near-miss candidate is expected to report.
    async fn existing(&self, store: &M) -> (EntityId, String);
}

/// **A freshly stored row**, written through [`add`] the way every
/// existence-guard case did before this — the ordinary run.
pub(super) struct Stored {
    pub handle: EntityId,
    pub name: &'static str,
    pub source: &'static str,
}

#[async_trait::async_trait]
impl<M: Memory + ?Sized> Backing<M> for Stored {
    async fn existing(&self, store: &M) -> (EntityId, String) {
        add(
            store,
            NewEntity::new(self.handle.clone(), self.name, self.source),
        )
        .await;
        (self.handle.clone(), self.source.to_string())
    }
}

/// **A record the build supplies**, already there before this ever runs —
/// the doubled run. The fixed handle every existence-guard spec is written
/// against (rule 234's own worked example): a fixture that wired no
/// supplied record here would pass on an index nothing could ever miss.
pub(super) struct Supplied;

#[async_trait::async_trait]
impl<M: Memory + ?Sized> Backing<M> for Supplied {
    async fn existing(&self, _store: &M) -> (EntityId, String) {
        (
            EntityId(SUPPLIED_VIEW_FOR_THE_GUARD_SPECS.into()),
            "jojobot".to_string(),
        )
    }
}

/// Edit a fact the guard is expected to wave through — provisioning any edge
/// object the patch attaches, for the same reason [`capture`] does.
pub(super) async fn edit<M: Memory + ?Sized>(
    store: &M,
    address: &FactAddress,
    patch: FactPatch,
) -> Fact {
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
