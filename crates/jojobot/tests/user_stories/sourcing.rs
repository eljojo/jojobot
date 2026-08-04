//! "Where did that come from? Not now — in two years, when I've forgotten I
//! ever told you, and the thing it came from has changed."
//!
//! A claim's source is either an entity or another claim: two shapes of
//! reference, not one wider one. An edge's object is an entity handle, and a
//! claim worked out from a claim has no entity to point at.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use super::dsl::Story;

#[tokio::test]
async fn a_claim_names_where_it_came_from() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · a claim from a source ───────────────────────────────────
    let s = story.session().await;

    s.add("place:north-trail", "North Trail").await;

    // Three conceptual steps land as two calls: `capture` carries the edge in
    // the same write as the content, so recording the claim and pointing it at
    // its source are one act.
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

    // Derived from the closure claim rather than from an entity: the
    // fact-to-fact link is an address, never an edge's object.
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

    // An event is an entity kind like any other, and a fact can be about one
    // exactly as it can about a place.
    s.add("event:trail-survey", "Trail Survey").await;
    s.fact("event:trail-survey", "found the loop safe for hikers")
        .await;

    let cleared = s
        .fact_about(
            "place:north-trail",
            "cleared for hiking, per the survey",
            "about",
            "event:trail-survey",
        )
        .await;

    // A second claim from the same survey, so the source has more than one
    // thing resting on it.
    s.add("org:north-trail-club", "North Trail Club").await;
    s.fact_about(
        "org:north-trail-club",
        "reopened its Sunday walks, per the survey",
        "about",
        "event:trail-survey",
    )
    .await;

    s.wrap("the clearance is on record, and where it came from")
        .await;

    // ── session 4 · the brief, and the challenge ────────────────────────────
    let s = story.session().await;

    // Everything about the place, testimony and inference side by side, in one
    // read.
    s.recall("place:north-trail")
        .await
        .says("testimony")
        .says("inference");

    // Where did the clearance come from? Named, not a bare id — the entity it
    // traces to is in the same read as the claim itself.
    s.recall("place:north-trail")
        .await
        .says("cleared for hiking")
        .says("event:trail-survey");

    // The cyclists claim answers the same question even though what it traces
    // to is another claim. Checked as the field rather than as a loose
    // substring: the closure claim's address is in this read anyway, being a
    // fact in its own right.
    s.recall("place:north-trail")
        .await
        .says("busy with cyclists")
        .says(&format!("\"derived_from\":\"{closure}\""));

    // And the question from the other end — what rests on this survey? — is
    // one walk, across kinds, without knowing either subject in advance.
    s.through("about", "event:trail-survey", "place")
        .await
        .says("place:north-trail");
    s.through("about", "event:trail-survey", "org")
        .await
        .says("org:north-trail-club");

    // GAP — one walk per kind, because the filter narrows to a kind and there
    // is no way to ask for every subject regardless. "What rests on this" is a
    // question about the source, and answering it means knowing beforehand
    // what sorts of thing might be resting.
    //   s.through_any("about", "event:trail-survey").await;

    s.wrap("caught up, and traceable in both directions").await;

    // ── session 5 · years later, conditions change ──────────────────────────
    let s = story.session().await;

    s.add("event:erosion-review", "Erosion Review").await;
    s.fact("event:erosion-review", "found erosion along the loop")
        .await;

    // Not a refutation — the survey was not wrong, conditions changed. Both
    // events stand; the claim is rewritten to current truth and its edge
    // re-pointed in the same call, so it does not go on tracing to a survey
    // that no longer matches what it says.
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

    // GAP — and the club's walks still rest on the survey, untouched. One
    // claim was re-pointed by the session that happened to be looking at it;
    // the walk above would have found the other, and nothing ran it. A source
    // that stops holding does not reach what was built on it, so staleness
    // spreads exactly as far as somebody remembers to look.
    s.through("about", "event:trail-survey", "org")
        .await
        .says("org:north-trail-club");
    //   s.superseded("event:trail-survey", by: "event:erosion-review").await;

    s.wrap("the record changed cleanly, and one claim was left behind")
        .await;

    story.finish().await;
}
