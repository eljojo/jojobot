use super::{Arc, InMemoryMemory, InMemoryRequests};

/// **A store that can stop being readable, the way a real one does.**
///
/// The service behind memory is a process on a network: it is up when jojobot
/// boots or it is not, and it can go away afterwards. Every other story runs
/// against a store that is always there, so the degraded half of the surface —
/// what a search says about its own coverage — has never been reached by one.
///
/// It stages the WORLD, not the answer. Only the reads that fill the index
/// fail; reads and writes still land, exactly as they would against a store
/// whose listing call is failing. **Two switches, because two different
/// stores are being modelled**: one that has gone away
/// ([`Blindable::goes_away`]) and one whose read of the whole corpus fails while
/// a read of one document still answers ([`Blindable::the_corpus_read_fails`]). Nothing here touches the
/// index or the coverage it reports: the story asks jojobot through the served
/// surface and jojobot works out what to say.
/// The instance settings query can fail independently while the roster query
/// still answers.
pub struct Blindable {
    inner: Arc<InMemoryMemory>,
    /// The instance settings read fails independently of projection reads.
    instance_blind: std::sync::atomic::AtomicBool,
    /// The store has gone away: the whole-corpus read and the one-document read
    /// both fail.
    blind: std::sync::atomic::AtomicBool,
    /// The whole-corpus read fails and the one-document read answers.
    corpus_blind: std::sync::atomic::AtomicBool,
    /// Writes read back with no moment, as writes the store kept before it
    /// stamped them do.
    unstamped: std::sync::atomic::AtomicBool,
    /// The next read of one document waits here, once armed. See
    /// [`Blindable::holds_the_next_document_read`].
    document_gate: std::sync::Mutex<Option<Arc<DocumentGate>>>,
}

/// **A read of one document held until the story lets it go.** The image of the
/// memory reads a document again after each write, so a read held here is a
/// write that has reached the store and not yet the image.
pub struct DocumentGate {
    reached: tokio::sync::Notify,
    released: tokio::sync::Notify,
}

impl DocumentGate {
    /// Resolves once a read is waiting on the gate.
    pub async fn reached(&self) {
        self.reached.notified().await;
    }

    /// Lets the waiting read go on.
    pub fn release(&self) {
        self.released.notify_one();
    }
}

impl Blindable {
    /// A store whose writes spend request ids into `ledger`, the one its server
    /// reads, as every other story's store does.
    pub(super) fn new(ledger: Arc<InMemoryRequests>) -> Self {
        Blindable {
            inner: Arc::new(InMemoryMemory::booted().with_requests(ledger)),
            instance_blind: std::sync::atomic::AtomicBool::new(false),
            blind: std::sync::atomic::AtomicBool::new(false),
            corpus_blind: std::sync::atomic::AtomicBool::new(false),
            unstamped: std::sync::atomic::AtomicBool::new(false),
            document_gate: std::sync::Mutex::new(None),
        }
    }

    /// **The next read of one document waits until the gate is released.** The
    /// read is the one the image makes after a write, so the write has landed in
    /// the store and is not yet in the image.
    pub fn holds_the_next_document_read(&self) -> Arc<DocumentGate> {
        let gate = Arc::new(DocumentGate {
            reached: tokio::sync::Notify::new(),
            released: tokio::sync::Notify::new(),
        });
        *self.document_gate.lock().expect("the gate is not poisoned") = Some(gate.clone());
        gate
    }

