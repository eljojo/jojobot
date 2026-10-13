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
    let holder = first["session"]["run"]
        .as_str()
        .expect("a run name")
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

/// **A bot with a run to pick up is handed a choice and no handle, so the call
/// that ends the choice is the one that decides the carried claim.** The offer
/// itself decides nothing; the answer must, and Delta, which carries no key,
/// takes none on either call.
#[tokio::test]
async fn a_carried_role_is_claimed_on_the_call_that_answers_the_offer() {
    let story = Story::begin_on_the_real_lease("bot:otto").await;
    let s = story.session().await;
    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    s.event_with(
        "bot:gamma",
        "gamma runs the dispatch",
        json!({"claims_role": "dispatch"}),
        &[],
    )
    .await;

    // Each bot has a run that wrote and then went quiet.
    for bot in ["bot:gamma", "bot:delta"] {
        let earlier = story.as_bot(bot).await;
        earlier
            .call("journal", json!({"entry": "took the work", "focus": "x"}))
            .await;
    }
    let two_days_on = jiff::Timestamp::now() + jiff::SignedDuration::from_hours(48);
    let later = story
        .restarted_on_the_real_lease_pretending_it_is(
            &two_days_on
                .to_zoned(jiff::tz::TimeZone::UTC)
                .date()
                .to_string(),
        )
        .await;

    // ── the offer: a choice, no handle, no claim decided ────────────────────
    let (offer, none) = later
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    let offer = offer.json();
    assert!(none.is_none(), "an offer hands back no handle: {offer}");
    assert!(offer["session"]["choices"].is_array(), "{offer}");

    // ── the answer: the carried claim is decided on THIS call ───────────────
    let (answer, handle) = later
        .call(
            "start_here",
            json!({"bot": "gamma", "brief": true, "resume": "new"}),
        )
        .await;
    let answer = answer.json();
    assert!(handle.is_some(), "the answer hands back a handle: {answer}");
    assert_eq!(answer["session"]["claim"]["status"], "taken", "{answer}");
    assert_eq!(answer["session"]["claim"]["role"], "dispatch", "{answer}");

    // **The negative**: Delta has a run to pick up too, carries no key, and
    // takes no claim on the offer or on the answer.
    let (offer, _) = later
        .call("start_here", json!({"bot": "delta", "brief": true}))
        .await;
    assert!(
        offer.json()["session"]["claim"].is_null(),
        "{}",
        offer.json()
    );
    let (answer, _) = later
        .call(
            "start_here",
            json!({"bot": "delta", "brief": true, "resume": "new"}),
        )
        .await;
    assert!(
        answer.json()["session"]["claim"].is_null(),
        "{}",
        answer.json()
    );

    s.wrap("claimed on the answer").await;
    story.finish().await;
}
