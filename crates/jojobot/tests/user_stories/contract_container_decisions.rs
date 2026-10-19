//! **This file is a contract, not a story.** It pins that a container holding
//! more decisions than one answer can carry stays under the one ceiling, leads
//! with the lowest numbers in number order, and counts the rest with the call
//! that reads on. The use case, reading what the town decided, is told in
//! `container_decisions.rs`.

use serde_json::json;

use super::dsl::Story;

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
