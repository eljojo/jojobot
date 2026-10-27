//! **A Memory whose one read fails and everything else works**, forwarding every
//! other port method to a real in-memory store. A method the double leaves to the
//! trait's default body would be a case that tests the default and not the store
//! (`down_guard` holds that).

use super::*;

/// A handler whose mailbox world answers nothing, over a memory the caller
/// may already have populated — a bot has to be stood up while the world is
/// up, since a claim that cannot be screened is refused.
/// A Memory with [`Down`]'s one read failing and everything else working.
pub(crate) struct DownMemory(pub(crate) Down, pub(crate) Arc<InMemoryMemory>);

#[jojobot_domain::forward(jojobot_domain::forward_memory, via = 1)]
impl Memory for DownMemory {
    fn image_field(&self, entity: &EntityId, key: &str) -> Option<Option<String>> {
        match self.0 {
            Down::ImageFields => None,
            _ => self.1.image_field(entity, key),
        }
    }

    async fn record_app_event(
        &self,
        event: jojobot_domain::app::AppEvent,
    ) -> Result<jojobot_domain::app::AppOutcome, MemoryError> {
        match self.0 {
            Down::AppEvent => Err(MemoryError::Store(
                "an app's event cannot be recorded".into(),
            )),
            _ => self.1.record_app_event(event).await,
        }
    }
    async fn list_entities(&self, kind: Option<EntityKind>) -> Result<Vec<Entity>, MemoryError> {
        match self.0 {
            Down::EntityIndex => Err(MemoryError::Store("the entity index cannot be read".into())),
            Down::ListingKind(token) if kind.is_some_and(|named| named.as_token() == token) => Err(
                MemoryError::Store(format!("the {token} listing cannot be read")),
            ),
            Down::TypeRoster
            | Down::Vocabulary
            | Down::Recall
            | Down::RecallNeverLoaded
            | Down::InstanceRecord
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_)
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::Writes => self.1.list_entities(kind).await,
        }
    }
    /// A creation that sets keys is a creation and its first claim in one act; the
    /// stand-in forwards it, so a case can make a piece of work with a status.
    async fn add_entity_with_first_claim(
        &self,
        new: NewEntity,
        first: jojobot_domain::memory::NewFact,
    ) -> Result<Guarded<(Entity, jojobot_domain::memory::Fact)>, MemoryError> {
        if let Down::ResemblerBeforeFirstClaim { handle, name } = self.0 {
            let racer = NewEntity::new(EntityId(handle.into()), name, "test");
            if let Guarded::Blocked {
                attempted,
                candidates,
            } = self.1.add_entity(racer.clone()).await?
            {
                self.1
                    .add_entity(NewEntity {
                        override_token: Some(jojobot_domain::memory::guard::override_token(
                            &attempted,
                            &candidates,
                        )),
                        override_reason: Some("a second writer of the same family".into()),
                        ..racer
                    })
                    .await?
                    .written()
                    .expect("the second writer's thing lands past its own screen");
            }
        }
        self.1.add_entity_with_first_claim(new, first).await
    }
    async fn declare_type(
        &self,
        declared: jojobot_domain::memory::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<jojobot_domain::memory::types::DeclaredType, MemoryError> {
        let landed = self.1.declare_type(declared, by_session).await;
        if let Down::TypeRosterAfterDeclare(declared) = &self.0 {
            declared.store(true, std::sync::atomic::Ordering::SeqCst);
        }
        landed
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, jojobot_domain::memory::types::Origin)>, MemoryError> {
        match self.0 {
            Down::Vocabulary => Err(MemoryError::Store(
                "the kind vocabulary cannot be read".into(),
            )),
            Down::EntityIndex
            | Down::TypeRoster
            | Down::Recall
            | Down::RecallNeverLoaded
            | Down::InstanceRecord
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_)
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::Writes => self.1.declared_kinds().await,
        }
    }

    async fn declared_types(
        &self,
    ) -> Result<Vec<jojobot_domain::memory::types::DeclaredType>, MemoryError> {
        match &self.0 {
            Down::TypeRoster => Err(MemoryError::Store("the type roster cannot be read".into())),
            Down::TypeRosterAfterDeclare(declared)
                if declared.load(std::sync::atomic::Ordering::SeqCst) =>
            {
                Err(MemoryError::Store("the type roster cannot be read".into()))
            }
            Down::Vocabulary
            | Down::EntityIndex
            | Down::Recall
            | Down::RecallNeverLoaded
            | Down::InstanceRecord
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_)
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::Writes => self.1.declared_types().await,
        }
    }
    async fn capture(&self, fact: NewFact) -> Result<Guarded<Fact>, MemoryError> {
        match self.0 {
            Down::Writes => Err(MemoryError::Store("a claim cannot be written".into())),
            Down::WritesAfter(allowed, ref written, damage)
                if written.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= allowed =>
            {
                Err(match damage {
                    Cause::Outage => MemoryError::Store("a claim cannot be written".into()),
                    Cause::Damage(what) => MemoryError::StoredDamage { what: what.into() },
                    Cause::Refusal(what) => MemoryError::Refused(what.into()),
                })
            }
            Down::EntityIndex
            | Down::TypeRoster
            | Down::Vocabulary
            | Down::Recall
            | Down::RecallNeverLoaded
            | Down::InstanceRecord
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_) => self.1.capture(fact).await,
        }
    }
    async fn recall(&self, subject: &EntityId) -> Result<Vec<Fact>, MemoryError> {
        match self.0 {
            Down::Recall => Err(MemoryError::Store("a claim cannot be read".into())),
            Down::RecallOfAfter(handle, allowed, ref reads)
                if handle == subject.as_str()
                    && reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= allowed =>
            {
                Err(MemoryError::Store("a claim cannot be read".into()))
            }
            Down::RecallNeverLoaded => Err(MemoryError::KindsNeverLoaded {
                attempted: Some(subject.to_string()),
            }),
            Down::EntityIndex
            | Down::TypeRoster
            | Down::Vocabulary
            | Down::Writes
            | Down::InstanceRecord
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_) => self.1.recall(subject).await,
        }
    }
    async fn claim_history(
        &self,
        address: &jojobot_domain::memory::FactAddress,
    ) -> Result<Vec<jojobot_domain::memory::ClaimWrite>, MemoryError> {
        if let Down::ClaimHistoryFailsAt(until) = &self.0 {
            use std::sync::atomic::Ordering;
            let left = until.load(Ordering::SeqCst);
            if left > 0 {
                until.store(left - 1, Ordering::SeqCst);
                if left == 1 {
                    return Err(MemoryError::Store(
                        "a claim's history cannot be read".into(),
                    ));
                }
            }
        }
        self.1.claim_history(address).await
    }
    async fn built_on(&self, source: &FactAddress) -> Result<Vec<Fact>, MemoryError> {
        if matches!(self.0, Down::BuiltOn) {
            panic!("what was built on {source} was read by a call that left the entries out");
        }
        self.1.built_on(source).await
    }
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<std::collections::BTreeMap<String, String>, MemoryError> {
        if matches!(self.0, Down::InstanceRecord) && entity.as_str() == "topic:instance" {
            panic!("the instance's record was read by a call that needed no zone");
        }
        if matches!(self.0, Down::ThingFields) && entity.as_str().starts_with("thing:") {
            return Err(MemoryError::Store("a thing's fields cannot be read".into()));
        }
        if matches!(self.0, Down::WorkFields) && entity.as_str().starts_with("work:") {
            return Err(MemoryError::Store("a work's fields cannot be read".into()));
        }
        if matches!(self.0, Down::FieldsOf(handle) if handle == entity.as_str()) {
            return Err(MemoryError::Store(format!(
                "the fields of {entity} cannot be read"
            )));
        }
        self.1.fields(entity).await
    }
    async fn update_fact(
        &self,
        address: &FactAddress,
        patch: FactPatch,
        caller: &EntityId,
    ) -> Result<Guarded<Fact>, MemoryError> {
        match self.0 {
            Down::Writes => Err(MemoryError::Store("a claim cannot be written".into())),
            Down::RoleRelease
                if patch.role_move.as_ref().is_some_and(|role_move| {
                    role_move.kind == jojobot_domain::session::RoleMoveKind::Release
                }) =>
            {
                Err(MemoryError::Store("a role cannot be released".into()))
            }
            Down::RoleReleaseOnly(held)
                if patch.role_move.as_ref().is_some_and(|role_move| {
                    role_move.kind == jojobot_domain::session::RoleMoveKind::Release
                        && role_move.role == held
                }) =>
            {
                Err(MemoryError::Store("a role cannot be released".into()))
            }
            Down::RoleReleaseExcept(kept_free)
                if patch.role_move.as_ref().is_some_and(|role_move| {
                    role_move.kind == jojobot_domain::session::RoleMoveKind::Release
                        && role_move.role != kept_free
                }) =>
            {
                Err(MemoryError::Store("a role cannot be released".into()))
            }
            Down::EntityIndex
            | Down::TypeRoster
            | Down::Vocabulary
            | Down::Recall
            | Down::RecallNeverLoaded
            | Down::InstanceRecord
            | Down::FieldsOf(_)
            | Down::ScanOf(_)
            | Down::RecallOfAfter(..)
            | Down::WritesAfter(..)
            | Down::ListingKind(_)
            | Down::RoleRelease
            | Down::RoleReleaseExcept(_)
            | Down::RoleReleaseOnly(_)
            | Down::ImageFields
            | Down::Nothing
            | Down::BuiltOn
            | Down::ThingFields
            | Down::WorkFields
            | Down::AppEvent
            | Down::ClaimHistoryFailsAt(_)
            | Down::ResemblerBeforeFirstClaim { .. }
            | Down::TypeRosterAfterDeclare(_) => self.1.update_fact(address, patch, caller).await,
        }
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<
        std::collections::HashMap<FactId, Vec<jojobot_domain::memory::ClaimWrite>>,
        MemoryError,
    > {
        // **Composed from this double's own reads, as the trait's default composes
        // it**, because the failure the double injects into a claim's history lives
        // in `claim_history`. Forwarding the batch read straight to the store would
        // step around it, and the case that counts that failure would never see it.
        let mut histories = std::collections::HashMap::new();
        for fact in self.recall(entity).await? {
            let chain = self.claim_history(&fact.address()).await?;
            histories.insert(fact.id, chain);
        }
        Ok(histories)
    }
    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Option<jojobot_domain::memory::search::DocScan>, MemoryError> {
        if matches!(self.0, Down::ScanOf(handle) if handle == entity.as_str()) {
            return Err(MemoryError::Store(format!(
                "the scan of {entity} cannot be read"
            )));
        }
        self.1.scan_entity(entity).await
    }
}
