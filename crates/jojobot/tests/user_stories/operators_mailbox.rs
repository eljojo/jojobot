//! "The report is for the operator. No bot should be able to read it back."
//!
//! **A bot has things to say to the operator, and they are not for any other
//! bot.** Once the instance names its operator, a bot writes to that person by
//! their handle and the first post opens their one box. Nothing on the surface
//! reads it back: not the bot that wrote it, not a colleague, not the front
//! door's search, not an outbox listing and not the archive. Only the
//! operator's own reading surface moves that mail.
//!
//! **Every refusal here sits beside the positive it depends on, in the same
//! block**: the same call made on an ordinary note in a bot's own box succeeds,
//! so a refusal that fired on everything would not pass. **And a refusal is
//! held to its words**: the private-box answer says the message is not one any
//! bot reads, which is a different answer from the one for a message in another
//! bot's box, so a build that mixed the two up would not pass either.

use serde_json::json;

use super::dsl::{Session, Story};

const FIGURE: &str = "the secret figure is 4242";

/// What the private-box refusal says, and the other refusal's own words. Two
/// different answers: this one is about the person's box, that one about a box
/// that is merely not the caller's.
const PRIVATE: &str = "not one any bot reads";
const NOT_YOURS: &str = "not your box";

/// One reader's ordinary mail: two notes in its own box, one to retire and one
/// to quarantine, and a word from them the front door finds.
struct Own {
    retire: String,
    quarantine: String,
    /// The title of the note to retire, which a hit and a listing both carry.
    title: String,
    word: &'static str,
}

/// Leave `to` two ordinary notes, written by `from`.
async fn leave_notes(from: &Session, to: &str, word: &'static str) -> Own {
    let title = format!("retire this {word} note");
    Own {
        retire: from
            .post(to, &title, &format!("the {word} is relined"))
            .await,
        title,
        quarantine: from
            .post(
                to,
                &format!("quarantine this {word} note"),
                &format!("the {word} glaze is tested"),
            )
            .await,
        word,
    }
}