    /// From now on every write reads back with no moment of its own.
    pub fn forgets_when_its_writes_happened(&self) {
        self.unstamped
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// The store stops answering the reads that fill the index, the
    /// whole-corpus read and the one-document read alike.
    pub fn goes_away(&self) {
        self.blind.store(true, std::sync::atomic::Ordering::Release);
    }

    /// Only the instance settings read fails; the stored writes still land.
    pub fn the_instance_fields_cannot_be_read(&self) {
        self.instance_blind
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// **Only the whole-corpus read stops answering.** A read of one document
    /// still does, as the real store reads it by its own query.
    pub fn the_corpus_read_fails(&self) {
        self.corpus_blind
            .store(true, std::sync::atomic::Ordering::Release);
    }

    /// …and comes back, from either.
    pub fn comes_back(&self) {
        self.instance_blind
            .store(false, std::sync::atomic::Ordering::Release);
        self.blind
            .store(false, std::sync::atomic::Ordering::Release);
        self.corpus_blind
            .store(false, std::sync::atomic::Ordering::Release);
    }
}

#[async_trait::async_trait]
impl jojobot_domain::memory::Memory for Blindable {
    async fn former_handles(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::FormerHandle>, jojobot_domain::memory::MemoryError>
    {
        self.inner.former_handles().await
    }

    async fn scan(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::search::DocScan>, jojobot_domain::memory::MemoryError>
    {
        if self.blind.load(std::sync::atomic::Ordering::Acquire)
            || self.corpus_blind.load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(jojobot_domain::memory::MemoryError::Store(
                "the store cannot be read".into(),
            ));
        }
        self.inner.scan().await
    }

    /// **One document is read by its own query, as the real store reads it**, so
    /// a store whose whole-corpus read fails still answers this one. A store that
    /// has gone away does not.
    async fn scan_entity(
        &self,
        entity: &jojobot_domain::memory::EntityId,
    ) -> Result<Option<jojobot_domain::memory::search::DocScan>, jojobot_domain::memory::MemoryError>
    {
        if self.blind.load(std::sync::atomic::Ordering::Acquire) {
            return Err(jojobot_domain::memory::MemoryError::Store(
                "the store cannot be read".into(),
            ));
        }
        let gate = self
            .document_gate
            .lock()
            .expect("the gate is not poisoned")
            .take();
        if let Some(gate) = gate {
            gate.reached.notify_one();
            gate.released.notified().await;
        }
        self.inner.scan_entity(entity).await
    }

    async fn add_entity(
        &self,
        new: jojobot_domain::memory::NewEntity,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.add_entity(new).await
    }

    async fn list_entities(
        &self,
        kind: Option<jojobot_domain::memory::EntityKind>,
    ) -> Result<Vec<jojobot_domain::memory::Entity>, jojobot_domain::memory::MemoryError> {
        self.inner.list_entities(kind).await
    }

    async fn declare_type(
        &self,
        declared: jojobot_domain::memory::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<jojobot_domain::memory::types::DeclaredType, jojobot_domain::memory::MemoryError>
    {
        self.inner.declare_type(declared, by_session).await
    }

    async fn declared_types(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::types::DeclaredType>, jojobot_domain::memory::MemoryError>
    {
        self.inner.declared_types().await
    }

    async fn declare_kind(
        &self,
        token: &str,
        origin: jojobot_domain::memory::types::Origin,
        fields: Vec<jojobot_domain::memory::types::Field>,
    ) -> Result<bool, jojobot_domain::memory::MemoryError> {
        self.inner.declare_kind(token, origin, fields).await
    }

    async fn declared_kinds(
        &self,
    ) -> Result<
        Vec<(String, jojobot_domain::memory::types::Origin)>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.declared_kinds().await
    }

    async fn reclaim_kind(&self, token: &str) -> Result<(), jojobot_domain::memory::MemoryError> {
        self.inner.reclaim_kind(token).await
    }

    async fn update_entity(
        &self,
        handle: &jojobot_domain::memory::EntityId,
        patch: jojobot_domain::memory::EntityPatch,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.update_entity(handle, patch).await
    }
    async fn add_entity_with_first_claim(
        &self,
        new: jojobot_domain::memory::NewEntity,
        first: jojobot_domain::memory::NewFact,
    ) -> Result<
        jojobot_domain::memory::Guarded<(
            jojobot_domain::memory::Entity,
            jojobot_domain::memory::Fact,
        )>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.add_entity_with_first_claim(new, first).await
    }
    async fn update_entity_with_claim(
        &self,
        handle: &jojobot_domain::memory::EntityId,
        patch: jojobot_domain::memory::EntityPatch,
        claim: jojobot_domain::memory::NewFact,
    ) -> Result<
        jojobot_domain::memory::Guarded<(
            jojobot_domain::memory::Entity,
            jojobot_domain::memory::Fact,
        )>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner
            .update_entity_with_claim(handle, patch, claim)
            .await
    }
    async fn rename_entity(
        &self,
        from: &jojobot_domain::memory::EntityId,
        to: &jojobot_domain::memory::EntityId,
        parent: Option<jojobot_domain::memory::EntityId>,
        date: jiff::civil::Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Entity>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn archive_entity(
        &self,
        id: &jojobot_domain::memory::EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Entity, jojobot_domain::memory::MemoryError> {
        self.inner.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &jojobot_domain::memory::EntityId,
        session: Option<&str>,
    ) -> Result<
        (
            jojobot_domain::memory::Entity,
            jojobot_domain::memory::Archived,
        ),
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.restore_entity(id, session).await
    }

    async fn capture(
        &self,
        fact: jojobot_domain::memory::NewFact,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.capture(fact).await
    }

    async fn recall(
        &self,
        subject: &jojobot_domain::memory::EntityId,
    ) -> Result<Vec<jojobot_domain::memory::Fact>, jojobot_domain::memory::MemoryError> {
        self.inner.recall(subject).await
    }

    async fn history(
        &self,
        entity: &jojobot_domain::memory::EntityId,
        key: &str,
    ) -> Result<Vec<jojobot_domain::memory::FieldWrite>, jojobot_domain::memory::MemoryError> {
        self.inner.history(entity, key).await
    }

    async fn key_trails(
        &self,
        entity: &jojobot_domain::memory::EntityId,
    ) -> Result<jojobot_domain::memory::KeyTrails, jojobot_domain::memory::MemoryError> {
        let mut trails = self.inner.key_trails(entity).await?;
        if self.unstamped.load(std::sync::atomic::Ordering::Acquire) {
            for write in &mut trails.record_writes {
                write.written_at = None;
            }
        }
        Ok(trails)
    }

    async fn claim_history(
        &self,
        address: &jojobot_domain::memory::FactAddress,
    ) -> Result<Vec<jojobot_domain::memory::ClaimWrite>, jojobot_domain::memory::MemoryError> {
        let mut writes = self.inner.claim_history(address).await?;
        if self.unstamped.load(std::sync::atomic::Ordering::Acquire) {
            for write in &mut writes {
                write.written_at = None;
            }
        }
        Ok(writes)
    }

    async fn keys_holding(
        &self,
        key: &str,
        test: &jojobot_domain::memory::filter::Test,
        kind: Option<jojobot_domain::memory::EntityKind>,
    ) -> Result<Vec<jojobot_domain::memory::EntityId>, jojobot_domain::memory::MemoryError> {
        self.inner.keys_holding(key, test, kind).await
    }

    async fn fields(
        &self,
        entity: &jojobot_domain::memory::EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, jojobot_domain::memory::MemoryError>
    {
        if entity.as_str() == "topic:instance"
            && self
                .instance_blind
                .load(std::sync::atomic::Ordering::Acquire)
        {
            return Err(jojobot_domain::memory::MemoryError::Store(
                "the instance settings cannot be read".into(),
            ));
        }
        self.inner.fields(entity).await
    }

    async fn update_fact(
        &self,
        address: &jojobot_domain::memory::FactAddress,
        patch: jojobot_domain::memory::FactPatch,
        caller: &jojobot_domain::memory::EntityId,
    ) -> Result<
        jojobot_domain::memory::Guarded<jojobot_domain::memory::Fact>,
        jojobot_domain::memory::MemoryError,
    > {
        self.inner.update_fact(address, patch, caller).await
    }

    async fn retract(
        &self,
        address: &jojobot_domain::memory::FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &jojobot_domain::memory::EntityId,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Retraction, jojobot_domain::memory::MemoryError> {
        self.inner
            .retract(address, reason, date, caller, session)
            .await
    }

    async fn merge(
        &self,
        folded: &jojobot_domain::memory::EntityId,
        survivor: &jojobot_domain::memory::EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &jojobot_domain::memory::EntityId,
        session: Option<&str>,
    ) -> Result<jojobot_domain::memory::Merge, jojobot_domain::memory::MemoryError> {
        self.inner
            .merge(folded, survivor, reason, date, caller, session)
            .await
    }

    async fn set_prose(
        &self,
        entity: &jojobot_domain::memory::EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<String, jojobot_domain::memory::MemoryError> {
        self.inner.set_prose(entity, prose, session).await
    }
}
