//! **The reads of the merge events the store walks with**, each handing the walk
//! the one step it asks for. The walks themselves are the domain's, in
//! [`jojobot_domain::memory::claim_walk`]: this file supplies what a step reads
//! from the `merge_event` table and nothing that decides where a walk goes or
//! when it ends.

use jojobot_domain::memory::claim_walk::{ClaimMove, ClaimWalk, FoldedInto, MergeLoop, ThingWalk};
use jojobot_domain::memory::{EntityId, FactAddress, FactId, MemoryError};
use sqlx::{MySql, Row, Transaction};

use super::{DoltMemory, Wearer, store};

impl DoltMemory {
    /// **Where the merge events say a claim went**, by the walk the domain owns:
    /// this supplies the events that moved the address the walk stands at.
    /// `None` when no event moved it. `address.home` must be a storage key.
    pub(super) async fn walk_claim(
        tx: &mut Transaction<'_, MySql>,
        address: &FactAddress,
    ) -> Result<Option<FactAddress>, MemoryError> {
        let mut walk = ClaimWalk::new(address.home.clone(), address.local.clone());
        loop {
            let (home, local) = walk.at();
            let rows = sqlx::query(
                "SELECT merge_no, survivor, survivor_id FROM merge_event
                 WHERE folded = ? AND folded_id = ? AND ordinal > 0",
            )
            .bind(home.as_str())
            .bind(local.as_str())
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
            let mut moves = Vec::with_capacity(rows.len());
            for row in &rows {
                moves.push(ClaimMove {
                    order: row.try_get::<i64, _>("merge_no").map_err(store)? as u64,
                    survivor: EntityId(row.try_get::<String, _>("survivor").map_err(store)?),
                    survivor_id: FactId(row.try_get::<String, _>("survivor_id").map_err(store)?),
                });
            }
            if !walk.step(moves) {
                return match walk.finish() {
                    Ok(end) => Ok(end.map(|(home, local)| FactAddress::new(home, local))),
                    Err(looped) => Err(Self::damage_of(tx, looped).await),
                };
            }
        }
    }

    /// **One step back along the events from the address a walk stands at**: the
    /// key and id a merge took the claim from, when a merge gave the claim this
    /// address. The walk of an address back to the claim it names, and the walk of
    /// a claim to every address it has had, both take their step here. `true` when
    /// the walk moved.
    pub(super) async fn step_back(
        tx: &mut Transaction<'_, MySql>,
        walk: &mut ClaimWalk<EntityId>,
    ) -> Result<bool, MemoryError> {
        let (home, local) = walk.at();
        let rows = sqlx::query(
            "SELECT merge_no, folded, folded_id FROM merge_event
             WHERE survivor = ? AND survivor_id = ? AND ordinal > 0",
        )
        .bind(home.as_str())
        .bind(local.as_str())
        .fetch_all(&mut **tx)
        .await
        .map_err(store)?;
        let mut origins = Vec::with_capacity(rows.len());
        for row in &rows {
            origins.push(ClaimMove {
                order: row.try_get::<i64, _>("merge_no").map_err(store)? as u64,
                survivor: EntityId(row.try_get::<String, _>("folded").map_err(store)?),
                survivor_id: FactId(row.try_get::<String, _>("folded_id").map_err(store)?),
            });
        }
        Ok(walk.step(origins))
    }

    /// **The key a pointer to a thing is served as**: the thing itself, or the
    /// thing it was merged into when the merge events say so, through a survivor
    /// that was merged later. A pointer a merge rewrote already names the
    /// survivor and is returned as it is; one it left naming the folded thing is
    /// followed.
    pub(super) async fn forward_key(
        &self,
        tx: &mut Transaction<'_, MySql>,
        key: &EntityId,
    ) -> Result<EntityId, MemoryError> {
        let mut walk = ThingWalk::new(key.clone());
        loop {
            let survivor = self.survivor_of(tx, walk.at()).await?;
            if !walk.step(survivor) {
                return match walk.finish() {
                    Ok(end) => Ok(end),
                    Err(looped) => Err(Self::damage_of(tx, looped).await),
                };
            }
        }
    }

    /// **The things a read serves under `folded`'s survivor because of this
    /// merge**: the live children of `folded` and of everything folded into it.
    /// A merge moves no row, so each still holds the key of the parent it was
    /// made under. Returned by the handle the row is stored under.
    ///
    /// `bare` is [`Self::folds_without_an_event`], which the caller reads once for
    /// the whole walk: a walk over the members of a collection asks this once for
    /// each member, and a read of the same set for each would cost a statement
    /// for each.
    pub(super) async fn children_served_under(
        &self,
        tx: &mut Transaction<'_, MySql>,
        folded: &EntityId,
        bare: &[String],
    ) -> Result<Vec<EntityId>, MemoryError> {
        let mut parents = vec![folded.clone()];
        parents.extend(self.fold_sources_of(tx, folded, bare).await?);
        let mut children = Vec::new();
        for parent in parents {
            let held: Vec<String> = sqlx::query_scalar(
                "SELECT id FROM entity WHERE parent = ? AND merged_into IS NULL ORDER BY id",
            )
            .bind(parent.as_str())
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
            children.extend(held.into_iter().map(EntityId));
        }
        Ok(children)
    }

    /// **The keys of the things folded with no merge event naming them**: the set
    /// a forwarding row alone places, which is small and fixed once the build
    /// keeps events. A walk reads it once and hands it to every step.
    pub(super) async fn folds_without_an_event(
        tx: &mut Transaction<'_, MySql>,
    ) -> Result<Vec<String>, MemoryError> {
        sqlx::query_scalar(
            "SELECT COALESCE(badge, id) FROM entity
             WHERE merged_into IS NOT NULL
               AND COALESCE(badge, id) NOT IN (SELECT folded FROM merge_event WHERE ordinal = 0)",
        )
        .fetch_all(&mut **tx)
        .await
        .map_err(store)
    }

