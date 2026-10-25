//! Reference kinds are judged after lookup without changing literal constraints.

use super::*;

pub fn guard_fit(
    kind: &str,
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
    declared: &[types::DeclaredType],
) -> Result<(), MemoryError> {
    guard_fit_resolved(kind, before, after, declared, &BTreeMap::new())
}

/// Validate an edge before it is written: the object's id grammar, then the
/// shape's kind rule. Both adapters call this, so neither can store an edge the
/// other would refuse.
pub fn validate_edge(edge: &Edge) -> Result<(), MemoryError> {
    validate_subject(&edge.object)?;
    if numbered::numbered_handle(&edge.object).is_some() {
        return Ok(());
    }
    validate_resolved_edge(edge, &edge.object)
}

/// Even a canonical handle ending in a number is checked by its own kind here.
pub fn validate_resolved_edge(edge: &Edge, target: &EntityId) -> Result<(), MemoryError> {
    match edge.shape.object_kind() {
        Some(required) if target.kind() != Some(required) => {
            Err(MemoryError::InvalidEdge(format!(
                "a '{}' edge points at a {required}, got '{}'",
                edge.shape, edge.object
            )))
        }
        _ => Ok(()),
    }
}
