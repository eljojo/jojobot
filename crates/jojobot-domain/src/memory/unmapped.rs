//! **A claim a merge left under the folded key and gave no id on the survivor.**
//!
//! A merge writes an event and moves no row, and the event holds the id each of
//! the folded thing's claims is served under on the survivor. A claim stored
//! under the folded key that the event does not list cannot be served there. It
//! is not refused and not dropped without a word: the survivor's answer names it
//! as left out, with the reason and the address that reads it.

use super::FactAddress;

/// One such claim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedClaim {
    /// Where the claim is stored, under the handle the folded thing answers to
    /// now. A read of this address returns the claim.
    pub address: FactAddress,
    /// The merge whose event does not list it.
    pub merge_no: u64,
}

impl UnmappedClaim {
    /// Why the survivor does not serve the claim.
    pub fn reason(&self) -> String {
        format!("unmapped by merge event {}", self.merge_no)
    }
}
