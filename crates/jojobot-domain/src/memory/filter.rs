//! **A filter tree, and an ordering.**
//!
//! A question about what things hold is a tree: leaves test one key, and `All`,
//! `Any` and `Not` combine them. The leaves are the questions a key can be asked:
//! is it there, is it that value, is it not that value, is it one of these, is it
//! none of these, does a list value hold this item, and does it stand in one of
//! the four orderings.
//!
//! **One scope for the whole tree, never one per leaf.** The tree is evaluated
//! against a bag of fields and does not know which bag. Handed what a thing
//! HOLDS (its folded fields) it answers a question about the thing; handed one
//! record's fields it answers a question about that record
//! ([`Filter::held_by_a_record`]). A list of key filters that mixes the two
//! scopes, as recall's always could, compiles to two trees: the thing's leaves,
//! and one group of record leaves that one record has to answer. A mixed list
//! then means exactly what it meant.
//!
//! **Licensing.** Declarations license every operator except equality, any-of
//! and exists. `contains` needs a key declared a LIST. An ordering needs a key
//! declared a date or a number. A leaf asking for one of these on a key no
//! declaration licenses is an error that names the declaration which would
//! license it ([`Filter::validate`]), and the evaluator never answers such a leaf
//! with equality: an ordering that quietly became equality would read as an
//! ordering. **When a selection names a kind, that kind's declaration governs.**
//! Without one, a declaration by any type licenses.
//!
//! **Negation.** `!=` and `not in` ask that the key is held, as a key filter
//! always has. [`Filter::Not`] is the complement over the selected population, so
//! it includes the things that lack the key: `Not(status = done)` keeps a thing
//! with no status, and `status != done` does not.
//!
//! **One hop over a reference list.** [`Filter::Refs`] asks a question of the
//! things a list key names: every one of them, or at least one. The question is
//! itself a filter, evaluated on each named thing and not on the holder, with a
//! [`Lookup`] supplying the things. [`Filter::Finished`] asks whether the thing
//! it is evaluated on is finished, by the same function the owed-and-late read
//! uses. A handle the lookup cannot find fails whatever is asked of it, so a
//! dangling reference never reads as satisfied.
//!
//! **Not a string parser and not a store push-down.** The tree is built in code.
//! Reading a tree out of an argument, and answering a tree from an index, are
//! other work.

pub mod expression;

use std::collections::BTreeMap;

use super::types::{Compare, DeclaredType, ValueType, list_holds, list_items};
use super::{MemoryError, kinds};

/// **One filter, or a combination of filters.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    /// Every one holds. **An empty list holds**: asking nothing is not the same
    /// as asking for a key nobody wrote.
    All(Vec<Filter>),
    /// At least one holds. **An empty list does not hold**: there is nothing for
    /// it to be satisfied by.
    Any(Vec<Filter>),
    /// The inner filter does not hold, over everything the filter is asked of,
    /// including what lacks the key.
    Not(Box<Filter>),
    /// A test of one key.
    Leaf(Leaf),
    /// **A question asked of the things a reference list names**, one hop. See
    /// [`Quantifier`] for what "holds" means over the list.
    Refs {
        /// The key whose value is a list of handles.
        key: String,
        /// Whether every named thing must answer, or at least one.
        quantifier: Quantifier,
        /// The question each named thing is asked.
        then: Box<Filter>,
    },
    /// **The thing this is evaluated on is finished**, by
    /// [`kinds::is_finished`] with the instance's own status words. An archived
    /// thing is not finished, whatever it holds.
    Finished,
}

/// **How a [`Filter::Refs`] reads its list.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quantifier {
    /// Every named thing answers. **An empty list holds**: there is nothing for
    /// it to fail on, as for an empty [`Filter::All`].
    All,
    /// At least one named thing answers. An empty list does not hold.
    Any,
}

/// **A thing as a filter sees it**: what kind it is, what it holds, and whether
/// it was archived.
#[derive(Debug, Clone, Copy)]
pub struct Subject<'a> {
    /// The kind token, for the questions that depend on it.
    pub kind: &'a str,
    /// The thing's folded fields.
    pub fields: &'a BTreeMap<String, String>,
    /// Whether the thing is archived.
    pub archived: bool,
}

/// **The things a reference list can name, and the instance's own record.**
pub trait Lookup {
    /// The thing this handle names now, or `None` when it names nothing.
    fn thing(&self, handle: &str) -> Option<Subject<'_>>;
    /// The instance's own fields: what its status words mean.
    fn instance(&self) -> &BTreeMap<String, String>;
}

