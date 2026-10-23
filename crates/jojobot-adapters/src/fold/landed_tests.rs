//! **A write that landed is answered as landed when only the fold's own re-read
//! could not run** (rule 130), over a store that lets the write through and
//! fails the read after it.

use super::*;
use jiff::civil::date;
use jojobot_domain::memory::testing::InMemoryMemory;

fn alpha() -> EntityId {
    EntityId::person("person:alpha")
}

async fn seeded(store: &Folded, id: &EntityId, name: &str) {
    store
        .add_entity(NewEntity::new(id.clone(), name, "user-named"))
        .await
        .expect("add ok")
        .written()
        .expect("not blocked");
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
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
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
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: Date,
        resemblance: jojobot_domain::memory::Override<'_>,
        session: Option<&str>,
    ) -> Result<Guarded<Entity>, MemoryError> {
        self.0
            .rename_entity(from, to, parent, date, resemblance, session)
            .await
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        self.0.capture(fact).await
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        self.0.recall(subject).await
    }
    async fn history(&self, entity: &EntityId, key: &str) -> Result<Vec<FieldWrite>, MemoryError> {
        self.0.history(entity, key).await
    }
    async fn claim_history(&self, address: &FactAddress) -> Result<Vec<ClaimWrite>, MemoryError> {
        self.0.claim_history(address).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        self.0.update_fact(address, patch, caller).await
    }
    /// **The one method this double exists to break.** Always fails,
    /// whatever the write just did — this is what `Folded::refresh` calls.
    async fn fields(&self, _entity: &EntityId) -> Result<BTreeMap<String, String>, MemoryError> {
        Err(MemoryError::Store(
            "read failed right after the write landed".into(),
        ))
    }
    async fn retract(
        &self,
        address: &FactAddress,
        reason: Option<&str>,
        date: Date,
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
        date: Date,
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
    async fn scan(&self) -> Result<Vec<DocScan>, MemoryError> {
        self.0.scan().await
    }
    async fn declare_type(
        &self,
        declared: DeclaredType,
        by_session: Option<&str>,
    ) -> Result<DeclaredType, MemoryError> {
        self.0.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<DeclaredType>, MemoryError> {
        self.0.declared_types().await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: types::Origin,
        fields: Vec<types::Field>,
    ) -> Result<bool, MemoryError> {
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
    folded.rebuild().await.expect("an empty store fills");
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

/// **A declaration that landed is never answered as a failure, whatever the
/// fold's re-read did** (rule 130). Each of the three verbs below changes how
/// every entity folds, so each reads the whole image again after it lands; the
/// double lets the write through and fails that read. The answer is success,
/// the store holds the declaration, and the image is gone so reads go to the
/// store (see [`Folded::follow`]). Paired with the store read in the same
/// case, so an answer of success cannot be a write that never landed.
async fn a_fold_whose_reread_fails_after_a_declaration() -> (Arc<InMemoryMemory>, Folded) {
    let inner = Arc::new(InMemoryMemory::booted());
    let folded = Folded::new(Arc::new(WritesThenFailsToReadFields(inner.clone())));
    folded.rebuild().await.expect("an empty store fills");
    seeded(&folded, &alpha(), "Alpha").await;
    (inner, folded)
}

#[tokio::test]
async fn a_type_declared_while_the_fold_cannot_reread_answers_landed_not_failed() {
    let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
    let declared = folded
        .declare_type(
            types::DeclaredType::new(
                "fold-behind-type",
                vec![types::Field::new("laps", types::ValueType::Text)],
            ),
            None,
        )
        .await
        .expect("a declaration that landed is not a failure");
    assert_eq!(declared.name, "fold-behind-type");
    let held = inner.declared_types().await.expect("types read");
    assert!(
        held.iter().any(|t| t.name == "fold-behind-type"),
        "the declaration is in the store: {held:?}"
    );
}

#[tokio::test]
async fn a_kind_declared_while_the_fold_cannot_reread_answers_landed_not_failed() {
    let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
    folded
        .declare_kind("theta", types::Origin::Declared, Vec::new())
        .await
        .expect("a declaration that landed is not a failure");
    let held = inner.declared_kinds().await.expect("kinds read");
    assert!(
        held.iter().any(|(token, _)| token == "theta"),
        "the kind is in the store: {held:?}"
    );
}

#[tokio::test]
async fn a_kind_reclaimed_while_the_fold_cannot_reread_answers_landed_not_failed() {
    let (inner, folded) = a_fold_whose_reread_fails_after_a_declaration().await;
    inner
        .declare_kind("theta", types::Origin::Shipped, Vec::new())
        .await
        .expect("the kind is shipped into the store");
    folded
        .reclaim_kind("theta")
        .await
        .expect("a reclaim that landed is not a failure");
    let held = inner.declared_kinds().await.expect("kinds read");
    assert!(
        held.iter().all(|(token, _)| token != "theta"),
        "the kind is gone from the store: {held:?}"
    );
}
