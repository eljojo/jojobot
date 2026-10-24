//! **The queries behind the boot's storage check.** One connection, one read
//! transaction: the pass sees one state of the store and writes nothing. It does
//! not use the fold's load width.
//!
//! Each class is one function returning the whole count and up to
//! [`IDS_SHOWN`] ids. The schema has no foreign keys, so a class is a question
//! asked of rows that should agree.

use std::collections::{BTreeMap, BTreeSet};

use jiff::{SignedDuration, Timestamp};
use jojobot_domain::clock::Clock;
use jojobot_domain::memory::{EntityId, FactAddress, FactId};
use jojobot_domain::request::KEPT_FOR;
use jojobot_domain::storage_check::{
    Finding, IDS_SHOWN, StorageCheck, StorageCheckError, StorageClass,
};
use sqlx::{MySql, MySqlPool, Transaction};

use super::memory::DoltMemory;

/// A receipt written this long ago and still empty was lost.
const RECEIPT_GRACE: SignedDuration = SignedDuration::from_mins(10);

fn stamp(at: Timestamp) -> String {
    at.strftime("%Y-%m-%dT%H:%M:%S%.9fZ").to_string()
}

fn store(e: sqlx::Error) -> StorageCheckError {
    StorageCheckError(format!("the store could not be read: {e}"))
}

/// The Dolt store, read for damage.
#[derive(Clone)]
pub struct DoltStorageCheck {
    pool: MySqlPool,
    clock: Clock,
}

impl DoltStorageCheck {
    pub fn open(pool: MySqlPool, clock: Clock) -> Self {
        DoltStorageCheck { pool, clock }
    }
}

#[async_trait::async_trait]
impl StorageCheck for DoltStorageCheck {
    async fn check_storage(&self) -> Result<Vec<Finding>, StorageCheckError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let now = self.clock.now();
        let mut found = Vec::new();
        for class in StorageClass::ALL {
            let (count, ids) = match class {
                StorageClass::MergedWithoutEvent => merged_without_event(&mut tx).await?,
                StorageClass::MergeEventNotApplied => merge_event_not_applied(&mut tx).await?,
                StorageClass::MergedIntoNothing => merged_into_nothing(&mut tx).await?,
                StorageClass::ClaimUnderFoldedKeyUnmapped => {
                    claims_the_merge_does_not_list(&mut tx).await?
                }
                StorageClass::DanglingReference => dangling_references(&mut tx).await?,
                StorageClass::HandleHomedWrite => writes_homed_under_a_handle(&mut tx).await?,
                StorageClass::LandedMergeMissing => landed_merges_missing(&mut tx).await?,
                StorageClass::HalfDoneWrite => half_done_writes(&mut tx, now).await?,
                StorageClass::LostReceipt => lost_receipts(&mut tx, now).await?,
            };
            found.push(Finding {
                class: *class,
                count,
                ids: ids.into_iter().take(IDS_SHOWN).collect(),
            });
        }
        // Nothing was written; the transaction ends by being dropped.
        Ok(found)
    }
}

type Counted = (u64, Vec<String>);

async fn ids_of(tx: &mut Transaction<'_, MySql>, sql: &str) -> Result<Counted, StorageCheckError> {
    let ids: Vec<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(sql))
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
    Ok((ids.len() as u64, ids))
}

/// **A1.** A merge's first row names the folded thing by its key: the permanent
/// badge when the row has one, else its id.
async fn merged_without_event(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    ids_of(
        tx,
        &format!("SELECT e.id {MERGED_WITHOUT_EVENT} ORDER BY e.id"),
    )
    .await
}

/// **The rows A1 names**, the one question the check and the repair of old merges
/// both ask: a thing marked merged whose key no merge event folds.
pub(super) const MERGED_WITHOUT_EVENT: &str = "FROM entity e
         WHERE e.merged_into IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM merge_event m WHERE m.ordinal = 0 AND m.folded = COALESCE(e.badge, e.id))";

/// **A2.** A merge event's folded thing is marked merged, and it exists. The
/// event names it by key, as A1 reads it.
async fn merge_event_not_applied(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    ids_of(
        tx,
        "SELECT DISTINCT m.folded FROM merge_event m
         LEFT JOIN entity e ON COALESCE(e.badge, e.id) = m.folded
         WHERE m.ordinal = 0 AND (e.id IS NULL OR e.merged_into IS NULL)
         ORDER BY m.folded",
    )
    .await
}

