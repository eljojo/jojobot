//! **Today's claim, field and identity rows as events.** Pure: it maps decoded
//! row values, reads no store and runs no SQL. Identity resolution precedes it:
//! every [`ObjectId`] here is the permanent key a row is filed under, never a
//! handle (rule 268), and a pointer a row holds is already resolved.
//!
//! **Nothing is invented.** A moment is the row's own and stays the source's
//! (`imported_source_time`); a row with no moment has none, and a write with no
//! pointer to its claim write has no writer and no run. Rows an older build
//! repaired arrive repaired: the row repairs run before this mapping (rule 706).
//!
//! **A retained row is evidence of its state, not of a sequence of acts**, as in
//! `request::events`. The history a row keeps (the writes of a claim, the
//! writes of a key) maps one event for one row; a merge is not replayed, its
//! result is: the writes of a folded thing arrive on the survivor at the place
//! the merge ledger put them ([`merge::apply_merges`], the one rule), and
//! [`ClaimEvent::ObjectsMerged`] then carries only the redirect.
//!
//! Not mapped, because [`ClaimEvent`] has no kind for them: `type_write`,
//! `fact_event_metadata` and a claim's `event_kind`.

use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;

use super::{
    ClaimBody, ClaimEvent, FieldValue, KeyId, KindId, MovedRecord, ObjectBody, ObjectSnapshot,
};
use crate::actor::Actor;
use crate::log::{Event, ObjectId, RecordId, RunId};
use crate::memory::merge::{self, MergeStep, ServedWrite};
use crate::memory::types::DeclaredType;
use crate::memory::write_origin::{self, WriteOrigin};
use crate::memory::{EntityId, WriteClass};

/// **A row the log has no kind for.** The column and the table are unwritten
/// in production; a row that holds one is refused by name and never dropped.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImportError {
    #[error("claim {home}/{local} holds an event_kind, which no event carries")]
    EventKindHeld { home: String, local: String },
    #[error("claim {home}/{local} holds a metadata row under {key}, which no event carries")]
    MetadataHeld {
        home: String,
        local: String,
        key: String,
    },
    #[error("merge numbers must run 1, 2, 3 without a gap: expected {expected}, found {found}")]
    MergeNumberGap { expected: i64, found: i64 },
}

/// **One `type_write` row**, its declaration decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeWriteRow {
    pub ordinal: u32,
    pub session: Option<String>,
    pub written_at: Option<Timestamp>,
    pub declaration: DeclaredType,
}

/// **One `fact_event_metadata` row.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MetadataRow {
    pub home: ObjectId,
    pub local: String,
    pub key: String,
    pub value: String,
}

/// The envelope kind and meaning every imported claim event wears.
pub const EVENT_KIND: &str = "claim";
pub const MEANING_VERSION: u32 = 1;

/// **One `entity` row with its aliases.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityRow {
    pub key: ObjectId,
    /// The handle the row answers to now.
    pub handle: String,
    pub kind: String,
    pub name: String,
    pub source: String,
    pub boot: Option<String>,
    pub parent: Option<ObjectId>,
    pub aliases: Vec<String>,
    pub crm: Option<String>,
    pub prose: String,
    pub badge: Option<String>,
    pub archived: Option<(String, Timestamp)>,
    /// The thing this one was folded into, as the row marks it: the permanent key.
    /// A merge made before `merge_event` existed has this and no event row.
    pub merged_into: Option<ObjectId>,
    /// The moment of the row's first `entity_write`, when it has one.
    pub taken_in: Option<Timestamp>,
}

/// **One `entity_write` row**: a write of an entity's own row. The thing is the
/// permanent key the write is attributed to (a thing folded into another keeps
/// its own), `filed_under` the handle the row was written under, and `snapshot`
/// what the write left, or `None` for a row an older build appended with no
/// snapshot. `written_at` is the stamp as stored, read once by the one parser;
/// a stamp that does not parse is `None`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityWriteRow {
    pub thing: ObjectId,
    pub filed_under: String,
    pub ordinal: u64,
    pub written_at: Option<Timestamp>,
    pub session: Option<String>,
    pub snapshot: Option<ObjectSnapshot>,
}

