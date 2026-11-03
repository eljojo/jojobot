//! **Claims, fields and identities as events**: what each event means, and what
//! the state is after any sequence of them. Pure: no store, and nothing here
//! emits an event.
//!
//! **Who wrote an event, and in which run, lives on the envelope**
//! ([`Event`]), never in the payload. The interpreter reads the actor and the
//! run from the envelope it is given; `None` stays only where the envelope
//! holds `None`, an imported write whose row named no run.
//!
//! Every payload carries complete decided values. [`State::apply`] is the one
//! interpreter: it keeps what an event says and checks nothing against today's
//! policy, so replaying an old event never re-admits it under a newer floor.
//! A change of meaning is a new kind of event or a dated version, never a second
//! reading of an old one.
//!
//! **The projection shares the fold.** [`State::fields`] hands the writes it
//! holds to [`memory::folded_fields`], the one fold; nothing in this module
//! restates which writes count or how a key resolves.
//!
//! **No renewal kind.** A role renewal is a heartbeat kept off the log.

use std::collections::{BTreeMap, BTreeSet};

use jiff::{Timestamp, civil::Date};

use super::{Event, ObjectId, RecordId, RunId};
use crate::actor::Actor;
use crate::app::AppStamp;
use crate::memory::{
    self, FactId, FactStatus, KeyWrite, Provenance, Standing, WriteClass, types::DeclaredType,
};

/// **The identity of a key across every thing that holds it.** The text a caller
/// typed is a spelling of it; the key a thing holds is this.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KeyId(pub String);

/// The identity of a kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KindId(pub String);

/// **What one write of a key did.** A clear is a write: it takes the key off the
/// thing until a later write puts it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Set(String),
    Clear,
}

/// **Everything an object is, complete when written.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectBody {
    pub kind: KindId,
    pub parent: Option<ObjectId>,
    pub name: String,
    pub source: String,
    pub aliases: Vec<String>,
    pub boot: Option<String>,
    pub metadata: BTreeMap<String, String>,
    /// Archive and restore are revisions of the object.
    pub archived: bool,
}

/// **Everything a claim says, complete when written.** `label` is the address
/// binding (`fN`) and never the claim's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimBody {
    pub label: String,
    pub home: ObjectId,
    pub content: String,
    pub details: Option<String>,
    pub provenance: Provenance,
    pub standing: Standing,
    pub status: FactStatus,
    pub recorded_at: Date,
    pub happened_at: Option<Date>,
    pub happened_through: Option<Date>,
    pub stale_after: Option<Date>,
    /// The edge's shape and the object it points at.
    pub edge: Option<(String, ObjectId)>,
    pub derived_from: Option<RecordId>,
    pub refs: Vec<ObjectId>,
    pub stands_for: Vec<RecordId>,
    /// The receipt as the store keeps it, settled claims only.
    pub receipt: Option<String>,
    pub app: Option<AppStamp>,
    pub inserted_at: Option<Timestamp>,
}

/// **A claim a merge moved**, and the label it is served under after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedRecord {
    pub record: RecordId,
    pub label: String,
}

/// **A write a merge moved**, and the place it takes in the survivor's stream
/// of the key. The interleave is decided when the merge is written; the
/// interpreter never re-sorts by position in the log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovedWrite {
    pub record: RecordId,
    pub key: KeyId,
    pub from_ordinal: u64,
    pub ordinal: u64,
}

/// One fact about claims, fields or identities, complete when written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaimEvent {
    ObjectCreated {
        object: ObjectId,
        body: ObjectBody,
    },
    ObjectRevised {
        object: ObjectId,
        body: ObjectBody,
    },
    /// A spelling now answers to the object.
    AddressBound {
        object: ObjectId,
        address: String,
    },
    /// A spelling no longer answers to the object.
    AddressRetired {
        object: ObjectId,
        address: String,
    },
    /// The folded object's claims and writes now belong to the survivor, and
    /// its spellings answer to the survivor.
    ObjectsMerged {
        folded: ObjectId,
        survivor: ObjectId,
        records: Vec<MovedRecord>,
        writes: Vec<MovedWrite>,
    },
    RecordCreated {
        record: RecordId,
        claim: ClaimBody,
    },
    /// The claim after a revision, complete. Status changes, archive and
    /// retract are revisions.
    RecordRevised {
        record: RecordId,
        claim: ClaimBody,
    },
    /// One key written on one subject by one claim. `ordinal` is the place in
    /// the (subject, key) stream, from one. `targets` are the objects the value
    /// pointed at when it was written.
    FieldWritten {
        record: RecordId,
        subject: ObjectId,
        key: KeyId,
        ordinal: u64,
        class: WriteClass,
        write: FieldValue,
        targets: Vec<ObjectId>,
    },
}

