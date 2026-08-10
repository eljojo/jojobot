//! "The notes service was down when I started this morning. It came back, and
//! then it went away again."
//!
//! Every other story runs against a store that is always there. This one is
//! about the half of the surface that only exists when it is not: `search`
//! reads an in-process index, so a store it could not read leaves material
//! missing, and an answer that came back without saying so would read as
//! "nothing says that".
//!
//! The session never learns the store is a store. What it gets is its own
//! position — whether this answer covered everything, and which way it is
//! behind when it did not — and the verb that reads past the index.

use super::dsl::Story;

#[tokio::test]
async fn a_session_searching_a_degraded_jojobot_is_told_what_its_answer_is_missing() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let s = story.session().await;

    // ── this morning · something recorded before anything went wrong ────────
    s.add("person:milhouse", "Milhouse").await;
    s.fact("person:milhouse", "allergic to shellfish").await;

    // ── the store goes away ─────────────────────────────────────────────────
    //
    // Nothing tells the session. It asks the same question it would have asked
    // anyway.
    store.goes_away();

    let behind = s.find("shellfish").await;
    // Searched — the hits are real, and an answer carrying hits must never
    // claim it searched nothing.
    behind.says("\"searched\":true");
    // …and WHICH WAY it is behind, which is the whole of what a caller does
    // with this. `unscanned` says trust an empty answer about nothing older
    // than this server; `stale` would say trust it about everything but the
    // newest write. Told one when it is in the other, a session reads an empty
    // result as "nothing says that", which is the conclusion this block exists
    // to prevent.
    behind.says("\"behind\":\"unscanned\"");
    // The way past the index is named, and it works: `recall` reads the store,
    // which is still answering everything except the read that fills the
    // index.
    s.recall("person:milhouse")
        .await
        .says("allergic to shellfish");

    // ── it comes back ───────────────────────────────────────────────────────
    //
    // The next search refreshes, so the index catches up without anybody
    // restarting anything, and the answer stops hedging.
    store.comes_back();

    let whole = s.find("shellfish").await;
    whole.says("\"searched\":true").never_says("\"behind\"");
    whole.says("allergic to shellfish");

    // ── and it goes away again ──────────────────────────────────────────────
    //
    // Now the index has read the store once and holds nearly all of it, so the
    // claim is a different one: what is here is real and one thing may be a
    // version the store has moved past. The token changes with it.
    store.goes_away();

    let stale = s.find("shellfish").await;
    stale.says("\"searched\":true");
    stale.says("\"behind\":\"stale\"");
    stale.says("allergic to shellfish");

    s.wrap("searched through a store that came and went, and was told so each time")
        .await;
    story.finish().await;
}
