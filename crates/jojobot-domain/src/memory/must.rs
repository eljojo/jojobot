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
/// `named` are the keys the write names. `overriding` is the written reason of a
/// caller who insists, after its verb checked the token. `today` resolves a
/// relative day in a rule.
///
/// - A record being made is refused when it does not meet a rule.
/// - **An edit is judged on the result, as a floor is.** A record that did not
///   meet a rule before keeps being repairable: it is refused only when the edit
///   names or changes a key the rule names, or when it met the rule before and
///   would no longer.
/// - **A caller who insists lands the write**, and the type is named in the
///   verdict so the store can say so.
/// - **A rule that cannot be read is not a reason to refuse a write** (rule 342:
///   a store a build filled must keep working). The type is returned for the
///   store to report, and the write goes on.
pub fn guard_must(
    declared: &[DeclaredType],
    subject: &str,
    before: Option<&BTreeMap<String, String>>,
    after: &BTreeMap<String, String>,
    named: &[String],
    overriding: Option<&str>,
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
        if overriding.is_some_and(|reason| !reason.trim().is_empty()) {
            verdict.overridden.push(declaration.name.clone());
            continue;
        }
        let mut missing: Vec<String> = Vec::new();
        for key in filter.keys() {
            if !after.contains_key(key) && !missing.iter().any(|held| held == key) {
                missing.push(key.to_string());
            }
        }
        return Err(MemoryError::BreaksMust {
            subject: subject.to_string(),
            name: declaration.name.clone(),
            rule: rule.to_string(),
            missing,
        });
    }
    Ok(verdict)
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

    #[test]
    fn a_written_reason_lifts_a_refusal_and_a_blank_one_does_not() {
        let declared = rule("a");
        let lifted = guard_must(
            &declared,
            "thing:kettle",
            None,
            &fields(&[]),
            &[],
            Some("because"),
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
            Some("  "),
            DAY,
        );
        assert!(matches!(blank, Err(MemoryError::BreaksMust { .. })));
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
}
