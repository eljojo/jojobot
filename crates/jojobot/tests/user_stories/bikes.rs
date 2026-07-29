//! "Keep track of my bikes." Make, warranty, parts, and how much they get ridden.
//!
//! A thing with a long life rather than a project with an end — so it probes
//! accumulation: the same measurement year after year, parts replaced under it,
//! and the questions you ask months later.
//!
//! `// GAP:` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn keeping_track_of_bikes() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · what they are ───────────────────────────────────────────
    let s = story.session().await;

    s.add("thing:gravel-bike", "Gravel Bike").await;
    s.fact("thing:gravel-bike", "ridden most weeks").await;

    // The purchase is a fact, not a clause inside another sentence — and it is
    // an EVENT, of type `purchased`. The date it carries is when it happened.
    s.fact_on("thing:gravel-bike", "purchased new", "2024-04-11")
        .await;
    s.fact(
        "thing:gravel-bike",
        "frame warranty runs five years from purchase",
    )
    .await;

    // GAP — that date went into the only date field there is, which means when
    // the CLAIM became known. The purchase happened on it. Nothing distinguishes
    // the two, so the store now holds a date whose meaning you can only recover
    // by reading the sentence beside it. This is rule 101 in code rather than in
    // prose, and it is what `happened_at` and `recorded_at` are for.
    // s.event("thing:gravel-bike", "purchased", "happened_at", "2024-04-11").await;

    // GAP — and the warranty's end is purchase plus five years, which is
    // arithmetic on a date the system cannot see as a date. "What is still under
    // warranty" is a query across every possession by a date property.

    s.add("thing:road-bike", "Road Bike").await;
    s.fact(
        "thing:road-bike",
        "hanging in the basement, unridden for two years",
    )
    .await;
    s.fact("thing:road-bike", "needs tyres before it can be sold")
        .await;

    // GAP — "unridden for two years" and "needs tyres before selling" are both
    // STATES, not descriptions. Nothing carries state, so both read as permanent
    // truths about the bike and will still read that way after it is sold.

    // GAP — the warranty's end date exists in the prose and nowhere the system
    // can reach. "What is still under warranty?" is a query across every
    // possession by a date property, and there is no property to query.
    // s.fact_with("thing:gravel-bike", "frame warranty", "expires", "2029-04-01").await;

    s.wrap("both bikes recorded").await;

    // ── session 2 · the shop, and the work it did ───────────────────────────
    let s = story.session().await;

    s.add("org:springfield-cyclery", "Springfield Cyclery")
        .await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact_about(
        "person:milhouse",
        "the mechanic who actually knows the bike",
        "membership",
        "org:springfield-cyclery",
    )
    .await;
    s.fact(
        "thing:gravel-bike",
        "chain and cassette both replaced last spring at the cyclery",
    )
    .await;

    // GAP — the service is an EVENT with a type and fields: who did it, what was
    // done, when. Rule 80's own worked example. Recorded as prose, "when did I
    // last service it" cannot be answered, and neither can "what has been done
    // to this bike", which is that event class filtered to one subject.
    // s.event_typed("service", "2026-04-18", &[("by", "person:milhouse"), ("did", "chain, cassette")]).await;

    // GAP — the chain is a PART of the bike, not a fact about it. Parentage is
    // not reachable, so it cannot be its own thing with its own history, and
    // "how many km on the chain since I fitted it" has nothing to hang on.
    // s.add_under("thing:gravel-bike", "thing:bike-chain", "Chain").await;

    s.wrap("service history, such as it is").await;

    // ── session 3 · the numbers, year after year ────────────────────────────
    let s = story.session().await;

    s.fact("thing:gravel-bike", "rode about 3,800 km in 2025")
        .await;
    s.fact("thing:gravel-bike", "rode 4,100 km so far in 2026")
        .await;
    s.fact("thing:road-bike", "rode 0 km in 2026").await;

    // GAP — THE ONE THIS STORY EXISTS FOR. Those are the same measurement at
    // three different times and nothing says so. There is no SERIES: they are
    // three unrelated sentences, so nothing can fetch "the yearly tallies" as a
    // set, slice them by year, or hand them to an agent to compare. The
    // comparing is the agent's job and always was; having something to compare
    // is jojobot's, and that is what is missing.
    // s.series("thing:gravel-bike", "km ridden", &[("2025", "3800"), ("2026", "4100")]).await;

    s.wrap("tallies in").await;

    // ── session 4 · months later, the questions you actually ask ────────────
    let s = story.session().await;

    s.find("warranty").await.says("thing:gravel-bike");
    s.find("cassette").await.says("thing:gravel-bike");
    s.recall("thing:road-bike").await.says("basement");
    s.list("thing").await.says("thing:gravel-bike");

    // Custody IS a fact: the pump is the entity, "loaned to" is the fact, and it
    // connects to the person. And a fact is current truth rewritten in place,
    // which is exactly right — where a thing is now has no business accumulating.
    s.add("thing:floor-pump", "Floor Pump").await;
    s.fact_about(
        "thing:floor-pump",
        "loaned to him, still not back",
        "about",
        "person:milhouse",
    )
    .await;
    s.find("loaned").await.says("thing:floor-pump");
    s.recall("thing:floor-pump").await.says("person:milhouse");

    // GAP — but the shape is `about`, the vague catch-all, because there is no
    // name for this link. So the edge is walkable and MEANINGLESS: "what have I
    // lent out, and to whom" cannot be asked, only searched for in whatever words
    // the note happened to use. What it wants is the annotation the payload work
    // is already introducing — a KEY on the edge:
    // s.fact_keyed("thing:floor-pump", "loaned-to", "person:milhouse").await;

    s.wrap("still riding").await;

    story.finish().await;
}
