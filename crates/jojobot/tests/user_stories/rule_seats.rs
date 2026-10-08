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
    .says("set_charter")
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
        .says("set_charter")
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
    let grown = s
        .call(
            "add_entity",
            json!({"kind": "bot", "handle": "psi", "name": "Psi", "source": "user-named"}),
        )
        .await
        .json();
    // The premise of what follows: the new colleague took sigma over the
    // ceiling, so the two writes below are asked of a bot that is over.
    assert_eq!(grown["id"], "bot:psi", "the creation stands: {grown}");
    let pushed = grown["pushes_over"]
        .as_array()
        .expect("the bots it pushed over are named");
    assert!(
        pushed.iter().any(|over| over["bot"] == "bot:sigma"),
        "sigma was not pushed over the ceiling, so it is not over when the writes below are \
         asked: {grown}"
    );
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
    // Every blocked body carries `candidates`, so one client branch reads them.
    assert!(body["candidates"].is_array(), "{body}");
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

/// **A boot the world grew past the ceiling says so, and names what to drop.**
/// Any entity grows every bot's snapshot, so a write that cannot be refused for
/// it can still leave a bot's boot over. The ceiling is a property of the boot
/// answer, so the answer reports it: the same sizes the refusals carry, and the
/// ways down.
///
/// Paired with the same bot one write earlier, exactly at the ceiling, where
/// the answer says nothing: a build that always said it passes the second half
/// alone.
#[tokio::test]
async fn a_boot_the_world_grew_past_the_ceiling_says_so_and_names_what_to_drop() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;

    // ── omega's boot sits exactly at the ceiling ────────────────────────────
    //
    // A write measures the floor without the session block a boot carries, so a
    // charter that fills the ceiling at the write boots a little over. The
    // boot's own answer says by how much, and that is what is taken back off.
    let filled = fill_the_ceiling_with_a_charter(&s, "bot:omega").await;
    let (probe, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let session_block = probe.json()["over_the_ceiling"]["over"]
        .as_u64()
        .expect("a boot over its ceiling says by how much");
    s.call(
        "set_charter",
        json!({"bot": "omega", "prose": charter_of(filled - session_block)}),
    )
    .await;
    let (at, _) = story.call("start_here", json!({"bot": "omega"})).await;
    at.never_says("over_the_ceiling");

    // ── a person arrives, which no rule refuses ─────────────────────────────
    s.add("person:milhouse", "Milhouse").await;
    let (over, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let body = over.json();
    let said = &body["over_the_ceiling"];
    assert!(said["over"].as_u64().unwrap_or(0) >= 1, "{body}");
    assert_eq!(said["budget"].as_u64(), Some(BOOT_CEILING), "{body}");
    let parts = said["floor_parts"].as_array().expect("the parts are named");
    assert_eq!(parts[0]["part"], "charter", "{body}");
    assert_eq!(
        parts
            .iter()
            .map(|p| p["characters"].as_u64().unwrap())
            .sum::<u64>(),
        said["floor"].as_u64().unwrap(),
        "the parts add up to the floor: {body}"
    );
    // The ways down are named by the verb that does each.
    let how = said["how_to_proceed"].as_str().expect("a way forward");
    for verb in ["set_charter", "update_fact"] {
        assert!(how.contains(verb), "{verb} is a way down: {how}");
    }

    s.wrap("added a person beside a full boot").await;
    story.finish().await;
}

/// **A charter is measured as the boot composes it.** The boot serves the
/// build's own layer joined to what the instance wrote, so a charter that fits
/// alone can still leave the boot over. The shipped identity carries a layer
/// and a bot nobody shipped for does not: filled to the ceiling by the same
/// check, their boots are the same size within what names and records weigh.
/// Counting the proposed charter alone left the shipped identity several
/// thousand characters over.
#[tokio::test]
async fn a_charter_is_measured_with_the_layer_the_build_ships() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;

    fill_the_ceiling_with_a_charter(&s, "bot:omega").await;
    fill_the_ceiling_with_a_charter(&s, "bot:assistant").await;
    let (omega, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let (assistant, _) = story.call("start_here", json!({"bot": "assistant"})).await;
    let (omega, assistant) = (omega.size(), assistant.size());
    assert!(
        omega.abs_diff(assistant) < 1_000,
        "the check lets the shipped identity's boot run {assistant} against {omega}"
    );

    s.wrap("filled two bots to the ceiling and compared their boots")
        .await;
    story.finish().await;
}

/// One rule on `bot`, written by the session's own bot — a claim with the
/// fields it is given, in force until somebody archives it.
async fn rule(
    s: &super::dsl::Session,
    bot: &str,
    content: &str,
    fields: serde_json::Value,
) -> String {
    let receipt = s
        .call(
            "capture",
            json!({"subject": bot, "content": content, "provenance": "testimony", "fields": fields}),
        )
        .await;
    receipt.json()["address"]
        .as_str()
        .expect("a write hands back the address")
        .to_string()
}

/// **A boot lists every rule in force that has no seat, one line each, so a
/// session can tell which one to load.** Seats stay all-or-nothing in what they
/// carry whole; the line is the map of the rest. The subject field is the line
/// when the rule has one, and the head of its words when it has not. A retired
/// rule is never listed, and a listed rule is fetched exactly, by its address.
#[tokio::test]
async fn a_boot_lists_each_unseated_rule_and_a_session_fetches_one_by_its_address() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;

    // Eight starred rules and the default five seats: the three oldest are not
    // seated. Two more in force, with no star, and one retired. They read like
    // rules a bot would hold, each with an area for its line.
    let starred_rules = [
        (
            "Check the oven timer before leaving the kitchen",
            "the oven",
        ),
        (
            "Sleep on any message that sounds angry before replying",
            "angry mail",
        ),
        (
            "Confirm a booking by the next morning or it does not count",
            "bookings",
        ),
        ("Keep a spare charger in the bag on any trip", "chargers"),
        ("Put the bins out on the evening before collection", "bins"),
        (
            "Back up the photos on the first of the month",
            "photo backups",
        ),
        (
            "Ask before moving anything on the shared shelf",
            "the shared shelf",
        ),
        (
            "Re-read a contract's dates before signing it",
            "contract dates",
        ),
    ];
    let mut starred = Vec::new();
    for (words, area) in starred_rules {
        starred.push(
            rule(
                &s,
                "bot:omega",
                words,
                json!({"starred": "true", "subject": area}),
            )
            .await,
        );
    }
    let plain = rule(
        &s,
        "bot:omega",
        "Keep the recycling out by Thursday night",
        json!({"subject": "recycling"}),
    )
    .await;
    let bare = rule(
        &s,
        "bot:omega",
        "Lock the shed after using the mower",
        json!({}),
    )
    .await;
    let retired = rule(
        &s,
        "bot:omega",
        "Feed the cat at six, before the post arrives",
        json!({"subject": "the cat"}),
    )
    .await;
    s.call(
        "update_fact",
        json!({"address": retired, "status": "archived"}),
    )
    .await;

    let (booted, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let body = booted.json();
    let listed = body["identity"]["unseated_rules"]["listed"]
        .as_array()
        .unwrap_or_else(|| panic!("a boot with unseated rules lists them: {body}"));
    let line_of = |address: &str| {
        listed
            .iter()
            .find(|entry| entry["address"] == address)
            .map(|entry| entry["line"].as_str().unwrap_or_default().to_string())
    };
    // The three oldest starred rules, and the two plain ones: five lines.
    assert_eq!(listed.len(), 5, "{body}");
    for old in &starred[..3] {
        assert!(line_of(old).is_some(), "{old} has no line: {body}");
    }
    assert_eq!(line_of(&plain).as_deref(), Some("recycling"), "{body}");
    assert!(
        line_of(&bare)
            .expect("a rule with no subject is listed too")
            .starts_with("Lock the shed"),
        "the head of its words is the line: {body}"
    );
    // A seated rule is carried whole and not listed again; a retired one is
    // neither.
    for seated in &starred[3..] {
        assert!(line_of(seated).is_none(), "{seated} is seated: {body}");
    }
    booted.never_says("Feed the cat");
    booted.never_says("the cat");
    // The note names the call that loads one rule by its address.
    let how = body["identity"]["unseated_rules"]["how_to_load"]
        .as_str()
        .expect("the note says how to load");
    assert!(how.contains("history_record"), "{how}");
    // The call that loads everything returns every active record, so the note
    // says the rules are among them rather than that it loads only rules.
    assert!(how.contains("among"), "{how}");

    // ── one rule, fetched exactly by the address its line carries ───────────
    let one = s
        .call("recall", json!({"history_record": starred[0]}))
        .await;
    one.says("Check the oven timer");
    one.never_says("Sleep on any message");
    one.never_says("Keep the recycling");

    s.wrap("saw the unseated rules and fetched one").await;
    story.finish().await;
}

/// **The list is bounded by a count of things, and past it the rest are
/// counted by what they are for.** A bot holding sixty rules does not get a
/// boot sixty lines longer: it gets the listed few, a count of the rest grouped
/// by their `purpose` (rules that name none are `unfiled`), and the call that
/// loads a group or all of them. The count is a literal here because nothing
/// outside this process declares it.
#[tokio::test]
async fn a_boot_lists_a_bounded_number_of_unseated_rules_and_counts_the_rest_by_purpose() {
    const LISTED: usize = 20;
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:upsilon", "Upsilon").await;

    // Omega holds sixty rules, five of them seated by their stars. Fifty-five
    // are not: ten of one purpose, fifteen of another, thirty with none.
    for n in 0..5 {
        rule(
            &s,
            "bot:omega",
            &format!("a seated rule {n}"),
            json!({"starred": "true"}),
        )
        .await;
    }
    for n in 0..55 {
        let fields = match n {
            0..10 => json!({"purpose": "hard-line", "subject": format!("line {n}")}),
            10..25 => json!({"purpose": "habit", "subject": format!("habit {n}")}),
            _ => json!({"subject": format!("unlabelled {n}")}),
        };
        rule(&s, "bot:omega", &format!("an unseated rule {n}"), fields).await;
    }
    // Upsilon holds thirty: the same five seated, twenty-five unseated.
    for n in 0..5 {
        rule(
            &s,
            "bot:upsilon",
            &format!("a seated rule {n}"),
            json!({"starred": "true"}),
        )
        .await;
    }
    for n in 0..25 {
        rule(
            &s,
            "bot:upsilon",
            &format!("an unseated rule {n}"),
            json!({"subject": format!("unlabelled {n}")}),
        )
        .await;
    }

    let (omega, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let body = omega.json();
    let unseated = &body["identity"]["unseated_rules"];
    assert_eq!(
        unseated["listed"].as_array().map(Vec::len),
        Some(LISTED),
        "{body}"
    );
    // Newest first: the twenty listed are the twenty most recently written, and
    // the thirty-five left out are the oldest.
    let listed = unseated["listed"].as_array().expect("the listed rules");
    assert_eq!(listed[0]["line"], "unlabelled 54", "{body}");
    assert_eq!(listed[LISTED - 1]["line"], "unlabelled 35", "{body}");
    let left_out = &unseated["left_out"];
    assert_eq!(left_out["count"], 55 - LISTED, "{body}");
    let by_purpose = left_out["by_purpose"]
        .as_object()
        .expect("grouped by purpose");
    assert_eq!(
        by_purpose
            .values()
            .map(|n| n.as_u64().unwrap())
            .sum::<u64>(),
        (55 - LISTED) as u64,
        "the groups add up to what was left out: {body}"
    );
    assert!(by_purpose.contains_key("unfiled"), "{body}");
    assert!(by_purpose.contains_key("habit"), "{body}");
    // The call that loads them is named, both for everything and for a group.
    let how = unseated["how_to_load"].as_str().expect("the call is named");
    for argument in ["history_record", "status", "purpose"] {
        assert!(how.contains(argument), "{argument} is named: {how}");
    }

    // The group call the note names does what it says: one purpose, the rules
    // in force, nothing of another group.
    let habits = s
        .call(
            "recall",
            json!({
                "subject": "bot:omega", "facts": true, "status": "active",
                "fields": [{"key": "purpose", "value": "habit", "scope": "record"}],
            }),
        )
        .await;
    habits.says("an unseated rule 10\"");
    habits.never_says("an unseated rule 3\"");
    habits.never_says("an unseated rule 30\"");

    // ── what the listing costs is bounded by the cap, not by the rule count ─
    //
    // Upsilon has twenty-five rules to list and omega has fifty-five: past the
    // cap the extra thirty cost a few group counts and nothing else.
    let (upsilon, _) = story.call("start_here", json!({"bot": "upsilon"})).await;
    let (omega_size, upsilon_size) = (omega.size(), upsilon.size());
    assert!(
        omega_size.abs_diff(upsilon_size) < 700,
        "thirty more rules cost {} characters of boot",
        omega_size.abs_diff(upsilon_size)
    );
    assert!(omega_size <= BOOT_CEILING as usize, "{omega_size}");

    s.wrap("booted a bot holding sixty rules").await;
    story.finish().await;
}

/// **The groups a boot counts the left-out rules under are bounded in number
/// and in the length of each name.** A rule's `purpose` is whatever its writer
/// typed, so grouping by the raw value let one long purpose, or one distinct
/// purpose per rule, cost a boot as much as listing every rule would have. Each
/// group name is cut to the length of a listed line, the groups are capped, and
/// what the cap leaves out is still counted, so the counts add up to the rules
/// left out. A purpose that is short and common keeps its name whole.
#[tokio::test]
async fn a_boot_bounds_the_purpose_groups_it_counts_and_the_length_of_each_name() {
    const LISTED: usize = 20;
    const GROUPS: usize = 10;
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:sigma", "Sigma").await;

    // Twenty rules fill the listing; the next ones are counted. Three of one
    // short purpose, one with an enormous purpose, and thirty each with a
    // purpose of its own.
    for n in 0..LISTED {
        rule(
            &s,
            "bot:sigma",
            &format!("a listed rule {n}"),
            json!({"subject": format!("listed {n}")}),
        )
        .await;
    }
    let enormous = "an enormously long purpose ".repeat(200);
    for n in 0..3 {
        rule(
            &s,
            "bot:sigma",
            &format!("a habit {n}"),
            json!({"purpose": "habit", "subject": format!("habit {n}")}),
        )
        .await;
    }
    rule(
        &s,
        "bot:sigma",
        "a rule with a purpose nobody could print",
        json!({"purpose": enormous, "subject": "the long one"}),
    )
    .await;
    for n in 0..30 {
        rule(
            &s,
            "bot:sigma",
            &format!("a one-off {n}"),
            json!({"purpose": format!("one-off purpose {n:02}"), "subject": format!("one-off {n}")}),
        )
        .await;
    }
    let left_over = 3 + 1 + 30;

    let (sigma, _) = story.call("start_here", json!({"bot": "sigma"})).await;
    let body = sigma.json();
    let left_out = &body["identity"]["unseated_rules"]["left_out"];
    assert_eq!(left_out["count"], left_over, "{body}");
    let by_purpose = left_out["by_purpose"]
        .as_object()
        .expect("grouped by purpose");

    assert!(
        by_purpose.len() <= GROUPS,
        "{} groups against a cap of {GROUPS}: {left_out}",
        by_purpose.len()
    );
    for name in by_purpose.keys() {
        assert!(
            name.chars().count() <= 80,
            "a group name is as short as a listed line: {name:?}"
        );
    }
    // The positive beside both bounds: the common short purpose keeps its name
    // and its count, as the groups did before.
    assert_eq!(by_purpose["habit"], 3, "{left_out}");
    // What the cap leaves out is counted, so the whole adds up.
    let shown: u64 = by_purpose.values().map(|n| n.as_u64().unwrap()).sum();
    let beyond = &left_out["other_groups"];
    assert_eq!(
        shown + beyond["rules"].as_u64().expect("the rest are counted"),
        left_over as u64,
        "the groups and what the cap left out add up to the rules left out: {left_out}"
    );
    assert!(
        beyond["groups"].as_u64().unwrap() > 0,
        "the cap left groups out: {left_out}"
    );

    s.wrap("booted a bot whose rules carry many purposes").await;
    story.finish().await;
}

/// **The call a boot names for loading rules returns the ones in force, and
/// says what it left out.** `recall` on a bot with `facts: true` hands back
/// every record it holds, retired ones included, because going straight to a
/// known address is the direct door. The boot names the same call with
/// `status: "active"`: the retired records stay out, and the answer says how
/// many and which call returns them.
#[tokio::test]
async fn loading_a_bots_rules_by_status_active_leaves_out_the_retired_and_counts_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    rule(
        &s,
        "bot:omega",
        "a rule in force",
        json!({"subject": "one"}),
    )
    .await;
    let retired = rule(
        &s,
        "bot:omega",
        "a rule retired long ago",
        json!({"subject": "two"}),
    )
    .await;
    s.call(
        "update_fact",
        json!({"address": retired, "status": "archived"}),
    )
    .await;

    // ── unchanged without the argument: every record, retired included ──────
    s.call("recall", json!({"subject": "bot:omega", "facts": true}))
        .await
        .says("a rule in force")
        .says("a rule retired long ago");

    // ── with it: the retired record is out, and the count and the way back ──
    let active = s
        .call(
            "recall",
            json!({"subject": "bot:omega", "facts": true, "status": "active"}),
        )
        .await;
    active
        .says("a rule in force")
        .never_says("a rule retired long ago");
    let body = active.json();
    let left_out = &body["objects"][0]["archived_left_out"];
    assert_eq!(left_out["count"], 1, "{body}");
    assert!(
        left_out["how_to_read"]
            .as_str()
            .is_some_and(|how| how.contains("archived")),
        "the call that returns them is named: {body}"
    );

    // ── and the other way: asking for the retired ones returns only those ───
    s.call(
        "recall",
        json!({"subject": "bot:omega", "facts": true, "status": "archived"}),
    )
    .await
    .says("a rule retired long ago")
    .never_says("a rule in force");

    s.wrap("loaded a bot's rules in force").await;
    story.finish().await;
}

/// **The floor a write measures counts the unseated listing**, because the
/// listing rides every boot. Two bots filled to the ceiling by the same check,
/// one holding thirty rules with no seat and one holding none, boot over it by
/// nearly the same amount — what a boot carries beyond what a write measures.
/// A floor that left the listing out would let the first boot over by the whole
/// listing as well.
#[tokio::test]
async fn the_floor_a_write_measures_counts_the_unseated_listing() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:omega", "Omega").await;
    s.add("bot:upsilon", "Upsilon").await;
    for n in 0..30 {
        rule(
            &s,
            "bot:omega",
            &format!("an unseated rule {n}"),
            json!({"subject": format!("area {n}")}),
        )
        .await;
    }
    fill_the_ceiling_with_a_charter(&s, "bot:omega").await;
    fill_the_ceiling_with_a_charter(&s, "bot:upsilon").await;
    let over = |answer: super::dsl::Answer| {
        answer.json()["over_the_ceiling"]["over"]
            .as_u64()
            .expect("a boot filled to the write's ceiling is over by what the write omits")
    };
    let (omega, _) = story.call("start_here", json!({"bot": "omega"})).await;
    let (upsilon, _) = story.call("start_here", json!({"bot": "upsilon"})).await;
    let (omega, upsilon) = (over(omega), over(upsilon));
    // The two differ by the note a boot adds when some rules are not carried,
    // which a write does not measure, and by nothing else: the listing alone
    // is several times that.
    assert!(
        omega.abs_diff(upsilon) < 700,
        "the listing is outside the floor a write measures: omega boots {omega} over, \
         upsilon {upsilon}"
    );

    s.wrap("filled two bots to the ceiling, one holding many rules")
        .await;
    story.finish().await;
}

