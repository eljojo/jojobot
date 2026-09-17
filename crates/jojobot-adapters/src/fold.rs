//! [`Folded`] — a live cache of what every thing holds, kept above the store
//! rather than inside it (decision log 292).
//!
//! `Memory::fields` folds a thing's field writes down to one value per key —
//! the newest write wins, an older one stands back up when the write that
//! beat it is retracted. The fold is correct wherever it runs; the cost is
//! calling it once per thing a candidate-picking read has to consider, and
//! against the real store that is a transaction per call. [`Folded`] holds
//! the same answer in RAM instead, built once from a full read at boot and
//! kept current by re-reading the one thing a write just touched — so a
//! caller who already has a `Folded` in hand pays no store round trip for a
//! question the cache already answers, while every other verb, and every
//! store this does not wrap, is untouched.
//!
//! ⚠️ **This is only correct with exactly one writer in the process.** The
//! cache is filled from writes THIS `Folded` observed; a row changed by
//! anything else — another worker, a restored backup, a migration tool, a
//! person running SQL by hand — leaves the cached copy silently wrong, and
//! nothing here would notice. jojobot runs one application worker against
//! its store today, which is the whole reason this is authoritative rather
//! than a guess. Add a second writer and this assumption breaks first.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use jiff::civil::Date;

use jojobot_domain::memory::{
    ClaimWrite, Entity, EntityId, EntityKind, EntityPatch, Fact, FactAddress, FactPatch,
    FieldWrite, FormerHandle, Guarded, Landed, Memory, MemoryError, Merge, NewEntity, NewFact,
    Retraction, WriteSummary,
    search::{self, DocScan},
    types::{self, DeclaredType},
};

/// A [`Memory`] wrapped in a RAM copy of every thing's folded fields.
///
/// Every verb but the four below (`capture`, `update_fact`, `retract`,
/// `merge` — the only ones that can move a field, see [`Memory::fields`]'s
/// own doc) forwards straight to the store underneath and touches nothing
/// here.
/// One entity's folded fields, beside the version
/// [`Memory::fields_versioned`] read them under.
type VersionedFields = (u64, BTreeMap<String, String>);

pub struct Folded {
    inner: Arc<dyn Memory>,
    /// **Each entry carries the version [`Memory::fields_versioned`] read it
    /// under, beside the fields themselves.** The version is what
    /// [`refresh`](Folded::refresh) compares against before installing a new
    /// read, so two concurrent refreshes for the same entity cannot have the
    /// slower one — carrying an earlier version — land after the faster one
    /// and overwrite what it correctly installed.
    cache: RwLock<HashMap<EntityId, VersionedFields>>,
}

impl Folded {
    /// Wrap a store with an **empty** fold. Nothing is read yet — the cache
    /// is filled by [`rebuild`](Folded::rebuild), separately, so a store
    /// that is not reachable yet cannot stop the wrap itself from
    /// succeeding. `fields` answers correctly even before a rebuild: a miss
    /// falls through to the store underneath.
    pub fn new(inner: Arc<dyn Memory>) -> Self {
        Folded {
            inner,
            cache: RwLock::new(HashMap::new()),
        }
    }

    /// Fill the fold from a full read of the store — the boot path. Returns
    /// how many things were folded.
    pub async fn rebuild(&self) -> Result<usize, MemoryError> {
        let entities = self.inner.list_entities(None).await?;
        let mut fresh = HashMap::with_capacity(entities.len());
        for entity in &entities {
            let (fields, version) = self.inner.fields_versioned(&entity.id).await?;
            fresh.insert(entity.id.clone(), (version, fields));
        }
        let folded = fresh.len();
        *self.cache.write().expect("fold lock") = fresh;
        Ok(folded)
    }

    /// [`rebuild`](Folded::rebuild), for a caller that already knows what it
    /// wrote and only needs the fold to catch up — never silently. A rebuild
    /// that fails here is reported rather than swallowed: the write already
    /// landed in the store, so this is the one thing left that can still go
    /// wrong.
    async fn rebuild_or_explain(&self, verb: &str) -> Result<(), MemoryError> {
        self.rebuild().await.map(|_| ()).map_err(|source| {
            MemoryError::Store(format!(
                "{verb} landed, but the fold could not rebuild to reflect it: {source}"
            ))
        })
    }

