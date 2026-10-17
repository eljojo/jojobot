//! **Who holds a key, and in what order** — the two questions a store answers
//! about what things hold, without a caller reading every thing to find out.
//!
//! [`Memory::keys_holding`] asks which things hold a key such that one leaf's
//! test passes. [`Memory::ordered_first_k`] asks for the head of the things
//! holding a key, in the order an ordering of that key places them. A store that
//! keeps its values by key answers each from what it keeps; **a store that does
//! not answers by the full read** ([`holding_by_reading`], [`head_by_reading`]),
//! which is the answer the others are held to. A caller never branches on which
//! store it has, so a store-backed index can take over from the read later and no
//! caller changes (rule 510).
//!
//! **Both answer from the fold of the thing's fields**, the same bag a key filter
//! and an ordering read, and through the same reading of a value
//! ([`filter::Leaf`](super::filter::Leaf) and [`Orderable`]): a value that is no
//! value of the type asked about does not match and has no place in the order.
//! Only things that stand in a default browse are answered (rule 292's index of
//! the keys a thing holds leaves out an archived thing, and so do these).

use std::cmp::Ordering;

use super::filter::{Direction, Filter, Orderable, Test};
use super::types::ValueType;
use super::{EntityId, EntityKind, Memory, MemoryError};

/// **The head of an ordering.** The things holding a key whose value has a place
/// in the order, nearest the front first, and whether more stand behind them.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct OrderedHead {
    /// The things at the front, each with the value it holds, as it is held.
    /// **At least `k` of them when that many are placed, and every thing that
    /// ties with the last**: a tie is placed by the order the caller's items came
    /// in, which this cannot know, so a tie is never cut.
    pub head: Vec<(EntityId, String)>,
    /// Whether things with a place in the order stand behind the head. False
    /// means the head is all of them.
    pub cut: bool,
}

/// **The ask of the full read for [`Memory::keys_holding`].**
pub async fn holding_by_reading<M>(
    store: &M,
    key: &str,
    test: &Test,
    kind: Option<EntityKind>,
) -> Result<Vec<EntityId>, MemoryError>
where
    M: Memory + ?Sized,
{
    let leaf = Filter::leaf(Some(key), test.clone());
    let mut held = Vec::new();
    for entity in store.list_entities(kind).await? {
        if entity.browsable() && leaf.holds(&store.fields(&entity.id).await?) {
            held.push(entity.id);
        }
    }
    held.sort();
    Ok(held)
}

/// **The ask of the full read for [`Memory::ordered_first_k`].**
pub async fn head_by_reading<M>(
    store: &M,
    key: &str,
    holds: ValueType,
    direction: Direction,
    kind: Option<EntityKind>,
    among: Option<&[EntityId]>,
    k: usize,
) -> Result<OrderedHead, MemoryError>
where
    M: Memory + ?Sized,
{
    let key = key.trim();
    let among: Option<std::collections::HashSet<&EntityId>> =
        among.map(|among| among.iter().collect());
    let mut placed: Vec<(Orderable, EntityId, String)> = Vec::new();
    for entity in store.list_entities(kind).await? {
        if !entity.browsable()
            || among
                .as_ref()
                .is_some_and(|among| !among.contains(&entity.id))
        {
            continue;
        }
        let fields = store.fields(&entity.id).await?;
        if let Some(held) = fields.get(key)
            && let Some(read) = Orderable::read_to_order(holds, held)
        {
            placed.push((read, entity.id, held.clone()));
        }
    }
    Ok(head_of(placed, direction, k))
}

/// **The head of these placed things**, nearest the front first and cut after the
/// last tie of the `k`th. For the stores that find the things placed by some
/// other means than the sorted copy they keep.
pub fn head_of(
    mut placed: Vec<(Orderable, EntityId, String)>,
    direction: Direction,
    k: usize,
) -> OrderedHead {
    placed.sort_by(|a, b| {
        let by_value = match direction {
            Direction::Ascending => a.0.cmp(&b.0),
            Direction::Descending => b.0.cmp(&a.0),
        };
        by_value.then_with(|| a.1.cmp(&b.1))
    });
    let mut end = k.min(placed.len());
    while end > 0 && end < placed.len() && placed[end].0.cmp(&placed[end - 1].0) == Ordering::Equal
    {
        end += 1;
    }
    let cut = end < placed.len();
    placed.truncate(end);
    OrderedHead {
        head: placed.into_iter().map(|(_, id, held)| (id, held)).collect(),
        cut,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placed(values: &[(&str, f64)]) -> Vec<(Orderable, EntityId, String)> {
        values
            .iter()
            .map(|(id, value)| {
                (
                    Orderable::Number(*value),
                    EntityId::new(EntityKind::THING, *id),
                    value.to_string(),
                )
            })
            .collect()
    }

    fn ids(head: &OrderedHead) -> Vec<&str> {
        head.head.iter().map(|(id, _)| id.slug()).collect()
    }

    #[test]
    fn a_head_is_cut_after_the_last_tie_of_the_kth() {
        let all = placed(&[("a", 1.0), ("b", 2.0), ("c", 2.0), ("d", 2.0), ("e", 3.0)]);
        let head = head_of(all.clone(), Direction::Ascending, 2);
        assert_eq!(ids(&head), ["a", "b", "c", "d"]);
        assert!(head.cut, "e stands behind the head");

        let head = head_of(all.clone(), Direction::Ascending, 1);
        assert_eq!(
            ids(&head),
            ["a"],
            "a tie is only extended when the cut falls in it"
        );

        let head = head_of(all.clone(), Direction::Descending, 1);
        assert_eq!(ids(&head), ["e"]);
        assert!(head.cut);

        let head = head_of(all.clone(), Direction::Ascending, 5);
        assert_eq!(head.head.len(), 5);
        assert!(!head.cut, "the head is all of them");

        let head = head_of(all, Direction::Ascending, 0);
        assert!(head.head.is_empty());
        assert!(head.cut, "things are placed, and none was asked for");

        let none = head_of(Vec::new(), Direction::Ascending, 3);
        assert_eq!(none, OrderedHead::default());
    }
}
