//! "I sent the wrong date. Make sure nobody acts on it."
//!
//! A sender corrects a message by posting a second one that replaces it. If the
//! reader has not opened the first, the box hands over the correction and names
//! the original apart, so the reader never acts on the wrong date. If the reader
//! has opened it, the correction arrives saying so. Nothing is deleted: the
//! original stays readable and says what replaced it, and the sender sees the
//! same in their own view.
//!
//! **Every half is paired with its opposite in the same story**: the replaced
//! message withheld beside the plain one delivered, the refusals beside the
//! replacement that lands.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_sender_corrects_a_message_before_it_is_read_and_after() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("bot:gamma", "Gamma").await;
    setup.add("bot:delta", "Delta").await;
    setup.add("bot:epsilon", "Epsilon").await;
    setup.add("person:lisa", "Lisa").await;
    setup
        .event_with(
            "topic:instance",
            "who the operator is",
            json!({"operator": "person:lisa"}),
            &[],
        )
        .await;
    setup.wrap("stood the readers up").await;

    let s = story.as_bot("bot:otto").await;

    // ── before it is read ───────────────────────────────────────────────────
    let wrong = s
        .post("gamma", "Deploy day", "The deploy is on friday.")
        .await;
    let right = s
        .call(
            "post_message",
            json!({
                "to": "gamma",
                "subject": "Deploy day, corrected",
                "body": "The deploy is on monday.",
                "supersedes": &wrong,
            }),
        )
        .await;
    right.says(&format!("\"supersedes\":\"{wrong}\""));
    let right_id = right.field("id");

    // A poll weighs the box without taking anything. The replaced message is
    // not counted as waiting, because a delivery would not hand it over, and it
    // is counted apart so nothing in the box goes uncounted. The bar's own
    // figure agrees with the poll.
    let g = story.as_bot("bot:gamma").await;
    let polled = g.call("read_mailbox", json!({"counts_only": true})).await;
    assert_eq!(polled.json()["counts"]["new"], 1, "{}", polled.json());
    assert_eq!(
        polled.json()["counts"]["superseded"],
        1,
        "{}",
        polled.json()
    );
    assert_eq!(
        polled.json()["status_bar"]["mail_waiting"],
        1,
        "{}",
        polled.json()
    );

    // The boot's snapshot of the caller's own box agrees with the poll.
    let booted = g
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    assert_eq!(
        booted.json()["identity"]["owned_mailbox"]["counts"]["new"],
        1,
        "{}",
        booted.json()
    );

    // The delivery hands over the correction and names the original apart.
    let delivery = g.drain().await;
    delivery.says("The deploy is on monday.");
    delivery.never_says("The deploy is on friday.");
    let delivered = delivery.json();
    assert_eq!(delivered["count"], 1, "{delivered}");
    assert_eq!(
        delivered["superseded"]["messages"][0]["id"],
        wrong.as_str(),
        "{delivered}"
    );
    assert_eq!(
        delivered["superseded"]["messages"][0]["superseded_by"],
        right_id.as_str(),
        "{delivered}"
    );

    // On request the original is readable, and says what replaced it.
    g.call("read_message", json!({"message_id": &wrong}))
        .await
        .says("The deploy is on friday.")
        .says(&format!("\"superseded_by\":\"{right_id}\""));

    // The sender sees the same in their own view.
    s.call("list_sent", json!({}))
        .await
        .says(&format!("\"superseded_by\":\"{right_id}\""));

    // ── what a sender is told was waiting for them, agreeing with the poll ──
    //
    // Epsilon holds one message to hand over and one replaced, and says 1 in a
    // message of its own; the reader of that message sees the same figure.
    let first = s.post("epsilon", "Where", "The fair is on friday.").await;
    s.call(
        "post_message",
        json!({"to": "epsilon", "body": "The fair is on sunday.", "supersedes": &first}),
    )
    .await;
    let e = story.as_bot("bot:epsilon").await;
    e.call("post_message", json!({"to": "otto", "body": "Got it."}))
        .await;
    let heard = s.drain().await;
    assert_eq!(
        heard.json()["messages"][0]["sender_mail_waiting_at_send"],
        1,
        "{}",
        heard.json()
    );

    // ── the opposite: two messages, no link, both delivered ─────────────────
    s.post("delta", "First", "No link on this one.").await;
    s.post("delta", "Second", "Nor on this one.").await;
    let d = story.as_bot("bot:delta").await;
    let plain = d.drain().await;
    assert_eq!(plain.json()["count"], 2, "{}", plain.json());
    plain.never_says("superseded");

    // ── after it was read ───────────────────────────────────────────────────
    let first = s
        .post("delta", "Meeting room", "The meeting is in room four.")
        .await;
    d.drain().await.says("room four");
    let second = s
        .call(
            "post_message",
            json!({
                "to": "delta",
                "body": "The meeting is in room nine.",
                "supersedes": &first,
            }),
        )
        .await
        .field("id");
    let again = d.drain().await;
    again.says("room nine");
    let seen = again.json();
    assert_eq!(seen["messages"][0]["id"], second.as_str(), "{seen}");
    assert_eq!(
        seen["messages"][0]["already_seen"]["id"],
        first.as_str(),
        "{seen}"
    );
    assert!(seen.get("superseded").is_none(), "{seen}");

    // ── the refusals, each with its reason ──────────────────────────────────
    // Another sender's message.
    let theirs = d
        .refused(
            "post_message",
            json!({"to": "gamma", "body": "no", "supersedes": &wrong}),
        )
        .await;
    theirs.says("in_reply_to");
    // The same box, but a message already replaced.
    s.refused(
        "post_message",
        json!({"to": "gamma", "body": "no", "supersedes": &wrong}),
    )
    .await
    .says(&right_id);
    // A message in another box.
    s.refused(
        "post_message",
        json!({"to": "delta", "body": "no", "supersedes": &right_id}),
    )
    .await
    .says("gamma");

    // An id that names nothing.
    s.refused(
        "post_message",
        json!({"to": "gamma", "body": "no", "supersedes": "no-such-message"}),
    )
    .await
    .says("no-such-message");
    // The operator's box takes a replacement like any other box, and refuses one
    // that names a message from another box the same way.
    s.refused(
        "post_message",
        json!({"to": "person:lisa", "body": "no", "supersedes": &wrong}),
    )
    .await
    .says("gamma");

    // ── a sent message is not retracted ─────────────────────────────────────
    s.refused("retract", json!({"address": &wrong}))
        .await
        .says("supersedes");

    s.wrap("corrected two messages").await;
    story.finish().await;
}

