//! "Gamma's floor is eight rules. Why does its boot carry five?"
//!
//! A boot carries a bot's marked rules up to a number of seats. The number is
//! a property of the bot: somebody else writes `rule_seats` on it, and a bot
//! that carries none boots with the default. The bot cannot write it about
//! itself — a ceiling holds only if raising it costs something other than
//! asking.

use serde_json::json;

use super::dsl::Story;

#[tokio::test]
async fn a_bot_whose_floor_is_eight_rules_boots_with_eight_once_somebody_else_says_so() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("bot:gamma", "Gamma").await;
    s.add("bot:delta", "Delta").await;
    // Eight rules each, all marked to ride the boot.
    for bot in ["bot:gamma", "bot:delta"] {
        for n in 0..8 {
            s.call(
                "capture",
                json!({
                    "subject": bot,
                    "content": format!("rule number {n}"),
                    "fields": {"starred": "true"},
                }),
            )
            .await;
        }
    }

    // ── no key: the default holds, for both ─────────────────────────────────
    let (gamma, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    assert_eq!(
        gamma.json()["identity"]["rules"].as_array().unwrap().len(),
        5
    );
    let (delta, _) = story
        .call("start_here", json!({"bot": "delta", "brief": true}))
        .await;
    assert_eq!(
        delta.json()["identity"]["rules"].as_array().unwrap().len(),
        5
    );

    // ── another identity gives Gamma eight seats ────────────────────────────
    let seats = s
        .event_with(
            "bot:gamma",
            "gamma's floor is eight rules",
            json!({"rule_seats": "8"}),
            &[],
        )
        .await;
    let (gamma, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    assert_eq!(
        gamma.json()["identity"]["rules"].as_array().unwrap().len(),
        8
    );
    // **The negative the positive rests on**: Delta holds the same eight
    // marked rules and carries no key, so it still boots with five.
    let (delta, _) = story
        .call("start_here", json!({"bot": "delta", "brief": true}))
        .await;
    assert_eq!(
        delta.json()["identity"]["rules"].as_array().unwrap().len(),
        5
    );

    // ── Gamma cannot give itself more ───────────────────────────────────────
    let gamma_run = story.as_bot("bot:gamma").await;
    gamma_run
        .refused(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "i would like twenty",
                "fields": {"rule_seats": "20"},
            }),
        )
        .await
        .says("rule_seats")
        .says("\"wrote\":false");
    // The edit path is held to the same line.
    gamma_run
        .refused(
            "update_fact",
            json!({"address": seats, "fields": {"rule_seats": "20"}}),
        )
        .await
        .says("rule_seats")
        .says("\"wrote\":false");
    // Nothing moved: the boot still carries eight.
    let (gamma, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    assert_eq!(
        gamma.json()["identity"]["rules"].as_array().unwrap().len(),
        8
    );

    s.wrap("gave gamma its seats").await;
    story.finish().await;
}

/// **A boot never declines, so what it cannot cut is held to the ceiling where
/// it is written.** Words that cannot be ranked down — a rule's own content —
/// are what a star or a seat count adds, and a write that takes the boot over
/// the ceiling is refused, naming the key that lowers it.
#[tokio::test]
async fn a_floor_the_boot_cannot_carry_is_refused_when_it_is_written() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:epsilon", "Epsilon").await;

    // Five seats' worth of rules, each two thousand characters: the floor
    // fits, so every one of these lands.
    let rule = "w ".repeat(1000);
    for n in 0..8 {
        s.call(
            "capture",
            json!({
                "subject": "bot:epsilon",
                "content": format!("{n} {rule}"),
                "fields": {"starred": "true"},
            }),
        )
        .await;
    }

    // ── a star that grows the floor past the ceiling ────────────────────────
    s.refused(
        "capture",
        json!({
            "subject": "bot:epsilon",
            "content": "heavyrule ".repeat(4000),
            "fields": {"starred": "true"},
        }),
    )
    .await
    .says("rule_seats")
    .says("\"wrote\":false");
    // Nothing was written.
    s.call("recall", json!({"subject": "bot:epsilon", "facts": true}))
        .await
        .never_says("heavyrule");

    // ── the same refusal, met by the bot starring its OWN rule ──────────────
    //
    // `rule_seats` is a key a bot cannot write about itself, so the way out
    // the refusal offers this caller is a different identity setting it. The
    // two ways that are the caller's own stay offered beside it.
    let epsilon_run = story.as_bot("bot:epsilon").await;
    epsilon_run
        .refused(
            "capture",
            json!({
                "subject": "bot:epsilon",
                "content": "ownheavy ".repeat(4000),
                "fields": {"starred": "true"},
            }),
        )
        .await
        .says("different identity")
        .says("update_fact")
        .says("shorten")
        .says("\"wrote\":false");

    // ── a seat count that does the same ─────────────────────────────────────
    //
    // Eight starred rules at the default five seats fit. Eight seats carry
    // three more, and the floor goes over.
    let eight = s
        .refused(
            "capture",
            json!({
                "subject": "bot:epsilon",
                "content": "epsilon needs eight seats",
                "fields": {"rule_seats": "8"},
            }),
        )
        .await;
    eight.says("characters").says("\"wrote\":false");
    // **The paired positive**: a count that lowers the floor lands, so the
    // refusal above is about the size and not about the key.
    let three = s
        .event_with(
            "bot:epsilon",
            "epsilon carries three",
            json!({"rule_seats": "3"}),
            &[],
        )
        .await;
    let (booted, _) = story
        .call("start_here", json!({"bot": "epsilon", "brief": true}))
        .await;
    assert_eq!(
        booted.json()["identity"]["rules"].as_array().unwrap().len(),
        3
    );
    // A value that is no count is refused whoever sends it.
    s.refused(
        "update_fact",
        json!({"address": three, "fields": {"rule_seats": "many"}}),
    )
    .await
    .says("rule_seats");

    // ── a bot already over stays repairable ─────────────────────────────────
    //
    // Sigma's charter alone is over the ceiling, so its boot ships oversized
    // whatever is written. A write that lowers the floor still lands, and one
    // that grows it does not.
    s.add("bot:sigma", "Sigma").await;
    for n in 0..7 {
        s.call(
            "capture",
            json!({
                "subject": "bot:sigma",
                "content": format!("short rule {n}"),
                "fields": {"starred": "true"},
            }),
        )
        .await;
    }
    s.call(
        "set_charter",
        json!({"bot": "sigma", "prose": "heavycharter ".repeat(3000)}),
    )
    .await;
    s.event_with(
        "bot:sigma",
        "sigma carries two",
        json!({"rule_seats": "2"}),
        &[],
    )
    .await;
    s.refused(
        "capture",
        json!({
            "subject": "bot:sigma",
            "content": "sigma wants seven",
            "fields": {"rule_seats": "7"},
        }),
    )
    .await
    .says("characters");

    s.wrap("kept the floor under the ceiling").await;
    story.finish().await;
}
