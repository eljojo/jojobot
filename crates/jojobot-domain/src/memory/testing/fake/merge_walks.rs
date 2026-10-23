//! **How the fake follows its own merges**: where a claim address went, where a
//! thing ended up and the names folded into an entity.

use super::*;

/// **The events that moved one claim address**, as the walk asks for them: the
/// position of a merge among all merges is its order, so the newest wins.
pub(super) fn claim_moves(
    merges: &[StoredMerge],
) -> impl Fn(&EntityId, &FactId) -> Result<Vec<ClaimMove<EntityId>>, MergeLoop> + '_ {
    |home, local| {
        Ok(merges
            .iter()
            .enumerate()
            .filter(|(_, merge)| &merge.folded == home)
            .filter_map(|(order, merge)| {
                merge
                    .claims
                    .iter()
                    .find(|(was, _)| was == local)
                    .map(|(_, now)| ClaimMove {
                        order: order as u64,
                        survivor: merge.survivor.clone(),
                        survivor_id: now.clone(),
                    })
            })
            .collect())
    }
}

/// **An entity as a receipt serves it**: with the names of what was folded into
/// it, as a listing of the roster serves them. The stored row carries none, so a
/// receipt built from the row alone would disagree with the listing.
pub(super) fn naming_folded_in(rows: &[Entity], entity: &mut Entity) {
    let mut listed = rows.to_vec();
    crate::memory::with_folded_names(&mut listed)
        .expect("the fake merges nothing into itself, so its merges never loop");
    if let Some(row) = listed.into_iter().find(|row| row.id == entity.id) {
        entity.folded_names = row.folded_names;
    }
}

/// **Where the merges sent a thing**, as the thing walk asks: the survivor of the
/// newest merge that folded it, `None` when nothing did.
pub(super) fn survivor_in(
    merges: &[StoredMerge],
) -> impl Fn(&EntityId) -> Result<Option<EntityId>, MergeLoop> + '_ {
    |folded| {
        Ok(merges
            .iter()
            .rev()
            .find(|merge| merge.folded == *folded)
            .map(|merge| merge.survivor.clone()))
    }
}

/// **Where a thing ended up after every merge**, through a survivor folded in its
/// turn.
pub(super) fn ended_at(merges: &[StoredMerge], thing: EntityId) -> EntityId {
    follow_thing(thing, survivor_in(merges))
        .expect("the fake merges nothing into itself, so its merges never loop")
}