/// **One `entity_former_handle` row**: a handle the thing no longer wears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormerHandleRow {
    pub former: String,
    pub object: ObjectId,
    pub ordinal: u32,
}

/// **One `fact_write` row**: the claim as that write left it, and what the
/// write recorded about itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactWriteRow {
    pub ordinal: u64,
    pub claim: ClaimBody,
    pub run: Option<String>,
    pub bot: Option<EntityId>,
    /// The bot behind `run`, when a session row answers to it.
    pub run_bot: Option<EntityId>,
    pub written_at: Option<Timestamp>,
}

/// **One `fact` row** with the writes behind it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FactRow {
    pub home: ObjectId,
    /// The local id under the home.
    pub local: String,
    /// The row as it stands.
    pub head: ClaimBody,
    pub writes: Vec<FactWriteRow>,
    /// The `event_kind` column; `None` is the only value that imports.
    pub event_kind: Option<String>,
}

/// **One `field_write` row.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldWriteRow {
    pub entity: ObjectId,
    pub key: KeyId,
    pub ordinal: u64,
    pub class: WriteClass,
    pub write: FieldValue,
    /// The local id of the claim that carried it, under `fact_home`.
    pub fact_home: ObjectId,
    pub fact_local: String,
    /// The claim write that carried it (0118); `None` for an older build's.
    pub fact_write_ordinal: Option<u64>,
    pub written_at: Option<Timestamp>,
    pub targets: Vec<ObjectId>,
}

/// **One merge**, from its `merge_event` rows: the claims it gave a new local id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeRow {
    pub no: i64,
    pub folded: ObjectId,
    pub survivor: ObjectId,
    /// When the fold landed.
    pub merged_at: Timestamp,
    /// The run that made the merge, when it had one.
    pub session: Option<String>,
    /// Old local id under the folded thing, to the id under the survivor.
    pub claims: Vec<(String, String)>,
}

/// **A record the build supplies**, as the import is handed it. It is no row of
/// the store: the build owns it, and its fields and prose still answer from the
/// build's own list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuppliedRow {
    /// The handle it answers at.
    pub handle: String,
    pub kind: String,
    pub name: String,
    pub source: String,
}

/// **The identity a supplied record has in the log.** Deterministic from its
/// handle, so the same handle gives the same key on every boot and every retry.
/// It carries a `/`, which no badge and no handle can hold, so no stored key can
/// take it. A handle is an address and may be renamed by a later build; this key
/// does not move with it.
pub fn supply_identity(handle: &str) -> ObjectId {
    ObjectId(format!("supplied/{handle}"))
}

/// Every row family this mapping reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rows {
    pub entities: Vec<EntityRow>,
    /// The records the build supplies.
    pub supplied: Vec<SuppliedRow>,
    pub entity_writes: Vec<EntityWriteRow>,
    pub former_handles: Vec<FormerHandleRow>,
    pub facts: Vec<FactRow>,
    pub field_writes: Vec<FieldWriteRow>,
    pub merges: Vec<MergeRow>,
    pub type_writes: Vec<TypeWriteRow>,
    pub metadata: Vec<MetadataRow>,
}

/// **The identity a claim is filed under in the log**: its home and local id.
/// A merge that gave a claim a new local id gives it a new identity here, as
/// the store does.
pub fn record_id(home: &ObjectId, local: &str) -> RecordId {
    RecordId(format!("{}/{}", home.0, local))
}

fn envelope(
    actor: Option<Actor>,
    run: Option<String>,
    at: Option<Timestamp>,
    payload: ClaimEvent,
) -> Event<ClaimEvent> {
    Event {
        event_kind: EVENT_KIND.into(),
        meaning_version: MEANING_VERSION,
        actor,
        run: run.map(RunId),
        session_card: None,
        request: None,
        part: None,
        occurred_at: None,
        written_at: None,
        imported_source_time: at,
        payload,
    }
}

fn plain(payload: ClaimEvent) -> Event<ClaimEvent> {
    envelope(None, None, None, payload)
}

