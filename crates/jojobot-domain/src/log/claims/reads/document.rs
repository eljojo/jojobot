//! **A thing's value parts, put into the shapes the port serves.** Each is b2's
//! read of the State; only the shape differs. Pure: the State is all it reads.

use std::collections::{BTreeMap, HashMap};

use super::{ObjectBody, ObjectVersion, Revision, State};
use crate::actor::Actor;
use crate::log::claims::ObjectSnapshot;
use crate::log::{ObjectId, RecordId};
use crate::memory::{
    ClaimWrite, Edge, EdgeShape, EntityId, EntitySnapshot, EntityVersion, FactAddress, FactId,
    FieldAges, FieldBacking, KeyTrails, types::DeclaredType,
};

/// The value parts of one thing's document.
pub struct DocumentParts {
    pub fields: BTreeMap<String, String>,
    pub version: u64,
    pub ages: FieldAges,
    pub histories: HashMap<FactId, Vec<ClaimWrite>>,
    pub backing: BTreeMap<String, FieldBacking>,
    pub trails: KeyTrails,
    pub entity_versions: Vec<EntityVersion>,
    /// **The writes of each claim the thing cannot serve**, by the address that
    /// reads it.
    pub unmapped_histories: HashMap<FactAddress, Vec<ClaimWrite>>,
}

/// The handle an object answers to now, or the id itself when it has none.
fn served(state: &State, object: &ObjectId) -> EntityId {
    state
        .handle_of(object)
        .unwrap_or_else(|| EntityId(object.0.clone()))
}

/// **The address a claim is served at now**: the handle of its home and its label.
fn address_of(state: &State, record: &RecordId) -> Option<FactAddress> {
    let Some(held) = state.records.get(record) else {
        // A record the State does not hold is served at the address it was filed under.
        return state.filed_address(record);
    };
    // **The claim as it stands now**, whose home and label a merge moved, and not
    // the claim as its last write left it: the store serves the relabelled address.
    let claim = &held.claim;
    Some(FactAddress::new(
        served(state, &claim.home),
        FactId(claim.label.clone()),
    ))
}

fn claim_write(state: &State, ordinal: usize, write: &Revision) -> ClaimWrite {
    let claim = &write.claim;
    ClaimWrite {
        ordinal,
        written_at: write.written_at,
        session: write.run.as_ref().map(|run| run.0.clone()),
        bot: write
            .actor
            .as_ref()
            .and_then(Actor::bot)
            .map(|bot| served(state, &ObjectId(bot.0.clone()))),
        content: claim.content.clone(),
        details: claim.details.clone(),
        provenance: claim.provenance,
        standing: claim.standing,
        status: claim.status,
        recorded_at: claim.recorded_at,
        happened_at: claim.happened_at,
        happened_through: claim.happened_through,
        edge: claim.edge.as_ref().and_then(|(shape, object)| {
            EdgeShape::from_token(shape).map(|shape| Edge {
                shape,
                object: served(state, object),
            })
        }),
        derived_from: claim
            .derived_from
            .as_ref()
            .and_then(|record| address_of(state, record)),
        stale_after: claim.stale_after,
        receipt: claim.receipt.clone(),
        app_stamp: claim.app.clone(),
        // By the handle each thing wore itself, a merged-away one included: the
        // store serves a ref as the thing it named, not as its survivor.
        refs: Some(claim.refs.iter().map(|o| state.handle_worn_by(o)).collect()),
        stands_for: Some(
            claim
                .stands_for
                .iter()
                .filter_map(|record| address_of(state, record))
                .collect(),
        ),
    }
}

fn snapshot(state: &State, body: &ObjectBody) -> EntitySnapshot {
    EntitySnapshot {
        kind: body.kind.0.clone(),
        name: body.name.clone(),
        source: body.source.clone(),
        crm: body.metadata.get("crm").cloned(),
        parent: body
            .parent
            .as_ref()
            .map(|parent| state.handle_worn_by(parent)),
        boot: body.boot.clone().unwrap_or_default(),
        archived_reason: body.metadata.get("archived_reason").cloned(),
        archived_at: body.metadata.get("archived_at").cloned(),
        merged_into: None,
        aliases: body.aliases.clone(),
        prose: body.metadata.get("prose").cloned().unwrap_or_default(),
    }
}

