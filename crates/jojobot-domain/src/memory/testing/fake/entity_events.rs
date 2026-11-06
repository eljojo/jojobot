//! **The fake's entity writers emit their claims events**, so the claims State
//! the fake keeps holds the objects a capture is decided against.
//!
//! The fake chooses no event itself. It decodes its own record into the
//! domain's [`EntityRow`], hands it to `entity_change_events` with the change,
//! and applies what comes back with `State::apply`, as the seam does.

use super::*;
use crate::log::ObjectId;
use crate::log::claims::entity_change::{EntityChange, entity_change_events};
use crate::log::claims::import::EntityRow;

impl InMemoryMemory {
    /// **Emit the claims events of a change to `row`**, the entity as the fake
    /// stores it (its parent already the key it wears). The decode to the
    /// domain's row is storage, as the Dolt adapter's is; the choice of events
    /// is the domain's.
    pub(super) fn emit_entity_change(
        &self,
        row: &Entity,
        change: EntityChange<'_>,
        session: Option<&str>,
    ) {
        let prose = self
            .prose
            .lock()
            .expect("fake mutex poisoned")
            .get(&row.id)
            .cloned()
            .unwrap_or_default();
        let decoded = EntityRow {
            key: ObjectId(row.badge.clone().unwrap_or_else(|| row.id.to_string())),
            handle: row.id.to_string(),
            kind: row.kind.as_token().to_string(),
            name: row.name.clone(),
            source: row.source.clone(),
            boot: Some(row.boot.as_token().to_string()),
            parent: row
                .parent
                .as_ref()
                .map(|parent| ObjectId(parent.to_string())),
            aliases: row.aliases.clone(),
            crm: row.crm.clone(),
            prose,
            badge: row.badge.clone(),
            archived: row
                .archived
                .as_ref()
                .map(|archived| (archived.reason.clone(), archived.at)),
            merged_into: None,
            taken_in: None,
        };
        let events = entity_change_events(
            &decoded,
            change,
            self.clock().now(),
            session.map(str::to_string),
        );
        let mut claims = self.claims.lock().expect("fake mutex poisoned");
        for event in &events {
            claims.apply(event);
        }
    }

    /// The claims State as the entity writers have left it.
    pub fn claims_state(&self) -> crate::log::claims::State {
        self.claims.lock().expect("fake mutex poisoned").clone()
    }
}

#[cfg(test)]
mod tests;
