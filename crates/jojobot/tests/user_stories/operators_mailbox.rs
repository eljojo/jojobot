//! "The report is for the operator. No bot should be able to read it back."
//!
//! **A bot has things to say to the operator, and they are not for any other
//! bot.** Once the instance names its operator, a bot writes to that person by
//! their handle and the first post opens their one box. Nothing on the surface
//! reads it back: not the bot that wrote it, not a colleague, not the front
//! door's search, not an outbox listing and not the archive. Only the
//! operator's own reading surface moves that mail.
//!
//! **Every refusal here sits beside the positive it depends on**: the post that
//! landed, counted from the store itself, and the same calls made against an
//! ordinary bot's box, which succeed.

use serde_json::json;

use super::dsl::{Answer, Session, Story};

const FIGURE: &str = "the secret figure is 4242";

/// Every way a bot has of reading mail, asked by one bot, none of which may
/// return the operator's message. `listing_sender` is who `list_sent` is asked
/// after (`None` for the bot's own outbox).
async fn tries_every_way_to_read_it(
    reader: &Session,
    id: &str,
    listing_sender: Option<&str>,
) -> Vec<Answer> {
    let mut answers = Vec::new();
    // Taking delivery of its own box.
    answers.push(reader.drain().await);
    // Opening the message by id.
    answers.push(
        reader
            .refused("read_message", json!({"message_id": id}))
            .await,
    );
    // Retiring it, which would tell its writer the person had dealt with it.
    answers.push(
        reader
            .refused("mark_processed", json!({"message_id": id, "notes": "done"}))
            .await,
    );
    // Quarantining it, which would hide it from the person.
    answers.push(
        reader
            .refused(
                "mark_processed",
                json!({"message_id": id, "quarantine": "not for me"}),
            )
            .await,
    );
    // Listing the outbox it sits in, with and without bodies.
    for include_bodies in [false, true] {
        let mut args = json!({"include_bodies": include_bodies});
        if let Some(sender) = listing_sender {
            args["sender"] = json!(sender);
        }
        answers.push(reader.call("list_sent", args).await);
    }
    // The front door, with mail, by a word from the body and one from the title.
    for word in ["secret", "4242", "quarterly"] {
        answers.push(reader.find_including_mail(word).await);
    }
    answers
}

#[tokio::test]
async fn a_bot_leaves_the_operator_a_message_and_no_bot_can_read_it_back() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:lisa", "Lisa").await;
    s.add("bot:sigma", "Sigma").await;

    // ── nobody is the operator yet: the post says how to name one ───────────
    s.refused("post_message", json!({"to": "person:lisa", "body": FIGURE}))
        .await
        .says("topic:instance");
    assert_eq!(story.mail_held_by("person:lisa").await, (0, 0, 0));

    // ── the operator is named, and that alone opens nothing ─────────────────
    s.add("topic:instance", "This instance").await;
    s.event_with(
        "topic:instance",
        "who the operator is",
        json!({"operator": "person:lisa"}),
        &[],
    )
    .await;
    assert_eq!(story.mail_held_by("person:lisa").await, (0, 0, 0));

    // ── only the operator has a box ─────────────────────────────────────────
    s.add("person:milhouse", "Milhouse").await;
    s.refused(
        "post_message",
        json!({"to": "person:milhouse", "body": "for somebody else"}),
    )
    .await
    .never_says("lisa");
    assert_eq!(story.mail_held_by("person:milhouse").await, (0, 0, 0));

    // ── the first post opens their box and lands in it ──────────────────────
    let sigma = story.as_bot("bot:sigma").await;
    let posted = s
        .call(
            "post_message",
            json!({
                "to": "person:lisa", "subject": "the quarterly figure", "body": FIGURE,
            }),
        )
        .await
        .json();
    let id = posted["id"].as_str().expect("a post hands back its id");
    assert_eq!(posted["mailbox"], "person-lisa", "{posted}");
    assert_eq!(
        story.mail_held_by("person:lisa").await,
        (1, 0, 0),
        "the post landed, counted from the store"
    );

    // ── the positive beside every refusal: ordinary mail between bots works ──
    let ordinary = s
        .post("bot:sigma", "an ordinary note", "the kiln is relined")
        .await;
    let delivered = sigma.drain().await;
    delivered.says("kiln");
    delivered.never_says("4242");
    s.find_including_mail("kiln").await.says(&ordinary);

    // ── nobody reads it back: the bot that wrote it, nor a colleague ────────
    for answer in tries_every_way_to_read_it(&s, id, None).await {
        answer.never_says("4242").never_says("secret");
    }
    for answer in tries_every_way_to_read_it(&sigma, id, Some("bot:otto")).await {
        answer.never_says("4242").never_says("secret");
    }
    // The outbox still says the message exists, by id, time and subject.
    s.call("list_sent", json!({}))
        .await
        .says("the quarterly figure")
        .says(id);
    assert_eq!(
        story.mail_held_by("person:lisa").await,
        (1, 0, 0),
        "no refusal moved the message"
    );

    // ── retired in the store, it is still private: the archive is not open ───
    story.retire_in_the_store(id).await;
    assert_eq!(story.mail_held_by("person:lisa").await, (0, 0, 1));
    for reader in [&s, &sigma] {
        reader
            .refused("read_message", json!({"message_id": id}))
            .await
            .never_says("4242");
        reader
            .find_including_mail("secret")
            .await
            .never_says("4242");
    }

    // ── the operator's window shows it on no page at all ────────────────────
    //
    // The listing renders a bot's own box on that bot's page and nothing else
    // renders mail, so the person's page, the instance's page and every bot's
    // page leave the message out. The ordinary note on a bot's page is the
    // positive: the window does show mail where it shows any.
    // A run's own record of having posted names the box, as it names any box
    // it posted to; it carries none of the message, so the text and the title
    // are what must be absent.
    for handle in ["person:lisa", "topic:instance", "bot:otto", "bot:sigma"] {
        story
            .page(handle)
            .await
            .never_says("4242")
            .never_says("quarterly");
    }
    story.page("bot:sigma").await.says("an ordinary note");

    story.finish().await;
}
