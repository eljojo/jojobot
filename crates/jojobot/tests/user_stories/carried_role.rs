//! "Gamma runs the dispatch role. Why does every boot have to say so?"
//!
//! A bot can carry the name of the role it claims. Booting as it, with no
//! `claim` named, claims that role exactly as naming it would: a second
//! claimant is refused while the first is fresh. A bot that carries no such
//! key boots as it always did, and the bot cannot give itself a role.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_bot_that_carries_a_role_claims_it_at_every_boot() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;

    // ── another identity says which role Gamma claims ───────────────────────
    let carried = s
        .event_with(
            "bot:gamma",
            "gamma runs the dispatch",
            json!({"claims_role": "dispatch"}),
            &[],
        )
        .await;

    // Booting as Gamma with no `claim` holds the role.
    let (first, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    let first = first.json();
    assert_eq!(first["session"]["claim"]["status"], "taken", "{first}");
    assert_eq!(first["session"]["claim"]["role"], "dispatch", "{first}");
    // The claim says it came from the bot and not from the caller.
    assert!(
        first["session"]["claim"]["claimed_because"].is_string(),
        "{first}"
    );
    let holder = first["session"]["sid"]
        .as_str()
        .expect("a handle")
        .to_string();

    // A second boot as Gamma is refused, and told who holds it.
    let (second, _) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "new"}),
        )
        .await;
    let second = second.json();
    assert_eq!(second["session"]["claim"]["status"], "refused", "{second}");
    assert_eq!(second["session"]["claim"]["holder"], holder, "{second}");

    // **The negative the positive rests on**: Delta carries no such key, and
    // its boot names no claim at all.
    let (delta, _) = story
        .call("start_here", json!({"bot": "delta", "brief": true}))
        .await;
    assert!(
        delta.json()["session"]["claim"].is_null(),
        "a bot with no key boots unchanged: {}",
        delta.json()
    );

    // ── Gamma cannot give itself a role ─────────────────────────────────────
    let gamma = story.as_bot("bot:gamma").await;
    gamma
        .refused(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "i would like another role",
                "fields": {"claims_role": "release"},
            }),
        )
        .await
        .says("claims_role")
        .says("\"wrote\":false");
    gamma
        .refused(
            "update_fact",
            json!({"address": carried, "fields": {"claims_role": "release"}}),
        )
        .await
        .says("claims_role")
        .says("\"wrote\":false");

    // ── a role named at the door wins, and a blank key claims nothing ───────
    let (explicit, _) = story
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "claim": "release", "resume": "new"}),
        )
        .await;
    let explicit = explicit.json();
    assert_eq!(
        explicit["session"]["claim"]["role"], "release",
        "{explicit}"
    );
    assert_eq!(
        explicit["session"]["claim"]["status"], "taken",
        "{explicit}"
    );
    s.add("bot:epsilon", "Epsilon").await;
    s.event_with(
        "bot:epsilon",
        "epsilon carries a blank role",
        json!({"claims_role": "   "}),
        &[],
    )
    .await;
    let (blank, _) = story
        .call("start_here", json!({"bot": "epsilon", "brief": true}))
        .await;
    assert!(
        blank.json()["session"]["claim"].is_null(),
        "a blank key names no role: {}",
        blank.json()
    );

    s.wrap("gave gamma its role").await;
    story.finish().await;
}
