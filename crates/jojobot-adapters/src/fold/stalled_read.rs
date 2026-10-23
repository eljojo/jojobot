//! A store double whose first versioned read stalls, for the test that two
//! writes reach the image in the order they reached the store.

use super::*;
use jojobot_domain::memory::testing::InMemoryMemory;

/// **A double whose write count is its own, and whose `fields_versioned`
/// stalls once, right after taking its snapshot.**
///
/// An in-memory store never yields inside the span a refresh runs, so two
/// futures against it on one runtime run one after the other and never
/// interleave — a race written against it would pass on the BROKEN code
/// for the same reason it never watches anything race. This stalls the
/// FIRST `fields_versioned` call after its snapshot is already taken, so
/// a test can let a second, faster write-and-refresh land entirely while
/// the first is still holding an early snapshot open — the exact
/// interleaving the `Folded::writing` gate exists for.
pub(super) struct StallFirstVersionedRead {
    pub(super) inner: Arc<InMemoryMemory>,
    pub(super) written: std::sync::atomic::AtomicU64,
    pub(super) stalled: Arc<tokio::sync::Notify>,
    pub(super) released: Arc<tokio::sync::Notify>,
    pub(super) first: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl Memory for StallFirstVersionedRead {
    async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.add_entity(new).await
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        self.inner.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
        self.inner.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: EntityPatch,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.inner.update_entity(handle, patch).await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, Archived), MemoryError> {
        self.inner.restore_entity(id, session).await
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
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    /// **The counter moves here, before the write is even reported back**
    /// — a write nobody has refreshed for yet still counts, because what
    /// this counts is "how many writes has the store taken," never "how
    /// many the fold has caught up with."
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.capture(fact).await?;
        if matches!(written, Guarded::Written(_)) {
            self.written
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(written)
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.inner.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.inner.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.inner.claim_history(address).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        let written = self.inner.update_fact(address, patch, caller).await?;
        if matches!(written, Guarded::Written(_)) {
            self.written
                .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
        Ok(written)
    }
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        self.inner.fields(entity).await
    }
    /// **The one method this double exists to control.** The snapshot is
    /// taken immediately — exactly as fresh as an unstalled read would be
    /// — and only the RETURN is held back, on the first call alone.
    async fn fields_versioned(
        &self,
        entity: &EntityId,
    ) -> Result<(BTreeMap<String, String>, u64), MemoryError> {
        let fields = self.inner.fields(entity).await?;
        let version = self.written.load(std::sync::atomic::Ordering::SeqCst);
        if self.first.swap(false, std::sync::atomic::Ordering::SeqCst) {
            self.stalled.notify_one();
            self.released.notified().await;
        }
        Ok((fields, version))
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Retraction, MemoryError> {
        let retracted = self
            .inner
            .retract(address, reason, date, caller, session)
            .await?;
        self.written
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(retracted)
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
        self.inner.set_prose(entity, prose, session).await
    }
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.inner.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }
    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await
    }
}
