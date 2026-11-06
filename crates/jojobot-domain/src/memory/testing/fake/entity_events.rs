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
    /// **The prose of `row`**, which the fake keeps under the handle the thing was
    /// written under and never re-keys on a rename, as the real store keeps it in
    /// a column of the row. The handle the row wears now answers first, then the
    /// one it is being renamed from (its former-handle entry is written after the
    /// event), then the handles it has left, newest first.
    ///
    /// The former handles are read before the prose is, so no two locks are held
    /// at once.
    fn prose_of(&self, row: &Entity, renamed_from: Option<&EntityId>) -> String {
        let badge = row.badge.as_deref();
        let former: Vec<EntityId> = self
            .former_handles
            .lock()
            .expect("fake mutex poisoned")
            .iter()
            .filter(|held| Some(held.badge.as_str()) == badge)
            .map(|held| held.former.clone())
            .rev()
            .collect();
        let prose = self.prose.lock().expect("fake mutex poisoned");
        std::iter::once(row.id.clone())
            .chain(renamed_from.cloned())
            .chain(former)
            .find_map(|handle| prose.get(&handle).cloned())
            .unwrap_or_default()
    }

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
        let renamed_from = match &change {
            EntityChange::Renamed { from, .. } => Some(EntityId((*from).to_string())),
            _ => None,
        };
        let prose = self.prose_of(row, renamed_from.as_ref());
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

    /// **Emit the claims event of declaring `declared`**, unless the State already
    /// holds exactly that declaration for its name. The choice is the domain's
    /// (`type_declaration_events`); the fake applies what comes back, as the seam
    /// applies it.
    pub(super) fn emit_type_declaration(
        &self,
        declared: &crate::memory::types::DeclaredType,
        session: Option<&str>,
    ) {
        let mut claims = self.claims.lock().expect("fake mutex poisoned");
        let events = crate::log::claims::type_change::type_declaration_events(
            claims.declared(&ObjectId(declared.name.clone())),
            declared,
            self.clock().now(),
            session.map(str::to_string),
        );
        for event in &events {
            claims.apply(event);
        }
    }

    /// **Emit the claims event of reclaiming `kind`**, whose rows now hold no
    /// declaration. The choice is the domain's (`type_withdrawal_events`): one
    /// withdrawal when the State holds a declaration for the kind, and none when
    /// it holds nothing. The fake applies what comes back, as the seam does.
    pub(super) fn emit_type_withdrawal(&self, kind: &str) {
        let kind = ObjectId(kind.to_string());
        let mut claims = self.claims.lock().expect("fake mutex poisoned");
        let events = crate::log::claims::type_change::type_withdrawal_events(
            &kind,
            claims.declared(&kind),
            self.clock().now(),
            None,
        );
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
