//! "Which run left me this?" — even when it was the first thing that run did.
//!
//! A run's record is made lazily, by its first write. A post that IS the first
//! write used to be stamped before the record existed and so carried no run at
//! all, which made `written_by_other_run` silent for exactly the message a
//! reader would most want to place: the one a fresh run opened with.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_post_that_is_a_runs_first_write_still_names_the_run() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("bot:gamma", "Gamma").await;
    setup.wrap("stood gamma up").await;

    // A fresh run, and the very first thing it does is post.
    let s = story.session().await;
    let first = s
        .post("gamma", "First", "The very first thing this run does.")
        .await;
    let second = s
        .post(
            "gamma",
            "Second",
            "And a second, once the run has its record.",
        )
        .await;
    s.wrap("posted two").await;

    // The reader is another bot's run, so both were written by another run.
    let g = story.as_bot("bot:gamma").await;
    // The positive: the second post, stamped from a record that existed, says so.
    g.call("read_message", json!({"message_id": &second}))
        .await
        .says("written_by_other_run");
    // The first post says so as well.
    g.call("read_message", json!({"message_id": &first}))
        .await
        .says("written_by_other_run");

    g.wrap("read both").await;
    story.finish().await;
}
