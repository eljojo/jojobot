//! **Two rules of the type registry, asked of the rule alone**: whether a
//! declaration may land on what a store holds under its name, and what keys a
//! kind holds once a declaration lands. Both stores call these two functions;
//! the contract cases that reach them through a store are not the only proof.
//! Tables pin variants, keys and order, never refusal prose (rule 665).

use super::*;

fn declared(name: &str, origin: Origin) -> DeclaredType {
    let mut declared = DeclaredType::new(name, vec![Field::new("note", ValueType::Text)]);
    declared.origin = origin;
    declared
}

/// **A caller's declaration may not land on a type the software ships**; every
/// other pairing lands. The name is reported trimmed, and no displaced record
/// is attached here: the store that meets the refusal attaches it.
#[test]
fn only_a_callers_declaration_over_a_shipped_type_is_refused() {
    //   (incoming, held, refused)
    let table = [
        (Origin::Declared, Some(Origin::Shipped), true),
        (Origin::Declared, Some(Origin::Declared), false),
        (Origin::Declared, None, false),
        (Origin::Shipped, Some(Origin::Shipped), false),
        (Origin::Shipped, Some(Origin::Declared), false),
        (Origin::Shipped, None, false),
    ];
    for (incoming, held, refused) in table {
        let outcome = guard_replacement(&declared("snacking", incoming), held);
        assert_eq!(
            matches!(outcome, Err(MemoryError::ShippedType { .. })),
            refused,
            "{incoming:?} over {held:?}: {outcome:?}"
        );
        assert!(outcome.is_ok() || refused, "{outcome:?}");
    }
    let named = guard_replacement(
        &declared("  snacking  ", Origin::Declared),
        Some(Origin::Shipped),
    );
    assert!(
        matches!(
            &named,
            Err(MemoryError::ShippedType { name, displaced: None }) if name == "snacking"
        ),
        "{named:?}"
    );
}

fn key(name: &str) -> Field {
    Field::new(name, ValueType::Text)
}

fn held(rows: &[(&str, Origin)]) -> Vec<HeldKindKey> {
    rows.iter()
        .map(|(key, origin)| HeldKindKey {
            key: key.to_string(),
            origin: *origin,
        })
        .collect()
}

/// The plan as `(key, origin, written)` triples, which is all a store reads.
fn rows(plan: &KindKeysPlan) -> Vec<(String, Origin, bool)> {
    plan.rows
        .iter()
        .map(|row| (row.key.clone(), row.origin, row.written.is_some()))
        .collect()
}

fn expected(list: &[(&str, Origin, bool)]) -> Vec<(String, Origin, bool)> {
    list.iter()
        .map(|(key, origin, written)| (key.to_string(), *origin, *written))
        .collect()
}

/// **A shipped declaration (the start) replaces the shipped keys with the
/// build's list and keeps the caller's additions, in the order they were added,
/// after it.** A key the build now ships that a caller had added is replaced by
/// the build's, and named so the store can report it. A start never refuses.
#[test]
fn a_shipped_declaration_replaces_shipped_keys_and_keeps_the_callers_after_them() {
    use Origin::{Declared, Shipped};
    //   (build's keys, held now, plan rows, replaced additions)
    type Case = (
        Vec<&'static str>,
        Vec<(&'static str, Origin)>,
        Vec<(&'static str, Origin, bool)>,
        Vec<&'static str>,
    );
    let table: Vec<Case> = vec![
        // First start: nothing held.
        (
            vec!["a", "b"],
            vec![],
            vec![("a", Shipped, true), ("b", Shipped, true)],
            vec![],
        ),
        // A key the build added appears, one it dropped goes.
        (
            vec!["a", "c"],
            vec![("a", Shipped), ("b", Shipped)],
            vec![("a", Shipped, true), ("c", Shipped, true)],
            vec![],
        ),
        // A caller's additions stay after the build's, in the order held.
        (
            vec!["a"],
            vec![("a", Shipped), ("x", Declared), ("w", Declared)],
            vec![
                ("a", Shipped, true),
                ("x", Declared, false),
                ("w", Declared, false),
            ],
            vec![],
        ),
        // A caller's key the build now ships cannot stay.
        (
            vec!["a", "x"],
            vec![("a", Shipped), ("x", Declared), ("w", Declared)],
            vec![
                ("a", Shipped, true),
                ("x", Shipped, true),
                ("w", Declared, false),
            ],
            vec!["x"],
        ),
    ];
    for (build, now, rows_expected, replaced) in table {
        let fields: Vec<Field> = build.iter().map(|k| key(k)).collect();
        let plan =
            plan_kind_keys("pet", Shipped, &fields, &held(&now)).expect("a start never refuses");
        assert_eq!(
            rows(&plan),
            expected(&rows_expected),
            "build {build:?} over {now:?}"
        );
        assert_eq!(
            plan.replaced_additions,
            replaced.iter().map(|k| k.to_string()).collect::<Vec<_>>(),
            "build {build:?} over {now:?}"
        );
    }
}

/// **A caller's declaration keeps every shipped key, replaces its own earlier
/// additions with its list, and is refused when it names a shipped key.**
/// Redeclaring the same additions is a no-op in effect; declaring none drops
/// the caller's earlier ones. The refusal is an entity refusal, whichever key
/// of the list clashes.
#[test]
fn a_callers_declaration_replaces_only_its_own_additions_and_never_a_shipped_key() {
    use Origin::{Declared, Shipped};
    let now = held(&[("a", Shipped), ("b", Shipped), ("x", Declared)]);
    //   (added keys, plan rows)
    let table = [
        (
            vec!["y"],
            vec![
                ("a", Shipped, false),
                ("b", Shipped, false),
                ("y", Declared, true),
            ],
        ),
        (
            vec!["x", "y"],
            vec![
                ("a", Shipped, false),
                ("b", Shipped, false),
                ("x", Declared, true),
                ("y", Declared, true),
            ],
        ),
        (vec![], vec![("a", Shipped, false), ("b", Shipped, false)]),
    ];
    for (added, rows_expected) in table {
        let fields: Vec<Field> = added.iter().map(|k| key(k)).collect();
        let plan = plan_kind_keys("pet", Declared, &fields, &now).expect("lands");
        assert_eq!(rows(&plan), expected(&rows_expected), "adding {added:?}");
        assert!(plan.replaced_additions.is_empty(), "adding {added:?}");
    }
    for clash in [vec!["a"], vec!["y", "b"], vec!["x", "a", "y"]] {
        let fields: Vec<Field> = clash.iter().map(|k| key(k)).collect();
        let outcome = plan_kind_keys("pet", Declared, &fields, &now);
        assert!(
            matches!(outcome, Err(MemoryError::InvalidEntity(_))),
            "{clash:?}: {outcome:?}"
        );
    }
    // With no shipped key held, the same names land.
    let free = held(&[("x", Declared)]);
    let plan = plan_kind_keys("pet", Declared, &[key("a")], &free).expect("lands");
    assert_eq!(rows(&plan), expected(&[("a", Declared, true)]));
}
