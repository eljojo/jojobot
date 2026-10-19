//! "What did the town decide, and what replaced what?"
//!
//! **A container of decisions shows them, in full and numbered.** Reading a
//! topic that holds decisions puts the standing ones first, as a thread does: each
//! with its number and its words, in number order, in the one call. A decision
//! that another replaced is archived, counted as replaced, and leads to the one
//! that replaced it. Reading the replaced decision by its number says so and
//! carries the new words.

use serde_json::{Value, json};

use super::dsl::{Session, Story};

const RULES: &str = "topic:rules-springfield";

/// A numbered topic of rules, with an entry for each decision given:
/// (slug, number, name, words). The names differ, so the near-name screen has
/// nothing to ask about.
async fn rules_of_springfield(s: &Session, decisions: &[(&str, &str, &str, &str)]) {
    s.add(RULES, "Springfield rules").await;
    s.call(
        "capture",
        json!({"subject": RULES, "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    for (slug, number, name, words) in decisions {
        s.call(
            "add_entity",
            json!({"kind": "decision", "handle": slug, "name": name,
                   "source": "user-named", "parent": RULES, "sets": {"number": number}}),
        )
        .await;
        s.call(
            "capture",
            json!({"subject": format!("decision:{slug}"), "content": words,
                   "provenance": "testimony"}),
        )
        .await;
    }
}

/// Six letters for each number, from a fixed generator, so no two slugs sit a
/// letter apart and the near-slug screen has nothing to ask about.
fn spelled(n: usize) -> String {
    let mut state = (n as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (0..6)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            char::from(b'a' + ((state >> 33) % 26) as u8)
        })
        .collect()
}

fn ids(read: &Value) -> Vec<String> {
    read["objects"]
        .as_array()
        .unwrap_or_else(|| panic!("an answer has objects: {read}"))
        .iter()
        .map(|object| object["id"].as_str().unwrap_or_default().to_string())
        .collect()
}

#[tokio::test]
async fn a_topic_leads_with_its_standing_decisions_numbered_and_counts_the_replaced_one() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    // Added out of order, so the order read back is the numbers' and not arrival's.
    rules_of_springfield(
        &s,
        &[
            (
                "springfield-14",
                "14",
                "Ten o'clock",
                "Fireworks end at ten",
            ),
            (
                "springfield-12",
                "12",
                "Leads",
                "Dogs on leads in the square",
            ),
            (
                "springfield-13",
                "13",
                "Nine o'clock",
                "Fireworks end at nine",
            ),
        ],
    )
    .await;
    s.call(
        "update_entity",
        json!({"handle": "decision:springfield-14",
               "sets": {"replaces": "decision:springfield-13"}}),
    )
    .await;
    s.call(
        "archive_entity",
        json!({"handle": "decision:springfield-13", "reason": "replaced by 14"}),
    )
    .await;

    // ── (1) the topic, in one call: 12 and 14 in number order, then the topic ──
    let read = s.call("recall", json!({"subject": RULES})).await.json();
    assert_eq!(
        ids(&read),
        ["decision:springfield-12", "decision:springfield-14", RULES],
        "the standing decisions lead, by number, and the replaced one is not live: {read}"
    );
    let objects = read["objects"].as_array().expect("objects");
    for (object, number, words) in [
        (&objects[0], "12", "Dogs on leads in the square"),
        (&objects[1], "14", "Fireworks end at ten"),
    ] {
        assert_eq!(object["fields"]["number"], number, "{object}");
        assert!(
            object["facts"].to_string().contains(words),
            "a decision serves its words without a records ask: {object}"
        );
    }
    // The replaced decision is counted, and names what replaced it.
    assert_eq!(
        read["decisions_archived"]["replaced"]["by"]["decision:springfield-13"],
        "decision:springfield-14",
        "{read}"
    );
    assert_eq!(read["decisions_archived"]["replaced"]["count"], 1, "{read}");
    story.finish().await;
}

