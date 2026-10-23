//! A store that forwards the port's required methods and the two combined
//! writes, and serves a subject's records in the order of their ids as text.

use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;

use super::{
    ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FieldWrite, FormerHandle, Guarded, Memory, MemoryError, Merge, NewEntity, NewFact, Retraction,
    search,
};
use crate::memory::Archived;
use crate::memory::types::{DeclaredType, Field, Origin};

/// **A store that serves `recall` in the order the real store does when records
/// of one thing are filed under ids of different lengths**: sorted by the id as
/// text, so `f10` comes before `f9`. A reader that takes the last record served
/// as the newest is right on the fake and wrong here, which is what a case run
/// against this store is for. Every other method is forwarded as it is.
pub struct ServedInIdTextOrder(pub Arc<dyn Memory>);

#[async_trait]
impl Memory for ServedInIdTextOrder {
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.0.former_handles().await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.0.list_entities(kind).await
    }
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.0.add_entity(new).await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.0.declare_type(declared, by_session).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: Origin,
        fields: Vec<Field>,
    ) -> Result<bool, MemoryError> {
        self.0.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, Origin)>, MemoryError> {
        self.0.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.0.reclaim_kind(token).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.0.declared_types().await
    }
    async fn update_entity(
        &self,
        id: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0.update_entity(id, patch).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: jiff::civil::Date,
        resemblance: crate::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.0.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.0.restore_entity(id, session).await
    }
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.0.add_entity_with_first_claim(new, first).await
    }
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.0.update_entity_with_claim(handle, patch, claim).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.0.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        let mut facts = self.0.recall(subject).await?;
        facts.sort_by(|a, b| a.id.as_str().cmp(b.id.as_str()));
        Ok(facts)
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.0.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.0.claim_history(address).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.0.fields(entity).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.0.update_fact(address, patch, caller).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.0.retract(address, reason, date, caller, session).await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        self.0
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.0.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<search::DocScan>, MemoryError> {
        self.0.scan().await
    }
}