/// **A lookup that finds nothing.** What [`Filter::holds`] evaluates with, so a
/// tree that needs other things answers as if none exist.
struct Nothing(BTreeMap<String, String>);

impl Lookup for Nothing {
    fn thing(&self, _handle: &str) -> Option<Subject<'_>> {
        None
    }
    fn instance(&self) -> &BTreeMap<String, String> {
        &self.0
    }
}

/// **A test of one key's value.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leaf {
    /// The key, exactly as it is spelled. **`None` asks about the VALUE under
    /// any key at all**: the test holds when some value in the bag passes it. A
    /// test that is about the key ([`Test::Exists`]) or that needs the key held
    /// ([`Test::NotEquals`], [`Test::NotIn`]) needs one.
    pub key: Option<String>,
    /// What is asked of the value.
    pub test: Test,
}

/// **Which of the four orderings.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    /// Smaller than.
    Less,
    /// Smaller than or equal to.
    LessOrEqual,
    /// Larger than.
    Greater,
    /// Larger than or equal to.
    GreaterOrEqual,
}

/// **What a leaf asks of a value.**
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Test {
    /// The key is carried at all.
    Exists,
    /// The value is that value, whole and trimmed.
    Equals(String),
    /// The key is held and its value is not that value.
    NotEquals(String),
    /// The value is one of these values, each whole and trimmed.
    In(Vec<String>),
    /// The key is held and its value is none of these values.
    NotIn(Vec<String>),
    /// The value is a list and holds this item. **Whole items only**: `skills` is
    /// not in `essay.rs, skills.rs`. How the list is read apart is behind
    /// [`list_holds`], and the tree does not know it.
    Contains(String),
    /// The value stands in this order to the one given, read as a date or as a
    /// number. Only those two value types are orderable.
    Order(Op, ValueType, String),
}

impl Op {
    /// The word this ordering is spoken as in a refusal. The four an existing
    /// recall can ask keep the words recall already uses.
    fn token(self, holds: ValueType) -> &'static str {
        match (self, holds) {
            (Op::Less, ValueType::Date) => "before",
            (Op::Greater, ValueType::Date) => "after",
            (Op::Less, _) => "less",
            (Op::Greater, _) => "greater",
            (Op::LessOrEqual, _) => "at most",
            (Op::GreaterOrEqual, _) => "at least",
        }
    }
}

impl Op {
    /// The symbol an expression writes this ordering as.
    fn symbol(self) -> &'static str {
        match self {
            Op::Less => "<",
            Op::LessOrEqual => "<=",
            Op::Greater => ">",
            Op::GreaterOrEqual => ">=",
        }
    }

    /// **The ordering as a refusal names it**: the word a key filter uses and the
    /// symbol an expression uses, so a caller who wrote either recognises it.
    fn said(self, holds: ValueType) -> String {
        format!("'{}' ({})", self.token(holds), self.symbol())
    }
}

impl Filter {
    /// A leaf asking that the key is carried.
    pub fn exists(key: &str) -> Filter {
        Filter::leaf(Some(key), Test::Exists)
    }

    /// A leaf asking that the key holds this value.
    pub fn equals(key: &str, wanted: &str) -> Filter {
        Filter::leaf(Some(key), Test::Equals(wanted.to_string()))
    }

    /// A leaf asking that the key is held and holds something else.
    pub fn not_equals(key: &str, wanted: &str) -> Filter {
        Filter::leaf(Some(key), Test::NotEquals(wanted.to_string()))
    }

    /// A leaf asking that the key holds one of these values.
    pub fn is_in(key: &str, wanted: &[&str]) -> Filter {
        Filter::leaf(Some(key), Test::In(owned(wanted)))
    }

    /// A leaf asking that the key is held and holds none of these values.
    pub fn not_in(key: &str, wanted: &[&str]) -> Filter {
        Filter::leaf(Some(key), Test::NotIn(owned(wanted)))
    }

    /// A leaf asking that the key's list holds this item.
    pub fn contains(key: &str, item: &str) -> Filter {
        Filter::leaf(Some(key), Test::Contains(item.to_string()))
    }

    /// A leaf asking that the key's value stands in this order to the one given,
    /// read as `holds`.
    pub fn ordering(key: &str, op: Op, holds: ValueType, wanted: &str) -> Filter {
        Filter::leaf(Some(key), Test::Order(op, holds, wanted.to_string()))
    }

