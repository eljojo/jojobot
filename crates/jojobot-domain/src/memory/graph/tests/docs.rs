use super::*;

pub(super) fn doc(entity: Entity, prose: &str, facts: Vec<Fact>) -> DocScan {
    let fields = facts
        .iter()
        .filter(|f| f.status == FactStatus::Active)
        .flat_map(|f| f.fields.iter())
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect();
    doc_holding(entity, prose, facts, fields)
}

/// A doc whose thing holds what the store says it holds, records or no
/// records — the shape a key written more than once really arrives in.
pub(super) fn doc_holding(
    entity: Entity,
    prose: &str,
    mut facts: Vec<Fact>,
    fields: BTreeMap<String, String>,
) -> DocScan {
    for fact in &mut facts {
        fact.field_links = links_of(&fact.fields);
    }
    let field_links = links_of(&fields);
    DocScan {
        doc_id: entity.id.to_string(),
        title: entity.name.clone(),
        prose: prose.to_string(),
        entity: Some(entity),
        facts,
        fields,
        owner: None,
        field_links,
        unmapped: Vec::new(),
    }
}

/// **The links a store decides at the write**, for a fixture that writes fields as
/// text: every key whose value names a handle links to the handles it names.
fn links_of(fields: &BTreeMap<String, String>) -> BTreeMap<String, Vec<EntityId>> {
    fields
        .iter()
        .filter_map(|(key, value)| {
            let named = crate::memory::handles_in_value(value);
            (!named.is_empty()).then(|| (key.clone(), named))
        })
        .collect()
}

/// An owned document: readable by one identity and nobody else.
pub(super) fn owned_by(entity: Entity, owner: &str, prose: &str) -> DocScan {
    DocScan {
        owner: Some(EntityId(owner.to_string())),
        ..doc(entity, prose, Vec::new())
    }
}