/// What the state holds of one claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub claim: ClaimBody,
    /// The actor on the envelope of the event that created the claim; a
    /// revision never replaces it.
    pub first_writer: Option<Actor>,
    pub revisions: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Write {
    record: RecordId,
    ordinal: u64,
    class: WriteClass,
    write: FieldValue,
    targets: Vec<ObjectId>,
    /// The actor and run on the envelope of the event that made this write.
    writer: Option<Actor>,
    run: Option<RunId>,
    /// When the store took the write in: the source's moment for an import,
    /// else the envelope's own. `None` stays `None`; the claim's moment is
    /// never put in its place.
    written_at: Option<Timestamp>,
}

/// **What the events so far say.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct State {
    objects: BTreeMap<ObjectId, ObjectBody>,
    addresses: BTreeMap<String, ObjectId>,
    redirects: BTreeMap<ObjectId, ObjectId>,
    records: BTreeMap<RecordId, Record>,
    streams: BTreeMap<(ObjectId, KeyId), Vec<Write>>,
}

impl State {
    /// **The interpreter.** Structural only. It reads the actor and the run of a
    /// write from the envelope.
    pub fn apply(&mut self, event: &Event<ClaimEvent>) {
        match &event.payload {
            ClaimEvent::ObjectCreated { object, body }
            | ClaimEvent::ObjectRevised { object, body } => {
                self.objects.insert(object.clone(), body.clone());
            }
            ClaimEvent::AddressBound { object, address } => {
                self.addresses.insert(address.clone(), object.clone());
            }
            ClaimEvent::AddressRetired { object, address } => {
                if self.addresses.get(address) == Some(object) {
                    self.addresses.remove(address);
                }
            }
            ClaimEvent::ObjectsMerged {
                folded,
                survivor,
                records,
                writes,
            } => self.merge(folded, survivor, records, writes),
            ClaimEvent::RecordCreated { record, claim } => {
                self.records.insert(
                    record.clone(),
                    Record {
                        claim: claim.clone(),
                        first_writer: event.actor.clone(),
                        revisions: 0,
                    },
                );
            }
            ClaimEvent::RecordRevised { record, claim } => {
                if let Some(held) = self.records.get_mut(record) {
                    held.claim = claim.clone();
                    held.revisions += 1;
                }
            }
            ClaimEvent::FieldWritten {
                record,
                subject,
                key,
                ordinal,
                class,
                write,
                targets,
            } => {
                let stream = self
                    .streams
                    .entry((subject.clone(), key.clone()))
                    .or_default();
                stream.push(Write {
                    record: record.clone(),
                    ordinal: *ordinal,
                    class: *class,
                    write: write.clone(),
                    targets: targets.clone(),
                    writer: event.actor.clone(),
                    run: event.run.clone(),
                    written_at: event.imported_source_time.or(event.written_at),
                });
            }
        }
    }

    fn merge(
        &mut self,
        folded: &ObjectId,
        survivor: &ObjectId,
        records: &[MovedRecord],
        writes: &[MovedWrite],
    ) {
        self.redirects.insert(folded.clone(), survivor.clone());
        for address in self.addresses.values_mut() {
            if address == folded {
                *address = survivor.clone();
            }
        }
        for moved in records {
            if let Some(held) = self.records.get_mut(&moved.record) {
                held.claim.home = survivor.clone();
                held.claim.label = moved.label.clone();
            }
        }
        let keys: Vec<(ObjectId, KeyId)> = self
            .streams
            .keys()
            .filter(|(subject, _)| subject == folded)
            .cloned()
            .collect();
        for (subject, key) in keys {
            let Some(stream) = self.streams.remove(&(subject, key.clone())) else {
                continue;
            };
            let target = self
                .streams
                .entry((survivor.clone(), key.clone()))
                .or_default();
            for mut write in stream {
                if let Some(moved) = writes.iter().find(|moved| {
                    moved.record == write.record
                        && moved.key == key
                        && moved.from_ordinal == write.ordinal
                }) {
                    write.ordinal = moved.ordinal;
                }
                target.push(write);
            }
        }
    }