    /// A leaf asking that the key's value stands in the relation `compare`
    /// names to the one given. `Equals` is equality and needs no declaration.
    pub fn comparing(key: &str, compare: Compare, wanted: &str) -> Filter {
        Filter::leaf(Some(key), Test::of(compare, wanted))
    }

    /// A leaf asking whether SOME value in the bag passes the test, whatever key
    /// it sits under.
    pub fn anywhere(test: Test) -> Filter {
        Filter::leaf(None, test)
    }

    /// A leaf from its parts.
    pub fn leaf(key: Option<&str>, test: Test) -> Filter {
        Filter::Leaf(Leaf {
            key: key.map(|key| key.trim().to_string()),
            test,
        })
    }

    /// The complement of a filter.
    pub fn negated(inner: Filter) -> Filter {
        Filter::Not(Box::new(inner))
    }

    /// **Does this bag of fields satisfy the filter.** It is handed the thing's
    /// folded fields or one record's, and does not know which.
    ///
    /// **It knows no other thing**, so a [`Filter::Refs`] over a list with an
    /// item finds nothing there and fails, and [`Filter::Finished`] has no kind
    /// to ask about. [`Filter::holds_in`] is the question with a lookup.
    pub fn holds(&self, fields: &BTreeMap<String, String>) -> bool {
        let subject = Subject {
            kind: "",
            fields,
            archived: false,
        };
        self.holds_in(&subject, &Nothing(BTreeMap::new()))
    }

    /// **Does this thing satisfy the filter, given the things a reference list
    /// can name.**
    pub fn holds_in(&self, subject: &Subject<'_>, lookup: &dyn Lookup) -> bool {
        match self {
            Filter::All(all) => all.iter().all(|filter| filter.holds_in(subject, lookup)),
            Filter::Any(any) => any.iter().any(|filter| filter.holds_in(subject, lookup)),
            Filter::Not(inner) => !inner.holds_in(subject, lookup),
            Filter::Leaf(leaf) => leaf.holds(subject.fields),
            Filter::Refs {
                key,
                quantifier,
                then,
            } => {
                let held = subject.fields.get(key).map_or("", String::as_str);
                // A handle the lookup cannot find fails whatever is asked of it.
                let answers = |handle: &&str| {
                    lookup
                        .thing(handle)
                        .is_some_and(|named| then.holds_in(&named, lookup))
                };
                let handles = list_items(held);
                match quantifier {
                    Quantifier::All => handles.iter().all(answers),
                    Quantifier::Any => handles.iter().any(answers),
                }
            }
            Filter::Finished => {
                !subject.archived
                    && kinds::is_finished(subject.kind, subject.fields, lookup.instance())
            }
        }
    }

    /// **Does one of these records satisfy the whole filter.** A question asked
    /// of records is answered by ONE record: two records each satisfying half of
    /// an `All` do not satisfy it.
    pub fn held_by_a_record<'a>(
        &self,
        mut records: impl Iterator<Item = &'a BTreeMap<String, String>>,
    ) -> bool {
        records.any(|record| self.holds(record))
    }

    /// **The keys the leaves name**, in the order they are met. A leaf that
    /// asks about the value under any key names none.
    pub fn keys(&self) -> Vec<&str> {
        match self {
            Filter::All(children) | Filter::Any(children) => {
                children.iter().flat_map(Filter::keys).collect()
            }
            Filter::Not(inner) => inner.keys(),
            Filter::Leaf(leaf) => leaf.key.as_deref().into_iter().collect(),
            // The list key is this thing's; the inner question's keys belong to
            // the things the list names, so they are not this thing's to hold.
            Filter::Refs { key, .. } => vec![key.as_str()],
            Filter::Finished => Vec::new(),
        }
    }

    /// **Refuse a filter that cannot be answered as asked, before any read.**
    ///
    /// `kind` is the kind the selection names, when it names one: that kind's
    /// declaration alone governs. With none, a declaration by any type licenses.
    /// A key nobody declared keeps equality, any-of and exists and nothing else,
    /// which is why this refuses rather than answers.
    pub fn validate(
        &self,
        declared: &[DeclaredType],
        kind: Option<&str>,
    ) -> Result<(), MemoryError> {
        match self {
            Filter::All(children) | Filter::Any(children) => children
                .iter()
                .try_for_each(|filter| filter.validate(declared, kind)),
            Filter::Not(inner) => inner.validate(declared, kind),
            Filter::Leaf(leaf) => leaf.validate(declared, kind),
            Filter::Refs { .. } | Filter::Finished => Ok(()),
        }
    }
}

