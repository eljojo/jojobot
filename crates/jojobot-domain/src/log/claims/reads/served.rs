//! **A thing's scan, put into the shapes the port serves**: the entity, the
//! claims homed on it and the claims a merge left unmapped. Pure: the State is
//! all it reads. The entity and each claim come from the State's one projection
//! of each ([`State::entity`], [`State::fact`]). The prose of the page rides on
//! the object as metadata, and the owner is none.

use super::super::State;
use crate::log::ObjectId;
use crate::memory::search::DocScan;
use crate::memory::unmapped::UnmappedClaim;
use crate::memory::{EntityId, FactAddress, FactId, types::DeclaredType};

impl State {
    /// **The handle an object is served under**: the one it wore itself, else the
    /// first of the handles that answer to it, else the id.
    fn served_handle(&self, object: &ObjectId) -> EntityId {
        self.handle_of(object)
            .unwrap_or_else(|| EntityId(object.0.clone()))
    }

    /// **The document of one thing as its scan serves it**, from the State
    /// alone. `None` when the State holds no such thing.
    pub fn document_scan(&self, object: &ObjectId, declared: &[DeclaredType]) -> Option<DocScan> {
        let thing = self.scan(object, declared)?;
        let handle = self.handle_of(&thing.object)?;
        let entity = self.entity(&handle)?;
        let facts = thing
            .claims
            .iter()
            .filter_map(|(record, _)| self.fact(record))
            .collect();
        let unmapped = thing
            .unmapped
            .iter()
            .map(|(_, held)| {
                let home = &held.claim.home;
                UnmappedClaim {
                    // Under the handle the folded thing answers to now, and the
                    // number of the merge that folded it.
                    address: FactAddress::new(
                        self.handle_worn_by(home),
                        FactId(held.claim.label.clone()),
                    ),
                    merge_no: self
                        .merge_log
                        .iter()
                        .find(|entry| &entry.folded == home)
                        .map_or(0, |entry| entry.number),
                }
            })
            .collect();
        Some(DocScan {
            doc_id: thing.object.0.clone(),
            title: entity.name.clone(),
            prose: thing
                .body
                .metadata
                .get("prose")
                .cloned()
                .unwrap_or_default(),
            entity: Some(entity),
            facts,
            fields: thing.fields.clone(),
            field_links: thing
                .field_links
                .iter()
                .map(|(key, objects)| {
                    (
                        key.clone(),
                        objects.iter().map(|o| self.served_handle(o)).collect(),
                    )
                })
                .collect(),
            owner: None,
            unmapped,
        })
    }
}
