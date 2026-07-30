//! Where a claim came from, and whether it can still be answered.
//!
//! The recorded design says every edge carries provenance, that only four
//! shapes may be written today, and that naming the fact-to-fact set — a
//! claim pointing at another claim — is the open design task. This story
//! answers that from the transcript rather than from the model: five
//! sessions, each a real moment, each recorded the way it would actually
//! happen, to see what pointing at a source needs.
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
    s.fact_about(
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

    s.guess(
        "place:north-trail",
        "the loop will be busy with cyclists once it reopens",
    )
    .await;

    // GAP — this claim is derived FROM the closure claim above, not from
    // an entity, and there is nothing to point it at. An edge's object is
    // a kind:slug entity id; a fact's own address is not one. Pointing a
    // claim at another claim needs something an entity-to-entity edge does
    // not give: this is the fact-to-fact case the recorded design has not
    // named yet.
    // s.sourced_from("the cyclists claim", "the closure claim").await;

    s.wrap("worked out, and untraceable to what it was worked out from")
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

    // GAP — and the cyclists claim has no answer to the same question. It
    // reads as an inference, same as the clearance claim, but nothing
    // traces it to what it was worked out from — there is nothing to trace
    // it to, because what it was derived from is a claim, not an entity.

    s.wrap("caught up, mostly traceable").await;

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
