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
    facts: Vec<Fact>,
    fields: BTreeMap<String, String>,
) -> DocScan {
    DocScan {
        doc_id: entity.id.to_string(),
        title: entity.name.clone(),
        prose: prose.to_string(),
        entity: Some(entity),
        facts,
        fields,
        owner: None,
        field_links: Default::default(),
        unmapped: Vec::new(),
    }
}

/// An owned document: readable by one identity and nobody else.
pub(super) fn owned_by(entity: Entity, owner: &str, prose: &str) -> DocScan {
    DocScan {
        owner: Some(EntityId(owner.to_string())),
        ..doc(entity, prose, Vec::new())
    }
}
