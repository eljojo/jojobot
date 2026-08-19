//! "Yes I have the pass — you should know this."
//!
//! It was written down months earlier and it was written down correctly. The
//! session that needed it asked the user instead, because nothing about
//! pulling up the event mentioned that somebody held anything.
//!
//! **So this story is about RETRIEVAL and not about storage.** Every way of
//! recording it would have stored it fine; what was missing is that reading
//! the thing says who holds what that gets them in. ⛔️ **A story that asks for
//! the pass and receives it would pass on the build that failed** — this one
//! asks for the event and nothing else, exactly as a session that does not
//! know there is anything to ask about would.
//!
//! The two records are both true and a reader acts on them differently: one is
//! good today, one ran out. Dropping the second answers "can I get in" with a
//! silence again, one layer along.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn reading_the_thing_says_who_holds_what_gets_them_in() {
    let story = Story::begin("bot:otto").await;

    // ── months earlier · somebody says what they hold, and it is written ────
    let s = story.session().await;
    s.add("event:winter-fest", "The Winter Fest").await;
    s.add("person:milhouse", "Milhouse").await;
    s.add("person:bart", "Bart").await;

    // His own words, so the record is testimony: he is going to act on this at
    // a door, and a guess reading back as settled is how somebody is turned
    // away.
    s.event_with(
        "person:milhouse",
        "holds a full pass for the whole run",
        json!({
            "admits": "event:winter-fest",
            "tier": "full",
            "valid_until": "2026-12-31",
        }),
        &[],
    )
    .await;

    // True when it was written and no longer true today. **Kept, because "you
    // had one and it ran out" is what a reader needs next.**
    s.event_with(
        "person:bart",
        "held a day pass",
        json!({
            "admits": "event:winter-fest",
            "tier": "day",
            "valid_until": "2026-01-04",
        }),
        &[],
    )
    .await;

    s.add("place:moes", "Moe's").await;
    s.wrap("wrote down what people said they hold").await;

    // ── months later · a session pulls the event, having been told nothing ──
    let later = story.session().await;
    let pulled = later.recall("event:winter-fest").await;

    // **It asked for the event. It was told about the pass.** No walk, no
    // relation named, no key mentioned anywhere in the call.
    pulled
        .says("holds a full pass for the whole run")
        .says("person:milhouse")
        .says("\"liveness\":\"live\"")
        .says("\"provenance\":\"testimony\"");

    // ── the hedge the operator wrote, and it must survive the block ────────
    //
    // ⭐ **A pass somebody THINKS they have is not a pass they have.** The
    // operator said it and said he was unsure, and both halves are recorded —
    // so a session reading this block to tell him he is covered has to be able
    // to see the second one. A dropped hedge turns musing into a fact at the
    // exact moment somebody acts on it.
    let unsure = story.session().await;
    unsure.add("person:nelson", "Nelson").await;
    unsure
        .hedged_with(
            "person:nelson",
            "thinks he still has a pass from last year",
            json!({ "admits": "event:winter-fest", "tier": "full" }),
        )
        .await;
    unsure
        .wrap("wrote down the one he was not sure about")
        .await;

    let with_the_hedge = story.session().await.recall("event:winter-fest").await;
    // **Both keys, on the same object, in one read.** They answer different
    // questions — how sure the operator was, and whether the pass is in force
    // on the day — and asserting either alone passes on a build that still
    // carries one word for both.
    with_the_hedge
        .says("thinks he still has a pass")
        .says("\"standing\":\"open\"")
        .says("\"liveness\":\"live\"");
    // …and the claim nobody hedged says so under the same key, so this cannot
    // pass on a build that marks everything open.
    with_the_hedge.says("\"standing\":\"settled\"");

    // The one that ran out is here and says so, rather than being dropped or
    // being served as though it still worked.
    pulled
        .says("held a day pass")
        .says("\"liveness\":\"lapsed\"");

    // ── and the one that was taken back ────────────────────────────────────
    // **A pass somebody withdrew must not read as one they hold.** The block
    // arrives unasked on every read, so a claim the operator took back would
    // otherwise tell every later session that they are covered — which costs
    // far more than saying nothing.
    let withdrawn = later
        .event_with(
            "person:bart",
            "was promised a pass by the organisers",
            json!({ "admits": "event:winter-fest", "tier": "guest" }),
            &[],
        )
        .await;
    later
        .retract(&withdrawn, "the organisers never issued it")
        .await;
    let after = later.recall("event:winter-fest").await;
    after
        .says("holds a full pass for the whole run")
        .never_says("was promised a pass by the organisers");

    // ── and the other silence, which is a different answer ──────────────────
    let nothing = later.recall("place:moes").await;
    nothing
        .says("nothing points at this")
        .never_says("\"standing\":\"live\"");
}
