//! The link decision of rule 646 in the in-memory store, run when the process
//! switch is on.

use super::*;

impl InMemoryMemory {
    /// **Every handle a field value names, lowered from whatever the caller
    /// wrote to the permanent id the store keeps** (rule 268, reaching a field
    /// the way it already reaches an edge's object and a ref), behind the mark
    /// that tells a stored id from a word. The mark is the decision that the
    /// item is a link, taken here once and whatever the key (rule 646). Each item
    /// was already checked to exist, so `storage_key` cannot miss on one; loose
    /// items beside them are kept as written.
    pub(super) fn lower_marked_fields(
        &self,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) {
        for (key, value) in fields.clone() {
            if crate::memory::at_write::handles_in_value(&value).is_empty() {
                continue;
            }
            let lowered: Vec<String> = value
                .split(',')
                .map(|item| {
                    let item = item.trim();
                    let id = EntityId(item.to_string());
                    if crate::memory::validate_subject(&id).is_err() {
                        return item.to_string();
                    }
                    let stored = self
                        .storage_key(&id)
                        .map_or_else(|| item.to_string(), |id| id.to_string());
                    crate::memory::marked_stored_handle(&stored)
                })
                .collect();
            fields.insert(key, lowered.join(", "));
        }
    }

    /// [`Self::compose_reference_fields`] over a resolution the caller shares
    /// with every other claim it serves.
    ///
    /// **Each item is served by what it wears.** One behind the mark is the
    /// thing's id, served as the handle it answers to now. One stored as plain
    /// handle text, from before ids were kept, is resolved through the thing's
    /// rename history, so it keeps pointing at the thing it named. Loose text is
    /// served as written.
    pub(super) fn compose_marked_in(
        &self,
        resolution: &Resolution<'_>,
        fields: &mut std::collections::BTreeMap<String, String>,
    ) {
        for (key, value) in fields.clone() {
            let items: Vec<&str> = value.split(',').map(str::trim).collect();
            let wearing = |item: &&str| {
                crate::memory::stored_handle_id(item).is_some()
                    || crate::memory::validate_subject(&EntityId((*item).to_string())).is_ok()
            };
            if !items.iter().any(wearing) {
                continue;
            }
            let composed: Vec<(Option<EntityId>, String)> = items
                .iter()
                .map(|item| {
                    let id = match crate::memory::stored_handle_id(item) {
                        Some(stored) => {
                            Some(resolution.current_handle(&EntityId(stored.to_string())))
                        }
                        None => {
                            let id = EntityId((*item).to_string());
                            crate::memory::validate_subject(&id).ok().map(|()| {
                                crate::memory::resolve_handle(
                                    &id,
                                    resolution.known(),
                                    resolution.former(),
                                )
                                .map_or(id, |entity| entity.id.clone())
                            })
                        }
                    };
                    let text = id
                        .as_ref()
                        .map_or_else(|| (*item).to_string(), |id| id.to_string());
                    (id, text)
                })
                .collect();
            fields.insert(key, crate::memory::list::serve_values(composed));
        }
    }
}
