//! The bag each key of a record is held under, as the real store reads it.

use super::*;

impl InMemoryMemory {
    /// **The bag each key of a record is held under now**, as the real store reads
    /// it: the class of the key's newest write on the record, for a key whose
    /// newest write did not take it off.
    pub(super) fn held_classes(
        &self,
        home: &EntityId,
        fact: &FactId,
    ) -> std::collections::BTreeMap<String, crate::memory::WriteClass> {
        self.with_writes(|writes| {
            let mut mine: Vec<&StoredWrite> = writes
                .iter()
                .filter(|w| &w.entity == home && &w.fact == fact)
                .collect();
            mine.sort_by(|a, b| a.key.cmp(&b.key).then(a.ordinal.cmp(&b.ordinal)));
            let mut held = std::collections::BTreeMap::new();
            for write in mine {
                match &write.value {
                    Some(_) => held.insert(write.key.clone(), write.class),
                    None => held.remove(&write.key),
                };
            }
            held
        })
    }
}
