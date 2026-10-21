//! **A bot's box, counted the way the bot counts it.** A story that says a note
//! was sent, or was not, asks `read_mailbox` with `counts_only` as that bot: the
//! answer a bot meets, from the image the server serves. The raw mail store
//! (`Story::mail_held_by`) can drift from that image without a story noticing.

use super::dsl::Story;

/// `(new, read, processed)` in the box `handle` owns, as `handle` polls it.
pub async fn counts_as(story: &Story, handle: &str) -> (usize, usize, usize) {
    let bot = story.as_bot(handle).await;
    let polled = bot
        .call("read_mailbox", serde_json::json!({"counts_only": true}))
        .await
        .json();
    let counts = &polled["counts"];
    let count = |state: &str| {
        counts[state]
            .as_u64()
            .unwrap_or_else(|| panic!("{handle}'s poll has no {state} count: {polled}"))
            as usize
    };
    (count("new"), count("read"), count("processed"))
}
