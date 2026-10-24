//! **The one-off repairs of stored rows that the boot's storage check names.**
//!
//! A repair is a startup pass: it runs once on a store, a marker in `store_pass`
//! says it has, and a boot that finds the marker reads nothing for it. Every
//! repair is listed in [`DoltMemory::repair_stored_rows`], which reads the
//! markers once and runs the ones that are owed, so a new repair is one more
//! line there and one more pass function. The question each repair asks is the
//! check's own question (rule 498): it reads the predicate the check reports by,
//! so a row the check calls damage is a row the repair looks at, and no other.
//!
//! **A repair never guesses.** A row it cannot place stays as it is, is named in
//! the log, and the check keeps reporting it.

use std::collections::{BTreeMap, BTreeSet};

use super::super::storage_check::{MERGED_WITHOUT_EVENT, THING};
use super::super::{Migrated, finished_passes, settled};
use super::*;

/// **The pass that points a bare name under a line key at the bot it names**, by
/// the name it logs under and its marker carries.
const BARE_LINE_VALUES_PASS: &str = "bare line values";

/// **The keys the damage was shown on.** A build that is gone stored the bare
/// name of a line under these as a link to nothing. The pass reads no other key:
/// a dangling link under any other key is not shown to be a bare name, and may be
/// a reference whose row was lost, which a repair must not point at another thing.
const BARE_NAME_KEYS: [&str; 1] = ["line"];

/// **The pass that gives a merge made before merge events existed its event**,
/// by the name it logs under and its marker carries.
const OLD_MERGE_EVENTS_PASS: &str = "old merge events";

/// The `session` a backfilled event wears when the store holds no moment for the
/// merge, so a reader can tell it from an event a run made.
const BACKFILLED: &str = "backfilled";

impl DoltMemory {
    /// **Run every repair that is owed, in order**, and say what each did. The
    /// markers are read once; a repair with a marker reads none of its table.
    pub async fn repair_stored_rows(&self) -> Vec<(&'static str, Migrated)> {
        let done = finished_passes(&self.pool).await;
        vec![
            self.repair_bare_line_values(&done).await,
            self.repair_old_merge_events(&done).await,
        ]
    }

    async fn repair_bare_line_values(&self, done: &BTreeSet<String>) -> (&'static str, Migrated) {
        let outcome = match done.contains(BARE_LINE_VALUES_PASS) {
            true => Migrated::Settled,
            false => {
                settled(
                    &self.pool,
                    BARE_LINE_VALUES_PASS,
                    self.point_bare_line_names_at_their_bots().await,
                )
                .await
            }
        };
        (BARE_LINE_VALUES_PASS, outcome)
    }

    async fn repair_old_merge_events(&self, done: &BTreeSet<String>) -> (&'static str, Migrated) {
        let outcome = match done.contains(OLD_MERGE_EVENTS_PASS) {
            true => Migrated::Settled,
            false => {
                settled(
                    &self.pool,
                    OLD_MERGE_EVENTS_PASS,
                    self.write_the_events_of_old_merges().await,
                )
                .await
            }
        };
        (OLD_MERGE_EVENTS_PASS, outcome)
    }

