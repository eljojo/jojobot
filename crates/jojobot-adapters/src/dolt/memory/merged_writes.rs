use super::{MergeStep, ServedWrite, merge_in};
use std::collections::HashMap;

/// Place writes by the merge ledger once, for one survivor or a grouped read.
pub(super) fn apply_merges<T: ServedWrite>(
    held: &mut HashMap<String, Vec<T>>,
    steps: &[MergeStep],
) {
    for step in steps {
        let theirs = held.remove(&step.folded).unwrap_or_default();
        let ours = held.remove(&step.survivor).unwrap_or_default();
        held.insert(step.survivor.clone(), merge_in(ours, theirs, &step.claims));
    }
}
