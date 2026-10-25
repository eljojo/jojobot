//! What serving claims resolves handles through, read once for a call that
//! serves many.

use super::*;

/// **What serving claims resolves handles through**, built once by a call that
/// serves many. The declared types are cloned when it is built; the rows and
/// the rename history are read the first time a value holds a handle, and then
/// kept for the rest of the call.
pub(super) struct Resolution<'a> {
    pub(super) store: &'a InMemoryMemory,
    pub(super) declared: Vec<crate::memory::types::DeclaredType>,
    pub(super) known: std::sync::OnceLock<Vec<Entity>>,
    pub(super) former: std::sync::OnceLock<Vec<FormerHandle>>,
}

impl Resolution<'_> {
    pub(super) fn known(&self) -> &[Entity] {
        self.known.get_or_init(|| self.store.known())
    }

    pub(super) fn former(&self) -> &[FormerHandle] {
        self.former.get_or_init(|| self.store.former())
    }

    /// The handle a stored value answers to today, as
    /// [`InMemoryMemory::current_handle`] answers it, over the rows read once.
    pub(super) fn current_handle(&self, stored: &EntityId) -> EntityId {
        match crate::memory::entity_wearing(stored.as_str(), self.known()) {
            Some(entity) => entity.id.clone(),
            None => stored.clone(),
        }
    }
}

impl InMemoryMemory {
    /// **What a handle is stored as, and what it is called today, together.**
    /// A member's address that goes through a member that numbers its own
    /// members (`outer/2/3`) is the member `outer/2` is, numbered `3`, so the
    /// prefix is resolved first and the number is read in its scope.
    pub(super) fn resolve(&self, id: &EntityId) -> Option<(EntityId, EntityId)> {
        let known = self.known();
        let former = self.former();
        let entity = match crate::memory::resolve_handle(id, &known, &former) {
            Some(found) => found,
            None => {
                let (outer, number) = crate::memory::numbered::numbered_handle(id)?;
                let (_, inner) = self.resolve(&outer)?;
                let nested = EntityId(format!("{inner}/{number}"));
                crate::memory::resolve_handle(&nested, &known, &former)?
            }
        };
        let key = match &entity.badge {
            Some(badge) => EntityId(badge.clone()),
            None => entity.id.clone(),
        };
        Some((key, entity.id.clone()))
    }

    /// [`guard::decide_named`] over this store's own resolution: a name it finds,
    /// a member's number included, exists.
    pub(super) fn decide_named(&self, id: &EntityId, index: &[Entity]) -> Decision {
        guard::decide_named(
            id,
            self.resolve(id).as_ref().map(|(_, handle)| handle),
            index,
        )
    }

    /// The handle of the row an edit, an archive, a restore or a parent names:
    /// a member's number is the member that holds it, and every other name is
    /// kept as sent, a former handle included, which these writes refuse.
    pub(super) fn row_handle(&self, id: &EntityId) -> EntityId {
        match crate::memory::numbered::numbered_handle(id) {
            Some(_) => self.resolve(id).map_or_else(|| id.clone(), |(_, h)| h),
            None => id.clone(),
        }
    }

    /// The storage key alone, for a caller that has no use for the handle.
    pub(super) fn storage_key(&self, id: &EntityId) -> Option<EntityId> {
        self.resolve(id).map(|(key, _)| key)
    }

    /// Whether the build supplies a record under this handle.
    pub(super) fn is_supplied(&self, id: &EntityId) -> bool {
        let supplied = self.supplied.lock().expect("fake mutex poisoned");
        supplied.record_for(id).is_some()
    }

    /// The things a write's reference keys name. Listed first and with the types
    /// lock released, because resolving each of them reads the types too.
    pub(super) fn named_by(
        &self,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        let types = self.types.lock().expect("fake mutex poisoned").clone();
        crate::memory::referenced_by(fields, &types)
    }
}