/// **A3.** The chain of `merged_into` ends at a thing that exists and is not
/// merged. A chain that names nothing, or comes back to a thing it passed, is
/// damage. Every merged row is read once and the chains are followed in memory.
async fn merged_into_nothing(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    let rows: Vec<(String, Option<String>)> =
        sqlx::query_as("SELECT id, merged_into FROM entity ORDER BY id")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
    let next: BTreeMap<&str, Option<&str>> = rows
        .iter()
        .map(|(id, into)| (id.as_str(), into.as_deref()))
        .collect();
    let mut damaged = Vec::new();
    for (id, into) in &rows {
        if into.is_none() {
            continue;
        }
        let mut seen: BTreeSet<&str> = BTreeSet::from([id.as_str()]);
        let mut at = into.as_deref();
        let broken = loop {
            match at {
                None => break false,
                Some(handle) => match next.get(handle) {
                    None => break true,
                    Some(_) if !seen.insert(handle) => break true,
                    Some(onward) => at = *onward,
                },
            }
        };
        if broken {
            damaged.push(id.clone());
        }
    }
    Ok((damaged.len() as u64, damaged))
}

/// **A4.** A claim stored under a merged thing's key is listed by one of that
/// merge's claim rows, which name the folded key and the claim's id.
async fn claims_the_merge_does_not_list(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    ids_of(
        tx,
        "SELECT CONCAT(e.id, '#', f.id) FROM fact f
         JOIN entity e ON COALESCE(e.badge, e.id) = f.entity
         WHERE e.merged_into IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM merge_event m
                           WHERE m.ordinal > 0 AND m.folded = f.entity AND m.folded_id = f.id)
         ORDER BY e.id, f.id",
    )
    .await
}

/// **A thing a reference can name**: a stored row by its key (the permanent
/// badge, or the id of a row that has none) or by its handle, or a handle the
/// build supplies as a record, which has no row.
///
/// **One test for each key, never `COALESCE(badge, id) = v`.** The planner cannot
/// use an expression, so each claim and field write scanned every thing and the
/// check's time grew with rows times things. The two tests together hold the same
/// rows, because a row without a badge is found by its id.
pub(super) const THING: &str = "(EXISTS (SELECT 1 FROM entity t WHERE t.badge = {v})
     OR EXISTS (SELECT 1 FROM entity t WHERE t.id = {v})
     OR EXISTS (SELECT 1 FROM supplied_handle s WHERE s.handle = {v}))";

/// **B.** Each entry is a label an id carries, the table, the column that names a
/// thing, and a further condition on the value. A row whose column is set and
/// names no thing is damage.
///
/// **A claim or field write homed under a handle is not checked**, only one homed
/// under a bare badge. The boot leaves writes under a handle no build supplied
/// alone and says nothing about them: the handle may be one a build once supplied
/// (the boot gives it a row) or a row removed by hand, and neither is the check's
/// to name. A badge no row wears is a row that was lost.
const THING_REFERENCES: &[(&str, &str, &str, &str)] = &[
    ("entity.parent", "entity", "parent", "TRUE"),
    ("fact.edge_object", "fact", "edge_object", "TRUE"),
    ("fact.entity", "fact", "entity", "r.`entity` NOT LIKE '%:%'"),
    (
        "field_write.entity",
        "field_write",
        "entity",
        "r.`entity` NOT LIKE '%:%'",
    ),
    ("field_link.target", "field_link", "target", "TRUE"),
    ("mailbox.owner", "mailbox", "owner", "TRUE"),
    ("session.bot", "session", "bot", "TRUE"),
    ("fact_write.bot", "fact_write", "bot", "TRUE"),
    // A claim derived from another names the thing that other claim is homed
    // under; the claim itself is checked below.
    ("fact.derived_from", "fact", "derived_from", "TRUE"),
    (
        "fact_write.derived_from",
        "fact_write",
        "derived_from",
        "TRUE",
    ),
];

/// **B, the writes the check leaves out of the dangling class.** A claim or field
/// write homed under a handle no row wears and no build supplies. A claim is
/// named by its address and a field write by its home and key.
async fn writes_homed_under_a_handle(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    let mut found: Vec<String> = Vec::new();
    for sql in [
        "SELECT DISTINCT CONCAT('fact: ', r.entity, '#', r.id) FROM fact r",
        "SELECT DISTINCT CONCAT('field_write: ', r.entity, '/', r.`key`) FROM field_write r",
    ] {
        let sql = format!(
            "{sql} WHERE r.entity LIKE '%:%' AND NOT {} ORDER BY 1",
            THING.replace("{v}", "r.entity")
        );
        found.extend(ids_of(tx, &sql).await?.1);
    }
    Ok((found.len() as u64, found))
}

