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
