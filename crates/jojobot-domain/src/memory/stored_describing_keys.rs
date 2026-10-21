//! **A type stored before the keys that describe a record were fixed still
//! answers** (rules 342 and 466). The declaration guard judges what a caller
//! declares; a read never refuses a type already kept.

use super::graph::{GraphQuery, Selection};
use super::search::SearchQuery;
use super::testing::InMemoryMemory;
use super::types::{self, DeclaredType, Field, Fold, ValueType};
use super::*;
use jiff::civil::date;

/// A pantry type as an older build kept it: `key` with a fold of its own, and a
/// plain key beside it.
fn stored_pantry(key: &str, fold: Fold) -> DeclaredType {
    DeclaredType::new(
        "legacy-pantry",
        vec![
            Field {
                folds: fold,
                holds: if fold == Fold::Sum {
                    ValueType::Number
                } else {
                    ValueType::Text
                },
                ..Field::new(key, ValueType::Text)
            },
            Field::new("donuts", ValueType::Number),
        ],
    )
}

/// The build's own declaration of the six, as every startup keeps it.
fn record_labels() -> DeclaredType {
    DeclaredType::shipped(
        "record-labels",
        types::DESCRIBING_KEYS
            .into_iter()
            .map(Field::describing)
            .collect(),
    )
}

async fn bake(memory: &InMemoryMemory, handle: &str, said: &str, fields: &[(&str, &str)]) {
    let id = EntityId(handle.into());
    if !memory
        .list_entities(None)
        .await
        .expect("listing")
        .iter()
        .any(|e| e.id == id)
    {
        memory
            .add_entity(NewEntity::new(id.clone(), handle, "test"))
            .await
            .expect("add ok")
            .written()
            .expect("nothing collides");
    }
    let mut fact = NewFact::about(id, said, date(2026, 8, 1));
    fact.fields = fields
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect();
    fact.sets = fact
        .fields
        .keys()
        .filter(|k| !types::DESCRIBING_KEYS.contains(&k.as_str()))
        .cloned()
        .collect();
    memory
        .capture(fact)
        .await
        .expect("capture ok")
        .written()
        .expect("nothing collides");
}

/// **A query carrying a stored type is never refused for the type**, for each of
/// the six describing keys under each fold a caller could once declare. The
/// pair beside it, a clean stored type, stays valid in the same case.
#[test]
fn a_query_carrying_a_stored_type_with_a_describing_key_is_not_refused() {
    let clean = DeclaredType::new(
        "legacy-clean",
        vec![Field::new("donuts", ValueType::Number)],
    );
    let valid = |declared: &DeclaredType| {
        let search = SearchQuery {
            answers_type: Some(declared.clone()),
            ..Default::default()
        }
        .validate();
        let fits = SearchQuery {
            fits_type: Some(declared.clone()),
            ..Default::default()
        }
        .validate();
        let recall = GraphQuery {
            select: Selection {
                answers_type: Some(declared.clone()),
                ..Selection::default()
            },
            ..GraphQuery::default()
        }
        .validate();
        (search, fits, recall)
    };
    let (a, b, c) = valid(&clean);
    assert!(a.is_ok() && b.is_ok() && c.is_ok(), "{a:?} {b:?} {c:?}");
    for key in types::DESCRIBING_KEYS {
        for fold in [Fold::Newest, Fold::Sum] {
            let (a, b, c) = valid(&stored_pantry(key, fold));
            assert!(a.is_ok(), "search answers_type, {key} as {fold:?}: {a:?}");
            assert!(b.is_ok(), "search fits_type, {key} as {fold:?}: {b:?}");
            assert!(c.is_ok(), "recall answers_type, {key} as {fold:?}: {c:?}");
        }
    }
}

/// **A describing key never folds onto a thing, whatever a stored type says**,
/// and a counter declared on another key still sums, in the same case.
#[tokio::test]
async fn a_stored_counter_on_a_describing_key_does_not_fold_onto_the_thing() {
    for key in types::DESCRIBING_KEYS {
        let memory = InMemoryMemory::booted();
        memory.stored_type_past_the_guard(record_labels());
        memory.stored_type_past_the_guard(stored_pantry(key, Fold::Sum));
        memory.stored_type_past_the_guard(DeclaredType::new(
            "legacy-donuts",
            vec![Field::summing("donuts")],
        ));
        let handle = "thing:tau".to_string();
        for _ in 0..3 {
            bake(&memory, &handle, "ate some", &[(key, "1"), ("donuts", "1")]).await;
        }
        // A write that sets the key on the thing is refused, as it is for the key
        // under no stored type at all.
        let mut sets = NewFact::about(EntityId(handle.clone()), "set it", date(2026, 8, 2));
        sets.fields = [(key.to_string(), "1".to_string())].into_iter().collect();
        sets.sets = [key.to_string()].into_iter().collect();
        assert!(memory.capture(sets).await.is_err(), "{key} cannot be set");
        let fields = memory
            .fields(&EntityId(handle.clone()))
            .await
            .expect("fields reads");
        assert!(
            !fields.contains_key(key),
            "{key} stays on its record: {fields:?}"
        );
        assert_eq!(fields["donuts"], "3", "{key}: {fields:?}");
    }
}

/// **The fold is the key's own before any declaration is asked.**
#[test]
fn a_describing_key_folds_as_describing_over_any_stored_declaration() {
    for key in types::DESCRIBING_KEYS {
        let stored = [record_labels(), stored_pantry(key, Fold::Sum)];
        assert_eq!(types::fold_of(key, &stored), Fold::Describes, "{key}");
    }
    let counted = [DeclaredType::new(
        "legacy-donuts",
        vec![Field::summing("donuts")],
    )];
    assert_eq!(types::fold_of("donuts", &counted), Fold::Sum);
}

/// **A declaration of a describing key is still refused, naming all six**, and
/// the re-declaration of a stored type without the key lands.
#[tokio::test]
async fn declaring_a_describing_key_is_refused_and_a_stored_type_redeclared_without_it_lands() {
    let memory = InMemoryMemory::booted();
    for key in types::DESCRIBING_KEYS {
        for fold in [Fold::Newest, Fold::Sum] {
            let refused = memory
                .declare_type(stored_pantry(key, fold), None)
                .await
                .expect_err("a describing key keeps its fold");
            let said = refused.to_string();
            assert!(
                types::DESCRIBING_KEYS.iter().all(|k| said.contains(k)),
                "{key} as {fold:?} is refused without naming all six: {said}"
            );
        }
    }
    memory.stored_type_past_the_guard(stored_pantry("subject", Fold::Newest));
    let redeclared = DeclaredType::new(
        "legacy-pantry",
        vec![Field::new("donuts", ValueType::Number)],
    );
    memory
        .declare_type(redeclared, None)
        .await
        .expect("the stored type is re-declared without the key");
    let held = memory.declared_types().await.expect("types read");
    let pantry = held
        .iter()
        .find(|t| t.name == "legacy-pantry")
        .expect("kept");
    assert!(pantry.field("subject").is_none(), "{pantry:?}");
    memory
        .declare_type(
            DeclaredType::new("donut-count", vec![Field::summing("donuts")]),
            None,
        )
        .await
        .expect("a counter on donuts lands");
}
