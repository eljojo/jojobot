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

/// "A rule was corrected in the next town's topic. Does the old one say so?"
///
/// **A decision names what replaced it wherever the replacement lives.** The
/// decision that replaces another is found by the `replaces` key it holds, so it
/// need not sit under the same topic. Reading the replaced one carries the new
/// words; a decision nobody replaced carries no `replaced_by`.
#[tokio::test]
async fn a_decision_replaced_by_one_under_another_topic_says_so_with_the_new_words() {
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
                "Dogs on leads",
                "Dogs on leads in the square",
            ),
        ],
    )
    .await;
    // The correction is made in the next town's topic.
    s.add("topic:rules-shelbyville", "Shelbyville rules").await;
    s.call(
        "capture",
        json!({"subject": "topic:rules-shelbyville", "content": "numbers its members by number",
               "provenance": "testimony", "sets": {"numbers_by": "number"}}),
    )
    .await;
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": "shelbyville-40", "name": "Eleven o'clock",
               "source": "user-named", "parent": "topic:rules-shelbyville",
               "sets": {"number": "40", "replaces": "decision:springfield-13"}}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": "decision:shelbyville-40", "content": "Fireworks end at eleven",
               "provenance": "testimony"}),
    )
    .await;
    s.call(
        "archive_entity",
        json!({"handle": "decision:springfield-13", "reason": "replaced by 40"}),
    )
    .await;

    let read = s.recall("decision:springfield-13").await.json();
    let replaced = &read["objects"][0];
    assert_eq!(replaced["id"], "decision:springfield-13", "{read}");
    assert_eq!(
        replaced["replaced_by"]["handle"], "decision:shelbyville-40",
        "{read}"
    );
    assert!(
        replaced["replaced_by"]["words"]
            .to_string()
            .contains("Fireworks end at eleven"),
        "it carries the words of the one that replaced it: {read}"
    );
    // Paired: a decision nobody replaced says nothing of the kind.
    let standing = s.recall("decision:springfield-14").await.json();
    assert!(
        standing["objects"][0].get("replaced_by").is_none(),
        "a standing decision is not replaced: {standing}"
    );
    story.finish().await;
}

/// A decision under the thread `thread:krusty-burger`, with its words, and the
/// decision it replaces when it replaces one.
async fn burger_decision(s: &Session, slug: &str, name: &str, words: &str, replaces: Option<&str>) {
    let mut sets = json!({});
    if let Some(old) = replaces {
        sets["replaces"] = old.into();
    }
    s.call(
        "add_entity",
        json!({"kind": "decision", "handle": slug, "name": name,
               "source": "user-named", "parent": "thread:krusty-burger", "sets": sets}),
    )
    .await;
    s.call(
        "capture",
        json!({"subject": format!("decision:{slug}"), "content": words,
               "provenance": "testimony"}),
    )
    .await;
}

async fn retire(s: &Session, slug: &str) {
    s.call(
        "archive_entity",
        json!({"handle": format!("decision:{slug}"), "reason": "replaced"}),
    )
    .await;
}

/// "The fryers rule was replaced by the grill rule, and the grill rule by the
/// doors rule. Which one stands?"
///
/// Reading any decision in a chain of replacements names the one that replaced
/// it and, when that one was replaced too, the one that stands now. A one-step
/// replacement reads as it did, and a loop in a corrupt chain ends.
#[tokio::test]
async fn a_replaced_decision_in_a_chain_points_at_the_one_standing_now() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thread:krusty-burger", "Krusty burger").await;
    burger_decision(
        &s,
        "fryers",
        "Fryers on at ten",
        "Fryers go on at ten",
        None,
    )
    .await;
    burger_decision(
        &s,
        "grill",
        "Grill first at eleven",
        "The grill goes first at eleven",
        Some("decision:fryers"),
    )
    .await;
    burger_decision(
        &s,
        "doors",
        "Doors open at noon",
        "The doors open at noon",
        Some("decision:grill"),
    )
    .await;
    retire(&s, "fryers").await;
    retire(&s, "grill").await;

    // (1) the oldest names its direct replacer and the one that stands.
    let oldest = s.recall("decision:fryers").await.json();
    let read = &oldest["objects"][0];
    assert_eq!(read["replaced_by"]["handle"], "decision:grill", "{oldest}");
    assert_eq!(
        read["standing_replacement"]["handle"], "decision:doors",
        "the end of the chain stands: {oldest}"
    );
    assert!(
        read["standing_replacement"]["words"]
            .to_string()
            .contains("The doors open at noon"),
        "it carries the words of the one that stands: {oldest}"
    );

    // (2) the middle one is replaced by one that stands: no second pointer.
    let middle = s.recall("decision:grill").await.json();
    assert_eq!(
        middle["objects"][0]["replaced_by"]["handle"], "decision:doors",
        "{middle}"
    );
    assert!(
        middle["objects"][0].get("standing_replacement").is_none(),
        "the direct replacer is the one that stands: {middle}"
    );

    // The thread names both replaced decisions, each with its direct replacer.
    let thread = s.recall("thread:krusty-burger").await.json();
    let named = thread.to_string();
    assert!(
        named.contains("decision:fryers") && named.contains("decision:grill"),
        "both archived decisions are named by the thread: {thread}"
    );
    story.finish().await;
}

/// A decision replaced in one step reads as before, and a corrupt chain that
/// loops back on itself ends rather than walking for ever.
#[tokio::test]
async fn a_one_step_replacement_reads_as_before_and_a_loop_in_a_chain_ends() {
    let story = Story::begin("bot:gamma").await;
    let s = story.session().await;
    s.add("thread:krusty-burger", "Krusty burger").await;
    burger_decision(
        &s,
        "napkins",
        "Napkins by the till",
        "Napkins by the till",
        None,
    )
    .await;
    burger_decision(
        &s,
        "straws",
        "Straws on request",
        "Straws only on request",
        Some("decision:napkins"),
    )
    .await;
    retire(&s, "napkins").await;

    // (3) one step: the replacer is the one that stands, and no second pointer.
    let one = s.recall("decision:napkins").await.json();
    assert_eq!(
        one["objects"][0]["replaced_by"]["handle"], "decision:straws",
        "{one}"
    );
    assert!(
        one["objects"][0].get("standing_replacement").is_none(),
        "{one}"
    );

    // (4) two retired decisions that each claim to replace the other.
    burger_decision(&s, "tray-red", "Red trays", "Trays are red", None).await;
    burger_decision(
        &s,
        "tray-blue",
        "Blue trays",
        "Trays are blue",
        Some("decision:tray-red"),
    )
    .await;
    s.call(
        "update_entity",
        json!({"handle": "decision:tray-red", "sets": {"replaces": "decision:tray-blue"}}),
    )
    .await;
    retire(&s, "tray-red").await;
    retire(&s, "tray-blue").await;
    let looped = s.recall("decision:tray-red").await.json();
    assert_eq!(
        looped["objects"][0]["replaced_by"]["handle"], "decision:tray-blue",
        "{looped}"
    );
    assert!(
        looped["objects"][0].get("standing_replacement").is_none(),
        "nothing in a loop stands: {looped}"
    );
    story.finish().await;
}
