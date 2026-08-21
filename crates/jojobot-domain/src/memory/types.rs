//! **A type is a named set of keys, and matching it is STRUCTURAL.**
//!
//! A record carrying the right keys is found by a query for a type whether or
//! not anybody declared it. **Declaring a type is write-time help** — it tells a
//! writer which keys to fill and what belongs in them — and it is never a
//! precondition for a record being findable. That is the whole shape: a
//! declaration describes, it does not admit.
//!
//! **Kind is identity; type is interface.** An entity has one permanent kind
//! baked into its handle, and it may satisfy several types. Nothing here is a
//! kind, nothing here changes what a thing IS, and a type that ended up as a
//! kind would have collapsed the two.
//!
//! **Nothing is hidden.** A record matching some of a type's keys comes back,
//! saying which ones it lacks. A value that does not hold what its key was
//! declared to hold comes back too, flagged. Life is messy and the record is
//! the truth; a read that dropped messy records would hide the ones most worth
//! finding.
//!
//! **A THING that already carries every key of a type is held to it on the way
//! in.** Matching stays structural and reading refuses nothing, but a write
//! that would leave such a thing without a key — by taking it away, or by
//! putting a value in it that the key does not hold — is refused, naming the
//! key and what it wanted. What a type asks for is a FLOOR: a thing carrying
//! only some of the keys has no fit to protect and stays free to be messy, and
//! a key no type mentions is welcome anywhere.
//!
//! **No key is registered anywhere.** Keys are scoped by the type that names
//! them, so there is no global list of legal keys and nothing to maintain. Two
//! types may use one key name and mean their own thing by it.
//!
//! **A type knows where it came from, and that is the one thing a caller
//! cannot write over.** The software ships some types and callers declare the
//! rest. A shipped type is closed: a caller can neither extend it, shrink it
//! nor replace it, and changing one is a change to the software. A caller's
//! own types stay entirely theirs. That is the whole of what [`Origin`] buys,
//! and the protection reads it rather than a list of names.

use std::collections::BTreeMap;

use super::{EntityId, EntityKind, MemoryError};

/// **What a key is declared to hold.**
///
/// The smallest set that makes filtering, sorting and joining real. A list is
/// deliberately not here: it is a shape, not a value, and nothing needs one yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ValueType {
    /// Anything at all. The default, and the honest answer for prose.
    Text,
    /// A number to sort or compare by.
    Number,
    /// A civil date, in the one spelling this project uses everywhere else.
    Date,
    /// **A span, as ONE value**: two civil dates with a slash between them,
    /// `2026-04-18/2026-04-25`.
    ///
    /// One key rather than two, because two keys cannot say they belong
    /// together — a thing carrying a start and no end reads as a thing with a
    /// key missing rather than as a span nobody finished, and every question
    /// about the span has to know both key names. A date carries no slash, so
    /// the spelling cannot be read two ways.
    DateRange,
    /// Yes or no.
    Boolean,
    /// **Another entity, and a walkable edge rather than a string that looks
    /// like one.** This is what makes a cross-entity question answerable from a
    /// type's own fields.
    Reference,
}

/// **How a key's writes come down to the one value the key holds.**
///
/// A thing's fields are its writes folded together, and until now every key
/// folded the one way: the newest write wins. That leaves a running total to
/// the agent, which has to read the history, add it up and write the answer
/// back — so the arithmetic lives in whichever session last touched the key
/// rather than in the software, and two sessions can disagree about it.
///
/// **It is a property of the KEY, declared once, and never of a write.** On the
/// write, two callers can disagree about the same key — one adding, one
/// replacing — and the value quietly means two things with nothing to say which.
/// A key IS a counter or it is not.
///
/// **This is not an aggregation language and must not become one.** There is
/// one fold beyond the default because there is one case for one, and the next
/// belongs here when a second real case arrives (rule 106).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Fold {
    /// The newest write wins. **The default, and it needs no declaration**
    /// (rule 9): a key nobody has declared folds this way, which is how every
    /// key folded before there was a choice.
    #[default]
    Newest,
    /// **A counter: the writes add up.** Three writes of one read back as
    /// three, and how many times the key was written is still the substrate's
    /// own answer — the projection changes, what is stored does not.
    Sum,
}

impl Fold {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Fold::Newest => "newest",
            Fold::Sum => "sum",
        }
    }

    /// The fold a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Fold> {
        match token.trim() {
            "newest" => Some(Fold::Newest),
            "sum" => Some(Fold::Sum),
            _ => None,
        }
    }
}

/// **How a key folds, over everything that has been declared.**
///
/// Every declaration is asked, rather than the first one owning the name, for
/// the reason [`super::graph`] asks them all about a relation: two types may
/// name one key and mean their own thing by it, and which of them a store hands
/// over first is its own ordering rather than anything a caller said. A key any
/// type declares a counter sums.
pub fn fold_of(key: &str, declared: &[DeclaredType]) -> Fold {
    let key = key.trim();
    if declared
        .iter()
        .any(|d| d.field(key).is_some_and(|f| f.folds == Fold::Sum))
    {
        Fold::Sum
    } else {
        Fold::Newest
    }
}

/// **How a filter compares a record's value with the one it is looking for.**
///
/// Not an expression language and not an operator set a caller composes: each
/// of these is licensed by what a key was DECLARED to hold, and a key with no
/// declaration behind it has equality and nothing else. That is the whole
/// mechanism keeping this from growing into a predicate builder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Compare {
    /// The value is that value. Whole and trimmed, never a substring.
    #[default]
    Equals,
    /// Earlier than that date.
    Before,
    /// Later than that date.
    After,
    /// Smaller than that number.
    Less,
    /// Larger than that number.
    Greater,
}

impl Compare {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Compare::Equals => "equals",
            Compare::Before => "before",
            Compare::After => "after",
            Compare::Less => "less",
            Compare::Greater => "greater",
        }
    }

    /// The comparison a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Compare> {
        match token.trim() {
            "equals" => Some(Compare::Equals),
            "before" => Some(Compare::Before),
            "after" => Some(Compare::After),
            "less" => Some(Compare::Less),
            "greater" => Some(Compare::Greater),
            _ => None,
        }
    }

    /// **What a key must be declared to hold for this comparison to be
    /// licensed**, or nothing for the one that needs no declaration.
    pub fn licensed_by(self) -> Option<ValueType> {
        match self {
            Compare::Equals => None,
            Compare::Before | Compare::After => Some(ValueType::Date),
            Compare::Less | Compare::Greater => Some(ValueType::Number),
        }
    }

    /// **Does the value a record carries stand in this relation to the value
    /// asked for.**
    ///
    /// A record value that does not parse as what the comparison needs does not
    /// match. It is not an error: the record is messy, which the typed path
    /// reports elsewhere and never refuses over.
    pub fn holds_between(self, held: &str, wanted: &str) -> bool {
        let (held, wanted) = (held.trim(), wanted.trim());
        match self {
            Compare::Equals => held == wanted,
            Compare::Before | Compare::After => {
                let (Ok(held), Ok(wanted)) = (
                    held.parse::<jiff::civil::Date>(),
                    wanted.parse::<jiff::civil::Date>(),
                ) else {
                    return false;
                };
                if self == Compare::Before {
                    held < wanted
                } else {
                    held > wanted
                }
            }
            Compare::Less | Compare::Greater => {
                let (Ok(held), Ok(wanted)) = (held.parse::<f64>(), wanted.parse::<f64>()) else {
                    return false;
                };
                if self == Compare::Less {
                    held < wanted
                } else {
                    held > wanted
                }
            }
        }
    }

    /// **Is this a value the comparison can be asked about at all.** The
    /// caller's own half: a date comparison against something that is no date
    /// is a malformed question rather than one with no answers.
    pub fn can_ask_for(self, wanted: &str) -> bool {
        match self.licensed_by() {
            None => true,
            Some(holds) => holds.holds(wanted),
        }
    }
}

