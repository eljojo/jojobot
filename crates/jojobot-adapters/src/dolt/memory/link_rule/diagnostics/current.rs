use super::*;
use std::collections::HashMap;

/// The same decoded writes and merge fold as the per-thing read, loaded once.
pub(super) async fn writes_in(
    tx: &mut Transaction<'_, MySql>,
    supplied: &Provisions,
    row_handles: &BTreeSet<String>,
) -> Result<HashMap<String, Vec<KeyWrite>>, MemoryError> {
    let rows = sqlx::query(
        "SELECT merge_no, ordinal, folded, survivor, folded_id, survivor_id
         FROM merge_event ORDER BY merge_no, ordinal",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    let mut steps = BTreeMap::new();
    for row in &rows {
        let number: i64 = row.try_get("merge_no").map_err(store)?;
        if row.try_get::<i64, _>("ordinal").map_err(store)? == 0 {
            steps.insert(
                number,
                MergeStep {
                    merge_no: number,
                    folded: row.try_get("folded").map_err(store)?,
                    survivor: row.try_get("survivor").map_err(store)?,
                    claims: HashMap::new(),
                },
            );
        } else if let Some(step) = steps.get_mut(&number) {
            step.claims.insert(
                row.try_get("folded_id").map_err(store)?,
                row.try_get("survivor_id").map_err(store)?,
            );
        }
    }
    let steps: Vec<_> = steps.into_values().collect();
    let successors: HashMap<_, _> = steps
        .iter()
        .map(|step| (step.folded.clone(), step.survivor.clone()))
        .collect();
    let mut incoming: HashMap<String, Vec<String>> = HashMap::new();
    for step in &steps {
        incoming
            .entry(step.survivor.clone())
            .or_default()
            .push(step.folded.clone());
    }
    let mut keys = BTreeSet::new();
    for (entity, _) in supplied.records() {
        if row_handles.contains(entity.id.as_str()) {
            continue;
        }
        let key = entity
            .badge
            .as_deref()
            .filter(|id| !id.is_empty())
            .unwrap_or(entity.id.as_str());
        follow_thing::<String, MemoryError>(key.to_string(), |key| {
            Ok(successors.get(key).cloned())
        })?;
        let earlier =
            jojobot_domain::memory::claim_walk::folded_into_walk(key.to_string(), |key| {
                incoming.get(key).cloned().unwrap_or_default()
            })?;
        keys.insert(key.to_string());
        keys.extend(earlier);
    }
    let keys: Vec<_> = keys.into_iter().collect();
    let mut held = boot_reads::DoltBootReader::writes_by_key(tx, Some(&keys)).await?;
    apply_merges(&mut held, &steps);
    Ok(held)
}
