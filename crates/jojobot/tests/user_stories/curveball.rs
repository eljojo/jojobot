//! "Something just landed on Thursday and I've already got two things that
//! day. Work out what gives."
//!
//! jojobot can hold each piece of a collision as it is decided — what was
//! already committed, what just arrived, what the replan settled. Whether it
//! can FIND the collision is a different question, and it needs one nothing can
//! ask yet.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

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

    // GAP — "Thursday" is prose in both, not a date either can be asked about.
    // The one date a claim carries is when it became known, not when the thing
    // it describes happens, so neither commitment has a day the system can see.
    //   s.due("project:atlas", "2027-01-14").await;

    s.wrap("the week as it already stood").await;

    // ── session 2 · the curveball arrives ───────────────────────────────────
    let s = story.session().await;

    s.add("person:ned-flanders", "Ned").await;
    s.add("event:birthday-party", "Ned's Party").await;
    s.fact(
        "event:birthday-party",
        "Ned is throwing it and wants them there, also Thursday",
    )
    .await;

    s.wrap("invited, and it lands on the same day").await;

    // ── session 3 · what it collides with ───────────────────────────────────
    let s = story.session().await;

    // A plain word search does surface both commitments together, because both
    // happen to say "Thursday".
    s.find("Thursday")
        .await
        .says("event:winter-fest")
        .says("event:birthday-party");

    // GAP — and that is the whole mechanism. Two commitments worded "the 14th"
    // and "Thursday" are the same day and would not collide in this search at
    // all: nothing asks what is scheduled for a date, only what mentions a
    // word. The collision was found by the wording matching, not by jojobot
    // knowing the two happen at once.
    //   s.on_day("2027-01-14").says("event:winter-fest").await;

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
    let accepted = s
        .fact(
            "event:birthday-party",
            "confirmed attending, per the replan",
        )
        .await;

    // What is protected.
    s.fact(
        "project:atlas",
        "draft deadline is protected — nothing else moves ahead of it",
    )
    .await;

    // GAP — the decision is entirely the session's. jojobot recorded three
    // sentences it was handed and checked none of them against the other two;
    // there is no schedule here for anything to collide against.

    // GAP — and the trade-off is not on the record. The claim says what was
    // decided and cannot say what it was decided AGAINST, so the reasoning
    // that survives is three unconnected sentences. A decision has nowhere to
    // name what it chose over what.
    let _ = &accepted;
    //   s.decided(&accepted, over: &[&winter_fest], because: "the draft is fixed").await;

    s.wrap("replanned, and the decision is on record").await;

    // ── session 5 · after ───────────────────────────────────────────────────
    let s = story.session().await;

    // The new shape of the week, read back — each piece real, on its own.
    s.recall("event:winter-fest").await.says("postponed");
    s.recall("event:birthday-party")
        .await
        .says("confirmed attending");
    s.recall("project:atlas").await.says("protected");

    // GAP — but "the new shape of the week" is three reads and an assembly,
    // not one call. Composing them is the session's job; having something to
    // compose is jojobot's.
    //   s.week_of("2027-01-11").await;

    s.wrap("the week has a new shape, told in three pieces")
        .await;

    story.finish().await;
}
