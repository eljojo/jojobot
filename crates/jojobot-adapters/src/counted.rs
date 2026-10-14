//! [`Counted`] — a [`Memory`] that counts every call that reaches the store
//! beneath it, and yields where the store yields.
//!
//! The in-memory fold (decision log 292) promises that a read after startup
//! issues no store statement. A statement is a call into the store, so the
//! store's port is where it is counted: each method adds one, then forwards.
//! The count is only ever read next to an answer compared with the store's own
//! (see [`crate::fold`]'s tests): zero calls with a wrong answer proves nothing.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use async_trait::async_trait;
use jiff::civil::Date;
use jojobot_domain::memory::{
    Archived, ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactId,
    FactPatch, FieldAges, FieldBacking, FieldWrite, FormerHandle, Guarded, Memory, MemoryError,
    Merge, NewEntity, NewFact, Retraction, WriteSummary,
    search::DocScan,
    types::{self, DeclaredType},
};

/// A store that counts the calls made to it, by name.
pub struct Counted {
    inner: Arc<dyn Memory>,
    total: AtomicUsize,
    by_name: std::sync::Mutex<BTreeMap<&'static str, usize>>,
    /// While set, the listing every fill of the image starts with fails.
    unreachable: AtomicBool,
    /// Set once: the next call of this name commits in the store and then never
    /// answers.
    hang_after: std::sync::Mutex<Option<&'static str>>,
}

impl Counted {
    pub fn new(inner: Arc<dyn Memory>) -> Self {
        Counted {
            inner,
            total: AtomicUsize::new(0),
            by_name: std::sync::Mutex::default(),
            unreachable: AtomicBool::new(false),
            hang_after: std::sync::Mutex::new(None),
        }
    }

    /// **A store that cannot be reached for the reads that fill an image.** The
    /// listing fails while `down` is set; every other call, writes included,
    /// goes through, so a case can fail a first fill and let the store recover.
    pub fn set_unreachable(&self, down: bool) {
        self.unreachable.store(down, Ordering::SeqCst);
    }

    /// **The next call of `name` commits and its caller never hears.** The store
    /// writes, then the call stays pending, so a caller that stops waiting drops a
    /// future whose write already landed. Only `capture` and `add_entity` hang.
    pub fn hang_after_next(&self, name: &'static str) {
        *self.hang_after.lock().expect("count lock") = Some(name);
    }

    async fn hang_if_armed(&self, name: &'static str) {
        let armed = {
            let mut slot = self.hang_after.lock().expect("count lock");
            match *slot == Some(name) {
                true => slot.take().is_some(),
                false => false,
            }
        };
        if armed {
            std::future::pending::<()>().await;
        }
    }

    /// Calls made since the last [`reset`](Self::reset).
    pub fn calls(&self) -> usize {
        self.total.load(Ordering::SeqCst)
    }

    /// The calls made since the last reset, by method name — what a failed
    /// assertion prints, so a reader sees WHICH read reached the store.
    pub fn seen(&self) -> BTreeMap<&'static str, usize> {
        self.by_name.lock().expect("count lock").clone()
    }

    pub fn reset(&self) {
        self.total.store(0, Ordering::SeqCst);
        self.by_name.lock().expect("count lock").clear();
    }

    fn hit(&self, name: &'static str) {
        self.total.fetch_add(1, Ordering::SeqCst);
        *self
            .by_name
            .lock()
            .expect("count lock")
            .entry(name)
            .or_default() += 1;
    }
}

