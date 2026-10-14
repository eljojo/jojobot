//! **Text as the store keeps it, and the one door out of that state.**
//!
//! A claim, a page and a note that carry a handle in their words are STORED
//! with the permanent badge in place of the handle, and RENDERED back to the
//! handle the badge answers to today ([`super::mention`]). A reader must never
//! be served the stored form: a badge is not addressable and nothing accepts it
//! back.
//!
//! **[`Stored`] is the type of text that has not been rendered.** Every read of
//! [`StoredMemory`] that returns text returns it inside a `Stored`, and a
//! `Stored` is not the plain value: it cannot be returned where the plain value
//! is wanted, and only the mention module can open it. So a decorator that
//! serves the inner store's text without rendering it does not compile.
//!
//! ⚠️ **What this closes, and what it does not.** It closes the FORGOTTEN
//! DELEGATE: a door in [`super::mention::Mentioning`] that hands back what the
//! store beneath it returned. It does NOT close the NEW DOOR: a door declared in
//! [`StoredMemory`] with plain text types compiles and serves unrendered text, so
//! **a new text-bearing door takes `Stored` in its signature here**. That
//! signature is the one place the choice is made, and review sees it.
//! `no_read_serves_a_badge_and_every_one_serves_the_handle` (the contract) is the
//! second guard and covers that hole.
//!
//! ```compile_fail,E0308
//! // A tenth door that returns what the store beneath it returned does not
//! // build: `recall` gives the stored form, and the plain form is not that type.
//! use std::sync::Arc;
//! use jojobot_domain::memory::stored::StoredMemory;
//! use jojobot_domain::memory::{EntityId, Fact, MemoryError};
//!
//! async fn tenth_door(
//!     below: Arc<dyn StoredMemory>,
//!     whose: &EntityId,
//! ) -> Result<Vec<Fact>, MemoryError> {
//!     below.recall(whose).await
//! }
//! ```

use std::sync::Arc;

use super::{Entity, EntityId};

/// **Text that leaves a store with its mentions as the store keeps them.**
///
/// Anybody can make one ([`Stored::new`]): a store makes stored text. Nobody
/// outside the memory module can open one, so the only way from `Stored<T>` to
/// `T` is the render in [`super::mention`].
#[derive(Debug, Clone, PartialEq)]
pub struct Stored<T>(T);

impl<T> Stored<T> {
    /// Text as the store keeps it.
    pub fn new(value: T) -> Self {
        Stored(value)
    }

    /// **The stored form, opened for rendering.** Visible to the memory module
    /// only, which is where [`super::mention`] lives: every call is a decision
    /// that the value is about to be rendered.
    pub(super) fn open(self) -> T {
        self.0
    }
}

