//! **The boot reader of a store with what the build supplies laid over it.**
//!
//! [`Provisioned`](crate::provisioned::Provisioned) changes three of the reads the
//! fill asks for: it adds the shipped defaults to a thing's fields, and it takes the
//! overlay's keys out of the fields, the ages and the backing it serves. A reader
//! over the same store has to give the fill the same answers, so it runs the same
//! steps over what the reader beneath it holds. A record the build supplies is left
//! to the read of one thing, which answers for it as the layer does.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use jojobot_domain::memory::boot_reader::{BootReader, FieldsVersioned};
use jojobot_domain::memory::owned::{Provisions, extended, is_overlay_key, without_overlay};
use jojobot_domain::memory::search::DocScan;
use jojobot_domain::memory::{
    ClaimWrite, EntityId, EntityVersion, FactId, FieldAges, FieldBacking, KeyTrails, MemoryError,
};

/// A boot reader, with the build's provisions laid over what it holds.
pub struct ProvisionedBootReader<R> {
    inner: R,
    provisions: Provisions,
}

impl<R: BootReader> ProvisionedBootReader<R> {
    /// The reader beneath, and the provisions the layer over the store carries.
    pub fn over(inner: R, provisions: Provisions) -> Self {
        ProvisionedBootReader { inner, provisions }
    }

    /// Whether the build supplies a record at this handle.
    fn supplied(&self, thing: &EntityId) -> bool {
        self.provisions.record_for(thing).is_some()
    }

    /// What a part holds, without the things the build supplies a record for.
    fn plain<T>(&self, mut held: HashMap<EntityId, T>) -> HashMap<EntityId, T> {
        held.retain(|thing, _| !self.supplied(thing));
        held
    }
}

#[async_trait]
impl<R: BootReader> BootReader for ProvisionedBootReader<R> {
    async fn fields_versioned_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldsVersioned>, MemoryError> {
        let held = self.plain(self.inner.fields_versioned_many(things).await?);
        Ok(held
            .into_iter()
            .map(|(thing, (held, version))| {
                let mut folded = self.provisions.fields_for(&thing);
                folded.extend(held);
                (thing, (without_overlay(folded), version))
            })
            .collect())
    }

    async fn field_ages_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldAges>, MemoryError> {
        let mut held = self.plain(self.inner.field_ages_many(things).await?);
        for ages in held.values_mut() {
            ages.keys.retain(|key, _| !is_overlay_key(key));
        }
        Ok(held)
    }

    async fn scans_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, DocScan>, MemoryError> {
        let mut held = self.plain(self.inner.scans_many(things).await?);
        for doc in held.values_mut() {
            if let Some(entity) = doc.entity.as_ref()
                && let Some(shipped) = self.provisions.prose_for(&entity.id)
            {
                doc.prose = extended(shipped, &doc.prose);
            }
        }
        Ok(held)
    }

    async fn claim_histories_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, HashMap<FactId, Vec<ClaimWrite>>>, MemoryError> {
        Ok(self.plain(self.inner.claim_histories_many(things).await?))
    }

    async fn backing_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, BTreeMap<String, FieldBacking>>, MemoryError> {
        let mut held = self.plain(self.inner.backing_many(things).await?);
        for backing in held.values_mut() {
            backing.retain(|key, _| !is_overlay_key(key));
        }
        Ok(held)
    }

    async fn key_trails_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, KeyTrails>, MemoryError> {
        Ok(self.plain(self.inner.key_trails_many(things).await?))
    }

    async fn entity_history_many(
        &self,
        things: &[EntityId],
    ) -> Result<HashMap<EntityId, Vec<EntityVersion>>, MemoryError> {
        Ok(self.plain(self.inner.entity_history_many(things).await?))
    }
}
