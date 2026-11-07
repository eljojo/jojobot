//! **The claims events of an entity's own row**, emitted into the transaction's
//! one batch when a writer changes the row. The row is read back from the table
//! inside the transaction, as the version kept for it is, and turned into the
//! object by the same function the import uses (`object_body`), so the log never
//! holds two readings of one row.
//!
//! A boot row repair and a merge call the version hook without calling this: a
//! repair never emits (it would make the import skip), and the merge is the claim
//! writers' to emit.

use jiff::{Timestamp, civil::Date};
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::entity_change::{EntityChange, entity_change_events};
use jojobot_domain::log::claims::import::EntityRow;
use jojobot_domain::log::claims::type_change::{type_declaration_events, type_withdrawal_events};
use jojobot_domain::memory::types::DeclaredType;

use super::import_rows::{maybe, text};
use super::*;

/// **Whether a store takes part in the event log**, asked once by each emitter.
/// A bare store (a boot pass, a seed, a direct-store case) writes its rows and
/// emits nothing. A journaled store (the one the server serves) emits into the
/// writer job's batch and **refuses** a write that reaches it with no job: the
/// error is `emit`'s, and the caller drops the transaction, so the write leaves
/// nothing behind. Nothing is skipped silently.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Journal {
    Bare,
    Journaled,
}

impl Journal {
    /// Whether the emitter goes on; the error is the refusal of a journaled store
    /// that was written with no writer job.
    pub(super) fn takes_part(self) -> Result<bool, MemoryError> {
        match self {
            Journal::Bare => Ok(false),
            Journal::Journaled => crate::writer::sql::emit(Vec::new())
                .map(|()| true)
                .map_err(store),
        }
    }
}

/// What a writer did to the row, and so which events follow.
pub(super) enum Change<'a> {
    /// The row did not exist before: the object is created and its handle bound.
    Created,
    /// The row was rewritten under the same handle.
    Revised,
    /// The row now answers to a new handle: the old one is retired and the new
    /// one bound.
    Renamed { from: &'a EntityId, day: Date },
}

/// Emit the events of `entity`'s row as it stands in `tx`.
pub(super) async fn emit_object_write(
    tx: &mut Transaction<'_, MySql>,
    entity: &EntityId,
    change: Change<'_>,
    clock: &Clock,
    journal: Journal,
    session: Option<&str>,
) -> Result<(), MemoryError> {
    if !journal.takes_part()? {
        return Ok(());
    }
    let Some(row) = sqlx::query(
        "SELECT id, kind, name, source, crm, parent, boot, prose, badge, merged_into, \
         archived_reason, archived_at FROM entity WHERE id = ?",
    )
    .bind(entity.as_str())
    .fetch_optional(&mut **tx)
    .await
    .map_err(store)?
    else {
        return Ok(());
    };
    let handle = text(&row, "id")?;
    let badge = maybe(&row, "badge")?;
    let key = ObjectId(badge.clone().unwrap_or_else(|| handle.clone()));
    let aliases: Vec<String> = match &badge {
        Some(badge) => {
            sqlx::query_scalar("SELECT alias FROM entity_alias WHERE entity = ? ORDER BY ordinal")
                .bind(badge)
                .fetch_all(&mut **tx)
                .await
                .map_err(store)?
        }
        None => Vec::new(),
    };
    // **The parent as its permanent key**, the badge it wears now: a row keeps the
    // key a parent had when it was written, and the key is what the import files.
    let parent = match maybe(&row, "parent")? {
        None => None,
        Some(stored) => {
            let key: Option<String> = sqlx::query_scalar(
                "SELECT COALESCE(badge, id) FROM entity WHERE id = ? OR badge = ? LIMIT 1",
            )
            .bind(&stored)
            .bind(&stored)
            .fetch_optional(&mut **tx)
            .await
            .map_err(store)?;
            Some(ObjectId(key.unwrap_or(stored)))
        }
    };
    let archived = match (maybe(&row, "archived_reason")?, maybe(&row, "archived_at")?) {
        (Some(reason), Some(at)) => at.parse::<Timestamp>().ok().map(|at| (reason, at)),
        _ => None,
    };
    let row = EntityRow {
        key,
        handle,
        kind: text(&row, "kind")?,
        name: text(&row, "name")?,
        source: text(&row, "source")?,
        boot: Some(text(&row, "boot")?),
        parent,
        aliases,
        crm: maybe(&row, "crm")?,
        prose: text(&row, "prose")?,
        badge,
        archived,
        merged_into: None,
        taken_in: None,
    };
    let change = match change {
        Change::Created => EntityChange::Created,
        Change::Revised => EntityChange::Revised,
        Change::Renamed { from, day } => EntityChange::Renamed {
            from: from.as_str(),
            day,
        },
    };
    let events = entity_change_events(&row, change, clock.now(), session.map(str::to_string));
    crate::writer::sql::emit(events).map_err(store)
}

