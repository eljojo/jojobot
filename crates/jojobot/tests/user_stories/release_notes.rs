//! "I was away for a deploy. Did anyone tell me what changed?"
//!
//! **jojobot tells each bot itself.** A change bots can use ships with a note, and
//! when the server starts it leaves ONE message in each bot's box listing the
//! notes that box has not been sent, from the system sender and not from a bot. A
//! restart sends nothing twice; an archived bot is told nothing; a bot with a
//! manager is told who to ask, and a bot without one is not told of one.

use serde_json::json;

use super::dsl::Story;

const TITLE: &str = "jojobot now mails you what changed";

#[tokio::test]
async fn a_bot_holds_one_message_from_the_system_listing_what_shipped_and_who_to_ask() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:moe", "Moe").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:bart", "Bart").await;
    let marge_first = story.as_bot("bot:marge").await;
    marge_first
        .call(
            "capture",
            json!({
                "subject": "bot:moe", "content": "bot:moe reports to bot:marge",
                "provenance": "testimony", "sets": {"reports_to": "bot:marge"},
            }),
        )
        .await;
    s.call(
        "archive_entity",
        json!({"handle": "bot:bart", "reason": "retired"}),
    )
    .await;

    // ── the server starts: the passes run over the board ────────────────────
    let later = story.restarted().await;
    let moe = later.as_bot("bot:moe").await;
    let delivered = moe.call("read_mailbox", json!({})).await.json();
    let messages = delivered["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1, "exactly one message: {delivered}");
    let note = &messages[0];
    assert_eq!(
        note["sender"], "jojobot",
        "from the system, not a bot: {note}"
    );
    let body = note["body"].as_str().expect("a body");
    assert!(body.contains(TITLE), "the note's title: {body}");
    assert!(
        body.contains("What you can now do")
            && body.contains("Example")
            && body.contains("What to do"),
        "what it says you can do, an example and what to do: {body}"
    );
    assert!(
        body.contains("@bot:marge"),
        "Homer is told his manager, as a mention: {body}"
    );
    moe.call(
        "mark_processed",
        json!({"message_id": note["id"], "notes": "read"}),
    )
    .await;

    // ── Marge has no manager and so no line naming one; she is sent her own ──
    let marge = later.as_bot("bot:marge").await;
    let delivered = marge.call("read_mailbox", json!({})).await.json();
    let messages = delivered["messages"].as_array().expect("messages");
    assert_eq!(messages.len(), 1, "Marge's own: {delivered}");
    assert_eq!(messages[0]["sender"], "jojobot");
    assert!(
        !messages[0]["body"]
            .as_str()
            .unwrap_or_default()
            .contains("@bot:"),
        "no manager, no mention: {}",
        messages[0]["body"]
    );

    // ── an archived bot is told nothing ─────────────────────────────────────
    assert_eq!(later.mail_held_by("bot:bart").await, (0, 0, 0));

    // ── the message is an ordinary one to every reader ──────────────────────
    let found = moe
        .call(
            "search",
            json!({"query": "mails you what changed", "include_mail": true}),
        )
        .await
        .json();
    assert!(
        found.to_string().contains(TITLE),
        "search finds the note: {found}"
    );
    moe.call("list_sent", json!({})).await;
    let replied = moe
        .call(
            "post_message",
            json!({"to": "marge", "body": "did you read it?", "in_reply_to": note["id"]}),
        )
        .await
        .json();
    assert_eq!(replied["state"], "new", "a reply breaks nothing: {replied}");

    // ── another start sends nothing more ────────────────────────────────────
    let again = later.restarted().await;
    assert_eq!(
        again.mail_held_by("bot:moe").await,
        (0, 0, 1),
        "Homer's note is processed and nothing new arrived"
    );
    again.finish().await;
}