    /// **Every key folded into `survivor`, directly or through the things
    /// folded into those**: the merge events in the order the merges were made,
    /// then what only a forwarding row says. A fold made before events were kept
    /// has no event, and [`Self::survivor_of`] reads its forwarding row, so a
    /// walk that asked the events alone would serve the survivor without the
    /// children such a fold gave it, where `index()` serves them.
    async fn fold_sources_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        survivor: &EntityId,
        bare: &[String],
    ) -> Result<Vec<EntityId>, MemoryError> {
        let mut keys: Vec<EntityId> = Self::merges_into(tx, survivor)
            .await?
            .into_iter()
            .map(|step| EntityId(step.folded))
            .collect();
        if bare.is_empty() {
            return Ok(keys);
        }
        let mut walk = FoldedInto::new(survivor.as_str().to_string());
        while let Some(key) = walk.pending() {
            let rows: Vec<String> = sqlx::query_scalar(
                "SELECT folded FROM merge_event WHERE ordinal = 0 AND survivor = ?",
            )
            .bind(&key)
            .fetch_all(&mut **tx)
            .await
            .map_err(store)?;
            let mut folded = rows;
            for held in bare {
                if !folded.contains(held)
                    && self
                        .survivor_of(tx, &EntityId(held.clone()))
                        .await?
                        .is_some_and(|placed| placed.as_str() == key)
                {
                    folded.push(held.clone());
                }
            }
            walk.reach(folded);
        }
        let reached = match walk.finish() {
            Ok(reached) => reached,
            Err(looped) => return Err(Self::damage_of(tx, looped).await),
        };
        for key in reached {
            if !keys.iter().any(|held| held.as_str() == key) {
                keys.push(EntityId(key));
            }
        }
        Ok(keys)
    }

    /// **The thing a merge folded this one into**, by the key the merge events
    /// hold, or none when no merge folded it. One hop: the survivor of the
    /// latest fold, not the end of the chain.
    ///
    /// **A fold made before events were kept has no event, and the forwarding
    /// row says where it went.** The row keeps the handle its survivor wore at
    /// the fold in `merged_into`. When no event names the thing, that row is the
    /// answer: the handle is read as the key of the thing it meant, which
    /// [`DoltMemory::wearer_of_folded_handle`] decides. A handle nothing ever wore
    /// places nothing, and a handle the fold and the rename do not tell apart is
    /// logged and places nothing: the pointer stays where it is. Nothing is
    /// written, so a store the older build filled is read the same way on every
    /// start.
    pub(super) async fn survivor_of(
        &self,
        tx: &mut Transaction<'_, MySql>,
        folded: &EntityId,
    ) -> Result<Option<EntityId>, MemoryError> {
        let survivor: Option<String> = sqlx::query_scalar(
            "SELECT survivor FROM merge_event WHERE ordinal = 0 AND folded = ?
             ORDER BY merge_no DESC LIMIT 1",
        )
        .bind(folded.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        if let Some(survivor) = survivor {
            return Ok(Some(EntityId(survivor)));
        }
        let folded_row: Option<(String, String)> = sqlx::query_as(
            "SELECT id, merged_into FROM entity
             WHERE merged_into IS NOT NULL AND (badge = ? OR (badge IS NULL AND id = ?))
             LIMIT 1",
        )
        .bind(folded.as_str())
        .bind(folded.as_str())
        .fetch_optional(&mut **tx)
        .await
        .map_err(store)?;
        let Some((handle, forwards_to)) = folded_row else {
            return Ok(None);
        };
        match Self::wearer_of_folded_handle(tx, &handle, folded.as_str(), &forwards_to).await? {
            // A row that names itself places nothing: the walk would stand still.
            Wearer::Placed(key) if key != folded.as_str() => Ok(Some(EntityId(key))),
            Wearer::Placed(_) | Wearer::NeverWorn => Ok(None),
            Wearer::Undecidable(why) => {
                self.warn_once(&handle, || {
                    tracing::warn!(
                        handle,
                        "store: {handle} forwards to {forwards_to}: {why}; a pointer at it \
                         stays where it is"
                    )
                });
                Ok(None)
            }
        }
    }

    /// **A merge loop as damage a person can find.** The walk names the things in
    /// the loop by the keys this store files them under, which no caller is told;
    /// each is named here by the handle it answers to, and a claim address by its
    /// handle and local id. The answer is [`MemoryError::StoredDamage`], which a
    /// caller is told only a person can repair.
    pub(super) async fn damage_of(
        tx: &mut Transaction<'_, MySql>,
        looped: MergeLoop,
    ) -> MemoryError {
        let mut named = Vec::with_capacity(looped.things.len());
        for thing in &looped.things {
            let (key, local) = match thing.split_once('#') {
                Some((key, local)) => (key, Some(local)),
                None => (thing.as_str(), None),
            };
            let handle: Option<String> =
                sqlx::query_scalar("SELECT id FROM entity WHERE badge = ? ORDER BY id LIMIT 1")
                    .bind(key)
                    .fetch_optional(&mut **tx)
                    .await
                    .ok()
                    .flatten();
            let handle = handle.unwrap_or_else(|| key.to_string());
            named.push(match local {
                Some(local) => format!("{handle}#{local}"),
                None => handle,
            });
        }
        tracing::error!(
            things = ?named,
            "stored damage: the merge events lead back to where they began"
        );
        MergeLoop { things: named }.into()
    }
}
