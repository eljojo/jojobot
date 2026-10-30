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
    let assistant = story.as_bot("bot:assistant").await;
    assistant
        .call(
            "capture",
            json!({
                "subject": "bot:homer", "content": "Homer reports to the assistant",
                "sets": {"reports_to": "bot:assistant"},
            }),
        )
        .await;
    assistant
        .call(
            "capture",
            json!({
                "subject": "bot:bart", "content": "Bart reports to Homer",
                "sets": {"reports_to": "bot:homer"},
            }),
        )
        .await;
    assistant
        .call(
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
    assistant.wrap("placed the reports on the chart").await;
    story.finish().await;
}

/// **Mail is read down the chart and never up, and an author reading up is not
/// told what the recipient wrote on it.** Homer manages Bart, who manages Lisa.
/// Lisa mails Bart, who closes it with a note for himself; Lisa still reads her
/// message's state and not the note. Otto mails Homer: Bart is told the message
/// is above him and finds nothing of it by search, while Homer finds the same
/// message by the same search.
#[tokio::test]
async fn mail_visible_story_an_author_reads_up_without_the_notes_and_nobody_reads_above() {
    let story = Story::begin("bot:homer").await;
    let homer = story.session().await;
    for (handle, name) in [
        ("bot:bart", "Bart"),
        ("bot:lisa", "Lisa"),
        ("bot:otto", "Otto"),
    ] {
        homer.add(handle, name).await;
    }
    let bart = story.as_bot("bot:bart").await;
    let lisa = story.as_bot("bot:lisa").await;
    let otto = story.as_bot("bot:otto").await;
    let assistant = story.as_bot("bot:assistant").await;
    for (report, manager) in [
        ("bot:homer", "bot:assistant"),
        ("bot:bart", "bot:homer"),
        ("bot:lisa", "bot:bart"),
    ] {
        assistant
            .call(
                "capture",
                json!({
                    "subject": report, "content": format!("{report} reports to {manager}"),
                    "sets": {"reports_to": manager},
                }),
            )
            .await;
    }

    // ── an author mails a box above its own; the recipient closes it with a note ──
    let up = lisa.post("bart", "Status", "upneedle").await;
    bart.call(
        "mark_processed",
        json!({"message_id": &up, "notes": "kept for bart alone"}),
    )
    .await;
    // The owner and the manager above the owner read the note.
    for reader in [&bart, &homer] {
        reader
            .call("read_message", json!({"message_id": &up}))
            .await
            .says("kept for bart alone");
    }
    // The author reads the message and its state, and is told notes were left out.
    let authored = lisa
        .call("read_message", json!({"message_id": &up}))
        .await
        .json();
    assert_eq!(authored["state"], "processed", "{authored}");
    assert_eq!(authored["notes_withheld"], true, "{authored}");
    assert!(
        !authored.to_string().contains("kept for bart alone"),
        "{authored}"
    );
    // The outbox and a search are the same read.
    let sent = lisa
        .call("list_sent", json!({"include_bodies": true}))
        .await
        .json();
    assert_eq!(sent["messages"][0]["id"], up, "{sent}");
    assert_eq!(sent["messages"][0]["notes_withheld"], true, "{sent}");
    assert!(!sent.to_string().contains("kept for bart alone"), "{sent}");
    let found = lisa.find_including_mail("upneedle").await.json();
    assert_eq!(found["results"][0]["id"], up, "{found}");
    assert_eq!(found["results"][0]["notes_withheld"], true, "{found}");
    assert!(
        !found.to_string().contains("kept for bart alone"),
        "{found}"
    );

    // ── nobody reads a box above its own ─────────────────────────────────────
    let above = otto.post("homer", "Request", "aboveneedle").await;
    let seen = homer.find_including_mail("aboveneedle").await.json();
    assert_eq!(seen["results"][0]["id"], above, "{seen}");
    let below = bart.find_including_mail("aboveneedle").await.json();
    assert_eq!(below["results"], json!([]), "{below}");
    let refused = bart
        .refused("read_message", json!({"message_id": &above}))
        .await;
    assert_eq!(
        refused.json()["refusal_code"],
        "NotYours",
        "{}",
        refused.raw()
    );
    refused.says("above").never_says("aboveneedle");

    homer.wrap("read the chart").await;
    bart.wrap("closed the report").await;
    lisa.wrap("sent the status").await;
    otto.wrap("sent the request").await;
    assistant.wrap("placed the reports on the chart").await;
    story.finish().await;
}
