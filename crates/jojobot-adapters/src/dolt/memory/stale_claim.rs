//! The refusals of an edit whose claim has moved on, counted in the transaction
//! the write appends in.

use super::*;

impl DoltMemory {
    /// **An edit made against a revision the claim has moved past is refused
    /// before anything is written**, and so is one whose claim another claim has
    /// displaced as the backing of a key it must back.
    pub(super) async fn refuses_a_stale_claim(
        &self,
        tx: &mut Transaction<'_, MySql>,
        key: &EntityId,
        address: &FactAddress,
        patch: &FactPatch,
    ) -> Result<(), MemoryError> {
        if patch.expected_revision.is_some() {
            // The writes are counted where the claim is stored, which is not
            // where it is addressed when a merge left it under the folded key.
            let stored = self
                .stored_address_of(tx, &FactAddress::new(key.clone(), address.local.clone()))
                .await?
                .unwrap_or_else(|| FactAddress::new(key.clone(), address.local.clone()));
            let current: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM fact_write WHERE entity = ? AND fact_id = ?",
            )
            .bind(stored.home.as_str())
            .bind(stored.local.as_str())
            .fetch_one(&mut **tx)
            .await
            .map_err(store)?;
            if let Some(refused) = jojobot_domain::memory::refuses_a_stale_revision(
                address,
                patch.expected_revision,
                current as usize,
            ) {
                return Err(refused);
            }
        }
        if !patch.must_back.is_empty() {
            let writes = self.writes_on(tx, key).await?;
            let declared = Self::types_in(tx).await?;
            let backing = jojobot_domain::memory::folded_backing(&writes, &declared);
            if let Some(refused) = jojobot_domain::memory::refuses_a_displaced_backing(
                address,
                patch.expected_revision,
                &patch.must_back,
                &backing,
            ) {
                return Err(refused);
            }
        }
        Ok(())
    }
}
