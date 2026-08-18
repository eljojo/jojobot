//! "Something just landed on Thursday and I've already got two things that
//! day. Work out what gives."
//!
//! jojobot can hold each piece of a collision as it is decided — what was
//! already committed, what just arrived, what the replan settled. Whether it
//! can FIND the collision is a different question, and it turns on how the
//! commitments were written down: a day under a declared date key is a day a
//! question reaches, and prose is not. Every commitment here is prose, so the
//! collision below is found by the wording and not by the dates.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for, and
//! an assertion beside it goes red on the day the capability lands.
//!
//! `// NOTE —` marks something a SESSION did not do. jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to — and it
//! proposes no call either, because there is no verb that would fix it.

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

    // "Thursday" is prose in both, and the choice was this session's. The one
    // date a plain claim carries is still when it became known — but a day the
    // thing happens on goes under a key of its own on a typed record, and a
    // key declared to hold a date is comparable, so "what falls on the 14th"
    // is a question the surface takes. Neither commitment above was written
    // that way, which is what the next two sessions pay for.

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

    // GAP — and that is the whole mechanism HERE. Two commitments worded "the
    // 14th" and "Thursday" are the same day and would not collide in this
    // search at all: it asks what mentions a word. Asking what falls on a date
    // is served, over a declared date key — and it is scoped to that one key,
    // because the key name is the schema. So two sessions that recorded their
    // dates under different names still do not collide, which is the residual:
    // the day is askable and the agreement it needs is nobody's job.
    // A day asked for WITHOUT naming a key is the missing piece, and it would
    // be an argument on this read rather than a verb beside it.
    //   s.shape("what falls on the 14th",
    //           json!({"date": "2027-01-14"})).await;
    s.has_no_argument("recall", "date", &["fields", "compare"])
        .await;

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

    // NOTE — the decision is entirely the session's. jojobot recorded three
    // sentences it was handed and checked none of them against the other two;
    // there is no schedule here for anything to collide against.

    // GAP — and the trade-off is not on the record. What a decision chose over
    // is nameable when the alternative is an ENTITY: a key declared to hold a
    // reference points the record at the party it gave up, and the walk goes
    // both ways. What it chose over here is a CLAIM — the postponed festival
    // is a fact address — and a reference holds a handle. `derived_from` is
    // the one claim-to-claim link and it says derived, never chosen over. So
    // the reasoning that survives is three unconnected sentences.
    //   s.decided("project:atlas", "…", instead_of: "event:winter-fest").await;
    s.has_no_verb("decide", &["capture", "update_fact"]).await;
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
    // not one call, and that is the wording's fault rather than the surface's.
    // A window IS one call over a declared date key: two filters on the same
    // key, after the Monday and before the Sunday, both holding on one record.
    // The residual is what the window cannot span — records whose date sits
    // under a different key name, and the claim's own date, which no filter
    // reaches.
    // Both residuals are the same absent argument: a day the read asks for
    // itself, rather than a key the caller has to name and every writer has to
    // have agreed on.
    //   s.shape("the week of the 11th",
    //           json!({"date": {"after": "2027-01-11", "before": "2027-01-18"}})).await;
    s.has_no_argument("recall", "date", &["fields", "compare"])
        .await;

    s.wrap("the week has a new shape, told in three pieces")
        .await;

    story.finish().await;
}