/// **A bot is made first and starred after, and the star is held to the ceiling.**
/// A creation cannot carry a star, because a star only labels its claim and a
/// creation sets keys on the new thing. So the way to a starred rule on a new bot
/// is to make the bot and capture the rule, and that capture is where the boot is
/// measured. A rule that fits lands, so a build that refused every starred rule
/// would not pass; a rule that would leave the boot over is refused and writes
/// nothing; and a creation that tries to carry the star is turned back by name.
#[tokio::test]
async fn a_bot_made_then_starred_is_held_to_the_ceiling() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let heavy = "heavyrule ".repeat(4000);

    // ── a creation cannot carry the star ────────────────────────────────────
    s.refused(
        "add_entity",
        json!({
            "kind": "bot", "handle": "omega", "name": "omega", "source": "user-named",
            "sets": {"starred": "true", "subject": heavy},
        }),
    )
    .await
    .says("starred")
    .says("capture");

    // ── the route: make the bot, then capture the rule ──────────────────────
    s.add("bot:sigma", "sigma").await;
    // Nothing was created for the refused creation: the handle is free and the
    // bot is not listed, beside the one that was made, so an empty listing would
    // not pass.
    s.list("bot")
        .await
        .never_says("bot:omega")
        .says("bot:sigma");
    let fits = s
        .call(
            "capture",
            json!({
                "subject": "bot:sigma",
                "content": "sigma rule",
                "fields": {"starred": "true", "subject": "a short rule"},
            }),
        )
        .await
        .json();
    assert!(
        fits["address"]
            .as_str()
            .is_some_and(|a| a.starts_with("bot:sigma#")),
        "{fits}"
    );

    // ── a rule that would leave the boot over is refused and writes nothing ─
    s.refused(
        "capture",
        json!({
            "subject": "bot:sigma",
            "content": "sigma heavy rule",
            "fields": {"starred": "true", "subject": heavy},
        }),
    )
    .await
    .says("rule_seats")
    .says("set_charter")
    .says("\"wrote\":false");
    story.finish().await;
}