#[tokio::test]
async fn a_replaced_decision_read_by_its_number_says_what_replaced_it_with_the_new_words() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    rules_of_springfield(
        &s,
        &[
            (
                "springfield-13",
                "13",
                "Nine o'clock",
                "Fireworks end at nine",
            ),
            (
                "springfield-14",
                "14",
                "Ten o'clock",
                "Fireworks end at ten",
            ),
        ],
    )
    .await;
    s.call(
        "update_entity",
        json!({"handle": "decision:springfield-14",
               "sets": {"replaces": "decision:springfield-13"}}),
    )
    .await;
    s.call(
        "archive_entity",
        json!({"handle": "decision:springfield-13", "reason": "replaced by 14"}),
    )
    .await;

    // ── (2) the replaced decision by its number ─────────────────────────────
    let read = s.recall(&format!("{RULES}/13")).await.json();
    let thirteen = &read["objects"][0];
    assert_eq!(thirteen["id"], "decision:springfield-13", "{read}");
    assert_eq!(
        thirteen["replaced_by"]["handle"], "decision:springfield-14",
        "{read}"
    );
    assert!(
        thirteen["replaced_by"]["words"]
            .to_string()
            .contains("Fireworks end at ten"),
        "it carries the words of the one that replaced it: {read}"
    );
    // Paired: the live decision replaces another and has nothing replacing it.
    let fourteen = s.recall(&format!("{RULES}/14")).await.json();
    assert!(
        fourteen["objects"][0].get("replaced_by").is_none(),
        "a standing decision is not replaced: {fourteen}"
    );
    story.finish().await;
}

#[tokio::test]
async fn a_topic_of_sixty_decisions_stays_under_the_ceiling_and_says_how_to_read_on() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("topic:rules-shelbyville", "Shelbyville rules").await;
    s.call(
        "capture",
        json!({"subject": "topic:rules-shelbyville", "content": "numbers its members",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    let long = "The council recorded this rule at length, with its reasons and its exceptions, \
                so that nobody reading it later has to ask what was meant. "
        .repeat(3);
    for n in 1..=60 {
        let slug = format!("sv-{}", spelled(n));
        s.call(
            "add_entity",
            json!({"kind": "decision", "handle": slug, "name": format!("Edict {}", spelled(n)),
                   "source": "user-named", "parent": "topic:rules-shelbyville",
                   "sets": {"number": n.to_string()}}),
        )
        .await;
        s.call(
            "capture",
            json!({"subject": format!("decision:{slug}"), "content": format!("{long} ({n})"),
                   "provenance": "testimony"}),
        )
        .await;
    }
    let answer = s
        .call("recall", json!({"subject": "topic:rules-shelbyville"}))
        .await;
    let read = answer.json();
    assert!(
        answer.raw().chars().count() <= jojobot_domain::text::ANSWER_CEILING,
        "the answer is under the ceiling: {} characters",
        answer.raw().chars().count()
    );
    let shown = read["objects"].as_array().expect("objects").len();
    assert!(
        shown > 1 && shown < 61,
        "some decisions lead, not all: {shown}"
    );
    // The handles are scrambled letters, so handle order is not number order: the
    // ones that lead are the lowest numbers, in order.
    let numbers: Vec<u64> = read["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .take(shown - 1)
        .map(|object| {
            object["fields"]["number"]
                .as_str()
                .and_then(|held| held.parse().ok())
                .unwrap_or_else(|| panic!("a decision carries its number: {object}"))
        })
        .collect();
    assert_eq!(
        numbers,
        (1..shown as u64).collect::<Vec<_>>(),
        "number order, not handle order: {read}"
    );
    let left = read["decisions_not_shown"]["count"]
        .as_u64()
        .unwrap_or_else(|| panic!("the rest are counted: {read}"));
    assert_eq!(left as usize + shown - 1, 60, "{read}");
    assert!(
        read["decisions_not_shown"]["how_to_proceed"]
            .as_str()
            .is_some_and(|how| how.contains("recall")),
        "never a bare count: {read}"
    );
    story.finish().await;
}
