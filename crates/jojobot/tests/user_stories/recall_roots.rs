//! "Where does the tree start?"
//!
//! The assistant asks recall for the roots: the things that sit under nothing
//! standing. A thing filed under a standing parent is not one. A thing whose
//! parent was archived is, because nothing standing answers to that parent any
//! more, and it comes back to the tree the day the parent does.

use serde_json::{Value, json};

use super::dsl::Story;

const BOARD: &str = "project:atlas";
const CARD: &str = "work:sigma";
const CHILD_CARD: &str = "work:tau";

fn ids(answer: &Value) -> Vec<String> {
    answer["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("the answer lists objects: {answer}"))
        .iter()
        .map(|object| object["id"].as_str().expect("an id").to_string())
        .collect()
}

#[tokio::test]
async fn the_roots_are_the_things_under_nothing_standing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(BOARD, "Roots story board").await;
    for (handle, parent) in [(CARD, BOARD), (CHILD_CARD, CARD)] {
        let (kind, slug) = handle.split_once(':').expect("a handle");
        s.call(
            "add_entity",
            json!({"kind": kind, "handle": slug, "name": handle, "source": "user-named",
                   "parent": parent}),
        )
        .await;
    }

    let roots = s.call("recall", json!({"roots": true})).await.json();
    let found = ids(&roots);
    assert!(found.contains(&BOARD.to_string()), "{found:?}");
    assert!(
        !found.contains(&CARD.to_string()) && !found.contains(&CHILD_CARD.to_string()),
        "a child of a standing thing is not a root: {found:?}"
    );

    // Roots narrows beside a kind: no work item here sits under nothing.
    let work = s
        .call("recall", json!({"roots": true, "kind": "work"}))
        .await
        .json();
    assert!(
        !ids(&work).contains(&CARD.to_string()),
        "a kind narrows the roots further: {work}"
    );

    // The board is archived, so nothing standing answers to it, and its card is
    // a root. The card's own child still sits under a standing thing.
    s.call(
        "archive_entity",
        json!({"handle": BOARD, "reason": "story: the parent goes"}),
    )
    .await;
    let after = ids(&s.call("recall", json!({"roots": true})).await.json());
    assert!(after.contains(&CARD.to_string()), "{after:?}");
    assert!(!after.contains(&BOARD.to_string()), "{after:?}");
    assert!(!after.contains(&CHILD_CARD.to_string()), "{after:?}");
}
