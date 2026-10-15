//! **Which refusal a fold earns before anything moves.**
//!
//! One function decides, for every store, how the two sides of a merge are
//! judged and in what order, so a test on the fake proves what production does.
//! A side is judged whole before the next: it must exist, then it must not be
//! a forwarding row. After both sides, the survivor must not sit below the thing
//! folded, because the folded thing's children are served under the survivor and
//! a survivor below it would be served as its own parent.
//!
//! The other guards of a merge (`refuses_merge_carrying`,
//! `refuses_merge_below_the_duplicate`, `refuses_merge_carrying_by`,
//! `refuses_merge_into_room`) ask about what the fold carries and are called by
//! each store after this one.

use super::{Entity, EntityId, MemoryError, numbered};

/// What the sides of a fold earn, before anything else is asked.
#[derive(Debug)]
pub enum SideRefusal {
    /// A side no stored row answers to. The store words this itself: telling a
    /// build-supplied record from a miss, and finding the nearest handles, read
    /// the store.
    Missing(EntityId),
    /// Any other refusal, ready to return.
    Refused(MemoryError),
}

/// **The refusal the sides of a fold earn, or `None` when they may be folded.**
/// `rows` are the stored rows, and only rows: a fold marks a stored row, and a
/// build-supplied record has none to mark.
pub fn refuses_merge_sides(
    folded: &EntityId,
    survivor: &EntityId,
    rows: &[Entity],
) -> Option<SideRefusal> {
    for side in [folded, survivor] {
        let Some(held) = rows.iter().find(|row| &row.id == side) else {
            return Some(SideRefusal::Missing(side.clone()));
        };
        if let Some(into) = &held.merged_into {
            return Some(SideRefusal::Refused(MemoryError::AlreadyMerged {
                attempted: side.to_string(),
                into: into.to_string(),
            }));
        }
    }
    numbered::descendants(folded, rows)
        .iter()
        .any(|row| &row.id == survivor)
        .then(|| {
            SideRefusal::Refused(MemoryError::MergeIntoDescendant {
                folded: folded.to_string(),
                survivor: survivor.to_string(),
            })
        })
}
