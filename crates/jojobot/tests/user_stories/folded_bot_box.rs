//! "Lisa's bot was folded into Marge's. Why did the next boot give it a
//! box again?"
//!
//! A bot folded into another is no identity, so a boot opens no box for it.
//! A bot that was never folded and lacks its box still gets one, and that
//! positive is what keeps the empty answer for the folded bot honest.

use serde_json::json;

use jojobot_domain::memory::{EntityId, NewEntity};

use super::box_as;
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

    // The negative it keeps honest: the folded bot gets none, and its boot
    // still answers, as the answer a bot reads and not the mail store beneath.
    let (folded, _) = story
        .call("start_here", json!({"bot": "lisa", "brief": true}))
        .await;
    let folded = folded.json();
    assert_ne!(
        folded["status"], "blocked",
        "the folded bot still boots: {folded}"
    );
    assert_eq!(
        folded["identity"]["bot"]["id"], "bot:lisa",
        "the boot answers for the folded bot: {folded}"
    );
    let owned = &folded["identity"]["owned_mailbox"];
    assert!(
        owned["name"].is_null() && owned["healed"].is_null(),
        "no box is named or opened for a folded bot: {folded}"
    );
    assert_eq!(
        delta["identity"]["owned_mailbox"]["name"], "delta",
        "the unfolded bot's box is named, beside the folded bot's none: {delta}"
    );
    // The same read, as the folded bot's survivor sees the board: delta's box
    // exists and lisa's does not.
    assert_eq!(
        box_as::counts_as(&story, "bot:delta").await,
        (0, 0, 0),
        "delta's healed box is empty and readable"
    );
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
    assert_eq!(box_as::counts_as(&story, "bot:marge").await, (0, 0, 0));

    let sent = s
        .call(
            "post_message",
            json!({"to": "bot:lisa", "body": "the damper is stuck"}),
        )
        .await
        .json();
    assert_eq!(sent["folded"]["sent_to"], "bot:lisa", "{sent}");
    assert_eq!(sent["folded"]["folded_into"], "bot:marge", "{sent}");
    assert_eq!(
        box_as::counts_as(&story, "bot:marge").await,
        (1, 0, 0),
        "the survivor polls its box and finds the post"
    );
    assert_eq!(story.box_counts_on_the_page("bot:marge").await, (1, 0, 0));
    story
        .page("bot:lisa")
        .await
        .says("<h1>Index of /bot:lisa/</h1>")
        .never_says("<h2>Mailbox ");
    story.finish().await;
}
