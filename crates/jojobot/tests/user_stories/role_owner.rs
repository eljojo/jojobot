//! "Alpha runs gamma. Why can a session booted as beta take it?"
//!
//! A role belongs to the bot that carries its name in `claims_role`, or, when
//! no bot does, to the bot whose child the role object is. A claim at the door
//! from any other bot is refused before anything is written: the refusal names
//! the owner and offers to boot as it. A name no bot owns stays free, and the
//! first claim makes its object under the claimant.

use serde_json::json;

use super::dsl::Story;

/// What the boot answered about the claim, whole.
fn claim_of(booted: &serde_json::Value) -> &serde_json::Value {
    &booted["session"]["claim"]
}

#[tokio::test]
async fn a_claim_at_the_door_is_refused_when_the_role_belongs_to_another_bot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:alpha", "Alpha").await;
    s.add("bot:beta", "Beta").await;

    // ── the owner by name: alpha carries gamma ─────────────────────────────
    s.event_with(
        "bot:alpha",
        "alpha runs gamma",
        json!({"claims_role": "gamma"}),
        &[],
    )
    .await;

    // Beta asks for gamma and is refused. The answer names alpha and offers to
    // boot as it.
    let (beta, _) = story
        .call(
            "start_here",
            json!({"bot": "beta", "brief": true, "claim": "gamma"}),
        )
        .await;
    let beta = beta.json();
    assert_eq!(claim_of(&beta)["status"], "refused", "{beta}");
    assert_eq!(claim_of(&beta)["owner"], "bot:alpha", "{beta}");
    assert!(
        claim_of(&beta)["how_to_proceed"]
            .as_str()
            .is_some_and(|way| way.contains("alpha") && way.contains("start_here")),
        "the refusal offers to boot as the owner: {beta}"
    );
    // Nothing was written anywhere: no role object, no role key on beta.
    s.list("role").await.never_says("role:gamma");
    s.call("recall", json!({"subject": "bot:beta", "facts": true}))
        .await
        .never_says("role/gamma");

    // In the same read, alpha claims gamma and is taken.
    let (alpha, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "gamma"}),
        )
        .await;
    let alpha = alpha.json();
    assert_eq!(claim_of(&alpha)["status"], "taken", "{alpha}");
    s.list("role").await.says("role:gamma");

    // ── the owner by object: omega has no carrier, only a parent ────────────
    let (first, _) = story
        .call(
            "start_here",
            json!({"bot": "alpha", "brief": true, "claim": "omega", "resume": "new"}),
        )
        .await;
    assert_eq!(
        claim_of(&first.json())["status"],
        "taken",
        "{}",
        first.json()
    );

    let (stranger, _) = story
        .call(
            "start_here",
            json!({"bot": "beta", "brief": true, "claim": "omega"}),
        )
        .await;
    let stranger = stranger.json();
    assert_eq!(claim_of(&stranger)["status"], "refused", "{stranger}");
    assert_eq!(claim_of(&stranger)["owner"], "bot:alpha", "{stranger}");
    s.call("recall", json!({"subject": "bot:beta", "facts": true}))
        .await
        .never_says("role/omega");

    // ── a name nobody owns is free, and the first claim keeps it ────────────
    let (free, _) = story
        .call(
            "start_here",
            json!({"bot": "beta", "brief": true, "claim": "sigma", "resume": "new"}),
        )
        .await;
    assert_eq!(claim_of(&free.json())["status"], "taken", "{}", free.json());
    s.list("role").await.says("role:sigma");

    s.wrap("kept gamma with alpha").await;
    story.finish().await;
}
