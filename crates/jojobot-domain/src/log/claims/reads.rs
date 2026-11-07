//! **What the fold re-reads after every write, answered from the State.** Each
//! read is one function of the writes the State already holds for one thing, so
//! it never scans the claims of any other thing. A read here is the same fold
//! the store's read runs ([`memory::folded_backing`], [`memory::folded_ages`]):
//! the value and what stands behind it cannot come from two passes that disagree.

use std::collections::BTreeMap;

use jiff::Timestamp;

use super::{FieldValue, KeyId, ObjectBody, ObjectVersion, Record, Revision, State, Write};
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

/// **One thing as the store's scan serves it, in the State's own terms**: the
/// object as it stands, the claims homed on it, what it holds, and the objects
/// the held values point at. The prose of the page, the store's document id and
/// the owner are not in the State; the shell supplies them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThingScan<'a> {
    pub object: ObjectId,
    pub body: &'a ObjectBody,
    /// The claims homed on the thing, every status, ordered by the number of
    /// the label and then the label.
    pub claims: Vec<(&'a RecordId, &'a Record)>,
    /// **The claims a merge left under the home of a folded thing and listed in
    /// no moved record**: the thing cannot serve them as its own, and they are
    /// named here, never dropped. They may share a label with a claim above.
    pub unmapped: Vec<(&'a RecordId, &'a Record)>,
    pub fields: BTreeMap<String, String>,
    /// For each held key, the objects its holding write pointed at when it was
    /// written, followed to their merge survivors. A key with none is absent.
    pub field_links: BTreeMap<String, Vec<ObjectId>>,
}

impl State {
    /// **The targets of the write holding each key of a thing**, from the
    /// streams of that thing alone: the newest write that reaches the thing from
    /// an active claim, a cleared key pointing at none.
    fn held_targets(&self, subject: &ObjectId) -> BTreeMap<String, Vec<ObjectId>> {
        let mut out = BTreeMap::new();
        for (key, stream) in self.streams_of(subject) {
            if let Some(write) = self.holding_write(stream)
                && !write.targets.is_empty()
            {
                out.insert(key.0.clone(), self.survivors_of(&write.targets));
            }
        }
        out
    }

    /// **What a thing holds, as a read serves it**: every item of a value that
    /// wears the mark of a stored handle is composed to the handle its thing
    /// answers to now, each thing named once; text that wears no mark is served
    /// as written. A mark naming a thing with no handle is served as the id it
    /// names, mark off, which is what the store serves.
    pub fn fields_served(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, String> {
        let mut fields = self.fields(subject, declared);
        for (key, value) in fields.clone() {
            let items: Vec<&str> = value.split(',').map(str::trim).collect();
            if !items
                .iter()
                .any(|item| memory::stored_handle_id(item).is_some())
            {
                continue;
            }
            let composed = items
                .into_iter()
                .map(|item| {
                    // A mark naming a thing with no handle is served as the id it
                    // names, mark off, as the store serves it.
                    let handle = memory::stored_handle_id(item).map(|id| {
                        self.handle_of(&ObjectId(id.to_string()))
                            .unwrap_or_else(|| EntityId(id.to_string()))
                    });
                    let text = handle
                        .as_ref()
                        .map_or_else(|| item.to_string(), |handle| handle.to_string());
                    (handle, text)
                })
                .collect();
            fields.insert(key, memory::list::serve_values(composed));
        }
        fields
    }

    /// **A thing as its scan serves it**, from that thing's own objects, claims
    /// and streams. A thing the State does not hold has none.
    pub fn scan(&self, subject: &ObjectId, declared: &[DeclaredType]) -> Option<ThingScan<'_>> {
        let subject = self.survivor(subject);
        let body = self.objects.get(subject)?;
        let (mut claims, mut unmapped): (Vec<_>, Vec<_>) = self
            .records_homed_on(subject)
            .into_iter()
            .filter_map(|id| self.records.get(id).map(|held| (id, held)))
            .partition(|(_, held)| held.claim.home == *subject);
        for list in [&mut claims, &mut unmapped] {
            list.sort_by_key(|(id, held)| {
                (
                    memory::numbered::whole_number(held.claim.label.trim_start_matches('f')),
                    held.claim.label.clone(),
                    (*id).clone(),
                )
            });
        }
        Some(ThingScan {
            object: subject.clone(),
            body,
            claims,
            unmapped,
            fields: self.fields_served(subject, declared),
            field_links: self.held_targets(subject),
        })
    }

    /// **The marker of the writes behind a thing's fields**: how many writes of
    /// its claims and of its own row it holds, folded things included. It only
    /// grows, so a slower read of an earlier state cannot beat one already
    /// answering for a later state. A lease renewal is a heartbeat kept off the
    /// log and adds none.
    pub fn write_marker(&self, subject: &ObjectId) -> u64 {
        let claim_writes: usize = self
            .records_homed_on(subject)
            .into_iter()
            .map(|record| self.claim_history(record).len())
            .sum();
        (claim_writes + self.entity_versions(subject).len()) as u64
    }

    /// **The newest moment any write on a thing was taken in**: its own row, and
    /// every write of every claim homed on it, of any status, and every write
    /// of a key on it, whatever its class. What is no change of the thing is
    /// decided by the domain's one rule ([`memory::review::counts_as_a_change`]):
    /// a bookkeeping key, and any write of a claim that carries the ledger of
    /// release notes. A write with no moment counts for none.
    pub fn newest_change(&self, subject: &ObjectId) -> Option<Timestamp> {
        let subject = self.survivor(subject);
        let streams: Vec<(&KeyId, &Vec<Write>)> = self.streams_of(subject).collect();
        let ledgers = memory::review::ledger_records(streams.iter().flat_map(|(key, stream)| {
            stream.iter().map(|write| {
                let value = match &write.write {
                    FieldValue::Set(value) => Some(value.as_str()),
                    FieldValue::Clear | FieldValue::Removed => None,
                };
                (key.0.as_str(), value, write.record.clone())
            })
        }));
        let own = self
            .entity_versions(subject)
            .into_iter()
            .filter_map(|version| version.written_at);
        let fields = streams
            .iter()
            .flat_map(|(key, stream)| stream.iter().map(move |write| (key, write)))
            .filter(|(key, write)| {
                memory::review::counts_as_a_change(Some(&key.0), &write.record, &ledgers)
            })
            .filter_map(|(_, write)| write.written_at);
        let claimed = self
            .records_homed_on(subject)
            .into_iter()
            .filter(|record| memory::review::counts_as_a_change(None, *record, &ledgers))
            .filter_map(|record| self.records.get(record))
            .flat_map(|held| held.history.iter().filter_map(|write| write.written_at));
        own.chain(fields).chain(claimed).max()
    }

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
                FieldValue::Clear | FieldValue::Removed => None,
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
mod marker_tests;
#[cfg(test)]
mod scan_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod trails_tests;
#[cfg(test)]
mod versions_tests;