fn observed_snapshot(state: &State, snapshot: &ObjectSnapshot) -> EntitySnapshot {
    EntitySnapshot {
        kind: snapshot.kind.clone(),
        name: snapshot.name.clone(),
        source: snapshot.source.clone(),
        crm: snapshot.crm.clone(),
        parent: snapshot
            .parent
            .as_ref()
            .map(|parent| state.handle_worn_by(parent)),
        boot: snapshot.boot.clone(),
        archived_reason: snapshot.archived_reason.clone(),
        archived_at: snapshot.archived_at.clone(),
        merged_into: snapshot
            .merged_into
            .as_ref()
            .map(|survivor| served(state, survivor)),
        aliases: snapshot.aliases.clone(),
        prose: snapshot.prose.clone(),
    }
}

fn entity_version(state: &State, ordinal: usize, version: &ObjectVersion) -> EntityVersion {
    EntityVersion {
        ordinal,
        thing: state.handle_worn_by(&version.thing),
        written_at: version.written_at,
        session: version.run.as_ref().map(|run| run.0.clone()),
        snapshot: Some(snapshot(state, &version.body)),
    }
}

impl State {
    /// **The value parts of the thing stored under `object`**, in the shapes the
    /// port serves, from the State alone. `None` when the State holds no such
    /// thing. The `session` of a write is the run on its envelope and its `bot` the
    /// handle of the actor, both empty for a write whose envelope names neither.
    pub fn document_parts(
        &self,
        object: &ObjectId,
        declared: &[DeclaredType],
    ) -> Option<DocumentParts> {
        let thing = self.scan(object, declared)?;
        let histories = self.claim_chains_of(object);
        let mut unmapped_histories = HashMap::new();
        for (record, held) in &thing.unmapped {
            // Read at the address the folded thing's own handle gives the claim.
            let address = FactAddress::new(
                self.handle_worn_by(&held.claim.home),
                FactId(held.claim.label.clone()),
            );
            unmapped_histories.insert(address, self.chain_of(record));
        }
        Some(DocumentParts {
            unmapped_histories,
            fields: thing.fields.clone(),
            version: self.write_marker(object),
            ages: self.field_ages_of(object, declared),
            histories,
            backing: self.backing(object, declared),
            trails: self.key_trails_served(object),
            entity_versions: self.entity_versions_of(object),
        })
    }

    /// **One claim's writes, oldest first**, the last being the claim now.
    fn chain_of(&self, record: &RecordId) -> Vec<ClaimWrite> {
        #[cfg(test)]
        State::note_chain();
        self.claim_history(record)
            .iter()
            .enumerate()
            .map(|(at, write)| claim_write(self, at + 1, write))
            .collect()
    }

    /// **The record of the claim at `label` on `object`**, found among the claims
    /// homed on the thing and no other.
    fn record_at(&self, object: &ObjectId, label: &FactId) -> Option<&RecordId> {
        let subject = self.survivor(object);
        self.records_homed_on(subject).into_iter().find(|record| {
            self.records
                .get(*record)
                .is_some_and(|held| held.claim.home == *subject && held.claim.label == label.0)
        })
    }

    /// **The writes of the one claim at `label` on `object`**, built for that claim
    /// and for no other. `None` when the thing holds no such claim.
    pub fn claim_chain_of(&self, object: &ObjectId, label: &FactId) -> Option<Vec<ClaimWrite>> {
        Some(self.chain_of(self.record_at(object, label)?))
    }