/// **The refusal at the floor names the summary that keeps its sources, and a
/// writer who follows it lands.** A bot whose boot is full is refused a star that
/// would take it over the ceiling. The way out the refusal offers is not only to
/// shorten or to fetch by skill: it is to write one record that stands for the
/// others, keep the others unstarred, and read the boot back. The story follows
/// it, and the sources of the summary come back when they are asked for.
#[tokio::test]
async fn a_refusal_at_the_floor_names_the_summary_that_keeps_its_sources_and_following_it_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:epsilon", "Epsilon").await;

    // Eight rules of two thousand characters each, all starred: the floor fits.
    let rule = "w ".repeat(1000);
    let mut sources = Vec::new();
    for n in 0..8 {
        sources.push(
            s.call(
                "capture",
                json!({
                    "subject": "bot:epsilon",
                    "content": format!("{n} {rule}"),
                    "fields": {"starred": "true"},
                }),
            )
            .await
            .field("address"),
        );
    }

    // ── the star that does not fit, and the move the refusal names ──────────
    s.refused(
        "capture",
        json!({
            "subject": "bot:epsilon",
            "content": "heavyrule ".repeat(4000),
            "fields": {"starred": "true"},
        }),
    )
    .await
    .says("stands_for")
    .says("\"wrote\":false");

    // ── follow it: one short record, marked as standing for the eight, and the
    //    eight kept but no longer starred ─────────────────────────────────────
    let summary = s
        .call(
            "capture",
            json!({
                "subject": "bot:epsilon",
                "content": "the eight working rules, in short",
                "fields": {"starred": "true"},
            }),
        )
        .await
        .field("address");
    s.call(
        "update_fact",
        json!({"address": &summary, "stands_for": &sources}),
    )
    .await;
    for address in &sources {
        s.call(
            "update_fact",
            json!({"address": address, "fields": {"starred": "false"}}),
        )
        .await;
    }

    // ── read the boot back: the summary rides it, the eight do not ──────────
    let (booted, _) = story
        .call("start_here", json!({"bot": "epsilon", "brief": true}))
        .await;
    let rules = booted.json()["identity"]["rules"]
        .as_array()
        .expect("the boot carries the bot's rules")
        .clone();
    assert_eq!(rules.len(), 1, "only the summary rides the boot: {rules:?}");
    assert!(
        rules[0]["content"]
            .as_str()
            .is_some_and(|content| content.contains("the eight working rules")),
        "the rule that rides is the summary: {rules:?}"
    );

    // ── and its sources come back when they are asked for ───────────────────
    let whole = s
        .shape(
            "the summary, sources included",
            json!({"subject": "bot:epsilon", "facts": true, "stood_for": true}),
        )
        .await;
    whole
        .says("the eight working rules")
        .says("0 w w")
        .says("7 w w");

    s.wrap("summarised the rules and kept their sources").await;
    story.finish().await;
}

