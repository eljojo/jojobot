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

/// A visit, made with only the first key of the type.
async fn visit(s: &Session, handle: &str, name: &str) -> Value {
    s.call(
        "add_entity",
        json!({"kind": "thing", "handle": handle, "name": name, "source": "user-named",
               "sets": {"vaccine_name": "the annual shot"}}),
    )
    .await
    .json()
}

/// A long type name with long keys is told once, on the first receipt.
#[tokio::test]
async fn a_type_with_a_long_name_and_long_keys_is_told_once() {
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

    // The first receipt carries the step whole, and the type and its keys are
    // long enough that the ledger key built from them is wider than 64 characters.
    let first = visit(&s, "thing:delta", "Delta").await;
    let steps = type_steps(&first);
    assert_eq!(
        steps,
        vec![(
            "pet-vaccination".to_string(),
            vec![
                "administered_on".to_string(),
                "next_due_on".to_string(),
                "veterinarian_name_and_clinic".to_string(),
            ]
        )],
        "{first}"
    );
    let width =
        "type-unlocks:pet-vaccination:administered_on,next_due_on,veterinarian_name_and_clinic";
    assert!(
        width.chars().count() > 64,
        "the story's key is wider than the column"
    );

    // The next visit lacks the same keys and is not told again.
    let second = visit(&s, "thing:omega", "Omega").await;
    assert!(type_steps(&second).is_empty(), "{second}");
    story.finish().await;
}
