//! Where a thing sits in the tree, asked of the graph.

use crate::{Failed, QueryHandle, Question};
use jojobot_domain::actor::Actor;
use jojobot_domain::memory::{EntityId, MemoryError};
use std::collections::HashSet;

/// **The ancestry of a thing, root first, ending in the thing.** Each parent is
/// one subject question, asked of the handle the child names, so a parent filed
/// under a name the thing used to wear is reached as what it is called now.
///
/// **The seen-set is the whole bound**: every step adds a handle the chain does
/// not hold, so a loop in the records ends the walk after the things there are.
/// A parent nothing answers to ends the chain at the child, which is the root of
/// what can be reached. A failure to read is the caller's, never a shorter path.
pub async fn ancestry(
    queries: &QueryHandle,
    asker: &Actor,
    thing: &EntityId,
    parent: Option<&EntityId>,
) -> Result<Vec<EntityId>, Failed> {
    let mut chain = vec![thing.clone()];
    let mut seen: HashSet<EntityId> = HashSet::from([thing.clone()]);
    let mut next = parent.cloned();
    while let Some(named) = next {
        let answer = match queries
            .ask(asker, Question::about(named.clone(), None, None))
            .await
        {
            Ok(answer) => answer,
            Err(Failed::Selection(MemoryError::UnknownEntity { .. })) => break,
            Err(failed) => return Err(failed),
        };
        let Some(object) = answer.selected.subject_object(&named) else {
            break;
        };
        if !seen.insert(object.entity.id.clone()) {
            break;
        }
        chain.push(object.entity.id.clone());
        next = object.entity.parent;
    }
    chain.reverse();
    Ok(chain)
}
