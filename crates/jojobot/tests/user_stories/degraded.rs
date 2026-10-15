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

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_session_searching_a_degraded_jojobot_is_told_what_its_answer_is_missing() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let earlier = story.session().await;

    // ── yesterday · something recorded while everything worked ──────────────
    earlier.add("person:milhouse", "Milhouse").await;
    earlier
        .fact("person:milhouse", "allergic to shellfish")
        .await;

    // ── this morning · the server starts while the store cannot be read ─────
    //
    // The boot's read of the whole store fails and is not fatal. The run kept
    // its key across the restart, so it goes on without booting again, and
    // adds what it learns today.
    store.the_corpus_read_fails();
    let restarted = story.restarted().await;
    let s = restarted.session_carried_from(&earlier).await;
    s.add("person:homer", "Homer").await;

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
    behind.never_says("allergic to shellfish");
    // The way past the index is named, and it works: `recall` reads the store,
    // which is still answering everything except the read that fills the
    // index.
    s.recall("person:milhouse")
        .await
        .says("allergic to shellfish");
    s.find("Homer").await.says("person:homer");

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
    store.the_corpus_read_fails();

    let stale = s.find("shellfish").await;
    stale.says("\"searched\":true");
    stale.says("\"behind\":\"stale\"");
    stale.says("allergic to shellfish");

    s.wrap("searched through a store that came and went, and was told so each time")
        .await;
    restarted.finish().await;
    story.finish().await;
}

/// **A server that read its store when it started answers from what it read when
/// the store goes away**, and does not call itself unscanned: the first read
/// that fills the index happened at the start, exactly as the binary's boot does
/// it. The pair of the story above, which is about an index that never got that
/// read.
#[tokio::test]
async fn a_server_that_scanned_its_store_at_start_is_not_unscanned_when_the_store_goes_away() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact("person:milhouse", "allergic to shellfish").await;

    store.the_corpus_read_fails();

    let found = s.find("shellfish").await;
    found.says("\"searched\":true");
    found.says("allergic to shellfish");
    found.never_says("\"behind\":\"unscanned\"");

    s.wrap("searched after the store went away, from what was read at the start")
        .await;
    story.finish().await;
}

/// **A write the store committed is findable by search, or search says it is
/// behind.** The server starts while the store cannot answer the whole-corpus
/// read, so its fold holds nothing and every write that lands answers that the
/// fold could not follow it. Each kind of write that can answer so is made here
/// and searched for straight after: a capture, a correction, a retraction and a
/// merge. The index follows each one, not only the fold.
#[tokio::test]
async fn a_write_that_lands_with_the_fold_behind_is_found_by_search() {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let earlier = story.session().await;
    earlier.add("person:milhouse", "Milhouse").await;
    store.the_corpus_read_fails();
    let restarted = story.restarted().await;
    let s = restarted.session_carried_from(&earlier).await;
    s.add("person:apu", "Apu").await;

    // ── a capture ───────────────────────────────────────────────────────────
    let thimbles = s.fact("person:apu", "collects thimbles").await;
    s.find("thimbles").await.says("collects thimbles");

    // ── a correction ────────────────────────────────────────────────────────
    s.correct(&thimbles, "collects spoons").await;
    s.find("spoons").await.says("collects spoons");
    s.find("thimbles").await.never_says("collects thimbles");

    // ── a retraction ────────────────────────────────────────────────────────
    s.retract(&thimbles, "written in error").await;
    s.find("spoons").await.never_says("collects spoons");

    // ── a merge ─────────────────────────────────────────────────────────────
    s.add("person:lisa", "Lisa").await;
    s.fact("person:lisa", "plays the saxophone").await;
    s.find("saxophone").await.says("plays the saxophone");
    s.merge_entities("person:lisa", "person:apu", "one person filed twice", None)
        .await;
    let merged = s.find("saxophone").await;
    merged.says("plays the saxophone");
    merged.says("\"id\":\"person:apu\"");
    merged.never_says("\"id\":\"person:lisa\"");

    // ── a creation that carries its first claim, and an edit that carries one ─
    s.call(
        "add_entity",
        json!({
            "kind": "person", "handle": "bart", "name": "Bart", "source": "user-named",
            "sets": {"hobby": "skateboards"},
        }),
    )
    .await;
    s.find("Bart").await.says("\"hit\":\"entity\"");
    s.call(
        "update_entity",
        json!({
            "handle": "person:apu", "name": "Apu Upholsterer",
            "sets": {"trade": "upholstery"},
        }),
    )
    .await;
    s.find("Upholsterer").await.says("\"hit\":\"entity\"");

    s.wrap("wrote while the fold was behind and found every write by search")
        .await;
    restarted.finish().await;
    story.finish().await;
}

/// **The pair of the story above**: on a server whose fold followed the write,
/// a capture is found by search straight after, so the refreshes that story
/// asserts are about the fold being behind and nothing else.
#[tokio::test]
async fn a_capture_the_fold_followed_is_found_by_search() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("person:apu", "Apu").await;
    s.fact("person:apu", "collects thimbles").await;

    s.find("thimbles").await.says("collects thimbles");

    s.wrap("wrote with the fold following and found it by search")
        .await;
    story.finish().await;
}
