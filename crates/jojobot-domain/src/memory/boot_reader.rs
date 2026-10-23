//! **The parts the fold's startup fill holds for each thing, asked for many things
//! at once.**
//!
//! The fill reads six parts of every thing: its fields with their version, the age
//! of each key, the writes of each claim, what backs each key, the history of each
//! key and the versions of its own row. Asked one thing at a time, a fill of N
//! things sends the store a number of statements that grows with N. A
//! [`BootReader`] answers for many things in a few statements. It is a capability
//! of its own, handed to the fold beside the store and used by the fill alone: it
//! is not part of the [`Memory`](super::Memory) port, and no runtime read reaches
//! it.
//!
//! **A bulk answer may leave a thing out.** A thing whose answer the store cannot
//! work out set-wise without a risk of differing from the per-thing read (one that
//! was merged, one that was renamed) is left out of the map, and the fill asks for
//! that thing the way it always did. So every part is, for every thing it names,
//! exactly the per-thing answer, and a thing it does not name costs what it cost.

use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;

use super::{
    ClaimWrite, EntityId, EntityVersion, FactId, FieldAges, FieldBacking, KeyTrails, MemoryError,
};

/// What [`Memory::fields_versioned`](super::Memory::fields_versioned) answers.
pub type FieldsVersioned = (BTreeMap<String, String>, u64);

/// **The six reads, each for a list of things.** Every method defaults to
/// answering for none, so a store takes up the reads it can answer one at a time.
#[async_trait]
pub trait BootReader: Send + Sync {
    /// [`Memory::fields_versioned`](super::Memory::fields_versioned) of each.
    async fn fields_versioned_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldsVersioned>, MemoryError> {
        Ok(HashMap::new())
    }

    /// [`Memory::field_ages`](super::Memory::field_ages) of each.
    async fn field_ages_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, FieldAges>, MemoryError> {
        Ok(HashMap::new())
    }

    /// [`Memory::claim_histories`](super::Memory::claim_histories) of each.
    async fn claim_histories_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, HashMap<FactId, Vec<ClaimWrite>>>, MemoryError> {
        Ok(HashMap::new())
    }

    /// [`Memory::backing`](super::Memory::backing) of each.
    async fn backing_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, BTreeMap<String, FieldBacking>>, MemoryError> {
        Ok(HashMap::new())
    }

    /// [`Memory::key_trails`](super::Memory::key_trails) of each.
    async fn key_trails_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, KeyTrails>, MemoryError> {
        Ok(HashMap::new())
    }

    /// [`Memory::entity_history`](super::Memory::entity_history) of each.
    async fn entity_history_many(
        &self,
        _things: &[EntityId],
    ) -> Result<HashMap<EntityId, Vec<EntityVersion>>, MemoryError> {
        Ok(HashMap::new())
    }
}

/// **The bulk answers of one fill, taken apart by thing.** A part a thing has no
/// answer for is `None`, and the fill reads that part of that thing on its own.
#[derive(Default)]
pub struct Parts {
    fields: HashMap<EntityId, FieldsVersioned>,
    ages: HashMap<EntityId, FieldAges>,
    histories: HashMap<EntityId, HashMap<FactId, Vec<ClaimWrite>>>,
    backing: HashMap<EntityId, BTreeMap<String, FieldBacking>>,
    trails: HashMap<EntityId, KeyTrails>,
    versions: HashMap<EntityId, Vec<EntityVersion>>,
}

/// **What the bulk reads hold for one thing.**
#[derive(Default)]
pub struct ThingParts {
    pub fields: Option<FieldsVersioned>,
    pub ages: Option<FieldAges>,
    pub histories: Option<HashMap<FactId, Vec<ClaimWrite>>>,
    pub backing: Option<BTreeMap<String, FieldBacking>>,
    pub trails: Option<KeyTrails>,
    pub versions: Option<Vec<EntityVersion>>,
}

impl Parts {
    /// **Ask a store's bulk reads for all six parts of `things`.**
    pub async fn read(bulk: &dyn BootReader, things: &[EntityId]) -> Result<Parts, MemoryError> {
        Ok(Parts {
            fields: bulk.fields_versioned_many(things).await?,
            ages: bulk.field_ages_many(things).await?,
            histories: bulk.claim_histories_many(things).await?,
            backing: bulk.backing_many(things).await?,
            trails: bulk.key_trails_many(things).await?,
            versions: bulk.entity_history_many(things).await?,
        })
    }

    /// **Take what the bulk reads hold for one thing**, once.
    pub fn take(&mut self, thing: &EntityId) -> ThingParts {
        ThingParts {
            fields: self.fields.remove(thing),
            ages: self.ages.remove(thing),
            histories: self.histories.remove(thing),
            backing: self.backing.remove(thing),
            trails: self.trails.remove(thing),
            versions: self.versions.remove(thing),
        }
    }
}
