use super::*;
use std::collections::{BTreeMap, BTreeSet};

mod current;
mod r1_locations;
mod supplied_only;

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
    /// R1 items in superseded writes, reported without repair locations.
    pub owned_historical: usize,
    pub owned_now_fields: Vec<OwnedNowField>,
    /// Addressable R1 items beyond the bounded sample; still counted in full.
    pub owned_now_fields_left_out: usize,
    /// R1 items whose holder has no entity row; the total still includes them.
    pub owned_now_unaddressable: usize,
    /// Distinct rowless holders of those items, never exposed by name or id.
    pub owned_now_unaddressable_holders: usize,
    /// Distinct supplied records without a row, holding marked or mixed links now.
    pub supplied_only_current: usize,
    /// Distinct such records with any eligible historical field write.
    pub supplied_only_historical: usize,
}

impl DoltMemory {
    /// Read the conversion and its repair locations in one transaction.
    pub async fn link_rule_diagnostics(&self) -> Result<LinkRuleDiagnostics, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut directory = self.directory_in(&mut tx).await?;
        directory.keep_older = crate::dolt::finished_passes(&self.pool)
            .await
            .contains(FIELD_LOWERING_PASS);
        let writes: Vec<(String, String, i64, Option<String>)> = sqlx::query_as(
            "SELECT entity, `key`, ordinal, value FROM field_write
             ORDER BY entity, `key`, ordinal DESC",
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
        let mut unaddressable_holders = BTreeSet::new();
        let mut historical = BTreeMap::new();
        let mut report = LinkRuleDiagnostics::default();
        let summary = &mut report.summary;
        let mut expected = BTreeSet::new();
        let mut newest = BTreeSet::new();
        for (entity, key, ordinal, value) in &writes {
            // A clear is the newest write too; skipping it first revives history.
            let held_now = newest.insert((entity, key));
            let Some(value) = value else { continue };
            summary.values_read += 1;
            let marking = directory.mark(key, value);
            let eligible = historical.entry(entity.clone()).or_insert(false);
            *eligible |= marking.marked_or_mixed_links();
            summary.bare_ids += marking.bare;
            summary.handles += marking.handles;
            if !held_now {
                report.owned_historical += marking.owned_now;
            } else if marking.owned_now > 0 {
                summary.owned_now += marking.owned_now;
                if let Some(handle) = directory.handle_of.get(entity) {
                    r1_locations::push_bounded(
                        &mut report.owned_now_fields,
                        &mut report.owned_now_fields_left_out,
                        handle,
                        key,
                        marking.owned_now,
                    );
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
        (
            report.supplied_only_current,
            report.supplied_only_historical,
        ) = self
            .supplied_only_link_counts(&mut tx, &directory, &historical)
            .await?;
        Ok(report)
    }
}
