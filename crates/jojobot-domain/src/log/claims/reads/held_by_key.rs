//! **Which things hold a key, and in what order**, read from the State alone: the
//! things a browse offers, each with the fields a read serves for it. A scan of the
//! things the State holds, with no copy kept beside it.

use super::super::State;
use crate::memory::filter::{Direction, Filter, Orderable, Test};
use crate::memory::holders::{OrderedHead, head_of};
use crate::memory::types::ValueType;
use crate::memory::{EntityId, EntityKind};

impl State {
    /// **The things a browse offers that are of `kind`, when one is named**, each
    /// with the fields a read serves for it.
    fn browsed(
        &self,
        kind: Option<EntityKind>,
    ) -> Vec<(EntityId, std::collections::BTreeMap<String, String>)> {
        let declared = self.declared_types();
        self.objects
            .iter()
            .filter_map(|(object, body)| {
                let entity = self.entity_of(object, body)?;
                (entity.browsable() && kind.is_none_or(|k| entity.kind == k))
                    .then(|| (entity.id, self.fields_served(object, &declared)))
            })
            .collect()
    }

    /// **The things whose fields hold a leaf**, in handle order: a thing a browse
    /// offers, of `kind` when one is named, whose `key` meets `test`.
    pub fn keys_holding(&self, key: &str, test: &Test, kind: Option<EntityKind>) -> Vec<EntityId> {
        let leaf = Filter::leaf(Some(key.trim()), test.clone());
        let mut held: Vec<EntityId> = self
            .browsed(kind)
            .into_iter()
            .filter(|(_, fields)| leaf.holds(fields))
            .map(|(id, _)| id)
            .collect();
        held.sort();
        held
    }

    /// **The front of the order of a key**: the things a browse offers that are of
    /// `kind` and among `among` when it is given, each holding a value of `holds`
    /// that has a place in the order, cut after the last tie of the `k`th.
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
        let among: Option<std::collections::HashSet<&EntityId>> =
            among.map(|among| among.iter().collect());
        let placed = self
            .browsed(kind)
            .into_iter()
            .filter(|(id, _)| among.as_ref().is_none_or(|among| among.contains(id)))
            .filter_map(|(id, fields)| {
                let held = fields.get(key)?;
                Some((Orderable::read_to_order(holds, held)?, id, held.clone()))
            })
            .collect();
        head_of(placed, direction, k)
    }
}
