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
        let (mut rewritten, mut left) = (0, 0);
        for (name, handles) in meant {
            // Each link row the name sits in, taken before anything changes it, so
            // that the log can name every row the pass changed and every row it left.
            let rows: Vec<(String, String, i64)> = sqlx::query_as(AssertSqlSafe(format!(
                "SELECT entity, `key`, ordinal FROM field_link
                 WHERE target = ? AND `key` IN ({keys}) ORDER BY 1, 2, 3"
            )))
            .bind(&name)
            .fetch_all(&mut *tx)
            .await
            .map_err(store)?;
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
                    left += rows.len();
                    self.say_rows_left(&mut tx, &rows, &name, "the name is shaped like a stored id, or the declarations of its key do not agree on one kind, or it also sits under another key").await?;
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
                left += rows.len();
                self.say_rows_left(
                    &mut tx,
                    &rows,
                    &name,
                    "nothing wears the handle it would mean",
                )
                .await?;
                continue;
            };
            let mut addresses = Vec::with_capacity(rows.len());
            for (entity, row_key, ordinal) in &rows {
                addresses
                    .push(Self::address_of_link_row(&mut tx, entity, row_key, *ordinal).await?);
            }
            let writes = Self::repoint_field_links(&mut tx, &declared, &name, &key).await?;
            for (address, (_, row_key, _)) in addresses.iter().zip(&rows) {
                tracing::info!(
                    address = %address,
                    key = %row_key,
                    old_target = %name,
                    new_target = %handle,
                    "store: repair changed a bare line link"
                );
            }
            rewritten += writes;
        }
        tx.commit().await.map_err(store)?;
        tracing::info!(
            changed = rewritten,
            left,
            "store: repair of bare line values: rows changed and rows left"
        );
        Ok(rewritten)
    }

    /// **Name each link row a repair left**, one warning each, with the address of
    /// the claim that holds it, its key, its target and why it was left. A target
    /// is a stored name or id, never the text of a value.
    async fn say_rows_left(
        &self,
        tx: &mut Transaction<'_, MySql>,
        rows: &[(String, String, i64)],
        target: &str,
        why: &str,
    ) -> Result<(), MemoryError> {
        for (entity, key, ordinal) in rows {
            let address = Self::address_of_link_row(tx, entity, key, *ordinal).await?;
            tracing::warn!(
                address = %address,
                key = %key,
                target = %target,
                why,
                "BARE REFERENCE VALUE LEFT — a person has to say which thing it meant"
            );
        }
        Ok(())
    }

    /// **The address of the claim a link row came from**: the holder's handle and
    /// the claim's local id, or the stored key when the holder has no handle row.
    async fn address_of_link_row(
        tx: &mut Transaction<'_, MySql>,
        entity: &str,
        key: &str,
        ordinal: i64,
    ) -> Result<String, MemoryError> {
        let holder: Option<String> =
            sqlx::query_scalar("SELECT id FROM entity WHERE badge = ? OR id = ? LIMIT 1")
                .bind(entity)
                .bind(entity)
                .fetch_optional(&mut **tx)
                .await
                .map_err(store)?;
        let claim: Option<String> = sqlx::query_scalar(
            "SELECT fact_id FROM field_write WHERE entity = ? AND `key` = ? AND ordinal = ?",
        )
        .bind(entity)
        .bind(key)
        .bind(ordinal)
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        Ok(format!(
            "{}#{}",
            holder.as_deref().unwrap_or(entity),
            claim.as_deref().unwrap_or("none")
        ))
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
