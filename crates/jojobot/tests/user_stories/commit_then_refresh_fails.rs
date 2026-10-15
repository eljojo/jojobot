//! "I sent it and it said it failed, so I sent it again, and now it is there
//! twice."
//!
//! A write that the store took can still fail to reach what a reader answers
//! from: the memory image cannot read the store again, or the search index
//! cannot take a mail change. A caller told the write failed would repeat it.
//! These stories make each of the two happen through the served verb and hold
//! the answer to what the store holds.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_retraction_the_store_took_is_answered_as_done_when_the_image_cannot_read_it_back() {
    let (first, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let early = first.session().await;

    // ── something recorded while everything works ───────────────────────────
    early.add("person:milhouse", "Milhouse").await;
    let address = early.fact("person:milhouse", "allergic to shellfish").await;
    early
        .wrap("recorded a claim before the server restarted")
        .await;

    // ── a restart fills the image, so the next write has an image to follow it ─
    //
    // A server that has never been restarted holds no image of the store, and a
    // write has nothing to refresh.
    let story = first.restarted().await;
    first.finish().await;
    let s = story.session().await;

    // ── the store stops answering the read that follows a write ─────────────
    store.goes_away();

    let taken_back = s
        .call(
            "retract",
            json!({"address": address, "reason": "written about the wrong person"}),
        )
        .await;
    // The answer is the write's own, with the record it produced, and it says
    // what it changed could not be read back. It does not say the write failed.
    taken_back.holds_at("/retracted/status", "archived");
    taken_back.holds_at("/changed", "not_read");
    taken_back.never_says("\"fix_by\"");

    // ── the store comes back ─────────────────────────────────────────────────
    store.comes_back();

    // The claim stands taken back, once. A caller who repeated the retraction
    // would be told it was already taken back, and this one is not asked to.
    s.recall("person:milhouse")
        .await
        .says("allergic to shellfish")
        .says("archived");
    let again = s
        .refused(
            "retract",
            json!({"address": address, "reason": "written about the wrong person"}),
        )
        .await;
    again.says("archived");

    s.wrap("took back a claim while the store could not be read, and was told it was done")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn a_message_the_board_took_is_answered_as_done_when_the_index_cannot_take_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the search index cannot take the next change ────────────────────────
    story.the_search_index_fails_its_next_commit();

    let id = s
        .post(
            "assistant",
            "the kiln",
            "the kiln is lit and the glaze is ready",
        )
        .await;
    assert!(!id.is_empty(), "the post hands back its id");

    // ── mail search finds it, once ───────────────────────────────────────────
    let found = s.find_including_mail("glaze").await;
    found
        .says("\"hit\":\"message\"")
        .says(&format!("\"id\":\"{id}\""));

    // ── and the one it was sent to takes delivery of it, once ───────────────
    let assistant = story.as_bot("bot:assistant").await;
    assistant.drain().await.says("the kiln").says(&id);
    assistant.drain().await.never_says("the glaze is ready");

    s.wrap("posted while the index was failing, and the post stood")
        .await;
    story.finish().await;
}
