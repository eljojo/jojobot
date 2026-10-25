use super::*;
use std::collections::BTreeMap;

pub(super) async fn handles_in(
    tx: &mut Transaction<'_, MySql>,
) -> Result<BTreeMap<String, EntityId>, MemoryError> {
    let rows: Vec<(String, Option<String>)> = sqlx::query_as("SELECT id, badge FROM entity")
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
    let mut handles = BTreeMap::new();
    for (handle, badge) in rows {
        let id = EntityId(handle.clone());
        handles.insert(handle, id.clone());
        if let Some(badge) = badge.filter(|badge| !badge.is_empty()) {
            handles.insert(badge, id);
        }
    }
    Ok(handles)
}
