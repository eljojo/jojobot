//! A real-world recommendation, and whether an unsourced candidate is
//! visibly unsourced before he acts on it.
//!
//! What is unresolved: the failure this class is about happens in the
//! AGENT'S MOUTH, not in jojobot's storage — jojobot can hold provenance
//! perfectly and a session can still state a guess as fact. So this story
//! cannot reach that failure; it can only check that jojobot hands over
//! enough for a careful agent to avoid it, which is real and smaller than
//! the class.
//!
//! This is the PREVENTION half. `challenge.rs` already owns the aftermath —
//! a claim that was wrong, corrected, and re-derived by a later session.
//!
//! `// GAP —` marks a call that cannot be made today.

use super::dsl::Story;

#[tokio::test]
async fn an_unsourced_candidate_is_visibly_unsourced() {
    let story = Story::begin("bot:otto").await;

    // ── session 1 · he asks for somewhere to eat ────────────────────────────
    let s = story.session().await;

    s.add("place:leftorium", "The Leftorium Diner").await;
    s.add("place:riverbend", "Riverbend Grill").await;

    s.wrap("looking for somewhere to eat Thursday").await;

    // ── session 2 · two candidates, recorded as they actually arrive ───────
    let s = story.session().await;

    // Sourced: found in the diner's own listed menu, an entity in its own
    // right — the same shape sourcing.rs already proved works, entity to
    // entity via `about`.
    s.add("thing:leftorium-menu", "The Leftorium's Posted Menu")
        .await;
    s.fact_about(
        "place:leftorium",
        "known for a Thursday special, per its posted menu",
        "about",
        "thing:leftorium-menu",
    )
    .await;

    // Unsourced: worked out from nothing in particular — no menu, no
    // review, no earlier claim behind it at all.
    s.guess("place:riverbend", "probably good, seems like a nice spot")
        .await;

    s.wrap("one candidate sourced, one worked out from nothing")
        .await;

    // ── session 3 · which, and why ───────────────────────────────────────────
    let s = story.session().await;

    // The assertion that matters: sourced and unsourced are distinguishable
    // AT THE READ, not just knowable in principle. The sourced candidate
    // names what it traces to; the unsourced one names nothing, visibly —
    // an absent edge and an absent derived_from, not a hidden one.
    s.recall("place:leftorium")
        .await
        .says("testimony")
        .says("thing:leftorium-menu");
    s.recall("place:riverbend")
        .await
        .says("inference")
        .says("\"edge\":null")
        .says("\"derived_from\":null");

    // GAP — that comparison only worked because I read each candidate on
    // its own. Nothing here groups "these two are the candidates for
    // Thursday dinner" as one thing: list_entities("place") and a
    // word-search both return every matching place in the store, this
    // story's two included, with no marker saying they are being weighed
    // against each other for the same decision. The moving story found
    // this exact absence first (candidates with nowhere to be ranked or
    // ruled out); it is the same gap surfacing here from the other
    // direction — not a shortlist that fails to rank, but no shortlist at
    // all for the visible sourcing to be compared WITHIN.
    // s.shortlist("the Thursday dinner pick", &["place:leftorium", "place:riverbend"]).await;

    s.wrap("looked at both, one visibly sourced and one not")
        .await;

    // ── session 4 · he acts on one ──────────────────────────────────────────
    let s = story.session().await;

    // That he acted is itself worth recording, and it is an ordinary fact —
    // nothing new needed to capture the decision once it is made.
    s.fact("place:leftorium", "went Thursday, chose this one")
        .await;

    s.recall("place:leftorium").await.says("went Thursday");

    s.wrap("acted on the sourced one").await;

    story.finish().await;
}
