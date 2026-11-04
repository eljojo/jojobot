//! **What the fold re-reads after every write, answered from the State.** Each
//! read is one function of the writes the State already holds for one thing, so
//! it never scans the claims of any other thing. A read here is the same fold
//! the store's read runs ([`memory::folded_backing`], [`memory::folded_ages`]):
//! the value and what stands behind it cannot come from two passes that disagree.

use std::collections::BTreeMap;

use jiff::Timestamp;

use super::{FieldValue, KeyId, ObjectVersion, Revision, State, Write};
use crate::log::{ObjectId, RecordId};
use crate::memory::{
    self, EntityId, FactAddress, FactId, FieldBacking, FieldWrite, KeyTrails, RecordFieldWrite,
    WriteClass, types::DeclaredType,
};

// **How many streams the reads have looked at on this thread**, so a case can
// say a read costs the streams of its own thing and not the size of the State.
#[cfg(test)]
thread_local! {
    pub(super) static STREAMS_EXAMINED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

impl State {
    /// **Who backs the value each key of a thing holds**, as the store's
    /// `backing` answers it. A key whose writes are summed, and a key a later
    /// write cleared, are absent.
    pub fn backing(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, FieldBacking> {
        memory::folded_backing(&self.key_writes(subject), declared)
    }

    /// **When the store took in the write that holds each key's value now**, the
    /// `keys` of the store's `field_ages`. A key whose holding write carries no
    /// moment is absent.
    pub fn held_since(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, Timestamp> {
        memory::folded_ages(&self.key_writes(subject), declared)
    }
}

impl State {
    /// **Every write of a thing's own row, and of every thing folded into it**,
    /// in the order they were made. Found from the thing and what was folded
    /// into it alone.
    pub fn entity_versions(&self, subject: &ObjectId) -> Vec<&ObjectVersion> {
        let subject = self.survivor(subject);
        let mut versions: Vec<&ObjectVersion> = std::iter::once(subject)
            .chain(self.folded_under.get(subject).into_iter().flatten())
            .filter_map(|object| self.versions.get(object))
            .flatten()
            .collect();
        versions.sort_by_key(|version| version.seq);
        versions
    }

    /// **Every write of one claim, oldest first**, the last being the claim now.
    /// A claim the State does not hold has none.
    pub fn claim_history(&self, record: &RecordId) -> &[Revision] {
        self.records
            .get(record)
            .map_or(&[], |held| held.history.as_slice())
    }

    /// **The writes of every claim homed on a thing**, each claim by its
    /// identity. Found from the claims of that thing alone.
    pub fn claim_histories(&self, subject: &ObjectId) -> BTreeMap<&RecordId, &[Revision]> {
        self.records_homed_on(subject)
            .into_iter()
            .map(|record| (record, self.claim_history(record)))
            .collect()
    }

    /// **Every write on a thing, and the records they were written in**, as the
    /// store's `key_trails` answers it: a key's trail is its writes that reach
    /// the thing, a key that stays on its record keeps its writes with their
    /// moments, and a record's key is kept off the thing when its newest write
    /// of that key, unless a later one took the key off, was made under a class
    /// that does not reach it. A write whose claim the State does not hold is
    /// left out. The home of an address is the thing's permanent key.
    pub fn key_trails(&self, subject: &ObjectId) -> KeyTrails {
        let subject = self.survivor(subject);
        let mut writes: Vec<(&KeyId, &Write)> = self
            .streams_of(subject)
            .flat_map(|(key, stream)| stream.iter().map(move |write| (key, write)))
            .collect();
        writes.sort_by_key(|(_, write)| write.ordinal);
        let mut trails = KeyTrails::default();
        let mut held: BTreeMap<(FactId, String), WriteClass> = BTreeMap::new();
        for (key, write) in writes {
            let Some(record) = self.records.get(&write.record) else {
                continue;
            };
            let carried = FactId(record.claim.label.clone());
            let value = match &write.write {
                FieldValue::Set(value) => Some(value.clone()),
                FieldValue::Clear => None,
            };
            match &value {
                Some(_) => {
                    held.insert((carried.clone(), key.0.clone()), write.class);
                }
                None => {
                    held.remove(&(carried.clone(), key.0.clone()));
                }
            }
            if !memory::reaches_the_thing(write.class) {
                trails.record_writes.push(RecordFieldWrite {
                    fact: carried,
                    key: key.0.clone(),
                    ordinal: write.ordinal,
                    value,
                    written_at: write.written_at,
                });
                continue;
            }
            trails
                .history
                .entry(key.0.clone())
                .or_default()
                .push(FieldWrite {
                    written_at: write.written_at,
                    recorded_by: record.claim.app.clone(),
                    writer: write.writer.clone(),
                    run: write.run.as_ref().map(|run| run.0.clone()),
                    value,
                    fact: FactAddress::new(EntityId(record.claim.home.0.clone()), carried),
                    recorded_at: record.claim.recorded_at,
                    status: record.claim.status,
                    provenance: record.claim.provenance,
                    standing: record.claim.standing,
                    note: record.claim.details.clone(),
                });
        }
        for ((fact, key), class) in held {
            if !memory::reaches_the_thing(class) {
                trails.kept_off.entry(fact).or_default().insert(key);
            }
        }
        trails
            .record_writes
            .sort_by(|a, b| (&a.key, a.ordinal).cmp(&(&b.key, b.ordinal)));
        trails
    }
}

#[cfg(test)]
mod histories_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trails_tests;
#[cfg(test)]
mod versions_tests;
