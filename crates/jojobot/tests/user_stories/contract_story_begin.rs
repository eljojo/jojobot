//! **This file is a contract, not a story.** It pins what the story harness sets
//! up when a story begins. Nothing here is a use case in a person's words: the
//! cases hold that the one-call start opens exactly the session that the two-call
//! start opens, so a story that begins either way meets the same instance.

use serde_json::json;

use super::dsl::{Session, Story};

/// How many times the bot has booted, as the week's usage record of the bot
/// counts the calls of `start_here`. Each session a story opens is one boot.
async fn boots_counted(who: &Session) -> String {
    let usage = who
        .call("recall", json!({"kind": "usage", "facts": true}))
        .await
        .json();
    usage["objects"][0]["fields"]["calls_start_here"]
        .as_str()
        .unwrap_or_else(|| panic!("the usage record counts the boots: {usage}"))
        .to_string()
}

/// **Starting a story and its first session in one call opens exactly one
/// session**, the one the two-call start opens: the bot has booted the same
/// number of times. A helper that opened a second session, or none, changes it.
#[tokio::test]
async fn beginning_with_a_session_opens_the_one_session_that_begin_then_session_opens() {
    let by_hand = {
        let story = Story::begin("bot:alpha").await;
        let first = story.session().await;
        boots_counted(&first).await
    };
    assert_eq!(by_hand, "1", "the two-call start boots the bot once");
    let (_story, first) = Story::begin_with_session("bot:alpha").await;
    assert_eq!(
        boots_counted(&first).await,
        by_hand,
        "the one-call start booted the bot another number of times than the two-call start"
    );
}
