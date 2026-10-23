//! "A booted session reads a skill beside its own bot. What happens to the
//! boot arguments it sent with it?"
//!
//! **A call never has an argument silently ignored.** The skill is served when
//! the call carries the key of a live run of the named bot and nothing else a
//! boot would act on. A role claim beside it refuses the whole call and names
//! `claim`, and the role stays free for a call that boots.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_claim_beside_a_skill_refuses_the_call_and_claims_nothing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;

    let (_, bart) = story.call("start_here", json!({"bot": "bart"})).await;
    let bart = bart.expect("a boot hands back a handle");

    // ── the skill alone beside the bot is served ────────────────────────────
    let served = bart
        .call("start_here", json!({"bot": "bart", "skill": "evidence"}))
        .await
        .json();
    assert!(served["skill"]["body"].is_string(), "{served}");

    // ── a claim beside it refuses the call, names the claim, serves no body ─
    let refused = bart
        .refused(
            "start_here",
            json!({"bot": "bart", "skill": "evidence", "claim": "reader"}),
        )
        .await
        .json();
    assert_eq!(refused["status"], "blocked", "{refused}");
    assert!(refused.get("skill").is_none(), "{refused}");
    assert!(
        refused["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("`claim`")),
        "{refused}"
    );

    // ── a section beside the skill is a second fetch: refused by name too ───
    let both = bart
        .refused(
            "start_here",
            json!({"bot": "bart", "skill": "evidence", "section": "opening"}),
        )
        .await
        .json();
    assert_eq!(both["status"], "blocked", "{both}");
    assert!(both.get("skill").is_none(), "{both}");
    assert!(
        both["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("`section`")),
        "{both}"
    );

    // ── and the claim was not made: a boot that asks for it takes it ────────
    let claimed = bart
        .call(
            "start_here",
            json!({"bot": "bart", "claim": "reader", "brief": true}),
        )
        .await
        .json();
    assert_eq!(claimed["session"]["claim"]["status"], "taken", "{claimed}");
    story.finish().await;
}
