//! [`Held`] — **the values the image holds, by key** (decision log 292).
//!
//! A filter asks which things hold a key with some value, and an ordering asks
//! which hold the smallest. Answered from the documents, either reads every
//! thing. Answered from here, either is a lookup.
//!
//! Three maps, all built from each document's folded fields and nothing else:
//!
//! - **by value**: for every key, every whole value held (trimmed, as an equality
//!   reads it) to the things holding it, and for a value with a comma in it every
//!   item to the things holding the list. A `contains` is a lookup, and so is a
//!   `!=`: the things holding the key, less the ones holding that value.
//! - **by place**: for every key some type declares a date or a number, the
//!   things holding a value of that type, in the order of the value. A range is a
//!   range of this map, and the head of an ordering is its front.
//!
//! **A value is read the way the evaluator reads it** (rule 435): equality is the
//! trimmed text, a list is read apart by [`list_items`], and a date or a number is
//! read by [`Orderable`]. A value that is no value of its declared type has no
//! place and is in no range, as it is in none when the evaluator is asked.
//!
//! **Kept current a document at a time.** [`Held::replace`] takes off what the
//! old document held and puts on what the new one holds, so a write costs the
//! fields of the thing it touched and nothing else. An empty entry is dropped, so
//! a map kept by a run of writes is the map a fresh build makes
//! ([`Held::is_what_a_build_would_make`]).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::ops::Bound;

use jojobot_domain::memory::EntityId;
use jojobot_domain::memory::filter::{Direction, Op, Orderable, Test};
use jojobot_domain::memory::types::{DeclaredType, ValueType, list_items};

/// What a place is read as: the two types an ordering reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum Placing {
    Date,
    Number,
}

impl Placing {
    fn of(holds: ValueType) -> Option<Placing> {
        match holds {
            ValueType::Date => Some(Placing::Date),
            ValueType::Number => Some(Placing::Number),
            _ => None,
        }
    }

    fn type_read(self) -> ValueType {
        match self {
            Placing::Date => ValueType::Date,
            Placing::Number => ValueType::Number,
        }
    }
}

type Holders = BTreeSet<EntityId>;

/// The things holding each value of a key.
type ByValue = HashMap<String, HashMap<String, Holders>>;

/// The values of a key in order, each with the things holding it.
type ByPlace = BTreeMap<Orderable, Holders>;

/// The head of an ordering, found in the fold: who is in front, with what, and
/// whether more stand behind.
pub(super) struct Front {
    /// The things at the front, nearest first, each with its place.
    pub placed: Vec<(EntityId, Orderable)>,
    /// Whether things with a place stand behind them.
    pub cut: bool,
}

#[derive(Default)]
pub(super) struct Held {
    /// The keys a type declares a date or a number, and so the places kept.
    declared: BTreeSet<(String, Placing)>,
    /// The things holding each key.
    carriers: HashMap<String, Holders>,
    /// The things holding each whole value of each key.
    whole: ByValue,
    /// The things holding each item of each list value of each key. **Only a
    /// value with a comma in it has items of its own**: a value without one is
    /// its own one item, and is found in `whole`.
    items: ByValue,
    /// The things holding a value that stands somewhere, for each key declared to
    /// hold dates or numbers.
    placed: HashMap<(String, Placing), ByPlace>,
}

impl Held {
    /// An empty copy for these declarations.
    pub fn declaring(types: &[DeclaredType]) -> Held {
        let declared = types
            .iter()
            .flat_map(|declared| declared.fields.iter())
            .filter_map(|field| Some((field.key.clone(), Placing::of(field.holds)?)))
            .collect();
        Held {
            declared,
            ..Held::default()
        }
    }

    /// Put on what a thing holds.
    pub fn add(&mut self, id: &EntityId, fields: &BTreeMap<String, String>) {
        for (key, value) in fields {
            self.carriers
                .entry(key.clone())
                .or_default()
                .insert(id.clone());
            insert_value(&mut self.whole, key, value.trim(), id);
            if value.contains(',') {
                for item in list_items(value) {
                    insert_value(&mut self.items, key, item, id);
                }
            }
            for placing in self.placings_of(key) {
                if let Some(read) = Orderable::read(placing.type_read(), value) {
                    self.placed
                        .entry((key.clone(), placing))
                        .or_default()
                        .entry(read)
                        .or_default()
                        .insert(id.clone());
                }
            }
        }
    }

    /// Take off what a thing held.
    pub fn remove(&mut self, id: &EntityId, fields: &BTreeMap<String, String>) {
        for (key, value) in fields {
            if let Some(held) = self.carriers.get_mut(key) {
                held.remove(id);
                if held.is_empty() {
                    self.carriers.remove(key);
                }
            }
            remove_value(&mut self.whole, key, value.trim(), id);
            if value.contains(',') {
                for item in list_items(value) {
                    remove_value(&mut self.items, key, item, id);
                }
            }
            for placing in self.placings_of(key) {
                let Some(read) = Orderable::read(placing.type_read(), value) else {
                    continue;
                };
                let slot = (key.clone(), placing);
                if let Some(places) = self.placed.get_mut(&slot) {
                    if let Some(held) = places.get_mut(&read) {
                        held.remove(id);
                        if held.is_empty() {
                            places.remove(&read);
                        }
                    }
                    if places.is_empty() {
                        self.placed.remove(&slot);
                    }
                }
            }
        }
    }

    /// Take off what a thing held and put on what it holds now.
    pub fn replace(
        &mut self,
        id: &EntityId,
        old: Option<&BTreeMap<String, String>>,
        new: &BTreeMap<String, String>,
    ) {
        if let Some(old) = old {
            self.remove(id, old);
        }
        self.add(id, new);
    }

