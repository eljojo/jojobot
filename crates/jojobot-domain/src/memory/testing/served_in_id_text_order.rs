//! A store that forwards the port's required methods and the two combined
//! writes, and serves a subject's records in the order of their ids as text.

use std::sync::Arc;

use super::Memory;

/// **A double that serves `recall` by the id as text**, so `f10` comes before
/// `f9`. The real store no longer lists a thing's claims that way (it lists them by
/// the number in the label), but a reader must not lean on the order, and this
/// double is how a case holds it to that: a reader that takes the last record
/// served as the newest is right on the fake and wrong here. Every other method is
/// forwarded as it is.
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
