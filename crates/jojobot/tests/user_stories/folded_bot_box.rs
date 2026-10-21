//! "Lisa's bot was folded into Marge's. Why did the next boot give it a
//! box again?"
//!
//! A bot folded into another is no identity, so a boot opens no box for it.
//! A bot that was never folded and lacks its box still gets one, and that
//! positive is what keeps the empty answer for the folded bot honest.

use serde_json::json;

use jojobot_domain::mailbox::Mailboxes;
use jojobot_domain::memory::{EntityId, NewEntity};

use super::dsl::Story;

/// A bot written to the store with no box, as a creation cut short leaves it.
async fn without_a_box(story: &Story, handle: &str, name: &str) {
    story
        .memory_store()
        .add_entity(NewEntity::new(EntityId(handle.into()), name, "user-named"))
        .await
        .expect("the store takes the bot")
        .written()
        .expect("an empty roster blocks nothing");
}

#[tokio::test]
async fn a_boot_opens_no_box_for_a_folded_bot_and_one_for_an_unfolded_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    without_a_box(&story, "bot:lisa", "Lisa").await;
    without_a_box(&story, "bot:delta", "Delta").await;
    story.the_store_changed_underneath().await;
    s.merge_entities("bot:lisa", "bot:marge", "one bot, two records", None)
        .await;

    // The positive: a bot never folded gets the box it lacked.
    let (delta, _) = story
        .call("start_here", json!({"bot": "delta", "brief": true}))
        .await;
    let delta = delta.json();
    assert_eq!(
        delta["identity"]["owned_mailbox"]["healed"], true,
        "{delta}"
    );

    // The negative it keeps honest: the folded bot gets none.
    story
        .call("start_here", json!({"bot": "lisa", "brief": true}))
        .await;
    let boxes: Vec<String> = story
        .mail_store()
        .list_mailboxes()
        .await
        .expect("the boxes list")
        .iter()
        .map(|held| held.name.as_str().to_string())
        .collect();
    assert!(boxes.contains(&"delta".to_string()), "{boxes:?}");
    assert!(!boxes.contains(&"lisa".to_string()), "{boxes:?}");
    story.finish().await;
}

/// **Mail to the folded bot's handle reaches the survivor**, and the receipt
/// says the fold. The folded bot has no box, so a post that did not follow the
/// fold would be refused for a missing box.
#[tokio::test]
async fn mail_to_a_folded_bot_reaches_the_survivor_and_the_receipt_names_the_fold() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:marge", "Marge").await;
    without_a_box(&story, "bot:lisa", "Lisa").await;
    story.the_store_changed_underneath().await;
    s.merge_entities("bot:lisa", "bot:marge", "one bot, two records", None)
        .await;
    assert_eq!(story.mail_held_by("bot:marge").await, (0, 0, 0));

    let sent = s
        .call(
            "post_message",
            json!({"to": "bot:lisa", "body": "the damper is stuck"}),
        )
        .await
        .json();
    assert_eq!(sent["folded"]["sent_to"], "bot:lisa", "{sent}");
    assert_eq!(sent["folded"]["folded_into"], "bot:marge", "{sent}");
    assert_eq!(story.mail_held_by("bot:marge").await, (1, 0, 0));
    assert_eq!(story.mail_held_by("bot:lisa").await, (0, 0, 0));
    story.finish().await;
}
