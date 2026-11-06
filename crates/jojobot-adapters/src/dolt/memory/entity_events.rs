//! **The claims events of an entity's own row**, emitted into the transaction's
//! one batch when a writer changes the row. The row is read back from the table
//! inside the transaction, as the version kept for it is, and turned into the
//! object by the same function the import uses (`object_body`), so the log never
//! holds two readings of one row.
//!
//! A boot row repair and a merge call the version hook without calling this: a
//! repair never emits (it would make the import skip), and the merge is the claim
//! writers' to emit.

use jiff::Timestamp;
use jojobot_domain::log::ObjectId;
use jojobot_domain::log::claims::ClaimEvent;
use jojobot_domain::log::claims::import::{EntityRow, live, object_body};

use super::import_rows::{maybe, text};
use super::*;

/// What a writer did to the row, and so which events follow.
pub(super) enum Change<'a> {
    /// The row did not exist before: the object is created and its handle bound.
    Created,
    /// The row was rewritten under the same handle.
    Revised,
    /// The row now answers to a new handle: the old one is retired and the new
    /// one bound.
    Renamed { from: &'a EntityId },
}

/// Emit the events of `entity`'s row as it stands in `tx`.
pub(super) async fn emit_object_write(
    tx: &mut Transaction<'_, MySql>,
    entity: &EntityId,
    change: Change<'_>,
    clock: &Clock,
    session: Option<&str>,
) -> Result<(), MemoryError> {
    // **Outside a writer job there is no batch to join**: the writes the one
    // writer serves are the log's, and a boot pass on the bare store never emits.
    if !crate::writer::sql::in_writer_job() {
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
    let body = object_body(&EntityRow {
        key: key.clone(),
        handle: handle.clone(),
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
    });
    let (at, run) = (clock.now(), session.map(str::to_string));
    let event = |payload| live(payload, at, run.clone());
    let mut events = Vec::new();
    match change {
        Change::Created => {
            events.push(event(ClaimEvent::ObjectCreated {
                object: key.clone(),
                body,
            }));
            events.push(event(ClaimEvent::AddressBound {
                object: key,
                address: handle,
            }));
        }
        Change::Revised => events.push(event(ClaimEvent::ObjectRevised { object: key, body })),
        Change::Renamed { from } => {
            events.push(event(ClaimEvent::ObjectRevised {
                object: key.clone(),
                body,
            }));
            events.push(event(ClaimEvent::AddressRetired {
                object: key.clone(),
                address: from.to_string(),
            }));
            events.push(event(ClaimEvent::AddressBound {
                object: key,
                address: handle,
            }));
        }
    }
    crate::writer::sql::emit(events).map_err(store)
}
