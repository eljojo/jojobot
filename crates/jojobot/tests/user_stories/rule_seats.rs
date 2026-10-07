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
    // Sigma's charter fills the ceiling exactly, and a new colleague then
    // grows its snapshot, which is the one write that never refuses. A write
    // that lowers the floor still lands, and one that grows it does not.
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
    let fits = fill_the_ceiling_with_a_charter(&s, "bot:sigma").await;
    s.add("bot:psi", "Psi").await;
    // Over the ceiling by what the new colleague added, which is more than
    // one character. A charter one longer grows the floor and is refused; one
    // a character shorter lowers it and lands although the boot is still over.
    s.refused(
        "set_charter",
        json!({"bot": "sigma", "prose": charter_of(fits + 1)}),
    )
    .await
    .says("characters");
    s.call(
        "set_charter",
        json!({"bot": "sigma", "prose": charter_of(fits - 1)}),
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

/// **The ceiling a boot may not cross**, as the refusal itself states it.
const BOOT_CEILING: u64 = 28_000;

/// A charter of `n` characters that costs exactly `n` in the boot: one letter
/// has no escape and does not collapse, so the floor moves one for one.
fn charter_of(n: u64) -> String {
    "x".repeat(n as usize)
}

/// **Write `bot` a charter that takes its boot to the ceiling and not past
/// it**, reading the size off the refusal of one too big rather than guessing
/// the weight of everything else a boot carries. Returns the charter's length.
async fn fill_the_ceiling_with_a_charter(s: &super::dsl::Session, bot: &str) -> u64 {
    let probe = 40_000;
    let refused = s
        .refused(
            "set_charter",
            json!({"bot": bot, "prose": charter_of(probe)}),
        )
        .await
        .json();
    let fits = probe
        - refused["over"]
            .as_u64()
            .expect("the refusal says by how much");
    s.call(
        "set_charter",
        json!({"bot": bot, "prose": charter_of(fits)}),
    )
    .await;
    fits
}

/// **A charter is part of what a boot cannot cut, so it is held to the ceiling
/// where it is written** — as a star is.
#[tokio::test]
async fn a_charter_that_would_take_the_boot_over_the_ceiling_is_refused_naming_the_overage() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.call(
        "set_charter",
        json!({"bot": "omega", "prose": "Holds the plan."}),
    )
    .await;

    // ── a charter far past the ceiling is refused, with the sizes ───────────
    let refused = s
        .refused(
            "set_charter",
            json!({"bot": "omega", "prose": charter_of(40_000)}),
        )
        .await;
    refused.says("\"wrote\":false");
    let body = refused.json();
    let floor = body["floor"].as_u64().expect("the refusal names the floor");
    assert_eq!(body["budget"].as_u64(), Some(BOOT_CEILING), "{body}");
    assert_eq!(
        body["over"].as_u64(),
        Some(floor - BOOT_CEILING),
        "the overage is the floor less the ceiling: {body}"
    );
    // **The parts are named, largest first, and they add up to the floor** —
    // so a writer cuts where it helps rather than where it is easy.
    let parts = body["floor_parts"].as_array().expect("the parts are named");
    assert_eq!(parts[0]["part"], "charter", "{body}");
    let sizes: Vec<u64> = parts
        .iter()
        .map(|p| p["characters"].as_u64().expect("a size"))
        .collect();
    assert!(
        sizes.windows(2).all(|w| w[0] >= w[1]),
        "largest first: {body}"
    );
    assert_eq!(sizes.iter().sum::<u64>(), floor, "{body}");
    for named in ["snapshot", "carried rules"] {
        assert!(
            parts.iter().any(|p| p["part"] == named),
            "the floor's parts include {named}: {body}"
        );
    }
    // Nothing was written.
    s.call("recall", json!({"subject": "bot:omega", "prose": true}))
        .await
        .says("Holds the plan.")
        .never_says("xxxxxxxx");

    // ── the boundary: exactly at the ceiling lands, one past does not ───────
    let fits = 40_000 - body["over"].as_u64().unwrap();
    s.call(
        "set_charter",
        json!({"bot": "omega", "prose": charter_of(fits)}),
    )
    .await;
    let one_past = s
        .refused(
            "set_charter",
            json!({"bot": "omega", "prose": charter_of(fits + 1)}),
        )
        .await
        .json();
    assert_eq!(one_past["over"].as_u64(), Some(1), "{one_past}");

    // ── a charter that shrinks the boot lands, whatever it is called ────────
    s.call(
        "set_charter",
        json!({"bot": "omega", "prose": "Holds the plan, briefly."}),
    )
    .await;

    s.wrap("held a charter to the ceiling").await;
    story.finish().await;
}

/// **Creating a bot grows every other bot's snapshot, and never refuses.** A
/// new colleague is not a mistake, so the answer says which bots it took over
/// the ceiling and the write stands.
#[tokio::test]
async fn creating_a_bot_says_which_bots_it_pushes_over_the_ceiling() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;

    // ── nobody is near the ceiling, so nobody is named ──────────────────────
    let roomy = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "upsilon", "name": "Upsilon", "source": "user-named"}),
        )
        .await;
    assert_eq!(roomy.json()["id"], "bot:upsilon");
    roomy.never_says("pushes_over");

    // ── omega sits at the ceiling, and the next colleague takes it over ─────
    fill_the_ceiling_with_a_charter(&s, "bot:omega").await;
    let crowded = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "psi", "name": "Psi", "source": "user-named"}),
        )
        .await;
    let body = crowded.json();
    assert_eq!(body["id"], "bot:psi", "the creation stands: {body}");
    let pushed = body["pushes_over"]
        .as_array()
        .expect("the bots it pushed over are named");
    assert_eq!(pushed.len(), 1, "{body}");
    assert_eq!(pushed[0]["bot"], "bot:omega", "{body}");
    assert!(pushed[0]["over"].as_u64().unwrap() > 0, "{body}");
    assert!(
        pushed[0]["floor"].as_u64().unwrap() > BOOT_CEILING,
        "{body}"
    );

    // ── a bot already over is not pushed again ──────────────────────────────
    //
    // Omega is over the ceiling since the creation above. Another colleague
    // grows its snapshot further, but it was not that creation that took it
    // over, so the answer does not name it.
    let again = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "rho", "name": "Rho", "source": "user-named"}),
        )
        .await;
    assert_eq!(again.json()["id"], "bot:rho");
    again.never_says("pushes_over");

    s.wrap("added colleagues beside a full boot").await;
    story.finish().await;
}
