//! "I made twenty chores in a row, and every receipt told me the same thing."
//!
//! **A type's step says what a thing lacks to answer the type, and the type is the
//! subject of the step.** A session that creates a run of things of one type missing
//! the same keys is told once, on the first receipt. A thing missing DIFFERENT keys
//! gets its own step, and a new session is told again. Each case reads the receipt
//! that has the step and the one that does not in the same story.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

/// Greek letters, so no two handles are near-misses of one another.
const LETTERS: [&str; 8] = [
    "alpha", "omega", "sigma", "kappa", "lambda", "epsilon", "theta", "zeta",
];

/// A chore, made with the keys given. `n` picks the letter.
async fn chore(s: &Session, n: usize, sets: Value) -> Value {
    let letter = LETTERS[n - 1];
    s.call(
        "add_entity",
        json!({"kind": "thing", "handle": letter, "name": format!("Chore {letter}"),
               "source": "user-named", "sets": sets}),
    )
    .await
    .json()
}

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

async fn declare_the_type(s: &Session) {
    s.call(
        "declare_type",
        json!({"name": "chore-item", "fields": [
            {"key": "title", "holds": "text"},
            {"key": "owner", "holds": "text"},
            {"key": "done_by", "holds": "date"},
        ]}),
    )
    .await;
}

/// The step a first receipt carries, whole: the type, the keys to add.
fn the_step() -> Vec<(String, Vec<String>)> {
    vec![(
        "chore-item".to_string(),
        vec!["owner".to_string(), "done_by".to_string()],
    )]
}

/// A run of things of one type missing the same keys is told once, then a thing
/// missing other keys is told for itself.
#[tokio::test]
async fn a_run_of_things_missing_the_same_keys_is_told_once_and_other_keys_are_told_again() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    declare_the_type(&s).await;

    // ── (1) the first receipt carries the step whole ────────────────────────
    let first = chore(&s, 1, json!({"title": "sweep the porch"})).await;
    assert_eq!(type_steps(&first), the_step(), "{first}");

    // ── (2) the next four carry none ────────────────────────────────────────
    for n in 2..=5 {
        let later = chore(&s, n, json!({"title": format!("chore number {n}")})).await;
        assert!(
            type_steps(&later).is_empty(),
            "chore {n} missing the same keys is not told again: {later}"
        );
    }

    // ── (3) a thing missing DIFFERENT keys is told for itself ───────────────
    let other = chore(
        &s,
        6,
        json!({"title": "wash the car", "owner": "the neighbours"}),
    )
    .await;
    assert_eq!(
        type_steps(&other),
        vec![("chore-item".to_string(), vec!["done_by".to_string()])],
        "{other}"
    );
    let same_again = chore(&s, 7, json!({"title": "mow", "owner": "the neighbours"})).await;
    assert!(
        type_steps(&same_again).is_empty(),
        "the second thing missing only done_by is not told again: {same_again}"
    );

    story.finish().await;
}

/// A new session is told again on its first create, whatever an earlier one was told.
#[tokio::test]
async fn a_new_session_is_told_again_on_its_first_create() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    declare_the_type(&s).await;
    let first = chore(&s, 1, json!({"title": "sweep the porch"})).await;
    assert_eq!(type_steps(&first), the_step(), "{first}");
    let second = chore(&s, 2, json!({"title": "dust"})).await;
    assert!(type_steps(&second).is_empty(), "{second}");

    let next = story.session_in("UTC", Some("new")).await;
    let told_again = chore(&next, 3, json!({"title": "water the plants"})).await;
    assert_eq!(
        type_steps(&told_again),
        the_step(),
        "a new session is told on its first create: {told_again}"
    );
    story.finish().await;
}
