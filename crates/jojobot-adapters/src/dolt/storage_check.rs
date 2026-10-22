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
use jojobot_domain::storage_check::{
    Finding, IDS_SHOWN, StorageCheck, StorageCheckError, StorageClass,
};
use sqlx::{MySql, MySqlPool, Transaction};

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
                StorageClass::HalfDoneWrite => half_done_writes(&mut tx).await?,
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
        "SELECT e.id FROM entity e
         WHERE e.merged_into IS NOT NULL
           AND NOT EXISTS (SELECT 1 FROM merge_event m WHERE m.ordinal = 0 AND m.folded = COALESCE(e.badge, e.id))
         ORDER BY e.id",
    )
    .await
}

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

/// **D1.** Parts with no receipt for the same bot and request id. One id per
/// request: its verb, its part count and when its oldest part landed.
async fn half_done_writes(tx: &mut Transaction<'_, MySql>) -> Result<Counted, StorageCheckError> {
    let rows: Vec<(String, String, String, i64, String)> = sqlx::query_as(
        "SELECT p.verb, p.bot_key, p.request_id, COUNT(*), MIN(p.written_at)
         FROM request_part p
         WHERE NOT EXISTS (SELECT 1 FROM request_receipt r
                           WHERE r.bot_key = p.bot_key AND r.request_id = p.request_id)
         GROUP BY p.verb, p.bot_key, p.request_id
         ORDER BY MIN(p.written_at)",
    )
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
