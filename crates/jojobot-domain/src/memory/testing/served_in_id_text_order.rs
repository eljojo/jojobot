//! A store that forwards the port's required methods and the two combined
//! writes, and serves a subject's records in the order of their ids as text.

use std::sync::Arc;

use super::Memory;

/// **A store that serves `recall` in the order the real store does when records
/// of one thing are filed under ids of different lengths**: sorted by the id as
/// text, so `f10` comes before `f9`. A reader that takes the last record served
/// as the newest is right on the fake and wrong here, which is what a case run
/// against this store is for. Every other method is forwarded as it is.
pub struct ServedInIdTextOrder(pub Arc<dyn Memory>);

#[crate::forward(
    forward_memory,
    via = 0,
    required_only,
    also(add_entity_with_first_claim, update_entity_with_claim)
)]
impl Memory for ServedInIdTextOrder {
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let mut facts = self.0.recall(subject).await?;
        facts.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(facts)
    }
}
