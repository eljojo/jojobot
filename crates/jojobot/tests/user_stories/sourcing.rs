//! Where a claim came from, and whether it can still be answered.
//!
//! Five sessions, each a real moment, each recorded the way it would
//! actually happen. A claim's source is either an entity (session 1's
//! email, session 3's survey) or another claim (session 2's derived
//! claim) — two different shapes of reference, not one wider one: an
//! edge's object is an entity id, and a claim derived from a claim has no
//! entity to point at.
//!
//! The subject is a place, never a person — people-related content
//! migrates last, unconditionally (rule 78), and nothing here needs one.
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn a_claim_names_where_it_came_from() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · a claim from a source ───────────────────────────────────
    let s = story.session().await;

    s.add("place:north-trail", "North Trail").await;

    // Three conceptual steps — create the source, record the claim, point
    // at the source — land as two calls, not three: `capture` already
    // carries an edge in the same call as its content, so recording the
    // claim and pointing it at the source are one write, not two. No batch
    // verb used or needed here.
    s.add("thing:trail-email", "An Email About The Trail").await;
    let closure = s
        .fact_about(
            "place:north-trail",
            "closed for resurfacing until spring, per an email",
            "about",
            "thing:trail-email",
        )
        .await;

    s.wrap("the closure is on record, and where it came from")
        .await;

    // ── session 2 · a claim derived from a claim ────────────────────────────
    let s = story.session().await;

    // Derived FROM the closure claim above, not from an entity — the
    // fact-to-fact link `guess_from` carries via `derived_from`, an
    // address, never an edge's `object`.
    s.guess_from(
        "place:north-trail",
        "the loop will be busy with cyclists once it reopens",
        &closure,
    )
    .await;

    s.wrap("worked out, and traceable to what it was worked out from")
        .await;

    // ── session 3 · a claim from an event ───────────────────────────────────
    let s = story.session().await;

    // The event, and its result as a fact about it — this half already
    // works today: `event` is an entity kind like any other, and a fact
    // can be about an event exactly as it can about a place.
    s.add("event:trail-survey", "Trail Survey").await;
    s.fact("event:trail-survey", "found the loop safe for hikers")
        .await;

    // The claim about the place, pointing at the event as its source —
    // same shape as session 1, because the source here is an entity too.
    let cleared = s
        .fact_about(
            "place:north-trail",
            "cleared for hiking, per the survey",
            "about",
            "event:trail-survey",
        )
        .await;

    s.wrap("the clearance is on record, and where it came from")
        .await;

    // ── session 4 · the brief, and the challenge ────────────────────────────
    let s = story.session().await;

    // The brief: everything about it, testimony and inference side by
    // side, in one read.
    s.recall("place:north-trail")
        .await
        .says("testimony")
        .says("inference");

    // The challenge: where did the clearance claim come from? Named, not a
    // bare id — the entity it traces to is right there in the same read
    // that shows the claim itself.
    s.recall("place:north-trail")
        .await
        .says("cleared for hiking")
        .says("event:trail-survey");

    // And the cyclists claim answers the same question, even though what
    // it traces to is another claim rather than an entity: named as that
    // claim's own address, not a bare id. Checked as the exact field
    // rendering — the closure claim's own address is present in this read
    // regardless (it is a fact in its own right), so a bare substring
    // check on the address alone would pass whether or not the cyclists
    // claim's `derived_from` actually carries it.
    s.recall("place:north-trail")
        .await
        .says("busy with cyclists")
        .says(&format!("\"derived_from\":\"{closure}\""));

    s.wrap("caught up, and traceable").await;

    // ── session 5 · years later, conditions change ──────────────────────────
    let s = story.session().await;

    s.add("event:erosion-review", "Erosion Review").await;
    s.fact("event:erosion-review", "found erosion along the loop")
        .await;

    // Not a refutation — the survey was not wrong, conditions changed.
    // Both events stand; the claim is rewritten to the current truth, and
    // its edge is re-pointed in the same call, so it does not go on tracing
    // to a survey that no longer matches what the claim now says.
    s.correct_with_source(
        &cleared,
        "closed pending repair, per the erosion review",
        "event:erosion-review",
    )
    .await;

    s.recall("place:north-trail")
        .await
        .says("closed pending repair")
        .says("event:erosion-review")
        .never_says("event:trail-survey");

    // Neither event is retracted — both happened, and both stay findable.
    s.find("trail-survey").await.says("event:trail-survey");
    s.find("erosion-review").await.says("event:erosion-review");

    s.wrap("the record changed cleanly, and both events still stand")
        .await;

    story.finish().await;
}