fn object_body(row: &EntityRow) -> ObjectBody {
    let mut metadata = BTreeMap::new();
    if let Some(crm) = &row.crm {
        metadata.insert("crm".to_string(), crm.clone());
    }
    if !row.prose.is_empty() {
        metadata.insert("prose".to_string(), row.prose.clone());
    }
    if let Some(badge) = &row.badge {
        metadata.insert("badge".to_string(), badge.clone());
    }
    if let Some((reason, at)) = &row.archived {
        metadata.insert("archived_reason".to_string(), reason.clone());
        metadata.insert("archived_at".to_string(), at.to_string());
    }
    ObjectBody {
        kind: KindId(row.kind.clone()),
        parent: row.parent.clone(),
        name: row.name.clone(),
        source: row.source.clone(),
        aliases: row.aliases.clone(),
        boot: row.boot.clone(),
        metadata,
        archived: row.archived.is_some(),
        // **Unbound, because no stored row binds an object to an app.** The
        // ownership rows come with the 9.3.3 to 9.3.5 build (rule 748); an app's
        // stamp on a claim says who wrote it and is not a binding. The build that
        // adds the rows reads them here.
        bound_to: None,
    }
}

/// What the claim write that carried a key write kept.
fn origin_of(write: &FactWriteRow) -> WriteOrigin {
    WriteOrigin {
        bot: write.bot.clone(),
        run: write.run.clone(),
        run_bot: write.run_bot.clone(),
        app_stamp: write.claim.app.clone(),
    }
}

/// A stored key write, placed by the merge ledger like any other.
#[derive(Debug, Clone)]
struct Placed {
    row: usize,
    key: String,
    ordinal: i64,
    written_at: Option<Timestamp>,
    fact: String,
}

impl ServedWrite for Placed {
    fn at(&self) -> (&str, i64, Option<Timestamp>, &str) {
        (&self.key, self.ordinal, self.written_at, &self.fact)
    }
    fn place(&mut self, ordinal: i64, fact: Option<&str>) {
        self.ordinal = ordinal;
        if let Some(fact) = fact {
            self.fact = fact.to_string();
        }
    }
}

/// **Where each stored key write sits once every merge is applied**: the thing
/// it belongs to now, its place in the key's order, and the local id of its
/// claim under that thing. A write the merges left out has no entry.
fn placed_by_merges(rows: &Rows) -> HashMap<usize, (ObjectId, u64, String)> {
    let mut held: HashMap<String, Vec<Placed>> = HashMap::new();
    for (row, write) in rows.field_writes.iter().enumerate() {
        held.entry(write.entity.0.clone())
            .or_default()
            .push(Placed {
                row,
                key: write.key.0.clone(),
                ordinal: write.ordinal as i64,
                written_at: write.written_at,
                fact: write.fact_local.clone(),
            });
    }
    let mut steps: Vec<&MergeRow> = rows.merges.iter().collect();
    steps.sort_by_key(|step| step.no);
    let steps: Vec<MergeStep> = steps
        .into_iter()
        .map(|step| MergeStep {
            merge_no: step.no,
            folded: step.folded.0.clone(),
            survivor: step.survivor.0.clone(),
            claims: step.claims.iter().cloned().collect(),
        })
        .collect();
    merge::apply_merges(&mut held, &steps);
    let mut out = HashMap::new();
    for (owner, writes) in held {
        for placed in writes {
            out.insert(
                placed.row,
                (ObjectId(owner.clone()), placed.ordinal as u64, placed.fact),
            );
        }
    }
    out
}