/// **Emit `TypeDeclared` when `declared` differs from the declaration the State
/// holds for its name.** A declaration the State already holds adds no event, so
/// a seed run over a log that holds the shipped types leaves the log as it was.
/// A bare store emits nothing; a journaled one refuses a write with no writer job,
/// and one whose writer has not opened the claims state.
pub(super) fn emit_type_declared(
    declared: &DeclaredType,
    clock: &Clock,
    journal: Journal,
    session: Option<&str>,
) -> Result<(), MemoryError> {
    if !journal.takes_part()? {
        return Ok(());
    }
    let kind = ObjectId(declared.name.clone());
    let Some(held) = crate::writer::sql::read_claims(|state| state.declared(&kind).cloned()) else {
        return Err(store(not_open()));
    };
    let events = type_declaration_events(
        held.as_ref(),
        declared,
        clock.now(),
        session.map(str::to_string),
    );
    if events.is_empty() {
        return Ok(());
    }
    crate::writer::sql::emit(events).map_err(store)
}

/// **Commit a kind writer's transaction.** A kind writer commits natively, which
/// seals nothing, so a transaction that emitted goes through the request commit
/// that seals its events into the batch. One that emitted nothing (a seed, a
/// repeat of what the State holds) commits as it always did.
pub(super) async fn commit_kind_write(
    tx: Transaction<'_, MySql>,
    token: &str,
    clock: &Clock,
) -> Result<(), MemoryError> {
    if crate::writer::sql::has_emitted() {
        super::super::requests::commit(tx, clock, || {
            serde_json::json!({ "kind": token }).to_string()
        })
        .await
        .map_err(store)
    } else {
        super::super::requests::commit_native(tx)
            .await
            .map_err(store)
    }
}

/// **Emit the withdrawal of the kind's declaration** the domain chooses, with the
/// declaration the State holds. A bare store emits nothing; a journaled one refuses
/// a write with no writer job, and one whose writer has not opened the claims state.
pub(super) fn emit_type_withdrawn(
    token: &str,
    clock: &Clock,
    journal: Journal,
    session: Option<&str>,
) -> Result<(), MemoryError> {
    if !journal.takes_part()? {
        return Ok(());
    }
    let kind = ObjectId(token.to_string());
    let Some(held) = crate::writer::sql::read_claims(|state| state.declared(&kind).cloned()) else {
        return Err(store(not_open()));
    };
    let events = type_withdrawal_events(
        &kind,
        held.as_ref(),
        clock.now(),
        session.map(str::to_string),
    );
    if events.is_empty() {
        return Ok(());
    }
    crate::writer::sql::emit(events).map_err(store)
}

/// The refusal of a journaled store whose writer has not opened the claims state.
fn not_open() -> sqlx::Error {
    sqlx::Error::Protocol(
        "a journaled store was written before its writer opened the claims state".into(),
    )
}
