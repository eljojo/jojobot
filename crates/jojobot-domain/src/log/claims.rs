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
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct KeyId(pub String);

/// The identity of a kind.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
pub struct KindId(pub String);

/// **What one write of a key did.** A clear is a write: it takes the key off the
/// thing until a later write puts it back.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum FieldValue {
    Set(String),
    Clear,
    /// **A stored clear, as the import reads it** (rule 677). Every clear the
    /// store holds took the key off the thing, whichever claim held the value, and
    /// that meaning does not change. A live clear is [`Self::Clear`].
    Removed,
}

/// **Everything an object is, complete when written.**
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
/// interpreter never re-sorts by position in the log. A write of the survivor's
/// own that the interleave moves on is named too, by the place it held.
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
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

/// **One spelling retired from an object**, and the day it stopped being current.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct Retirement {
    address: String,
    object: ObjectId,
    day: Date,
}

/// **One numbered merge**: the two objects, when the store took it in, the run
/// that made it and each claim's label before and after.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
struct MergeEntry {
    /// **The number the merge holds**, counting the numbers the store left empty
    /// before it: the number the store holds it under.
    number: u64,
    folded: ObjectId,
    survivor: ObjectId,
    at: Option<Timestamp>,
    run: Option<RunId>,
    relabelled: Vec<(String, String)>,
}

/// A replay repair the adapter must report, without giving the interpreter I/O.
#[derive(Debug, Default)]
pub struct Applied {
    pub replaced_handle: Option<String>,
}

/// **A holder of the claims State that lends it to a read.**
pub trait ClaimsLender: Send + Sync {
    /// Run `read` on the State and say so, or return `false` and run nothing
    /// when the State cannot answer: it is behind the log, or the import has not
    /// filled it yet. The caller then reads the store, slower and correct.
    fn lend(&self, read: &mut dyn FnMut(&State)) -> bool;
}

/// A State behind a lock answers always: nothing it was fed can be missing.
impl ClaimsLender for std::sync::Mutex<State> {
    fn lend(&self, read: &mut dyn FnMut(&State)) -> bool {
        read(&self.lock().expect("claims state lock"));
        true
    }
}

fn text_bytes(text: &str) -> u64 {
    text.len() as u64
}

/// The text an object serves: its name, source, aliases, boot text and metadata.
fn object_bytes(body: &ObjectBody) -> u64 {
    text_bytes(&body.name)
        + text_bytes(&body.source)
        + body
            .aliases
            .iter()
            .map(|alias| text_bytes(alias))
            .sum::<u64>()
        + body.boot.as_deref().map_or(0, text_bytes)
        + body
            .metadata
            .iter()
            .map(|(key, value)| text_bytes(key) + text_bytes(value))
            .sum::<u64>()
}

/// The text a claim serves under its current wording.
fn claim_bytes(claim: &ClaimBody) -> u64 {
    text_bytes(&claim.content) + claim.details.as_deref().map_or(0, text_bytes)
}

/// The text one write of a key serves: the key and the value, and nothing for a
/// clear or a removal.
fn write_bytes(key: &KeyId, write: &Write) -> u64 {
    match &write.write {
        FieldValue::Set(value) => text_bytes(&key.0) + text_bytes(value),
        _ => 0,
    }
}

/// The text a stream serves: each claim's newest write of the key.
fn stream_bytes(key: &KeyId, stream: &[Write]) -> u64 {
    let mut seen = BTreeSet::new();
    stream
        .iter()
        .rev()
        .filter(|write| seen.insert(&write.record))
        .map(|write| write_bytes(key, write))
        .sum()
}

/// **The bytes of text the State serves now, and the records that hold them**: every
/// object and claim at any status, and no history. An overhead measure divides what the process holds by this.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct Payload {
    pub records: u64,
    pub bytes: u64,
}

/// **The folded fields of each thing, held beside the streams they fold.** It is
/// derived from the streams and the declarations, so two states that hold the same
/// events are equal whatever they hold here.
#[derive(Debug, Clone, Default)]
struct HeldFields(BTreeMap<ObjectId, BTreeMap<String, String>>);