impl ValueType {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            ValueType::Text => "text",
            ValueType::Number => "number",
            ValueType::Date => "date",
            ValueType::DateRange => "date_range",
            ValueType::Boolean => "boolean",
            ValueType::Reference => "reference",
        }
    }

    /// The type a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<ValueType> {
        match token.trim() {
            "text" => Some(ValueType::Text),
            "number" => Some(ValueType::Number),
            "date" => Some(ValueType::Date),
            "date_range" => Some(ValueType::DateRange),
            "boolean" => Some(ValueType::Boolean),
            "reference" => Some(ValueType::Reference),
            _ => None,
        }
    }

    /// **Whether a value holds what this key was declared to hold.**
    ///
    /// The value type's own half of the question. [`Field::accepts`] is what a
    /// record is measured against, because a reference may also name the kind
    /// it points at and this cannot see that.
    pub fn holds(self, value: &str) -> bool {
        let value = value.trim();
        match self {
            ValueType::Text => true,
            ValueType::Number => value.parse::<f64>().is_ok(),
            ValueType::Boolean => matches!(value, "true" | "false"),
            ValueType::Date => value.parse::<jiff::civil::Date>().is_ok(),
            // Both ends, in order. A range that ends before it starts is not a
            // span somebody wrote down badly, it is two dates in the wrong
            // slots, and storing it would make every question about the span
            // answer nonsense.
            ValueType::DateRange => match value.split_once('/') {
                Some((from, to)) => {
                    match (
                        from.trim().parse::<jiff::civil::Date>(),
                        to.trim().parse::<jiff::civil::Date>(),
                    ) {
                        (Ok(from), Ok(to)) => from <= to,
                        _ => false,
                    }
                }
                None => false,
            },
            // A handle, which is what makes it walkable. The kind has to be one
            // this store knows, or it is a string with a colon in it.
            ValueType::Reference => EntityId(value.to_string()).kind().is_some(),
        }
    }
}

/// **Where a type came from.**
///
/// Two values, because there are two writers: the software ships a type, or a
/// caller declares one. It is a property of the type and never something a
/// caller states — it is read off how the declaration arrived (rule 9).
///
/// It is what the protection reads. A list of protected names would be an
/// enumeration somebody has to maintain, and it would go stale on the day a
/// type is added to it (rule 106).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum Origin {
    /// The software declared it. **Closed**: a caller cannot replace it, and
    /// changing it is a code change.
    Shipped,
    /// A caller declared it. Theirs to redeclare, replace and reshape.
    #[default]
    Declared,
}

impl Origin {
    /// The token this reads and writes as.
    pub fn as_token(self) -> &'static str {
        match self {
            Origin::Shipped => "shipped",
            Origin::Declared => "declared",
        }
    }

    /// The origin a token names, or nothing when it names none.
    pub fn of_token(token: &str) -> Option<Origin> {
        match token.trim() {
            "shipped" => Some(Origin::Shipped),
            "declared" => Some(Origin::Declared),
            _ => None,
        }
    }
}

/// One key of a type, what it holds, and how its writes fold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub holds: ValueType,
    /// **The kind a reference points at**, when the declaration names one.
    ///
    /// It sits here rather than inside [`ValueType::Reference`] because what a
    /// key points AT is a narrowing of the value type rather than a value type
    /// of its own: everything that asks whether a key is a reference — the
    /// relation walk, the comparison licence — asks about the value type and
    /// wants the same answer for `place:` and for any handle at all.
    ///
    /// `None` is a reference to anything jojobot knows, and it is what a
    /// declaration that says nothing means: naming a kind NARROWS the key, so
    /// the unnarrowed reference stays the default (rule 62).
    pub points_at: Option<EntityKind>,
    /// How the writes of this key come down to one value. [`Fold::Newest`]
    /// unless the declaration says otherwise.
    pub folds: Fold,
    /// **Whether a thing has to hold this key to be one of these at all.**
    ///
    /// The required keys are the minimum that makes a thing that thing, and
    /// they are the only ones fitting is measured against. **An optional key is
    /// welcome and never demanded**: a thing without it is complete, and
    /// nothing about it is missing.
    ///
    /// **Optional is the default** (rule 9), and the reason is not tidiness: a
    /// required key on a KIND is a refusal waiting to happen, so a declaration
    /// says which keys earn one rather than which keys are let off.
    ///
    /// **It says nothing about the VALUE.** The two are independent, and
    /// collapsing them is what turns "the colour has to be a colour" into
    /// "every bike must have a colour".
    ///
    /// ⚠️ **Which of the two a declaration ENFORCES is its owner's question.**
    /// A kind's key is checked whenever it is set and its required set is a
    /// floor a write may not break ([`super::guard_fit`]); a type a caller
    /// declared governs no write at all, and both properties are read by
    /// whoever asks how a thing answers it.
    pub required: bool,
    /// **Whether this key holds a LIST — zero or more of what it declares.**
    ///
    /// Written as its items separated by commas, so `zero or more` includes
    /// zero: an empty value is a list with nothing in it, which is a thing
    /// having none of something rather than a key nobody filled.
    ///
    /// It is a shape on the key rather than a value type of its own, because
    /// every question about the key — what it holds, what kind it points at,
    /// whether a comparison is licensed — has the same answer for one of them
    /// and for many.
    pub list: bool,
    /// **The named set this key holds one of**, when the declaration names one.
    ///
    /// A closed vocabulary: the values are part of the declaration, and a write
    /// outside them is refused. It is what a key wants when the set genuinely
    /// is small, known and finite — and it is not the way to enumerate an
    /// open-ended set, which is a list somebody then has to maintain.
    ///
    /// It sits here rather than inside [`ValueType`] for the reason
    /// [`Field::points_at`] does: naming the values NARROWS what a key holds
    /// rather than saying what kind of thing it holds. Every question about the
    /// key — the comparison licence, the fold, what a list wraps — has the same
    /// answer narrowed and unnarrowed. Keeping it out of the value type is also
    /// what keeps that type a bare copyable enum.
    ///
    /// `None` is a key narrowed to nothing, and it is what a declaration that
    /// says nothing means. `Some` of an empty set is a key nothing can ever be
    /// written to, so [`validate_type`] refuses it rather than the shape making
    /// it unsayable — one rule, in the one place both stores call.
    pub one_of: Option<Vec<String>>,
}

impl Field {
    pub fn new(key: &str, holds: ValueType) -> Field {
        Field {
            key: key.trim().to_string(),
            holds,
            points_at: None,
            folds: Fold::Newest,
            required: false,
            list: false,
            one_of: None,
        }
    }

