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

use super::{ClaimBody, ClaimEvent, FieldValue, KeyId, KindId, MovedRecord, ObjectBody};
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
    /// The moment of the row's first `entity_write`, when it has one.
    pub taken_in: Option<Timestamp>,
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
    /// Old local id under the folded thing, to the id under the survivor.
    pub claims: Vec<(String, String)>,
}

/// Every row family this mapping reads.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Rows {
    pub entities: Vec<EntityRow>,
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
        // The rows an app binds an object by are not read here yet.
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
    // so only numbers 1, 2, 3 without a gap keep the count equal to the highest.
    let mut numbers: Vec<i64> = rows.merges.iter().map(|step| step.no).collect();
    numbers.sort_unstable();
    for (at, found) in numbers.into_iter().enumerate() {
        let expected = at as i64 + 1;
        if found != expected {
            return Err(ImportError::MergeNumberGap { expected, found });
        }
    }
    let mut events = Vec::new();
    for row in &rows.entities {
        events.push(envelope(
            None,
            None,
            row.taken_in,
            ClaimEvent::ObjectCreated {
                object: row.key.clone(),
                body: object_body(row),
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
    for (at, write) in rows.field_writes.iter().enumerate() {
        let Some((subject, ordinal, local)) = placed.get(&at) else {
            continue;
        };
        // A claim a merge moved is filed under the survivor now, with its new id.
        let origin = find_fact(rows, &write.fact_home, &write.fact_local)
            .or_else(|| find_fact(rows, subject, local))
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
                record: record_id(&claim_home(rows, write, subject), local),
                subject: subject.clone(),
                key: write.key.clone(),
                ordinal: *ordinal,
                class: write.class,
                write: write.write.clone(),
                targets: write.targets.clone(),
            },
        ));
    }
    let mut steps: Vec<&MergeRow> = rows.merges.iter().collect();
    steps.sort_by_key(|step| step.no);
    for step in steps {
        events.push(plain(ClaimEvent::ObjectsMerged {
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
        }));
    }
    Ok(events)
}

fn find_fact<'a>(rows: &'a Rows, home: &ObjectId, local: &str) -> Option<&'a FactRow> {
    rows.facts
        .iter()
        .find(|fact| &fact.home == home && fact.local == local)
}

/// The home a key write's claim is filed under after the merges: the thing the
/// write sits on now, unless the claim was never moved.
fn claim_home(rows: &Rows, write: &FieldWriteRow, subject: &ObjectId) -> ObjectId {
    if find_fact(rows, &write.fact_home, &write.fact_local).is_some() {
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
