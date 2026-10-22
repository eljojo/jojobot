//! "I wrote to a bot that was folded into another, and the answer told me where it went."
//!
//! **The served texts of `post_message` say before the post that a post to a
//! folded bot goes to its survivor.** The roster names are the cast.

use super::dsl::Story;

/// **The served description of `post_message` and of its `to` argument say,
/// before the post, that a post to a folded bot goes to the survivor and that
/// the receipt's `folded` block says so.** The positive the texts promise is
/// `mail_to_a_folded_bot_reaches_the_survivor_and_the_receipt_names_the_fold`,
/// which reads that block off a real receipt.
#[tokio::test]
async fn post_message_says_before_the_post_that_a_folded_bots_mail_goes_to_its_survivor() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    for text in [
        s.description("post_message").await,
        s.argument_description("post_message", "to").await,
    ] {
        assert!(text.contains("`folded`"), "{text}");
    }
    story.finish().await;
}
