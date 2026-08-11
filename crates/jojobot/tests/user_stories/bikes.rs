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
    s.has_no_verb("record_occurrence", &["capture", "recall"])
        .await;

    // Purchase plus five years is arithmetic, and the arithmetic is the
    // session's — but its ANSWER goes in as a value under a key rather than as
    // another sentence, so the date the question turns on is somewhere a
    // question can reach it.
    s.event_with(
        "thing:gravel-bike",
        "frame warranty",
        "warranty",
        json!({"expires": "2029-04-11"}),
        &[],
    )
    .await;
    s.recall("thing:gravel-bike")
        .await
        .says("frame warranty runs five years from purchase")
        .says("2029-04-11");

    s.add("thing:road-bike", "Road Bike").await;
    s.fact(
        "thing:road-bike",
        "hanging in the basement, unridden for two years",
    )
    .await;
    // The other side of the question below: a bike whose cover has already run
    // out. Without one, "what is still under warranty" would come back with
    // everything and look like an answer.
    s.event_with(
        "thing:road-bike",
        "frame warranty",
        "warranty",
        json!({"expires": "2025-06-30"}),
        &[],
    )
    .await;
    // The claim stays one crisp line, and the nuance that would ruin it as a
    // claim rides beside it instead of being folded into the sentence.
    s.call(
        "capture",
        json!({
            "subject": "thing:road-bike",
            "content": "needs tyres before it can be sold",
            "details": "the rear rim is worn too, so tyres alone may not be enough",
            "provenance": "testimony",
        }),
    )
    .await;
    s.recall("thing:road-bike")
        .await
        .says("\"content\":\"needs tyres before it can be sold\"")
        .says("the rear rim is worn too");

    // GAP — both of those are STATES rather than descriptions. Nothing carries
    // state, so they read as permanent truths about the bike and will still
    // read that way after it is sold.
    //   s.state("thing:road-bike", "for sale").await;
    s.recall("thing:road-bike")
        .await
        .says("hanging in the basement")
        .never_says("\"state\"");

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
    s.has_no_verb("latest", &["search", "recall"]).await;

    // GAP — the chain is a PART of the bike, not a fact about it. Parentage is
    // not reachable, so it cannot be its own thing with its own history, and
    // "how many km on the chain since I fitted it" has nothing to hang on.
    //   s.add_under("thing:gravel-bike", "thing:bike-chain", "Chain").await;
    //
    // Asking for it anyway is refused, and the refusal names the argument.
    // The alternative is worse than the gap: a flat entity created and success
    // reported, which leaves a caller unable to tell "parentage is not on the
    // surface yet" from "I set it and it worked".
    s.refused(
        "add_entity",
        json!({
            "kind": "thing", "handle": "bike-chain", "name": "Chain",
            "source": "user-named", "parent": "thing:gravel-bike",
        }),
    )
    .await
    .says("parent")
    // A blocked ANSWER with a way forward, not a schema error thrown back at
    // the client: `wrote` is the field only the refusal carries, and a
    // deserializer failing would never reach it.
    .says("\"wrote\":false");
    s.list("thing").await.never_says("thing:bike-chain");

    // The same creation without it lands, so the refusal is about the argument
    // and not about the entity. The chain goes in as its own thing and lands
    // flat: nothing on the created record says what it is part of.
    s.add("thing:bike-chain", "Chain").await;
    s.list("thing")
        .await
        .says("thing:bike-chain")
        .never_says("parent");

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
    s.has_no_verb("series", &["capture", "search"]).await;

    s.wrap("tallies in").await;

    // ── session 4 · months later, the questions actually asked ──────────────
    let s = story.session().await;

    s.find("warranty")
        .await
        .says("thing:gravel-bike")
        .says("thing:road-bike");
    s.find("cassette").await.says("thing:gravel-bike");

    // The title question — and the search above is not it. The word is on both
    // bikes and it cannot say which cover has run out.
    //
    // Declaring `warranty` is what makes the stored date orderable, and it is
    // retroactive — the two records were written three sessions ago by a
    // client that had never heard of the type.
    s.call(
        "declare_type",
        json!({
            "name": "warranty",
            "fields": [{ "key": "expires", "holds": "date" }],
        }),
    )
    .await
    .says("\"name\":\"warranty\"");

    let covered = s
        .shape(
            "what is still under warranty",
            json!({ "fields": [{ "key": "expires", "compare": "after", "value": "2026-08-01" }] }),
        )
        .await;
    covered.says("thing:gravel-bike");
    covered.never_says("thing:road-bike");

    s.recall("thing:road-bike").await.says("basement");
    s.list("thing").await.says("thing:gravel-bike");

    // Custody is a fact: the pump is the entity, "loaned to" is the fact, and
    // it points at the person. A fact is current truth rewritten in place,
    // which is right — where a thing is now has no business accumulating.
    s.add("thing:floor-pump", "Floor Pump").await;
    s.fact_about(
        "thing:floor-pump",
        "loaned out, still not back",
        "connection",
        "person:milhouse",
    )
    .await;
    s.find("loaned").await.says("thing:floor-pump");
    s.recall("thing:floor-pump").await.says("person:milhouse");

    // The shape is `connection`, which says a link is there and that how it
    // relates was not recorded. That is the honest shape for a loan and it is
    // no name for one — and the traffic goes both ways, which is what makes
    // the difference bite: his torque wrench is here, pointed at him by the
    // very same shape, and it is the opposite arrangement.
    s.add("thing:torque-wrench", "Torque Wrench").await;
    s.fact_about(
        "thing:torque-wrench",
        "his, borrowed for the bottom bracket and not given back yet",
        "connection",
        "person:milhouse",
    )
    .await;
    let linked = s.through("connection", "person:milhouse", "thing").await;
    linked.says("thing:floor-pump");
    linked.says("thing:torque-wrench");

    // So "what have I lent out, and to whom" needs the link NAMED, and a key
    // is where a name goes: `loaned_to` holds his handle, and declaring it to
    // hold a reference is what turns the key into a relation the query can
    // travel.
    //
    // It rides on an event because an event is where keys live — a plain fact
    // carries none — so the lending goes down as chronology and the claim
    // above stays the current truth beside it.
    s.event_with(
        "thing:floor-pump",
        "lent out at the spring service",
        "loan",
        json!({"loaned_to": "person:milhouse"}),
        &[],
    )
    .await;
    s.call(
        "declare_type",
        json!({
            "name": "loan",
            "fields": [{ "key": "loaned_to", "holds": "reference" }],
        }),
    )
    .await
    .says("\"name\":\"loan\"");

    // Walked inbound from him: what he has of mine, which is not what I have
    // of his. The wrench is in the untyped walk above and out of this one, and
    // that pair is the whole difference between a link and a named link.
    let lent = s
        .shape(
            "what he has of mine",
            json!({
                "subject": "person:milhouse",
                "facts": false,
                "follow": { "relation": "loaned_to", "direction": "in" },
            }),
        )
        .await;
    lent.says("thing:floor-pump");
    lent.never_says("thing:torque-wrench");

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
    s.has_no_verb("close_entity", &["update_entity", "list_entities"])
        .await;

    s.wrap("one sold, and the record cannot tell").await;

    story.finish().await;
}
