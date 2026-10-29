//! Concrete answers; their execution stays in the shared read owner.

use crate::TickRead;
use jojobot_domain::attention;
use jojobot_domain::memory::readiness::Blocker;
use jojobot_domain::memory::{EntityId, MemoryError, graph};
use std::collections::BTreeMap;

pub type Backing = BTreeMap<String, jojobot_domain::memory::FieldBacking>;

/// **The verdict on one piece of work.**
pub struct Verdict {
    /// Whether the work can be started: the readiness tree's answer, and
    /// nothing else decides it.
    pub ready: bool,
    /// What stops it, to explain the verdict. Empty when it is ready.
    pub blockers: Vec<Blocker>,
}

/// **What the narrowings found**, aligned with the objects they kept.
pub struct Narrowed {
    /// Present when the question asked what is owed.
    pub owed: Option<OwedPass>,
    /// Present when the question asked what is ready.
    pub ready: Option<ReadyPass>,
}

/// **Where a question failed**, because a caller says different things about a
/// selection it could not read and a dependency it could not read.
pub enum Failed {
    /// The walk over the records the question selects.
    Selection(MemoryError),
    /// The read of the work a ready question depends on.
    Readiness(MemoryError),
}

/// **A question, answered**: the selection and then its narrowings.
pub struct Answered {
    /// The backing requested for each returned object.
    pub backing: BTreeMap<EntityId, Result<Backing, MemoryError>>,
    /// Bots whose stored focus is no longer a live declaration.
    pub stale_working_on: std::collections::BTreeSet<EntityId>,
    /// The objects the question found, after the narrowings.
    pub selected: graph::Selected,
    /// What the narrowings found.
    pub narrowed: Narrowed,
    /// **What the ticks of each object come to**, one entry to an object and in
    /// its order: the read a `recall` gives a list, for an object that is a list
    /// and nothing for any other. The same function builds both
    /// ([`TickRead::of`], rule 498).
    pub ticks: Vec<TickCell>,
}

/// **What the tick read of one object came to.** A list whose records could not
/// be read is its own case, never the same as a list nobody ticked (rule 130).
pub enum TickCell {
    /// The object is not a list.
    NotAList,
    /// The records of the list were read.
    Read(TickRead),
    /// The object is a list and its records could not be read.
    Unreadable,
}

/// **What the overdue pass found**, keyed by the objects it kept: a later
/// narrowing or window reorders or drops objects, and a value kept by position
/// would then belong to another object.
pub struct OwedPass {
    /// How many objects the pass dropped.
    pub excluded: usize,
    /// **Why the dropped objects were dropped, and of what kind.** The counts
    /// add up to [`OwedPass::excluded`]: this groups the same objects and
    /// counts none of them differently.
    pub excluded_because: BTreeMap<LeftOut, BTreeMap<String, usize>>,
    /// How many days past the day it is measured from each kept object fell
    /// due (below zero: how many days ahead), read off the same `Due` the order
    /// came from, so the number cannot disagree with the position it is in.
    pub by_days: BTreeMap<EntityId, Option<i64>>,
    /// Why each kept object that cannot say when it falls due cannot.
    pub unreadable_because: BTreeMap<EntityId, attention::Unreadable>,
}

/// **Why the owed pass left an object out.** An object is in exactly one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LeftOut {
    /// It has a due day and the day is after the day asked about.
    NotDueYet,
    /// It has a cadence and has never been checked in, so its first cycle has
    /// not started.
    NotYetOpened,
    /// A work item or project that is finished, or a thing its carrier reads as
    /// ended, owes nothing.
    Finished,
    /// Its carrier answers that nothing falls due: it carries none of the keys a
    /// due day is worked out from, so no carrier speaks for it.
    NoSchedule,
}

impl LeftOut {
    /// Every reason there is.
    pub const ALL: [LeftOut; 4] = [
        LeftOut::NotDueYet,
        LeftOut::NotYetOpened,
        LeftOut::Finished,
        LeftOut::NoSchedule,
    ];

    /// The word a served answer uses for this reason.
    pub fn token(self) -> &'static str {
        match self {
            LeftOut::NotDueYet => "not_due_yet",
            LeftOut::NotYetOpened => "not_yet_opened",
            LeftOut::Finished => "finished",
            LeftOut::NoSchedule => "no_schedule",
        }
    }
}

/// **What the readiness pass found**, aligned with the objects it kept.
pub struct ReadyPass {
    /// The verdict on each kept object.
    pub verdicts: Vec<Verdict>,
    /// How many pieces of work the pass left out.
    pub left_out: usize,
}
