//! **A thing's scan, put into the shapes the port serves**: the entity, the
//! claims homed on it and the claims a merge left unmapped. Pure: the State is
//! all it reads. The prose of the page and the owner are not in the State's
//! claims; the prose rides on the object as metadata, and the owner is none.

use std::collections::{BTreeMap, BTreeSet};

use super::super::{ClaimBody, FieldValue, ObjectBody, Record, State};
use crate::log::{ObjectId, RecordId};
use crate::memory::search::DocScan;
use crate::memory::unmapped::UnmappedClaim;
use crate::memory::{
    Archived, Boot, Edge, EdgeShape, Entity, EntityId, Fact, FactAddress, FactId, kinds,
    types::DeclaredType,
};

impl State {
    /// **The handle an object is served under**, or the id itself when it has
    /// none.
    fn served_handle(&self, object: &ObjectId) -> EntityId {
        self.handle_of(object)
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

    /// **The names of the things folded into this one**, each once, and none the
    /// survivor already holds. The order is that of the folded objects' ids.
    fn folded_names(&self, survivor: &ObjectId, body: &ObjectBody) -> Vec<String> {
        let mut names: Vec<String> = Vec::new();
        for folded in self.folded_under.get(survivor).into_iter().flatten() {
            let Some(held) = self.objects.get(folded) else {
                continue;
            };
            let known = held.name == body.name
                || body.aliases.contains(&held.name)
                || names.contains(&held.name);
            if !known {
                names.push(held.name.clone());
            }
        }
        names
    }

    /// **The entity a thing is served as**, from its object. `None` when the
    /// State holds no such object, or its handle names a kind nobody declares.
    pub fn served_entity(&self, object: &ObjectId) -> Option<Entity> {
        let object = self.survivor(object);
        let body: &ObjectBody = self.objects.get(object)?;
        let id = self.handle_of(object)?;
        let kind = kinds::resolve(id.kind_token()).ok()?;
        let archived = body
            .metadata
            .get("archived_reason")
            .zip(body.metadata.get("archived_at"))
            .and_then(|(reason, at)| {
                at.parse().ok().map(|at| Archived {
                    reason: reason.clone(),
                    at,
                })
            });
        Some(Entity {
            id,
            merged_into: None,
            kind,
            name: body.name.clone(),
            aliases: body.aliases.clone(),
            folded_names: self.folded_names(object, body),
            source: body.source.clone(),
            crm: body.metadata.get("crm").cloned(),
            parent: body
                .parent
                .as_ref()
                .map(|parent| self.served_handle(parent)),
            boot: Boot::from_token(body.boot.as_deref().unwrap_or_default()),
            badge: body.metadata.get("badge").cloned(),
            archived,
        })
    }

    /// **One claim as it is served**, with the fields it carries, which of them
    /// it set on its thing and which of them reach it.
    fn served_fact(&self, record: &RecordId, held: &Record) -> Fact {
        let claim: &ClaimBody = &held.claim;
        let handle = self.served_handle(&claim.home);
        let mut fields = BTreeMap::new();
        let mut sets = BTreeSet::new();
        let mut reaching = BTreeSet::new();
        for (subject, key) in self.streams_written_by(record) {
            let newest = self
                .streams
                .get(&(subject, key.clone()))
                .and_then(|stream| stream.iter().rev().find(|write| write.record == *record));
            let Some(write) = newest else { continue };
            if let FieldValue::Set(value) = &write.write {
                fields.insert(key.0.clone(), value.clone());
                if write.class == crate::memory::WriteClass::Sets {
                    sets.insert(key.0.clone());
                }
                if crate::memory::reaches_the_thing(write.class) {
                    reaching.insert(key.0.clone());
                }
            }
        }
        let field_links = self.marked_links(&fields);
        let fields = self.compose_marks(fields);
        Fact {
            id: FactId(claim.label.clone()),
            home: handle.clone(),
            subject: handle,
            content: claim.content.clone(),
            details: claim.details.clone(),
            provenance: claim.provenance,
            standing: claim.standing,
            status: claim.status,
            recorded_at: claim.recorded_at,
            happened_at: claim.happened_at,
            happened_through: claim.happened_through,
            inserted_at: claim.inserted_at,
            stale_after: claim.stale_after,
            receipt: claim.receipt.clone(),
            app_stamp: claim.app.clone(),
            written_by_another: false,
            edge: claim.edge.as_ref().and_then(|(shape, object)| {
                EdgeShape::from_token(shape).map(|shape| Edge {
                    shape,
                    object: self.served_handle(object),
                })
            }),
            fields,
            refs: claim
                .refs
                .iter()
                .map(|object| self.served_handle(object))
                .collect(),
            derived_from: claim
                .derived_from
                .as_ref()
                .and_then(|record| self.served_address(record)),
            stands_for: claim
                .stands_for
                .iter()
                .filter_map(|record| self.served_address(record))
                .collect(),
            sets,
            reaching,
            field_links,
        }
    }

    /// **The document of one thing as its scan serves it**, from the State
    /// alone. `None` when the State holds no such thing.
    pub fn document_scan(&self, object: &ObjectId, declared: &[DeclaredType]) -> Option<DocScan> {
        let thing = self.scan(object, declared)?;
        let entity = self.served_entity(object)?;
        let facts = thing
            .claims
            .iter()
            .map(|(record, held)| self.served_fact(record, held))
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
