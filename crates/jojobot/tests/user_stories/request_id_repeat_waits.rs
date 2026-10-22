//! "I sent it, heard nothing, and sent it again. The second answer said it was
//! there, and I could not find it."
//!
//! A write carries a `request_id` so that a resend is safe. The store spends the
//! id in the commit that stores the write, and the memory image takes the write
//! in a moment later. A resend that reaches the server between the two used to
//! be answered from the ledger: it said the write had landed, at an address a
//! read could not yet find. The resend now waits for the first call to leave the
//! verb, and its answer is the first call's receipt.

use std::time::Duration;

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_resend_made_while_the_first_call_is_still_inside_the_verb_waits_and_then_finds_the_write()
 {
    let (story, store) = Story::begin_over_a_store_that_can_go_away("bot:otto").await;
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;

    // ── the first call has reached the store and is not yet in the image ────
    let gate = store.holds_the_next_document_read();
    let write = json!({
        "subject": "person:milhouse",
        "content": "allergic to shellfish",
        "provenance": "testimony",
        "request_id": "note-1",
    });
    let first = s.call("capture", write.clone());
    let resend = async {
        gate.reached().await;
        let second = s.call("capture", write.clone());
        tokio::pin!(second);
        let early = tokio::time::timeout(Duration::from_millis(500), &mut second).await;
        assert!(
            early.is_err(),
            "the resend was answered while the first call was still inside the verb"
        );
        gate.release();
        second.await
    };
    let (first, repeat) = tokio::join!(first, resend);

    // ── the resend answers with the first call's receipt ────────────────────
    let body = first.json();
    let address = body["address"]
        .as_str()
        .or_else(|| body["fact"]["address"].as_str())
        .unwrap_or_else(|| panic!("a capture hands back its address: {body}"))
        .to_string();
    repeat.says("\"repeat\":true");
    repeat.says(&address);

    // ── and a read right after finds the claim at that address ──────────────
    let read = s.recall("person:milhouse").await;
    read.says("allergic to shellfish");
    read.says(&address);

    s.wrap("sent a note twice while the first was still being written")
        .await;
    story.finish().await;
}