    /// The state after `events`, from nothing.
    pub fn replay<'a>(events: impl IntoIterator<Item = &'a Event<ClaimEvent>>) -> Self {
        let mut state = Self::default();
        for event in events {
            state.apply(event);
        }
        state
    }

    /// **The object a spelling answers to**, following a merge to its survivor.
    pub fn resolve(&self, address: &str) -> Option<&ObjectId> {
        self.addresses
            .get(address)
            .map(|object| self.survivor(object))
    }

    /// The object an identity stands for now: itself, or its merge survivor.
    pub fn survivor<'a>(&'a self, object: &'a ObjectId) -> &'a ObjectId {
        let mut at = object;
        while let Some(next) = self.redirects.get(at) {
            at = next;
        }
        at
    }

    pub fn object(&self, object: &ObjectId) -> Option<&ObjectBody> {
        self.objects.get(self.survivor(object))
    }

    pub fn record(&self, record: &RecordId) -> Option<&Record> {
        self.records.get(record)
    }

    /// The claims homed on an object whose status is active, in identity order.
    pub fn active_records(&self, home: &ObjectId) -> Vec<&RecordId> {
        let home = self.survivor(home);
        self.records
            .iter()
            .filter(|(_, held)| {
                held.claim.status == FactStatus::Active && self.survivor(&held.claim.home) == home
            })
            .map(|(id, _)| id)
            .collect()
    }

    /// **The writes of a subject, as the fold reads them**: every write that
    /// reaches the thing, with its claim's standing.
    fn key_writes(&self, subject: &ObjectId) -> Vec<KeyWrite> {
        let subject = self.survivor(subject);
        let mut out = Vec::new();
        for ((held, key), stream) in &self.streams {
            if held != subject {
                continue;
            }
            for write in stream {
                if !memory::reaches_the_thing(write.class) {
                    continue;
                }
                let Some(record) = self.records.get(&write.record) else {
                    continue;
                };
                out.push(KeyWrite {
                    key: key.0.clone(),
                    ordinal: write.ordinal,
                    value: match &write.write {
                        FieldValue::Set(value) => Some(value.clone()),
                        FieldValue::Clear => None,
                    },
                    fact: FactId(record.claim.label.clone()),
                    status: record.claim.status,
                    provenance: record.claim.provenance,
                    standing: record.claim.standing,
                    note: record.claim.details.clone(),
                    recorded_by: record.claim.app.clone(),
                    writer: write.writer.clone(),
                    run: write.run.as_ref().map(|run| run.0.clone()),
                    written_at: write.written_at,
                });
            }
        }
        out
    }

    /// **What the thing holds**: the one fold over the writes of its active
    /// claims that reach it.
    pub fn fields(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, String> {
        memory::folded_fields(&self.key_writes(subject), declared)
    }

    /// **The objects a thing points at now**: the frozen targets of the write
    /// holding each of its keys, a cleared or retracted key pointing at none.
    pub fn links_from(&self, subject: &ObjectId) -> BTreeSet<ObjectId> {
        let subject = self.survivor(subject);
        let mut out = BTreeSet::new();
        for ((held, _), stream) in &self.streams {
            if held != subject {
                continue;
            }
            // The stream is kept in arrival order; the ordinal says which is newest.
            let newest = stream
                .iter()
                .filter(|write| {
                    memory::reaches_the_thing(write.class)
                        && self
                            .records
                            .get(&write.record)
                            .is_some_and(|record| record.claim.status == FactStatus::Active)
                })
                .max_by_key(|write| write.ordinal);
            if let Some(write) = newest
                && write.write != FieldValue::Clear
            {
                out.extend(
                    write
                        .targets
                        .iter()
                        .map(|target| self.survivor(target).clone()),
                );
            }
        }
        out
    }

    /// The things that point at an object now.
    pub fn links_into(&self, target: &ObjectId) -> BTreeSet<ObjectId> {
        let target = self.survivor(target);
        let subjects: BTreeSet<&ObjectId> =
            self.streams.keys().map(|(subject, _)| subject).collect();
        subjects
            .into_iter()
            .filter(|subject| self.links_from(subject).contains(target))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests;
