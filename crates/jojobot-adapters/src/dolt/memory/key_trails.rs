//! **The trails of one thing, folded from its writes**, which the read of a
//! single thing returns.

use std::collections::HashMap;

use jojobot_domain::memory::KeyTrails;

use super::*;

/// **The trails of one thing, from every write on it and the records they were
/// written in.** A key's trail is its writes that reach the thing, a key that stays
/// on its record keeps its writes with their moments, and a record's key is kept off
/// the thing when its newest write of that key, unless a later one took the key off,
/// was made under a class that does not reach it.
pub(super) fn trails_from(
    rows: &[RawWrite],
    facts: &[Fact],
    handles: &HashMap<EntityId, EntityId>,
) -> KeyTrails {
    let by_id: std::collections::HashMap<&FactId, &Fact> =
        facts.iter().map(|f| (&f.id, f)).collect();
    let mut trails = jojobot_domain::memory::KeyTrails::default();
    let mut held: std::collections::BTreeMap<(FactId, String), jojobot_domain::memory::WriteClass> =
        std::collections::BTreeMap::new();
    for row in rows {
        let key = row.key.clone();
        let value = row.value.clone();
        let carried = FactId(row.fact.clone());
        let class = row.class;
        match &value {
            Some(_) => {
                held.insert((carried.clone(), key.clone()), class);
            }
            None => {
                held.remove(&(carried.clone(), key.clone()));
            }
        }
        if !jojobot_domain::memory::reaches_the_thing(class) {
            // **A key that stays on its record keeps its writes here**, with
            // their moments, because the history of a key on a thing leaves
            // them out.
            if by_id.contains_key(&carried) {
                trails
                    .record_writes
                    .push(jojobot_domain::memory::RecordFieldWrite {
                        fact: carried,
                        key,
                        ordinal: row.ordinal.max(0) as u64,
                        value,
                        written_at: row.written_at,
                    });
            }
            continue;
        }
        let Some(fact) = by_id.get(&carried) else {
            continue;
        };
        trails
            .history
            .entry(key)
            .or_default()
            .push(super::field_write_of(
                fact,
                value,
                row.written_at,
                row.origin.clone(),
                handles,
            ));
    }
    for ((fact, key), class) in held {
        if !jojobot_domain::memory::reaches_the_thing(class) {
            trails.kept_off.entry(fact).or_default().insert(key);
        }
    }
    trails
        .record_writes
        .sort_by(|a, b| (&a.key, a.ordinal).cmp(&(&b.key, b.ordinal)));
    trails
}
