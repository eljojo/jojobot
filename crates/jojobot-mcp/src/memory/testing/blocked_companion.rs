//! **A Memory whose claim written beside a merge is always blocked**, forwarding
//! every other port method to a real in-memory store, generated from the port
//! (`jojobot-forward`).

use super::*;

/// **The merge-with-claim act answers blocked, as a store does when the claim it
/// carries meets a guard.** The verb builds the claim itself, so no real store is
/// made to block it; this fake produces the answer directly, so the verb's handling
/// of it is tested.
pub(crate) struct BlockedCompanionMemory(pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for BlockedCompanionMemory {
    async fn merge_with_claim(
        &self,
        _folded: &EntityId,
        _survivor: &EntityId,
        _reason: Option<&str>,
        _date: jiff::civil::Date,
        _caller: &EntityId,
        claim: NewFact,
    ) -> Result<Guarded<(jojobot_domain::memory::Merge, Fact)>, MemoryError> {
        Ok(Guarded::Blocked {
            attempted: claim.subject,
            candidates: Vec::new(),
        })
    }
}
