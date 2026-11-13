//! "I sent it and it said it failed, so I sent it again, and now it is there
//! twice."
//!
//! A write that the store took can still fail to reach what a reader answers
//! from: the memory image cannot read the store again, or the search index
//! cannot take a mail change. A caller told the write failed would repeat it.
//! These stories make each of the two happen through the served verb and hold
//! the answer to what the store holds.

use super::dsl::Story;

#[tokio::test]
async fn a_message_the_board_took_is_answered_as_done_when_the_index_cannot_take_it() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    // ── the assistant has dealt with the release notes the start left it ────
    // They come first in the box and fill the answer ceiling, so the message
    // this story is about would otherwise be named in `not_shown` and not carried.
    let assistant = story.as_bot("bot:assistant").await;
    let notes = assistant.drain().await.json();
    let left_out = notes["not_shown"]["ids"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for note in notes["messages"]
        .as_array()
        .into_iter()
        .flatten()
        .chain(&left_out)
    {
        let id = note["id"].as_str().or(note.as_str()).expect("a note id");
        assistant.processed(id, "read").await;
    }

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
    assistant.drain().await.says("the kiln").says(&id);
    assistant.drain().await.never_says("the glaze is ready");

    s.wrap("posted while the index was failing, and the post stood")
        .await;
    story.finish().await;
}
