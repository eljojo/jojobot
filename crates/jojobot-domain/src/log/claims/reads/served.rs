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

    /// **Whether a thing's page holds anything**, read off the thing's own row: no
    /// document is built for the question. `false` for a thing the State does not
    /// hold.
    pub fn has_prose(&self, object: &ObjectId) -> bool {
        self.objects
            .get(self.survivor(object))
            .and_then(|body| body.metadata.get("prose"))
            .is_some_and(|prose| !prose.trim().is_empty())
    }

    /// **The things that hold a record pointing at a thing, or a record worked out
    /// from one of its claims**, each once, in handle order. What a merge or a
    /// rename of the thing can change in what those things serve.
    pub fn holders_of(&self, target: &crate::memory::EntityId) -> Vec<EntityId> {
        let mut holders: std::collections::BTreeSet<EntityId> = self
            .referring(target)
            .into_iter()
            .map(|fact| fact.address().home)
            .collect();
        if let Some(object) = self.resolve(&target.0) {
            for record in self.records_homed_on(object) {
                for built in self.records_built_on(record) {
                    if let Some(fact) = self.fact(built) {
                        holders.insert(fact.address().home);
                    }
                }
            }
        }
        holders.into_iter().collect()
    }

    /// **The claims of a thing that cannot be served**, listed in no event of the
    /// merge that left them under it: only these are built, and no claim of the
    /// thing is.
    pub fn unmapped_claims_of(&self, object: &ObjectId) -> Vec<UnmappedClaim> {
        let subject = self.survivor(object);
        let mut unmapped: Vec<_> = self
            .records_homed_on(subject)
            .into_iter()
            .filter_map(|id| self.records.get(id).map(|held| (id, held)))
            .filter(|(_, held)| held.claim.home != *subject)
            .collect();
        unmapped.sort_by(|a, b| {
            crate::memory::claim_label_order(&a.1.claim.label, &b.1.claim.label)
                .then_with(|| a.0.cmp(b.0))
        });
        unmapped
            .into_iter()
            .map(|(_, held)| {
                let home = &held.claim.home;
                UnmappedClaim {
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
            .collect()
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
