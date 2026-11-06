//! **A thing's scan, put into the shapes the port serves**: the entity, the
//! claims homed on it and the claims a merge left unmapped. Pure: the State is
//! all it reads. The entity and each claim come from the State's one projection
//! of each ([`State::entity`], [`State::fact`]). The prose of the page rides on
//! the object as metadata, and the owner is none.

use super::super::State;
use crate::log::{ObjectId, RecordId};
use crate::memory::search::DocScan;
use crate::memory::unmapped::UnmappedClaim;
use crate::memory::{EntityId, FactAddress, FactId, types::DeclaredType};

impl State {
    /// **The handle an object is served under**: the one it wore itself, else the
    /// first of the handles that answer to it, else the id.
    fn served_handle(&self, object: &ObjectId) -> EntityId {
        self.own_handle_of(self.survivor(object))
            .or_else(|| self.handle_of(object))
            .unwrap_or_else(|| EntityId(object.0.clone()))
    }

    /// **The address a claim is served at now.**
    fn served_address(&self, record: &RecordId) -> Option<FactAddress> {
        let claim = &self.claim_history(record).last()?.claim;
        Some(FactAddress::new(
            self.served_handle(&claim.home),
            FactId(claim.label.clone()),
        ))
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
            .filter_map(|(record, _)| {
                self.served_address(record).map(|address| UnmappedClaim {
                    address,
                    merge_no: 0,
                })
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
