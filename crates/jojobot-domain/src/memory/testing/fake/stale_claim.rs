//! The refusals of an edit whose claim has moved on, under the lock the write
//! appends under.

use super::*;

impl InMemoryMemory {
    /// **An edit made against a revision the claim has moved past is refused
    /// before anything is written**, and so is one whose claim another claim has
    /// displaced as the backing of a key it must back.
    pub(super) fn refuses_a_stale_claim(
        &self,
        key: &EntityId,
        facts: &[Fact],
        address: &FactAddress,
        patch: &FactPatch,
    ) -> Result<(), MemoryError> {
        if patch.expected_revision.is_some() {
            let current = self
                .claim_writes_view()
                .iter()
                .filter(|(home, id, _)| home == key && id == &address.local)
                .count();
            if let Some(refused) = super::super::super::refuses_a_stale_revision(
                address,
                patch.expected_revision,
                current,
            ) {
                return Err(refused);
            }
        }
        if !patch.must_back.is_empty() {
            let backing = super::super::super::folded_backing(
                &Self::key_writes_on(&self.writes_view(), key, facts),
                &self.declarations(),
            );
            if let Some(refused) = super::super::super::refuses_a_displaced_backing(
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