fn owned(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| value.to_string()).collect()
}

impl Test {
    /// The test a comparison and a value ask: equality for `Equals`, the
    /// ordering for the four an existing recall can ask.
    pub fn of(compare: Compare, wanted: &str) -> Test {
        let order = |op, holds| Test::Order(op, holds, wanted.to_string());
        match compare {
            Compare::Equals => Test::Equals(wanted.to_string()),
            Compare::Before => order(Op::Less, ValueType::Date),
            Compare::After => order(Op::Greater, ValueType::Date),
            Compare::Less => order(Op::Less, ValueType::Number),
            Compare::Greater => order(Op::Greater, ValueType::Number),
        }
    }

    /// Whether one HELD value passes the test. [`Test::Exists`] is about the key
    /// and is decided by the leaf, so a value passes it.
    fn passes(&self, held: &str) -> bool {
        let equal = |one: &str| Compare::Equals.holds_between(held, one);
        match self {
            Test::Exists => true,
            Test::Equals(wanted) => equal(wanted),
            Test::NotEquals(wanted) => !equal(wanted),
            Test::In(wanted) => wanted.iter().any(|one| equal(one)),
            Test::NotIn(wanted) => !wanted.iter().any(|one| equal(one)),
            Test::Contains(item) => list_holds(held, item),
            Test::Order(op, holds, wanted) => ordered(*op, *holds, held, wanted),
        }
    }

    /// Whether the test asks that the key is held and so cannot be answered by
    /// a key that is absent.
    fn needs_the_key_held(&self) -> bool {
        matches!(self, Test::NotEquals(_) | Test::NotIn(_))
    }
}

/// **Does `held` stand in this order to `wanted`, both read as `holds`.** A held
/// value that is no value of that type does not match. It is not an error: the
/// record is messy.
fn ordered(op: Op, holds: ValueType, held: &str, wanted: &str) -> bool {
    use std::cmp::Ordering;
    let order = match holds {
        ValueType::Date => match (
            held.trim().parse::<jiff::civil::Date>(),
            wanted.trim().parse::<jiff::civil::Date>(),
        ) {
            (Ok(held), Ok(wanted)) => held.cmp(&wanted),
            _ => return false,
        },
        ValueType::Number => match (held.trim().parse::<f64>(), wanted.trim().parse::<f64>()) {
            (Ok(held), Ok(wanted)) => match held.partial_cmp(&wanted) {
                Some(order) => order,
                None => return false,
            },
            _ => return false,
        },
        // Only a date and a number are orderable; validation refuses the rest.
        _ => return false,
    };
    match op {
        Op::Less => order == Ordering::Less,
        Op::LessOrEqual => order != Ordering::Greater,
        Op::Greater => order == Ordering::Greater,
        Op::GreaterOrEqual => order != Ordering::Less,
    }
}

/// **Whether a declaration licenses `key` to hold what a leaf needs.**
///
/// With no `kind`, any declaration does. With one, the key is asked of that
/// kind: **a declaration named like a kind belongs to that kind**, so one kind's
/// `due` never licenses another's. If the kind declares the key, its word is the
/// answer, even against a type that says otherwise. If it does not, a type under
/// any name that is no kind is vocabulary and licenses any object carrying the
/// key.
fn declares(
    declared: &[DeclaredType],
    kind: Option<&str>,
    key: &str,
    accepts: impl Fn(&super::types::Field) -> bool,
) -> bool {
    let Some(kind) = kind else {
        return declared.iter().any(|d| d.field(key).is_some_and(&accepts));
    };
    if let Some(field) = declared
        .iter()
        .find(|d| d.name == kind)
        .and_then(|d| d.field(key))
    {
        return accepts(field);
    }
    declared
        .iter()
        .filter(|d| !super::kinds::known(&d.name))
        .any(|d| d.field(key).is_some_and(&accepts))
}

impl Leaf {
    fn holds(&self, fields: &BTreeMap<String, String>) -> bool {
        match (&self.key, &self.test) {
            (Some(key), Test::Exists) => fields.contains_key(key),
            (Some(key), test) => fields.get(key).is_some_and(|held| test.passes(held)),
            // **Any key at all.** The bag is the thing's folded fields or one
            // record's and never its prose, so this answers *held as a value*
            // rather than *written down somewhere*.
            (None, Test::Exists) => false,
            (None, test) if test.needs_the_key_held() => false,
            (None, test) => fields.values().any(|held| test.passes(held)),
        }
    }

