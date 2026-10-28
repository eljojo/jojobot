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

use std::collections::BTreeSet;

use super::super::storage_check::MERGED_WITHOUT_EVENT;
use super::super::{Migrated, finished_passes, settled};
use super::*;

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
        vec![self.repair_old_merge_events(&done).await]
    }

    // LEGACY_READER(old-merge-event): work:q-gate-binary-coverage
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
        let (mut written, mut left) = (0, 0);
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
                left += 1;
                tracing::warn!(
                    folded = %id,
                    survivor = into.as_deref().unwrap_or("none"),
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
            let moment = if session.is_some() {
                "backfilled"
            } else {
                "recorded"
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
                survivor = into.as_deref().unwrap_or("none"),
                moment,
                "store: repair wrote the merge event of a merge made before merge events existed"
            );
            written += 1;
        }
        tx.commit().await.map_err(store)?;
        tracing::info!(
            written,
            left,
            "store: repair of old merge events: events written and merges left"
        );
        Ok(written)
    }
}