/// **Rows counted, values named.** The ids a query names, each once, and how many
/// rows hold them: fifty claims under one ghost badge are fifty rows damaged
/// under one name. `sql` selects the id and its row count, grouped by the id.
async fn named_rows_of(
    tx: &mut Transaction<'_, MySql>,
    sql: &str,
) -> Result<Counted, StorageCheckError> {
    let named: Vec<(String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
    let rows = named.iter().map(|(_, rows)| *rows as u64).sum();
    Ok((rows, named.into_iter().map(|(id, _)| id).collect()))
}

/// **B.** Dangling references: the column and the value it holds, named once, with
/// every row that holds it counted.
async fn dangling_references(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    let mut found: Vec<String> = Vec::new();
    let mut rows: u64 = 0;
    let mut add = |(counted, named): Counted| {
        rows += counted;
        found.extend(named);
    };
    for (label, table, column, only_when) in THING_REFERENCES {
        let value = format!("r.`{column}`");
        let sql = format!(
            "SELECT CONCAT('{label}: ', {value}) AS id, COUNT(*) FROM `{table}` r
             WHERE {value} IS NOT NULL AND {only_when} AND NOT {}
             GROUP BY 1 ORDER BY 1",
            THING.replace("{v}", &value)
        );
        add(named_rows_of(tx, &sql).await?);
    }
    // A claim derived from a claim names the claim's home and its local id. The
    // home is checked above; a home that stands and holds no such claim is named
    // here, by the address the claim was derived from. **A source named by the id
    // a merge gave it is a source**: no row is stored at that address, so the
    // candidates are resolved the way a capture resolves its source.
    for (label, table) in [
        ("fact.derived_from_id", "fact"),
        ("fact_write.derived_from_id", "fact_write"),
    ] {
        let sql = format!(
            "SELECT r.derived_from, r.derived_from_id, COUNT(*)
             FROM `{table}` r
             WHERE r.derived_from IS NOT NULL AND r.derived_from_id IS NOT NULL
               AND r.derived_from NOT LIKE '%:%' AND {home}
               AND NOT EXISTS (SELECT 1 FROM fact s
                               WHERE s.entity = r.derived_from AND s.id = r.derived_from_id)
             GROUP BY 1, 2 ORDER BY 1, 2",
            home = THING.replace("{v}", "r.derived_from")
        );
        let candidates: Vec<(String, String, i64)> = sqlx::query_as(sqlx::AssertSqlSafe(sql))
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
        for (home, local, count) in candidates {
            let source = FactAddress::new(EntityId(home.clone()), FactId(local.clone()));
            let written = DoltMemory::written_at_or_before(tx, &source)
                .await
                .map_err(|e| StorageCheckError(format!("the store could not be read: {e}")))?;
            if written.is_none() {
                add((count as u64, vec![format!("{label}: {home}#{local}")]));
            }
        }
    }
    // A message names a mailbox, and an event names a message.
    for sql in [
        "SELECT CONCAT('message.mailbox: ', m.mailbox) AS id, COUNT(*) FROM message m
         WHERE NOT EXISTS (SELECT 1 FROM mailbox b WHERE b.name = m.mailbox)
         GROUP BY 1 ORDER BY 1",
        "SELECT CONCAT('message_event.message_id: ', v.message_id) AS id, COUNT(*)
         FROM message_event v
         WHERE NOT EXISTS (SELECT 1 FROM message m WHERE m.id = v.message_id)
         GROUP BY 1 ORDER BY 1",
        "SELECT CONCAT('entity_former_handle.badge: ', h.badge) AS id, COUNT(*)
         FROM entity_former_handle h
         WHERE NOT EXISTS (SELECT 1 FROM entity t WHERE t.badge = h.badge)
         GROUP BY 1 ORDER BY 1",
    ] {
        add(named_rows_of(tx, sql).await?);
    }
    Ok((rows, found))
}

/// **D2.** A `merge_entities` receipt names, in `landed`, the folded thing and the
/// survivor by the handles they had when the merge landed. The folded thing must
/// be merged, and the chain of `merged_into` from it must end where the chain
/// from the survivor ends. A handle that was renamed since is found through the
/// former-handle table. Every receipt in the retention window is checked.
async fn landed_merges_missing(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    let receipts: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT bot_key, request_id, landed FROM request_receipt
         WHERE verb = 'merge_entities' ORDER BY written_at",
    )
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    if receipts.is_empty() {
        return Ok((0, Vec::new()));
    }
    let things: Vec<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT id, badge, merged_into FROM entity")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
    let former: Vec<(String, String)> =
        sqlx::query_as("SELECT former_handle, badge FROM entity_former_handle")
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
    let by_handle: BTreeMap<&str, usize> = things
        .iter()
        .enumerate()
        .map(|(at, (id, _, _))| (id.as_str(), at))
        .collect();
    let by_badge: BTreeMap<&str, usize> = things
        .iter()
        .enumerate()
        .filter_map(|(at, (_, badge, _))| badge.as_deref().map(|badge| (badge, at)))
        .collect();
    // **One row per thing that left the handle**, so a handle can map to several.
    let mut by_former: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (handle, badge) in &former {
        by_former
            .entry(handle.as_str())
            .or_default()
            .push(badge.as_str());
    }
    // **Every thing a handle named, not the first.** A handle a rename freed can
    // be taken by a new thing inside the retention window, so one handle is
    // current for one thing and former for any number of others. The receipt
    // means the one that wore it when the merge landed, which this row alone
    // cannot say.
    let wearers = |handle: &str| -> Vec<usize> {
        let current = by_handle.get(handle).copied();
        let former = by_former
            .get(handle)
            .into_iter()
            .flatten()
            .filter_map(|badge| by_badge.get(badge).copied())
            .filter(|at| Some(*at) != current);
        current.into_iter().chain(former).collect()
    };
    // Where a thing's chain of `merged_into` ends, and whether it was merged at
    // all. A loop has no end.
    let end_of = |start: usize| -> Option<usize> {
        let mut at = start;
        let mut seen = BTreeSet::from([at]);
        while let Some(into) = &things[at].2 {
            at = *by_handle.get(into.as_str())?;
            if !seen.insert(at) {
                return None;
            }
        }
        Some(at)
    };
    let mut missing = Vec::new();
    for (bot, request, landed) in receipts {
        let named: Option<(String, String)> = serde_json::from_str::<serde_json::Value>(&landed)
            .ok()
            .and_then(|v| {
                Some((
                    v.get("folded")?.as_str()?.to_string(),
                    v.get("survivor")?.as_str()?.to_string(),
                ))
            });
        let Some((folded, survivor)) = named else {
            missing.push(format!(
                "{bot}/{request}: the receipt's landed record does not name a merge"
            ));
            continue;
        };
        let survivor_ends: Vec<usize> = wearers(&survivor).into_iter().filter_map(end_of).collect();
        let held = wearers(&folded)
            .into_iter()
            .filter(|at| things[*at].2.is_some())
            .filter_map(end_of)
            .any(|folded_end| survivor_ends.contains(&folded_end));
        if !held {
            missing.push(format!("{bot}/{request}: {folded} into {survivor}"));
        }
    }
    Ok((missing.len() as u64, missing))
}