    /// **A key a thing of this type has to hold.** The ordinary constructor
    /// leaves a key optional, so the required set is what a declaration states
    /// rather than what it forgets to exempt.
    pub fn required(key: &str, holds: ValueType) -> Field {
        Field {
            required: true,
            ..Field::new(key, holds)
        }
    }

    /// **A key holding zero or more of what it declares.**
    pub fn listing(key: &str, holds: ValueType) -> Field {
        Field {
            list: true,
            ..Field::new(key, holds)
        }
    }

    /// The same key, required — for the declarations built through a
    /// constructor that already says something else about the key.
    pub fn needed(self) -> Field {
        Field {
            required: true,
            ..self
        }
    }

    /// **The items a value carries under this key** — the value itself for an
    /// ordinary key, and the comma-separated items for a list.
    ///
    /// An empty list value carries nothing, which is why this can be empty and
    /// why a caller reading it back gets a thing that holds none of something
    /// rather than a key nobody wrote.
    pub fn items<'v>(&self, value: &'v str) -> Vec<&'v str> {
        if !self.list {
            return vec![value];
        }
        value
            .split(',')
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .collect()
    }

    /// **A counter.** It holds a number because a total of anything else is not
    /// a total, and [`validate_type`] holds a declaration to that.
    pub fn summing(key: &str) -> Field {
        Field {
            folds: Fold::Sum,
            ..Field::new(key, ValueType::Number)
        }
    }

    /// A reference narrowed to one kind — the declaration saying not just that
    /// this key holds a handle, but which kind of thing is on the other end.
    pub fn pointing_at(key: &str, kind: EntityKind) -> Field {
        Field {
            points_at: Some(kind),
            ..Field::new(key, ValueType::Reference)
        }
    }

    /// **A key holding one of a named set**, and nothing else.
    ///
    /// It holds text, because a closed vocabulary is tokens: what narrows the
    /// key is the set, and the value type under it stays the honest widest
    /// answer. A set on a key holding something else is sayable and is nobody's
    /// case yet.
    pub fn one_of<I, S>(key: &str, values: I) -> Field
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Field {
            one_of: Some(
                values
                    .into_iter()
                    .map(|v| v.as_ref().trim().to_string())
                    .collect(),
            ),
            ..Field::new(key, ValueType::Text)
        }
    }

    /// **Whether a value holds what this key was declared to hold** — the value
    /// type, and for a reference the kind it points at.
    ///
    /// This rather than [`ValueType::holds`] is what a record is measured
    /// against, because the kind a reference names is part of the declaration
    /// and a check that read the value type alone would call
    /// `pet:santas-little-helper` a good venue.
    pub fn accepts(&self, value: &str) -> bool {
        // **Every item, and a list with nothing in it holds nothing badly.** A
        // key declared to hold zero or more is answered by zero.
        self.items(value)
            .into_iter()
            .all(|item| self.accepts_one(item))
    }

    /// One item of what this key holds — the whole value for an ordinary key.
    fn accepts_one(&self, value: &str) -> bool {
        if !self.holds.holds(value) {
            return false;
        }
        // **Whole, trimmed and case-sensitive**, the same comparison
        // [`Compare::Equals`] makes. A set whose members differ only by case is
        // a set somebody wrote badly, and folding the case here would hide that
        // rather than help anybody.
        if let Some(values) = &self.one_of
            && !values.iter().any(|allowed| allowed == value.trim())
        {
            return false;
        }
        match self.points_at {
            None => true,
            Some(kind) => EntityId(value.trim().to_string()).kind() == Some(kind),
        }
    }

    /// **The declaration as one token**, which is how it crosses the wire and
    /// how a store keeps it: `date`, or `reference` for a handle of any kind,
    /// or `reference:place` for one narrowed to a kind.
    ///
    /// One spelling for both directions, paired with [`Field::of_token`] — a
    /// store column and a served field that disagreed about the spelling would
    /// lose the kind on the first round trip.
    pub fn holds_token(&self) -> String {
        let held = match self.points_at {
            Some(kind) => format!("{}:{}", self.holds.as_token(), kind.as_token()),
            None => self.holds.as_token().to_string(),
        };
        if self.list {
            format!("list:{held}")
        } else {
            held
        }
    }

    /// **The set as one store cell**, its values separated by commas, and
    /// nothing for a key narrowed to nothing.
    ///
    /// The comma is what separates the items of a list value, so the two
    /// spellings agree — and it is why [`validate_type`] refuses a value
    /// carrying one. Paired with [`Field::one_of_from_cell`] for the reason
    /// [`Field::holds_token`] is paired with [`Field::of_token`]: two halves
    /// that disagreed about the spelling would lose the set on the first round
    /// trip.
    pub fn one_of_cell(&self) -> Option<String> {
        self.one_of.as_ref().map(|values| values.join(","))
    }

    /// The set a stored cell names, and nothing for a cell that names none.
    ///
    /// **An empty cell reads as no set rather than as an empty one.** A set
    /// with no values is refused at the declaration, so a cell holding one is
    /// damage — and reading it as a narrowing to nothing would make every write
    /// to that key impossible, where reading it as unnarrowed leaves the key
    /// exactly as wide as a key nobody declared a set for.
    pub fn one_of_from_cell(cell: Option<&str>) -> Option<Vec<String>> {
        let values: Vec<String> = cell?
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect();
        (!values.is_empty()).then_some(values)
    }

    /// The field a key and a declaration token name, or nothing when the token
    /// names no value type or no kind.
    pub fn of_token(key: &str, token: &str) -> Option<Field> {
        // **A list wraps what it holds**, so the prefix comes off first and the
        // rest is read exactly as an ordinary key's token is. That is what lets
        // `list:reference:place` mean what both halves already mean.
        if let Some(element) = token.trim().strip_prefix("list:") {
            return Some(Field {
                list: true,
                ..Field::of_token(key, element)?
            });
        }
        match token.trim().split_once(':') {
            None => Some(Field::new(key, ValueType::of_token(token)?)),
            Some((holds, kind)) => {
                // A kind narrows a reference and nothing else: there is no
                // sense in which a date points at a place, and accepting the
                // token would store a narrowing that never gets read.
                if ValueType::of_token(holds)? != ValueType::Reference {
                    return None;
                }
                Some(Field::pointing_at(
                    key,
                    EntityKind::from_token(kind.trim())?,
                ))
            }
        }
    }
}

/// **A named set of keys.** It arrives complete with its fields: a type with no
/// fields is a name nobody can query by, and a caller should never have to
/// configure one into usefulness.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredType {
    pub name: String,
    pub fields: Vec<Field>,
    /// Where this declaration came from. Set by the writer that made it, never
    /// by the caller — see [`Origin`].
    pub origin: Origin,
}

/// A key whose value does not hold what the type said it would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistyped {
    pub key: String,
    /// What the type said the key holds.
    pub declared: ValueType,
    /// The kind the key points at, when it is a reference that names one.
    /// Carried beside `declared` because `reference` alone cannot say what is
    /// wrong with a value that IS a handle — a reader looking at
    /// `pet:santas-little-helper` under a key wanting a place needs the kind to
    /// see the mistake at all.
    pub points_at: Option<EntityKind>,
    /// The values the key is narrowed to, when the declaration names a set.
    /// Carried for the reason `points_at` is: a closed set holds text, so a
    /// reader told the key wants text is looking at a value that IS text and
    /// cannot see the mistake at all.
    pub one_of: Option<Vec<String>>,
    /// What the record actually carries, so a reader can see the mistake
    /// rather than being told one happened.
    pub value: String,
    /// Whether the key is one this type requires. A bad value in a required
    /// key leaves the type unanswered; a bad value in an optional one is a
    /// mistake worth reporting and does not decide whether the thing is one of
    /// these.
    pub required: bool,
}