#[async_trait]
impl Memory for Counted {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.hit("add_entity");
        let landed = self.inner.add_entity(new).await;
        self.hang_if_armed("add_entity").await;
        landed
    }
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.hit("add_entity_with_first_claim");
        self.inner.add_entity_with_first_claim(new, first).await
    }
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
        claim: NewFact,
    ) -> Result<Guarded<(Entity, Fact)>, MemoryError> {
        self.hit("update_entity_with_claim");
        self.inner
            .update_entity_with_claim(handle, patch, claim)
            .await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.hit("list_entities");
        if self.unreachable.load(Ordering::SeqCst) {
            return Err(MemoryError::Store("the store is unreachable".into()));
        }
        self.inner.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.hit("former_handles");
        self.inner.former_handles().await
    }
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<BTreeMap<String, FieldBacking>, MemoryError> {
        self.hit("backing");
        self.inner.backing(entity).await
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        self.hit("built_on");
        self.inner.built_on(source).await
    }
    async fn thought_writers(&self, thread: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.hit("thought_writers");
        self.inner.thought_writers(thread).await
    }
    async fn referring_to(&self, target: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.hit("referring_to");
        self.inner.referring_to(target).await
    }
    async fn children(&self, parent: &EntityId) -> Result<Vec<EntityId>, MemoryError> {
        self.hit("children");
        self.inner.children(parent).await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.hit("update_entity");
        self.inner.update_entity(handle, patch).await
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
        self.hit("rename_entity");
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.hit("archive_entity");
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.hit("restore_entity");
        self.inner.restore_entity(id, session).await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.hit("capture");
        let landed = self.inner.capture(fact).await;
        self.hang_if_armed("capture").await;
        landed
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.hit("recall");
        self.inner.recall(subject).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.hit("update_fact");
        self.inner.update_fact(address, patch, caller).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.hit("history");
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.hit("claim_history");
        self.inner.claim_history(address).await
    }
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::EntityVersion>, MemoryError> {
        self.hit("entity_history");
        self.inner.entity_history(entity).await
    }
    async fn keys_kept_off_the_thing(
        &self,
        address: &FactAddress,
    ) -> Result<BTreeSet<String>, MemoryError> {
        self.hit("keys_kept_off_the_thing");
        self.inner.keys_kept_off_the_thing(address).await
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<HashMap<FactId, Vec<ClaimWrite>>, MemoryError> {
        self.hit("claim_histories");
        self.inner.claim_histories(entity).await
    }
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<jojobot_domain::memory::MergeEvent>, MemoryError> {
        self.hit("merge_events");
        self.inner.merge_events(entity).await
    }
    async fn moved_address(
        &self,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        self.hit("moved_address");
        self.inner.moved_address(address).await
    }
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<jojobot_domain::memory::unmapped::UnmappedClaim>, MemoryError> {
        self.hit("unmapped_claims");
        self.inner.unmapped_claims(entity).await
    }
    async fn key_trails(
        &self,
        entity: &EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, MemoryError> {
        self.hit("key_trails");
        self.inner.key_trails(entity).await
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.hit("fields");
        self.inner.fields(entity).await
    }
    async fn field_ages(&self, entity: &EntityId) -> Result<FieldAges, MemoryError> {
        self.hit("field_ages");
        self.inner.field_ages(entity).await
    }
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &BTreeMap<String, String>,
    ) -> Vec<String> {
        self.hit("echoed_defaults");
        self.inner.echoed_defaults(entity, fields).await
    }
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        self.hit("fields_versioned");
        self.inner.fields_versioned(entity).await
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        self.hit("retract");
        self.inner
            .retract(address, reason, date, caller, session)
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
        self.hit("merge");
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, MemoryError> {
        self.hit("set_prose");
        self.inner.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.hit("scan");
        self.inner.scan().await
    }
    async fn scan_entity(&self, entity: &EntityId) -> Result<Option<DocScan>, MemoryError> {
        self.hit("scan_entity");
        self.inner.scan_entity(entity).await
    }
    async fn composed_prose(&self, entity: &EntityId, own: &str) -> Result<String, MemoryError> {
        self.hit("composed_prose");
        self.inner.composed_prose(entity, own).await
    }
    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.hit("write_summary");
        self.inner.write_summary().await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.hit("declare_type");
        self.inner.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.hit("declared_types");
        self.inner.declared_types().await
    }
    async fn displaced_type(&self, name: &str) -> Result<Option<types::Displaced>, MemoryError> {
        self.hit("displaced_type");
        self.inner.displaced_type(name).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<(), MemoryError> {
        self.hit("declare_kind");
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.hit("reclaim_kind");
        self.inner.reclaim_kind(token).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.hit("declared_kinds");
        self.inner.declared_kinds().await
    }
}
