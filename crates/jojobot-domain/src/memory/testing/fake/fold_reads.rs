//! The reads the fake gives a fold to judge it by.

use super::*;

/// **What the fake reads to judge a fold**, from the copy of its claims taken
/// before the locks.
pub(super) struct FakeFoldReads<'a> {
    pub(super) store: &'a InMemoryMemory,
    pub(super) facts: &'a [Fact],
    pub(super) folded: &'a EntityId,
    pub(super) survivor: &'a EntityId,
    pub(super) folded_key: &'a EntityId,
    pub(super) survivor_key: &'a EntityId,
    pub(super) survivor_handle: &'a EntityId,
    pub(super) held: &'a std::collections::BTreeMap<String, String>,
}

#[async_trait::async_trait]
impl crate::memory::merge::MergeReads for FakeFoldReads<'_> {
    async fn carried(&mut self) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        let carried = crate::memory::folded_fields(
            &self.store.writes_on(self.folded_key, self.facts),
            &self.store.declarations(),
        );
        Ok(self.store.with_manager_served(&carried))
    }

    async fn lineage(
        &mut self,
        named: Option<EntityId>,
    ) -> Result<crate::memory::Lineage, MemoryError> {
        Ok(self
            .store
            .lineage_among(self.facts, self.survivor_handle, named))
    }

    async fn lineage_of(
        &mut self,
        subject: &EntityId,
    ) -> Result<crate::memory::Lineage, MemoryError> {
        Ok(self.store.lineage_among(self.facts, subject, None))
    }

    async fn chain_above_survivor(&mut self) -> Result<Vec<EntityId>, MemoryError> {
        Ok(self
            .store
            .chain_above_among(self.facts, self.survivor_handle))
    }

    async fn incoming(&mut self) -> Result<Vec<Fact>, MemoryError> {
        Ok(self
            .facts
            .iter()
            .filter(|f| &f.home == self.folded_key)
            .map(|f| {
                // **Read against the survivor**, because the duplicate's claims are
                // thoughts of the survivor's only if the survivor's bot wrote them.
                let another = crate::memory::authorship::is_connection(f.edge.as_ref())
                    && self
                        .store
                        .another_bots_claim(&f.home, &f.id, self.survivor_handle);
                let mut served = self.store.served(f.clone(), self.folded);
                served.written_by_another = another;
                served
            })
            .collect())
    }

    async fn survivors_room(&mut self) -> Result<crate::memory::merge::SurvivorRoom, MemoryError> {
        Ok(crate::memory::merge::SurvivorRoom {
            claims: self
                .facts
                .iter()
                .filter(|f| &f.home == self.survivor_key)
                .map(|f| self.store.served(f.clone(), self.survivor))
                .collect(),
            held: self.held.clone(),
        })
    }
}
