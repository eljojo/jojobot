//! "Close the answered dispatches together; tell my manager if the pile is work."

use serde_json::{Value, json};

use super::dsl::Story;

#[tokio::test]
async fn closing_a_batch_refuses_a_blank_id_before_closing_any_message() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:otto", "Otto").await;
    let otto = story.as_bot("bot:otto").await;
    let first = otto
        .post("gamma", "First dispatch", "Check the ledger.")
        .await;
    let second = otto
        .post("gamma", "Second dispatch", "Check the rules.")
        .await;
    gamma.drain().await;

    gamma
        .refused(
            "mark_processed",
            json!({"message_ids": [&first, " ", &second], "notes": "both checked"}),
        )
        .await
        .says("message_ids");
    for id in [&first, &second] {
        let untouched = gamma
            .call("read_message", json!({"message_id": id}))
            .await
            .json();
        assert_eq!(untouched["state"], "read", "{untouched}");
        assert_eq!(untouched["notes"], Value::Null, "{untouched}");
    }

    let closed = gamma
        .call(
            "mark_processed",
            json!({"message_ids": [&first, &second], "notes": "both checked"}),
        )
        .await
        .json();
    assert_eq!(closed["processed"], 2, "{closed}");
    assert_eq!(closed["ids"], json!([&first, &second]), "{closed}");
    for id in [&first, &second] {
        let stored = gamma
            .call("read_message", json!({"message_id": id}))
            .await
            .json();
        assert_eq!(stored["state"], "processed", "{stored}");
        assert_eq!(stored["notes"], "both checked", "{stored}");
    }
    let repeated = gamma
        .call(
            "mark_processed",
            json!({"message_ids": [&first, &second], "notes": "replace the outcome"}),
        )
        .await
        .json();
    assert_eq!(repeated["processed"], 0, "{repeated}");
    assert_eq!(
        repeated["already_processed"],
        json!([&first, &second]),
        "{repeated}"
    );
    for id in [&first, &second] {
        let stored = gamma
            .call("read_message", json!({"message_id": id}))
            .await
            .json();
        assert_eq!(stored["notes"], "both checked", "{stored}");
    }
    story.finish().await;
}

#[tokio::test]
async fn a_read_mail_pile_teaches_batch_close_and_names_the_manager_once_until_compaction() {
    let story = Story::begin("bot:gamma").await;
    let gamma = story.session().await;
    gamma.add("bot:otto", "Otto").await;
    let otto = story.as_bot("bot:otto").await;
    otto.call(
        "capture",
        json!({"subject": "bot:gamma", "content": "reports to Otto",
                   "provenance": "testimony", "sets": {"reports_to": "bot:otto"}}),
    )
    .await;
    for _ in 0..19 {
        otto.post("gamma", "Ledger dispatch", "Check one ledger entry.")
            .await;
    }
    let below = gamma.drain().await.json();
    assert_eq!(below["status_bar"]["mail_pile"], Value::Null, "{below}");
    let twentieth = otto
        .post("gamma", "Last dispatch", "Check the rule too.")
        .await;
    let unread = gamma
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .json();
    assert_eq!(unread["status_bar"]["mail_pile"], Value::Null, "{unread}");
    let at_twenty = gamma.drain().await.json();
    let pile = &at_twenty["status_bar"]["mail_pile"];
    assert_eq!(pile["read_and_not_closed"], 20, "{at_twenty}");
    let teaching = pile["say"].as_str().expect("the pile gives a next action");
    for name in ["mark_processed", "message_ids", "post_message", "bot:otto"] {
        assert!(teaching.contains(name), "missing {name}: {at_twenty}");
    }
    let once = gamma
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .json();
    assert_eq!(once["status_bar"]["mail_pile"], Value::Null, "{once}");
    let compacted = gamma
        .call("start_here", json!({"compacted": true, "brief": true}))
        .await
        .json();
    assert_eq!(
        compacted["status_bar"]["mail_pile"]["read_and_not_closed"], 20,
        "{compacted}"
    );
    gamma.processed(&twentieth, "checked the rule").await;
    let below_again = gamma
        .call("start_here", json!({"compacted": true, "brief": true}))
        .await
        .json();
    assert_eq!(
        below_again["status_bar"]["mail_pile"],
        Value::Null,
        "{below_again}"
    );
    story.finish().await;
}
