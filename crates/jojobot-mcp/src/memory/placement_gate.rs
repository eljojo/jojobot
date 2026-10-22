//! **The one entry every verb that writes a caller's keys asks before it writes
//! a place.** A section's or an entry's place and a section's `edges` are the
//! engine's alone, and the refusal is judged here on the kind the STORE holds for
//! the thing, not on the kind its address was typed with: a thing retyped into a
//! document still answers to its old handle, and a handle typed with the old kind
//! must not walk a caller's place past the gate.
//!
//! `add_entity`, `update_entity`, `capture` (and `capture_batch`, which writes one
//! capture per item) and `update_fact` each make this one call (rule 598). The
//! pure test of the keys is the domain's `refuses_a_callers_placement`.

use super::*;
use jojobot_domain::memory::{FactId, document, entries};

impl Jojobot {
    /// **The refusal of a caller's own place, ready to return, or none.**
    /// `record` is the local id of the claim an edit changes, when the write is an
    /// edit; a new claim has none. `named` is every key the call writes: its fields
    /// and settings and the keys it clears.
    ///
    /// **Nothing is read unless a guarded key is named**, so the common write pays
    /// nothing for this gate. When one is, the kind is the one of the claim that is
    /// edited, else of the handle the thing answers to now. A new claim is a section
    /// when it carries `order`; an edited claim is one when the store holds it as
    /// one. A store that cannot answer refuses rather than passes.
    pub(crate) async fn refuses_placement(
        &self,
        verb: &'static str,
        thing: &EntityId,
        record: Option<&FactId>,
        named: &[&str],
    ) -> Option<Result<CallToolResult, McpError>> {
        if !named
            .iter()
            .any(|key| entries::is_placement_key(key) || key.trim() == document::EDGES)
        {
            return None;
        }
        let (kind, section) = match self.stored_kind(thing, record).await {
            Ok(held) => held,
            Err(e) => return Some(memory_declined(verb, e)),
        };
        let section =
            section.unwrap_or_else(|| named.iter().any(|key| key.trim() == document::ORDER));
        entries::refuses_a_callers_placement(kind, section, named.iter().copied())
            .map(|refused| memory_declined(verb, refused))
    }

    /// The kind the store holds for `thing`, and whether `record` is a section.
    /// The claim is found by local id within the home the store resolves for the
    /// typed handle, as `update_fact`'s archive check finds it.
    async fn stored_kind(
        &self,
        thing: &EntityId,
        record: Option<&FactId>,
    ) -> Result<(Option<EntityKind>, Option<bool>), MemoryError> {
        if let Some(local) = record {
            let carried = match self.memory.recall(thing).await {
                Ok(carried) => carried,
                Err(MemoryError::UnknownEntity { .. }) => Vec::new(),
                Err(e) => return Err(e),
            };
            if let Some(fact) = carried.iter().find(|fact| &fact.id == local) {
                return Ok((fact.home.kind(), Some(document::is_section(fact))));
            }
        }
        let now = self.current_handle(thing).await?;
        Ok((now.kind(), record.map(|_| false)))
    }
}
