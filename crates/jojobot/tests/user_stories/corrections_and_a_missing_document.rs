//! "I asked for the manual and it was not there. Then I found a claim that
//! describes something that never happened. Which way do I put each right?"
//!
//! **A refusal and a teaching decide what a bot does next, so each says what
//! the code does.** Reading a document that does not exist runs nothing and
//! writes nothing, so its refusal opens as a read's does and sends nobody to
//! create the document. The first capture of a session names all three
//! corrections, and the third is a retraction of a claim about an event that
//! never happened.
//!
//! **Paired with the write verb**, whose refusal on a missing thing still says
//! nothing was written: a build that rewrote every opening would pass the
//! read alone.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_read_of_a_missing_document_opens_as_a_read_does() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    let read = s
        .refused("read_document", json!({"document": "document:homer"}))
        .await;
    read.says("Nothing ran")
        .never_says("Nothing was written")
        .never_says("add_entity");

    let write = s
        .refused(
            "capture",
            json!({"subject": "person:milhouse", "content": "bakes on Sundays"}),
        )
        .await;
    write.says("Nothing was written");

    story.finish().await;
}

#[tokio::test]
async fn the_first_capture_of_a_session_names_all_three_corrections() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    let first = s
        .call(
            "capture",
            json!({"subject": "person:milhouse", "content": "bakes on Sundays"}),
        )
        .await;
    let lines = first.teachings().join("\n");
    for verb in ["update_fact", "archive", "retract"] {
        assert!(lines.contains(verb), "{verb} is not taught: {lines}");
    }

    story.finish().await;
}
