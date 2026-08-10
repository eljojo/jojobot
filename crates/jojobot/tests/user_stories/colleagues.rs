//! "We don't send a message to the pm mailbox. We send it to the pm."
//!
//! A bot has exactly one box, so its name carries no addressing power its
//! handle does not — and a caller had to learn that a colleague keeps their
//! mail somewhere named after them, a correspondence nothing enforced.
//!
//! Reading was already addressed by identity: `read_mailbox` takes no box name,
//! because the session handle says whose box it is. Only writing still asked
//! for a container. This is that asymmetry closed.
//!
//! The guard survives the move and gets better at its job: a mistyped
//! colleague comes back with the colleague you meant, from a directory you
//! already have.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn you_write_to_a_colleague_and_never_to_their_container() {
    let story = Story::begin("bot:otto").await;

    let s = story.session().await;
    s.add("bot:epsilon", "Epsilon").await;

    // ── you address the identity, and never say where its mail lives ────────
    let sent = s
        .call(
            "post_message",
            json!({
                "to": "epsilon",
                "subject": "the trail survey",
                "body": "the survey is done, numbers in the wiki",
            }),
        )
        .await;
    sent.says("\"state\":\"new\"");

    // The full handle addresses the same colleague. One correspondent kind, so
    // a caller never has to spell it.
    s.call(
        "post_message",
        json!({
            "to": "bot:epsilon",
            "body": "and the photographs, when you get to them",
        }),
    )
    .await
    .says("\"state\":\"new\"");

    // ── it really reached them, which is the only proof that matters ────────
    //
    // epsilon opens its own box — with no box name, because reading was always
    // addressed by identity — and both messages are there.
    let epsilon = story.as_bot("bot:epsilon").await;
    let theirs = epsilon.drain().await;
    theirs.says("the survey is done");
    theirs.says("and the photographs");

    // ── a mistyped colleague is a near miss, never a confident nothing ──────
    //
    // The worst answer here is a cheerful success into a box nobody reads. The
    // refusal names who was meant, from the directory the caller already has.
    let typo = s
        .refused(
            "post_message",
            json!({ "to": "epsilo", "body": "this must not vanish" }),
        )
        .await;
    typo.says("bot:epsilon");
    typo.says("\"wrote\":false");

    // …and nothing was written, which is the half a refusal message alone does
    // not prove. epsilon's box holds only the two that were addressed properly,
    // and both are leftovers now rather than fresh mail.
    let after = epsilon.drain().await;
    after.never_says("this must not vanish");

    // ── and the sender asks after a colleague, not after a container ────────
    let where_it_went = s.call("list_sent", json!({ "to": "epsilon" })).await;
    where_it_went.says("the survey is done");

    s.wrap("wrote to a colleague without knowing where they keep their mail")
        .await;
    story.finish().await;
}