/// **The events that make the state today's rows serve.** Objects first, then
/// the claims, then the key writes, then the redirects of the merges.
pub fn import(rows: &Rows) -> Result<Vec<Event<ClaimEvent>>, ImportError> {
    if let Some(fact) = rows.facts.iter().find(|fact| fact.event_kind.is_some()) {
        return Err(ImportError::EventKindHeld {
            home: fact.home.0.clone(),
            local: fact.local.clone(),
        });
    }
    if let Some(row) = rows.metadata.first() {
        return Err(ImportError::MetadataHeld {
            home: row.home.0.clone(),
            local: row.local.clone(),
            key: row.key.clone(),
        });
    }
    // The state counts merges and today's store numbers one above the highest,
    // so the count equals the highest only when every number up to it is held. **A
    // number a merge made before `merge_event` existed once held may be missing**:
    // a thing marked merged with no row is there to hold it, one thing to a
    // number. A hole no such thing accounts for is refused.
    let with_event: std::collections::HashSet<&ObjectId> =
        rows.merges.iter().map(|step| &step.folded).collect();
    let marked_without_row = rows
        .entities
        .iter()
        .filter(|row| row.merged_into.is_some() && !with_event.contains(&row.key))
        .count();
    let mut numbers: Vec<i64> = rows.merges.iter().map(|step| step.no).collect();
    numbers.sort_unstable();
    let (mut expected, mut holes) = (1_i64, 0_usize);
    for found in numbers {
        if found < expected {
            return Err(ImportError::MergeNumberGap { expected, found });
        }
        let missing = (found - expected) as usize;
        if holes + missing > marked_without_row {
            return Err(ImportError::MergeNumberGap { expected, found });
        }
        holes += missing;
        expected = found + 1;
    }
    let mut events = Vec::new();
    for row in &rows.entities {
        events.push(envelope(
            None,
            None,
            row.taken_in,
            ClaimEvent::ObjectBaselined {
                object: row.key.clone(),
                body: object_body(row),
            },
        ));
    }
    // **Each record the build supplies is created under its own identity** and
    // answers to its handle, except where a stored row wears that handle: the
    // stored row holds the binding (rule 508) and the supplied object stays
    // unbound. Only its identity moves here; its fields and prose still answer
    // from the build's own list.
    for row in &rows.supplied {
        let object = supply_identity(&row.handle);
        events.push(plain(ClaimEvent::ObjectCreated {
            object: object.clone(),
            body: ObjectBody {
                kind: KindId(row.kind.clone()),
                parent: None,
                name: row.name.clone(),
                source: row.source.clone(),
                aliases: Vec::new(),
                boot: None,
                metadata: BTreeMap::new(),
                archived: false,
                bound_to: None,
            },
        }));
        if !rows
            .entities
            .iter()
            .any(|entity| entity.handle == row.handle)
        {
            events.push(plain(ClaimEvent::AddressBound {
                object,
                address: row.handle.clone(),
            }));
        }
    }
    // **Every write of an entity's own row is an observation, in the order the
    // store reads them**: a write nobody stamped first, then by moment. Rows of
    // equal moment fall in the order of their thing's key and ordinal, so the same
    // rows always give the same events. No moment is put in the place of a missing
    // one, and a row with no snapshot keeps none.
    let mut written: Vec<&EntityWriteRow> = rows.entity_writes.iter().collect();
    written.sort_by(|a, b| {
        (a.written_at, &a.thing, a.ordinal).cmp(&(b.written_at, &b.thing, b.ordinal))
    });
    for write in written {
        events.push(envelope(
            None,
            write.session.clone(),
            write.written_at,
            ClaimEvent::ObjectWriteObserved {
                object: write.thing.clone(),
                filed_under: write.filed_under.clone(),
                ordinal: write.ordinal,
                snapshot: write.snapshot.clone(),
            },
        ));
    }
    let mut formers: Vec<&FormerHandleRow> = rows.former_handles.iter().collect();
    formers.sort_by_key(|former| former.ordinal);
    for former in formers {
        for retire in [false, true] {
            let (object, address) = (former.object.clone(), former.former.clone());
            events.push(plain(if retire {
                ClaimEvent::AddressRetired { object, address }
            } else {
                ClaimEvent::AddressBound { object, address }
            }));
        }
    }
    for row in &rows.entities {
        events.push(plain(ClaimEvent::AddressBound {
            object: row.key.clone(),
            address: row.handle.clone(),
        }));
    }
    let mut declared: Vec<&TypeWriteRow> = rows.type_writes.iter().collect();
    declared.sort_by_key(|row| (row.declaration.name.clone(), row.ordinal));
    for row in declared {
        events.push(envelope(
            None,
            row.session.clone(),
            row.written_at,
            ClaimEvent::TypeDeclared {
                kind: ObjectId(row.declaration.name.clone()),
                declaration: row.declaration.clone(),
            },
        ));
    }
    for fact in &rows.facts {
        events.extend(claim_events(fact));
    }
    let placed = placed_by_merges(rows);
    let facts: HashMap<(&ObjectId, &str), &FactRow> = rows
        .facts
        .iter()
        .map(|fact| ((&fact.home, fact.local.as_str()), fact))
        .collect();
    for (at, write) in rows.field_writes.iter().enumerate() {
        let Some((subject, ordinal, local)) = placed.get(&at) else {
            continue;
        };
        let stored = facts
            .get(&(&write.fact_home, write.fact_local.as_str()))
            .copied();
        // A claim a merge moved is filed under the survivor now, with its new id.
        let origin = stored
            .or_else(|| facts.get(&(subject, local.as_str())).copied())
            .and_then(|fact| {
                fact.writes
                    .iter()
                    .find(|held| Some(held.ordinal) == write.fact_write_ordinal)
            });
        let (actor, run) = match origin {
            Some(held) => {
                let by = write_origin::written_by(Some(origin_of(held)), held.claim.app.as_ref());
                (by.writer, by.run)
            }
            None => (None, None),
        };
        events.push(envelope(
            actor,
            run,
            write.written_at,
            ClaimEvent::FieldWritten {
                record: record_id(&claim_home(stored.is_some(), write, subject), local),
                subject: subject.clone(),
                key: write.key.clone(),
                ordinal: *ordinal,
                class: write.class,
                write: write.write.clone(),
                targets: write.targets.clone(),
            },
        ));
    }
    // A merge made before `merge_event` existed is told only by the mark on the
    // folded entity. It has no moment and no run, and it comes first because it
    // is older than every merge that has a row. **The first as many as there are
    // holes in the numbers are counted merges, each holding a missing number**;
    // the rest are redirects the store never numbered, which the count leaves out.
    let mut counted = holes;
    for row in &rows.entities {
        let Some(survivor) = &row.merged_into else {
            continue;
        };
        if with_event.contains(&row.key) {
            continue;
        }
        events.push(plain(if counted > 0 {
            counted -= 1;
            ClaimEvent::ObjectsMerged {
                folded: row.key.clone(),
                survivor: survivor.clone(),
                records: Vec::new(),
                writes: Vec::new(),
            }
        } else {
            ClaimEvent::ObjectRedirected {
                folded: row.key.clone(),
                survivor: survivor.clone(),
            }
        }));
    }
    let mut steps: Vec<&MergeRow> = rows.merges.iter().collect();
    steps.sort_by_key(|step| step.no);
    for step in steps {
        events.push(envelope(
            None,
            step.session.clone(),
            Some(step.merged_at),
            ClaimEvent::ObjectsMerged {
                folded: step.folded.clone(),
                survivor: step.survivor.clone(),
                // The claims the merge gave a new id keep the label they now wear.
                records: step
                    .claims
                    .iter()
                    .map(|(_, new)| MovedRecord {
                        record: record_id(&step.survivor, new),
                        label: new.clone(),
                    })
                    .collect(),
                writes: Vec::new(),
            },
        ));
    }
    Ok(events)
}

