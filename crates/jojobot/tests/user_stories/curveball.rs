//! Something lands that collides with what was already planned — one of the
//! most common sessions there actually is, and there was no story for it.
//!
//! What this proves: jojobot can hold each piece of a collision as it is
//! decided — what was already committed, what just arrived, and what the
//! replan settled. What it does not prove is that jojobot can FIND the
//! collision itself: that needs a question over a time range, across
//! everything, and no such question can be asked today.
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn a_curveball_collides_with_the_week() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the week as it stands ───────────────────────────────────
    let s = story.session().await;

    s.add("event:winter-fest", "Winter Fest").await;
    let winter_fest = s
        .fact("event:winter-fest", "already on the calendar for Thursday")
        .await;

    s.add("project:atlas", "The Report").await;
    s.fact("project:atlas", "draft due by Thursday, and it matters")
        .await;

    // GAP — "Thursday" is prose in both facts, not a date either can be
    // asked about. A fact's date is when the CLAIM became known, not when
    // the thing it describes happens — the same absence the moving story
    // already found on a flight's departure date. Nothing here can be asked
    // "what happens this week" from a date field, because neither commitment
    // has one.

    s.wrap("the week as it already stood").await;

    // ── session 2 · the curveball arrives ───────────────────────────────────
    let s = story.session().await;

    s.add("person:ned-flanders", "Ned").await;
    s.add("event:birthday-party", "Ned's Party").await;
    s.fact(
        "event:birthday-party",
        "Ned is throwing it and wants him there, also Thursday",
    )
    .await;

    s.wrap("invited, and it lands on the same day").await;

    // ── session 3 · what it collides with ───────────────────────────────────
    let s = story.session().await;

    // Worth proving rather than assuming: a plain word search DOES surface
    // both commitments together, because both happen to say "Thursday".
    s.find("Thursday")
        .await
        .says("event:winter-fest")
        .says("event:birthday-party");

    // GAP — and that is the whole mechanism, which is why it is not one.
    // Two commitments worded "the 14th" and "Thursday" are the same day and
    // would not collide in this search at all. Nothing here asks "what is
    // scheduled for this date" — only "what mentions this word" — so the
    // collision was found by wording landing on the same word, not by jojobot
    // knowing the two happen at the same time.

    s.wrap("both commitments in view, side by side by luck of the wording")
        .await;

    // ── session 4 · the replan ──────────────────────────────────────────────
    let s = story.session().await;

    // What moves: postponed in place, not appended beside the old claim.
    s.correct(
        &winter_fest,
        "postponed a week to make room for Ned's party",
    )
    .await;

    // What is accepted: a new claim, not a correction — nothing here was wrong.
    s.fact(
        "event:birthday-party",
        "confirmed attending, per the replan",
    )
    .await;

    // What is protected: stated as a fact like everything else here.
    s.fact(
        "project:atlas",
        "draft deadline is protected — nothing else moves ahead of it",
    )
    .await;

    // GAP — the decision above is entirely the agent's: what to keep, what
    // to move, and why. jojobot recorded three sentences it was handed and
    // checked none of them against the other two — there is no schedule
    // here for anything to collide against in the first place.

    s.wrap("replanned, and the decision is on record").await;

    // ── session 5 · after ────────────────────────────────────────────────────
    let s = story.session().await;

    // The new shape of the week, read back — each piece real, on its own.
    s.recall("event:winter-fest").await.says("postponed");
    s.recall("event:birthday-party")
        .await
        .says("confirmed attending");
    s.recall("project:atlas").await.says("protected");

    // GAP — "the new shape of the week" is three reads and an assembly, not
    // one call. Composing them into a single answer is the agent's job, the
    // same division of labour the bikes and sourcing stories already found:
    // having something to compare is jojobot's; comparing it is not.

    s.wrap("the week has a new shape, told in three pieces")
        .await;

    story.finish().await;
}
