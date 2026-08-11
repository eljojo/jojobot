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
//! **Nothing is rejected and nothing is hidden.** A record matching some of a
//! type's keys comes back, saying which ones it lacks. A value that does not
//! hold what its key was declared to hold comes back too, flagged. Life is
//! messy and the record is the truth; a typed path that refused messy records
//! would be a gate, and this is not one.
//!
//! **No key is registered anywhere.** Keys are scoped by the type that names
//! them, so there is no global list of legal keys and nothing to maintain. Two
//! types may use one key name and mean their own thing by it.

use std::collections::BTreeMap;

use super::{EntityId, MemoryError};

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
    /// Yes or no.
    Boolean,
    /// **Another entity, and a walkable edge rather than a string that looks
    /// like one.** This is what makes a cross-entity question answerable from a
    /// type's own fields.
    Reference,
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
            "boolean" => Some(ValueType::Boolean),
            "reference" => Some(ValueType::Reference),
            _ => None,
        }
    }

    /// **Whether a value holds what this key was declared to hold.**
    ///
    /// Only ever asked so the answer can be reported. Nothing here refuses a
    /// write or drops a record.
    pub fn holds(self, value: &str) -> bool {
        let value = value.trim();
        match self {
            ValueType::Text => true,
            ValueType::Number => value.parse::<f64>().is_ok(),
            ValueType::Boolean => matches!(value, "true" | "false"),
            ValueType::Date => value.parse::<jiff::civil::Date>().is_ok(),
            // A handle, which is what makes it walkable. The kind has to be one
            // this store knows, or it is a string with a colon in it.
            ValueType::Reference => EntityId(value.to_string()).kind().is_some(),
        }
    }
}

/// One key of a type, and what it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    pub key: String,
    pub holds: ValueType,
}

impl Field {
    pub fn new(key: &str, holds: ValueType) -> Field {
        Field {
            key: key.trim().to_string(),
            holds,
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
}

/// A key whose value does not hold what the type said it would.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mistyped {
    pub key: String,
    /// What the type said the key holds.
    pub declared: ValueType,
    /// What the record actually carries, so a reader can see the mistake
    /// rather than being told one happened.
    pub value: String,
}

/// **How a record answers a type.**
///
/// A record that carries none of a type's keys is not a match at all, and the
/// absence of this value is how that is said — otherwise every record matches
/// every type and an answer means nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// The keys the record carries, in the type's own order.
    pub held: Vec<String>,
    /// The keys it does not, by NAME. A count would leave the caller's next
    /// move — fill them, or ignore them — with nothing to act on.
    pub lacking: Vec<String>,
    /// Keys that are there and hold something else. Reported, never a reason
    /// to drop the record.
    pub mistyped: Vec<Mistyped>,
}

impl Match {
    /// Every declared key is there. A complete match still reports its
    /// mistyped keys, because holding all of them badly is not the same as
    /// holding all of them.
    pub fn complete(&self) -> bool {
        self.lacking.is_empty()
    }
}

impl DeclaredType {
    /// The field this type declares under `key`, if it declares one.
    pub fn field(&self, key: &str) -> Option<&Field> {
        let key = key.trim();
        self.fields.iter().find(|f| f.key == key)
    }

    pub fn new(name: &str, fields: Vec<Field>) -> DeclaredType {
        DeclaredType {
            name: name.trim().to_string(),
            fields,
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
        DeclaredType::new(
            &self.name,
            self.fields
                .iter()
                .map(|f| Field::new(&f.key, f.holds))
                .collect(),
        )
    }

    /// **Does this record answer to this type, and how well.**
    ///
    /// Structural: the record is never asked what it was declared to be, only
    /// what it carries. `None` means it carries none of these keys.
    pub fn matched_by(&self, record: &BTreeMap<String, String>) -> Option<Match> {
        let mut held = Vec::new();
        let mut lacking = Vec::new();
        let mut mistyped = Vec::new();
        for field in &self.fields {
            match record.get(&field.key) {
                Some(value) => {
                    held.push(field.key.clone());
                    if !field.holds.holds(value) {
                        mistyped.push(Mistyped {
                            key: field.key.clone(),
                            declared: field.holds,
                            value: value.clone(),
                        });
                    }
                }
                None => lacking.push(field.key.clone()),
            }
        }
        if held.is_empty() {
            return None;
        }
        Some(Match {
            held,
            lacking,
            mistyped,
        })
    }
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
/// This is the ONLY thing that is ever refused here. A record is never checked
/// against a declaration, so nothing on this path can stop a record being
/// written or found.
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
        if seen.contains(&field.key.as_str()) {
            return Err(MemoryError::InvalidType(format!(
                "type '{}' names the key '{}' twice",
                declared.name, field.key
            )));
        }
        seen.push(&field.key);
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
                Field::new("starts", ValueType::Date),
                Field::new("seats", ValueType::Number),
                Field::new("venue", ValueType::Reference),
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
    /// FLAGGED, and the record still matches.**
    ///
    /// The typed path is not a gate. Both halves in one case: the bad value is
    /// reported with what was declared and what is actually there, and the
    /// record it sits on is still a complete match.
    #[test]
    fn a_value_that_fails_its_type_is_flagged_rather_than_dropped() {
        let found = booking()
            .matched_by(&record(&[
                ("starts", "next tuesday"),
                ("seats", "4"),
                ("venue", "place:moes"),
            ]))
            .expect("a record with a bad value still matches");
        assert!(
            found.complete(),
            "a mistyped value is not a missing key: {found:?}",
        );
        assert_eq!(found.mistyped.len(), 1, "{found:?}");
        assert_eq!(found.mistyped[0].key, "starts");
        assert_eq!(found.mistyped[0].declared, ValueType::Date);
        assert_eq!(
            found.mistyped[0].value, "next tuesday",
            "the reader sees the mistake rather than being told one happened",
        );
    }

    /// Each value type, both ways in one case: a check that only ever saw the
    /// good value would pass on a build where everything holds everything.
    #[test]
    fn each_value_type_accepts_its_own_and_refuses_the_rest() {
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

    /// **A key belongs to the type that names it, and to nothing else.** Two
    /// types using one key name each mean their own thing by it, which is what
    /// keeps keys out of any global register.
    #[test]
    fn two_types_may_name_one_key_and_mean_their_own_thing() {
        let seats_as_text =
            DeclaredType::new("seating", vec![Field::new("seats", ValueType::Text)]);
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
}