impl PartialEq for HeldFields {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl Eq for HeldFields {}

// **How many things the folds have folded on this thread**, so a case can say a
// read folds nothing and a write folds one thing.
#[cfg(test)]
thread_local! {
    static FOLDS_COUNTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static SCANS_COUNTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    static VISITS_COUNTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// **What the events so far say.**
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct State {
    objects: BTreeMap<ObjectId, ObjectBody>,
    addresses: BTreeMap<String, ObjectId>,
    redirects: BTreeMap<ObjectId, ObjectId>,
    /// The spellings an object answered to once and no longer does, for the
    /// claim lookup. It is read only after the current spellings, so a spelling
    /// bound again answers to its new object.
    retired_addresses: BTreeMap<String, ObjectId>,
    records: BTreeMap<RecordId, Record>,
    #[serde(serialize_with = "serialize_entries")]
    streams: BTreeMap<(ObjectId, KeyId), Vec<Write>>,
    /// Every declaration of each kind, oldest first.
    declarations: BTreeMap<ObjectId, Vec<DeclaredType>>,
    /// **What each thing holds, folded** under the declarations in force: a write
    /// folds the thing it lands on, a declaration folds the things that hold a key
    /// it names, and a read takes this and folds nothing. Keyed by the survivor of a
    /// merge; a thing that holds nothing has no entry. **Not serialized**: it is
    /// worked out from the streams and the declarations as each event applies, and the
    /// State is only ever built by applying events, so a serialized copy is measured
    /// and never loaded.
    #[serde(skip)]
    held_fields: HeldFields,
    /// **The declaration in force of each kind, as one shared list**, rebuilt when a
    /// declaration lands or is withdrawn and never by a read, so a read of the types
    /// copies nothing. **Not serialized**, for the reason `held_fields` is not: it is
    /// rebuilt from `declarations` whenever one lands or is withdrawn.
    #[serde(skip)]
    declared_snapshot: std::sync::Arc<Vec<DeclaredType>>,
    /// Every observed write of each object's own row, in the order the events
    /// arrived, under the object that was written.
    observed: BTreeMap<ObjectId, Vec<ObservedWrite>>,
    /// Every write of each object's own row, oldest first, under the object
    /// that was written.
    versions: BTreeMap<ObjectId, Vec<ObjectVersion>>,
    /// Every object folded into each survivor, at any depth.
    folded_under: BTreeMap<ObjectId, BTreeSet<ObjectId>>,
    version_seq: u64,
    /// Every numbered merge, in the order they were made.
    merge_log: Vec<MergeEntry>,
    /// Every spelling retired from an object, in the order they were retired.
    retirements: Vec<Retirement>,
    duplicate_handle_bindings: u64,
    last_duplicate_handle_binding_at: Option<Timestamp>,
    /// The log this state was built from holds a completed claims import: an
    /// `ImportCompleted` event was applied. It travels with the state, so a state
    /// that starts from a snapshot covering the import still knows.
    import_completed: bool,
    /// **What the State serves now, kept as events apply** (see [`Payload`]), so
    /// a reader takes the count without walking the State under its lock.
    payload: Payload,
    indexes: indexes::Indexes,
}

// JSON object keys cannot hold tuples; entries preserve their structure.
fn serialize_entries<K: serde::Serialize, V: serde::Serialize, S: serde::Serializer>(
    entries: &BTreeMap<K, V>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.collect_seq(entries.iter())
}

impl State {
    /// **The interpreter.** Structural only. It reads the actor and the run of a
    /// write from the envelope.
    pub fn apply(&mut self, event: &Event<ClaimEvent>) -> Applied {
        let mut applied = Applied::default();
        match &event.payload {
            // **The current state at the cut serves the object and is never a
            // version of it** (the baseline of an import).
            ClaimEvent::ObjectBaselined { object, body } => {
                let held = self.objects.insert(object.clone(), body.clone());
                self.keep_object(held.as_ref(), body);
                let before = held.map(|held| held.parent);
                self.index_object(object, before, &body.parent);
            }
            ClaimEvent::ObjectCreated { object, body }
            | ClaimEvent::ObjectRevised { object, body } => {
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
                let held = self.objects.insert(object.clone(), body.clone());
                self.keep_object(held.as_ref(), body);
                let before = held.map(|held| held.parent);
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
                if indexes::handle_shaped(address)
                    && let Some(former) = self.own_handle_of(object)
                    && former.0 != *address
                {
                    self.retire_address(object, &former.0, None);
                    self.duplicate_handle_bindings += 1;
                    self.last_duplicate_handle_binding_at =
                        event.imported_source_time.or(event.written_at);
                    applied.replaced_handle = Some(former.0);
                }
                if let Some(held) = self.addresses.insert(address.clone(), object.clone()) {
                    self.unbind_spelling(&held, address);
                }
                self.bind_spelling(object, address);
            }
            ClaimEvent::AddressRetired {
                object,
                address,
                day,
            } => {
                self.retire_address(object, address, Some(*day));
            }
            ClaimEvent::ObjectsMerged {
                folded,
                survivor,
                records,
                writes,
            } => self.merge(
                folded,
                survivor,
                records,
                writes,
                Some((
                    event.imported_source_time.or(event.written_at),
                    event.run.clone(),
                )),
            ),
            ClaimEvent::ObjectRedirected { folded, survivor } => {
                self.merge(folded, survivor, &[], &[], None)
            }
            ClaimEvent::MergeNumbersLeftEmpty { count } => self.leave_merge_numbers_empty(*count),
            ClaimEvent::ImportCompleted { .. } => self.import_completed = true,
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
                self.keep_claim(before.as_ref(), claim);
                if before.is_none() {
                    self.note_origin(record, claim);
                }
                self.index_record(record, before.as_ref(), claim);
            }
            ClaimEvent::RecordRevised { record, claim } => {
                if let Some(held) = self.records.get_mut(record) {
                    let before = std::mem::replace(&mut held.claim, claim.clone());
                    self.payload.bytes =
                        self.payload.bytes - claim_bytes(&before) + claim_bytes(claim);
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
                self.append_write(
                    subject,
                    key,
                    Write {
                        record: record.clone(),
                        ordinal: *ordinal,
                        class: *class,
                        write: write.clone(),
                        targets: targets.clone(),
                        writer: event.actor.clone(),
                        run: event.run.clone(),
                        written_at: event.imported_source_time.or(event.written_at),
                    },
                );
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
                self.append_write(
                    subject,
                    key,
                    Write {
                        record: record.clone(),
                        ordinal: *ordinal,
                        class: *class,
                        write: FieldValue::Removed,
                        targets: targets.clone(),
                        writer: event.actor.clone(),
                        run: event.run.clone(),
                        written_at: event.imported_source_time.or(event.written_at),
                    },
                );
                self.index_write(record, subject, key, *ordinal);
            }
            ClaimEvent::TypeDeclared { kind, declaration } => {
                // The keys the kind named before and the keys it names now fold
                // differently when the new declaration says something else of them.
                let mut keys = self.keys_named_by(kind);
                self.declarations
                    .entry(kind.clone())
                    .or_default()
                    .push(declaration.clone());
                keys.extend(declaration.fields.iter().map(|field| field.key.clone()));
                self.rebuild_declared_snapshot();
                self.refold_holders_of(&keys);
            }
            ClaimEvent::TypeWithdrawn { kind } => {
                let keys = self.keys_named_by(kind);
                self.declarations.remove(kind);
                self.rebuild_declared_snapshot();
                self.refold_holders_of(&keys);
            }
        }
        applied
    }

    /// **The kept count takes an object written over what it held** (nothing, when
    /// it is new): the new text in, the old text out.
    fn keep_object(&mut self, held: Option<&ObjectBody>, body: &ObjectBody) {
        match held {
            Some(held) => self.payload.bytes -= object_bytes(held),
            None => self.payload.records += 1,
        }
        self.payload.bytes += object_bytes(body);
    }

    /// **The kept count takes a claim created over what the record held** (nothing,
    /// when it is new).
    fn keep_claim(&mut self, held: Option<&ClaimBody>, claim: &ClaimBody) {
        match held {
            Some(held) => self.payload.bytes -= claim_bytes(held),
            None => self.payload.records += 1,
        }
        self.payload.bytes += claim_bytes(claim);
    }

    /// **A write joins its stream, and the kept count moves by what its claim's
    /// newest write of the key was and now is**: a later write of the same claim
    /// replaces an earlier one, which is history.
    fn append_write(&mut self, subject: &ObjectId, key: &KeyId, write: Write) {
        let stream = self
            .streams
            .entry((subject.clone(), key.clone()))
            .or_default();
        let replaced = stream
            .iter()
            .rev()
            .find(|held| held.record == write.record)
            .map_or(0, |held| write_bytes(key, held));
        self.payload.bytes = self.payload.bytes - replaced + write_bytes(key, &write);
        stream.push(write);
    }

    fn retire_address(&mut self, object: &ObjectId, address: &str, day: Option<Date>) {
        if self.addresses.get(address) != Some(object) {
            return;
        }
        self.addresses.remove(address);
        self.unbind_spelling(object, address);
        self.retired_addresses
            .insert(address.into(), object.clone());
        if let Some(day) = day {
            self.retirements.push(Retirement {
                address: address.into(),
                object: object.clone(),
                day,
            });
        }
    }

    /// The durable duplicate bindings repaired while filling this state.
    pub fn duplicate_handle_bindings(&self) -> u64 {
        self.duplicate_handle_bindings
    }

    /// The replacing event's own instant, absent when that event named none.
    pub fn last_duplicate_handle_binding_at(&self) -> Option<Timestamp> {
        self.last_duplicate_handle_binding_at
    }

    /// **A thing named by any spelling it answers to, as a measure reads it**:
    /// the entity, its fields as a read serves them, and how many claims are
    /// homed on it (any status). `None` for a name nothing holds.
    pub fn thing_as_read(
        &self,
        name: &str,
        declared: &[DeclaredType],
    ) -> Option<(memory::Entity, BTreeMap<String, String>, usize)> {
        let object = self.resolve(name)?;
        let handle = self.handle_of(object)?;
        let entity = self.entity(&handle)?;
        let fields = self.fields_served(self.survivor(object), declared);
        Some((entity, fields, self.records_homed_on(object).len()))
    }

    /// **The runs that first wrote the claims homed on a thing**, by any spelling
    /// it answers to. A claim's bot is told through its run, so the door asks
    /// which bot each of these belongs to.
    pub fn first_runs_on(&self, name: &str) -> BTreeSet<RunId> {
        let Some(object) = self.resolve(name) else {
            return BTreeSet::new();
        };
        self.records_homed_on(object)
            .into_iter()
            .filter_map(|id| self.records.get(id)?.first_run.clone())
            .collect()
    }

    /// **The handle an object answers to now**, when it has one.
    pub fn handle(&self, object: &ObjectId) -> Option<crate::memory::EntityId> {
        self.handle_of(object)
    }

    /// **Whether the log this state was built from holds a completed claims
    /// import.**
    pub fn import_completed(&self) -> bool {
        self.import_completed
    }

    /// **The declaration of a kind in force now**: the newest one.
    pub fn declared(&self, kind: &ObjectId) -> Option<&DeclaredType> {
        self.declarations.get(kind).and_then(|held| held.last())
    }

    /// **Every declaration in force now**, the newest of each kind: what the
    /// types declare, as `prepare` reads it.
    pub fn declared_types(&self) -> Vec<DeclaredType> {
        self.declarations
            .values()
            .filter_map(|held| held.last().cloned())
            .collect()
    }

    /// **Every declaration in force now, as one shared list** that a declaration
    /// rebuilds and a read only clones the pointer of.
    pub fn declared_types_shared(&self) -> std::sync::Arc<Vec<DeclaredType>> {
        self.declared_snapshot.clone()
    }

    /// The keys the declaration in force of a kind names.
    fn keys_named_by(&self, kind: &ObjectId) -> BTreeSet<String> {
        self.declared(kind)
            .into_iter()
            .flat_map(|held| held.fields.iter().map(|field| field.key.clone()))
            .collect()
    }

    fn rebuild_declared_snapshot(&mut self) {
        self.declared_snapshot = std::sync::Arc::new(self.declared_types());
    }

    /// Every name the State holds a declaration for.
    pub fn declared_names(&self) -> impl Iterator<Item = &ObjectId> {
        self.declarations.keys()
    }

    /// Whether any object, archived or not, is of this kind.
    pub fn holds_object_of_kind(&self, kind: &ObjectId) -> bool {
        self.objects.values().any(|object| object.kind.0 == kind.0)
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
        counted: Option<(Option<Timestamp>, Option<RunId>)>,
    ) {
        let numbered = counted.is_some();
        // A redirect that closes a loop would make `survivor` spin forever.
        if self.survivor(survivor) == self.survivor(folded) {
            return;
        }
        // **The survivor is the end of its own chain.** A mark that folds into a thing
        // already folded elsewhere arrives before the mark that folded that thing when
        // the import orders marks by entity row, and everything filed under the middle
        // thing would be unreachable: reads look under the end of the chain only.
        // The merge log keeps the survivor the event named: the store's merge
        // event does, and a later fold of that survivor does not rewrite it.
        let named = survivor;
        let survivor = &self.survivor(survivor).clone();
        self.index_before_merge(records);
        self.redirects.insert(folded.clone(), survivor.clone());
        let mut under = self.folded_under.remove(folded).unwrap_or_default();
        under.insert(folded.clone());
        self.folded_under
            .entry(survivor.clone())
            .or_default()
            .extend(under);
        let mut relabelled = Vec::new();
        for moved in records {
            if let Some(held) = self.records.get_mut(&moved.record) {
                relabelled.push((held.claim.label.clone(), moved.label.clone()));
                held.claim.home = survivor.clone();
                held.claim.label = moved.label.clone();
            }
        }
        if let Some((at, run)) = counted {
            // The counter advances after this entry is logged, so the merge holds
            // the number after the latest.
            self.merge_log.push(MergeEntry {
                number: self.highest_merge_number() + 1,
                folded: folded.clone(),
                survivor: named.clone(),
                at,
                run,
                relabelled,
            });
        }
        let keys: Vec<(ObjectId, KeyId)> = self
            .streams
            .keys()
            .filter(|(subject, _)| subject == folded)
            .cloned()
            .collect();
        let mut moved_writers: BTreeSet<RecordId> = BTreeSet::new();
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
            // **The survivor's own writes take their new places first**: the
            // store numbers the interleaved key from one, so a folded write
            // placed among them moves the later ones on.
            for write in target.iter_mut() {
                if let Some(moved) = writes.iter().find(|moved| {
                    moved.record == write.record
                        && moved.key == key
                        && moved.from_ordinal == write.ordinal
                }) {
                    write.ordinal = moved.ordinal;
                }
            }
            // The two streams become one, and the newest write of a claim that wrote
            // under both is the later in the joined order.
            let before = stream_bytes(&key, &stream) + stream_bytes(&key, target);
            let mut writers: BTreeSet<RecordId> = BTreeSet::new();
            for mut write in stream {
                writers.insert(write.record.clone());
                if let Some(moved) = writes.iter().find(|moved| {
                    moved.record == write.record
                        && moved.key == key
                        && moved.from_ordinal == write.ordinal
                }) {
                    write.ordinal = moved.ordinal;
                }
                target.push(write);
            }
            // **The joined stream is in ordinal order**, so the last write of a claim
            // is its newest to every reader, whether it takes it by position or by
            // ordinal. A tie keeps the survivor's write first.
            target.sort_by_key(|write| write.ordinal);
            // The sort can change which write is newest for a claim of the survivor's
            // own stream too, so every writer of the joined stream names again.
            moved_writers.extend(target.iter().map(|write| write.record.clone()));
            self.payload.bytes = self.payload.bytes - before + stream_bytes(&key, target);
            let highest = target.iter().map(|write| write.ordinal).max();
            self.indexes
                .moved_stream(&folded_key, &survivor_key, highest, &writers);
            moved_writers.extend(writers);
        }
        // Interleaving the writes can change which write of a key is a record's
        // newest, and so what it names.
        for record in &moved_writers {
            self.refresh_naming(record);
        }
        // The folded thing's streams are the survivor's now.
        self.held_fields.0.remove(folded);
        self.refold(survivor);
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

    /// **Whether the object holds a lease**, a role: its renewals are heartbeats
    /// kept off the log, so the State does not hold what a read of it serves.
    pub fn holds_a_lease(&self, object: &ObjectId) -> bool {
        self.object(object)
            .is_some_and(|body| body.kind.0 == crate::memory::EntityKind::ROLE.as_token())
    }

    /// **Whether a merge touched this object**: it was folded into another, or
    /// something was folded into it.
    pub fn was_merged(&self, object: &ObjectId) -> bool {
        let survivor = self.survivor(object);
        survivor != object || self.folded_under.contains_key(survivor)
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

    /// **What the State serves now, by its own text** (see [`Payload`]), as the
    /// events so far have kept it.
    pub fn payload(&self) -> Payload {
        self.payload
    }

    /// **The count taken by walking the whole State**: the oracle the kept count
    /// is held to after every event in the tests, and nothing else's.
    #[cfg(test)]
    fn payload_by_walk(&self) -> Payload {
        let mut payload = Payload::default();
        let size = |text: &str| text.len() as u64;
        // Every object the State serves, whatever its status: an archived or folded
        // one is still held with the text it had.
        for body in self.objects.values() {
            payload.records += 1;
            payload.bytes += size(&body.name) + size(&body.source);
            payload.bytes += body.aliases.iter().map(|alias| size(alias)).sum::<u64>();
            payload.bytes += body.boot.as_deref().map_or(0, size);
            payload.bytes += body
                .metadata
                .iter()
                .map(|(key, value)| size(key) + size(value))
                .sum::<u64>();
        }
        // Every claim at any status, with the wording it has now.
        for held in self.records.values() {
            payload.records += 1;
            payload.bytes +=
                size(&held.claim.content) + held.claim.details.as_deref().map_or(0, size);
        }
        // The current write of each key by each claim: a later write of the same
        // claim replaces an earlier one, which is history.
        for ((_, key), writes) in &self.streams {
            let mut seen = BTreeSet::new();
            for write in writes.iter().rev() {
                if !seen.insert(&write.record) {
                    continue;
                }
                if let FieldValue::Set(value) = &write.write {
                    payload.bytes += size(&key.0) + size(value);
                }
            }
        }
        payload
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
                removed: matches!(write.write, FieldValue::Removed),
                key: key.0.clone(),
                ordinal: write.ordinal,
                value: match &write.write {
                    FieldValue::Set(value) => Some(value.clone()),
                    // A stored removal holds no value and is no blank: a blank is a
                    // value on a list and on a view declaration key.
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
    /// claims that reach it. Under the declarations in force, which is what a read
    /// passes, the held fold answers and nothing is folded; under any other list
    /// the fold is made.
    pub fn fields(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, String> {
        if std::ptr::eq(declared.as_ptr(), self.declared_snapshot.as_ptr())
            && declared.len() == self.declared_snapshot.len()
        {
            return self
                .held_fields
                .0
                .get(self.survivor(subject))
                .cloned()
                .unwrap_or_default();
        }
        self.fold_subject(subject, declared)
    }

    /// **The fold of one thing's writes** under `declared`.
    fn fold_subject(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, String> {
        #[cfg(test)]
        FOLDS_COUNTED.with(|n| n.set(n.get() + 1));
        memory::folded_fields(&self.key_writes(subject), declared)
    }

    /// **Fold a thing again and hold it**, after a write or a revision that reaches
    /// its streams. A thing that holds nothing has no entry.
    fn refold(&mut self, subject: &ObjectId) {
        let subject = self.survivor(subject).clone();
        let folded = self.fold_subject(&subject, &self.declared_snapshot.clone());
        if folded.is_empty() {
            self.held_fields.0.remove(&subject);
        } else {
            self.held_fields.0.insert(subject, folded);
        }
    }

    /// **Fold again the things that hold a key a declaration names**, after the
    /// declarations in force changed what those keys fold to.
    fn refold_holders_of(&mut self, keys: &BTreeSet<String>) {
        let subjects: BTreeSet<ObjectId> = self
            .streams
            .keys()
            .filter(|(_, key)| keys.contains(&key.0))
            .map(|(subject, _)| subject.clone())
            .collect();
        for subject in subjects {
            self.refold(&subject);
        }
    }

    /// The count of things folded on this thread, for a case.
    #[cfg(test)]
    pub(crate) fn folds_counted() -> usize {
        FOLDS_COUNTED.with(std::cell::Cell::get)
    }

    /// Reset that count.
    #[cfg(test)]
    pub(crate) fn reset_folds_counted() {
        FOLDS_COUNTED.with(|n| n.set(0));
        SCANS_COUNTED.with(|n| n.set(0));
        VISITS_COUNTED.with(|n| n.set(0));
    }

    /// The count of things a read built a scan of on this thread, for a case.
    #[cfg(test)]
    pub(crate) fn scans_counted() -> usize {
        SCANS_COUNTED.with(std::cell::Cell::get)
    }

    /// The count of things a browse looked at on this thread, for a case.
    #[cfg(test)]
    pub(crate) fn visits_counted() -> usize {
        VISITS_COUNTED.with(std::cell::Cell::get)
    }

    /// Note a scan of one thing.
    #[cfg(test)]
    pub(super) fn note_scan() {
        SCANS_COUNTED.with(|n| n.set(n.get() + 1));
    }

    /// Note one thing looked at by a browse.
    #[cfg(test)]
    pub(super) fn note_visit() {
        VISITS_COUNTED.with(|n| n.set(n.get() + 1));
    }

    /// **Whether what is held is what a fold of the streams makes**, for the case
    /// that holds the upkeep to a full fold.
    #[cfg(test)]
    pub(crate) fn held_fields_are_what_a_fold_makes(&self) -> Result<(), String> {
        let declared = self.declared_snapshot.clone();
        let mut subjects: BTreeSet<ObjectId> = self
            .streams
            .keys()
            .map(|(subject, _)| self.survivor(subject).clone())
            .collect();
        subjects.extend(self.held_fields.0.keys().cloned());
        for subject in subjects {
            let made = self.fold_subject(&subject, &declared);
            let held = self
                .held_fields
                .0
                .get(&subject)
                .cloned()
                .unwrap_or_default();
            if made != held {
                return Err(format!(
                    "{subject:?} holds {held:?} and a fold makes {made:?}"
                ));
            }
        }
        Ok(())
    }
}

pub mod emit;
pub mod entity_change;
mod generation;
pub mod import;
mod indexes;
pub mod job;
mod prepare;
mod reads;
pub use reads::DocumentParts;
mod rules;
pub mod type_change;
pub use indexes::Located;
pub use prepare::{
    Admitted, BootFloor, CaptureRequest, ClaimSize, Declined, Door, Draft, Measures, Refusal,
    Route, prepare, server_written_error, server_written_key,
};
pub use rules::{HeldRecord, StarredRule};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod generation_tests;
