//! **Whether a piece of work can be started, and what stops it if it cannot.**
//!
//! Work is ready when it is not itself finished, every thing it `depends_on` is
//! finished, and nothing is held under `waiting_on`. The question is a filter tree ([`tree`]), so the
//! shipped behaviour runs on the same evaluator a bot's own view runs on.
//! [`blocked_by`] answers the other half, which a boolean cannot: which
//! dependency stops it, or what it waits on. It is computed for the work a
//! caller asked about and for nothing else.
//!
//! **One hop.** A dependency is finished or it is not. What stops the
//! dependency is its own record's business, so a cycle reads as blocked on both
//! sides and no walk can loop.

use super::filter::{Filter, Lookup, Quantifier, Subject};
use super::types::list_items;

/// The key a piece of work holds the things it needs finished first under.
pub const DEPENDS_ON: &str = "depends_on";

/// The key a piece of work holds what it is waiting on under.
pub const WAITING_ON: &str = "waiting_on";

/// **The ask a view names to get this question.**
pub const ASK: &str = "ready";

/// **The ask a view names to get a whole plan**: every piece of work that is not
/// finished, each with whether it is ready and, when it is not, what stops it.
pub const ASK_PLAN: &str = "plan";

/// **What a view asks about readiness.** Both asks run the same readiness
/// question on each piece of work; they differ in which pieces they keep.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Only the work that can be started now.
    Ready,
    /// All the work that is not finished, ready or not.
    Plan,
}

impl Ask {
    /// Every ask there is.
    pub const ALL: [Ask; 2] = [Ask::Ready, Ask::Plan];

    /// The ask a view's `asks` key names, when it names this question.
    pub fn named(word: &str) -> Option<Ask> {
        match word {
            ASK => Some(Ask::Ready),
            ASK_PLAN => Some(Ask::Plan),
            _ => None,
        }
    }

    /// **Whether work with this verdict stays in the answer.** A ready ask keeps
    /// what is ready. A plan keeps everything the work itself has not ended: a
    /// piece of work that is finished or archived is not part of the plan.
    pub fn keeps(self, ready: bool, blockers: &[Blocker]) -> bool {
        match self {
            Ask::Ready => ready,
            Ask::Plan => !blockers
                .iter()
                .any(|blocker| matches!(blocker, Blocker::Finished | Blocker::Archived)),
        }
    }
}

/// **The tree for "can be started".**
pub fn tree() -> Filter {
    Filter::All(vec![
        Filter::negated(Filter::Finished),
        Filter::Refs {
            key: DEPENDS_ON.into(),
            quantifier: Quantifier::All,
            then: Box::new(Filter::Finished),
        },
        Filter::negated(Filter::exists(WAITING_ON)),
    ])
}

/// **Why one dependency does not let the work start.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Why {
    /// The dependency exists and is not finished.
    Unfinished,
    /// The dependency is archived. An archived thing is not finished.
    Archived,
    /// The handle names nothing.
    Dangling,
}

/// **One thing that stops the work.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Blocker {
    /// The work is itself finished, so there is nothing to start.
    Finished,
    /// The work is itself archived. Archived work is not started, however it is
    /// named.
    Archived,
    /// A dependency, by the handle the work holds, and why it blocks.
    Dependency { handle: String, why: Why },
    /// What the work waits on, as it is held.
    WaitingOn(String),
}

/// **Everything that stops this work, in the order the work names it**: that
/// it is finished, then the dependencies, then what it waits on. Empty exactly
/// when [`tree`] holds.
pub fn blocked_by(subject: &Subject<'_>, lookup: &dyn Lookup) -> Vec<Blocker> {
    let fields = subject.fields;
    let mut blockers = Vec::new();
    if subject.archived {
        blockers.push(Blocker::Archived);
    }
    if Filter::Finished.holds_in(subject, lookup) {
        blockers.push(Blocker::Finished);
    }
    let held = fields.get(DEPENDS_ON).map_or("", String::as_str);
    for handle in list_items(held) {
        let finished = Filter::Finished;
        let why = match lookup.thing(handle) {
            None => Some(Why::Dangling),
            Some(named) if named.archived => Some(Why::Archived),
            Some(named) if !finished.holds_in(&named, lookup) => Some(Why::Unfinished),
            Some(_) => None,
        };
        if let Some(why) = why {
            blockers.push(Blocker::Dependency {
                handle: handle.to_string(),
                why,
            });
        }
    }
    if let Some(waiting) = fields.get(WAITING_ON) {
        blockers.push(Blocker::WaitingOn(waiting.clone()));
    }
    blockers
}

/// **Whether this work can be started**, asked of the tree. Archived work is
/// not ready: the tree's `Finished` leaf is false for it, so the tree alone would
/// read it as unfinished and startable.
pub fn is_ready(subject: &Subject<'_>, lookup: &dyn Lookup) -> bool {
    !subject.archived && tree().holds_in(subject, lookup)
}

#[cfg(test)]
mod tests;