/// **Unstarring three rules is taught `stands_for`, unless the record that lands
/// names what it replaces.** A rule that is unstarred is kept but no longer rides
/// the boot, and a record written beside the unstars does not lead back to them
/// unless it stands for them. The receipt of that record says so, naming
/// `stands_for`. The pair beside it: the same three unstars and a summary that
/// stands for them carry no such teaching, and two unstars alone are not enough.
#[tokio::test]
async fn unstarring_three_rules_is_taught_stands_for_unless_the_summary_names_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add("bot:delta", "Delta").await;
    s.add("bot:gamma", "Gamma").await;

    // Two bots, three starred rules each.
    let mut rules = std::collections::BTreeMap::new();
    for bot in ["bot:delta", "bot:gamma"] {
        let mut addresses = Vec::new();
        for n in 0..3 {
            addresses.push(
                s.call(
                    "capture",
                    json!({
                        "subject": bot,
                        "content": format!("{bot} rule {n}"),
                        "fields": {"starred": "true"},
                    }),
                )
                .await
                .field("address"),
            );
        }
        rules.insert(bot, addresses);
    }
    let unstar = |address: String| {
        let s = &s;
        async move {
            s.call(
                "update_fact",
                json!({"address": address, "fields": {"starred": "false"}}),
            )
            .await
        }
    };
    // The teaching sits in the receipt's teaching list; the unstar's own receipt
    // never carries it, whichever unstar it is.
    let taught = |answer: &super::dsl::Answer| {
        answer.json()["teaching"].as_array().is_some_and(|lines| {
            lines
                .iter()
                .any(|line| line.as_str().is_some_and(|l| l.contains("unstarred")))
        })
    };

    // ── two unstars and a record: not enough to teach anything ──────────────
    for address in &rules["bot:gamma"][..2] {
        let answer = unstar(address.clone()).await;
        assert!(!taught(&answer), "an unstar is not taught about itself");
    }
    let few = s
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "a record beside two unstars"}),
        )
        .await;
    assert!(!taught(&few), "two unstars teach nothing: {}", few.raw());

    // ── three unstars and a record that names nothing: taught ───────────────
    for address in &rules["bot:delta"] {
        let answer = unstar(address.clone()).await;
        assert!(!taught(&answer), "an unstar is not taught about itself");
    }
    let alone = s
        .call(
            "capture",
            json!({"subject": "bot:delta", "content": "a summary that names nothing"}),
        )
        .await;
    assert!(
        taught(&alone),
        "three unstars and a bare record are taught: {}",
        alone.raw()
    );
    alone.says("stands_for");

    // ── the pair: the summary that stands for them carries no teaching ──────
    let third = unstar(rules["bot:gamma"][2].clone()).await;
    assert!(!taught(&third), "an unstar is not taught about itself");
    let summary = s
        .call(
            "capture",
            json!({"subject": "bot:gamma", "content": "the three rules, in short"}),
        )
        .await;
    // Written bare first, as the ordinary flow does, so it is taught once; the
    // mark then lands on it and the next record beside it is not.
    assert!(
        taught(&summary),
        "a bare record beside three unstars is taught"
    );
    let address = summary.field("address");
    let marked = s
        .call(
            "update_fact",
            json!({"address": &address, "stands_for": &rules["bot:gamma"]}),
        )
        .await;
    assert!(
        !taught(&marked),
        "a record that stands for them carries no teaching: {}",
        marked.raw()
    );

    s.wrap("unstarred rules and summarised them").await;
    story.finish().await;
}