impl Mistyped {
    /// **What the key wanted, as the declaration spells it** — `date`, or
    /// `reference:place`. The same token `declare_type` takes, so a refusal
    /// tells a caller what to write by naming what was asked for.
    pub fn wanted(&self) -> String {
        // **The set, when there is one, and never the value type under it.** A
        // closed set holds text, so naming the value type here would tell a
        // reader that text is the problem while the value in front of them is
        // text — the values are the whole of what the key wants.
        if let Some(values) = &self.one_of {
            return format!("one of {}", values.join(", "));
        }
        match self.points_at {
            Some(kind) => format!("{}:{}", self.declared.as_token(), kind.as_token()),
            None => self.declared.as_token().to_string(),
        }
    }
}

/// **How a record answers a type.**
///
/// A record that carries none of a type's keys is not a match at all, and the
/// absence of this value is how that is said — otherwise every record matches
/// every type and an answer means nothing.
///
/// **What gets asked is usually a THING's fields**, folded from its writes by
/// [`super::folded_fields`], rather than one record's. The matcher takes a flat
/// map and does not know which it was handed, which is what let the unit change
/// without the algorithm changing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The keys the record carries, in the type's own order.
    pub held: Vec<String>,
    /// The keys it does not, by NAME. A count would leave the caller's next
    /// move — fill them, or ignore them — with nothing to act on.
    ///
    /// **Every declared key it lacks, required or not** — a reader asking what
    /// a thing is missing wants the optional ones too, because that is what
    /// says what could still be written down.
    pub lacking: Vec<String>,
    /// The subset of `lacking` the type REQUIRES. **This is what completeness
    /// is measured against**; the rest is a description of what else there is
    /// room for.
    pub lacking_required: Vec<String>,
    /// Keys that are there and hold something else — by name, by what the key
    /// wanted and by what is actually in it, so a reader sees the mistake.
    pub mistyped: Vec<Mistyped>,
}

impl Match {
    /// **Every key the type names is there and holds what it was declared to
    /// hold** — the question a READER asks.
    ///
    /// *Which of these ARE deliveries* is this one, and *which of these are
    /// described like a delivery* is the tolerant question beside it, which
    /// keeps a thing and names its gaps. A reader picks; nothing here refuses
    /// anything.
    ///
    /// Holding a key badly is not holding it, so a value that breaks its
    /// declaration leaves the thing short exactly as an absent key does.
    ///
    /// ⚠️ **This is NOT the floor a write is held to** — see
    /// [`Match::meets_the_floor`]. The two questions differ on the optional
    /// keys, and a type a caller declared has no required keys unless the
    /// caller said so: measured by the floor, such a type is answered by
    /// anything carrying one of its keys, and the strict question becomes the
    /// tolerant one.
    pub fn complete(&self) -> bool {
        self.lacking.is_empty() && self.mistyped.is_empty()
    }

    /// **Every REQUIRED key is there and holds what it was declared to hold** —
    /// the question a WRITE is held to (rule 214).
    ///
    /// The required set is the minimum that makes a thing that thing, so this
    /// is what a write may not take away: once a thing meets the floor, a write
    /// that would drop it below is refused. See [`super::guard_fit`].
    ///
    /// **An optional key decides nothing here, held, absent or wrong.** That is
    /// the whole of what optional means: it is welcome, it is never demanded,
    /// and a thing without it is not a thing with something missing. What is in
    /// it when it IS there is a separate question, checked on the write where
    /// the key belongs to a KIND and reported by `mistyped` either way.
    pub fn meets_the_floor(&self) -> bool {
        self.lacking_required.is_empty() && !self.mistyped.iter().any(|m| m.required)
    }
}

impl DeclaredType {
    /// The field this type declares under `key`, if it declares one.
    pub fn field(&self, key: &str) -> Option<&Field> {
        let key = key.trim();
        self.fields.iter().find(|f| f.key == key)
    }

    /// A type a caller declared. The ordinary constructor, because a caller is
    /// who declares nearly all of them.
    pub fn new(name: &str, fields: Vec<Field>) -> DeclaredType {
        DeclaredType {
            name: name.trim().to_string(),
            fields,
            origin: Origin::Declared,
        }
    }

    /// **A type the software ships**, which a caller cannot replace.
    ///
    /// The only way one is made. Nothing on the served surface reaches this:
    /// the verb builds its declarations with [`DeclaredType::new`], so the
    /// origin follows how the declaration arrived rather than what anybody
    /// asked for.
    pub fn shipped(name: &str, fields: Vec<Field>) -> DeclaredType {
        DeclaredType {
            origin: Origin::Shipped,
            ..DeclaredType::new(name, fields)
        }
    }

    /// The same declaration with every label trimmed — what a store keeps, so
    /// that two stores keep the same thing.
    ///
    /// It is the constructors doing it rather than a second rule: a
    /// declaration built by hand and one built by [`DeclaredType::new`] have to
    /// be one record by the time either is stored, or a key reads back with a
    /// space on it and matches nothing.
    pub fn normalized(&self) -> DeclaredType {
        DeclaredType {
            origin: self.origin,
            ..DeclaredType::new(
                &self.name,
                self.fields
                    .iter()
                    .map(|f| Field {
                        points_at: f.points_at,
                        folds: f.folds,
                        required: f.required,
                        list: f.list,
                        one_of: f
                            .one_of
                            .as_ref()
                            .map(|values| values.iter().map(|v| v.trim().to_string()).collect()),
                        ..Field::new(&f.key, f.holds)
                    })
                    .collect(),
            )
        }
    }

    /// **Does this record answer to this type, and how well.**
    ///
    /// Structural: the record is never asked what it was declared to be, only
    /// what it carries. `None` means it carries none of these keys.
    pub fn matched_by(&self, record: &BTreeMap<String, String>) -> Option<Match> {
        let mut held = Vec::new();
        let mut lacking = Vec::new();
        let mut lacking_required = Vec::new();
        let mut mistyped = Vec::new();
        for field in &self.fields {
            match record.get(&field.key) {
                Some(value) => {
                    held.push(field.key.clone());
                    if !field.accepts(value) {
                        mistyped.push(Mistyped {
                            key: field.key.clone(),
                            declared: field.holds,
                            points_at: field.points_at,
                            one_of: field.one_of.clone(),
                            value: value.clone(),
                            required: field.required,
                        });
                    }
                }
                None => {
                    lacking.push(field.key.clone());
                    if field.required {
                        lacking_required.push(field.key.clone());
                    }
                }
            }
        }
        if held.is_empty() {
            return None;
        }
        Some(Match {
            held,
            lacking,
            lacking_required,
            mistyped,
        })
    }
}

