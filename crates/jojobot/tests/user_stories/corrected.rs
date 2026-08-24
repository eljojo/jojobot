//! "The record says the club does not meet on Tuesdays. Has it always said
//! that, or did we have it wrong and fix it?"
//!
//! **A question a person actually asks, and it is not about the claim.** It is
//! about whether jojobot ever thought otherwise. The answer that is true today
//! is the same either way; what changes is whether the operator learns that
//! their own record misled them for months, or never finds out.
//!
//! ⭐ **The value is the sentence the assistant can add:** *and you should know
//! the record said the opposite until September, so anything you decided off it
//! before then was decided off the wrong thing.* Without a trace, that sentence
//! cannot be said by anybody, and the same correct answer arrives with the one
//! thing somebody would want to know missing from it.
//!
//! ⚠️ **The negative is what gives it meaning.** A claim nobody corrected comes
//! back as its own single write. Without that half, a read that returned a
//! chain for everything would pass — and it would tell a reader that jojobot
//! changed its mind about every claim it holds, which is worse than saying
//! nothing.
//!
//! ⛔️ **Nothing is rendered unasked.** The ordinary read carries no chain at
//! all; a session asks for one when the question is worth the page.
//!
//! **The runs state the days they work in**, because this is a story about
//! months passing and a run that says nothing is a run happening now.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_reader_can_tell_a_corrected_record_from_one_that_was_always_right() {
    let story = Story::begin("bot:otto").await;

    // ── April · the club, as the operator describes it ──────────────────────
    let spring = story.session_on("2026-04-14", None).await;

    spring.add("org:north-trail-club", "North Trail Club").await;
    let meets = spring
        .fact("org:north-trail-club", "meets on Tuesdays")
        .await;
    // A second claim, written the same day and never touched again. It is the
    // control this whole story rests on.
    let rides = spring
        .fact(
            "org:north-trail-club",
            "rides out from the trailhead car park",
        )
        .await;

    spring.wrap("wrote down what the club does").await;

    // ── September · the operator says we had it wrong ───────────────────────
    //
    // Not a change in the world: the club never met on Tuesdays and the record
    // was wrong from the day it was written. That is the rewrite case — the
    // record states what is true now, and there is one claim rather than two.
    let autumn = story.session_on("2026-09-15", Some("new")).await;

    autumn
        .correct(
            &meets,
            "does not meet on Tuesdays — it never did, we had that wrong",
        )
        .await;
    autumn
        .recall("org:north-trail-club")
        .await
        .claim(&meets)
        .says("does not meet on Tuesdays");

    autumn.wrap("corrected the meeting day").await;

    // ── December · a fresh session, and the question ────────────────────────
    let winter = story.session_on("2026-12-20", Some("new")).await;

    // The ordinary read first: what the record says now, and no chain with it.
    // ⛔️ A read that carried the trace unasked would cost every session that
    // never wanted it.
    let now = winter.recall("org:north-trail-club").await;
    now.claim(&meets).says("does not meet on Tuesdays");
    assert!(
        now.json()["objects"][0].get("record_history").is_none(),
        "the trace arrived without being asked for: {}",
        now.raw(),
    );

    // **Then the question.** The subject rides along because the read wants a
    // selection as well as a trace.
    let asked = winter
        .shape(
            "has the record always said that?",
            json!({
                "subject": "org:north-trail-club",
                "history_record": meets,
            }),
        )
        .await;

    // ⭐ **The answer: it has not.** Two writes, oldest first, and the first one
    // says the opposite of what the record says now.
    asked
        .number("/objects/0/record_history/count", 2)
        .number("/objects/0/record_history/writes/0/nth", 1)
        .number("/objects/0/record_history/writes/1/nth", 2);
    assert_eq!(
        asked.json()["objects"][0]["record_history"]["writes"][0]["content"],
        "meets on Tuesdays",
        "what the record used to say is not in its own trace: {}",
        asked.raw(),
    );
    assert_eq!(
        asked.json()["objects"][0]["record_history"]["writes"][1]["content"],
        "does not meet on Tuesdays — it never did, we had that wrong",
    );

    // ⚠️ **What the trace does NOT say, so the story does not imply it**: no
    // write carries a moment. Every write of a claim keeps the moment the claim
    // first entered the store, so the substrate knows the ORDER and not the day
    // each correction happened. A session telling the operator *the record was
    // wrong until September* is reading its own chronology, not this.
    assert!(
        asked.json()["objects"][0]["record_history"]["writes"][0]
            .get("inserted_at")
            .is_none(),
        "a write reports a moment the substrate does not record: {}",
        asked.raw(),
    );

    // ⚠️ **The claim nobody corrected.** One write, its own first, and nothing
    // more — so a chain means somebody changed their mind rather than meaning
    // the read has a chain for everything.
    let untouched = winter
        .shape(
            "and has this one ever changed?",
            json!({
                "subject": "org:north-trail-club",
                "history_record": rides,
            }),
        )
        .await;
    untouched
        .number("/objects/0/record_history/count", 1)
        .number("/objects/0/record_history/writes/0/nth", 1);
    assert_eq!(
        untouched.json()["objects"][0]["record_history"]["writes"][0]["content"],
        "rides out from the trailhead car park",
    );

    winter
        .wrap("answered what the record says, and that it once said the opposite")
        .await;

    story.finish().await;
}
