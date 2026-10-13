//! "I was restarted, and the mail I was handed is gone from my head."
//!
//! A message is `read` once a read delivered it and stays owed until it is
//! processed. A bot that lost what it was handed, to a crash or a compaction,
//! boots and sees counts. **The boot's mailbox block names the call that
//! returns the bodies when there is such mail, and says nothing when there is
//! none**: a box holding only new mail, or mail that was finished, is not owed
//! a recovery read.

use serde_json::json;

use super::dsl::Story;

/// The identifier of the recovery read, which the boot has to name.
const RECOVERY_ARGUMENT: &str = "new_only";

#[tokio::test]
async fn a_boot_names_the_read_that_returns_mail_delivered_and_not_finished() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.post("gamma", "the first", "one").await;
    s.post("gamma", "the second", "two").await;
    s.post("delta", "only news", "three").await;

    // Gamma takes delivery of both and loses what it was handed.
    let gamma = story.as_bot("bot:gamma").await;
    let delivered = gamma.call("read_mailbox", json!({})).await.json();
    let ids: Vec<String> = delivered["messages"]
        .as_array()
        .expect("both messages are delivered")
        .iter()
        .map(|m| m["id"].as_str().expect("an id").to_string())
        .collect();
    assert_eq!(ids.len(), 2, "{delivered}");

    let reboot = |bot: &'static str| {
        let story = &story;
        async move {
            let (booted, _) = story
                .call(
                    "start_here",
                    json!({"bot": bot, "brief": true, "resume": "new"}),
                )
                .await;
            booted.json()["identity"]["owned_mailbox"].clone()
        }
    };

    // ── owed: counts say two are read, and the block names the way back ─────
    let owed = reboot("gamma").await;
    assert_eq!(owed["counts"]["read"], 2, "{owed}");
    assert!(
        owed.to_string().contains(RECOVERY_ARGUMENT),
        "the boot does not name the read that returns delivered mail: {owed}"
    );
    assert!(
        owed.to_string().contains("read_message"),
        "the boot does not name the read of one message by id: {owed}"
    );

    // ── finished: after both are processed the sentence is gone ─────────────
    for id in &ids {
        gamma
            .call("mark_processed", json!({"message_id": id}))
            .await;
    }
    let finished = reboot("gamma").await;
    assert_eq!(finished["counts"]["read"], 0, "{finished}");
    assert!(
        !finished.to_string().contains(RECOVERY_ARGUMENT),
        "a box with nothing owed names a recovery read: {finished}"
    );

    // ── only news: mail nobody has taken is not a recovery case ─────────────
    let news = reboot("delta").await;
    assert_eq!(news["counts"]["new"], 1, "{news}");
    assert!(
        !news.to_string().contains(RECOVERY_ARGUMENT),
        "a box with only new mail names a recovery read: {news}"
    );

    story.finish().await;
}
