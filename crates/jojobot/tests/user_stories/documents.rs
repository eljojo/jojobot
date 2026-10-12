//! "I keep a manual as one markdown file. I want to put it in whole, read it back
//! whole, and change one part without touching the rest."
//!
//! **A document is held by section.** `put_document` splits a file at its
//! headings and stores each heading with what sits under it as one claim, so a
//! part can be edited, cited and read the history of on its own. `read_document`
//! returns the file byte for byte.
//!
//! **The negatives are the half that makes the positive mean something.** An
//! edit to one part leaves every other part's bytes where they were, and a second
//! put with no place to go is refused rather than writing the text twice.

use serde_json::json;

use super::dsl::Story;

const MANUAL: &str = "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Closing\n\nDrain the oil and wipe the lid.\n";

/// **The file put is the file read, and one edited part moves alone.**
#[tokio::test]
async fn a_manual_put_whole_is_read_back_whole_and_an_edit_moves_one_part() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;

    let put = session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await
        .json();
    assert_eq!(put["sections"], 3, "{put}");

    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(read["markdown"], MANUAL, "{read}");
    assert_eq!(read["name"], "Fryer manual");

    // The second section is the opening. Its address is what a claim cites and
    // what an edit goes through.
    let recalled = session.recall("document:homer").await;
    let address = recalled.json()["objects"][0]["facts"][1]["address"]
        .as_str()
        .expect("a section has an address")
        .to_string();
    session
        .call(
            "update_fact",
            json!({
                "address": address, "field": "details",
                "find": "at six", "replace": "at five",
            }),
        )
        .await;

    let edited = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        edited["markdown"],
        MANUAL.replace("at six", "at five"),
        "only the opening changed: {edited}"
    );
}

/// **A second put with no place to go is refused, and one that names a place
/// lands there.**
#[tokio::test]
async fn a_second_put_must_say_where_it_goes() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;

    let refused = session
        .refused(
            "put_document",
            json!({"document": "document:homer", "markdown": "## Cleaning\n\nOnce a week.\n"}),
        )
        .await;
    refused.says("after").says("update_fact");

    let opening =
        session.recall("document:homer").await.json()["objects"][0]["facts"][1]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string();
    session
        .call(
            "put_document",
            json!({
                "document": "document:homer", "after": opening,
                "markdown": "## Cleaning\n\nOnce a week.\n\n",
            }),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Opening\n\nSwitch the fryer on at six.\n\n## Cleaning\n\nOnce a week.\n\n## Closing\n\nDrain the oil and wipe the lid.\n",
    );
}

/// **A part moves to where it belongs, and the rest of the manual does not
/// move.** One call names the section it should follow; the page reads in the
/// new order and the sections that stayed are the ones put.
#[tokio::test]
async fn a_part_is_moved_behind_another_with_one_call() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let facts = session.recall("document:homer").await.json();
    let address_of = |n: usize| {
        facts["objects"][0]["facts"][n]["address"]
            .as_str()
            .expect("a section has an address")
            .to_string()
    };
    let (opening, closing) = (address_of(1), address_of(2));

    session
        .call(
            "update_fact",
            json!({"address": closing, "after": address_of(0)}),
        )
        .await;
    let read = session
        .call("read_document", json!({"document": "document:homer"}))
        .await
        .json();
    assert_eq!(
        read["markdown"],
        "# Fryer manual\n\nRead this before the first shift.\n\n## Closing\n\nDrain the oil and wipe the lid.\n## Opening\n\nSwitch the fryer on at six.\n\n",
    );

    // An ordinary claim has no place to move, and the refusal says what
    // `after` is for.
    let remark = session
        .fact("document:homer", "reviewed by a colleague")
        .await;
    session
        .refused("update_fact", json!({"address": remark, "after": opening}))
        .await
        .says("after");
}

/// **A search for a word in a manual lands on the part that holds it, and says
/// which manual that is.** Two parts of one manual match: they arrive together,
/// each naming the document.
#[tokio::test]
async fn a_search_lands_on_a_part_and_names_the_manual() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    session
        .call(
            "put_document",
            json!({"document": "document:homer", "markdown": MANUAL}),
        )
        .await;
    let found = session
        .call("search", json!({"query": "fryer"}))
        .await
        .json();
    let results = found["results"].as_array().expect("results");
    let parts: Vec<&serde_json::Value> = results
        .iter()
        .filter(|hit| hit["document"]["handle"] == "document:homer")
        .collect();
    assert!(!parts.is_empty(), "a part of the manual was found: {found}");
    assert!(
        parts
            .iter()
            .all(|hit| hit["document"]["name"] == "Fryer manual"),
        "{found}"
    );
    assert!(
        parts.iter().any(|hit| hit["content"] == "## Opening"),
        "the part that mentions the fryer is the opening: {found}"
    );
}
