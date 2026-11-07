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
use crate::app::{AppId, AppStamp};
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
    /// **A stored clear, as the import reads it** (rule 677). Every clear the
    /// store holds took the key off the thing, whichever claim held the value, and
    /// that meaning does not change. A live clear is [`Self::Clear`].
    Removed,
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
    /// **The app that owns the object.** Structural: only an event that binds
    /// sets it, and no write of a key moves or drops it.
    pub bound_to: Option<AppId>,
}

/// **What one write of an object's own row left, as that write recorded it.**
/// The columns of a stored snapshot, decoded: `parent` and `merged_into` are the
/// permanent keys they name. `merged_into` is history only; identity is decided
/// by the merge events and nothing else.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectSnapshot {
    pub kind: String,
    pub name: String,
    pub source: String,
    pub crm: Option<String>,
    pub parent: Option<ObjectId>,
    pub boot: String,
    pub archived_reason: Option<String>,
    pub archived_at: Option<String>,
    pub merged_into: Option<ObjectId>,
    pub aliases: Vec<String>,
    pub prose: String,
}

/// **One write of an object's own row that an import observed**: where it was
/// filed, which write of that stream it was, and what it left. A write an older
/// build recorded with no snapshot has none: the snapshot is unknown, and nothing
/// stands in for it. The moment and the run are the envelope's, as for any other
/// imported write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedWrite {
    /// The handle the row was filed under when it was written.
    pub filed_under: String,
    pub ordinal: u64,
    pub snapshot: Option<ObjectSnapshot>,
    pub run: Option<RunId>,
    pub source_time: Option<Timestamp>,
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
    /// The receipt, validated, settled claims only.
    pub receipt: Option<crate::memory::Receipt>,
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
    /// **An object as an import observed it at the cut.** The state it serves
    /// from then on, and never a version of it: "observed in this state at
    /// activation", not "this was created then".
    ObjectBaselined {
        object: ObjectId,
        body: ObjectBody,
    },
    /// **A write of an object's own row that an import observed.** A historical
    /// observation, never shown as a newly performed act, and it changes no
    /// object: the baseline is what the object is.
    ObjectWriteObserved {
        object: ObjectId,
        filed_under: String,
        ordinal: u64,
        snapshot: Option<ObjectSnapshot>,
    },
    /// A spelling now answers to the object.
    AddressBound {
        object: ObjectId,
        address: String,
    },
    /// A spelling no longer answers to the object. `day` is the day the spelling
    /// stopped being current: the caller's own day, which no instant yields.
    AddressRetired {
        object: ObjectId,
        address: String,
        day: Date,
    },
    /// The folded object's claims and writes now belong to the survivor, and
    /// its spellings answer to the survivor.
    ObjectsMerged {
        folded: ObjectId,
        survivor: ObjectId,
        records: Vec<MovedRecord>,
        writes: Vec<MovedWrite>,
    },
    /// **A merge the store holds no number for**: the mark on the folded entity
    /// is all that is left of a merge made before the merge-event table. The
    /// folded object answers to the survivor as in [`Self::ObjectsMerged`], and
    /// the merge count does not move, so the count stays the highest number the
    /// store holds.
    ObjectRedirected {
        folded: ObjectId,
        survivor: ObjectId,
    },
    /// **The import ran to its end.** The last member of the import's batch, and
    /// the only member of it when the store held nothing to map, so a log that
    /// holds this event holds a completed import. `events` is how many events the
    /// mapping made before it. It changes no state.
    ImportCompleted {
        events: u64,
    },
    /// **Merge numbers the store never used**: a gap in the stored numbers that no
    /// merge row and no merge mark explains. The numbers are never reissued, so
    /// the highest merge number counts them.
    MergeNumbersLeftEmpty {
        count: u64,
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
    /// **A key a stored clear took off the thing.** The import maps every stored
    /// clear to this event, whose meaning is fixed: the key leaves the thing,
    /// whichever claim held its value, and the clearing claim's own write of it is
    /// withdrawn. Live writes never emit it. Places in the stream as
    /// [`Self::FieldWritten`] does.
    FieldRemoved {
        record: RecordId,
        subject: ObjectId,
        key: KeyId,
        ordinal: u64,
        class: WriteClass,
        targets: Vec<ObjectId>,
    },
    /// A type is declared, or declared again. Each declaration is one event and
    /// the earlier ones stay in the history; the newest is the one in force.
    TypeDeclared {
        kind: ObjectId,
        declaration: DeclaredType,
    },
    /// **The store no longer holds a declaration for the kind.** The state drops
    /// every declaration it held for it, so a read of the kind answers as the
    /// store's rows do.
    TypeWithdrawn {
        kind: ObjectId,
    },
}

