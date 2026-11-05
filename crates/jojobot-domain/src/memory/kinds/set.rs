//! **The kinds one store declares**, held by that store and asked of it.

use super::{NotAKind, standing_in};
use crate::memory::types::Origin;

/// What a store declares, as the store reports it. Two stores in one process
/// hold two of these, and each answers for itself.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KindSet(Vec<(String, Origin)>);

impl KindSet {
    pub fn new(held: Vec<(String, Origin)>) -> Self {
        KindSet(held)
    }

    /// Accepts a declared token; an empty set is a store that was never
    /// seeded, which is not the refusal a typo gets.
    pub fn standing(&self, token: &str) -> Result<(), NotAKind> {
        standing_in(&self.0, token)
    }
}
