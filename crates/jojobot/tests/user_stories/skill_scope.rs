//! "A skill is for some bots. Does the index an agent is handed say so?"
//!
//! **The scope narrows what is listed and never what is fetched.** A skill scoped
//! to one bot is listed at that bot's boot and not at another bot's or at an
//! anonymous one, while a fetch by name answers all three. No shipped skill is
//! scoped, so the case runs over a list of its own: one skill for everybody and
//! one for a single roster bot.

use jojobot_mcp::orientation::skills::Skill;
use serde_json::{Value, json};

use super::dsl::Story;

const EVERYONE: &str = "probe-for-everyone";
const BART_ONLY: &str = "probe-for-bart";

/// Skills for this case only: the build's own scope list stays what it ships.
static SCOPED: &[Skill] = &[
    Skill {
        name: EVERYONE,
        when_to_use: "Probe line for every bot.",
        body: "PROBE BODY FOR EVERYONE",
        covers: &[],
        for_bots: &[],
    },
    Skill {
        name: BART_ONLY,
        when_to_use: "Probe line for one bot.",
        body: "PROBE BODY FOR BART",
        covers: &[],
        for_bots: &["bot:bart"],
    },
];

/// The names a boot's index lists.
fn listed(boot: &Value) -> Vec<String> {
    boot["skills"]
        .as_array()
        .unwrap_or_else(|| panic!("a boot lists its skills: {boot}"))
        .iter()
        .map(|skill| skill["name"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn a_skill_scoped_to_one_bot_is_listed_for_that_bot_and_fetched_by_all() {
    let story = Story::begin_with_skills("bot:otto", SCOPED).await;
    let s = story.session().await;
    s.add("bot:bart", "Bart").await;
    s.add("bot:marge", "Marge").await;

    // ── the scoped bot's boot lists both skills ─────────────────────────────
    let (bart_boot, bart) = story.call("start_here", json!({"bot": "bart"})).await;
    let bart = bart.expect("a boot hands back a handle");
    assert_eq!(
        listed(&bart_boot.json()),
        [EVERYONE, BART_ONLY],
        "{}",
        bart_boot.json()
    );

    // ── another bot's boot and the anonymous boot list only the shared one ──
    let (marge_boot, marge) = story.call("start_here", json!({"bot": "marge"})).await;
    let marge = marge.expect("a boot hands back a handle");
    assert_eq!(
        listed(&marge_boot.json()),
        [EVERYONE],
        "{}",
        marge_boot.json()
    );
    let (anonymous, none) = story.call("start_here", json!({})).await;
    assert!(none.is_none(), "an anonymous boot opens no run");
    assert_eq!(
        listed(&anonymous.json()),
        [EVERYONE],
        "{}",
        anonymous.json()
    );

    // ── a fetch by name answers all three, the scoped skill included ────────
    for (who, fetched) in [
        (
            "bart",
            bart.call("start_here", json!({"skill": BART_ONLY})).await,
        ),
        (
            "marge",
            marge.call("start_here", json!({"skill": BART_ONLY})).await,
        ),
        (
            "anonymous",
            story
                .call("start_here", json!({"skill": BART_ONLY}))
                .await
                .0,
        ),
    ] {
        let fetched = fetched.json();
        assert_eq!(
            fetched["skill"]["body"], "PROBE BODY FOR BART",
            "{who} could not fetch the scoped skill: {fetched}"
        );
    }
    story.finish().await;
}
