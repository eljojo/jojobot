//! **The refusals of an edit whose claim has moved on**, stated once for every
//! store: by the revision of the claim, and by a newer claim on a key it must back.

use super::*;

/// **The refusal of an edit made against a revision the claim has moved past**,
/// or nothing when the edit names no revision or names the one the claim is at.
///
/// `current` is the number of writes the claim holds, counted by the store in
/// the transaction that would append the next one. Both stores call this, so
/// the rule is stated once.
pub fn refuses_a_stale_revision(
    address: &FactAddress,
    expected: Option<usize>,
    current: usize,
) -> Option<MemoryError> {
    expected
        .filter(|expected| *expected != current)
        .map(|expected| MemoryError::StaleRevision {
            address: address.to_string(),
            expected,
            current,
        })
}

/// **The refusal of an edit whose claim a newer write has displaced as the
/// backing of a key it must back**, or nothing when the claim still backs every
/// one of [`FactPatch::must_back`]. `backing` is the thing's folded backing as the
/// store reads it in the transaction that would append the edit, and `expected`
/// the revision the caller read: the refusal is read as one revision past it, the
/// claim having moved on in the one way the revision does not count. Both stores
/// call this, so the rule is stated once.
pub fn refuses_a_displaced_backing(
    address: &FactAddress,
    expected: Option<usize>,
    keys: &std::collections::BTreeSet<String>,
    backing: &BTreeMap<String, FieldBacking>,
) -> Option<MemoryError> {
    let backs = |key: &String| {
        backing
            .get(key)
            .is_some_and(|held| held.fact == address.local)
    };
    if keys.iter().all(backs) {
        return None;
    }
    let expected = expected.unwrap_or_default();
    Some(MemoryError::StaleRevision {
        address: address.to_string(),
        expected,
        current: expected + 1,
    })
}
