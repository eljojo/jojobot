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
/// A transition to done gathers the live lists hung on the subject and hands
/// their entries to the same gate every status write uses.
pub(super) fn refuses_a_kind_misfit(
    state: &State,
    declared: &[DeclaredType],
    home: &ObjectId,
    request: &CaptureRequest,
    classes: &BTreeMap<String, memory::WriteClass>,
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
    let fields = request
        .fields
        .iter()
        .filter(|(key, _)| {
            classes
                .get(key.trim())
                .is_some_and(|class| memory::reaches_the_thing(*class))
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    let id = FactId(request.record.0.clone());
    let after = memory::stood_after_record(
        &writes,
        &memory::NewRecord {
            id: &id,
            fields: &fields,
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
    let mut hung = Vec::new();
    if memory::done_gate::moves_to_done(&kind, &before, &after) {
        for object in state.standing_objects() {
            let Some(body) = state.object(object) else {
                continue;
            };
            if body.kind.0 != "list"
                || body.archived
                || body
                    .parent
                    .as_ref()
                    .is_none_or(|parent| state.survivor(parent) != home)
            {
                continue;
            }
            let fields = state.fields_served(object, declared);
            let facts: Vec<_> = state
                .records_homed_on(object)
                .into_iter()
                .filter_map(|record| state.fact(record))
                .collect();
            hung.push((fields, facts));
        }
    }
    let lists: Vec<_> = hung
        .iter()
        .map(|(fields, facts)| memory::done_gate::HungList { fields, facts })
        .collect();
    let open = memory::done_gate::open_entries_of(&lists);
    memory::guard_fit_resolved(&kind, &before, &after, &governs, &resolved, &open).err()
}
