//! **Which refusal a fold earns before anything moves.**
//!
//! One function decides, for every store, how the two sides of a merge are
//! judged and in what order, so a test on the fake proves what production does.
//! A side is judged whole before the next: it must exist, then it must not be
//! a forwarding row. After both sides, the survivor must not sit below the thing
//! folded, because the folded thing's children are served under the survivor and
//! a survivor below it would be served as its own parent.
//!
//! The guards that ask about what the fold carries (`refuses_merge_carrying`,
//! `refuses_merge_below_the_duplicate`, `refuses_merge_into_room`) run in one
//! order in [`refuses_merge_guards`], over what a store reads through
//! [`MergeReads`]. A store keeps only the reads.

use std::collections::BTreeMap;

use async_trait::async_trait;

use super::{
    Entity, EntityId, Fact, Lineage, MemoryError, THOUGHT_BODY_CAP, THOUGHT_CAPACITY,
    needs_the_chart, numbered, refuses_merge_below_the_duplicate, refuses_merge_carrying,
    refuses_merge_into_room, thought_room,
};

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

/// **The refusal for a side no stored row answers to.** A handle the build
/// supplies is real and still not a row (rule 234): the handle reads, lists and
/// searches as existing, so a miss would tell the caller in the next breath that
/// it does not. The store says which it is, and finds the nearest handles for a
/// miss; the words are here.
pub fn missing_side(
    side: &EntityId,
    supplied: bool,
    nearest: Vec<super::guard::EntityMatch>,
) -> MemoryError {
    if supplied {
        MemoryError::SuppliedHandle {
            attempted: side.to_string(),
        }
    } else {
        MemoryError::UnknownEntity {
            attempted: side.to_string(),
            nearest,
        }
    }
}

/// The survivor's room, as it stands when the fold is judged.
pub struct SurvivorRoom {
    /// Every claim the survivor holds, served under its handle.
    pub claims: Vec<Fact>,
    /// What the survivor holds as fields, for the ceilings on its room.
    pub held: BTreeMap<String, String>,
}

/// **What a store reads to judge a fold, each read only when the order reaches
/// it.** A store answers from its own state, inside the transaction or under the
/// lock the fold is made under, so no write can land between a question and the
/// move.
#[async_trait]
pub trait MergeReads: Send {
    /// What the duplicate holds as fields, its manager served as a handle.
    async fn carried(&mut self) -> Result<BTreeMap<String, String>, MemoryError>;
    /// The chart around the survivor, with `named` the manager the carried keys
    /// name.
    async fn lineage(&mut self, named: Option<EntityId>) -> Result<Lineage, MemoryError>;
    /// The chain above the survivor.
    async fn chain_above_survivor(&mut self) -> Result<Vec<EntityId>, MemoryError>;
    /// Every claim the duplicate holds, served under its handle.
    async fn incoming(&mut self) -> Result<Vec<Fact>, MemoryError>;
    /// The survivor's room.
    async fn survivors_room(&mut self) -> Result<SurvivorRoom, MemoryError>;
}

/// The two sides of a fold and who folds, as they answer now.
pub struct Fold<'a> {
    /// The caller.
    pub caller: &'a EntityId,
    /// The duplicate, as its handle answers now.
    pub folded: &'a EntityId,
    /// The survivor, as its handle answers now.
    pub survivor: &'a EntityId,
    /// The survivor as the caller named it, which the room's refusals name.
    pub survivor_named: &'a EntityId,
}

/// **The guards of a fold, in their one order, each asked only after the one
/// before it let the fold through.** What the duplicate carries is weighed
/// against the caller; then the survivor must not sit above the duplicate in the
/// chart; then the duplicate's thoughts must fit the survivor's room. Returns the
/// refusal, or a read's failure, as an error.
pub async fn refuses_merge_guards<R: MergeReads + ?Sized>(
    fold: &Fold<'_>,
    reads: &mut R,
) -> Result<(), MemoryError> {
    let carried = reads.carried().await?;
    let lineage = match needs_the_chart(&carried) {
        Some(named) => Some(reads.lineage(named).await?),
        None => None,
    };
    if let Some(err) = refuses_merge_carrying(
        fold.caller,
        fold.survivor,
        fold.folded,
        &carried,
        lineage.as_ref(),
    ) {
        return Err(err);
    }
    let above = reads.chain_above_survivor().await?;
    if let Some(err) = refuses_merge_below_the_duplicate(fold.survivor, fold.folded, &above) {
        return Err(err);
    }
    let incoming = reads.incoming().await?;
    if thought_room(&incoming).is_empty() {
        return Ok(());
    }
    let room = reads.survivors_room().await?;
    let number = |key: &str| {
        room.held
            .get(key)
            .and_then(|v| v.trim().parse::<usize>().ok())
    };
    match refuses_merge_into_room(
        fold.survivor_named,
        &incoming,
        thought_room(&room.claims),
        number(THOUGHT_CAPACITY),
        number(THOUGHT_BODY_CAP),
    ) {
        Some(err) => Err(err),
        None => Ok(()),
    }
}
