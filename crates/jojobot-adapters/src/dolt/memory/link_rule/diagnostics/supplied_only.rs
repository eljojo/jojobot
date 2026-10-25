use super::*;

impl DoltMemory {
    pub(super) async fn supplied_only_link_counts(
        &self,
        tx: &mut Transaction<'_, MySql>,
        directory: &Directory,
        writes: &[(String, String, i64, String)],
    ) -> Result<(usize, usize), MemoryError> {
        let row_handles: BTreeSet<String> = sqlx::query_scalar("SELECT id FROM entity")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        let mut current = BTreeSet::new();
        let mut historical = BTreeSet::new();
        for (entity, defaults) in self.supplied.records() {
            if row_handles.contains(entity.id.as_str()) {
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
            fields.extend(self.held_by(tx, &EntityId(storage_key.to_string())).await?);
            if fields
                .iter()
                .any(|(key, value)| eligible(directory, key, value))
            {
                current.insert(storage_key.to_string());
            }
            if writes.iter().any(|(holder, key, _, value)| {
                holder == storage_key && eligible(directory, key, value)
            }) {
                historical.insert(storage_key.to_string());
            }
        }
        Ok((current.len(), historical.len()))
    }
}

fn eligible(directory: &Directory, key: &str, value: &str) -> bool {
    if !jojobot_domain::memory::at_write::field_link_targets(value).is_empty() {
        return true;
    }
    let marking = directory.mark(key, value);
    marking.mixed
        && !jojobot_domain::memory::at_write::field_link_targets(&marking.value).is_empty()
}
