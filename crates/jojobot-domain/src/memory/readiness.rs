//! **Whether a piece of work can be started, and what stops it if it cannot.**
//!
//! Work is ready when every thing it `depends_on` is finished and nothing is
//! held under `waiting_on`. The question is a filter tree ([`tree`]), so the
//! shipped behaviour runs on the same evaluator a bot's own view runs on.
//! [`blocked_by`] answers the other half, which a boolean cannot: which
//! dependency stops it, or what it waits on. It is computed for the work a
//! caller asked about and for nothing else.
//!
//! **One hop.** A dependency is finished or it is not. What stops the
//! dependency is its own record's business, so a cycle reads as blocked on both
//! sides and no walk can loop.

use std::collections::BTreeMap;

use super::filter::{Filter, Lookup, Quantifier, Subject};
use super::types::list_items;

/// The key a piece of work holds the things it needs finished first under.
pub const DEPENDS_ON: &str = "depends_on";

/// The key a piece of work holds what it is waiting on under.
pub const WAITING_ON: &str = "waiting_on";

/// **The ask a view names to get this question.**
pub const ASK: &str = "ready";

/// **The tree for "can be started".**
pub fn tree() -> Filter {
    Filter::All(vec![
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
    /// A dependency, by the handle the work holds, and why it blocks.
    Dependency { handle: String, why: Why },
    /// What the work waits on, as it is held.
    WaitingOn(String),
}

/// **Everything that stops this work, in the order the work names it**:
/// dependencies first, then what it waits on. Empty exactly when [`tree`]
/// holds.
pub fn blocked_by(fields: &BTreeMap<String, String>, lookup: &dyn Lookup) -> Vec<Blocker> {
    let mut blockers = Vec::new();
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

/// **Whether this work can be started**, asked of the tree.
pub fn is_ready(fields: &BTreeMap<String, String>, lookup: &dyn Lookup) -> bool {
    let subject = Subject {
        kind: super::EntityKind::WORK.as_token(),
        fields,
        archived: false,
    };
    tree().holds_in(&subject, lookup)
}

#[cfg(test)]
mod tests;
