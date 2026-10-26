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
//!
//! Field histories use the same interleave on every store: each side keeps its
//! own ordinal order, and the survivor wins when write moments are equal.

use std::collections::BTreeMap;

use async_trait::async_trait;

use super::capacity;
use super::{
    Entity, EntityId, Fact, FactId, KeyWrite, Lineage, MemoryError, THOUGHT_BODY_CAP,
    THOUGHT_CAPACITY, needs_the_chart, numbered, refuses_merge_below_the_duplicate,
    refuses_merge_carrying, refuses_merge_into_room, thought_room,
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
    /// The chart around `subject`, the duplicate as its handle answers now, as a
    /// write that names no manager reads it: what a clear of a key on it meets.
    async fn lineage_of(&mut self, subject: &EntityId) -> Result<Lineage, MemoryError>;
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
    let (lineage, duplicate_lineage) = match needs_the_chart(&carried) {
        Some(named) => (
            Some(reads.lineage(named).await?),
            Some(reads.lineage_of(fold.folded).await?),
        ),
        None => (None, None),
    };
    if let Some(err) = refuses_merge_carrying(
        fold.caller,
        fold.survivor,
        fold.folded,
        &carried,
        lineage.as_ref(),
        duplicate_lineage.as_ref(),
    ) {
        return Err(err);
    }
    let above = reads.chain_above_survivor().await?;
    if let Some(err) = refuses_merge_below_the_duplicate(fold.survivor, fold.folded, &above) {
        return Err(err);
    }
    let incoming = reads.incoming().await?;
    let arriving = capacity::live_records(&incoming).len();
    if arriving == 0 && thought_room(&incoming).is_empty() {
        return Ok(());
    }
    let room = reads.survivors_room().await?;
    let number = |key: &str| {
        room.held
            .get(key)
            .and_then(|v| v.trim().parse::<usize>().ok())
    };
    if let Some(err) = refuses_merge_into_room(
        fold.survivor_named,
        &incoming,
        thought_room(&room.claims),
        number(THOUGHT_CAPACITY),
        number(THOUGHT_BODY_CAP),
    ) {
        return Err(err);
    }
    // **The records the duplicate brings face the survivor's record ceiling**,
    // asked even when the duplicate brings no thought.
    // **Against the ceiling the merged thing will hold**: it folds to the
    // survivor's number or the duplicate's, so the lower of the two counts.
    let held = capacity::held_after_a_merge(fold.survivor.kind(), &room.held, &carried);
    if arriving > 0
        && let capacity::Admission::Refused(err) = capacity::admit(
            capacity::Container {
                handle: fold.survivor_named,
                kind: fold.survivor.kind(),
                held: &held,
            },
            &capacity::live_records(&room.claims),
            arriving,
            false,
        )
    {
        return Err(err);
    }
    Ok(())
}

/// **One merge, as the merge events keep it.**
pub struct MergeStep {
    pub merge_no: i64,
    pub folded: String,
    pub survivor: String,
    /// For each claim the merge gave a new id: the id it had, to the id it was given.
    pub claims: std::collections::HashMap<String, String>,
}

/// **A field write a read can place among another thing's writes.**
pub trait ServedWrite: Clone {
    /// The key, the write's place in that key's order, the moment the store took
    /// it in and the id of the claim that carried it.
    fn at(&self) -> (&str, i64, Option<jiff::Timestamp>, &str);
    /// Give the write its place in the key's order and, when it came from a
    /// folded thing, the id its claim wears on the survivor.
    fn place(&mut self, ordinal: i64, fact: Option<&str>);
}

impl ServedWrite for KeyWrite {
    fn at(&self) -> (&str, i64, Option<jiff::Timestamp>, &str) {
        let fact = self.fact.as_str();
        (&self.key, self.ordinal as i64, self.written_at, fact)
    }
    fn place(&mut self, ordinal: i64, fact: Option<&str>) {
        self.ordinal = ordinal as u64;
        if let Some(fact) = fact {
            self.fact = FactId(fact.to_string());
        }
    }
}

/// **Place writes by the merge ledger once**, for one survivor or a grouped read.
/// `held` has the stored writes of the survivor and of every key folded into it,
/// directly or through another; the survivor's entry comes out holding the writes
/// after each merge into it, in the order the merges were made.
pub fn apply_merges<T: ServedWrite>(
    held: &mut std::collections::HashMap<String, Vec<T>>,
    steps: &[MergeStep],
) {
    for step in steps {
        let theirs = held.remove(&step.folded).unwrap_or_default();
        let ours = held.remove(&step.survivor).unwrap_or_default();
        held.insert(step.survivor.clone(), merge_in(ours, theirs, &step.claims));
    }
}

/// **A merge's writes read as the merge puts them**: the folded thing's writes
/// of each key interleaved with the survivor's by the moment each was made
/// ([`interleave`], the one rule the merge moves rows by), numbered from one,
/// each carrying the id its claim wears on the survivor. A write of a claim the
/// merge did not map is left out, as the claim is.
pub fn merge_in<T: ServedWrite>(
    ours: Vec<T>,
    theirs: Vec<T>,
    claims: &std::collections::HashMap<String, String>,
) -> Vec<T> {
    let mut by_key: std::collections::BTreeMap<String, (Vec<T>, Vec<T>)> = Default::default();
    let theirs = theirs
        .into_iter()
        .filter(|write| claims.contains_key(write.at().3))
        .map(|write| (true, write));
    for (folded, write) in theirs.chain(ours.into_iter().map(|write| (false, write))) {
        let sides = by_key.entry(write.at().0.to_string()).or_default();
        match folded {
            true => sides.0.push(write),
            false => sides.1.push(write),
        }
    }
    let mut merged = Vec::new();
    for (mut theirs, mut ours) in by_key.into_values() {
        theirs.sort_by_key(|write| write.at().1);
        ours.sort_by_key(|write| write.at().1);
        if theirs.is_empty() {
            merged.extend(ours);
            continue;
        }
        for (place, (side, mut write)) in interleave(theirs, ours).into_iter().enumerate() {
            let fact = (side == Side::Folded)
                .then(|| claims.get(write.at().3).cloned())
                .flatten();
            write.place(place as i64 + 1, fact.as_deref());
            merged.push(write);
        }
    }
    merged
}

/// Which thing a write came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Folded,
    Survivor,
}

/// **Two histories of one key, interleaved oldest first.**
///
/// Each side is already in the order it was written, and that order is kept:
/// the two are merged, not sorted, so a clock that stepped back inside one
/// thing's history cannot reorder it. A write with no stamp is older than every
/// stamped one. **On a tie the survivor's write comes after the duplicate's,
/// two unstamped writes included**, so the survivor wins: it is the thing kept.
pub fn interleave<T: ServedWrite>(folded: Vec<T>, survivor: Vec<T>) -> Vec<(Side, T)> {
    let (mut theirs, mut ours) = (
        folded.into_iter().peekable(),
        survivor.into_iter().peekable(),
    );
    let mut merged = Vec::new();
    loop {
        let side = match (theirs.peek(), ours.peek()) {
            (Some(their), Some(our)) if their.at().2 <= our.at().2 => Side::Folded,
            (Some(_), Some(_)) | (None, Some(_)) => Side::Survivor,
            (Some(_), None) => Side::Folded,
            (None, None) => return merged,
        };
        let next = match side {
            Side::Folded => theirs.next(),
            Side::Survivor => ours.next(),
        };
        merged.push((side, next.expect("peeked")));
    }
}