    /// **Every link under a line key whose target names no thing and is not shaped
    /// like an id this store draws, repaired when every declaration of the key
    /// points at one kind and exactly one live thing of that kind wears the name.**
    /// A target shaped like a drawn id is a reference whose row was lost, not a
    /// bare name, and is left; so is a name that also sits under another key,
    /// because `repoint_field_links` rewrites every link to a target. The write is
    /// that function, the one that rewrites a value and rebuilds its link rows for
    /// a merge. Answers how many field writes it rewrote.
    async fn point_bare_line_names_at_their_bots(&self) -> Result<usize, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let declared = Self::types_in(&mut tx).await?;
        let keys = BARE_NAME_KEYS.map(|key| format!("'{key}'")).join(", ");
        let dangling: Vec<(String, String)> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT DISTINCT r.target, r.`key` FROM field_link r
             WHERE r.`key` IN ({keys}) AND NOT {} ORDER BY 1, 2",
            THING.replace("{v}", "r.target")
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        // The handle each name could mean, by the one kind every declaration of
        // its key points at. A key whose declarations disagree, or any of which
        // points at no kind, leaves the name unplaced; so does a name shaped like
        // an id this store draws.
        let mut meant: BTreeMap<String, Option<BTreeSet<String>>> = BTreeMap::new();
        for (name, key) in &dangling {
            let kinds: BTreeSet<Option<&'static str>> = declared
                .iter()
                .filter_map(|d| d.field(key))
                .map(|field| field.points_at.map(EntityKind::as_token))
                .collect();
            let entry = meant
                .entry(name.clone())
                .or_insert_with(|| Some(BTreeSet::new()));
            match (kinds.iter().collect::<Vec<_>>().as_slice(), entry.as_mut()) {
                ([Some(kind)], Some(handles))
                    if !jojobot_domain::handle::is_drawn(name, super::super::ids::ID_LEN) =>
                {
                    handles.insert(format!("{kind}:{name}"));
                }
                _ => *entry = None,
            }
        }
        let mut rewritten = 0;
        for (name, handles) in meant {
            let elsewhere: i64 = sqlx::query_scalar(AssertSqlSafe(format!(
                "SELECT COUNT(*) FROM field_link WHERE target = ? AND `key` NOT IN ({keys})"
            )))
            .bind(&name)
            .fetch_one(&mut *tx)
            .await
            .map_err(store)?;
            let handle = match handles.as_ref().map(|h| h.iter().collect::<Vec<_>>()) {
                Some(one) if one.len() == 1 && elsewhere == 0 => one[0].clone(),
                _ => {
                    tracing::warn!(
                        name = %name,
                        "BARE REFERENCE VALUE LEFT — the name is shaped like a stored id, or the \
                         declarations of its key do not agree on one kind, or it also sits under \
                         another key, so it was not pointed at anything. A person has to say \
                         which thing it meant."
                    );
                    continue;
                }
            };
            let key: Option<String> = sqlx::query_scalar(
                "SELECT COALESCE(badge, id) FROM entity WHERE id = ? AND merged_into IS NULL",
            )
            .bind(&handle)
            .fetch_optional(&mut *tx)
            .await
            .map_err(store)?;
            let Some(key) = key else {
                tracing::warn!(
                    name = %name,
                    handle = %handle,
                    "BARE REFERENCE VALUE LEFT — nothing wears the handle it would mean, so it was \
                     not pointed at anything. A person has to say which thing it meant."
                );
                continue;
            };
            let writes = Self::repoint_field_links(&mut tx, &declared, &name, &key).await?;
            tracing::info!(
                name = %name,
                handle = %handle,
                writes,
                "store: pointed a bare name under a reference key at the thing it names"
            );
            rewritten += writes;
        }
        tx.commit().await.map_err(store)?;
        Ok(rewritten)
    }

    /// **The merge event of every thing marked merged that no event folds**, from
    /// the survivor the thing's own row records. The moment is the first write of
    /// the thing that carries the merge, and a thing with no such write gets the
    /// moment of this pass and the session [`BACKFILLED`]. A thing whose survivor
    /// has no row is left and named. Answers how many events it wrote.
    async fn write_the_events_of_old_merges(&self) -> Result<usize, MemoryError> {
        let mut tx = self.pool.begin().await.map_err(store)?;
        let merged: Vec<(String, String, Option<String>)> = sqlx::query_as(AssertSqlSafe(format!(
            "SELECT e.id, COALESCE(e.badge, e.id), e.merged_into {MERGED_WITHOUT_EVENT} \
             ORDER BY e.id"
        )))
        .fetch_all(&mut *tx)
        .await
        .map_err(store)?;
        let mut written = 0;
        for (id, key, into) in merged {
            let survivor: Option<String> = match &into {
                Some(into) => {
                    sqlx::query_scalar("SELECT COALESCE(badge, id) FROM entity WHERE id = ?")
                        .bind(into)
                        .fetch_optional(&mut *tx)
                        .await
                        .map_err(store)?
                }
                None => None,
            };
            let Some(survivor) = survivor else {
                tracing::warn!(
                    folded = %id,
                    "MERGE EVENT NOT WRITTEN — the thing is marked merged and its survivor has no \
                     row, so no event was written. A person has to say where it was folded."
                );
                continue;
            };
            let first: Option<String> = sqlx::query_scalar(
                "SELECT written_at FROM entity_write
                 WHERE entity IN (?, ?) AND merged_into IS NOT NULL
                 ORDER BY written_at LIMIT 1",
            )
            .bind(&id)
            .bind(&key)
            .fetch_optional(&mut *tx)
            .await
            .map_err(store)?;
            let (at, session) = match first.filter(|at| at.parse::<jiff::Timestamp>().is_ok()) {
                Some(at) => (at, None),
                None => (self.clock.now().to_string(), Some(BACKFILLED)),
            };
            append_merge_event(
                &mut tx,
                &EntityId(key),
                &EntityId(survivor),
                at,
                session,
                &[],
            )
            .await?;
            tracing::info!(
                folded = %id,
                "store: wrote the merge event of a merge made before merge events existed"
            );
            written += 1;
        }
        tx.commit().await.map_err(store)?;
        Ok(written)
    }
}
