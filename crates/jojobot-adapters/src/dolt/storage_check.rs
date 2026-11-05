//! **The queries behind the boot's storage check.** One connection, one read
//! transaction: the pass sees one state of the store and writes nothing. It does
//! not use the fold's load width.
//!
//! Each class is one function returning the whole count and up to
//! [`IDS_SHOWN`] ids. The schema has no foreign keys, so a class is a question
//! asked of rows that should agree.

use std::collections::{BTreeMap, BTreeSet};

use jojobot_domain::memory::{EntityId, FactAddress, FactId, MemoryError};
use jojobot_domain::storage_check::{
    Finding, IDS_SHOWN, StorageCheck, StorageCheckError, StorageClass,
};
use sqlx::{MySql, MySqlPool, Transaction};

use super::memory::DoltMemory;

fn store(e: sqlx::Error) -> StorageCheckError {
    StorageCheckError(format!("the store could not be read: {e}"))
}

/// The Dolt store, read for damage.
#[derive(Clone)]
pub struct DoltStorageCheck {
    pool: MySqlPool,
}

impl DoltStorageCheck {
    pub fn open(pool: MySqlPool) -> Self {
        DoltStorageCheck { pool }
    }
}

#[async_trait::async_trait]
impl StorageCheck for DoltStorageCheck {
    async fn check_storage(&self) -> Result<Vec<Finding>, StorageCheckError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let mut found = Vec::new();
        let mut merge_event_loops = Counted::default();
        for class in StorageClass::ALL {
            let (count, ids) = match class {
                StorageClass::MergedWithoutEvent => merged_without_event(&mut tx).await?,
                StorageClass::MergeEventNotApplied => merge_event_not_applied(&mut tx).await?,
                StorageClass::MergedIntoNothing => merged_into_nothing(&mut tx).await?,
                StorageClass::ClaimUnderFoldedKeyUnmapped => {
                    claims_the_merge_does_not_list(&mut tx).await?
                }
                StorageClass::DanglingReference => {
                    dangling_references(&mut tx, &mut merge_event_loops).await?
                }
                StorageClass::MergeEventLoop => std::mem::take(&mut merge_event_loops),
                StorageClass::HandleHomedWrite => writes_homed_under_a_handle(&mut tx).await?,
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
/// event names it by key, as A1 reads it. Each id says which of the two is
/// wrong, because the way out differs: a row can be marked, a missing row cannot.
async fn merge_event_not_applied(
    tx: &mut Transaction<'_, MySql>,
) -> Result<Counted, StorageCheckError> {
    ids_of(
        tx,
        "SELECT DISTINCT CONCAT(m.folded, IF(e.id IS NULL, ': no row', ': not marked merged'))
         FROM merge_event m
         LEFT JOIN entity e ON COALESCE(e.badge, e.id) = m.folded
         WHERE m.ordinal = 0 AND (e.id IS NULL OR e.merged_into IS NULL)
         ORDER BY 1",
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

/// **A link row whose write no later write of the same key on the same record has
/// replaced.** An edit appends a write beside the old one and leaves the old write
/// and its link rows, so a correction of a dangling value leaves the dangling row
/// behind as history. Every read decides who points at a thing from the records'
/// current values and none serves a replaced write's target, so such a row is not
/// damage. A link row with no write at all is still damage.
const LINK_OF_A_CURRENT_WRITE: &str = "NOT EXISTS (
         SELECT 1 FROM field_write w
         JOIN field_write later ON later.entity = w.entity AND later.`key` = w.`key`
                               AND later.fact_id = w.fact_id AND later.ordinal > w.ordinal
         WHERE w.entity = r.entity AND w.`key` = r.`key` AND w.ordinal = r.ordinal)";

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
    (
        "field_link.target",
        "field_link",
        "target",
        LINK_OF_A_CURRENT_WRITE,
    ),
    ("mailbox.owner", "mailbox", "owner", "TRUE"),
    ("session.bot", "session", "bot", "TRUE"),
    ("fact_write.bot", "fact_write", "bot", "TRUE"),
    // A claim derived from another names the thing that other claim is homed
    // under; the claim itself is checked below. **A source homed under a handle
    // is left alone, as a claim or field write homed under one is**: the handle may
    // be one a build once supplied, which the boot gives a row, so only a source
    // homed under a bare badge is checked.
    (
        "fact.derived_from",
        "fact",
        "derived_from",
        "r.`derived_from` NOT LIKE '%:%'",
    ),
    (
        "fact_write.derived_from",
        "fact_write",
        "derived_from",
        "r.`derived_from` NOT LIKE '%:%'",
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
    merge_event_loops: &mut Counted,
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
            match DoltMemory::written_at_or_before(tx, &source).await {
                Ok(Some(_)) => {}
                Ok(None) => {
                    add((count as u64, vec![format!("{label}: {home}#{local}")]));
                }
                Err(MemoryError::StoredDamage { .. }) => {
                    merge_event_loops.0 += count as u64;
                    merge_event_loops.1.push(format!("{label}: {home}#{local}"));
                }
                Err(e) => {
                    return Err(StorageCheckError(format!(
                        "the store could not be read: {e}"
                    )));
                }
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
