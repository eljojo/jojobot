//! **The kinds a store declares, held in memory** and asked at every admission.
//!
//! A read of the store fills the set. A write of a kind ends a generation, and a
//! fill that began in an earlier generation is dropped instead of kept, so a
//! fill that read the rows before a declaration cannot publish them after it.

use std::sync::RwLock;

use jojobot_domain::memory::kinds::KindSet;

#[derive(Default)]
pub(super) struct HeldKinds(RwLock<State>);

#[derive(Default)]
struct State {
    generation: u64,
    set: Option<KindSet>,
}

impl HeldKinds {
    /// The held set, or the generation a read of the store must publish under.
    pub(super) fn get(&self) -> Result<KindSet, u64> {
        let state = self
            .0
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.set.clone().ok_or(state.generation)
    }

    /// Keeps `set` only when no kind write has ended since `generation` was
    /// handed out.
    pub(super) fn publish(&self, generation: u64, set: &KindSet) {
        let mut state = self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if state.generation == generation {
            state.set = Some(set.clone());
        }
    }

    /// A kind write has ended: the next admission reads the store again.
    pub(super) fn forget(&self) {
        let mut state = self
            .0
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state.generation += 1;
        state.set = None;
    }
}

/// Ends a generation when a kind write ends, by any path.
pub(super) struct ForgetKinds<'a>(pub(super) &'a HeldKinds);

impl Drop for ForgetKinds<'_> {
    fn drop(&mut self) {
        self.0.forget();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jojobot_domain::memory::types::Origin;

    fn of(tokens: &[&str]) -> KindSet {
        KindSet::new(
            tokens
                .iter()
                .map(|token| (token.to_string(), Origin::Declared))
                .collect(),
        )
    }

    /// **A fill that read the rows before a declaration cannot keep them after
    /// it.** The reader takes its generation, the declaration ends, and only then
    /// does the reader publish what it read: the set stays empty, so the next
    /// admission reads the store again. **Paired**: a fill with no write between
    /// its start and its publish is kept.
    #[test]
    fn a_fill_that_began_before_a_kind_write_is_not_kept() {
        let held = HeldKinds::default();
        let Err(generation) = held.get() else {
            panic!("a new store holds no kinds yet");
        };
        drop(ForgetKinds(&held));
        held.publish(generation, &of(&["sloth"]));
        assert!(held.get().is_err(), "the pre-declaration rows were kept");

        let Err(generation) = held.get() else {
            panic!("the stale fill was dropped");
        };
        held.publish(generation, &of(&["sloth", "walrus"]));
        assert!(held.get().is_ok(), "a fill with no write after it is kept");
    }
}
