//! "This rule's reasoning is three paragraphs — why does it read as one?"
//!
//! A claim's headline is one line, and its details are the description under
//! it. A bot's rules carry their reasoning in details and are read at every
//! boot by the bot they govern, so the breaks a writer put there have to
//! survive every surface that serves them: the write's own read-back, the
//! recall, the boot and the operator's page.

use serde_json::json;

use super::dsl::Story;

const REASONING: &str = "Why this rule exists.\n\nWhat it forbids, next to \
                         @thing:handcart and nothing else.\n\nWhat to do instead.";

const CORRECTED: &str = "The corrected reasoning.\n\nSecond paragraph of the correction.";

#[tokio::test]
async fn a_rules_reasoning_keeps_its_paragraphs_from_the_write_to_the_page() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;

    s.add("bot:gamma", "Gamma").await;
    s.add("thing:handcart", "The Handcart").await;

    // ── the write: details with paragraph breaks are accepted ───────────────
    let written = s
        .call(
            "capture",
            json!({
                "subject": "bot:gamma",
                "content": "never rides the handcart alone",
                "details": REASONING,
                "provenance": "testimony",
                "fields": {"starred": "true"},
            }),
        )
        .await;
    // The receipt says how many bytes were STORED, and the breaks are bytes.
    assert_eq!(
        written.json()["details_bytes"].as_u64(),
        Some(REASONING.len() as u64),
        "the receipt counts the bytes stored"
    );

    // ── the headline above it is still one line ─────────────────────────────
    s.refused(
        "capture",
        json!({
            "subject": "bot:gamma",
            "content": "a headline\nin two lines",
            "provenance": "testimony",
        }),
    )
    .await
    .says("one line");

    // ── the recall: the same bytes come back ────────────────────────────────
    let recalled = s.recall("bot:gamma").await;
    let facts = recalled.json()["objects"][0]["facts"].clone();
    let rule = facts
        .as_array()
        .expect("the facts are a list")
        .iter()
        .find(|f| f["content"] == "never rides the handcart alone")
        .expect("the rule is there");
    assert_eq!(rule["details"], REASONING);

    // ── the boot: the rule the bot reads every morning keeps its breaks ─────
    let (boot, _) = story
        .call("start_here", json!({"bot": "gamma", "brief": true}))
        .await;
    let rules = boot.json()["identity"]["rules"].clone();
    assert_eq!(
        rules[0]["details"], REASONING,
        "the boot flattened the rule's paragraphs: {rules}"
    );

    // ── a correction may change the paragraphs, and reads back the same way ─
    let address = rule["address"].as_str().expect("an address").to_string();
    s.call(
        "update_fact",
        json!({"address": address, "details": CORRECTED}),
    )
    .await;
    let recalled = s.recall("bot:gamma").await;
    let corrected = recalled.json()["objects"][0]["facts"]
        .as_array()
        .expect("the facts are a list")
        .iter()
        .find(|f| f["address"] == address.as_str())
        .expect("the rule is still there")
        .clone();
    assert_eq!(corrected["details"], CORRECTED);

    // ── the operator's page shows the breaks and keeps the link ─────────────
    s.call(
        "update_fact",
        json!({"address": address, "details": REASONING}),
    )
    .await;
    let page = story.page("bot:gamma").await;
    page.says("Why this rule exists.<br>\n<br>\nWhat it forbids");
    page.says("What to do instead.");
    // The mention in the middle paragraph is still a link to the thing.
    page.says(">@thing:handcart</a>");
}