/// **The reads and writes of a store, with the text they carry still stored.**
///
/// [`super::mention::Mentioning`] reads the store beneath it through this
/// trait and nothing else. It is the same set of doors as the ones Mentioning
/// serves, with every one that returns text a caller wrote returning it as
/// [`Stored`].
///
/// ⚠️ **The hole is here.** A door added to this trait with plain text types
/// compiles unrendered. A new door that returns a claim, a page, a note or a
/// chain of wordings takes `Stored`; a door that returns handles, names, counts
/// or declarations takes the plain type.
#[async_trait::async_trait]
pub trait StoredMemory: Send + Sync {
    async fn add_entity(
        &self,
        new: super::NewEntity,
    ) -> Result<super::Guarded<Entity>, super::MemoryError>;
    async fn add_entity_with_first_claim(
        &self,
        new: super::NewEntity,
        first: super::NewFact,
    ) -> Result<Stored<super::Guarded<(Entity, super::Fact)>>, super::MemoryError>;
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
        claim: super::NewFact,
    ) -> Result<Stored<super::Guarded<(Entity, super::Fact)>>, super::MemoryError>;
    async fn list_entities(
        &self,
        kind: Option<super::EntityKind>,
    ) -> Result<Vec<Entity>, super::MemoryError>;
    async fn former_handles(&self) -> Result<Vec<super::FormerHandle>, super::MemoryError>;
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
    ) -> Result<super::Guarded<Entity>, super::MemoryError>;
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: jiff::civil::Date,
        override_token: Option<&str>,
        override_reason: Option<&str>,
        session: Option<&str>,
    ) -> Result<super::Guarded<Entity>, super::MemoryError>;
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, super::MemoryError>;
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, super::Archived), super::MemoryError>;
    async fn capture(
        &self,
        fact: super::NewFact,
    ) -> Result<Stored<super::Guarded<super::Fact>>, super::MemoryError>;
    async fn recall(
        &self,
        subject: &EntityId,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError>;
    async fn update_fact(
        &self,
        address: &super::FactAddress,
        patch: super::FactPatch,
        caller: &super::EntityId,
    ) -> Result<Stored<super::Guarded<super::Fact>>, super::MemoryError>;
    async fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Stored<Vec<super::FieldWrite>>, super::MemoryError>;
    async fn claim_history(
        &self,
        address: &super::FactAddress,
    ) -> Result<Stored<Vec<super::ClaimWrite>>, super::MemoryError>;
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<Vec<super::EntityVersion>>, super::MemoryError>;
    async fn keys_kept_off_the_thing(
        &self,
        address: &super::FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, super::MemoryError>;
    /// Handles, ids and moments only: no text a caller wrote, so the plain type.
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<super::MergeEvent>, super::MemoryError>;
    async fn moved_address(
        &self,
        address: &super::FactAddress,
    ) -> Result<Option<super::FactAddress>, super::MemoryError>;
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<super::unmapped::UnmappedClaim>, super::MemoryError>;
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<
        Stored<std::collections::HashMap<super::FactId, Vec<super::ClaimWrite>>>,
        super::MemoryError,
    >;
    async fn field_ages(&self, entity: &EntityId) -> Result<super::FieldAges, super::MemoryError>;
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<std::collections::BTreeMap<String, String>>, super::MemoryError>;
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<std::collections::BTreeMap<String, super::FieldBacking>>, super::MemoryError>;
    async fn retract(
        &self,
        address: &super::FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &super::EntityId,
        session: Option<&str>,
    ) -> Result<Stored<super::Retraction>, super::MemoryError>;
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Stored<super::Merge>, super::MemoryError>;
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<Stored<String>, super::MemoryError>;
    async fn scan(&self) -> Result<Stored<Vec<super::search::DocScan>>, super::MemoryError>;
    async fn write_summary(&self) -> Result<Option<super::WriteSummary>, super::MemoryError>;
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Vec<String>;
    async fn composed_prose(
        &self,
        entity: &EntityId,
        own: &str,
    ) -> Result<String, super::MemoryError>;
    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<Option<super::search::DocScan>>, super::MemoryError>;
    async fn built_on(
        &self,
        source: &super::FactAddress,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError>;
    async fn referring_to(
        &self,
        target: &EntityId,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError>;
    async fn thought_writers(&self, thread: &EntityId)
    -> Result<Vec<EntityId>, super::MemoryError>;
    async fn declare_type(
        &self,
        declared: super::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<super::types::DeclaredType, super::MemoryError>;
    async fn declared_types(&self) -> Result<Vec<super::types::DeclaredType>, super::MemoryError>;
    async fn displaced_type(
        &self,
        name: &str,
    ) -> Result<Option<super::types::Displaced>, super::MemoryError>;
    async fn declare_kind(
        &self,
        token: &str,
        origin: super::types::Origin,
        fields: Vec<super::types::Field>,
    ) -> Result<(), super::MemoryError>;
    async fn reclaim_kind(&self, token: &str) -> Result<(), super::MemoryError>;
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, super::types::Origin)>, super::MemoryError>;
}

/// **A store's own reads, taken as stored.** Wraps the store beneath the
/// mention layer: text-bearing answers come back as [`Stored`].
pub(super) struct Unrendered(pub(super) Arc<dyn super::Memory>);

#[async_trait::async_trait]
impl StoredMemory for Unrendered {
    async fn add_entity(
        &self,
        new: super::NewEntity,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.0.add_entity(new).await
    }
    async fn add_entity_with_first_claim(
        &self,
        new: super::NewEntity,
        first: super::NewFact,
    ) -> Result<Stored<super::Guarded<(Entity, super::Fact)>>, super::MemoryError> {
        self.0
            .add_entity_with_first_claim(new, first)
            .await
            .map(Stored::new)
    }
    async fn update_entity_with_claim(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
        claim: super::NewFact,
    ) -> Result<Stored<super::Guarded<(Entity, super::Fact)>>, super::MemoryError> {
        self.0
            .update_entity_with_claim(handle, patch, claim)
            .await
            .map(Stored::new)
    }
    async fn list_entities(
        &self,
        kind: Option<super::EntityKind>,
    ) -> Result<Vec<Entity>, super::MemoryError> {
        self.0.list_entities(kind).await
    }
    async fn former_handles(&self) -> Result<Vec<super::FormerHandle>, super::MemoryError> {
        self.0.former_handles().await
    }
    async fn update_entity(
        &self,
        handle: &EntityId,
        patch: super::EntityPatch,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.0.update_entity(handle, patch).await
    }
    async fn rename_entity(
        &self,
        from: &EntityId,
        to: &EntityId,
        parent: Option<EntityId>,
        date: jiff::civil::Date,
        override_token: Option<&str>,
        override_reason: Option<&str>,
        session: Option<&str>,
    ) -> Result<super::Guarded<Entity>, super::MemoryError> {
        self.0
            .rename_entity(
                from,
                to,
                parent,
                date,
                override_token,
                override_reason,
                session,
            )
            .await
    }
    async fn archive_entity(
        &self,
        id: &EntityId,
        reason: &str,
        session: Option<&str>,
    ) -> Result<Entity, super::MemoryError> {
        self.0.archive_entity(id, reason, session).await
    }
    async fn restore_entity(
        &self,
        id: &EntityId,
        session: Option<&str>,
    ) -> Result<(Entity, super::Archived), super::MemoryError> {
        self.0.restore_entity(id, session).await
    }
    async fn capture(
        &self,
        fact: super::NewFact,
    ) -> Result<Stored<super::Guarded<super::Fact>>, super::MemoryError> {
        self.0.capture(fact).await.map(Stored::new)
    }
    async fn recall(
        &self,
        subject: &EntityId,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError> {
        self.0.recall(subject).await.map(Stored::new)
    }
    async fn update_fact(
        &self,
        address: &super::FactAddress,
        patch: super::FactPatch,
        caller: &super::EntityId,
    ) -> Result<Stored<super::Guarded<super::Fact>>, super::MemoryError> {
        self.0
            .update_fact(address, patch, caller)
            .await
            .map(Stored::new)
    }
    async fn history(
        &self,
        entity: &EntityId,
        key: &str,
    ) -> Result<Stored<Vec<super::FieldWrite>>, super::MemoryError> {
        self.0.history(entity, key).await.map(Stored::new)
    }
    async fn claim_history(
        &self,
        address: &super::FactAddress,
    ) -> Result<Stored<Vec<super::ClaimWrite>>, super::MemoryError> {
        self.0.claim_history(address).await.map(Stored::new)
    }
    async fn entity_history(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<Vec<super::EntityVersion>>, super::MemoryError> {
        self.0.entity_history(entity).await.map(Stored::new)
    }
    async fn keys_kept_off_the_thing(
        &self,
        address: &super::FactAddress,
    ) -> Result<std::collections::BTreeSet<String>, super::MemoryError> {
        self.0.keys_kept_off_the_thing(address).await
    }
    async fn merge_events(
        &self,
        entity: Option<&EntityId>,
    ) -> Result<Vec<super::MergeEvent>, super::MemoryError> {
        self.0.merge_events(entity).await
    }
    async fn moved_address(
        &self,
        address: &super::FactAddress,
    ) -> Result<Option<super::FactAddress>, super::MemoryError> {
        self.0.moved_address(address).await
    }
    async fn unmapped_claims(
        &self,
        entity: &EntityId,
    ) -> Result<Vec<super::unmapped::UnmappedClaim>, super::MemoryError> {
        self.0.unmapped_claims(entity).await
    }
    async fn claim_histories(
        &self,
        entity: &EntityId,
    ) -> Result<
        Stored<std::collections::HashMap<super::FactId, Vec<super::ClaimWrite>>>,
        super::MemoryError,
    > {
        self.0.claim_histories(entity).await.map(Stored::new)
    }
    async fn field_ages(&self, entity: &EntityId) -> Result<super::FieldAges, super::MemoryError> {
        self.0.field_ages(entity).await
    }
    async fn fields(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<std::collections::BTreeMap<String, String>>, super::MemoryError> {
        self.0.fields(entity).await.map(Stored::new)
    }
    async fn backing(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<std::collections::BTreeMap<String, super::FieldBacking>>, super::MemoryError>
    {
        self.0.backing(entity).await.map(Stored::new)
    }
    async fn retract(
        &self,
        address: &super::FactAddress,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &super::EntityId,
        session: Option<&str>,
    ) -> Result<Stored<super::Retraction>, super::MemoryError> {
        self.0
            .retract(address, reason, date, caller, session)
            .await
            .map(Stored::new)
    }
    async fn merge(
        &self,
        folded: &EntityId,
        survivor: &EntityId,
        reason: Option<&str>,
        date: jiff::civil::Date,
        caller: &EntityId,
        session: Option<&str>,
    ) -> Result<Stored<super::Merge>, super::MemoryError> {
        self.0
            .merge(folded, survivor, reason, date, caller, session)
            .await
            .map(Stored::new)
    }
    async fn set_prose(
        &self,
        entity: &EntityId,
        prose: &str,
        session: Option<&str>,
    ) -> Result<Stored<String>, super::MemoryError> {
        self.0
            .set_prose(entity, prose, session)
            .await
            .map(Stored::new)
    }
    async fn scan(&self) -> Result<Stored<Vec<super::search::DocScan>>, super::MemoryError> {
        self.0.scan().await.map(Stored::new)
    }
    async fn write_summary(&self) -> Result<Option<super::WriteSummary>, super::MemoryError> {
        self.0.write_summary().await
    }
    async fn echoed_defaults(
        &self,
        entity: &EntityId,
        fields: &std::collections::BTreeMap<String, String>,
    ) -> Vec<String> {
        self.0.echoed_defaults(entity, fields).await
    }
    async fn composed_prose(
        &self,
        entity: &EntityId,
        own: &str,
    ) -> Result<String, super::MemoryError> {
        self.0.composed_prose(entity, own).await
    }
    async fn scan_entity(
        &self,
        entity: &EntityId,
    ) -> Result<Stored<Option<super::search::DocScan>>, super::MemoryError> {
        self.0.scan_entity(entity).await.map(Stored::new)
    }
    async fn built_on(
        &self,
        source: &super::FactAddress,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError> {
        self.0.built_on(source).await.map(Stored::new)
    }
    async fn referring_to(
        &self,
        target: &EntityId,
    ) -> Result<Stored<Vec<super::Fact>>, super::MemoryError> {
        self.0.referring_to(target).await.map(Stored::new)
    }
    async fn thought_writers(
        &self,
        thread: &EntityId,
    ) -> Result<Vec<EntityId>, super::MemoryError> {
        self.0.thought_writers(thread).await
    }
    async fn declare_type(
        &self,
        declared: super::types::DeclaredType,
        by_session: Option<&str>,
    ) -> Result<super::types::DeclaredType, super::MemoryError> {
        self.0.declare_type(declared, by_session).await
    }
    async fn declared_types(&self) -> Result<Vec<super::types::DeclaredType>, super::MemoryError> {
        self.0.declared_types().await
    }
    async fn displaced_type(
        &self,
        name: &str,
    ) -> Result<Option<super::types::Displaced>, super::MemoryError> {
        self.0.displaced_type(name).await
    }
    async fn declare_kind(
        &self,
        token: &str,
        origin: super::types::Origin,
        fields: Vec<super::types::Field>,
    ) -> Result<(), super::MemoryError> {
        self.0.declare_kind(token, origin, fields).await
    }
    async fn reclaim_kind(&self, token: &str) -> Result<(), super::MemoryError> {
        self.0.reclaim_kind(token).await
    }
    async fn declared_kinds(
        &self,
    ) -> Result<Vec<(String, super::types::Origin)>, super::MemoryError> {
        self.0.declared_kinds().await
    }
}
