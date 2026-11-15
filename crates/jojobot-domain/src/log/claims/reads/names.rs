//! **The entities a set of texts names, read from the State by key.** A mention layer
//! renders the stored marks a text holds and resolves the handles it writes; both need
//! the few things the text names, and none needs the whole listing. A mark carries the
//! object it names, a handle is an address the State keeps, a merge is a redirect, and
//! a numbered collection says which key it numbers by, so each is one lookup.

use std::collections::BTreeSet;

use super::super::State;
use crate::log::ObjectId;
use crate::memory::numbered::numbered_handle;
use crate::memory::{Entity, EntityId, Names, Wanted};

impl State {
    /// **What a set of texts asks for, as the renderers take it**: see [`Names`].
    /// Never reads a store and declines nothing: names, badges, merges and numbering
    /// are logged object and key events. A supplied record, which the State holds
    /// under a supply identity and the supply layer serves, is left to that layer.
    pub fn names_read(&self, wanted: &Wanted) -> Names {
        let mut names = Names::default();
        let mut seen: BTreeSet<EntityId> = BTreeSet::new();
        let mut known = |entity: Entity, names: &mut Names| {
            if seen.insert(entity.id.clone()) {
                names.known.push(entity);
            }
        };
        for badge in &wanted.badges {
            // The badge is the object's own id; the row it wears forwards to its
            // survivor when a merge folded it away, and the survivor in its turn.
            let mut at = ObjectId(badge.clone());
            let mut visited: BTreeSet<ObjectId> = BTreeSet::new();
            while visited.insert(at.clone())
                && let Some(row) = self.listed_row(&at)
            {
                known(row, &mut names);
                match self.redirects.get(&at) {
                    Some(next) => at = next.clone(),
                    None => break,
                }
            }
        }
        for handle in &wanted.handles {
            match numbered_handle(handle) {
                Some((collection, _)) => self.name_a_member(handle, &collection, &mut names),
                None => match self.row_wearing(handle) {
                    Some(row) => known(row, &mut names),
                    None => names.missed.push(handle.clone()),
                },
            }
        }
        names
    }

    /// **The row of the thing a handle is worn by now**, or by one that gave it up, the
    /// way the port's `resolve_handle` reads it. `None` when nothing answers to it, and
    /// for a record the supply layer serves.
    fn row_wearing(&self, handle: &EntityId) -> Option<Entity> {
        let object = match self.addresses.get(&handle.0) {
            Some(object) => object.clone(),
            None => self.retired_addresses.get(&handle.0)?.clone(),
        };
        // A handle the thing gave up answers with the row of the thing wearing its badge,
        // which `resolve_handle` finds by that badge; a renderer looks a handle up among
        // the ids wearing it now, so the old spelling names nothing there, as before.
        self.listed_row(&object)
    }

    /// **One numbered address `collection/N`, resolved through the collection** the
    /// way [`State::resolved_entity`] walks it. A member enters `members` under the
    /// address; the collection, and each collection a chain passes through, enters
    /// `known` as the entity of its own handle and under its numbering marker when it
    /// numbers. An address no member holds is a miss.
    fn name_a_member(&self, address: &EntityId, collection: &EntityId, names: &mut Names) {
        let member = self.resolved_entity(address);
        let mut at = Some(collection.clone());
        while let Some(handle) = at {
            let Some(container) = self.resolved_entity(&handle) else {
                break;
            };
            let numbers = self
                .resolve(&container.id.0)
                .and_then(|object| self.numbers_by(object))
                .is_some();
            if numbers {
                // Under the id the thing wears, as the listing enters it, and not under
                // the spelling the text used: a handle the thing gave up, or a numbered
                // address of a member that numbers its own, names no marker of its own.
                let marker = EntityId(format!("{}/#", container.id));
                if !names.known.iter().any(|held| held.id == marker) {
                    names.known.push(Entity {
                        id: marker,
                        badge: None,
                        ..container.clone()
                    });
                }
            }
            if !names.known.iter().any(|held| held.id == container.id) {
                names.known.push(container);
            }
            at = numbered_handle(&handle).map(|(outer, _)| outer);
        }
        // The collection numbers whether or not the address is held, so a miss still
        // carries the marker; the miss itself is named.
        match member {
            Some(member) => names.members.push(Entity {
                id: address.clone(),
                ..member
            }),
            None => names.missed.push(address.clone()),
        }
    }

    /// **A thing's row as the listing serves it**: the handle it wore itself, and the
    /// handle of what it was folded into. `None` for an object the State does not hold
    /// and for one under a supply identity.
    fn listed_row(&self, object: &ObjectId) -> Option<Entity> {
        if object.0.starts_with(super::super::import::SUPPLY_PREFIX) {
            return None;
        }
        let body = self.objects.get(object)?;
        let mut row = self.entity_row(body, self.own_handle_of(object)?)?;
        row.merged_into = self
            .redirects
            .get(object)
            .map(|survivor| self.handle_worn_by(survivor));
        Some(row)
    }
}
