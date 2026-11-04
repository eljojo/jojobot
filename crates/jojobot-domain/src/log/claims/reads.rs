//! **What the fold re-reads after every write, answered from the State.** Each
//! read is one function of the writes the State already holds for one thing, so
//! it never scans the claims of any other thing. A read here is the same fold
//! the store's read runs ([`memory::folded_backing`], [`memory::folded_ages`]):
//! the value and what stands behind it cannot come from two passes that disagree.

use std::collections::BTreeMap;

use jiff::Timestamp;

use super::State;
use crate::log::ObjectId;
use crate::memory::{self, FieldBacking, types::DeclaredType};

impl State {
    /// **Who backs the value each key of a thing holds**, as the store's
    /// `backing` answers it. A key whose writes are summed, and a key a later
    /// write cleared, are absent.
    pub fn backing(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, FieldBacking> {
        memory::folded_backing(&self.key_writes(subject), declared)
    }

    /// **When the store took in the write that holds each key's value now**, the
    /// `keys` of the store's `field_ages`. A key whose holding write carries no
    /// moment is absent.
    pub fn held_since(
        &self,
        subject: &ObjectId,
        declared: &[DeclaredType],
    ) -> BTreeMap<String, Timestamp> {
        memory::folded_ages(&self.key_writes(subject), declared)
    }
}

#[cfg(test)]
mod tests;