/// Every way a bot has of reading mail, asked by one bot. Each way is asked of
/// the operator's message, which it refuses, and of an ordinary note, which it
/// serves, in the same block.
///
/// `other_box_note` is a note in a different bot's box, for the refusal that is
/// not the private one. `outbox_title` is the title of a message the listed
/// outbox shows, and `listing_sender` is who `list_sent` is asked after (`None` for the
/// bot's own outbox).
async fn tries_every_way_to_read_it(
    reader: &Session,
    private: &str,
    own: &Own,
    other_box_note: &str,
    outbox_title: &str,
    listing_sender: Option<&str>,
) {
    // Taking delivery of its own box: the ordinary notes arrive, the
    // operator's does not.
    let delivered = reader.drain().await;
    delivered.says(own.word).never_says("4242");

    // Opening a message by id: the ordinary note opens, the operator's is the
    // private-box refusal, and a note in another bot's box is a different one.
    reader
        .call("read_message", json!({"message_id": own.retire}))
        .await
        .says(own.word);
    reader
        .refused("read_message", json!({"message_id": private}))
        .await
        .says(PRIVATE)
        .never_says(NOT_YOURS)
        .never_says("4242");
    reader
        .refused("read_message", json!({"message_id": other_box_note}))
        .await
        .says(NOT_YOURS)
        .never_says(PRIVATE);

    // Retiring: it would tell its writer the person had dealt with it.
    reader
        .refused(
            "mark_processed",
            json!({"message_id": private, "notes": "done"}),
        )
        .await
        .says(PRIVATE)
        .never_says(NOT_YOURS);
    // Quarantining: it would hide the message from the person.
    reader
        .refused(
            "mark_processed",
            json!({"message_id": private, "quarantine": "not for me"}),
        )
        .await
        .says(PRIVATE)
        .never_says(NOT_YOURS);
    // The same two calls on the ordinary notes land.
    reader
        .call(
            "mark_processed",
            json!({"message_id": own.retire, "notes": "done"}),
        )
        .await
        .says("processed");
    reader
        .call(
            "mark_processed",
            json!({"message_id": own.quarantine, "quarantine": "not for me"}),
        )
        .await
        .says(&own.quarantine);

    // Listing the outbox it sits in, with and without bodies: the ordinary
    // message is listed in the same answer that leaves the operator's text out.
    for include_bodies in [false, true] {
        let mut args = json!({"include_bodies": include_bodies});
        if let Some(sender) = listing_sender {
            args["sender"] = json!(sender);
        }
        reader
            .call("list_sent", args)
            .await
            .says(outbox_title)
            .never_says("4242")
            .never_says("secret");
    }

    // The front door, with mail, by a word from the body and one from the
    // title: nothing of the operator's message, and the ordinary note found by
    // its own word.
    for word in ["secret", "4242", "quarterly"] {
        reader
            .find_including_mail(word)
            .await
            .never_says("4242")
            .never_says("secret");
    }
    reader.find_including_mail(own.word).await.says(&own.title);
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

    // ── ordinary mail between bots: the positives every refusal rests on ────
    let to_sigma = s
        .post("bot:sigma", "an ordinary note", "the kiln is relined")
        .await;
    let to_otto = sigma
        .post("bot:otto", "a note back", "the oven is preheated")
        .await;
    let sigmas = leave_notes(&s, "bot:sigma", "kiln").await;
    let ottos = leave_notes(&sigma, "bot:otto", "oven").await;
    let delivered = sigma.drain().await;
    delivered.says("kiln").never_says("4242");
    s.find_including_mail("kiln").await.says(&to_sigma);

    // ── nobody reads it back: the bot that wrote it, nor a colleague ────────
    tries_every_way_to_read_it(&s, id, &ottos, &to_sigma, "an ordinary note", None).await;
    tries_every_way_to_read_it(
        &sigma,
        id,
        &sigmas,
        &to_otto,
        "an ordinary note",
        Some("bot:otto"),
    )
    .await;

    // The outbox still says the message exists, by id, time and subject, and
    // lists the ordinary note beside it.
    s.call("list_sent", json!({}))
        .await
        .says("the quarterly figure")
        .says(id)
        .says("an ordinary note");
    // Asking after what was sent to the person says the same and no more; the
    // same question about a bot's box lists that note in full.
    s.call("list_sent", json!({"to": "person:lisa"}))
        .await
        .says("the quarterly figure")
        .says(id)
        .never_says("4242");
    s.call("list_sent", json!({"to": "bot:sigma"}))
        .await
        .says("an ordinary note")
        .never_says("the quarterly figure");

    // The answer to a post carries the poster's own waiting mail: a note left
    // for sigma rides back on sigma's next post, and the operator's message,
    // which is in nobody's box, rides on none.
    s.post("bot:sigma", "a late note", "the saw is sharpened")
        .await;
    let rides = sigma
        .call(
            "post_message",
            json!({"to": "bot:otto", "subject": "thanks", "body": "got it"}),
        )
        .await;
    rides.says("sharpened").never_says("4242");
    assert_eq!(
        story.mail_held_by("person:lisa").await,
        (1, 0, 0),
        "no refusal moved the message"
    );

    // ── retired in the store, it is still private: the archive is not open ───
    //
    // The processed note in a bot's box is readable by any bot; the processed
    // message in the person's box is readable by none.
    story.retire_in_the_store(id).await;
    assert_eq!(story.mail_held_by("person:lisa").await, (0, 0, 1));
    for (reader, note) in [(&s, &ottos), (&sigma, &sigmas)] {
        reader
            .call("read_message", json!({"message_id": note.retire}))
            .await
            .says(note.word);
        reader
            .refused("read_message", json!({"message_id": id}))
            .await
            .says(PRIVATE)
            .never_says("4242");
        reader
            .find_including_mail("secret")
            .await
            .never_says("4242");
        reader
            .find_including_mail(note.word)
            .await
            .says(&note.title);
    }
    // And the other bot's processed note is readable from here too: the
    // archive is open on every ordinary box.
    s.call("read_message", json!({"message_id": sigmas.retire}))
        .await
        .says(sigmas.word);

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
