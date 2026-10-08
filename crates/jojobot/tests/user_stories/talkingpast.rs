//! "I see the devs messaging without realizing they got messages back."
//!
//! Two agents corresponding. Each posts at the end of a piece of work, which is
//! exactly the moment a reply is most likely to be waiting — and neither is
//! blocked, neither is wrong, and neither notices. The failure is invisible
//! from inside a session, so nothing inside a session can be asked to fix it.
//!
//! So posting takes delivery. One call where there were two: the message is
//! filed, and whatever was waiting comes back with the receipt and becomes
//! yours to finish.
//!
//! **And the cost is paid rather than hidden.** Mail now leaves `new` without
//! anybody looking, so the sender's only pickup signal would lie. A delivery
//! says which way it was taken, and `list_sent` shows the sender the
//! difference.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn posting_hands_back_the_reply_that_was_already_waiting() {
    let story = Story::begin("bot:otto").await;

    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;

    // ── gamma answers a question otto has not read yet ──────────────────────
    let gamma = story.as_bot("bot:gamma").await;
    let answer = gamma
        .post("otto", "the damper", "hand-cut is fine, ship it")
        .await;

    // ── otto posts its own work, thinking about nothing else ────────────────
    //
    // It does not poll, does not open its box, and asks for nothing. This is
    // the call an agent makes at the end of a piece of work.
    let posted = s
        .call(
            "post_message",
            json!({
                "to": "gamma",
                "subject": "the damper, from my end",
                "body": "leaving the damper hand-cut unless you say otherwise",
            }),
        )
        .await;

    // The receipt is still a receipt: filed, and the body not shipped back.
    posted.says("\"state\":\"new\"");

    // …and the reply that was already waiting comes back with it, whole.
    posted.says("hand-cut is fine, ship it");
    posted.says("your_mail");

    // ── it was a real delivery, not a preview ───────────────────────────────
    //
    // The message is otto's to finish now. Opening the box afterwards names it
    // as a LEFTOVER rather than handing it over as fresh mail, which is what
    // proves the post took delivery rather than peeking.
    let box_now = s.drain().await;
    box_now.number("/leftovers/count", 1);

    // ── and the sender can tell what actually happened ──────────────────────
    //
    // gamma asks where its message got to. It reads `read` — but nobody went
    // looking, and saying only `read` would tell gamma its report had been
    // picked up.
    let where_it_went = gamma
        .call("list_sent", json!({ "include_bodies": false }))
        .await;
    where_it_went.says(&format!("\"id\":\"{answer}\""));
    where_it_went.says("\"taken_by\":\"posting\"");

    // The positive it rests on: a delivery somebody actually went and got
    // reads differently, in the same answer's vocabulary. otto's own message
    // to gamma is still sitting in new, so gamma opens its box for real.
    let gamma_box = gamma.drain().await;
    gamma_box.says("unless you say otherwise");
    let after = s
        .call("list_sent", json!({ "include_bodies": false }))
        .await;
    after.says("\"taken_by\":\"reading\"");

    s.wrap("posted, and was handed the answer it had not thought to ask for")
        .await;
    story.finish().await;
}

/// "I was working through two messages and every post I sent shipped me the
/// same two envelopes again."
///
/// A bot still acting on its mail posts as it goes. The delivery that rides on
/// a post hands back mail nobody had taken; what an earlier read already gave
/// the bot is counted, with the call that returns it, and stays owed.
#[tokio::test]
async fn a_post_does_not_ship_again_the_mail_the_bot_is_already_working() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    let gamma = story.as_bot("bot:gamma").await;

    // ── two messages arrive, and otto reads them and starts on the work ─────
    let first = gamma.post("otto", "the damper", "hand-cut is fine").await;
    let second = gamma.post("otto", "the flue", "the flue is clear").await;
    let box_now = s.drain().await;
    box_now.says("hand-cut is fine").says("the flue is clear");
    let envelope = |id: &str| format!("\"id\":\"{id}\"");

    // ── otto posts twice while still on them ────────────────────────────────
    for note in ["halfway through the damper", "the damper is done"] {
        let posted = s
            .call(
                "post_message",
                json!({"to": "gamma", "subject": "progress", "body": note}),
            )
            .await;
        posted
            .says("\"leftovers\"")
            // **The count is read at its own address.** A text needle for
            // `"count":2` is satisfied by 20, and by any other key called
            // `count` in the answer.
            .number("/your_mail/leftovers/count", 2)
            // The call that returns them is named, so a bot that wants them
            // back knows where.
            .says("read_mailbox")
            .never_says(&envelope(&first))
            .never_says(&envelope(&second))
            .never_says("hand-cut is fine");
    }

    // ── a third message arrives, and it rides back whole ────────────────────
    let third = gamma.post("otto", "the grate", "the grate is bent").await;
    let posted = s
        .call(
            "post_message",
            json!({"to": "gamma", "subject": "progress", "body": "on the grate now"}),
        )
        .await;
    posted
        .says(&envelope(&third))
        .says("the grate is bent")
        // The third rides back whole, and the two already worked are counted.
        .number("/your_mail/count", 1)
        .number("/your_mail/leftovers/count", 2)
        .never_says(&envelope(&first));

    // ── nothing was lost: the crash contract still hands back all three ─────
    let again = s.call("read_mailbox", json!({"new_only": false})).await;
    again
        .says(&envelope(&first))
        .says(&envelope(&second))
        .says(&envelope(&third))
        .says("\"seen_before\":true");

    s.wrap("posted while working two messages, and was not handed them again")
        .await;
    story.finish().await;
}
