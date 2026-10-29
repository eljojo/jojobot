use super::*;

/// A doc built by hand, so the index's behaviour can be examined without a
/// store underneath it.
pub(super) fn scan(doc_id: &str, entity: Option<Entity>, prose: &str, facts: Vec<Fact>) -> DocScan {
    // **The store is what folds a thing's fields, so this stands in for
    // it — by calling the one fold, never by restating what it does.**
    // Which writes count and which keys never fold are the fold's to
    // decide; a fixture with its own copy of those rules drifts from the
    // store silently and teaches the index a system nobody runs.
    //
    // The writes are recovered from the records because a doc built here
    // has no substrate under it: every doc writes each of its keys once, in
    // the order its records are listed, which is the case where the records
    // and the writes agree about what the thing holds. A fixture that needs
    // them to disagree says so itself.
    let mut ordinals: BTreeMap<String, u64> = BTreeMap::new();
    let mut writes: Vec<KeyWrite> = Vec::new();
    for fact in facts
        .iter()
        .filter(|f| entity.as_ref().is_some_and(|e| f.home == e.id))
    {
        for (key, value) in &fact.fields {
            let ordinal = ordinals.entry(key.clone()).or_default();
            *ordinal += 1;
            writes.push(KeyWrite {
                recorded_by: None,
                key: key.clone(),
                ordinal: *ordinal,
                value: Some(value.clone()),
                fact: fact.id.clone(),
                status: fact.status,
                note: fact.details.clone(),
                provenance: fact.provenance,
                standing: fact.standing,
                written_at: None,
            });
        }
    }
    // Nothing is declared over a doc built by hand, so every key here folds
    // the unconfigured way. A fixture that needs a counter declares one.
    let fields = folded_fields(&writes, &[]);
    DocScan {
        doc_id: doc_id.into(),
        title: entity.as_ref().map(|e| e.name.clone()).unwrap_or_default(),
        prose: prose.into(),
        entity,
        facts,
        fields,
        owner: None,
        field_links: Default::default(),
        unmapped: Vec::new(),
    }
}

mod clock_rank;