/// **Can any value ever satisfy this key**, and when none can, which pair of
/// the key's own properties rules everything out.
///
/// **One question, not a list of the mistakes somebody thought of.** A check
/// that enumerates today's instances fires only on today's instances (rule
/// 106), and a declaration nothing can satisfy is one defect however it is
/// reached. Two properties narrow what may be WRITTEN — what the key holds, and
/// the set when it has one — and the fold decides what the key is READ as. Ask
/// whether those leave anything, and the cases fall out of the question rather
/// than the question being assembled from the cases.
///
/// **What it does NOT ask is how a set is spelled.** A value named twice and a
/// value carrying a comma are refused elsewhere, and neither makes the key
/// unwritable: the first means what it would mean without the repeat, and the
/// second only collides with the list separator.
///
/// **A set with one value the key can hold is satisfiable**, whatever else is
/// in it. The other values are values nobody can write, which is a caller's
/// mistake and not a key nobody can write to — and refusing them would be the
/// enumeration this question replaced.
fn unsatisfiable(field: &Field) -> Option<String> {
    // ① **What may be written.** The set is the whole of what the key takes, so
    // a set holding no value the key's own type accepts leaves nothing — and a
    // set with no values at all is that same emptiness reached sooner.
    if let Some(values) = &field.one_of {
        if values.is_empty() {
            return Some(format!(
                "the key '{}' is narrowed to a set with no values in it, so there is nothing it \
                 could be written with",
                field.key,
            ));
        }
        if !values.iter().any(|value| field.holds.holds(value)) {
            return Some(format!(
                "the key '{}' holds {} and its set names no value that is one",
                field.key,
                field.holds_token(),
            ));
        }
    }
    // ② **What it is read as.** A counter comes back as the total of its
    // writes, so the key has to be able to hold that total — which is any
    // number at all, however few the writes were.
    if field.folds == Fold::Sum {
        if field.one_of.is_some() {
            return Some(format!(
                "the key '{}' is a counter, so it reads back as the total of its writes, and a \
                 total is any number at all — a set names the values it names",
                field.key,
            ));
        }
        if field.holds != ValueType::Number {
            return Some(format!(
                // The whole declared token, narrowing included: a key sent as
                // `reference:place` is refused over what the caller wrote, and
                // `reference` alone would name a half they did not send.
                "the key '{}' is a counter and holds {}, and a total of those is not one",
                field.key,
                field.holds_token(),
            ));
        }
    }
    None
}

/// **Is this declaration one a store can keep.**
///
/// A name and its keys are short labels, so a malformed one is refused rather
/// than mangled — the same posture the entity fields take, for the same reason.
///
/// **A type with no fields is refused, and that is rule 94 rather than
/// tidiness**: a type arrives complete with its keys. A name with nothing under
/// it is a type nobody can query by, and it would have to be configured into
/// usefulness before it did anything.
///
/// **A key nothing could ever satisfy is the other thing refused here**, and it
/// is [`unsatisfiable`] that asks it — one question over the properties that
/// narrow a key, rather than a list of the ways a declaration can contradict
/// itself. A counter holding prose, a counter narrowed to a set, and a set
/// holding no value its own key accepts are all answers to it.
///
/// Everything refused here is about the DECLARATION rather than about anything
/// answering it. No record is checked on this path, and nothing here stops a
/// record being found. The write-time check on what a record holds lives at
/// [`super::guard_fit`], where the thing's own fields are in hand.
pub fn validate_type(declared: &DeclaredType) -> Result<(), MemoryError> {
    label("type name", &declared.name)?;
    if declared.fields.is_empty() {
        return Err(MemoryError::InvalidType(format!(
            "type '{}' names no keys, and a type is the keys it names",
            declared.name
        )));
    }
    let mut seen: Vec<&str> = Vec::new();
    for field in &declared.fields {
        label("key", &field.key)?;
        // **The same number a written key gets.** A type names keys and a
        // record carries them, and they are one namespace: a type free to name
        // a longer key would declare a floor no record could meet.
        if field.key.chars().count() > crate::memory::MAX_KEY_CHARS {
            return Err(MemoryError::InvalidType(format!(
                "type '{}' names a key of {} characters, and a key may be {}: no record could \
                 carry it",
                declared.name,
                field.key.chars().count(),
                crate::memory::MAX_KEY_CHARS,
            )));
        }
        if seen.contains(&field.key.as_str()) {
            return Err(MemoryError::InvalidType(format!(
                "type '{}' names the key '{}' twice",
                declared.name, field.key
            )));
        }
        // **How a set is SPELLED**, which is a different question from whether
        // anything can satisfy the key: a repeated value and a value carrying a
        // comma both leave the key perfectly writable. They are refused because
        // the store should not keep a caller's mistake, and because a comma is
        // what separates the items of a list, so no list of that key could ever
        // spell such a value.
        if let Some(values) = &field.one_of {
            let mut named: Vec<&str> = Vec::new();
            for value in values {
                label("set value", value)?;
                if value.contains(',') {
                    return Err(MemoryError::InvalidType(format!(
                        "type '{}' names '{}' in the set for the key '{}', and a value in a set \
                         may not carry a comma: a comma separates the items of a list, so no list \
                         of this key could spell it",
                        declared.name, value, field.key,
                    )));
                }
                if named.contains(&value.as_str()) {
                    return Err(MemoryError::InvalidType(format!(
                        "type '{}' names '{}' twice in the set for the key '{}'",
                        declared.name, value, field.key,
                    )));
                }
                named.push(value);
            }
        }
        // **And the one question about whether the key can hold anything at
        // all.**
        if let Some(why) = unsatisfiable(field) {
            return Err(MemoryError::InvalidType(format!(
                "type '{}' declares a key nothing could satisfy: {why}",
                declared.name,
            )));
        }
        seen.push(&field.key);
    }
    Ok(())
}

/// **May this declaration be written over what the store already holds under
/// that name.**
///
/// A declaration replaces the one it lands on, whole — that is what a type is.
/// The one thing it may not land on is a type the software ships: those are
/// closed, and a caller can neither extend one, shrink one nor replace one.
/// Changing a shipped type is a code change.
///
/// **It reads the origin rather than a list of names** (rule 106), and it is
/// one function both stores call: a rule each of them re-implemented would be
/// a rule they eventually disagree about, and the disagreement would show up
/// as the real store losing a shipped type the fake kept.
///
/// `held` is what the store has under this name now, or nothing when the name
/// is free.
pub fn guard_replacement(incoming: &DeclaredType, held: Option<Origin>) -> Result<(), MemoryError> {
    if incoming.origin == Origin::Declared && held == Some(Origin::Shipped) {
        return Err(MemoryError::ShippedType {
            name: incoming.name.trim().to_string(),
        });
    }
    Ok(())
}

/// **May this declaration be written over the kind the store holds under that
/// token** — the same question [`guard_replacement`] asks of a type, asked of
/// a kind.
///
/// It is one function both stores call, for the reason the type's guard is:
/// a rule each of them re-implemented is a rule they eventually disagree
/// about, and the disagreement would show up as the real store letting a
/// caller take a kind the fake kept closed.
///
/// **The refusal names what the caller can do instead.** A bare no on a write
/// that will never land however often it is sent costs more than the mistake
/// it reports: nothing the caller puts in this call reaches a kind the
/// software owns, and what they can do is declare under a token of their own.
///
/// `held` is the origin the store has under this token now, or nothing when
/// the token is free.
pub fn guard_kind_replacement(
    token: &str,
    incoming: Origin,
    held: Option<Origin>,
) -> Result<(), MemoryError> {
    if incoming == Origin::Declared && held == Some(Origin::Shipped) {
        return Err(MemoryError::InvalidEntity(format!(
            "'{token}' is a kind the software ships, and a caller cannot redeclare one: \
             sending this again will not change the answer, because changing a kind the \
             software owns is a change to the software. Declare a kind under a token of \
             your own instead, and that one is yours to reshape"
        )));
    }
    Ok(())
}

