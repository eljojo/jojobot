//! "I made three vaccination records in a row, and the second one was told what
//! was missing too, or none was told at all."
//!
//! **A type's step is said once per type and set of missing keys, whatever the
//! length of the type's name and its keys.** The ledger that remembers what a
//! session was told is a store with a column of fixed width, and a key wider than
//! it is refused and never taught. The story's type has a long name and long
//! keys, so the key the ledger needs is wider than the column.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// The type steps a receipt carries, as `(type, keys to add)`.
fn type_steps(receipt: &Value) -> Vec<(String, Vec<String>)> {
    receipt["unlocks"]["steps"]
        .as_array()
        .map(|steps| {
            steps
                .iter()
                .filter_map(|step| {
                    Some((
                        step["type"].as_str()?.to_string(),
                        step["add"]
                            .as_array()?
                            .iter()
                            .filter_map(|key| key.as_str().map(str::to_string))
                            .collect(),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// A visit, made with only the keys named, so it lacks the rest of the type.
async fn visit(s: &Session, handle: &str, name: &str, with: &[(&str, &str)]) -> Value {
    s.call(
        "add_entity",
        json!({"kind": "thing", "handle": handle, "name": name, "source": "user-named",
               "sets": with.iter().cloned().collect::<std::collections::BTreeMap<_, _>>()}),
    )
    .await
    .json()
}

/// **A long type and key set is told once, the same set again is not, a different
/// long set is told, and a new session is told again.** The type and its keys are
/// long enough that the ledger key built from them is wider than the store's
/// column, so the key is the prefix and a digest of the set. A digest that were
/// the same for every long set would tell the first and never the second.
#[tokio::test]
async fn a_long_type_and_key_set_is_told_once_per_set_and_once_per_session() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.call(
        "declare_type",
        json!({"name": "pet-vaccination", "fields": [
            {"key": "vaccine_name", "holds": "text"},
            {"key": "administered_on", "holds": "text"},
            {"key": "next_due_on", "holds": "text"},
            {"key": "veterinarian_name_and_clinic", "holds": "text"},
        ]}),
    )
    .await;
    let steps_for = |keys: &[&str]| {
        vec![(
            "pet-vaccination".to_string(),
            keys.iter().map(|key| key.to_string()).collect::<Vec<_>>(),
        )]
    };
    let shot = [("vaccine_name", "the annual shot")];

    // The first receipt carries the step whole.
    let first = visit(&s, "thing:delta", "Delta", &shot).await;
    assert_eq!(
        type_steps(&first),
        steps_for(&[
            "administered_on",
            "next_due_on",
            "veterinarian_name_and_clinic"
        ]),
        "{first}"
    );
    // The same set of missing keys again is not told.
    let again = visit(&s, "thing:omega", "Omega", &shot).await;
    assert!(type_steps(&again).is_empty(), "{again}");
    // A different long set of missing keys is told.
    let other = visit(
        &s,
        "thing:sigma",
        "Sigma",
        &[
            ("vaccine_name", "the annual shot"),
            ("administered_on", "today"),
        ],
    )
    .await;
    assert_eq!(
        type_steps(&other),
        steps_for(&["next_due_on", "veterinarian_name_and_clinic"]),
        "{other}"
    );
    // A new session is told the first set again.
    let fresh = story.session_in("UTC", Some("new")).await;
    let renewed = visit(&fresh, "thing:tau", "Tau", &shot).await;
    assert_eq!(
        type_steps(&renewed),
        steps_for(&[
            "administered_on",
            "next_due_on",
            "veterinarian_name_and_clinic"
        ]),
        "{renewed}"
    );
    story.finish().await;
}