/// What the state holds of one claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    pub claim: ClaimBody,
    /// The actor on the envelope of the event that created the claim; a
    /// revision never replaces it.
    pub first_writer: Option<Actor>,
    /// The run on the envelope of the event that created the claim, for a
    /// writer no bot is named for: the run says which bot wrote it.
    pub first_run: Option<RunId>,
    pub revisions: u32,
    /// **Every write of the claim, oldest first**: the claim as the creating
    /// event and each revising event left it. The last is the claim now.
    pub history: Vec<Revision>,
    /// When the store took in the newest event that created or revised the
    /// claim, by the same rule as [`Write::written_at`]. The store counts every
    /// write of a claim as a touch, whether or not it wrote a field.
    pub written_at: Option<Timestamp>,
}

/// **One write of one claim**: the claim complete as that write left it, and
/// what the envelope of the event says of the write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revision {
    pub claim: ClaimBody,
    /// The actor on the envelope of the event that made this write.
    pub actor: Option<Actor>,
    /// The run on the envelope of the event that made this write.
    pub run: Option<RunId>,
    /// When the store took the write in, by the rule of [`Record::written_at`].
    pub written_at: Option<Timestamp>,
}

/// **One write of an object's own row**: the object complete as that write left
/// it, and what the envelope says of the write. `seq` is the place of the write
/// among all object writes, so the versions of an object and of the things
/// folded into it read in the order they were made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObjectVersion {
    pub seq: u64,
    /// The object that was written, which is not the one asked about when the
    /// write was made before a merge folded it in.
    pub thing: ObjectId,
    pub body: ObjectBody,
    pub actor: Option<Actor>,
    pub run: Option<RunId>,
    pub written_at: Option<Timestamp>,
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
    /// The spellings an object answered to once and no longer does, for the
    /// claim lookup. It is read only after the current spellings, so a spelling
    /// bound again answers to its new object.
    retired_addresses: BTreeMap<String, ObjectId>,
    records: BTreeMap<RecordId, Record>,
    streams: BTreeMap<(ObjectId, KeyId), Vec<Write>>,
    /// Every declaration of each kind, oldest first.
    declarations: BTreeMap<ObjectId, Vec<DeclaredType>>,
    /// Every observed write of each object's own row, in the order the events
    /// arrived, under the object that was written.
    observed: BTreeMap<ObjectId, Vec<ObservedWrite>>,
    /// Every write of each object's own row, oldest first, under the object
    /// that was written.
    versions: BTreeMap<ObjectId, Vec<ObjectVersion>>,
    /// Every object folded into each survivor, at any depth.
    folded_under: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    version_seq: u64,
    indexes: indexes::Indexes,
}