    /// Re-read one thing's fields from the store underneath and replace what
    /// the cache holds for it — the write-side half of the same mechanism
    /// [`rebuild`](Folded::rebuild) runs at boot. **Re-reading rather than
    /// computing the new value from the write just made**: `fields` is where
    /// [`Memory::fields`]'s newest-write-wins, per-key, retraction-aware fold
    /// already lives, so this asks it again instead of a second copy of the
    /// same arithmetic learning to disagree with it.
    ///
    /// **The read happens before the lock is ever taken**, so a slow read for
    /// one entity blocks nobody's read of another and nobody's read of this
    /// one either — only the two lines below, deciding whether to install,
    /// hold it. A read carrying a version behind what is already installed
    /// is refused rather than applied: the write it reflects still happened,
    /// exactly as fresh a moment ago as the one that beat it here, but a
    /// cache can hold only one answer and the newer one is that answer.
    async fn refresh(&self, entity: &EntityId) -> Result<(), MemoryError> {
        let (fields, version) = self.inner.fields_versioned(entity).await?;
        let mut cache = self.cache.write().expect("fold lock");
        let current = cache.get(entity).map(|(v, _)| *v).unwrap_or(0);
        if version >= current {
            cache.insert(entity.clone(), (version, fields));
        }
        Ok(())
    }
}

/// **The write already landed; only the re-read after it failed.** Every
/// write method below reaches for this instead of propagating `source`
/// straight through `?`, which would tell the caller the write itself had
/// failed — false, since `landed` is only ever built from what the inner
/// store already returned as written.
fn fold_behind(landed: Landed, source: MemoryError) -> MemoryError {
    MemoryError::FoldBehind {
        landed,
        behind: search::Behind::Stale,
        source: Box::new(source),
    }
}

#[async_trait]
impl Memory for Folded {
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

