//! Where a claim came from, and whether it can still be answered.
//!
//! The recorded design says every edge carries provenance, that only four
//! shapes may be written today, and that naming the fact-to-fact set — a
//! claim pointing at another claim — is the open design task. This story
//! answers that from the transcript rather than from the model: five
//! sessions, each a real moment, each recorded the way it would actually
//! happen, to see what pointing at a source needs.
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn a_claim_names_where_it_came_from() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · a claim from a source ───────────────────────────────────
    let s = story.session().await;

    s.add("person:milhouse", "Milhouse").await;

    // Three conceptual steps — create the source, record the claim, point
    // at the source — land as two calls, not three: `capture` already
    // carries an edge in the same call as its content, so recording the
    // claim and pointing it at the source are one write, not two. No batch
    // verb used or needed here.
    s.add("thing:marathon-email", "An Email From Milhouse")
        .await;
    s.fact_about(
        "person:milhouse",
        "running a marathon in October, per his email",
        "about",
        "thing:marathon-email",
    )
    .await;

    s.wrap("the marathon is on record, and where it came from")
        .await;

    // ── session 2 · a claim derived from a claim ────────────────────────────
    let s = story.session().await;

    s.guess(
        "person:milhouse",
        "won't want a heavy dinner the week before the marathon",
    )
    .await;

    // GAP — this claim is derived FROM the marathon claim above, not from
    // an entity, and there is nothing to point it at. An edge's object is
    // a kind:slug entity id; a fact's own address is not one. Pointing a
    // claim at another claim needs something an entity-to-entity edge does
    // not give: this is the fact-to-fact case the recorded design has not
    // named yet.
    // s.sourced_from("the dinner claim", "the marathon claim").await;

    s.wrap("worked out, and untraceable to what it was worked out from")
        .await;

    // ── session 3 · a claim from an event ───────────────────────────────────
    let s = story.session().await;

    // The event, and its result as a fact about it — this half already
    // works today: `event` is an entity kind like any other, and a fact
    // can be about an event exactly as it can about a person.
    s.add("event:allergy-screening", "Allergy Screening").await;
    s.fact("event:allergy-screening", "came back positive for peanuts")
        .await;

    // The claim about the person, pointing at the event as its source —
    // same shape as session 1, because the source here is an entity too.
    let allergy = s
        .fact_about(
            "person:milhouse",
            "allergic to peanuts, per the test",
            "about",
            "event:allergy-screening",
        )
        .await;

    s.wrap("the allergy is on record, and where it came from")
        .await;

    // ── session 4 · the brief, and the challenge ────────────────────────────
    let s = story.session().await;

    // The brief: everything about him, testimony and inference side by
    // side, in one read.
    s.recall("person:milhouse")
        .await
        .says("testimony")
        .says("inference");

    // The challenge: where did the allergy claim come from? Named, not a
    // bare id — the entity it traces to is right there in the same read
    // that shows the claim itself.
    s.recall("person:milhouse")
        .await
        .says("allergic to peanuts")
        .says("event:allergy-screening");

    // GAP — and the dinner claim has no answer to the same question. It
    // reads as an inference, same as the allergy claim, but nothing traces
    // it to what it was worked out from — there is nothing to trace it to,
    // because what it was derived from is a claim, not an entity.

    s.wrap("caught up before dinner, mostly traceable").await;

    // ── session 5 · years later, the test is redone ─────────────────────────
    let s = story.session().await;

    s.add("event:allergy-clearance", "Allergy Clearance").await;
    s.fact("event:allergy-clearance", "came back negative for peanuts")
        .await;

    // Not a refutation — the first test was not wrong, the world changed.
    // Both events stand; the claim is rewritten to the current truth, and
    // its edge is re-pointed in the same call, so it does not go on tracing
    // to a test that no longer matches what the claim now says.
    s.correct_with_source(
        &allergy,
        "no longer allergic to peanuts, per the followup test",
        "event:allergy-clearance",
    )
    .await;

    s.recall("person:milhouse")
        .await
        .says("no longer allergic")
        .says("event:allergy-clearance")
        .never_says("event:allergy-screening");

    // Neither test is retracted — both happened, and both stay findable.
    s.find("allergy-screening")
        .await
        .says("event:allergy-screening");
    s.find("allergy-clearance")
        .await
        .says("event:allergy-clearance");

    s.wrap("the record changed cleanly, and both tests still stand")
        .await;

    story.finish().await;
}
