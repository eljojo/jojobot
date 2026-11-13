//! **A Memory whose claim writes carry no moment**, as writes imported from a store
//! that kept them before it stamped its writes do. Every other port method is
//! forwarded to a real in-memory store (`jojobot-forward`).

use super::*;

pub(crate) struct UnstampedWrites(pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for UnstampedWrites {
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<
        std::collections::HashMap<FactId, Vec<jojobot_domain::memory::ClaimWrite>>,
        MemoryError,
    > {
        let mut histories = self.0.claim_histories(entity).await?;
        for write in histories.values_mut().flatten() {
            write.written_at = None;
        }
        Ok(histories)
    }
}
