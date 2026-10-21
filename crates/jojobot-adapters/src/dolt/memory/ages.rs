//! **The newest write that counts as a change of a thing**, in the Dolt store.
//!
//! The decision is the domain's ([`jojobot_domain::memory::review::counts_as_a_change`]):
//! jojobot's own bookkeeping, the record of the release notes a bot was told,
//! is not a change. This reads the rows that decision is asked of.

use std::collections::BTreeSet;

use jojobot_domain::memory::review;

use super::*;

impl DoltMemory {
    /// **The newest moment of any field write or record write stored under these
    /// names**, leaving out the writes that are no change of the thing. `ledgers`
    /// are the records that carry the ledger, as the thing's own writes found them.
    pub(super) async fn newest_change(
        tx: &mut Transaction<'_, MySql>,
        names: &[String],
        ledgers: &BTreeSet<String>,
        key: &EntityId,
    ) -> Result<Option<jiff::Timestamp>, MemoryError> {
        let mut newest: Option<jiff::Timestamp> = None;
        for name in names {
            let fields =
                sqlx::query("SELECT `key`, fact_id, written_at FROM field_write WHERE entity = ?")
                    .bind(name)
                    .fetch_all(&mut **tx)
                    .await
                    .map_err(store)?;
            for row in &fields {
                let held: String = row.try_get("key").map_err(store)?;
                let fact: String = row.try_get("fact_id").map_err(store)?;
                if review::counts_as_a_change(Some(&held), &fact, ledgers) {
                    let stamp: Option<String> = row.try_get("written_at").map_err(store)?;
                    newest = newest.max(parse_stamp(stamp, key.as_str()));
                }
            }
            let facts = sqlx::query("SELECT fact_id, written_at FROM fact_write WHERE entity = ?")
                .bind(name)
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?;
            for row in &facts {
                let fact: String = row.try_get("fact_id").map_err(store)?;
                if review::counts_as_a_change(None, &fact, ledgers) {
                    let stamp: Option<String> = row.try_get("written_at").map_err(store)?;
                    newest = newest.max(parse_stamp(stamp, key.as_str()));
                }
            }
        }
        Ok(newest)
    }
}
