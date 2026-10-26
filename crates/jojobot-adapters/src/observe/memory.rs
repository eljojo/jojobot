//! Forward the Memory port without losing its overrides.

use super::{MemoryOperation, Observed, Operation};
use async_trait::async_trait;
use jiff::civil::Date;
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldAges, FieldBacking, FieldWrite, FormerHandle, Guarded, Memory, MemoryError,
    Merge, NewEntity, NewFact, Retraction, WriteSummary, filter, guard,
    holders::OrderedHead,
    numbered,
    search::DocScan,
    types::{self, DeclaredType},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[async_trait]
impl Memory for Observed<dyn Memory> {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        self.inner.image_field(entity, key)
    }

    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::AddEntity),
            self.inner.add_entity(new),
        )
        .await
    }
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::AddEntityWithFirstClaim),
            self.inner.add_entity_with_first_claim(new, first),
        )
        .await
    }
    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::RecordAppEvent),
            self.inner.record_app_event(event),
        )
        .await
    }
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::UpdateEntityWithClaim),
            self.inner.update_entity_with_claim(handle, patch, claim),
        )
        .await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ListEntities),
            self.inner.list_entities(kind),
        )
        .await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::FormerHandles),
            self.inner.former_handles(),
        )
        .await
    }
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Backing),
            self.inner.backing(entity),
        )
        .await
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::BuiltOn),
            self.inner.built_on(source),
        )
        .await
    }
    async fn keys_holding(
        &self,
        key: &str,
        test: &filter::Test,
        kind: Option<EntityKind>,
    ) -> Result<Vec<EntityId>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::KeysHolding),
            self.inner.keys_holding(key, test, kind),
        )
        .await
    }
    async fn ordered_first_k(
        &self,
        key: &str,
        holds: types::ValueType,
        direction: filter::Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> Result<OrderedHead, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::OrderedFirstK),
            self.inner
                .ordered_first_k(key, holds, direction, kind, among, k),
        )
        .await
    }
    fn read_width(&self) -> usize {
        self.inner.read_width()
    }
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ThoughtWriters),
            self.inner.thought_writers(thread),
        )
        .await
    }
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ReferringTo),
            self.inner.referring_to(target),
        )
        .await
    }
    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Children),
            self.inner.children(parent),
        )
        .await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::UpdateEntity),
            self.inner.update_entity(handle, patch),
        )
        .await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::RenameEntity),
            self.inner
                .rename_entity(from, to, parent, date, resemblance, session),
        )
        .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ArchiveEntity),
            self.inner.archive_entity(id, reason, session),
        )
        .await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::RestoreEntity),
            self.inner.restore_entity(id, session),
        )
        .await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Capture),
            self.inner.capture(fact),
        )
        .await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Recall),
            self.inner.recall(subject),
        )
        .await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::UpdateFact),
            self.inner.update_fact(address, patch, caller),
        )
        .await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::History),
            self.inner.history(entity, key),
        )
        .await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ClaimHistory),
            self.inner.claim_history(address),
        )
        .await
    }
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::EntityHistory),
            self.inner.entity_history(entity),
        )
        .await
    }
    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<BTreeSet<String>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::KeysKeptOffTheThing),
            self.inner.keys_kept_off_the_thing(address),
        )
        .await
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ClaimHistories),
            self.inner.claim_histories(entity),
        )
        .await
    }
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::MergeEvents),
            self.inner.merge_events(entity),
        )
        .await
    }
    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::MovedAddress),
            self.inner.moved_address(address),
        )
        .await
    }
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::unmapped::UnmappedClaim>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::UnmappedClaims),
            self.inner.unmapped_claims(entity),
        )
        .await
    }
    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::KeyTrails),
            self.inner.key_trails(entity),
        )
        .await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Fields),
            self.inner.fields(entity),
        )
        .await
    }
    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::FieldAges),
            self.inner.field_ages(entity),
        )
        .await
    }
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.answer(
            Operation::Memory(MemoryOperation::EchoedDefaults),
            self.inner.echoed_defaults(entity, fields),
        )
        .await
    }
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::FieldsVersioned),
            self.inner.fields_versioned(entity),
        )
        .await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Retract),
            self.inner.retract(address, reason, date, caller, session),
        )
        .await
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Merge, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::Merge),
            self.inner
                .merge(folded, survivor, reason, date, caller, session),
        )
        .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::SetProse),
            self.inner.set_prose(entity, prose, session),
        )
        .await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.result(Operation::Memory(MemoryOperation::Scan), self.inner.scan())
            .await
    }
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ScanEntity),
            self.inner.scan_entity(entity),
        )
        .await
    }
    async fn has_prose(&self, entity: &EntityId) -> Result<bool, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::HasProse),
            self.inner.has_prose(entity),
        )
        .await
    }
    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ComposedProse),
            self.inner.composed_prose(entity, own),
        )
        .await
    }
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::WriteSummary),
            self.inner.write_summary(),
        )
        .await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DeclareType),
            self.inner.declare_type(declared, by_session),
        )
        .await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DeclaredTypes),
            self.inner.declared_types(),
        )
        .await
    }
    async fn declared_types_shared(
        &self,
    ) -> Result<std::sync::Arc<Vec<DeclaredType>>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DeclaredTypesShared),
            self.inner.declared_types_shared(),
        )
        .await
    }
    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DisplacedType),
            self.inner.displaced_type(name),
        )
        .await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DeclareKind),
            self.inner.declare_kind(token, origin, fields),
        )
        .await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::ReclaimKind),
            self.inner.reclaim_kind(token),
        )
        .await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::DeclaredKinds),
            self.inner.declared_kinds(),
        )
        .await
    }
    async fn numbering_of(
        &self,
        entity: &EntityId,
    ) -> Result<Option<numbered::Numbering>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::NumberingOf),
            self.inner.numbering_of(entity),
        )
        .await
    }
    async fn numbers_held(&self, asked: &EntityId) -> Result<Vec<guard::EntityMatch>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::NumbersHeld),
            self.inner.numbers_held(asked),
        )
        .await
    }
    async fn numbered_scope(
        &self,
        collection: &EntityId,
        all: &[Entity],
    ) -> Result<Option<numbered::Numbered>, MemoryError> {
        self.result(
            Operation::Memory(MemoryOperation::NumberedScope),
            self.inner.numbered_scope(collection, all),
        )
        .await
    }
}