    /// **The cache is keyed by the literal handle, and `fields` checks no
    /// freshness on a hit.** The store's own `fields` resolves a stale
    /// handle through its rename history and answers current data (see
    /// [`Memory::fields`]'s own adapters); this cache would go on answering
    /// whatever it held under the OLD handle at the moment of the rename,
    /// for ever, since nothing ever writes a field under that handle again
    /// to trigger an ordinary refresh. So a successful rename drops the old
    /// handle's entry: the next read under it is a cache miss, which falls
    /// through to the store and gets the current answer, exactly as a miss
    /// always has.
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        override_token: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        let renamed = self
            .inner
            .rename_entity(from, to, parent, date, override_token)
            .await?;
        if matches!(renamed, Guarded::Written(_)) {
            self.cache.write().expect("fold lock").remove(from);
        }
        Ok(renamed)
    }

    async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
        self.inner.archive_entity(id, reason).await
    }

    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        match self.inner.capture(fact).await? {
            Guarded::Written(fact) => match self.refresh(&fact.home).await {
                Ok(()) => Ok(Guarded::Written(fact)),
                Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
            },
            blocked => Ok(blocked),
        }
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
    ) -> Result<Guarded<Fact>, MemoryError> {
        match self.inner.update_fact(address, patch).await? {
            Guarded::Written(fact) => match self.refresh(&fact.home).await {
                Ok(()) => Ok(Guarded::Written(fact)),
                Err(source) => Err(fold_behind(Landed::Fact(Box::new(fact)), source)),
            },
            blocked => Ok(blocked),
        }
    }

    /// **The one method this wrapper exists for.** A thing the fold has
    /// answered before is served from RAM; a miss — nothing has been written
    /// through this `Folded` for it, and no `rebuild` has run since it was
    /// created — falls through to the store, exactly what an unwrapped
    /// `Memory` would answer.
    async fn fields(&self, entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        if let Some((_, fields)) = self.cache.read().expect("fold lock").get(entity) {
            return Ok(fields.clone());
        }
        self.inner.fields(entity).await
    }

    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Retraction, MemoryError> {
        let taken_back = self.inner.retract(address, reason, date).await?;
        match self.refresh(&taken_back.retracted.home).await {
            Ok(()) => Ok(taken_back),
            Err(source) => Err(fold_behind(
                Landed::Retraction(Box::new(taken_back)),
                source,
            )),
        }
    }

    /// **Both sides refreshed.** The folded thing's writes moved to the
    /// survivor (see [`Memory::merge`]), so its own cached fields may have
    /// shrunk and the survivor's may have grown; serving either from a stale
    /// entry would answer with a thing that is no longer whole.
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: Date,
    ) -> Result<Merge, MemoryError> {
        let done = self.inner.merge(folded, survivor, reason, date).await?;
        // **Both sides attempted regardless of whether the first fails.** The
        // merge already landed on both; a caller told only the folded side is
        // behind while the survivor's own refresh never even ran would learn
        // that the hard way on its next stale read.
        let folded_refreshed = self.refresh(folded).await;
        let survivor_refreshed = self.refresh(survivor).await;
        match (folded_refreshed, survivor_refreshed) {
            (Ok(()), Ok(())) => Ok(done),
            (Err(source), _) | (Ok(()), Err(source)) => {
                Err(fold_behind(Landed::Merge(Box::new(done)), source))
            }
        }
    }

    async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
        self.inner.set_prose(entity, prose).await
    }

    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.inner.scan().await
    }

    async fn write_summary(&self) -> Result<Option<WriteSummary>, MemoryError> {
        self.inner.write_summary().await
    }

    /// **A declaration can change how EVERY entity's existing writes fold**
    /// — `newest` against a counter — so there is no one handle to refresh
    /// the way a capture refreshes its own home. The whole cache is rebuilt
    /// from the store instead, the same read [`rebuild`](Folded::rebuild)
    /// runs at boot. The write already landed; only the rebuild is what
    /// this can fail to report.
    async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
        let written = self.inner.declare_type(declared).await?;
        self.rebuild_or_explain("declare_type").await?;
        Ok(written)
    }

    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.inner.declared_types().await
    }

    /// **The same reason [`declare_type`](Self::declare_type) rebuilds
    /// rather than refreshes**: a kind's keys fold exactly like a declared
    /// type's, over every entity that answers to it.
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<(), MemoryError> {
        self.inner.declare_kind(token, origin, fields).await?;
        self.rebuild_or_explain("declare_kind").await
    }

    async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
        self.inner.declared_kinds().await
    }

    /// **Taking a kind back changes folding too**: a key that summed while
    /// the kind held it falls back to newest-write-wins once reclaimed, for
    /// every entity that answered to it.
    async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
        self.inner.reclaim_kind(token).await?;
        self.rebuild_or_explain("reclaim_kind").await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::civil::date;
    use jojobot_domain::memory::testing::{InMemoryMemory, contract};

    fn alpha() -> EntityId {
        EntityId::person("person:alpha")
    }

    fn beta() -> EntityId {
        EntityId::person("person:beta")
    }

    async fn seeded(store: &Folded, id: &EntityId, name: &str) {
        store
            .add_entity(NewEntity::new(id.clone(), name, "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
    }

    /// **The boot path.** A store already holding a write before `Folded`
    /// ever wraps it is folded correctly once `rebuild` runs — the cache is
    /// not only ever filled by write-through.
    #[tokio::test]
    async fn rebuild_folds_what_the_store_already_held() {
        let inner = Arc::new(InMemoryMemory::booted());
        inner
            .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");
        inner
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(
                    alpha(),
                    "was here before the fold wrapped it",
                    date(2026, 9, 1),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        let folded = Folded::new(inner);
        let before_rebuild = folded.fields(&alpha()).await.expect("fields ok");
        assert_eq!(
            before_rebuild.get("due_on").map(String::as_str),
            Some("2026-09-14"),
            "a cache miss falls through to the store, so this reads right even before rebuild"
        );

        let folded_count = folded.rebuild().await.expect("rebuild ok");
        assert_eq!(folded_count, 1, "one entity to fold");
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
    }

    /// **Write-through keeps the fold current, and reordering after a
    /// retraction is where a fold built from arithmetic instead of a re-read
    /// would go wrong.** Two writes name the same key; retracting the newer
    /// one must bring the older, still-active write back — not merely blank
    /// the key, and not still show the retracted value.
    #[tokio::test]
    async fn capture_and_retract_keep_the_fold_equal_to_the_store() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        let first = folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "first due date", date(2026, 9, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the newest write wins, positive case"
        );

        let second = folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
                ..NewFact::about(alpha(), "moved to next week", date(2026, 9, 8))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-21".to_string()),
            "the second write is now newest"
        );
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );

        folded
            .retract(
                &FactAddress {
                    home: alpha(),
                    local: second.id.clone(),
                },
                Some("moved back"),
                date(2026, 9, 9),
            )
            .await
            .expect("retract ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "retracting the newer write must bring the older active one back, not blank the key"
        );
        assert_eq!(
            folded.fields(&alpha()).await.expect("fields ok"),
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
        let _ = first;
    }

    /// **A merge moves fields, not only claims — the fold has to follow to
    /// both sides.** The folded thing's key must stop reading and the
    /// survivor's must start.
    #[tokio::test]
    async fn merge_moves_the_fold_to_the_survivor() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;
        seeded(&folded, &beta(), "Beta").await;

        folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "alpha's own due date", date(2026, 9, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        folded
            .merge(&alpha(), &beta(), Some("same thing"), date(2026, 9, 10))
            .await
            .expect("merge ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            None,
            "the folded handle's own fields must not still show what moved, negative case"
        );
        assert_eq!(
            folded
                .fields(&beta())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the survivor must now hold what moved, positive case"
        );
        assert_eq!(
            folded.fields(&beta()).await.expect("fields ok"),
            folded
                .inner
                .fields(&beta())
                .await
                .expect("scanned fields ok"),
            "the folded answer must equal the scanned answer"
        );
    }

    /// **`fields` answers from the cache, not by re-asking the store every
    /// time.** A write made straight to the wrapped store — bypassing
    /// `Folded`, the shape a second writer would take — is invisible to a
    /// `fields` call that hits the cache, exactly the load-bearing
    /// single-writer assumption this module's doc warns about. Without this
    /// case, `fields` falling straight through to the store on every call —
    /// correct, but pointless — would look identical to every other test
    /// here passing.
    #[tokio::test]
    async fn fields_are_served_from_the_cache_not_rederived_on_every_call() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;
        folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-14".to_string())]),
                ..NewFact::about(alpha(), "cached", date(2026, 9, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        // Straight to the wrapped store, never through `Folded::capture` —
        // the cache is never told this happened.
        folded
            .inner
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "2026-09-21".to_string())]),
                ..NewFact::about(alpha(), "written around the fold", date(2026, 9, 8))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .inner
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-21".to_string()),
            "the store itself already sees the bypassed write"
        );
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"2026-09-14".to_string()),
            "the fold still answers with what it cached, not what the store now holds"
        );
    }

    /// **The behavioural contract holds wrapped in a fold, not only bare.**
    /// `Folded` forwards every guard, every read and every write it does not
    /// itself refresh straight to the store underneath, so the whole existing
    /// suite is the regression test for that claim — a decorator that changed
    /// what any of it answers would fail here, with no case written for the
    /// fold itself.
    #[tokio::test]
    async fn fake_satisfies_the_contract_when_folded() {
        contract::run_all(&Folded::new(Arc::new(InMemoryMemory::booted()))).await;
    }

    /// **A store whose WRITE succeeds and whose immediate re-read fails** —
    /// exactly what [`Folded::refresh`] sees when the inner write lands and
    /// the re-read after it cannot run. Every other fake in this crate fails
    /// everything or fails nothing, so this branch of `Folded` had never run
    /// before this double existed.
    struct WritesThenFailsToReadFields(Arc<InMemoryMemory>);

    #[async_trait]
    impl Memory for WritesThenFailsToReadFields {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.0.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
            self.0.list_entities(kind).await
        }
        async fn former_handles(&self) -> Result<Vec<FormerHandle>, MemoryError> {
            self.0.former_handles().await
        }
        async fn update_entity(
            &self,
            handle: &EntityId,
            patch: EntityPatch,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.0.update_entity(handle, patch).await
        }
        async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
            self.0.archive_entity(id, reason).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            override_token: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.0
                .rename_entity(from, to, parent, date, override_token)
                .await
        }
        async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
            self.0.capture(fact).await
        }
        async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
            self.0.recall(subject).await
        }
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.0.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.0.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
        ) -> Result<Guarded<Fact>, MemoryError> {
            self.0.update_fact(address, patch).await
        }
        /// **The one method this double exists to break.** Always fails,
        /// whatever the write just did — this is what `Folded::refresh` calls.
        async fn fields(
            &self,
            _entity: &EntityId,
        ) -> Result<BTreeMap<String, String>, MemoryError> {
            Err(MemoryError::Store(
                "read failed right after the write landed".into(),
            ))
        }
        async fn retract(
            &self,
            address: &FactAddress,
            reason: Option<&str>,
            date: Date,
        ) -> Result<Retraction, MemoryError> {
            self.0.retract(address, reason, date).await
        }
        async fn merge(
            &self,
            folded: &EntityId,
            survivor: &EntityId,
            reason: Option<&str>,
            date: Date,
        ) -> Result<Merge, MemoryError> {
            self.0.merge(folded, survivor, reason, date).await
        }
        async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
            self.0.set_prose(entity, prose).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.0.scan().await
        }
        async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
            self.0.declare_type(declared).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.0.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<(), MemoryError> {
            self.0.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.0.declared_kinds().await
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.0.reclaim_kind(token).await
        }
    }

    /// **The write says it landed, never that it failed, when only the fold's
    /// own re-read could not run.** Before this fix, `capture` propagated the
    /// refresh's error straight out via `?`, so a caller of a write that
    /// genuinely succeeded was told it had not.
    #[tokio::test]
    async fn a_write_that_lands_and_a_refresh_that_fails_answers_landed_not_failed() {
        let inner = Arc::new(InMemoryMemory::booted());
        let flaky = Arc::new(WritesThenFailsToReadFields(inner.clone()));
        let folded = Folded::new(flaky);
        seeded(&folded, &alpha(), "Alpha").await;

        let err = folded
            .capture(NewFact::about(
                alpha(),
                "captured while the fold could not refresh",
                date(2026, 9, 14),
            ))
            .await
            .expect_err("the inner refresh always fails in this double, so this cannot be Ok");

        let MemoryError::FoldBehind { landed, behind, .. } = err else {
            panic!("a write that landed must answer FoldBehind, never a bare error: {err:?}");
        };
        assert_eq!(
            behind,
            search::Behind::Stale,
            "a write-path fold gap is always Stale, never Unscanned"
        );
        let Landed::Fact(fact) = landed else {
            panic!("capture's landed payload is a Fact");
        };
        assert_eq!(fact.content, "captured while the fold could not refresh");

        // **The positive half.** Without this, a write that never happened
        // would look identical to one that did — both come back as
        // `FoldBehind`, and only reading the store proves the row is real.
        let recalled = inner.recall(&alpha()).await.expect("recall ok");
        assert!(
            recalled.iter().any(|f| f.id == fact.id),
            "the write must be visible in the store even though the fold refresh failed: \
             {recalled:?}",
        );
    }

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
    /// interleaving [`Folded::refresh`]'s version guard exists for.
    struct StallFirstVersionedRead {
        inner: Arc<InMemoryMemory>,
        written: std::sync::atomic::AtomicU64,
        stalled: Arc<tokio::sync::Notify>,
        released: Arc<tokio::sync::Notify>,
        first: std::sync::atomic::AtomicBool,
    }

    #[async_trait]
    impl Memory for StallFirstVersionedRead {
        async fn add_entity(&self, new: NewEntity) -> Result<Guarded<Entity>, MemoryError> {
            self.inner.add_entity(new).await
        }
        async fn list_entities(
            &self,
            kind: Option<EntityKind>,
        ) -> Result<Vec<Entity>, MemoryError> {
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
        async fn archive_entity(&self, id: &EntityId, reason: &str) -> Result<Entity, MemoryError> {
            self.inner.archive_entity(id, reason).await
        }
        async fn rename_entity(
            &self,
            from: &EntityId,
            to: &EntityId,
            parent: Option<EntityId>,
            date: Date,
            override_token: Option<&str>,
        ) -> Result<Guarded<Entity>, MemoryError> {
            self.inner
                .rename_entity(from, to, parent, date, override_token)
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
        async fn history(
            &self,
            entity: &EntityId,
            key: &str,
        ) -> Result<Vec<FieldWrite>, MemoryError> {
            self.inner.history(entity, key).await
        }
        async fn claim_history(
            &self,
            address: &FactAddress,
        ) -> Result<Vec<ClaimWrite>, MemoryError> {
            self.inner.claim_history(address).await
        }
        async fn update_fact(
            &self,
            address: &FactAddress,
            patch: FactPatch,
        ) -> Result<Guarded<Fact>, MemoryError> {
            let written = self.inner.update_fact(address, patch).await?;
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
        ) -> Result<Retraction, MemoryError> {
            let retracted = self.inner.retract(address, reason, date).await?;
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
        ) -> Result<Merge, MemoryError> {
            self.inner.merge(folded, survivor, reason, date).await
        }
        async fn set_prose(&self, entity: &EntityId, prose: &str) -> Result<String, MemoryError> {
            self.inner.set_prose(entity, prose).await
        }
        async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
            self.inner.scan().await
        }
        async fn declare_type(&self, declared: DeclaredType) -> Result<DeclaredType, MemoryError> {
            self.inner.declare_type(declared).await
        }
        async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
            self.inner.declared_types().await
        }
        async fn declare_kind(
            &self,
            token: &str,
            origin: types::Origin,
            fields: Vec<types::Field>,
        ) -> Result<(), MemoryError> {
            self.inner.declare_kind(token, origin, fields).await
        }
        async fn declared_kinds(&self) -> Result<Vec<(String, types::Origin)>, MemoryError> {
            self.inner.declared_kinds().await
        }
        async fn reclaim_kind(&self, token: &str) -> Result<(), MemoryError> {
            self.inner.reclaim_kind(token).await
        }
    }

    /// **The race itself, driven rather than merely asserted possible.**
    ///
    /// A writes `due_on = A`; its refresh takes an early snapshot (version 1,
    /// `due_on = A`) and stalls. While it is stalled, B writes `due_on = B`
    /// end to end — its own refresh is not the first call, so it runs
    /// straight through, installing version 2. Only then is A's stalled
    /// snapshot released to attempt its own install, carrying version 1.
    ///
    /// Without the version guard, A's install runs unconditionally and the
    /// fold ends up serving `due_on = A` — the earlier value — even though
    /// the store itself has moved on to `B` and nothing will touch this
    /// entity again to correct it. Both writes succeeded; only which one the
    /// CACHE remembers is what a version compares.
    #[tokio::test]
    async fn a_slower_refresh_carrying_an_earlier_version_does_not_undo_a_faster_ones_install() {
        let inner = Arc::new(InMemoryMemory::booted());
        inner
            .add_entity(NewEntity::new(alpha(), "Alpha", "user-named"))
            .await
            .expect("add ok")
            .written()
            .expect("not blocked");

        let stalled = Arc::new(tokio::sync::Notify::new());
        let released = Arc::new(tokio::sync::Notify::new());
        let double = Arc::new(StallFirstVersionedRead {
            inner,
            written: std::sync::atomic::AtomicU64::new(0),
            stalled: stalled.clone(),
            released: released.clone(),
            first: std::sync::atomic::AtomicBool::new(true),
        });
        let folded = Arc::new(Folded::new(double));

        let a_folded = folded.clone();
        let a_task = tokio::spawn(async move {
            a_folded
                .capture(NewFact {
                    fields: BTreeMap::from([("due_on".to_string(), "A".to_string())]),
                    ..NewFact::about(alpha(), "A writes first", date(2026, 9, 1))
                })
                .await
        });

        // A's write has landed and its refresh is holding an early snapshot
        // open — exactly the window the bug lived in.
        stalled.notified().await;

        folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "B".to_string())]),
                ..NewFact::about(
                    alpha(),
                    "B writes second, while A is stalled",
                    date(2026, 9, 2),
                )
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"B".to_string()),
            "B's install lands correctly while A is still stalled"
        );

        // Release A's stalled, now-stale snapshot to attempt its install.
        released.notify_one();
        a_task
            .await
            .expect("the task did not panic")
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"B".to_string()),
            "a slower refresh carrying an earlier version must not undo a fresher install"
        );
    }

    /// **Declaring a fold changes what already-cached writes answer, not
    /// only writes made afterwards.** `folded_fields` interprets a thing's
    /// existing writes against the CURRENT declarations every time it runs;
    /// a cache that answered from a snapshot taken under the old
    /// declaration would go on answering wrongly for ever, since nothing
    /// ever touches this entity's own key again to trigger a normal
    /// refresh.
    ///
    /// The worked example: write, write, read (newest-write-wins, since
    /// nothing is declared), declare `laps` a counter, read again (the SAME
    /// two writes now sum).
    #[tokio::test]
    async fn declaring_a_counter_changes_what_the_same_two_writes_already_fold_to() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        folded
            .capture(NewFact {
                fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
                ..NewFact::about(alpha(), "lap one", date(2026, 9, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");
        folded
            .capture(NewFact {
                fields: BTreeMap::from([("laps".to_string(), "1".to_string())]),
                ..NewFact::about(alpha(), "lap two", date(2026, 9, 2))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("laps"),
            Some(&"1".to_string()),
            "undeclared, laps folds newest-write-wins: the second write alone",
        );

        folded
            .declare_type(types::DeclaredType::new(
                "running",
                vec![types::Field::summing("laps")],
            ))
            .await
            .expect("declare ok");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("laps"),
            Some(&"2".to_string()),
            "declared a counter, the same two writes now sum to two",
        );
    }

    /// **The store resolves a stale handle through its own rename history;
    /// the cache answered first, and answered wrong.** Read once under a
    /// handle, rename, write again under the new one, then ask by the OLD
    /// handle again: the answer must be what the thing holds NOW, not a
    /// photograph of what it held the moment before the rename — with no
    /// error and no notice, since the miss this relies on is silent by
    /// design.
    #[tokio::test]
    async fn a_rename_drops_the_old_handles_entry_so_it_answers_current_data() {
        let inner = Arc::new(InMemoryMemory::booted());
        let folded = Folded::new(inner);
        seeded(&folded, &alpha(), "Alpha").await;

        folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "before".to_string())]),
                ..NewFact::about(alpha(), "before the rename", date(2026, 9, 1))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        // Read under the old handle before the rename — the same read a
        // caller holding it would make, and what populates the cache entry
        // this fix has to drop.
        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"before".to_string()),
        );

        let gamma = EntityId::person("person:gamma");
        folded
            .rename_entity(&alpha(), &gamma, None, date(2026, 9, 2), None)
            .await
            .expect("rename ok")
            .written()
            .expect("not blocked");

        // A write under the CURRENT handle, after the rename.
        folded
            .capture(NewFact {
                fields: BTreeMap::from([("due_on".to_string(), "after".to_string())]),
                ..NewFact::about(gamma.clone(), "after the rename", date(2026, 9, 3))
            })
            .await
            .expect("capture ok")
            .written()
            .expect("not blocked");

        assert_eq!(
            folded
                .fields(&alpha())
                .await
                .expect("fields ok")
                .get("due_on"),
            Some(&"after".to_string()),
            "the old handle must resolve through history to CURRENT data, not the snapshot the \
             cache took before the rename",
        );
    }
}
