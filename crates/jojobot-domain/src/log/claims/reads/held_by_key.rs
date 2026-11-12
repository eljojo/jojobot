//! **Which things hold a key, and in what order**, read from the State alone, in
//! one pass over the things it holds. The pass looks each thing's key up in the
//! fold the State already holds, in place: it builds no entity and copies no map
//! for a thing it does not return, and an id is built only for a thing the read
//! answers with.

use std::borrow::Cow;
use std::cmp::Ordering;

use super::super::State;
use crate::log::ObjectId;
use crate::memory::filter::{Direction, Filter, Orderable, Test};
use crate::memory::holders::OrderedHead;
use crate::memory::types::ValueType;
use crate::memory::{EntityId, EntityKind};

impl State {
    /// **The things a browse offers, borrowed**: every thing that is not archived and
    /// not folded into another, wears a handle of a kind the process knows, and is
    /// of `kind` when one is named. Each is its object and the handle it wears.
    /// Nothing is built.
    pub(super) fn browse(
        &self,
        kind: Option<EntityKind>,
    ) -> impl Iterator<Item = (&ObjectId, &str)> + '_ {
        self.objects.iter().filter_map(move |(object, body)| {
            #[cfg(test)]
            State::note_visit();
            if body.archived || self.redirects.contains_key(object) {
                return None;
            }
            let handle = self.own_handle_str(object)?;
            let found = EntityKind::from_token(handle.split_once(':')?.0)?;
            kind.is_none_or(|wanted| wanted == found)
                .then_some((object, handle))
        })
    }

    /// **The value a thing holds under a key, as a read serves it**, looked up in
    /// the held fold and copied only when a mark of a stored handle has to be
    /// composed to the handle. `None` for a key the thing does not hold.
    pub(super) fn served_value(&self, object: &ObjectId, key: &str) -> Option<Cow<'_, str>> {
        let value = self.held_fields.0.get(object)?.get(key)?;
        Some(match self.serve_handles(value) {
            Some(served) => Cow::Owned(served),
            None => Cow::Borrowed(value.as_str()),
        })
    }

    /// **The things whose fields hold a leaf**, in handle order: a thing a browse
    /// offers, of `kind` when one is named, whose `key` meets `test`.
    pub fn keys_holding(&self, key: &str, test: &Test, kind: Option<EntityKind>) -> Vec<EntityId> {
        let key = key.trim();
        let leaf = Filter::leaf(Some(key), test.clone());
        let mut held: Vec<EntityId> = self
            .browse(kind)
            .filter(|(object, _)| {
                leaf.holds_the_value_of_its_key(self.served_value(object, key).as_deref())
            })
            .map(|(_, handle)| EntityId(handle.to_string()))
            .collect();
        held.sort();
        held
    }

    /// **The front of the order of a key**: the things a browse offers that are of
    /// `kind` and among `among` when it is given, each holding a value of `holds`
    /// that has a place in the order, cut after the last tie of the `k`th. The
    /// things are placed borrowed; an id and a value are built for the head alone.
    pub fn ordered_first_k(
        &self,
        key: &str,
        holds: ValueType,
        direction: Direction,
        kind: Option<EntityKind>,
        among: Option<&[EntityId]>,
        k: usize,
    ) -> OrderedHead {
        let key = key.trim();
        let among: Option<std::collections::HashSet<&str>> =
            among.map(|among| among.iter().map(EntityId::as_str).collect());
        let mut placed: Vec<(Orderable, &str, Cow<'_, str>)> = self
            .browse(kind)
            .filter(|(_, handle)| among.as_ref().is_none_or(|among| among.contains(handle)))
            .filter_map(|(object, handle)| {
                let held = self.served_value(object, key)?;
                Some((Orderable::read_to_order(holds, &held)?, handle, held))
            })
            .collect();
        placed.sort_by(|a, b| {
            let by_value = match direction {
                Direction::Ascending => a.0.cmp(&b.0),
                Direction::Descending => b.0.cmp(&a.0),
            };
            by_value.then_with(|| a.1.cmp(b.1))
        });
        let mut end = k.min(placed.len());
        while end > 0
            && end < placed.len()
            && placed[end].0.cmp(&placed[end - 1].0) == Ordering::Equal
        {
            end += 1;
        }
        let cut = end < placed.len();
        placed.truncate(end);
        OrderedHead {
            head: placed
                .into_iter()
                .map(|(_, handle, held)| (EntityId(handle.to_string()), held.into_owned()))
                .collect(),
            cut,
        }
    }
}
