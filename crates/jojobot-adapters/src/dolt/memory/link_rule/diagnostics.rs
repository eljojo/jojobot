use super::*;
use std::collections::BTreeSet;

mod r1_locations;

/// A location the operator can inspect, with no field value or stored id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OwnedNowField {
    pub entity: EntityId,
    pub key: String,
}

/// The read-only conversion counts and the R1 locations that need repair.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LinkRuleDiagnostics {
    pub summary: LinkRuleSummary,
    pub owned_now_fields: Vec<OwnedNowField>,
    /// R1 items whose holder has no entity row; the total still includes them.
    pub owned_now_unaddressable: usize,
    /// Distinct rowless holders of those items, never exposed by name or id.
    pub owned_now_unaddressable_holders: usize,
}

impl DoltMemory {
    /// Read the conversion and its repair locations in one transaction.
    pub async fn link_rule_diagnostics(&self) -> Result<LinkRuleDiagnostics, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut directory = self.directory_in(&mut tx).await?;
        directory.keep_older = crate::dolt::finished_passes(&self.pool)
            .await
            .contains(FIELD_LOWERING_PASS);
        let writes: Vec<(String, String, i64, String)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write WHERE value IS NOT NULL",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let held: BTreeSet<(String, String, i64, String)> =
            sqlx::query_as::<_, (String, String, i64, String)>(
                "SELECT entity, `key`, ordinal, target FROM field_link",
            )
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?
            .into_iter()
            .collect();
        let handles = r1_locations::handles_in(&mut tx).await?;
        let mut unaddressable_holders = BTreeSet::new();
        let mut report = LinkRuleDiagnostics::default();
        let summary = &mut report.summary;
        let mut expected = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            summary.values_read += 1;
            let marking = directory.mark(key, value);
            summary.bare_ids += marking.bare;
            summary.handles += marking.handles;
            summary.owned_now += marking.owned_now;
            if marking.owned_now > 0 {
                if let Some(handle) = handles.get(entity) {
                    for _ in 0..marking.owned_now {
                        report.owned_now_fields.push(OwnedNowField {
                            entity: handle.clone(),
                            key: key.clone(),
                        });
                    }
                } else {
                    report.owned_now_unaddressable += marking.owned_now;
                    unaddressable_holders.insert(entity.clone());
                }
            }
            if marking.mixed && marking.handles > 0 {
                summary.mixed_lists += 1;
            }
            for (_, ambiguous) in &marking.left {
                match ambiguous {
                    true => summary.ambiguous += 1,
                    false => summary.unresolved += 1,
                }
            }
            for target in jojobot_domain::memory::at_write::field_link_targets(&marking.value) {
                expected.insert((entity.clone(), key.clone(), *ordinal, target));
            }
        }
        summary.links_dropped = held.difference(&expected).count();
        summary.links_added = expected.difference(&held).count();
        report.owned_now_unaddressable_holders = unaddressable_holders.len();
        Ok(report)
    }
}
