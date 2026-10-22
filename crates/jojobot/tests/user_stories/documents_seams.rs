//! "I put sections into a document one at a time and move them about. Every
//! heading must start on a line of its own after a blank line, whatever order the
//! sections were written in."
//!
//! **One function owns the seam between two sections**, and every put and every
//! move goes through it. The cases are the shapes that glued a heading onto the
//! words before it, or left it indented where markdown reads it as code. **Each
//! reads the text back and puts it whole into a second document**, which must
//! split into the same sections: a glued heading folds into the section before it.

use serde_json::json;

use super::dsl::{Session, Story};

/// Put `markdown` into `document`, behind `after` when one is named.
async fn put(session: &Session, document: &str, after: Option<&str>, markdown: &str) {
    let mut args = json!({"document": document, "markdown": markdown});
    if let Some(after) = after {
        args["after"] = json!(after);
    }
    session.call("put_document", args).await;
}

/// The addresses of a document's sections, in the order they were written.
async fn addresses(session: &Session, document: &str) -> Vec<String> {
    session.recall(document).await.json()["objects"][0]["facts"]
        .as_array()
        .expect("a document lists its sections")
        .iter()
        .map(|fact| fact["address"].as_str().expect("an address").to_string())
        .collect()
}

/// The text of a document.
async fn text(session: &Session, document: &str) -> String {
    session
        .call("read_document", json!({"document": document}))
        .await
        .json()["markdown"]
        .as_str()
        .expect("the document reads")
        .to_string()
}

/// **The text read back puts whole into a second document and splits into the
/// sections it was read from.**
async fn assert_resplits(session: &Session, document: &str, sections: usize) {
    let read = text(session, document).await;
    session.add("document:marge", "Fryer notes").await;
    put(session, "document:marge", None, &read).await;
    assert_eq!(
        addresses(session, "document:marge").await.len(),
        sections,
        "the text splits back into the sections it was read from: {read:?}"
    );
    assert_eq!(text(session, "document:marge").await, read);
}

/// **A section moved behind the one that followed it starts on a line of its
/// own, and the section that comes first has no leading blank.** A put gave the
/// second section its blank line as a lead; the move makes it first, and the moved
/// section lands behind words that end with no line feed.
#[tokio::test]
async fn a_move_behind_the_section_that_followed_it_joins_both_ways() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    put(&session, "document:homer", None, "# A\ntext").await;
    let a = addresses(&session, "document:homer").await[0].clone();
    put(&session, "document:homer", Some(&a), "## B\nx").await;
    let b = addresses(&session, "document:homer").await[1].clone();

    session
        .call("update_fact", json!({"address": a, "after": b}))
        .await;
    assert_eq!(
        text(&session, "document:homer").await,
        "## B\nx\n\n# A\ntext"
    );
    assert_resplits(&session, "document:homer", 2).await;
}

/// **A put whose last section ends with no line feed is joined to the section
/// after it.** The section after was written with an empty lead, so the put
/// carries the line feeds on its own tail.
#[tokio::test]
async fn a_put_that_ends_without_a_line_feed_is_joined_to_the_next_section() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    put(&session, "document:homer", None, "# A\ntext\n\n## B\nx").await;
    let a = addresses(&session, "document:homer").await[0].clone();

    put(&session, "document:homer", Some(&a), "## C\ny").await;
    assert_eq!(
        text(&session, "document:homer").await,
        "# A\ntext\n\n## C\ny\n\n## B\nx"
    );
    assert_resplits(&session, "document:homer", 3).await;
}

/// **A tail that ends on spaces does not indent the next heading.** The last
/// section ends with a blank line and four spaces; a heading after it would sit
/// at four spaces, where markdown reads it as code and the section folds into the
/// one before. One more line feed puts the spaces on a line of their own.
#[tokio::test]
async fn a_tail_that_ends_on_spaces_does_not_indent_the_next_heading() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    put(&session, "document:homer", None, "# A\ntext\n\n    ").await;
    let a = addresses(&session, "document:homer").await[0].clone();

    put(&session, "document:homer", Some(&a), "## B\nx").await;
    assert_eq!(
        text(&session, "document:homer").await,
        "# A\ntext\n\n    \n## B\nx"
    );
    assert_resplits(&session, "document:homer", 2).await;
}

/// **A move that closes a gap leaves no heading glued where the section was.**
/// The section taken from between two others leaves them side by side, the first
/// with no line feed at its end and the second with none at its start. The move
/// writes the lead of the second only, and the section moved lands behind the
/// last with its own blank line. **Paired**: a gap that already holds a line feed
/// is left as it was stored.
#[tokio::test]
async fn a_move_that_closes_a_gap_leaves_the_neighbours_joined() {
    let story = Story::begin("bot:gamma").await;
    let session = story.session().await;
    session.add("document:homer", "Fryer manual").await;
    put(&session, "document:homer", None, "# A\ntext").await;
    let a = addresses(&session, "document:homer").await[0].clone();
    put(&session, "document:homer", Some(&a), "## X\nx\n\n").await;
    let x = addresses(&session, "document:homer").await[1].clone();
    put(&session, "document:homer", Some(&x), "## B\nb").await;
    let b = addresses(&session, "document:homer").await[2].clone();
    assert_eq!(
        text(&session, "document:homer").await,
        "# A\ntext\n\n## X\nx\n\n## B\nb"
    );

    session
        .call("update_fact", json!({"address": x, "after": b}))
        .await;
    assert_eq!(
        text(&session, "document:homer").await,
        "# A\ntext\n\n## B\nb\n\n## X\nx\n\n"
    );
    assert_resplits(&session, "document:homer", 3).await;

    // A gap that already holds a line feed is left as stored.
    session.add("document:lisa", "Notes").await;
    put(
        &session,
        "document:lisa",
        None,
        "# A\ntext\n## X\nx\n## B\nb",
    )
    .await;
    let lisa = addresses(&session, "document:lisa").await;
    session
        .call("update_fact", json!({"address": lisa[1], "after": lisa[2]}))
        .await;
    assert_eq!(
        text(&session, "document:lisa").await,
        "# A\ntext\n## B\nb\n\n## X\nx\n"
    );
}