    /// **The writes of every claim homed on the thing**, each claim by its label:
    /// one chain for each claim and nothing else built.
    pub fn claim_chains_of(&self, object: &ObjectId) -> HashMap<FactId, Vec<ClaimWrite>> {
        let subject = self.survivor(object);
        self.records_homed_on(subject)
            .into_iter()
            .filter_map(|record| Some((record, self.records.get(record)?)))
            .filter(|(_, held)| held.claim.home == *subject)
            .map(|(record, held)| (FactId(held.claim.label.clone()), self.chain_of(record)))
            .collect()
    }

    /// **A thing's key trails as the port serves them**: the homes of the writes
    /// are the handles their things answer to now.
    pub fn key_trails_served(&self, object: &ObjectId) -> KeyTrails {
        let mut trails = self.key_trails(object);
        for write in trails.history.values_mut().flatten() {
            write.fact.home = served(self, &ObjectId(write.fact.home.0.clone()));
        }
        trails
    }

    /// **The writes of one key on a thing**, oldest first, built for that key and
    /// for no other.
    pub fn key_history(&self, object: &ObjectId, key: &str) -> Vec<crate::memory::FieldWrite> {
        let mut writes = self
            .trails_of(object, Some(key))
            .history
            .remove(key)
            .unwrap_or_default();
        for write in &mut writes {
            write.fact.home = served(self, &ObjectId(write.fact.home.0.clone()));
        }
        writes
    }

    /// **The keys the claim at `label` keeps off its thing**: its newest write of
    /// the key, unless a later one took the key off, was made under a class that
    /// does not reach the thing. Read from that claim's own writes.
    pub fn keys_kept_off(
        &self,
        object: &ObjectId,
        label: &FactId,
    ) -> std::collections::BTreeSet<String> {
        let Some(record) = self.record_at(object, label) else {
            return Default::default();
        };
        let mut kept = std::collections::BTreeSet::new();
        for (subject, key) in self.streams_written_by(record) {
            let Some(stream) = self.streams.get(&(subject, key.clone())) else {
                continue;
            };
            let mut mine: Vec<&super::super::Write> = stream
                .iter()
                .filter(|write| &write.record == record)
                .collect();
            mine.sort_by_key(|write| write.ordinal);
            let mut held = None;
            for write in mine {
                held = match write.write {
                    super::super::FieldValue::Set(_) => Some(write.class),
                    _ => None,
                };
            }
            if held.is_some_and(|class| !crate::memory::reaches_the_thing(class)) {
                kept.insert(key.0.clone());
            }
        }
        kept
    }

    /// **A thing's field ages**: the newest change of the thing and when each held
    /// key was last set. No scan of the thing is built.
    pub fn field_ages_of(&self, object: &ObjectId, declared: &[DeclaredType]) -> FieldAges {
        FieldAges {
            newest: self.newest_change(object),
            keys: self.held_since(object, declared).into_iter().collect(),
        }
    }

    /// **The versions of a thing's own row**, in the order they happened.
    pub fn entity_versions_of(&self, object: &ObjectId) -> Vec<EntityVersion> {
        let mut versions: Vec<EntityVersion> = self
            .observed_chain(object)
            .into_iter()
            .map(|(thing, write)| EntityVersion {
                ordinal: 0,
                thing: self.handle_worn_by(thing),
                written_at: write.source_time,
                session: write.run.as_ref().map(|run| run.0.clone()),
                snapshot: write
                    .snapshot
                    .as_ref()
                    .map(|snapshot| observed_snapshot(self, snapshot)),
            })
            .chain(
                self.entity_versions(object)
                    .into_iter()
                    .map(|version| entity_version(self, 0, version)),
            )
            .collect();
        // In the order they happened, the store's own order: a version nobody
        // stamped sorts first, and equal moments keep the order they were gathered
        // in. The ordinal is this list's own count.
        versions.sort_by_key(|version| version.written_at);
        versions
            .into_iter()
            .enumerate()
            .map(|(at, version)| EntityVersion {
                ordinal: at + 1,
                ..version
            })
            .collect()
    }
}
