//! **The names a refused name resembles**, read from the State the refusal was
//! decided on. A refusal for a name nothing holds carries them, so the caller
//! is served the candidates of the very State that refused it and not of one
//! that has moved since.

use super::super::State;
use crate::memory::EntityId;
use crate::memory::guard::{self, Decision, EntityMatch};

/// What the guard answers for `name` over the State: the near names, empty when
/// nothing resembles it. A name the State resolves has none.
pub(super) fn of(state: &State, name: &str) -> Vec<EntityMatch> {
    match guard::decide_named(&EntityId(name.to_string()), None, &state.entities()) {
        Decision::Block(matches) => matches,
        Decision::Proceed => Vec::new(),
    }
}