/// **D1.** Parts with no receipt for the same bot and request id, inside the
/// window the ledger keeps ids for. One id per request: its verb, its part count
/// and when its oldest part landed.
///
/// **A part older than the window is not damage.** A write purges a receipt
/// older than the window on every claim, and a part only on a claim that is a
/// part, so a request that finished leaves parts with no receipt once it ages.
/// A request cut part-way and left past the window cannot be finished by a
/// resend either: the ledger no longer reads its parts.
async fn half_done_writes(
    tx: &mut Transaction<'_, MySql>,
    now: Timestamp,
) -> Result<Counted, StorageCheckError> {
    let rows: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT p.verb, p.bot_key, p.request_id, COUNT(*), MIN(p.written_at)
         FROM request_part p
         WHERE p.written_at >= ?
           AND NOT EXISTS (SELECT 1 FROM request_receipt r
                           WHERE r.bot_key = p.bot_key AND r.request_id = p.request_id)
         GROUP BY p.verb, p.bot_key, p.request_id
         ORDER BY MIN(p.written_at)",
    )
    .bind(stamp(now - KEPT_FOR))
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    let ids = rows
        .into_iter()
        .map(|(verb, bot, request, parts, since)| {
            format!("{verb} {bot}/{request}: {parts} parts since {since}")
        })
        .collect::<Vec<_>>();
    Ok((ids.len() as u64, ids))
}

/// **D3.** A receipt row with no receipt body, written before the grace period.
async fn lost_receipts(
    tx: &mut Transaction<'_, MySql>,
    now: Timestamp,
) -> Result<Counted, StorageCheckError> {
    let rows: Vec<(String, String, String, String)> = sqlx::query_as(
        "SELECT verb, bot_key, request_id, written_at FROM request_receipt
         WHERE receipt IS NULL AND written_at < ?
         ORDER BY written_at",
    )
    .bind(stamp(now - RECEIPT_GRACE))
    .fetch_all(&mut **tx)
    .await
    .map_err(store)?;
    let ids = rows
        .into_iter()
        .map(|(verb, bot, request, at)| format!("{verb} {bot}/{request}: written {at}"))
        .collect::<Vec<_>>();
    Ok((ids.len() as u64, ids))
}