impl State {
    /// **The interpreter.** Structural only. It reads the actor and the run of a
    /// write from the envelope.
    pub fn apply(&mut self, event: &Event<ClaimEvent>) {
        match &event.payload {
            // **The current state at the cut serves the object and is never a
            // version of it** (the baseline of an import).
            ClaimEvent::ObjectBaselined { object, body } => {
                let before = self.objects.get(object).map(|held| held.parent.clone());
                self.objects.insert(object.clone(), body.clone());
                self.index_object(object, before, &body.parent);
            }
            ClaimEvent::ObjectCreated { object, body }
            | ClaimEvent::ObjectRevised { object, body } => {
                let before = self.objects.get(object).map(|held| held.parent.clone());
                self.version_seq += 1;
                self.versions
                    .entry(object.clone())
                    .or_default()
                    .push(ObjectVersion {
                        seq: self.version_seq,
                        thing: object.clone(),
                        body: body.clone(),
                        actor: event.actor.clone(),
                        run: event.run.clone(),
                        written_at: event.imported_source_time.or(event.written_at),
                    });
                self.objects.insert(object.clone(), body.clone());
                self.index_object(object, before, &body.parent);
            }
            ClaimEvent::ObjectWriteObserved {
                object,
                filed_under,
                ordinal,
                snapshot,
            } => self
                .observed
                .entry(object.clone())
                .or_default()
                .push(ObservedWrite {
                    filed_under: filed_under.clone(),
                    ordinal: *ordinal,
                    snapshot: snapshot.clone(),
                    run: event.run.clone(),
                    source_time: event.imported_source_time,
                }),
            ClaimEvent::AddressBound { object, address } => {
                if let Some(held) = self.addresses.insert(address.clone(), object.clone()) {
                    self.unbind_spelling(&held, address);
                }
                self.bind_spelling(object, address);
            }
            ClaimEvent::AddressRetired {
                object, address, ..
            } => {
                if self.addresses.get(address) == Some(object) {
                    self.addresses.remove(address);
                    self.unbind_spelling(object, address);
                    self.retired_addresses
                        .insert(address.clone(), object.clone());
                }
            }
            ClaimEvent::ObjectsMerged {
                folded,
                survivor,
                records,
                writes,
            } => self.merge(folded, survivor, records, writes, true),
            ClaimEvent::ObjectRedirected { folded, survivor } => {
                self.merge(folded, survivor, &[], &[], false)
            }
            ClaimEvent::MergeNumbersLeftEmpty { count } => self.leave_merge_numbers_empty(*count),
            ClaimEvent::ImportCompleted { .. } => {}
            ClaimEvent::RecordCreated { record, claim } => {
                let before = self.records.get(record).map(|held| held.claim.clone());
                self.records.insert(
                    record.clone(),
                    Record {
                        claim: claim.clone(),
                        first_writer: event.actor.clone(),
                        first_run: event.run.clone(),
                        revisions: 0,
                        history: vec![Revision {
                            claim: claim.clone(),
                            actor: event.actor.clone(),
                            run: event.run.clone(),
                            written_at: event.imported_source_time.or(event.written_at),
                        }],
                        written_at: event.imported_source_time.or(event.written_at),
                    },
                );
                self.index_record(record, before.as_ref(), claim);
            }
            ClaimEvent::RecordRevised { record, claim } => {
                if let Some(held) = self.records.get_mut(record) {
                    let before = std::mem::replace(&mut held.claim, claim.clone());
                    held.revisions += 1;
                    held.written_at = event.imported_source_time.or(event.written_at);
                    held.history.push(Revision {
                        claim: claim.clone(),
                        actor: event.actor.clone(),
                        run: event.run.clone(),
                        written_at: held.written_at,
                    });
                    self.index_record(record, Some(&before), claim);
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
                self.index_write(record, subject, key, *ordinal);
            }
            ClaimEvent::FieldRemoved {
                record,
                subject,
                key,
                ordinal,
                class,
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
                    write: FieldValue::Removed,
                    targets: targets.clone(),
                    writer: event.actor.clone(),
                    run: event.run.clone(),
                    written_at: event.imported_source_time.or(event.written_at),
                });
                self.index_write(record, subject, key, *ordinal);
            }
            ClaimEvent::TypeDeclared { kind, declaration } => {
                self.declarations
                    .entry(kind.clone())
                    .or_default()
                    .push(declaration.clone());
            }
            ClaimEvent::TypeWithdrawn { kind } => {
                self.declarations.remove(kind);
            }
        }
    }

    /// **The declaration of a kind in force now**: the newest one.
    pub fn declared(&self, kind: &ObjectId) -> Option<&DeclaredType> {
        self.declarations.get(kind).and_then(|held| held.last())
    }

    /// Every declaration of a kind, oldest first.
    pub fn declarations_of(&self, kind: &ObjectId) -> &[DeclaredType] {
        self.declarations.get(kind).map_or(&[], Vec::as_slice)
    }

    fn merge(
        &mut self,
        folded: &ObjectId,
        survivor: &ObjectId,
        records: &[MovedRecord],
        writes: &[MovedWrite],
        numbered: bool,
    ) {
        // A redirect that closes a loop would make `survivor` spin forever.
        if self.survivor(survivor) == self.survivor(folded) {
            return;
        }
        // **The survivor is the end of its own chain.** A mark that folds into a thing
        // already folded elsewhere arrives before the mark that folded that thing when
        // the import orders marks by entity row, and everything filed under the middle
        // thing would be unreachable: reads look under the end of the chain only.
        let survivor = &self.survivor(survivor).clone();
        self.index_before_merge(records);
        self.redirects.insert(folded.clone(), survivor.clone());
        let mut under = self.folded_under.remove(folded).unwrap_or_default();
        under.insert(folded.clone());
        self.folded_under
            .entry(survivor.clone())
            .or_default()
            .extend(under);
        for address in self.addresses.values_mut() {
            if address == folded {
                *address = survivor.clone();
            }
        }
        self.move_spellings(folded, survivor);
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
            let folded_key = (subject.clone(), key.clone());
            let survivor_key = (survivor.clone(), key.clone());
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
            let highest = target.iter().map(|write| write.ordinal).max();
            self.indexes
                .moved_stream(&folded_key, &survivor_key, highest);
        }
        self.index_after_merge(folded, survivor, records, numbered);
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

    /// **The spellings an object answers to now**, in order.
    pub fn addresses_of(&self, object: &ObjectId) -> Vec<String> {
        self.addresses
            .iter()
            .filter(|(_, held)| *held == object)
            .map(|(address, _)| address.clone())
            .collect()
    }

    pub fn object(&self, object: &ObjectId) -> Option<&ObjectBody> {
        self.objects.get(self.survivor(object))
    }

    /// **The writes of an object's own row that an import observed**, under that
    /// object and in the order they arrived. A thing folded into another keeps its
    /// own; they are read under the key they were written for.
    pub fn observed_writes(&self, object: &ObjectId) -> &[ObservedWrite] {
        self.observed.get(object).map_or(&[], Vec::as_slice)
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
        for (key, stream) in self.streams_of(subject) {
            self.push_key_writes(key, stream, &mut out);
        }
        out
    }

    /// **The streams of one subject**, found by range: they sit together in the
    /// map, ordered by subject and then key, so a read never scans the streams
    /// of other things. The subject is already the survivor of any merge.
    fn streams_of<'a>(
        &'a self,
        subject: &'a ObjectId,
    ) -> impl Iterator<Item = (&'a KeyId, &'a Vec<Write>)> {
        self.streams
            .range((subject.clone(), KeyId(String::new()))..)
            // Counted where a stream is first looked at, before any filter, so
            // a scan of the whole map cannot hide from the case that counts.
            .inspect(|_| {
                #[cfg(test)]
                reads::STREAMS_EXAMINED.with(|n| n.set(n.get() + 1));
            })
            .take_while(move |((held, _), _)| held == subject)
            .map(|((_, key), stream)| (key, stream))
    }

    /// The writes of one key of a subject, as [`Self::key_writes`] reads them.
    fn key_writes_of(&self, subject: &ObjectId, key: &KeyId) -> Vec<KeyWrite> {
        let mut out = Vec::new();
        if let Some(stream) = self
            .streams
            .get(&(self.survivor(subject).clone(), key.clone()))
        {
            self.push_key_writes(key, stream, &mut out);
        }
        out
    }

    fn push_key_writes(&self, key: &KeyId, stream: &[Write], out: &mut Vec<KeyWrite>) {
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
                    FieldValue::Clear | FieldValue::Removed => None,
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

    /// **What the thing holds**: the one fold over the writes of its active
    /// claims that reach it.
    pub fn fields(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, String> {
        memory::folded_fields(&self.key_writes(subject), declared)
    }
}

pub mod entity_change;
pub mod import;
mod indexes;
mod prepare;
mod reads;
mod rules;
pub mod type_change;
pub use indexes::Located;
pub use prepare::{
    Admitted, BootFloor, CaptureRequest, ClaimSize, Door, Draft, Measures, Refusal, Route, prepare,
};
pub use rules::{HeldRecord, StarredRule};

#[cfg(test)]
mod tests;