/// The home a key write's claim is filed under after the merges: the thing the
/// write sits on now, unless the claim was never moved.
fn claim_home(filed: bool, write: &FieldWriteRow, subject: &ObjectId) -> ObjectId {
    if filed {
        write.fact_home.clone()
    } else {
        subject.clone()
    }
}

/// One event for each write of a claim, each in the envelope of its own write.
fn claim_events(fact: &FactRow) -> Vec<Event<ClaimEvent>> {
    let record = record_id(&fact.home, &fact.local);
    let mut writes: Vec<&FactWriteRow> = fact.writes.iter().collect();
    writes.sort_by_key(|write| write.ordinal);
    let mut out = Vec::new();
    for (at, write) in writes.iter().enumerate() {
        let by = write_origin::written_by(Some(origin_of(write)), write.claim.app.as_ref());
        let (record, claim) = (record.clone(), write.claim.clone());
        out.push(envelope(
            by.writer,
            by.run,
            write.written_at,
            if at == 0 {
                ClaimEvent::RecordCreated { record, claim }
            } else {
                ClaimEvent::RecordRevised { record, claim }
            },
        ));
    }
    match writes.last() {
        None => out.push(plain(ClaimEvent::RecordCreated {
            record,
            claim: fact.head.clone(),
        })),
        Some(last) if last.claim != fact.head => out.push(plain(ClaimEvent::RecordRevised {
            record,
            claim: fact.head.clone(),
        })),
        Some(_) => {}
    }
    out
}

#[cfg(test)]
mod tests;
