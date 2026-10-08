//! "Where did that come from? Not now — in two years, when I've forgotten I
//! ever told you, and the thing it came from has changed."
//!
//! A claim's source is either an entity or another claim: two shapes of
//! reference, not one wider one. An edge's object is an entity handle, and a
//! claim worked out from a claim has no entity to point at.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for, and
//! an assertion beside it goes red on the day the capability lands.
//!
//! `// NOTE —` marks something a SESSION did not do. jojobot answers nothing
//! about it, so no assertion can hold it and none pretends to — and it
//! proposes no call either, because there is no verb that would fix it.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_claim_names_where_it_came_from() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · a claim from a source ───────────────────────────────────
    let s = story.session().await;

    s.add("place:north-gorge", "North Gorge").await;

    // Three conceptual steps land as two calls: `capture` carries the edge in
    // the same write as the content, so recording the claim and pointing it at
    // its source are one act.
    s.add("thing:gorge-email", "An Email About The Gorge").await;
    let closure = s
        .fact_about(
            "place:north-gorge",
            "closed for resurfacing until spring, per an email",
            "about",
            "thing:gorge-email",
        )
        .await;

    s.wrap("the closure is on record, and where it came from")
        .await;

    // ── session 2 · a claim derived from a claim ────────────────────────────
    let s = story.session().await;

    // Derived from the closure claim rather than from an entity: the
    // fact-to-fact link is an address, never an edge's object.
    s.guess_from(
        "place:north-gorge",
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
    s.add("event:gorge-survey", "Gorge Survey").await;
    s.fact("event:gorge-survey", "found the loop safe for hikers")
        .await;

    let cleared = s
        .fact_about(
            "place:north-gorge",
            "cleared for hiking, per the survey",
            "about",
            "event:gorge-survey",
        )
        .await;

    // A second claim from the same survey, so the source has more than one
    // thing resting on it.
    s.add("org:north-gorge-club", "North Gorge Club").await;
    s.fact_about(
        "org:north-gorge-club",
        "reopened its Sunday walks, per the survey",
        "about",
        "event:gorge-survey",
    )
    .await;

    s.wrap("the clearance is on record, and where it came from")
        .await;

    // ── session 4 · the brief, and the challenge ────────────────────────────
    let s = story.session().await;

    // Everything about the place, testimony and inference side by side, in one
    // read.
    s.recall("place:north-gorge")
        .await
        .says("testimony")
        .says("inference");

    // Where did the clearance come from? Named, not a bare id — the entity it
    // traces to is in the same read as the claim itself.
    s.recall("place:north-gorge")
        .await
        .says("cleared for hiking")
        .says("event:gorge-survey");

    // The cyclists claim answers the same question even though what it traces
    // to is another claim. Checked as the field rather than as a loose
    // substring: the closure claim's address is in this read anyway, being a
    // fact in its own right.
    s.recall("place:north-gorge")
        .await
        .says("busy with cyclists")
        .says(&format!("\"derived_from\":\"{closure}\""));

    // And the question from the other end — what rests on this survey? — is
    // one walk, across kinds, without knowing either subject in advance.
    s.through("about", "event:gorge-survey", "place")
        .await
        .says("place:north-gorge");
    s.through("about", "event:gorge-survey", "org")
        .await
        .says("org:north-gorge-club");

    // And in one walk with no kind at all: "what rests on this" needs no
    // guess about what sorts of thing might be resting.
    s.through_any("about", "event:gorge-survey")
        .await
        .says("place:north-gorge")
        .says("org:north-gorge-club");

    s.wrap("caught up, and traceable in both directions").await;

    // ── session 5 · years later, conditions change ──────────────────────────
    let s = story.session().await;

    s.add("event:erosion-review", "Erosion Review").await;
    s.fact("event:erosion-review", "found erosion along the loop")
        .await;

    // Not a refutation — the survey was not wrong, conditions changed. Both
    // events stand; the earlier session's claim is archived and the current
    // truth stands beside it, drawn at the erosion review, so the record does
    // not go on tracing to a survey that no longer matches what it says.
    let closed = s
        .replace(
            &cleared,
            "closed pending repair, per the erosion review",
            json!({"shape": "about", "object": "event:erosion-review"}),
        )
        .await;

    // The claim that stands traces to the erosion review and not to the survey.
    // The archived original is still on the page, saying archived, with the
    // survey it was drawn from.
    let now = s.recall("place:north-gorge").await;
    now.claim(&closed)
        .says("closed pending repair")
        .says("event:erosion-review")
        .never_says("event:gorge-survey");
    now.claim(&cleared)
        .says("\"status\":\"archived\"")
        .says("event:gorge-survey");

    // Neither event is retracted — both happened, and both stay findable.
    s.find("gorge-survey").await.says("event:gorge-survey");
    s.find("erosion-review").await.says("event:erosion-review");

    // NOTE — and the club's walks still rest on the survey, untouched. One
    // claim was re-pointed by the session that happened to be looking at it;
    // the walk above would have found the other, and nothing ran it. A source
    // that stops holding does not reach what was built on it, so staleness
    // spreads exactly as far as somebody remembers to look.
    s.through("about", "event:gorge-survey", "org")
        .await
        .says("org:north-gorge-club");
    //   s.superseded("event:gorge-survey", by: "event:erosion-review").await;

    s.wrap("the record changed cleanly, and one claim was left behind")
        .await;

    // ── session 6 · the claim under a derivation is taken back ──────────────
    let s = story.session().await;

    // A jotting that should never have been filed. Taken back, with nothing
    // to replace it — archiving covers this the same way it would a claim a
    // later one replaced, because a derivation resting on either reads the
    // same marker.
    s.add("event:the-jotting", "The Jotting").await;
    let jotted = s
        .fact("event:the-jotting", "counted forty walkers on a Sunday")
        .await;
    s.guess_from(
        "place:north-gorge",
        "the loop is busiest at weekends",
        &jotted,
    )
    .await;

    // While the count stands, the derivation says so — a reader can act on it
    // without going to look at what it rests on.
    s.find("busiest at weekends")
        .await
        .says("\"source_standing\":\"stands\"");

    s.retract(&jotted, "the count was of the wrong gorge").await;

    // ⭐ The point of the whole story: the derivation is untouched and still
    // findable — a marker, never a deletion — and it now says what became of
    // the claim it was worked out from. Nobody had to remember to look.
    s.find("busiest at weekends")
        .await
        .says("the loop is busiest at weekends")
        .says("\"source_standing\":\"archived\"");

    s.wrap("a claim outlived the one under it, and says so")
        .await;

    story.finish().await;
}
