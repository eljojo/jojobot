//! **A Memory whose writes land and whose fold never confirms**, forwarding every
//! other port method to a real in-memory store (`down_guard` holds that).

use super::*;

/// **Every write lands and the fold behind it never confirms — the surface's
/// own view of what `Folded` returns when its refresh fails after a
/// successful inner write.** `jojobot-adapters`' own tests prove `Folded`
/// converts that shape into [`MemoryError::FoldBehind`]; this fake produces
/// the same error directly, so a verb's handling of it is tested without
/// standing up a real fold and a store that fails on cue underneath it.
pub(crate) struct FoldBehindMemory(pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 0)]
impl Memory for FoldBehindMemory {
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        match self.0.capture(fact).await? {
            Guarded::Written(fact) => Err(fold_behind(Landed::Fact(Box::new(fact)))),
            blocked => Ok(blocked),
        }
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        match self.0.update_fact(address, patch, caller).await? {
            Guarded::Written(fact) => Err(fold_behind(Landed::Fact(Box::new(fact)))),
            blocked => Ok(blocked),
        }
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Retraction, MemoryError> {
        let taken_back = self
            .0
            .retract(address, reason, date, caller, session)
            .await?;
        Err(fold_behind(Landed::Retraction(Box::new(taken_back))))
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Merge, MemoryError> {
        let done = self
            .0
            .merge(folded, survivor, reason, date, caller, session)
            .await?;
        Err(fold_behind(Landed::Merge(Box::new(done))))
    }
    // **A thing and its claim in one act land, and the fold behind them never
    // confirms**, the shape `capture` has here: the trait's default would refuse the
    // act, which no store does.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        match self.0.add_entity_with_first_claim(new, first).await? {
            Guarded::Written(created) => Err(fold_behind(Landed::Creation(Box::new(created)))),
            blocked => Ok(blocked),
        }
    }
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        match self
            .0
            .update_entity_with_claim(handle, patch, claim)
            .await?
        {
            Guarded::Written(edited) => Err(fold_behind(Landed::Creation(Box::new(edited)))),
            blocked => Ok(blocked),
        }
    }
}

fn fold_behind(landed: Landed) -> MemoryError {
    MemoryError::FoldBehind {
        landed,
        behind: Behind::Stale,
        source: Box::new(MemoryError::Store("the fold's re-read failed".into())),
    }
}
