//! **A capture is held to its subject's kind.** The write may not drop a thing
//! below the keys its kind requires, nor put a key a value the kind says the
//! key does not hold. Judged on the result: the thing's fields as they will
//! stand once the record lands, against the fields it holds now.

use std::collections::BTreeMap;

use super::{CaptureRequest, ObjectId, State};
use crate::memory::{
    self, EntityId, FactId, MemoryError,
    types::{DeclaredType, Origin},
};

/// The refusal a capture meets from its subject's kind, if it meets one.
///
/// The open entries a list hung on the thing holds are not read: a state of
/// claims does not yet answer which entries of a list stand open, so a write
/// that finishes a thing is judged on its other rules alone.
pub(super) fn refuses_a_kind_misfit(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
) -> Option<MemoryError> {
    let kind = state.object(home)?.kind.0.clone();
    let keys = memory::kinds::keys_of(&kind);
    let governs: Vec<DeclaredType> = if keys.is_empty() {
        Vec::new()
    } else {
        vec![DeclaredType {
            name: kind.clone(),
            fields: keys,
            origin: Origin::Shipped,
            must: None,
        }]
    };
    let writes = state.key_writes(home);
    let before = memory::folded_fields(&writes, declared);
    let id = FactId(request.record.0.clone());
    let after = memory::stood_after_record(
        &writes,
        &memory::NewRecord {
            id: &id,
            fields: &request.fields,
            status: request.draft.status,
            provenance: request.draft.provenance,
            standing: request.draft.standing,
            note: request.draft.details.as_ref(),
            recorded_by: request.draft.app.as_ref(),
        },
        declared,
    );
    // A reference to a numbered handle is judged by the kind of the thing it
    // names, so the handle it resolves to is handed to the guard.
    let mut resolved: BTreeMap<EntityId, EntityId> = BTreeMap::new();
    for (_, items) in memory::reference_field_values(&after, declared) {
        for item in items {
            let id = EntityId(item);
            if memory::numbered::numbered_handle(&id).is_some()
                && let Some(handle) = state
                    .resolve(id.as_str())
                    .and_then(|object| state.handle_of(object))
            {
                resolved.insert(id, handle);
            }
        }
    }
    memory::guard_fit_resolved(&kind, &before, &after, &governs, &resolved, &[]).err()
}
