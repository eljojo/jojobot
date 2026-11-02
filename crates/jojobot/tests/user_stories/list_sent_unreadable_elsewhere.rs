//! "Mail I sent is missing from my outbox. Did it never arrive, or can nobody
//! read it?"
//!
//! A message the store cannot read has no readable sender, so one of the damaged
//! messages in a box the caller cannot see may be the caller's own. `list_sent`
//! counts that damage and names nothing of it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn list_sent_counts_the_damage_in_a_box_it_cannot_show_and_names_none_of_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:sigma", "Sigma").await;
    s.add("bot:marge", "Marge").await;
    let sigma = story.as_bot("bot:sigma").await;

    // ── no damage anywhere: the count is zero, and the key is still there ───
    let clean = sigma.call("list_sent", json!({})).await.json();
    assert_eq!(clean["unreadable_elsewhere"]["count"], 0, "{clean}");

    // ── a message in a sibling's box and one in the caller's own cannot be read
    story.mail_is_unreadable_in("marge", "4213").await;
    story.mail_is_unreadable_in("sigma", "4212").await;
    let met = sigma.call("list_sent", json!({})).await.json();
    // The sibling's damage is counted, and nothing of it is told.
    assert_eq!(met["unreadable_elsewhere"]["count"], 1, "{met}");
    for leaked in ["marge", "4213"] {
        assert!(
            !met["unreadable_elsewhere"].to_string().contains(leaked),
            "the count names {leaked}: {met}"
        );
    }
    // The caller's own damage is listed, in the same read.
    let own = met["unreadable"].as_array().expect("unreadable is a list");
    assert!(
        own.iter().any(|entry| entry["mailbox"] == "sigma"),
        "the caller's own damaged box is named: {met}"
    );
    assert!(
        own.iter().all(|entry| entry["mailbox"] != "marge"),
        "the sibling's damaged box is not named: {met}"
    );

    sigma.wrap("counted the damage out of sight").await;
    s.wrap("stood two bots up").await;
    story.finish().await;
}
