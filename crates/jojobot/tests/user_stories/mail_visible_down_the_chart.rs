//! A manager reads a report without taking the report's work.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn mail_visible_story_a_manager_reads_a_new_report_without_taking_it() {
    let story = Story::begin("bot:homer").await;
    let homer = story.session().await;
    for (handle, name) in [
        ("bot:bart", "Bart"),
        ("bot:lisa", "Lisa"),
        ("bot:marge", "Marge"),
        ("bot:otto", "Otto"),
    ] {
        homer.add(handle, name).await;
    }
    let bart = story.as_bot("bot:bart").await;
    let lisa = story.as_bot("bot:lisa").await;
    let marge = story.as_bot("bot:marge").await;
    let otto = story.as_bot("bot:otto").await;
    homer
        .call(
            "capture",
            json!({
                "subject": "bot:bart", "content": "Bart reports to Homer",
                "sets": {"reports_to": "bot:homer"},
            }),
        )
        .await;
    bart.call(
        "capture",
        json!({
            "subject": "bot:lisa", "content": "Lisa reports to Bart",
            "sets": {"reports_to": "bot:bart"},
        }),
    )
    .await;
    let id = otto.post("lisa", "The report", "chartneedle").await;
    let found = homer.find_including_mail("chartneedle").await.json();
    assert_eq!(found["results"][0]["id"], id, "{found}");
    let peek = homer
        .call("read_message", json!({"message_id": &id}))
        .await
        .json();
    assert_eq!(peek["body"], "chartneedle", "{peek}");
    assert_eq!(peek["state"], "new", "{peek}");
    assert_eq!(peek["taken"], false, "{peek}");
    let report = homer
        .call(
            "list_sent",
            json!({"sender": "bot:otto", "include_bodies": true}),
        )
        .await
        .json();
    assert_eq!(report["messages"][0]["id"], id, "{report}");
    let withheld = marge
        .call(
            "list_sent",
            json!({"sender": "bot:otto", "include_bodies": true}),
        )
        .await
        .json();
    assert_eq!(withheld["messages"], json!([]), "{withheld}");
    assert_eq!(withheld["withheld"], 1, "{withheld}");
    let outbox = otto
        .call("read_message", json!({"message_id": &id}))
        .await
        .json();
    assert_eq!(outbox["state"], "new", "{outbox}");
    assert_eq!(outbox["taken"], false, "{outbox}");
    lisa.call("read_mailbox", json!({"counts_only": true}))
        .await
        .number("/counts/new", 1);
    homer
        .call("read_mailbox", json!({"counts_only": true}))
        .await
        .number("/counts/total", 0);
    let beside = marge.find_including_mail("chartneedle").await.json();
    assert_eq!(beside["results"], json!([]), "{beside}");
    marge
        .refused("read_message", json!({"message_id": &id}))
        .await
        .never_says("chartneedle");
    // A manager can finish its session while the report's message stays owed.
    homer.wrap("read the report without taking its work").await;
    let delivered = lisa
        .call("read_message", json!({"message_id": &id}))
        .await
        .json();
    assert_eq!(delivered["state"], "read", "{delivered}");
    assert_eq!(delivered["seen_before"], false, "{delivered}");
    lisa.call(
        "mark_processed",
        json!({"message_id": &id, "notes": "acted"}),
    )
    .await;
    lisa.wrap("handled my own report").await;
    bart.wrap("kept the report with its owner").await;
    marge
        .wrap("asked for the report instead of reading beside the chart")
        .await;
    otto.wrap("sent the report").await;
    story.finish().await;
}