/// "I corrected a message, and the reader set my correction aside. Is the first
/// one lost?" A replacement that was set aside is not a replacement: nobody can
/// act on it, so the message it replaced is handed over as an ordinary one, and
/// the correction stays set aside.
///
/// **The positive is the correction landing**, in the box beside it: with the
/// replacement left in place the original is withheld, and with it set aside the
/// original arrives. The two halves are read from the same box, one after the
/// other.
#[tokio::test]
async fn a_message_whose_replacement_was_set_aside_is_still_handed_over() {
    let story = Story::begin("bot:otto").await;
    let setup = story.session().await;
    setup.add("bot:gamma", "Gamma").await;
    setup.wrap("stood gamma up").await;

    let s = story.as_bot("bot:otto").await;
    let wrong = s
        .post("gamma", "Deploy day", "The deploy is on friday.")
        .await;
    let right = s
        .call(
            "post_message",
            json!({
                "to": "gamma",
                "subject": "Deploy day, corrected",
                "body": "The deploy is on monday.",
                "supersedes": &wrong,
            }),
        )
        .await
        .field("id");

    // ── with the correction in place, the original is withheld ──────────────
    let g = story.as_bot("bot:gamma").await;
    let polled = g.call("read_mailbox", json!({"counts_only": true})).await;
    assert_eq!(polled.json()["counts"]["new"], 1, "{}", polled.raw());
    assert_eq!(polled.json()["counts"]["superseded"], 1, "{}", polled.raw());

    // ── the reader sets the correction aside ────────────────────────────────
    g.call(
        "mark_processed",
        json!({"message_id": &right, "quarantine": "the sender meant a different box"}),
    )
    .await
    .says("\"state\":\"quarantined\"");

    // ── the original is owed again, and the correction stays set aside ──────
    let after = g.call("read_mailbox", json!({"counts_only": true})).await;
    assert_eq!(after.json()["counts"]["new"], 1, "{}", after.raw());
    assert!(
        after.json()["counts"].get("superseded").is_none(),
        "the original is counted as replaced by a message nobody can read: {}",
        after.raw()
    );
    assert_eq!(
        after.json()["quarantined"]["count"],
        1,
        "the correction is still set aside: {}",
        after.raw()
    );
    let delivery = g.drain().await;
    delivery.says("The deploy is on friday.");
    delivery.never_says("The deploy is on monday.");
    let delivered = delivery.json();
    assert_eq!(delivered["count"], 1, "{delivered}");
    assert_eq!(
        delivered["messages"][0]["id"],
        wrong.as_str(),
        "{delivered}"
    );
    assert!(delivered.get("superseded").is_none(), "{delivered}");

    // The original no longer says it was replaced.
    let read = g
        .call("read_message", json!({"message_id": &wrong}))
        .await
        .json();
    assert!(read.get("superseded_by").is_none(), "{read}");

    story.finish().await;
}
