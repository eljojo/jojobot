//! "One card's record is damaged. Does my list of cards tell me so, or does it
//! just look short?"
//!
//! A read of a kind serves the things it can read and names each one it could
//! not, with the reason and who can mend it. Without the name, a short list
//! reads as the whole shelf.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_recall_over_a_kind_names_the_card_it_cannot_read_and_serves_the_rest() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let s = story.session().await;
    s.add("work:omega", "Mow the lawn").await;

    // ── nothing is damaged: the card comes back, and no `unreadable` key ─────
    let whole = s.call("recall", json!({"kind": "work"})).await.json();
    assert_eq!(
        whole["objects"].as_array().map(Vec::len),
        Some(1),
        "{whole}"
    );
    assert!(whole.get("unreadable").is_none(), "{whole}");

    // ── a second card is written while its record cannot be read back ───────
    store.damages_the_record_of("work:sigma");
    s.add("work:sigma", "Paint the fence").await;
    let short = s.call("recall", json!({"kind": "work"})).await.json();
    let handles: Vec<&str> = short["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .filter_map(|object| object["id"].as_str())
        .collect();
    assert_eq!(handles, ["work:omega"], "{short}");
    let named = short["unreadable"]
        .as_array()
        .expect("unreadable is a list");
    assert_eq!(named.len(), 1, "{short}");
    assert_eq!(named[0]["handle"], "work:sigma", "{short}");
    assert_eq!(named[0]["fix_by"], "person", "{short}");
    assert!(
        named[0]["unreadable"]
            .as_str()
            .is_some_and(|why| !why.is_empty()),
        "the reason is given: {short}"
    );

    s.wrap("read a shelf with one damaged card").await;
    story.finish().await;
}