    fn validate(&self, declared: &[DeclaredType], kind: Option<&str>) -> Result<(), MemoryError> {
        let Some(key) = self.key.as_deref() else {
            return match &self.test {
                Test::Exists => Err(MemoryError::InvalidQuery(
                    "asking that a key is there needs the key. Name it, or ask for a value".into(),
                )),
                Test::NotEquals(_) | Test::NotIn(_) => Err(MemoryError::InvalidQuery(
                    "'not' asks that a key is held and holds something else, so it needs the \
                     key. Name it, or negate the whole question"
                        .into(),
                )),
                Test::Contains(_) => Err(MemoryError::InvalidQuery(
                    "'contains' needs a key: a list is declared on a key, and a filter naming \
                     no key has no declaration to read. Name the key"
                        .into(),
                )),
                Test::Order(op, holds, _) => Err(MemoryError::InvalidQuery(format!(
                    "{} needs a key: an ordering is licensed by what that key was declared \
                     to hold, and a filter naming no key has no declaration to read. Name the \
                     key, or ask for the value itself",
                    op.said(*holds),
                ))),
                Test::Equals(_) | Test::In(_) => Ok(()),
            };
        };
        if key.trim().is_empty() {
            return Err(MemoryError::InvalidQuery(
                "a key filter names no key".into(),
            ));
        }
        match &self.test {
            Test::Contains(_) if !declares(declared, kind, key, |field| field.list) => {
                return Err(MemoryError::InvalidQuery(format!(
                    "'contains' needs a type declaring '{key}' to hold a list. Declare one, \
                     or ask for the value itself",
                )));
            }
            Test::Order(op, holds, wanted) => {
                if !matches!(holds, ValueType::Date | ValueType::Number) {
                    return Err(MemoryError::InvalidQuery(format!(
                        "{} orders dates and numbers, and '{}' is neither",
                        op.said(*holds),
                        holds.as_token(),
                    )));
                }
                if !holds.holds(wanted) {
                    return Err(MemoryError::InvalidQuery(format!(
                        "{} asks about {}s and '{wanted}' is not one",
                        op.said(*holds),
                        holds.as_token(),
                    )));
                }
                if !declares(declared, kind, key, |field| field.holds == *holds) {
                    return Err(MemoryError::InvalidQuery(format!(
                        "{} needs a type declaring '{}' to hold a {}. Declare one, or ask for \
                         the value itself",
                        op.said(*holds),
                        key,
                        holds.as_token(),
                    )));
                }
            }
            _ => {}
        }
        Ok(())
    }
}

/// **Which way an ordering runs.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Smallest first.
    Ascending,
    /// Largest first.
    Descending,
}

/// **An ordering: a key and a direction.**
///
/// A key declared to hold a date or a number orders as one, so `2` comes before
/// `10`. **Any other key orders as text**, so `10` comes before `2`. Things that
/// do not hold the key come last in either direction, and so do things whose
/// value is not what the key was declared to hold: neither has a place in the
/// order. **Ties keep the order the items came in**, which is the order recall
/// uses today.
///
/// **It is applied to the whole selected population before any cut**, so a page
/// of an answer is a page of the order and never an order of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Order {
    /// The key the things are ordered by.
    pub key: String,
    /// Smallest first or largest first.
    pub direction: Direction,
}

/// **What an ordering did, for the answer to say.**
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ordered {
    /// **The ordering read the key as text although a held value would have
    /// parsed as a number or a date.** The key was not declared one or the other,
    /// so `10` sorts before `2`, and the answer owes the caller one line saying
    /// so. False when the key was declared text (the author said text), and
    /// false when no held value looks like a number or a date, so a key holding
    /// only words is ordered as text with no note.
    pub read_as_text_though_it_looks_otherwise: bool,
}

/// What a value is read as when it is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReadAs {
    Text,
    Number,
    Date,
}

/// A value, read.
#[derive(Debug, Clone, PartialEq)]
enum Sortable {
    Text(String),
    Number(f64),
    Date(jiff::civil::Date),
}

impl Order {
    /// An ordering by this key in this direction.
    pub fn new(key: &str, direction: Direction) -> Order {
        Order {
            key: key.trim().to_string(),
            direction,
        }
    }

