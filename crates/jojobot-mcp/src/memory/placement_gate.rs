//! **The one entry every verb that writes a caller's keys asks before it writes
//! a place.** A section's or an entry's place and a section's `edges` are the
//! engine's alone, and copied places are dropped on the kind the STORE holds for
//! the thing, not on the kind its address was typed with: a thing retyped into a
//! document still answers to its old handle, and a handle typed with the old kind
//! must not walk a caller's place past the gate.
//!
//! `add_entity`, `update_entity`, `capture` (and `capture_batch`, which writes one
//! capture per item) and `update_fact` each make this one call (rule 598). The
//! domain owns which read keys are copied and which section keys stay refused.

use super::*;
use jojobot_domain::memory::{FactAddress, FactId, document, entries};

#[derive(Default)]
pub(super) struct DroppedKeys(std::collections::BTreeSet<String>);

impl DroppedKeys {
    pub(super) fn note(&self, body: &mut serde_json::Value) {
        if !self.0.is_empty() {
            body["dropped_keys"] = serde_json::json!(self.0);
            body["dropped_keys_note"] =
                "These keys are written by the server; their copied values were dropped.".into();
        }
    }
}

impl Jojobot {
    /// **Drop copied server keys and retain the refusal of section edges.**
    /// `record` is the local id of the claim an edit changes, when the write is an
    /// edit; a new claim has none. Both bags and clears are judged before they
    /// join, so a copied key in both bags cannot cause a conflict.
    ///
    /// **Nothing is read unless a guarded key is named**, so the common write pays
    /// nothing for this gate. When one is, the kind is the one of the claim that is
    /// edited, else of the handle the thing answers to now. A new claim is a section
    /// when it carries `order`; an edited claim is one when the store holds it as
    /// one. A store that cannot answer refuses rather than passes.
    pub(super) async fn drop_read_keys<'a>(
        &self,
        thing: &EntityId,
        record: Option<&FactId>,
        bags: impl IntoIterator<Item = &'a mut std::collections::BTreeMap<String, String>>,
        cleared: &mut Vec<String>,
    ) -> Result<DroppedKeys, MemoryError> {
        let mut bags: Vec<_> = bags.into_iter().collect();
        let named: Vec<&str> = bags
            .iter()
            .flat_map(|bag| bag.keys().map(String::as_str))
            .chain(cleared.iter().map(String::as_str))
            .collect();
        if !named.iter().any(|key| {
            entries::is_placement_key(key)
                || key.trim() == document::EDGES
                || jojobot_domain::memory::copied_read_key(None, key)
        }) {
            return Ok(DroppedKeys::default());
        }
        let (kind, section) = if named
            .iter()
            .any(|key| entries::is_placement_key(key) || key.trim() == document::EDGES)
        {
            self.stored_kind(thing, record).await?
        } else {
            (None, None)
        };
        let section =
            section.unwrap_or_else(|| named.iter().any(|key| key.trim() == document::ORDER));
        if let Some(refused) = entries::refuses_a_callers_placement(kind, section, named) {
            return Err(refused);
        }
        let mut dropped = DroppedKeys::default();
        let mut keep = |key: &String| {
            if jojobot_domain::memory::copied_read_key(kind, key) {
                dropped.0.insert(key.trim().to_string());
                false
            } else {
                true
            }
        };
        for bag in &mut bags {
            bag.retain(|key, _| keep(key));
        }
        cleared.retain(keep);
        Ok(dropped)
    }

    /// The kind the store holds for `thing`, and whether `record` is a section.
    ///
    /// **An edit's address is read where its claim is served now**, past a merge and
    /// under the handle its thing wears today (`served_address`), the way
    /// `update_fact` reads it for a list delta: an address from before a merge names
    /// a folded document that holds nothing, and its sections are the survivor's.
    /// **A claim that is still not found is judged a section** of the document it was
    /// addressed to, never "not one": the edit that follows will say the claim is
    /// missing, and a gate that guessed the other way would let an injection through
    /// on every address it could not read.
    async fn stored_kind(
        &self,
        thing: &EntityId,
        record: Option<&FactId>,
    ) -> Result<(Option<EntityKind>, Option<bool>), MemoryError> {
        let Some(local) = record else {
            let now = self.current_handle(thing).await?;
            return Ok((now.kind(), None));
        };
        let served = self
            .served_address(&FactAddress::new(thing.clone(), local.clone()))
            .await?;
        let carried = match self.memory.recall(&served.home).await {
            Ok(carried) => carried,
            Err(MemoryError::UnknownEntity { .. }) => Vec::new(),
            Err(e) => return Err(e),
        };
        Ok(match carried.iter().find(|fact| fact.id == served.local) {
            Some(fact) => (fact.home.kind(), Some(document::is_section(fact))),
            None => (served.home.kind(), Some(true)),
        })
    }
}
