//! "Two of us take turns at the chore. Where does whose go it was belong?"
//!
//! **The measured failure this story is the reachability check for:** told two
//! people take turns, a cold agent records whose go it was as a key on the thing
//! and rewrites it each sitting, so no later sitting can read whose turn is next.
//! A list with tick back keeps that.
//!
//! **The signal is the key coming back to an earlier person.** One change of
//! person is ordinary, so the receipt is silent until the key names a person it
//! held before and does not hold now.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_key_that_comes_back_to_an_earlier_person_is_told_where_turns_live() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("thing:the-furnace", "The furnace").await;
    s.add("person:homer", "Homer").await;
    s.add("person:bart", "Bart").await;

    let mut hints = Vec::new();
    for who in ["person:homer", "person:bart", "person:homer"] {
        let write = s
            .call(
                "capture",
                json!({
                    "subject": "thing:the-furnace",
                    "content": "whose go it was to change the filter",
                    "sets": { "last_changed_by": who },
                }),
            )
            .await;
        assert_eq!(write.json()["success"], true, "{}", write.json());
        hints.push(write.json().get("hint").cloned());
    }

    // The first two writes are an ordinary change of person.
    assert_eq!(hints[0], None, "{hints:?}");
    assert_eq!(hints[1], None, "{hints:?}");
    // The third comes back to Homer: the receipt names the list.
    let line = hints[2]
        .as_ref()
        .and_then(|hint| hint.as_str())
        .unwrap_or_else(|| panic!("the third write carries a hint: {hints:?}"));
    assert!(line.contains("list"), "{line}");
}
