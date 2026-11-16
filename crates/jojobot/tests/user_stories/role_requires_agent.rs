//! "Smithers runs the household and Marge's assistant keeps the weekly dinner
//! plan. After a night when a finished sitting's agent booted again and took
//! the dinner plan, Smithers marks Marge's assistant as one whose role claims
//! name an agent. Can it still claim the dinner plan without saying which
//! agent it is?"
//!
//! A bot that carries `requires_agent` beside `claims_role` has its role claims
//! refused when they name no agent, and the refusal says to send the value of
//! `$PASEO_AGENT_ID`. The bot cannot write that mark on itself: a different
//! identity does.

use serde_json::json;

use super::dsl::Story;

fn marking(bot: &str, value: &str) -> serde_json::Value {
    json!({
        "subject": bot, "content": format!("{bot} claims its role only with an agent"),
        "provenance": "testimony", "sets": {"requires_agent": value},
    })
}

#[tokio::test]
async fn a_bot_marked_as_needing_an_agent_has_its_role_claim_refused_when_it_names_none() {
    let (story, s) = Story::begin_with_session("bot:otto").await;
    s.add("bot:marge", "Marge").await;
    s.add("bot:smithers", "Smithers").await;
    let marge = story.as_bot("bot:marge").await;
    let smithers = story.as_bot("bot:smithers").await;

    // ── the bot cannot mark itself, and a different identity can ────────────
    marge.refused("capture", marking("bot:marge", "true")).await;
    smithers.call("capture", marking("bot:marge", "true")).await;
    s.call("recall", json!({"subject": "bot:marge"}))
        .await
        .says("\"requires_agent\":\"true\"");

    // ── a fresh boot that names no agent is refused, and nothing is held ────
    // **A boot with no sid is the failure this key is for**: `story.call` carries
    // none, where a running session's call would ride its own.
    for agent in [None, Some("$PASEO_AGENT_ID")] {
        let (bare, _) = story
            .call(
                "start_here",
                json!({"bot": "marge", "brief": true, "claim": "dinners", "resume": "new", "agent": agent}),
            )
            .await;
        let bare = bare.json();
        let claim = &bare["session"]["claim"];
        assert_eq!(claim["status"], "refused", "{bare}");
        assert_eq!(claim["fix_by"], "change", "{bare}");
        assert!(
            claim["how_to_proceed"]
                .as_str()
                .is_some_and(|way| way.contains("PASEO_AGENT_ID")),
            "the refusal says which value to send: {bare}"
        );
        s.list("role").await.never_says("role:dinners");
    }

    // ── a running session's claim with no agent is refused as well ──────────
    let again = marge
        .call(
            "start_here",
            json!({"bot": "marge", "brief": true, "claim": "dinners"}),
        )
        .await
        .json();
    assert_eq!(again["session"]["claim"]["status"], "refused", "{again}");

    // ── a fresh boot naming an agent is taken, and the role keeps it ────────
    let (named, _) = story
        .call(
            "start_here",
            json!({
                "bot": "marge", "brief": true, "claim": "dinners",
                "agent": "agent-marge-1", "resume": "new"
            }),
        )
        .await;
    let named = named.json();
    assert_eq!(named["session"]["claim"]["status"], "taken", "{named}");
    s.call("recall", json!({"subject": "role:dinners"}))
        .await
        .says("\"agent\":\"agent-marge-1\"");

    marge.wrap("claimed the dinner plan with its agent").await;
}