/// **A creation whose own arguments take the new bot's boot over the ceiling is
/// refused, and the way forward the refusal names is the real one.** The floor a
/// creation is measured on is the new bot's boot: its name, its aliases and its
/// source ride in it, and what the call sets does not. So an oversized name is
/// what the refusal is about, and a shorter one is what lets the same call land.
/// A set value of the same size lands beside it, which shows the size of `sets`
/// is not what was refused.
///
/// Paired with the same creation under a short name, which lands: a build that
/// refused every creation naming a seat count would not pass.
#[tokio::test]
async fn a_creation_with_an_oversized_name_is_refused_and_a_shorter_one_lands() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    let creation = |handle: &str, name: &str| {
        json!({
            "kind": "bot", "handle": handle, "name": name, "source": "user-named",
            "sets": {"rule_seats": "3"},
        })
    };

    // ── the oversized name is what is refused, and the answer says which ────
    let refused = s
        .refused("add_entity", creation("sigma", &"n".repeat(60_000)))
        .await;
    refused
        .says("floor_parts")
        .says("aliases")
        .says("\"wrote\":false");
    s.list("bot").await.never_says("bot:sigma");

    // ── the way forward it names: the same call under a shorter name lands ──
    let made = s
        .call("add_entity", creation("sigma", "Sigma"))
        .await
        .json();
    assert_eq!(made["id"], "bot:sigma", "{made}");

    // ── a set value as large as that name lands, so `sets` was not the cause ─
    let made = s
        .call(
            "add_entity",
            json!({
                "kind": "bot", "handle": "psi", "name": "Psi", "source": "user-named",
                "sets": {"rule_seats": "3", "note": "v".repeat(60_000)},
            }),
        )
        .await
        .json();
    assert_eq!(made["id"], "bot:psi", "{made}");
    story.finish().await;
}
