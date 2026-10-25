use super::*;

impl DoltMemory {
    pub(super) async fn supplied_only_link_counts(
        &self,
        tx: &mut Transaction<'_, MySql>,
        directory: &Directory,
        eligible_history: &BTreeMap<String, bool>,
    ) -> Result<(usize, usize), MemoryError> {
        let by_key = current::writes_in(tx, &self.supplied, &directory.row_handles).await?;
        let mut current = BTreeSet::new();
        let mut historical = BTreeSet::new();
        for (entity, defaults) in self.supplied.records() {
            if directory.row_handles.contains(entity.id.as_str()) {
                continue;
            }
            let storage_key = entity
                .badge
                .as_deref()
                .filter(|badge| !badge.is_empty())
                .unwrap_or(entity.id.as_str());
            // The provisioned read lays stored folded fields over these defaults.
            // Use its store fold, including status and merges, before counting.
            let mut fields = defaults.clone();
            fields.extend(self.supplied.fields_for(&entity.id));
            fields.extend(folded_fields(
                by_key
                    .get(storage_key)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &directory.declared,
            ));
            if fields
                .iter()
                .any(|(key, value)| directory.mark(key, value).marked_or_mixed_links())
            {
                current.insert(storage_key.to_string());
            }
            if eligible_history.get(storage_key).copied().unwrap_or(false) {
                historical.insert(storage_key.to_string());
            }
        }
        Ok((current.len(), historical.len()))
    }
}