/// One plain label: a name or a key. Both are stored as one cell and read back
/// as one token, so a control character or a backtick in either is refused at
/// the door.
fn label(what: &str, value: &str) -> Result<(), MemoryError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(MemoryError::InvalidType(format!("{what} is empty")));
    }
    if value.chars().count() > 190 {
        return Err(MemoryError::InvalidType(format!("{what} is too long")));
    }
    if value.chars().any(|c| c == '`' || c.is_control()) {
        return Err(MemoryError::InvalidType(format!(
            "{what} must be one plain line (no newline, no backtick)"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::MAX_KEY_CHARS;

    fn record(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect()
    }

    fn booking() -> DeclaredType {
        DeclaredType::new(
            "booking",
            vec![
                Field::required("starts", ValueType::Date),
                Field::required("seats", ValueType::Number),
                Field::required("venue", ValueType::Reference),
            ],
        )
    }

    /// **The load-bearing one: nobody declared anything on the record.**
    ///
    /// A record is matched by what it carries, so a bag of keys answers a type
    /// it has never heard of. Paired with the negative it rests on — a record
    /// carrying none of the keys is not a match at all — because "it matched"
    /// means nothing if everything matches everything.
    #[test]
    fn a_record_answers_a_type_it_never_declared() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let carried = record(&[
            ("starts", "2026-08-10"),
            ("seats", "4"),
            ("venue", "place:moes"),
        ]);
        let found = booking()
            .matched_by(&carried)
            .expect("a record carrying every key matches");
        assert!(found.complete(), "every key is there: {found:?}");
        assert!(found.lacking.is_empty());
        assert!(found.mistyped.is_empty(), "{found:?}");

        assert_eq!(
            booking().matched_by(&record(&[("mood", "curious")])),
            None,
            "a record sharing no key with a type is not a partial match, it is not a match",
        );
    }

    /// **A partial match comes back, and it names what it lacks.**
    ///
    /// The common case rather than the edge case: a query returning only
    /// complete matches would hide exactly the records worth finding. The names
    /// are the point — a count leaves a caller with nothing to act on.
    #[test]
    fn a_partial_match_is_returned_and_names_the_keys_it_lacks() {
        let found = booking()
            .matched_by(&record(&[("starts", "2026-08-10")]))
            .expect("one key is enough to match");
        assert!(!found.complete(), "it is missing two keys: {found:?}");
        assert_eq!(found.held, vec!["starts"]);
        assert_eq!(
            found.lacking,
            vec!["seats", "venue"],
            "the keys it lacks come back by name: {found:?}",
        );
    }

    /// **A value that does not hold what the key was declared to hold is
    /// FLAGGED and still comes back — and it does not complete the type.**
    ///
    /// Reading is not a gate: the record matches, it is returned, and the bad
    /// value is reported with what was declared and what is actually there, so
    /// a reader can go and fix it.
    ///
    /// **Completeness is the other question.** Holding a key badly is not
    /// holding it, so this record does not answer `booking` whole even though
    /// no key is absent — and `lacking` being empty beside it is the beat that
    /// says so, because it is what a definition reading `lacking` alone would
    /// have gone by.
    #[test]
    fn a_value_that_fails_its_type_is_flagged_and_does_not_complete_the_type() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        let found = booking()
            .matched_by(&record(&[
                ("starts", "next tuesday"),
                ("seats", "4"),
                ("venue", "place:moes"),
            ]))
            .expect("a record with a bad value still matches");
        assert!(found.lacking.is_empty(), "no key is absent: {found:?}",);
        assert!(
            !found.complete(),
            "…and it still does not fit, because the date slot holds no date: {found:?}",
        );
        assert_eq!(found.mistyped.len(), 1, "{found:?}");
        assert_eq!(found.mistyped[0].key, "starts");
        assert_eq!(found.mistyped[0].declared, ValueType::Date);
        assert_eq!(
            found.mistyped[0].value, "next tuesday",
            "the reader sees the mistake rather than being told one happened",
        );

        // The positive it rests on: the same type, the same keys, good values.
        // Without it this passes on a build where nothing is ever complete.
        assert!(
            booking()
                .matched_by(&record(&[
                    ("starts", "2026-08-10"),
                    ("seats", "4"),
                    ("venue", "place:moes"),
                ]))
                .expect("it carries every key")
                .complete(),
        );
    }

    /// Each value type, both ways in one case: a check that only ever saw the
    /// good value would pass on a build where everything holds everything.
    #[test]
    fn each_value_type_accepts_its_own_and_refuses_the_rest() {
        // **The set is this case's subject, not its setup, and it is pinned on
        // both sides of the set on purpose.** `Reference` decides whether a
        // value is a handle by asking it for its kind, so against an unloaded
        // set it reports a well-formed handle as not a reference — an answer
        // that is wrong rather than absent, which nothing else here would
        // catch.
        crate::memory::kinds::load_shipped();
        for (holds, good, bad) in [
            (ValueType::Number, "4", "four"),
            (ValueType::Boolean, "true", "yes"),
            (ValueType::Date, "2026-08-10", "next tuesday"),
            (ValueType::Reference, "place:moes", "moes"),
        ] {
            assert!(holds.holds(good), "{holds:?} refused {good:?}");
            assert!(!holds.holds(bad), "{holds:?} accepted {bad:?}");
        }
        // Text holds anything, which is the honest answer for prose and the
        // reason it is the one type that can never be mistyped.
        assert!(ValueType::Text.holds("next tuesday"));
    }

    /// **A key declared to hold one of a named set takes those values and no
    /// others.**
    ///
    /// Three beats, and the third is what makes the first two mean anything: a
    /// member is accepted, a non-member is not, and the SAME non-member is
    /// accepted by an unnarrowed text key. Without the third this passes
    /// identically on a build where a closed set refuses everything.
    #[test]
    fn a_closed_set_takes_its_own_values_and_no_others() {
        let outcome = Field::one_of("outcome", ["ran", "skipped", "snoozed"]);
        for member in ["ran", "skipped", "snoozed"] {
            assert!(outcome.accepts(member), "the set names {member:?}");
        }
        assert!(
            !outcome.accepts("done"),
            "'done' is not one of the values the declaration names",
        );
        assert!(
            Field::new("outcome", ValueType::Text).accepts("done"),
            "…and the refusal is the SET talking, not text: an unnarrowed key takes it",
        );
    }

    /// **A closed set narrows a list the same way it narrows one value.**
    ///
    /// The list prefix wraps what a key holds, and a set is on the key, so
    /// every item is measured against it. Paired with a list whose items are
    /// all members, because a build that refused every list would pass the
    /// negative alone.
    #[test]
    fn a_closed_set_narrows_every_item_of_a_list() {
        let outcome = Field {
            list: true,
            ..Field::one_of("outcome", ["ran", "skipped", "snoozed"])
        };
        assert!(
            outcome.accepts("ran, snoozed"),
            "every item is one the set names",
        );
        assert!(
            !outcome.accepts("ran, done"),
            "one item outside the set is enough to refuse the value",
        );
    }

    /// **A value outside the set is flagged on a read, and the flag names the
    /// values.**
    ///
    /// Reading refuses nothing, so the record comes back — and a reader that
    /// has to go and fix it needs the allowed values rather than the word
    /// `text`, which is what the key holds underneath and says nothing about
    /// what is wrong.
    #[test]
    fn a_value_outside_the_set_is_flagged_and_names_what_was_allowed() {
        let check_in = DeclaredType::new(
            "check_in",
            vec![Field::one_of("outcome", ["ran", "skipped", "snoozed"]).needed()],
        );
        let found = check_in
            .matched_by(&record(&[("outcome", "done")]))
            .expect("the record carries the key");
        assert!(
            !found.complete(),
            "holding a key badly is not holding it: {found:?}",
        );
        assert_eq!(found.mistyped.len(), 1, "{found:?}");
        let wanted = found.mistyped[0].wanted();
        for value in ["ran", "skipped", "snoozed"] {
            assert!(
                wanted.contains(value),
                "the reader is told what it may write: {wanted:?}",
            );
        }

        // The positive it rests on: a member of the set completes the type.
        assert!(
            check_in
                .matched_by(&record(&[("outcome", "snoozed")]))
                .expect("the record carries the key")
                .complete(),
        );
    }

    /// **A declaration with a set nothing can satisfy is refused, and the
    /// refusal says which mistake it was.**
    ///
    /// Three mistakes with three answers, because a caller comparing its own
    /// input against one generic complaint has to work out which of them it
    /// made. A set with no values is a key nothing can ever be written to; a
    /// value named twice is a caller error the store should not keep; a value
    /// carrying a comma cannot survive the list shape, where a comma is what
    /// separates one item from the next.
    #[test]
    fn a_closed_set_that_cannot_be_satisfied_is_refused_by_its_own_name() {
        let declaring = |field: Field| {
            validate_type(&DeclaredType::new("check_in", vec![field]))
                .expect_err("this declaration is refused")
                .to_string()
        };

        let empty = declaring(Field::one_of("outcome", Vec::<String>::new()));
        assert!(
            empty.contains("outcome") && empty.contains("no values"),
            "the refusal names the empty set: {empty:?}",
        );

        let twice = declaring(Field::one_of("outcome", ["ran", "skipped", "ran"]));
        assert!(
            twice.contains("outcome") && twice.contains("ran") && twice.contains("twice"),
            "the refusal names the repeated value: {twice:?}",
        );

        let comma = declaring(Field::one_of("outcome", ["ran", "did not, quite"]));
        assert!(
            comma.contains("outcome") && comma.contains("comma"),
            "the refusal names the separator the value collides with: {comma:?}",
        );

        // The positive all three rest on: the same declaration, a good set.
        validate_type(&DeclaredType::new(
            "check_in",
            vec![Field::one_of("outcome", ["ran", "skipped", "snoozed"])],
        ))
        .expect("a set of three plain values is a declaration a store can keep");
    }

    /// **A declaration nothing could ever satisfy is refused, whichever pair of
    /// its own properties rules everything out.**
    ///
    /// One question rather than a list of mistakes: a key is narrowed by what
    /// it HOLDS and by its SET, and what it is READ as is decided by its fold.
    /// The cases below are what falls out of asking whether those leave
    /// anything — they are not the check's structure, and a fifth pair of
    /// properties would be caught by the same question.
    ///
    /// **Each refusal is paired with the declaration it differs from by one
    /// property**, because a case sending only bad declarations passes on a
    /// build that refuses every declaration.
    #[test]
    fn a_declaration_no_value_could_satisfy_is_refused() {
        let declaring = |field: Field| validate_type(&DeclaredType::new("snacking", vec![field]));
        let refused = |field: Field| {
            declaring(field)
                .expect_err("no value could satisfy this key")
                .to_string()
        };

        // ① The set is empty, so it names no value at all.
        let said = refused(Field::one_of("flavour", Vec::<String>::new()));
        assert!(
            said.contains("flavour") && said.contains("no values"),
            "{said}",
        );

        // ② The set names values, and the key holds something none of them is.
        let said = refused(Field {
            one_of: Some(vec!["cherry".to_string(), "plain".to_string()]),
            ..Field::new("flavour", ValueType::Number)
        });
        assert!(
            said.contains("flavour") && said.contains("number"),
            "the refusal names the key and the half that rules the set out: {said}",
        );
        // …and one value the key CAN hold is enough, because the key is
        // writable then. The dead value is a value nobody can write, not a
        // declaration nothing can satisfy.
        declaring(Field {
            one_of: Some(vec!["4".to_string(), "cherry".to_string()]),
            ..Field::new("flavour", ValueType::Number)
        })
        .expect("one value the key holds is a key somebody can write to");

        // ③ A counter is read as the total of its writes, which is any number
        // at all, and a set holds only the values it names.
        let said = refused(Field {
            folds: Fold::Sum,
            one_of: Some(vec!["1".to_string(), "2".to_string()]),
            ..Field::new("donuts", ValueType::Number)
        });
        assert!(
            said.contains("donuts") && said.contains("total"),
            "the refusal names the key and what a counter is read as: {said}",
        );
        // The same key without the set is what a counter is.
        declaring(Field::summing("donuts")).expect("a counter holding a number is a counter");
        // …and the same set without the counter is an ordinary narrowed key.
        declaring(Field {
            one_of: Some(vec!["1".to_string(), "2".to_string()]),
            ..Field::new("donuts", ValueType::Number)
        })
        .expect("a narrowed key that folds newest is ordinary");

        // ④ A counter over a key that holds something a total is not. The
        // oldest of these, and it is the same question: what the fold produces
        // has to be something the key can hold.
        let said = refused(Field {
            folds: Fold::Sum,
            ..Field::new("visits", ValueType::Date)
        });
        assert!(said.contains("visits") && said.contains("date"), "{said}");
    }

    /// **A declared reference names the kind it points at, and a handle of
    /// another kind does not hold it.**
    ///
    /// Three beats, because each covers the way the other two pass on a build
    /// that is wrong: the right kind is accepted, the wrong one is not, and a
    /// reference that named no kind still takes any handle. Without the third,
    /// this passes identically on a build that narrowed every reference to one
    /// kind nobody asked for.
    #[test]
    fn a_reference_declared_for_one_kind_refuses_a_handle_of_another() {
        // **The set is this case's subject, not its setup.** Which kind a
        // handle names is the thing being refused on, and the set is what
        // answers it.
        crate::memory::kinds::load_shipped();
        let venue = Field::pointing_at("venue", EntityKind::PLACE).needed();
        assert!(
            venue.accepts("place:moes"),
            "the kind it was declared for is what it holds",
        );
        assert!(
            !venue.accepts("pet:santas-little-helper"),
            "a handle of another kind does not hold a key declared for places",
        );
        assert!(
            Field::required("venue", ValueType::Reference).accepts("pet:santas-little-helper"),
            "a reference that named no kind is any handle, which is what makes \
             naming one a narrowing rather than the default",
        );
    }

    /// **A wrong-kinded handle is mistyped, and the report names the kind that
    /// was wanted.**
    ///
    /// The value type alone cannot say it: both handles are references, so a
    /// reader told only `reference` sees a key holding a handle and no mistake.
    /// Paired with the whole match it sits on, so it cannot pass by the record
    /// failing to match at all.
    #[test]
    fn a_reference_of_the_wrong_kind_is_reported_with_the_kind_it_wanted() {
        let stay = DeclaredType::new(
            "stay",
            vec![
                Field::pointing_at("venue", EntityKind::PLACE).needed(),
                Field::required("starts", ValueType::Date),
            ],
        );
        let found = stay
            .matched_by(&record(&[
                ("venue", "pet:santas-little-helper"),
                ("starts", "2026-08-10"),
            ]))
            .expect("it carries both keys");
        assert_eq!(found.lacking, Vec::<String>::new(), "{found:?}");
        assert_eq!(found.mistyped.len(), 1, "{found:?}");
        assert_eq!(found.mistyped[0].key, "venue");
        assert_eq!(
            found.mistyped[0].points_at,
            Some(EntityKind::PLACE),
            "the reader is told which kind the key wanted, which `reference` \
             alone cannot say: {found:?}",
        );
        assert_eq!(found.mistyped[0].value, "pet:santas-little-helper");
    }

    /// **The declaration is one token, and it round-trips.** A store column
    /// and a served field both carry this spelling, so a narrowing that could
    /// be written and not read back would be a declaration the store forgets.
    ///
    /// The refusals are here beside it because the token is parsed from a
    /// caller's string: a kind on something that is not a reference is a
    /// narrowing nothing would ever read, and an unknown kind is a key that
    /// would accept nothing at all.
    #[test]
    fn a_declaration_survives_the_token_it_is_written_as() {
        // **This case runs in a booted process.** It parses a handle and asserts
        // about something else, so the set is setup — and setup comes from
        // standing a store up, filled from what that store holds.
        let _booted = crate::memory::testing::InMemoryMemory::booted();
        // **Required is not in the token and must not be**: it is a property of
        // the key beside what the key holds, kept in its own column, so a
        // fixture claiming it here would be asserting that the token carries
        // something it does not.
        for field in [
            Field::new("starts", ValueType::Date),
            Field::new("venue", ValueType::Reference),
            Field::pointing_at("venue", EntityKind::PLACE),
            Field::listing("came_with", ValueType::Reference),
            Field {
                points_at: Some(EntityKind::PLACE),
                ..Field::listing("venues", ValueType::Reference)
            },
            Field::new("away", ValueType::DateRange),
        ] {
            assert_eq!(
                Field::of_token(&field.key, &field.holds_token()).as_ref(),
                Some(&field),
                "{field:?} did not survive '{}'",
                field.holds_token(),
            );
        }
        assert_eq!(
            Field::pointing_at("venue", EntityKind::PLACE)
                .needed()
                .holds_token(),
            "reference:place",
            "the spelling itself, since it is what a caller writes and a column keeps",
        );
        assert_eq!(Field::of_token("venue", "reference:sofa"), None);
        assert_eq!(Field::of_token("starts", "date:place"), None);
    }

    /// **A key belongs to the type that names it, and to nothing else.** Two
    /// types using one key name each mean their own thing by it, which is what
    /// keeps keys out of any global register.
    #[test]
    fn two_types_may_name_one_key_and_mean_their_own_thing() {
        let seats_as_text =
            DeclaredType::new("seating", vec![Field::required("seats", ValueType::Text)]);
        let loose = record(&[("seats", "a few")]);
        assert!(
            seats_as_text
                .matched_by(&loose)
                .expect("it carries the key")
                .mistyped
                .is_empty(),
            "the type that declared it text is satisfied",
        );
        assert_eq!(
            booking()
                .matched_by(&loose)
                .expect("it carries the key")
                .mistyped
                .len(),
            1,
            "and the type that declared it a number says so, about the same record",
        );
    }

    /// **A summing key that points at a kind is refused because a reference is
    /// no number**, and the refusal names the key and the value type as the
    /// caller spelled it.
    ///
    /// **This is not a rule about the combination, and the name says so.** The
    /// two halves of a declaration were built apart — one says which kind a
    /// reference points at, the other says how the key's writes fold — and a
    /// caller can name both on one key. Nothing weighs the pair. What refuses
    /// it is the counter's own rule: a total of anything but a number is not a
    /// total, and a narrowing is only ever set on a reference. Take that one
    /// rule out and this declaration is accepted, which is what the sabotage
    /// behind this case showed.
    ///
    /// The narrowing is what the refusal has to carry: `reference` alone would
    /// leave a caller who wrote `reference:place` looking for a word they did
    /// not send.
    ///
    /// Both halves are declared apart in the same case, because a refusal for
    /// every declaration would pass this on a build that refuses everything.
    #[test]
    fn a_summing_key_that_points_at_a_kind_is_refused_because_a_reference_is_no_number() {
        let refused = validate_type(&DeclaredType::new(
            "snacking",
            vec![Field {
                folds: Fold::Sum,
                ..Field::pointing_at("donuts", EntityKind::PLACE).needed()
            }],
        ))
        .expect_err("a total of handles is not a total");
        let said = refused.to_string();
        assert!(said.contains("donuts"), "the key a caller must fix: {said}");
        assert!(
            said.contains("reference:place"),
            "…and the half that is wrong, spelled as the caller sent it: {said}",
        );

        // The two halves apart, each of which this refusal must not reach.
        validate_type(&DeclaredType::new(
            "stay",
            vec![Field::pointing_at("venue", EntityKind::PLACE).needed()],
        ))
        .expect("a narrowed reference that folds newest is an ordinary key");
        validate_type(&DeclaredType::new(
            "snacking",
            vec![Field::summing("donuts").needed()],
        ))
        .expect("and a counter holding a number is what a counter is");
    }

    /// **A declared key is bounded by the same number a written key is.**
    ///
    /// A type names keys and a record carries them, and they are one
    /// namespace: a type allowed to name a key longer than a record may carry
    /// declares a floor no record can ever meet, and the thing that would
    /// refuse the record is a store error rather than this rule.
    ///
    /// The bound was 190 against a column of 191, which is the store deciding
    /// with one character to spare. [`MAX_KEY_CHARS`] is where the decision
    /// lives now.
    ///
    /// Both ends again, because a limit that refused every key would pass on
    /// the long one alone.
    #[test]
    fn a_declared_key_is_bounded_where_a_written_key_is() {
        let refused = validate_type(&DeclaredType::new(
            "snacking",
            vec![Field::required(
                &"k".repeat(MAX_KEY_CHARS + 1),
                ValueType::Text,
            )],
        ))
        .expect_err("a declared key past the limit names a key no record may carry");
        let said = refused.to_string();
        assert!(
            said.contains(&MAX_KEY_CHARS.to_string()),
            "the refusal must carry the limit a caller has to write under: {said}"
        );

        validate_type(&DeclaredType::new(
            "snacking",
            vec![Field::required(&"k".repeat(MAX_KEY_CHARS), ValueType::Text)],
        ))
        .expect("a key at the limit is one a record may carry, so a type may name it");
    }
}
