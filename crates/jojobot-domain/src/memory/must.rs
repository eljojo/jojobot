//! **A rule a type declares for the claims written about its things.**
//!
//! A type may carry a `must`: one saved expression, in the words a view's
//! `where` uses, that every claim's resulting fields have to satisfy. The write
//! is refused when they do not. The engine knows types and expressions and
//! nothing about what any rule is for.
//!
//! **The fields judged are the record's own, both bags**: what the claim
//! labels itself with and what it sets on the thing. A key that holds nothing
//! but blanks counts as absent, so a rule that asks for a key is not met by
//! an empty one. What a value says is not judged: a key proves nothing about
//! a check (rule 619).

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;

use super::MemoryError;
use super::filter::{Filter, Op, Test, expression};
use super::types::DeclaredType;

/// **What a refusal of a rule says that was not met**: the keys the record lacks
/// when it lacks any, else what the rule asks of the keys it holds.
pub(crate) fn reason(missing: &[String], unmet: &[String]) -> String {
    if !missing.is_empty() {
        format!("It carries none of: {}", missing.join(", "))
    } else if !unmet.is_empty() {
        format!("To meet it: {}", unmet.join(" or "))
    } else {
        "It carries every key the rule names.".to_string()
    }
}

/// **Refuse a `must` that does not read**, when it is declared.
pub fn validate_must(must: &str) -> Result<(), MemoryError> {
    expression::parse(must, Date::constant(2000, 1, 1))
        .map(|_| ())
        .map_err(|refused| match refused {
            MemoryError::InvalidQuery(words) => MemoryError::InvalidType(format!(
                "the `must` of a type is a saved expression, and {words}"
            )),
            other => other,
        })
}

/// **What a caller who insists sends**: the token a refusal minted and the
/// written reason, checked by the verb for a reason that counts. The store
/// matches the token itself, against the rules the claim fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Override {
    pub token: String,
    pub reason: String,
}

/// **What judging the rules of the declared types found, besides a refusal.**
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MustVerdict {
    /// The types whose rule could not be read, for the store to report.
    pub unreadable: Vec<String>,
    /// The types whose rule the claim did not meet and a written reason lifted.
    pub overridden: Vec<String>,
}

/// **The token a refusal mints, and a retry hands back.** It answers for the
/// thing, the type and the rule as written, so it lifts no other refusal. It is
/// built by the one function that mints the tokens of the other refusals
/// (rule 472: no norm is a dead end, and one mechanism lifts them).
pub fn override_token(subject: &str, name: &str, rule: &str) -> String {
    super::guard::override_token(
        &super::EntityId(format!("must:{subject}#{name}#{rule}")),
        &[],
    )
}

