//! "What did I decide about the kiln last week? I know I wrote it down
//! somewhere."
//!
//! A bot's own working history is a thing it should be able to find the way it
//! finds everything else — through the front door, without knowing which store
//! the answer sits in. Search is the bot's Spotlight: what is its own, and what
//! is the operator's and shared.
//!
//! And the half that has no equivalent anywhere else on the surface: **a run is
//! its owner's alone.** Another bot's chronology is not this one's to read, and
//! a bot searching its own history must be able to tell "there is nothing"
//! from "that is not yours" — which it cannot, if both come back empty.

use super::dsl::Story;

#[tokio::test]
async fn a_bot_finds_its_own_past_work_and_not_another_bots() {
    let story = Story::begin("bot:otto").await;

    // ── last week · a run that wrote something down ─────────────────────────
    let s = story.session().await;
    s.add("thing:gravel-bike", "The Gravel Bike").await;
    // The colleague, stood up from inside a session: the door mints no
    // identity, so somebody has to create one before it can be booted.
    s.add("bot:gamma", "Gamma").await;
    s.journal("settled the damper question: it stays hand-cut for now")
        .await;
    s.wrap("the damper decision, recorded").await;

    // ── another bot, working on its own thing the same week ─────────────────
    let other = story.as_bot("bot:gamma").await;
    other
        .journal("looked at the damper too and came to the opposite view")
        .await;
    other.wrap("gamma's own reading of the damper").await;

    // ── today · a new run of the first bot goes looking ─────────────────────
    let s = story.session().await;

    let found = s.find("damper").await;

    // The positive first, and everything below depends on it: the bot's own
    // beat comes back through the ordinary front door, with no second verb and
    // no flag to know about.
    found.says("\"hit\":\"session\"");
    found.says("it stays hand-cut for now");

    // …and the half that has to hold in the same answer: gamma's beat is not
    // in it. Asserted here rather than in a search of its own, because an
    // empty answer would satisfy the absence on its own — this one is absent
    // from a list that just proved it is not empty.
    found.never_says("came to the opposite view");

    // The hit says whose run it is and which run, so a reader can act on it.
    found.says("\"bot\":\"bot:otto\"");

    s.wrap("went looking for last week's decision and found it")
        .await;

    // ── and the mirror, so this is scoping rather than one fixture hiding ───
    //
    // The same question, asked by the other bot, returns the other bot's beat
    // and not this one's. Without it, a build that simply never indexed
    // gamma's run would pass everything above.
    let other = story.as_bot("bot:gamma").await;
    let theirs = other.find("damper").await;
    theirs
        .says("came to the opposite view")
        .never_says("it stays hand-cut for now");

    story.finish().await;
}

#[tokio::test]
async fn a_run_never_outranks_what_the_operator_wrote() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // The operator's own record, and a run that talks about it far more.
    s.add("thing:gravel-bike", "The Gravel Bike").await;
    s.fact("thing:gravel-bike", "the chainring is worn").await;
    s.journal(
        "chainring chainring chainring chainring chainring chainring chainring chainring \
         chainring chainring — went round in circles on the chainring",
    )
    .await;

    let found = s.find("chainring").await;

    // Reachable: the run is in the answer at all.
    found.says("\"hit\":\"session\"");

    // …and below. A session is context rather than an answer, so whatever the
    // operator wrote comes first even when the run mentions the word far more
    // often. A rank so low it never surfaced would satisfy "lower" and defeat
    // the point of indexing runs at all, which is why the line above is here.
    let body = found.raw();
    let first_session = body.find("\"hit\":\"session\"").expect("the run is in it");
    let first_fact = body.find("\"hit\":\"fact\"").expect("the claim is in it");
    assert!(
        first_fact < first_session,
        "a run ranks below what the operator wrote: {body}"
    );

    s.wrap("looked up the chainring").await;
    story.finish().await;
}
