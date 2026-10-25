//! "Who read this, who handled it, and what did each of them say?"
//!
//! **A message used to keep only where it stands now.** A second retirement
//! overwrote the first one's notes, and nothing said which run touched it or
//! when. Each change of state is kept now, in order, and read back through
//! `read_message` with `history: true`.
//!
//! ⛔️ **The history read takes no delivery.** A message nobody has taken stays
//! `new` after its history is read, and without the flag the verb delivers as it
//! always did. That pair is what keeps a question about a message from being an
//! act on it.
//!
//! ⚠️ **A message that never changed says so.** An empty history answers with a
//! note, because an empty list reads as *nothing happened* and a message changed
//! before jojobot kept events reads the same.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_reader_can_see_each_change_of_a_message_and_the_run_that_made_it() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("bot:gamma", "Gamma").await;
    setup.wrap("stood gamma up").await;

    let sender = story.session().await;
    let id = sender
        .post("gamma", "The count", "the count on this one is wrong")
        .await;
    sender.wrap("posted").await;

    // ── Two runs of gamma handle it ───────────────────────────────────────────
    let first = story.as_bot("bot:gamma").await;
    first
        .call("read_message", json!({"message_id": &id}))
        .await
        .says("seen_before");
    first
        .call(
            "mark_processed",
            json!({"message_id": &id, "notes": "counted it twice"}),
        )
        .await;
    let first_run = first.run().to_string();
    first.wrap("handled it").await;

    let second = story.as_bot("bot:gamma").await;
    second
        .call(
            "mark_processed",
            json!({"message_id": &id, "notes": "the second count agrees"}),
        )
        .await;
    let second_run = second.run().to_string();
    assert_ne!(
        first_run, second_run,
        "two runs, or the story proves nothing"
    );

    // ── The question ──────────────────────────────────────────────────────────
    let history = second
        .call("read_message", json!({"message_id": &id, "history": true}))
        .await;
    history.number("/history/count", 2);
    let events = history.json()["history"]["events"].clone();
    let kinds: Vec<&str> = (0..2)
        .map(|at| events[at]["kind"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(kinds, vec!["read", "processed"], "{}", history.raw());
    let runs: Vec<&str> = (0..2)
        .map(|at| events[at]["run"].as_str().unwrap_or_default())
        .collect();
    assert_eq!(
        runs,
        vec![first_run.as_str(), first_run.as_str()],
        "each change names the run that made it, and the second run changed nothing: {}",
        history.raw()
    );
    assert_eq!(events[1]["notes"], "counted it twice", "{}", history.raw());
    assert_eq!(events[0]["prior_state"], "new");
    assert_eq!(events[1]["prior_state"], "read");
    assert_eq!(
        history.json()["taken"],
        false,
        "a history read says it took nothing: {}",
        history.raw()
    );

    // The ordinary read answers as it always did.
    let current = second
        .call("read_message", json!({"message_id": &id}))
        .await;
    assert_eq!(current.json()["state"], "processed", "{}", current.raw());
    assert_eq!(
        current.json()["notes"],
        "counted it twice",
        "{}",
        current.raw()
    );
    second.wrap("read it back").await;
    story.finish().await;
}

#[tokio::test]
async fn reading_a_history_takes_no_delivery_and_a_plain_read_still_does() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("bot:gamma", "Gamma").await;
    setup.wrap("stood gamma up").await;
    let sender = story.session().await;
    let id = sender
        .post("gamma", "Untouched", "nobody has opened this")
        .await;
    sender.wrap("posted").await;

    let g = story.as_bot("bot:gamma").await;
    let asked = g
        .call("read_message", json!({"message_id": &id, "history": true}))
        .await;
    asked.number("/history/count", 0);
    assert_eq!(asked.json()["state"], "new", "{}", asked.raw());
    assert!(
        asked.json()["history"]["note"].is_string(),
        "an empty history says what empty means: {}",
        asked.raw()
    );
    // The positive the absence depends on: still unread, so the box counts it new.
    g.call("read_mailbox", json!({"counts_only": true}))
        .await
        .number("/counts/new", 1);

    // Without the flag, delivery happens as it always did.
    g.call("read_message", json!({"message_id": &id}))
        .await
        .says("\"state\":\"read\"");
    let after = g
        .call("read_message", json!({"message_id": &id, "history": true}))
        .await;
    after.number("/history/count", 1);
    assert_eq!(after.json()["history"]["events"][0]["kind"], "read");
    g.wrap("looked").await;
    story.finish().await;
}
