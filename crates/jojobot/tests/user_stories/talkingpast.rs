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
    // The message is otto's to finish now. Opening the box afterwards hands it
    // back as a LEFTOVER rather than as fresh mail, which is what proves the
    // post took delivery rather than peeking.
    let box_now = s.drain().await;
    box_now.says("\"seen_before\":true");

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
