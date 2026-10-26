//! **A Memory whose writes land and whose fold never confirms**, forwarding every
//! other port method to a real in-memory store (`down_guard` holds that).

use super::*;
use jojobot_domain::memory::FactId;
use std::collections::BTreeMap;

/// **Every write lands and the fold behind it never confirms — the surface's
/// own view of what `Folded` returns when its refresh fails after a
/// successful inner write.** `jojobot-adapters`' own tests prove `Folded`
/// converts that shape into [`MemoryError::FoldBehind`]; this fake produces
/// the same error directly, so a verb's handling of it is tested without
/// standing up a real fold and a store that fails on cue underneath it.
pub(crate) struct FoldBehindMemory(pub(crate) Arc<InMemoryMemory>);

#[async_trait]
impl Memory for FoldBehindMemory {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        self.0.image_field(entity, key)
    }

    async fn former_handles(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::FormerHandle>, MemoryError> {
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
        declared: jojobot_domain::memory::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<jojobot_domain::memory::types::DeclaredType, MemoryError> {
        self.0.declare_type(declared, by_session).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<bool, MemoryError> {
        self.0.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        self.0.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.0.reclaim_kind(token).await
    }
    async fn declared_types(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::types::DeclaredType>, MemoryError> {
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
        resemblance: jojobot_domain::memory::Override<'_>,
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
    ) -> Result<(Entity, jojobot_domain::memory::Archived), MemoryError> {
        self.0.restore_entity(id, session).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        match self.0.capture(fact).await? {
            Guarded::Written(fact) => Err(fold_behind(Landed::Fact(Box::new(fact)))),
            blocked => Ok(blocked),
        }
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.0.recall(subject).await
    }
    async fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, MemoryError> {
        self.0.history(entity, key).await
    }
    async fn claim_history(
        &self,
        address: &jojobot_domain::memory::FactAddress,
    ) -> Result<Vec<jojobot_domain::memory::ClaimWrite>, MemoryError> {
        self.0.claim_history(address).await
    }
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        self.0.fields(entity).await
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
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.0.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<jojobot_domain::memory::search::DocScan>, MemoryError> {
        self.0.scan().await
    }
    // The methods the trait gives a body of its own, forwarded so that a case built on
    // this double meets the store's answer and not the default's.
    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        self.0.record_app_event(event).await
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.0.built_on(source).await
    }
    async fn field_ages(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::FieldAges, MemoryError> {
        self.0.field_ages(entity).await
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
    async fn numbering_of(
        &self,
        entity: &EntityId,
    ) -> Result<Option<jojobot_domain::memory::numbered::Numbering>, MemoryError> {
        self.0.numbering_of(entity).await
    }
    async fn numbers_held(
        &self,
        asked: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::guard::EntityMatch>, MemoryError> {
        self.0.numbers_held(asked).await
    }
    async fn numbered_scope(
        &self,
        collection: &EntityId,
        all: &[Entity],
    ) -> Result<Option<jojobot_domain::memory::numbered::Numbered>, MemoryError> {
        self.0.numbered_scope(collection, all).await
    }
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, jojobot_domain::memory::FieldBacking>, MemoryError> {
        self.0.backing(entity).await
    }
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.0.thought_writers(thread).await
    }
    async fn keys_holding(
        &self,
        key: &str,
        test: &jojobot_domain::memory::filter::Test,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        self.0.keys_holding(key, test, kind).await
    }
    async fn ordered_first_k(
        &self,
        key: &str,
        holds: jojobot_domain::memory::types::ValueType,
        direction: jojobot_domain::memory::filter::Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> Result<jojobot_domain::memory::holders::OrderedHead, MemoryError> {
        self.0
            .ordered_first_k(key, holds, direction, kind, among, k)
            .await
    }
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.0.referring_to(target).await
    }
    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.0.children(parent).await
    }
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.0.entity_history(entity).await
    }
    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, MemoryError> {
        self.0.keys_kept_off_the_thing(address).await
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<
        std::collections::HashMap<FactId, Vec<jojobot_domain::memory::ClaimWrite>>,
        MemoryError,
    > {
        self.0.claim_histories(entity).await
    }
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        self.0.merge_events(entity).await
    }
    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.0.moved_address(address).await
    }
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::unmapped::UnmappedClaim>, MemoryError> {
        self.0.unmapped_claims(entity).await
    }
    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.0.key_trails(entity).await
    }
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.0.echoed_defaults(entity, fields).await
    }
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        self.0.fields_versioned(entity).await
    }
    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<jojobot_domain::memory::search::DocScan>, MemoryError> {
        self.0.scan_entity(entity).await
    }
    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        self.0.has_prose(entity).await
    }
    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.0.composed_prose(entity, own).await
    }
    async fn write_summary(
        &self,
    ) -> Result<Option<jojobot_domain::memory::WriteSummary>, MemoryError> {
        self.0.write_summary().await
    }
    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<jojobot_domain::memory::types::DeclaredType>>, MemoryError> {
        self.0.declared_types_shared().await
    }
    async fn displaced_type(
        &self,
        name: &str,
    ) -> Result<Option<jojobot_domain::memory::types::Displaced>, MemoryError> {
        self.0.displaced_type(name).await
    }
    fn read_width(&self) -> usize {
        self.0.read_width()
    }
}

fn fold_behind(landed: Landed) -> MemoryError {
    MemoryError::FoldBehind {
        landed,
        behind: Behind::Stale,
        source: Box::new(MemoryError::Store("the fold's re-read failed".into())),
    }
}
