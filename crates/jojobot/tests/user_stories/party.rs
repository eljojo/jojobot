//! "I want to throw a birthday party. Help me work out who comes, what to
//! cook, and where."
//!
//! What it probes is what a graph should be best at: a set of people whose
//! state changes, a walk from an event to its guests to what they eat, and one
//! person reachable only through another.
//!
//! `// GAP —` marks what a beat needed and could not have. The commented-out
//! call is the missing capability, written the way it would be asked for.

use super::dsl::Story;

#[tokio::test]
async fn throwing_a_birthday_party() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · the idea ────────────────────────────────────────────────
    let s = story.session().await;

    s.add("person:bodoque", "Bodoque").await;
    s.add("event:birthday-party", "Bodoque's Birthday").await;
    s.fact_about(
        "event:birthday-party",
        "a birthday party for them, ten or twelve people",
        "about",
        "person:bodoque",
    )
    .await;

    s.add("place:moes", "Moe's").await;
    s.fact(
        "event:birthday-party",
        "either Moe's or my place; not decided",
    )
    .await;
    s.fact(
        "event:birthday-party",
        "asking people whether the 14th or the 21st works",
    )
    .await;

    // GAP — two venues and two dates, both under evaluation. Nothing can hold
    // candidates, rank them, or record one as ruled out, and choosing is what
    // planning mostly is.
    //   s.shortlist("the venue", &["place:moes"]).await;

    // GAP — the party has no date, because there is no field for one. A
    // claim's date is when it became known, and neither the 14th nor the 21st
    // has happened yet.
    //   s.happens_on("event:birthday-party", "2027-01-14").await;

    s.wrap("party sketched").await;

    // ── session 2 · the guests, and what they need ──────────────────────────
    let s = story.session().await;

    s.add("person:patana", "Patana").await;
    s.add("person:barney-gumble", "Barney").await;
    s.add("person:ned-flanders", "Ned").await;

    for guest in [
        "person:patana",
        "person:barney-gumble",
        "person:ned-flanders",
    ] {
        s.fact_about(
            guest,
            "invited to the party",
            "attendance",
            "event:birthday-party",
        )
        .await;
    }

    s.fact("person:patana", "vegetarian").await;
    s.fact("person:barney-gumble", "does not drink").await;
    s.fact("person:ned-flanders", "bringing a partner").await;

    // GAP — that partner is a PERSON, reachable only through Ned, and no shape
    // says one person stands in a relation to another: the four point at a
    // place, an org, an event, or vaguely at anything. For a system whose
    // hardest content is people, nothing models people to people, so "who is
    // Ned bringing" and "does anybody here not get along" are the same missing
    // edge.
    //   s.fact_about("person:ned-flanders", "their partner", "relation", "person:maude").await;

    s.wrap("invitations out").await;

    // ── session 3 · the replies trickle in ──────────────────────────────────
    let s = story.session().await;

    s.fact("person:patana", "coming to the party").await;

    // Not coming is information as load-bearing as coming, so it gets the same
    // edge. The shape says these two stand in an attendance relation; it does
    // not claim anybody is there, any more than `location` claims somebody is
    // still at a place. Which way it went lives in the fact.
    s.fact_about(
        "person:barney-gumble",
        "cannot make it, away that weekend",
        "attendance",
        "event:birthday-party",
    )
    .await;

    s.find("cannot make").await.says("person:barney-gumble");
    s.recall("person:barney-gumble")
        .await
        .says("away that weekend");

    // GAP — and here is what the walk costs. Asking the party for its guests
    // returns everyone who relates to it in one call, which is the graph doing
    // its job — and the one who is coming, the one who is not, and the one who
    // has not answered come back indistinguishable, because the edge carries
    // nothing but its shape. The answer is only in each fact's prose, so the
    // question worth asking costs a read per guest and a judgement per read.
    let guests = s
        .through("attendance", "event:birthday-party", "person")
        .await;
    guests
        .says("person:patana")
        .says("person:barney-gumble")
        .says("person:ned-flanders");
    //   s.through("attendance", "event:birthday-party").where_key("rsvp", "no").await;
    //   s.through("attendance", "event:birthday-party").missing_key("rsvp").await;

    s.wrap("two replies in, one outstanding").await;

    // ── session 4 · what to cook ────────────────────────────────────────────
    let s = story.session().await;

    // The multi-hop: from the party, to who is attending, to what they eat.
    // The first hop is one call and the second is real too.
    s.through("attendance", "event:birthday-party", "person")
        .await
        .says("person:patana");
    s.recall("person:patana").await.says("vegetarian");
    s.find("vegetarian").await.says("person:patana");

    // GAP — but not in one question. "What do my guests eat" is event →
    // attendees → their dietary facts, which is two calls and a session
    // holding the intermediate result rather than one walk.
    //   s.through("attendance", "event:birthday-party").recall_all().await;

    // The practical half, and the one actually worried about. It is not a
    // verb: "do I have enough chairs" is arithmetic over two things already
    // recorded, and the arithmetic is the session's.
    s.add("thing:folding-chairs", "Folding Chairs").await;
    s.fact(
        "thing:folding-chairs",
        "six of them, stacked in the basement",
    )
    .await;
    s.find("chairs").await.says("thing:folding-chairs");

    // GAP — but neither side of that sum is a number. "Six of them" is prose
    // and so is "ten or twelve people", so the session gets two sentences and
    // has to parse quantities out of English it wrote itself. A claim's value
    // is always prose, so every question with arithmetic in it dies at the
    // read.
    //   s.fact_keyed("thing:folding-chairs", "count", "6").await;

    s.wrap("menu still open").await;

    story.finish().await;
}
