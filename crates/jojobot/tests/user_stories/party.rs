//! "I want to throw a birthday party." Who comes, what to eat, where.
//!
//! Probes the things a graph should be best at and mostly is not: a set of
//! people whose state changes, a walk from an event to its guests to what they
//! eat, and one person reached only through another.
//!
//! `// GAP:` marks a call that cannot be made today.

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
        "a birthday party for him, ten or twelve people",
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

    // GAP — two venues and two dates, both under evaluation. Same shape as the
    // shortlist in the moving story and it recurs because CHOOSING is what
    // planning is. Nothing can hold candidates, rank them, or record one as
    // ruled out.

    // GAP — the party has no date, because there is no field for one. A fact's
    // date is when a claim became known, and neither the 14th nor the 21st has
    // happened. This is the third domain in three stories where a forward date
    // has nowhere to go.

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
    s.fact("person:ned-flanders", "bringing his partner").await;

    // GAP — "his partner" is a PERSON, reachable only through Ned, and there is
    // no shape that says one person stands in a relation to another. The four
    // shapes point at a place, an org, an event, or vaguely at anything. For a
    // system whose hardest content is people, nothing models people to people —
    // so "who is Ned bringing" and "does anybody here not get along" are the
    // same missing edge.
    // s.fact_about("person:ned-flanders", "his partner", "relation", "person:maude").await;

    s.wrap("invitations out").await;

    // ── session 3 · the replies trickle in ──────────────────────────────────
    let s = story.session().await;

    s.fact("person:patana", "coming to the party").await;
    s.fact("person:barney-gumble", "cannot make the party")
        .await;

    // GAP — THE ONE THIS STORY EXISTS FOR. A guest list is a set whose members
    // each carry a STATE that changes: invited, yes, no, silent. Recorded as
    // facts, "coming" and "cannot make it" sit beside "invited" with nothing
    // superseding anything, and Ned — who has not answered — is indistinguishable
    // from Ned before he was asked.
    // s.attendance("event:birthday-party").set("person:patana", "yes").await;

    // GAP — and therefore the question actually asked, every single time:
    // s.attendance("event:birthday-party").still_silent().says("person:ned-flanders");

    s.wrap("two replies in, one outstanding").await;

    // ── session 4 · what to cook ────────────────────────────────────────────
    let s = story.session().await;

    // The multi-hop this story is for: from the party, to who is attending, to
    // what they eat. The attendance edges exist, so some of this walk is real
    // today — worth proving rather than assuming, since a story that finds
    // something WORKING is as useful as one that finds a hole.
    s.find("party").await.says("event:birthday-party");
    s.find("vegetarian").await.says("person:patana");
    s.recall("person:patana").await.says("vegetarian");

    // GAP — but not in one question. "What do my guests eat" is event →
    // attendees → their dietary facts, and it takes three reads and an agent
    // holding the intermediate result rather than one walk.
    // s.through("event:birthday-party", "attendance").recall_all().says("vegetarian");

    // GAP — the practical half, and it is the half a person actually worries
    // about. Do I have enough chairs. Nothing counts, nothing knows what is
    // owned, and the folding table may be at somebody else's house.
    // s.have_enough("thing:folding-chairs", 12).await;

    s.wrap("menu still open").await;

    story.finish().await;
}
