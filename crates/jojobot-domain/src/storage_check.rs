//! **The boot's read-only check of the store for damage a cut-off write leaves.**
//!
//! A write that was cut, refused half way or lost its receipt can leave rows
//! that disagree with each other. The schema has no foreign keys by design, so
//! the store itself never says so. At every boot, after migrations and before
//! the fold is filled, one pass counts each class of damage and the boot logs
//! what it found. The pass writes nothing and repairs nothing.
//!
//! **The classes are one list** ([`StorageClass::ALL`]). A class added to it is
//! checked by every store that implements [`StorageCheck`] and reported by the
//! boot, with no other change.

/// One kind of damage the pass looks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum StorageClass {
    /// A thing marked merged into another, with no merge event naming it. The
    /// startup repair writes the event of a merge made before events existed, so
    /// a store that still holds one has a survivor that no row answers for.
    MergedWithoutEvent,
    /// A merge event whose folded thing is not marked merged.
    MergeEventNotApplied,
    /// A merged thing that points at nothing, or at a loop.
    MergedIntoNothing,
    /// A claim stored under a folded thing's key that no merge event lists, so a
    /// read cannot say where the claim went.
    ClaimUnderFoldedKeyUnmapped,
    /// A row that names a thing, a mailbox or a message that does not exist. The
    /// schema has no foreign keys, so nothing else says so.
    DanglingReference,
    /// A source address whose claim-history merge events lead back to an
    /// address the walk already passed.
    MergeEventLoop,
    /// A claim or field write homed under a handle that no row wears and no build
    /// supplies. The handle may be one a build once supplied, which the boot
    /// gives a row, or a row removed by hand, so it is reported and not called
    /// damage.
    HandleHomedWrite,
    /// A merge's request receipt whose `landed` record names a merge that the
    /// store does not hold: the folded thing is not merged, or not into the
    /// survivor. Only an edit outside jojobot or a restore makes this.
    LandedMergeMissing,
    /// A request, inside the window the ledger keeps ids for, that wrote some of
    /// its parts and never recorded its receipt.
    HalfDoneWrite,
    /// A request receipt with no receipt body, older than ten minutes.
    LostReceipt,
}

