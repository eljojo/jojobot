//! "Keep track of my bikes — what they are, what's been done to them, how much
//! I ride them, and what's still under warranty."
//!
//! A thing with a long life rather than a project with an end, so what it
//! probes is accumulation: the same measurement year after year, parts
//! replaced under it, and the questions asked months later.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn keeping_track_of_bikes() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · what they are ───────────────────────────────────────────
    let s = story.session().await;

    s.add("thing:gravel-bike", "Gravel Bike").await;
    s.fact("thing:gravel-bike", "ridden most weeks").await;

    s.fact_on("thing:gravel-bike", "purchased new", "2024-04-11")
        .await;
    s.fact(
        "thing:gravel-bike",
        "frame warranty runs five years from purchase",
    )
    .await;

    // GAP — that date went into the only date field there is, which means when
    // the claim became known. The purchase happened on it, and nothing
    // distinguishes the two, so the store holds a date whose meaning can only
    // be recovered by reading the sentence beside it.
    //   s.event("thing:gravel-bike", "purchased", happened_at: "2024-04-11").await;

    // GAP — and the warranty's end is purchase plus five years, arithmetic on
    // a date the system cannot see as a date. "What is still under warranty"
    // is a query across every possession by a date property, and there is no
    // property to query.
    //   s.fact_with("thing:gravel-bike", "frame warranty", "expires", "2029-04-11").await;

    s.add("thing:road-bike", "Road Bike").await;
    s.fact(
        "thing:road-bike",
        "hanging in the basement, unridden for two years",
    )
    .await;
    s.fact("thing:road-bike", "needs tyres before it can be sold")
        .await;

    // GAP — both of those are STATES rather than descriptions. Nothing carries
    // state, so they read as permanent truths about the bike and will still
    // read that way after it is sold.

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

    // Who works where IS walkable, so the shop's people come back in one call
    // without knowing their names first.
    s.through("membership", "org:springfield-cyclery", "person")
        .await
        .says("person:milhouse");

    // A service IS an event with a type and fields, and it goes in as one:
    // what was done and when, as values, with `refs` naming who did it. "What
    // has been done to this bike" is one read of its record.
    let service = s
        .event_with(
            "thing:gravel-bike",
            "annual service",
            "service",
            json!({"done_on": "2026-04-18", "work": "chain, cables, bearings"}),
            &["person:milhouse"],
        )
        .await;
    s.recall("thing:gravel-bike")
        .await
        .claim(&service)
        .says("2026-04-18")
        .says("person:milhouse");

    // GAP — and no read orders them or takes the newest. "When did I last
    // service it" comes back as every service ever recorded, and the session
    // picks the latest date out by reading them.
    //   s.latest("thing:gravel-bike", event_type: "service").await;

    // GAP — the chain is a PART of the bike, not a fact about it. Parentage is
    // not reachable, so it cannot be its own thing with its own history, and
    // "how many km on the chain since I fitted it" has nothing to hang on.
    //   s.add_under("thing:gravel-bike", "thing:bike-chain", "Chain").await;

    s.wrap("service history, such as it is").await;

    // ── session 3 · the numbers, year after year ────────────────────────────
    let s = story.session().await;

    s.fact("thing:gravel-bike", "rode about 3,800 km in 2025")
        .await;
    s.fact("thing:gravel-bike", "rode 4,100 km so far in 2026")
        .await;
    s.fact("thing:road-bike", "rode 0 km in 2026").await;

    // GAP — those are the same measurement at three different times and
    // nothing says so. There is no series: three unrelated sentences, so
    // nothing can fetch the yearly tallies as a set or slice them by year.
    // Comparing them is the session's job; having something to compare is
    // jojobot's, and that is the half missing.
    //   s.series("thing:gravel-bike", "km ridden", &[("2025", "3800")]).await;

    s.wrap("tallies in").await;

    // ── session 4 · months later, the questions actually asked ──────────────
    let s = story.session().await;

    s.find("warranty").await.says("thing:gravel-bike");
    s.find("cassette").await.says("thing:gravel-bike");
    s.recall("thing:road-bike").await.says("basement");
    s.list("thing").await.says("thing:gravel-bike");

    // Custody is a fact: the pump is the entity, "loaned to" is the fact, and
    // it points at the person. A fact is current truth rewritten in place,
    // which is right — where a thing is now has no business accumulating.
    s.add("thing:floor-pump", "Floor Pump").await;
    s.fact_about(
        "thing:floor-pump",
        "loaned out, still not back",
        "about",
        "person:milhouse",
    )
    .await;
    s.find("loaned").await.says("thing:floor-pump");
    s.recall("thing:floor-pump").await.says("person:milhouse");

    // GAP — but the shape is `about`, the catch-all, because there is no name
    // for this link. The walk below returns everything pointed at this person
    // by any claim, lending or otherwise, so "what have I lent out, and to
    // whom" cannot be asked — only searched for in whatever words the note
    // happened to use.
    s.through("about", "person:milhouse", "thing")
        .await
        .says("thing:floor-pump");
    //   s.fact_keyed("thing:floor-pump", "loaned-to", "person:milhouse").await;

    s.wrap("still riding").await;

    // ── session 5 · one of them is sold ─────────────────────────────────────
    let s = story.session().await;

    s.fact("thing:road-bike", "sold — tyres thrown in, gone in March")
        .await;

    // Everything ever recorded about it is still current truth about a bike
    // that is no longer here, and still comes back in an ordinary read.
    s.recall("thing:road-bike")
        .await
        .says("sold")
        .says("hanging in the basement")
        .says("needs tyres before it can be sold");
    s.list("thing").await.says("thing:road-bike");

    // GAP — an entity has no end. Nothing says this one has left the
    // operator's life, so it goes on answering "what do I own", and its facts
    // go on reading as present tense. The alternatives available today are
    // both wrong: rewrite each claim into the past, which destroys the record,
    // or leave them, which is what happened here.
    //   s.closed("thing:road-bike", "sold in March").await;

    s.wrap("one sold, and the record cannot tell").await;

    story.finish().await;
}
