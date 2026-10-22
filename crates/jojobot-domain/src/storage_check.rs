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
    /// A thing marked merged into another, with no merge event naming it.
    /// Merges made before the event existed are like this and are tolerated.
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
    /// A request that wrote some of its parts and never recorded its receipt.
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
        StorageClass::HalfDoneWrite,
        StorageClass::LostReceipt,
    ];

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
            StorageClass::HalfDoneWrite => "D1 half-done write",
            StorageClass::LostReceipt => "D3 lost receipt",
        }
    }

    /// What the finding means and what, if anything, a person does about it.
    pub fn meaning(self) -> &'static str {
        match self {
            StorageClass::MergedWithoutEvent => {
                "a thing is marked merged and no merge event names it; merges made before the \
                 event existed are like this, so a non-zero count is expected"
            }
            StorageClass::MergeEventNotApplied => {
                "a merge event names a folded thing that is not marked merged"
            }
            StorageClass::MergedIntoNothing => {
                "a merged thing points at nothing that exists, or at a loop"
            }
            StorageClass::ClaimUnderFoldedKeyUnmapped => {
                "a claim is stored under a merged thing and no merge event says where it went"
            }
            StorageClass::DanglingReference => {
                "a row names a thing, mailbox or message that does not exist; each id says \
                 the column and the value"
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
    pub count: u64,
    /// Up to [`IDS_SHOWN`] ids or addresses, each naming one damaged row.
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
