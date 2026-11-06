//! **The names a refused name resembles**, read from the State the refusal was
//! decided on. A refusal for a name nothing holds carries them, so the caller
//! is served the candidates of the very State that refused it and not of one
//! that has moved since.

use super::super::State;
use crate::memory::guard::{self, Decision, EntityMatch};
use crate::memory::{Archived, Boot, Entity, EntityId};

/// The things the State holds, as the guard reads them: every object that is
/// not folded into another, under the handle it answers to now.
fn index(state: &State) -> Vec<Entity> {
    state
        .objects
        .iter()
        .filter(|(object, _)| !state.redirects.contains_key(*object))
        .filter_map(|(object, body)| {
            let id = state.handle_of(object)?;
            let kind = id.kind()?;
            let folded_names = state
                .folded_under
                .get(object)
                .into_iter()
                .flatten()
                .filter_map(|folded| state.objects.get(folded))
                .map(|folded| folded.name.clone())
                .collect();
            Some(Entity {
                id,
                merged_into: None,
                kind,
                name: body.name.clone(),
                aliases: body.aliases.clone(),
                folded_names,
                source: body.source.clone(),
                crm: body.metadata.get("crm").cloned(),
                parent: body
                    .parent
                    .as_ref()
                    .and_then(|parent| state.handle_of(parent)),
                boot: Boot::from_token(body.boot.as_deref().unwrap_or_default()),
                badge: body.metadata.get("badge").cloned(),
                archived: body.archived.then(|| Archived {
                    reason: body
                        .metadata
                        .get("archived_reason")
                        .cloned()
                        .unwrap_or_default(),
                    at: body
                        .metadata
                        .get("archived_at")
                        .and_then(|at| at.parse().ok())
                        .unwrap_or(jiff::Timestamp::UNIX_EPOCH),
                }),
            })
        })
        .collect()
}

/// What the guard answers for `name` over the State: the near names, empty when
/// nothing resembles it. A name the State resolves has none.
pub(super) fn of(state: &State, name: &str) -> Vec<EntityMatch> {
    match guard::decide_named(&EntityId(name.to_string()), None, &index(state)) {
        Decision::Block(matches) => matches,
        Decision::Proceed => Vec::new(),
    }
}