    /// **An ordering from the words a caller sends**: a key, and `ascending` or
    /// `descending` (`asc` and `desc` too). No direction is ascending.
    pub fn asked(key: &str, direction: Option<&str>) -> Result<Order, MemoryError> {
        if key.trim().is_empty() {
            return Err(MemoryError::InvalidQuery(
                "an order names no key. Name the key the things are ordered by".into(),
            ));
        }
        let direction = match direction.map(str::trim).filter(|d| !d.is_empty()) {
            None | Some("ascending" | "asc") => Direction::Ascending,
            Some("descending" | "desc") => Direction::Descending,
            Some(other) => {
                return Err(MemoryError::InvalidQuery(format!(
                    "an order runs ascending or descending, and '{other}' is neither. Leave the \
                     direction out for ascending"
                )));
            }
        };
        Ok(Order::new(key, direction))
    }

    /// **An ordering written as one line**, `due` or `due descending`, the way a
    /// view holds it: the key, and a direction word when the last word is one.
    pub fn in_words(words: &str) -> Result<Order, MemoryError> {
        let words = words.trim();
        match words.rsplit_once(char::is_whitespace) {
            Some((key, direction))
                if matches!(direction, "asc" | "ascending" | "desc" | "descending") =>
            {
                Order::asked(key, Some(direction))
            }
            _ => Order::asked(words, None),
        }
    }

    /// **What the declarations say this key holds**, as far as an ordering
    /// cares: a date, a number, or neither (and so text). A key two types declare
    /// as a date and as a number reads as neither: no one reading is the
    /// declared one.
    fn read_as(&self, declared: &[DeclaredType], kind: Option<&str>) -> ReadAs {
        let says =
            |holds: ValueType| declares(declared, kind, &self.key, |field| field.holds == holds);
        match (says(ValueType::Date), says(ValueType::Number)) {
            (true, false) => ReadAs::Date,
            (false, true) => ReadAs::Number,
            _ => ReadAs::Text,
        }
    }

    fn read(held: &str, read_as: ReadAs) -> Option<Sortable> {
        let held = held.trim();
        match read_as {
            ReadAs::Text => Some(Sortable::Text(held.to_string())),
            ReadAs::Number => held
                .parse::<f64>()
                .ok()
                .filter(|number| number.is_finite())
                .map(Sortable::Number),
            ReadAs::Date => held.parse::<jiff::civil::Date>().ok().map(Sortable::Date),
        }
    }

    /// **Put the items in order.** `fields_of` gives the bag an item is ordered
    /// by, or nothing for an item with no fields. `kind` is the kind the
    /// selection names, as for [`Filter::validate`].
    pub fn sort<T>(
        &self,
        items: &mut [T],
        fields_of: impl Fn(&T) -> Option<&BTreeMap<String, String>>,
        declared: &[DeclaredType],
        kind: Option<&str>,
    ) -> Ordered {
        use std::cmp::Ordering;
        let read_as = self.read_as(declared, kind);
        fn held<'a, T>(
            fields_of: &impl Fn(&T) -> Option<&BTreeMap<String, String>>,
            item: &'a T,
            key: &str,
        ) -> Option<&'a String> {
            fields_of(item).and_then(|fields| fields.get(key))
        }
        let value = |item: &T| {
            held(&fields_of, item, &self.key).and_then(|held| Order::read(held, read_as))
        };
        let said_text = declares(declared, kind, &self.key, |field| {
            field.holds == ValueType::Text
        });
        let looks_otherwise = read_as == ReadAs::Text
            && !said_text
            && items.iter().any(|item| {
                held(&fields_of, item, &self.key).is_some_and(|held| {
                    Order::read(held, ReadAs::Number).is_some()
                        || Order::read(held, ReadAs::Date).is_some()
                })
            });
        // A stable sort, so ties keep the order the items came in.
        items.sort_by(|a, b| match (value(a), value(b)) {
            (Some(a), Some(b)) => {
                let ascending = match (&a, &b) {
                    (Sortable::Number(a), Sortable::Number(b)) => {
                        a.partial_cmp(b).unwrap_or(Ordering::Equal)
                    }
                    (Sortable::Date(a), Sortable::Date(b)) => a.cmp(b),
                    (Sortable::Text(a), Sortable::Text(b)) => a.cmp(b),
                    // One key is read one way, so a pair never mixes.
                    _ => Ordering::Equal,
                };
                match self.direction {
                    Direction::Ascending => ascending,
                    Direction::Descending => ascending.reverse(),
                }
            }
            (Some(_), None) => Ordering::Less,
            (None, Some(_)) => Ordering::Greater,
            (None, None) => Ordering::Equal,
        });
        Ordered {
            read_as_text_though_it_looks_otherwise: looks_otherwise,
        }
    }
}

#[cfg(test)]
mod tests;