    fn placings_of(&self, key: &str) -> Vec<Placing> {
        [Placing::Date, Placing::Number]
            .into_iter()
            .filter(|placing| self.declared.contains(&(key.to_string(), *placing)))
            .collect()
    }

    /// **The things a leaf keeps**, or `None` when the leaf asks something these
    /// maps do not hold: an ordering of a key no type declares as that type.
    /// Whoever asks reads the documents then.
    pub fn holding(&self, key: &str, test: &Test) -> Option<Holders> {
        let nobody = Holders::new();
        let carrying = self.carriers.get(key).unwrap_or(&nobody);
        let with = |value: &str| -> &Holders {
            self.whole
                .get(key)
                .and_then(|values| values.get(value.trim()))
                .unwrap_or(&nobody)
        };
        Some(match test {
            Test::Exists => carrying.clone(),
            Test::Equals(wanted) => with(wanted).clone(),
            Test::In(wanted) => wanted.iter().flat_map(|one| with(one)).cloned().collect(),
            Test::NotEquals(wanted) => carrying.difference(with(wanted)).cloned().collect(),
            Test::NotIn(wanted) => {
                let equal: Holders = wanted.iter().flat_map(|one| with(one)).cloned().collect();
                carrying.difference(&equal).cloned().collect()
            }
            Test::Contains(item) => {
                let item = item.trim();
                if item.is_empty() || item.contains(',') {
                    Holders::new()
                } else {
                    let listed = self
                        .items
                        .get(key)
                        .and_then(|items| items.get(item))
                        .unwrap_or(&nobody);
                    with(item).union(listed).cloned().collect()
                }
            }
            Test::Order(op, holds, wanted) => {
                let placing = Placing::of(*holds)?;
                if !self.declared.contains(&(key.to_string(), placing)) {
                    return None;
                }
                let Some(bound) = Orderable::read(*holds, wanted) else {
                    return Some(Holders::new());
                };
                let Some(places) = self.placed.get(&(key.to_string(), placing)) else {
                    return Some(Holders::new());
                };
                let range = match op {
                    Op::Less => (Bound::Unbounded, Bound::Excluded(bound)),
                    Op::LessOrEqual => (Bound::Unbounded, Bound::Included(bound)),
                    Op::Greater => (Bound::Excluded(bound), Bound::Unbounded),
                    Op::GreaterOrEqual => (Bound::Included(bound), Bound::Unbounded),
                };
                places
                    .range(range)
                    .flat_map(|(_, held)| held.iter())
                    .cloned()
                    .collect()
            }
        })
    }

    /// **The front of the order of a key**, or `None` when no type declares the
    /// key as `holds`. `counts` says whether a thing is one to place, so a thing
    /// left out of the head is not counted as standing behind it. The groups are
    /// taken whole: a tie is never cut.
    pub fn front(
        &self,
        key: &str,
        holds: ValueType,
        direction: Direction,
        k: usize,
        counts: impl Fn(&EntityId) -> bool,
    ) -> Option<Front> {
        let placing = Placing::of(holds)?;
        if !self.declared.contains(&(key.to_string(), placing)) {
            return None;
        }
        let none = ByPlace::new();
        let places = self
            .placed
            .get(&(key.to_string(), placing))
            .unwrap_or(&none);
        let mut groups: Box<dyn Iterator<Item = (&Orderable, &Holders)>> = match direction {
            Direction::Ascending => Box::new(places.iter()),
            Direction::Descending => Box::new(places.iter().rev()),
        };
        let mut placed: Vec<(EntityId, Orderable)> = Vec::new();
        let mut cut = false;
        // A number with no end has no place in an ordering of things.
        let in_order = |read: &Orderable| !matches!(read, Orderable::Number(n) if !n.is_finite());
        for (read, held) in &mut groups {
            if !in_order(read) {
                continue;
            }
            let counted: Vec<&EntityId> = held.iter().filter(|id| counts(id)).collect();
            if counted.is_empty() {
                continue;
            }
            if placed.len() >= k {
                cut = true;
                break;
            }
            placed.extend(counted.into_iter().map(|id| (id.clone(), *read)));
        }
        Some(Front { placed, cut })
    }

    /// **Whether these maps are what a build over these documents makes.** For the
    /// case that holds the incremental upkeep to the full scan.
    #[cfg(test)]
    pub fn is_what_a_build_would_make<'a>(
        &self,
        types: &[DeclaredType],
        documents: impl Iterator<Item = (&'a EntityId, &'a BTreeMap<String, String>)>,
    ) -> Result<(), String> {
        let mut built = Held::declaring(types);
        for (id, fields) in documents {
            built.add(id, fields);
        }
        let same = |name: &str, held: bool| -> Result<(), String> {
            if held {
                Ok(())
            } else {
                Err(format!("the {name} kept by upkeep differ from a build"))
            }
        };
        same("declared places", self.declared == built.declared)?;
        same("carriers", self.carriers == built.carriers)?;
        same("values", self.whole == built.whole)?;
        same("items", self.items == built.items)?;
        same("places", self.placed == built.placed)
    }
}

fn insert_value(map: &mut ByValue, key: &str, value: &str, id: &EntityId) {
    map.entry(key.to_string())
        .or_default()
        .entry(value.to_string())
        .or_default()
        .insert(id.clone());
}

fn remove_value(map: &mut ByValue, key: &str, value: &str, id: &EntityId) {
    let Some(values) = map.get_mut(key) else {
        return;
    };
    if let Some(held) = values.get_mut(value) {
        held.remove(id);
        if held.is_empty() {
            values.remove(value);
        }
    }
    if values.is_empty() {
        map.remove(key);
    }
}
