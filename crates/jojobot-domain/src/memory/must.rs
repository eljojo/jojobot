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
use super::filter::expression;
use super::types::DeclaredType;

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

/// **Judge a record's resulting fields against every rule declared**, and name
/// the types whose rule could not be read.
///
/// `before` is the record as it stood, and none for a record being made. `named`
/// are the keys the write names. `today` resolves a relative day in a rule.
///
/// - A record being made is refused when it does not meet a rule.
/// - **An edit is judged on the result, as a floor is.** A record that did not
///   meet a rule before keeps being repairable: it is refused only when the edit
///   names or changes a key the rule names, or when it met the rule before and
///   would no longer.
/// - **A rule that cannot be read is not a reason to refuse a write** (rule 342:
///   a store a build filled must keep working). The type is returned for the
///   store to report, and the write goes on.
pub fn guard_must(
    declared: &[DeclaredType],
    before: Option<&BTreeMap<String, String>>,
    after: &BTreeMap<String, String>,
    named: &[String],
    today: Date,
) -> Result<Vec<String>, MemoryError> {
    let carried = |fields: &BTreeMap<String, String>| -> BTreeMap<String, String> {
        fields
            .iter()
            .filter(|(_, value)| !value.trim().is_empty())
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    };
    let after = carried(after);
    let before = before.map(carried);
    let mut unreadable = Vec::new();
    for declaration in declared {
        let Some(rule) = declaration.must.as_deref() else {
            continue;
        };
        let Ok(filter) = expression::parse(rule, today) else {
            unreadable.push(declaration.name.clone());
            continue;
        };
        if filter.holds(&after) {
            continue;
        }
        let asked: BTreeSet<&str> = filter.keys().into_iter().collect();
        if let Some(before) = &before {
            // A record that broke the rule already keeps being repairable: only
            // an edit that names or changes a key the rule names is judged.
            let touched = asked.iter().any(|key| {
                named.iter().any(|written| written == key) || before.get(*key) != after.get(*key)
            });
            if !filter.holds(before) && !touched {
                continue;
            }
        }
        let mut missing: Vec<String> = Vec::new();
        for key in filter.keys() {
            if !after.contains_key(key) && !missing.iter().any(|held| held == key) {
                missing.push(key.to_string());
            }
        }
        return Err(MemoryError::BreaksMust {
            name: declaration.name.clone(),
            rule: rule.to_string(),
            missing,
        });
    }
    Ok(unreadable)
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
        let refused =
            guard_must(&declared, None, &fields(&[]), &[], DAY).expect_err("no key is carried");
        let MemoryError::BreaksMust { missing, .. } = refused else {
            panic!("the wrong refusal");
        };
        assert_eq!(missing, ["a", "b"]);
    }

    #[test]
    fn a_rule_that_does_not_read_is_returned_and_refuses_nothing() {
        let declared = rule("a &&");
        let unreadable = guard_must(&declared, None, &fields(&[]), &[], DAY)
            .expect("a damaged rule never refuses a write");
        assert_eq!(unreadable, ["tally"]);
        // The positive: a readable rule is not reported.
        assert!(
            guard_must(&rule("a"), None, &fields(&[("a", "x")]), &[], DAY)
                .expect("met")
                .is_empty()
        );
    }

    #[test]
    fn an_edit_is_judged_when_it_met_the_rule_before_and_does_not_now() {
        let declared = rule("a");
        let before = fields(&[("a", "x")]);
        let refused = guard_must(&declared, Some(&before), &fields(&[("a", " ")]), &[], DAY);
        assert!(matches!(refused, Err(MemoryError::BreaksMust { .. })));
        // The negative: a record that never met it stays repairable by an edit
        // that does not name or change the key.
        let legacy = fields(&[("b", "1")]);
        let after = fields(&[("b", "2")]);
        assert!(guard_must(&declared, Some(&legacy), &after, &[], DAY).is_ok());
        // …and is judged once the edit names the key.
        assert!(guard_must(&declared, Some(&legacy), &after, &["a".to_string()], DAY).is_err());
    }
}