impl StorageClass {
    /// Every class the pass checks, in the order the boot reports them.
    pub const ALL: &'static [StorageClass] = &[
        StorageClass::MergedWithoutEvent,
        StorageClass::MergeEventNotApplied,
        StorageClass::MergedIntoNothing,
        StorageClass::ClaimUnderFoldedKeyUnmapped,
        StorageClass::DanglingReference,
        // The source scan must finish before its loop findings are reported.
        StorageClass::MergeEventLoop,
        StorageClass::HandleHomedWrite,
        StorageClass::LandedMergeMissing,
        StorageClass::HalfDoneWrite,
        StorageClass::LostReceipt,
    ];

    /// **Whether the boot reports this class at info level and does not call it
    /// damage.** A write homed under a handle with no row is left alone on
    /// purpose, so a store holds one without anything being wrong.
    pub fn reported_at_info(self) -> bool {
        matches!(self, StorageClass::HandleHomedWrite)
    }

    /// The short name a log line carries.
    pub fn token(self) -> &'static str {
        match self {
            StorageClass::MergedWithoutEvent => "A1 merged without a merge event",
            StorageClass::MergeEventNotApplied => "A2 merge event not applied",
            StorageClass::MergedIntoNothing => "A3 merged into nothing or a loop",
            StorageClass::ClaimUnderFoldedKeyUnmapped => {
                "A4 claim under a folded key no merge event lists"
            }
            StorageClass::DanglingReference => "B dangling reference",
            StorageClass::MergeEventLoop => "A5 merge-event loop",
            StorageClass::HandleHomedWrite => "B write homed under a handle with no row",
            StorageClass::LandedMergeMissing => "D2 landed merge not in the store",
            StorageClass::HalfDoneWrite => "D1 half-done write",
            StorageClass::LostReceipt => "D3 lost receipt",
        }
    }

    /// What the finding means and what, if anything, a person does about it.
    pub fn meaning(self) -> &'static str {
        match self {
            StorageClass::MergedWithoutEvent => {
                "a thing is marked merged and no merge event names it; the startup repair \
                 writes the event of an old merge once, and it could not write this one \
                 because the thing's survivor has no row. A restart does not run the repair \
                 again, so a person restores a copy of the store taken before that merge, or \
                 lets the thing's history read as unmerged"
            }
            StorageClass::MergeEventNotApplied => {
                "a merge event names a folded thing that is not marked merged, or that no row \
                 holds, and each id says which. Where a row is there, a person marks it merged \
                 into the survivor the event names and restarts, so the memory copy is read \
                 again. Where no row is there nothing can be marked, and a person restores a \
                 copy of the store taken before that merge"
            }
            StorageClass::MergedIntoNothing => {
                "a merged thing points at nothing that exists, or at a loop, so a read cannot \
                 name its survivor; a person points it at the thing it was folded into, or \
                 restores a copy of the store taken before that merge, and then restarts"
            }
            StorageClass::ClaimUnderFoldedKeyUnmapped => {
                "a claim is stored under a merged thing and no merge event says where it went, \
                 so a read of the survivor leaves it out and names it under claims_left_out; a \
                 person lists the claim in the merge's event or moves it to the survivor, and \
                 then restarts"
            }
            StorageClass::DanglingReference => {
                "a row names a thing, mailbox or message that does not exist; each id says \
                 the column and the value; a person restores the missing thing from a copy of \
                 the store, or clears or deletes the row that names it, and then restarts. A \
                 bare line name the startup repair left is corrected with update_fact on the \
                 claim its log line names, which makes reads serve the right handle and ends \
                 this report for that row; the old stored write stays as history. A new claim \
                 that restates the value does not end it, because the old claim's write is \
                 still its own current one"
            }
            StorageClass::MergeEventLoop => {
                "a claim's source address leads through merge events that form a loop, so its \
                 history cannot be read; each id names the source column and address; a person \
                 corrects the merge-event mapping or restores a copy of the store taken before \
                 that merge, and then restarts"
            }
            StorageClass::HandleHomedWrite => {
                "a claim or field write is homed under a handle that no row wears; the boot \
                 gives a row to a handle a build once supplied and leaves the others alone"
            }
            StorageClass::LandedMergeMissing => {
                "a merge receipt says a merge landed and the store does not hold it; only an \
                 edit outside jojobot or a restore does this; a person restores the copy of the \
                 store that held the merge, or sends the merge again with merge_entities under \
                 a new request_id, because the same id answers with its first receipt and \
                 writes nothing"
            }
            StorageClass::HalfDoneWrite => {
                "a request wrote some parts and has no receipt; only the same caller sending \
                 the same call again can finish it"
            }
            StorageClass::LostReceipt => {
                "a request's receipt was never stored; a resend answers without its first receipt"
            }
        }
    }
}

/// How many ids one finding carries. The count is always whole.
pub const IDS_SHOWN: usize = 20;

/// What the pass found for one class. A class with `count` zero was checked and
/// found nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub class: StorageClass,
    /// **How many rows are damaged.** For a dangling reference this counts rows,
    /// so fifty rows that hold one ghost value count fifty and name one id.
    pub count: u64,
    /// Up to [`IDS_SHOWN`] ids or addresses. Each names one damaged row, except
    /// for a dangling reference, where an id names one value and the rows that
    /// hold it are counted in `count`.
    pub ids: Vec<String>,
}

/// A store that cannot be read for the check. The boot warns and goes on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StorageCheckError(pub String);

impl std::fmt::Display for StorageCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for StorageCheckError {}

/// **The one port method.** A finding for every class in [`StorageClass::ALL`],
/// in that order, whether or not it found anything.
#[async_trait::async_trait]
pub trait StorageCheck: Send + Sync {
    async fn check_storage(&self) -> Result<Vec<Finding>, StorageCheckError>;
}

/// **A store that holds the damage it is given**, for the cases that seed one
/// instance of each class.
#[derive(Debug, Clone, Default)]
pub struct SeededStorage {
    seeded: Vec<(StorageClass, String)>,
}

impl SeededStorage {
    /// One damaged row of `class`, named by `id`.
    pub fn with(mut self, class: StorageClass, id: &str) -> Self {
        self.seeded.push((class, id.to_string()));
        self
    }
}

#[async_trait::async_trait]
impl StorageCheck for SeededStorage {
    async fn check_storage(&self) -> Result<Vec<Finding>, StorageCheckError> {
        Ok(StorageClass::ALL
            .iter()
            .map(|class| {
                let ids: Vec<&String> = self
                    .seeded
                    .iter()
                    .filter(|(held, _)| held == class)
                    .map(|(_, id)| id)
                    .collect();
                Finding {
                    class: *class,
                    count: ids.len() as u64,
                    ids: ids.into_iter().take(IDS_SHOWN).cloned().collect(),
                }
            })
            .collect())
    }
}