/// **Judge a record's resulting fields against every rule declared**, and name
/// the types whose rule could not be read.
///
/// `subject` is the thing the claim is about, or the claim's address for an
/// edit. `before` is the record as it stood, and none for a record being made.
/// `named` are the keys the write names. `overriding` is the token and written
/// reason of a caller who insists. `today` resolves a relative day in a rule, and
/// is the caller's day.
///
/// - A record being made is refused when it does not meet a rule.
/// - **An edit is judged on the result, as a floor is.** A record that did not
///   meet a rule before keeps being repairable: it is refused only when the edit
///   names or changes a key the rule names, or when it met the rule before and
///   would no longer.
/// - **A token lifts the rule of the type whose refusal minted it, and the
///   rules refused before it.** The types a claim fails are refused one at a
///   time, in the order the types are held; the token of the second is minted
///   only by the refusal that follows the lifting of the first. A token that
///   answers for none of the rules the claim fails lifts nothing, and the
///   refusal is the first rule's. The lifted types are named in the verdict so
///   the store can say so.
/// - **A rule that cannot be read is not a reason to refuse a write** (rule 342:
///   a store a build filled must keep working). The type is returned for the
///   store to report, and the write goes on.
pub fn guard_must(
    declared: &[DeclaredType],
    subject: &str,
    before: Option<&BTreeMap<String, String>>,
    after: &BTreeMap<String, String>,
    named: &[String],
    overriding: Option<&Override>,
    today: Date,
) -> Result<MustVerdict, MemoryError> {
    let carried = |fields: &BTreeMap<String, String>| -> BTreeMap<String, String> {
        fields
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    let after = carried(after);
    let before = before.map(carried);
    let mut verdict = MustVerdict::default();
    let mut failing: Vec<(&DeclaredType, &str, Filter)> = Vec::new();
    for declaration in declared {
        let Some(rule) = declaration.must.as_deref() else {
            continue;
        };
        let Ok(filter) = expression::parse(rule, today) else {
            verdict.unreadable.push(declaration.name.clone());
            continue;
        };
        if filter.holds(&after) {
            continue;
        }
        if let Some(before) = &before {
            // A record that broke the rule already keeps being repairable: only
            // an edit that names or changes a key the rule names is judged.
            let touched = filter.keys().into_iter().any(|key| {
                named.iter().any(|written| written == key) || before.get(key) != after.get(key)
            });
            if !filter.holds(before) && !touched {
                continue;
            }
        }
        failing.push((declaration, rule, filter));
    }
    // **The rules lifted are those up to the one the token answers for.** A
    // reason of blanks is no reason, so it answers for none.
    let lifted_through = overriding
        .filter(|insisting| !insisting.reason.trim().is_empty())
        .and_then(|insisting| {
            failing.iter().position(|(declaration, rule, _)| {
                insisting.token == override_token(subject, &declaration.name, rule)
            })
        });
    let lifted = lifted_through.map_or(0, |last| last + 1);
    verdict.overridden = failing[..lifted]
        .iter()
        .map(|(declaration, _, _)| declaration.name.clone())
        .collect();
    let Some((declaration, rule, filter)) = failing.get(lifted) else {
        return Ok(verdict);
    };
    let mut missing: Vec<String> = Vec::new();
    for key in filter.keys() {
        if !after.contains_key(key) && !missing.iter().any(|held| held == key) {
            missing.push(key.to_string());
        }
    }
    // **A key that is carried and holds the wrong value is not missing**, so a
    // rule that fails by a value would otherwise name nothing. Only then is the
    // comparison named; a rule failed by an absent key reads as it always did.
    let unmet = if missing.is_empty() {
        unmet_by(filter, &after).into()
    } else {
        Box::default()
    };
    Err(MemoryError::BreaksMust {
        subject: subject.to_string(),
        name: declaration.name.clone(),
        rule: (*rule).to_string(),
        missing,
        unmet,
    })
}

/// **What a refusal asks the caller to send**, in the connectives the rule was
/// written with: the keys joined by "and" where the rule needs all of them and
/// by "or" where it needs one, a group in parentheses where the two meet. Only
/// the keys the record lacks are named, and a part the record cannot be made to
/// meet by sending a key (a negation, a value that is held) is left out.
pub fn needs(rule: &str, missing: &[String]) -> String {
    let Ok(filter) = expression::parse(rule, Date::constant(2000, 1, 1)) else {
        return missing.join(" or ");
    };
    match asked_for(&filter, missing) {
        Some((words, _)) => words,
        None => missing.join(" or "),
    }
}

/// How the words of one part of a rule are joined.
#[derive(Clone, Copy, PartialEq)]
enum Joined {
    One,
    All,
    Either,
}

fn asked_for(filter: &Filter, missing: &[String]) -> Option<(String, Joined)> {
    match filter {
        Filter::Leaf(leaf) => leaf
            .key
            .as_deref()
            .filter(|key| missing.iter().any(|lacking| lacking == key))
            .map(|key| (key.to_string(), Joined::One)),
        Filter::All(children) | Filter::Any(children) => {
            let joined = if matches!(filter, Filter::All(_)) {
                Joined::All
            } else {
                Joined::Either
            };
            let parts: Vec<(String, Joined)> = children
                .iter()
                .filter_map(|child| asked_for(child, missing))
                .collect();
            match parts.len() {
                0 => None,
                1 => parts.into_iter().next(),
                _ => {
                    let word = if joined == Joined::All {
                        " and "
                    } else {
                        " or "
                    };
                    let words = parts
                        .into_iter()
                        .map(|(words, part)| {
                            if part == Joined::One || part == joined {
                                words
                            } else {
                                format!("({words})")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(word);
                    Some((words, joined))
                }
            }
        }
        Filter::Not(_) | Filter::Refs { .. } | Filter::Finished => None,
    }
}

/// **What a rule asks of each key a record carries and holds the wrong value
/// under**, one sentence for each comparison that failed, in the order the rule
/// names them. A comparison under a `!` is read as the rule asks it, so a guard
/// such as `!(purpose = pledge)` is named only when the record fails it. A key the
/// record does not carry is no part of this: the refusal names it as missing.
fn unmet_by(filter: &Filter, fields: &BTreeMap<String, String>) -> Vec<String> {
    let mut asked = Vec::new();
    walk_unmet(filter, true, fields, &mut asked);
    let mut seen = BTreeSet::new();
    asked.retain(|sentence| seen.insert(sentence.clone()));
    asked
}

fn walk_unmet(
    filter: &Filter,
    positive: bool,
    fields: &BTreeMap<String, String>,
    asked: &mut Vec<String>,
) {
    match filter {
        Filter::All(children) | Filter::Any(children) => {
            for child in children {
                walk_unmet(child, positive, fields, asked);
            }
        }
        Filter::Not(inner) => walk_unmet(inner, !positive, fields, asked),
        Filter::Leaf(leaf) => {
            let Some(key) = leaf.key.as_deref() else {
                return;
            };
            let Some(held) = fields.get(key) else {
                return;
            };
            // The leaf meets what the rule asks of it when it holds exactly when it
            // is asked to.
            if Filter::Leaf(leaf.clone()).holds(fields) == positive {
                return;
            }
            asked.push(format!(
                "{key} must {}; it holds {held}",
                what_is_asked(&leaf.test, positive)
            ));
        }
        Filter::Refs { .. } | Filter::Finished => {}
    }
}

/// The words for what a test asks of a value. `positive` is false for a test
/// under a `!`, which asks the opposite.
fn what_is_asked(test: &Test, positive: bool) -> String {
    let (be, not_be) = if positive {
        ("be", "not be")
    } else {
        ("not be", "be")
    };
    match test {
        Test::Exists if positive => "be carried".to_string(),
        Test::Exists => "not be carried".to_string(),
        Test::Equals(value) => format!("{be} {value}"),
        Test::NotEquals(value) => format!("{not_be} {value}"),
        Test::In(values) => format!("{be} one of {}", values.join(", ")),
        Test::NotIn(values) => format!("{not_be} one of {}", values.join(", ")),
        Test::Contains(value) if positive => format!("contain {value}"),
        Test::Contains(value) => format!("not contain {value}"),
        Test::Order(op, _, value) => {
            let word = match (op, positive) {
                (Op::Less, true) | (Op::GreaterOrEqual, false) => "less than",
                (Op::LessOrEqual, true) | (Op::Greater, false) => "at most",
                (Op::Greater, true) | (Op::LessOrEqual, false) => "greater than",
                (Op::GreaterOrEqual, true) | (Op::Less, false) => "at least",
            };
            format!("be {word} {value}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(must: &str) -> Vec<DeclaredType> {
        vec![
            DeclaredType::new(
                "tally",
                vec![super::super::types::Field::new(
                    "a",
                    super::super::types::ValueType::Text,
                )],
            )
            .with_must(must),
        ]
    }

    fn fields(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }

    const DAY: Date = Date::constant(2026, 10, 1);

    #[test]
    fn a_missing_key_is_named_once_however_often_the_rule_names_it() {
        let declared = rule("a || (b && a)");
        let refused = guard_must(
            &declared,
            "thing:kettle",
            None,
            &fields(&[]),
            &[],
            None,
            DAY,
        )
        .expect_err("no key is carried");
        let MemoryError::BreaksMust { missing, .. } = refused else {
            panic!("the wrong refusal");
        };
        assert_eq!(missing, ["a", "b"]);
    }

    #[test]
    fn a_rule_that_does_not_read_is_returned_and_refuses_nothing() {
        let declared = rule("a &&");
        let unreadable = guard_must(
            &declared,
            "thing:kettle",
            None,
            &fields(&[]),
            &[],
            None,
            DAY,
        )
        .expect("a damaged rule never refuses a write");
        assert_eq!(unreadable.unreadable, ["tally"]);
        // The positive: a readable rule is not reported.
        assert!(
            guard_must(
                &rule("a"),
                "thing:kettle",
                None,
                &fields(&[("a", "x")]),
                &[],
                None,
                DAY
            )
            .expect("met")
            .unreadable
            .is_empty()
        );
    }

    fn insisting(subject: &str, name: &str, rule: &str, reason: &str) -> Override {
        Override {
            token: override_token(subject, name, rule),
            reason: reason.to_string(),
        }
    }

    #[test]
    fn a_written_reason_lifts_a_refusal_and_a_blank_one_does_not() {
        let declared = rule("a");
        let lifted = guard_must(
            &declared,
            "thing:kettle",
            None,
            &fields(&[]),
            &[],
            Some(&insisting("thing:kettle", "tally", "a", "because")),
            DAY,
        )
        .expect("a written reason lifts the rule");
        assert_eq!(lifted.overridden, ["tally"]);
        // The negative: a reason of blanks is no reason.
        let blank = guard_must(
            &declared,
            "thing:kettle",
            None,
            &fields(&[]),
            &[],
            Some(&insisting("thing:kettle", "tally", "a", "  ")),
            DAY,
        );
        assert!(matches!(blank, Err(MemoryError::BreaksMust { .. })));
    }

    fn two_rules() -> Vec<DeclaredType> {
        let keyed = |name: &str, must: &str| {
            DeclaredType::new(
                name,
                vec![super::super::types::Field::new(
                    "a",
                    super::super::types::ValueType::Text,
                )],
            )
            .with_must(must)
        };
        vec![keyed("first", "a"), keyed("second", "b")]
    }

    fn refused_name(result: Result<MustVerdict, MemoryError>) -> String {
        match result {
            Err(MemoryError::BreaksMust { name, .. }) => name,
            other => panic!("the claim was expected to be refused: {other:?}"),
        }
    }

    /// **A token lifts the rule of the type whose refusal minted it, and no
    /// other.** A claim that fails two rules is refused for the first; the token
    /// of that refusal moves the refusal to the second, whose own token lands it.
    /// **Paired**: the token of the second rule is not minted before the first is
    /// lifted, and a token that answers for neither lifts nothing.
    #[test]
    fn a_token_lifts_only_the_rule_it_was_minted_for_and_the_next_refusal_has_its_own() {
        let declared = two_rules();
        let judge = |overriding: Option<&Override>| {
            guard_must(
                &declared,
                "thing:kettle",
                None,
                &fields(&[]),
                &[],
                overriding,
                DAY,
            )
        };
        assert_eq!(refused_name(judge(None)), "first");
        let first = insisting("thing:kettle", "first", "a", "the log was lost");
        assert_eq!(refused_name(judge(Some(&first))), "second");
        let second = insisting("thing:kettle", "second", "b", "the log was lost");
        let landed = judge(Some(&second)).expect("the token of the last refusal lands it");
        assert_eq!(landed.overridden, ["first", "second"]);
        // A token for a thing, a type or a rule the claim does not fail lifts nothing.
        for foreign in [
            insisting("thing:delta", "first", "a", "because"),
            insisting("thing:kettle", "third", "a", "because"),
            insisting("thing:kettle", "first", "c", "because"),
        ] {
            assert_eq!(refused_name(judge(Some(&foreign))), "first", "{foreign:?}");
        }
    }

    /// **The refusal asks for all of the keys a rule needs all of, and for one of
    /// those it needs one of**, and groups where the two meet.
    #[test]
    fn the_keys_a_refusal_asks_for_are_joined_as_the_rule_joins_them() {
        let none: Vec<String> = Vec::new();
        for (rule_text, lacking, said) in [
            ("a && b", ["a", "b"].as_slice(), "a and b"),
            ("a || b", ["a", "b"].as_slice(), "a or b"),
            (
                "(a && b) || c",
                ["a", "b", "c"].as_slice(),
                "(a and b) or c",
            ),
            (
                "a && (b || c)",
                ["a", "b", "c"].as_slice(),
                "a and (b or c)",
            ),
            ("a && b", ["b"].as_slice(), "b"),
            (
                "!(purpose = pledge) || witness || receipt_note",
                ["witness", "receipt_note"].as_slice(),
                "witness or receipt_note",
            ),
        ] {
            let lacking: Vec<String> = lacking.iter().map(|key| key.to_string()).collect();
            assert_eq!(needs(rule_text, &lacking), said, "{rule_text}");
        }
        assert_eq!(needs("a &&", &none), "");
    }

    #[test]
    fn a_token_answers_for_the_thing_the_type_and_the_rule() {
        let one = override_token("thing:kettle", "tally", "a");
        assert_eq!(one, override_token("thing:kettle", "tally", "a"));
        assert_ne!(one, override_token("thing:delta", "tally", "a"));
        assert_ne!(one, override_token("thing:kettle", "other", "a"));
        assert_ne!(one, override_token("thing:kettle", "tally", "b"));
    }

    #[test]
    fn an_edit_is_judged_when_it_met_the_rule_before_and_does_not_now() {
        let declared = rule("a");
        let before = fields(&[("a", "x")]);
        let refused = guard_must(
            &declared,
            "thing:kettle",
            Some(&before),
            &fields(&[("a", " ")]),
            &[],
            None,
            DAY,
        );
        assert!(matches!(refused, Err(MemoryError::BreaksMust { .. })));
        // The negative: a record that never met it stays repairable by an edit
        // that does not name or change the key.
        let legacy = fields(&[("b", "1")]);
        let after = fields(&[("b", "2")]);
        assert!(
            guard_must(
                &declared,
                "thing:kettle",
                Some(&legacy),
                &after,
                &[],
                None,
                DAY
            )
            .is_ok()
        );
        // …and is judged once the edit names the key.
        assert!(
            guard_must(
                &declared,
                "thing:kettle",
                Some(&legacy),
                &after,
                &["a".to_string()],
                None,
                DAY
            )
            .is_err()
        );
    }
    /// What a refusal of `rule_text` over `held` names as missing and as unmet.
    fn refused_for(rule_text: &str, held: &[(&str, &str)]) -> (Vec<String>, Vec<String>) {
        let refused = guard_must(
            &rule(rule_text),
            "thing:kettle",
            None,
            &fields(held),
            &[],
            None,
            DAY,
        )
        .expect_err("the rule is not met");
        let MemoryError::BreaksMust { missing, unmet, .. } = refused else {
            panic!("the wrong refusal");
        };
        (missing, unmet.into_vec())
    }

    /// **A rule failed by a value names the key and what the rule asks of it**,
    /// for each comparison a rule can make. No key is missing, so a refusal that
    /// named only missing keys would name nothing.
    #[test]
    fn a_rule_failed_by_a_value_names_the_key_and_what_it_asks_of_it() {
        for (rule_text, held, asked) in [
            (
                "purpose = note",
                &[("purpose", "other")][..],
                "purpose must be note; it holds other",
            ),
            (
                "purpose != draft",
                &[("purpose", "draft")][..],
                "purpose must not be draft; it holds draft",
            ),
            (
                "purpose in (note, memo)",
                &[("purpose", "other")][..],
                "purpose must be one of note, memo; it holds other",
            ),
            (
                "tags contains urgent",
                &[("tags", "calm, slow")][..],
                "tags must contain urgent; it holds calm, slow",
            ),
        ] {
            let (missing, unmet) = refused_for(rule_text, held);
            assert!(missing.is_empty(), "{rule_text}: {missing:?}");
            assert_eq!(unmet, [asked], "{rule_text}");
        }
    }

    /// **Each alternative that failed is named**, and a guard that held says
    /// nothing. **Paired**: a rule failed by an absent key reads as it did, with
    /// the key missing and nothing unmet.
    #[test]
    fn every_failed_alternative_is_named_and_an_absent_key_reads_as_before() {
        let (missing, unmet) = refused_for(
            "purpose = note || status in (open, shut)",
            &[("purpose", "other"), ("status", "ajar")],
        );
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(
            unmet,
            [
                "purpose must be note; it holds other",
                "status must be one of open, shut; it holds ajar"
            ]
        );
        let (missing, unmet) = refused_for("purpose = note", &[]);
        assert_eq!(missing, ["purpose"]);
        assert!(unmet.is_empty(), "{unmet:?}");
        // The guard idiom with every key present and the proof value wrong.
        let (missing, unmet) = refused_for(
            "!(purpose = pledge) || witness = signed",
            &[("purpose", "pledge"), ("witness", "unsigned")],
        );
        assert!(missing.is_empty(), "{missing:?}");
        assert_eq!(
            unmet,
            [
                "purpose must not be pledge; it holds pledge",
                "witness must be signed; it holds unsigned"
            ]
        );
    }
}
