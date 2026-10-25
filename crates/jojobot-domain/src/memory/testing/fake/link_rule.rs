//! The link decision of rule 646 in the in-memory store, run when the process
//! switch is on.

use super::*;

impl InMemoryMemory {
    pub(super) fn references_checked(
        &self,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        let declared = self.types.lock().expect("fake mutex poisoned").clone();
        let lone = crate::memory::lone_reference_names(fields, &declared);
        if !lone.is_empty() {
            let known = self.known();
            for (name, refusal) in lone {
                if !known
                    .iter()
                    .any(|entity| entity.badge.as_deref() == Some(name))
                {
                    return Err(refusal);
                }
            }
        }
        crate::memory::referenced_by(fields, &declared)
    }

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
        let declared = self.declarations();
        let known = self.known();
        for (key, value) in fields.clone() {
            // A key a type declares a reference also takes the bare id of an existing
            // thing; that is read here, once, and the mark it is given is what stays.
            let reference = declared
                .iter()
                .filter_map(|d| d.field(&key))
                .any(|f| f.holds == crate::memory::types::ValueType::Reference);
            if !reference && crate::memory::at_write::handles_in_value(&value).is_empty() {
                continue;
            }
            let lowered: Vec<String> = value
                .split(',')
                .map(|item| {
                    let item = item.trim();
                    let id = EntityId(item.to_string());
                    if crate::memory::validate_subject(&id).is_err() {
                        let worn = reference
                            && known
                                .iter()
                                .any(|entity| entity.badge.as_deref() == Some(item));
                        return match worn {
                            true => crate::memory::marked_stored_handle(item),
                            false => item.to_string(),
                        };
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

    /// **What each key's stored value links to, by its marks**, as the handles the
    /// marked things answer to now; empty unless the link rule is on.
    pub(super) fn marked_links_in(
        &self,
        resolution: &Resolution<'_>,
        stored: &std::collections::BTreeMap<String, String>,
    ) -> std::collections::BTreeMap<String, Vec<EntityId>> {
        let mut links = std::collections::BTreeMap::new();
        if crate::memory::link_rule() != crate::memory::LinkRule::On {
            return links;
        }
        for (key, value) in stored {
            let mut named: Vec<EntityId> = Vec::new();
            for item in value.split(',') {
                if let Some(id) = crate::memory::stored_handle_id(item) {
                    let handle = resolution.current_handle(&EntityId(id.to_string()));
                    if !named.contains(&handle) {
                        named.push(handle);
                    }
                }
            }
            if !named.is_empty() {
                links.insert(key.clone(), named);
            }
        }
        links
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
            if !items
                .iter()
                .any(|item| crate::memory::stored_handle_id(item).is_some())
            {
                continue;
            }
            // Only a mark names a thing: text that wears none is served as written.
            let composed: Vec<(Option<EntityId>, String)> = items
                .iter()
                .map(|item| {
                    let id = crate::memory::stored_handle_id(item)
                        .map(|stored| resolution.current_handle(&EntityId(stored.to_string())));
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
