//! "I closed the laptop on Friday. On Monday the client picked up where it
//! left off, still holding the handle it had."
//!
//! The handle a session carries is the only thing that says who is asking, and
//! it lives in the server process. A restart takes every live one with it, and
//! nothing tells the client: it just goes on sending the string it holds. So
//! the first move of a session that has been away is a call on a handle that
//! addresses nothing, and what comes back decides whether it recovers or
//! concludes the server is broken.
//!
//! Two stories, because the two halves of the surface answer differently and a
//! session hits whichever it reaches for first: one that writes, one that
//! reads.

use serde_json::json;

use super::dsl::Story;

/// The handle jojobot could have minted and never did — a session that is
/// gone, rather than a string that was never a handle. Spelled here so both
/// stories send the same thing and a reader can see it is well-formed.
const LOST: &str = "zzzz";

#[tokio::test]
async fn a_write_on_a_handle_the_server_no_longer_holds_is_told_where_to_get_another() {
    let story = Story::begin("bot:otto").await;

    // ── friday · a session doing ordinary work ──────────────────────────────
    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact("person:milhouse", "allergic to shellfish").await;

    // The positive the refusal below depends on: this same verb, on the handle
    // this session actually holds, lands. Without it the refusal proves only
    // that journalling is broken.
    s.journal("recorded what was said, and stopping for the day")
        .await;
    s.wrap("friday's work, told and closed").await;

    // ── monday · the client sends the handle it kept ────────────────────────
    //
    // Nothing on the client changed over the weekend. The server did: it
    // restarted, and every handle it was holding went with it. So the first
    // thing it sends is the string it has been carrying since Friday.
    let s = story.session().await;
    let refusal = s
        .refused("journal", json!({"entry": "back at it", "sid": LOST}))
        .await;

    // The refusal has to do three things: say nothing was written, name the
    // handle it turned away so the caller can tell which of its strings is
    // stale, and say where the next one comes from. A refusal that only said
    // "no" would leave a session guessing whether jojobot was down.
    refusal
        .says("\"wrote\":false")
        .says(&format!("\"attempted\":\"{LOST}\""))
        // …and where the next handle comes from. The verb is named, not the
        // sentence around it: the wording is free to improve.
        .says("start_here");

    // ── the other way to arrive with no identity ────────────────────────────
    //
    // A handle that is gone and no handle at all are different situations, and
    // a caller in the second one is not helped by being told about the first.
    // A client that never booted sends nothing rather than something stale.
    let unbound = s
        .refused(
            "capture",
            json!({
                "subject": "person:milhouse",
                "content": "something learned before booting",
                "sid": null,
            }),
        )
        .await;
    unbound.says("\"wrote\":false").never_says(LOST);

    // ── monday, one call later · the way back works ─────────────────────────
    //
    // The advice is only worth anything if following it recovers the session,
    // so the story follows it rather than asserting the sentence and stopping.
    // The refusal cost this caller nothing: the same verb, on the handle the
    // door gave it, lands on the very next call.
    s.journal("booted again after the weekend, carrying on")
        .await;
    s.recall("person:milhouse")
        .await
        .says("allergic to shellfish");

    s.wrap("lost the handle over the weekend, went back to the door, carried on")
        .await;
    story.finish().await;
}

#[tokio::test]
async fn a_read_on_a_lost_handle_is_refused_while_one_with_no_handle_is_served() {
    let story = Story::begin("bot:otto").await;

    let s = story.session().await;
    s.add("person:milhouse", "Milhouse").await;
    s.fact("person:milhouse", "allergic to shellfish").await;

    // ── the two callers a read must serve, and it answers both in full ──────
    //
    // A session holding its own handle, and a caller holding none at all. The
    // second is legitimate: a read is attributed when there is an identity to
    // attribute it to, and refused for want of one it never required.
    s.recall("person:milhouse")
        .await
        .says("allergic to shellfish");
    s.call("recall", json!({"subject": "person:milhouse", "sid": null}))
        .await
        .says("allergic to shellfish");

    // ── and the one it must not ─────────────────────────────────────────────
    //
    // A handle that addresses nothing is not the same as no handle. Serving it
    // would tell a session its identity is still good, and the next thing it
    // writes would land unattributed or not at all.
    let refusal = s
        .refused("recall", json!({"subject": "person:milhouse", "sid": LOST}))
        .await;
    refusal
        .says("\"wrote\":false")
        .says(&format!("\"attempted\":\"{LOST}\""))
        // The way back is the door here too.
        .says("start_here")
        // Refused means refused: the claim it would have served is not in the
        // answer beside the refusal.
        .never_says("allergic to shellfish");

    s.wrap("read on a handle that was gone, and got told so")
        .await;
    story.finish().await;
}
