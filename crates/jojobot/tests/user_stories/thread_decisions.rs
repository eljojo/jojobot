//! "The kitchen renovation has been going for months. When I ask my assistant
//! where it stands, does it say what we decided, or what it found out?"
//!
//! A thread collects research, and good research rarely says what was chosen. A
//! session that reads the research first comes away well informed and
//! confidently wrong. **A decision is its own child of the thread, of the
//! `decision` kind, and reading the thread puts its standing decisions first**,
//! whatever else the answer holds. A decision that changes is never reworded: a
//! new decision names the one it replaces, and the old one is archived with its
//! reason.

use serde_json::{Value, json};

use jojobot_domain::text::ANSWER_CEILING;

use super::dsl::{Session, Story};

const KITCHEN: &str = "thread:simpson-kitchen";

/// The handles of an answer's objects, in the order they came back.
fn handles(answer: &Value) -> Vec<String> {
    answer["objects"]
        .as_array()
        .map(|all| {
            all.iter()
                .filter_map(|o| o["id"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// The object of this handle in an answer.
fn the_object<'a>(answer: &'a Value, handle: &str) -> &'a Value {
    answer["objects"]
        .as_array()
        .and_then(|all| all.iter().find(|o| o["id"] == handle))
        .unwrap_or_else(|| panic!("{handle} is in the answer: {answer}"))
}

/// The kitchen thread, two standing decisions under it and ten small pieces of
/// research on the thread itself.
async fn a_kitchen_with_two_decisions_and_ten_findings(s: &Session) {
    s.add(KITCHEN, "The Simpsons' kitchen renovation").await;
    s.add_under(KITCHEN, "decision:cabinets-oak", "Cabinets in oak")
        .await;
    s.fact(
        "decision:cabinets-oak",
        "Marge decided the cabinets are oak, not painted pine",
    )
    .await;
    s.add_under(KITCHEN, "decision:floor-tile", "Floor in tile")
        .await;
    s.fact(
        "decision:floor-tile",
        "Homer decided the floor is slate tile, not vinyl",
    )
    .await;
    for finding in 1..=10 {
        s.fact(
            KITCHEN,
            &format!("finding {finding}: a contractor quoted a different price for the work"),
        )
        .await;
    }
}

/// **A cold read of the thread shows what was decided before what was found.**
/// The decisions are children, with their words, ahead of the thread itself and
/// its ten findings, and nobody asked for records.
#[tokio::test]
async fn a_cold_read_of_a_thread_puts_its_decisions_before_its_research() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_kitchen_with_two_decisions_and_ten_findings(&s).await;

    let read = s.call("recall", json!({"subject": KITCHEN})).await;
    let answer = read.json();
    assert_eq!(
        handles(&answer),
        vec!["decision:cabinets-oak", "decision:floor-tile", KITCHEN],
        "the standing decisions come first, then the thread: {answer}"
    );
    for (decision, words) in [
        ("decision:cabinets-oak", "oak"),
        ("decision:floor-tile", "slate"),
    ] {
        let facts = the_object(&answer, decision)["facts"]
            .as_array()
            .unwrap_or_else(|| panic!("{decision} carries its words unasked: {answer}"))
            .clone();
        assert!(
            facts[0]["content"].as_str().unwrap_or("").contains(words),
            "{decision} says what was decided: {facts:?}"
        );
    }
    // The positive the next lines rest on: the thread is in the same answer, and
    // its research is behind the pointer rather than in front of the decisions.
    let thread = the_object(&answer, KITCHEN);
    assert!(
        thread.get("facts").is_none(),
        "the research was not asked for: {thread}"
    );
    read.never_says("finding 3");
    story.finish().await;
}

/// **A thread nobody decided anything under answers as it always did**: itself,
/// alone, with nothing added in front of it.
#[tokio::test]
async fn a_thread_with_no_decisions_answers_alone() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(KITCHEN, "The Simpsons' kitchen renovation").await;
    s.fact(KITCHEN, "finding 1: the old cabinets are rotten")
        .await;
    s.add("project:atlas", "Atlas").await;
    s.add_under("project:atlas", "work:phi", "Phi").await;
    s.add_under("project:atlas", "decision:sink-copper", "Sink")
        .await;
    s.fact("decision:sink-copper", "the sink is copper").await;

    let thread = s.call("recall", json!({"subject": KITCHEN})).await.json();
    assert_eq!(handles(&thread), vec![KITCHEN], "{thread}");
    // A decision is a thread's child. What sits under anything else is not pulled
    // in front of its parent.
    let project = s
        .call("recall", json!({"subject": "project:atlas"}))
        .await
        .json();
    assert_eq!(handles(&project), vec!["project:atlas"], "{project}");
    story.finish().await;
}

/// **With the research grown past the ceiling, the decisions still come back and
/// the answer names what it left out.** The thread holds three findings that
/// together are larger than one answer; asked for its records, the decisions
/// fill first, the thread comes after with the findings that fit, and the count
/// of findings it left out is on the thread beside the call that returns them.
#[tokio::test]
async fn research_past_the_ceiling_leaves_the_decisions_in_and_counts_the_research_out() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_kitchen_with_two_decisions_and_ten_findings(&s).await;
    for bulk in 1..=3 {
        let page: String = (0..1000)
            .map(|word| format!("report{bulk}word{word} "))
            .collect();
        s.fact(KITCHEN, &format!("bulky finding {bulk}: {page}"))
            .await;
    }

    let (answer, size) = s
        .answer_and_size("recall", json!({"subject": KITCHEN, "facts": true}))
        .await;
    assert!(
        size <= ANSWER_CEILING,
        "the answer fits one ceiling: {size}"
    );
    let ids = handles(&answer);
    assert_eq!(
        ids[..2],
        ["decision:cabinets-oak", "decision:floor-tile"],
        "the decisions survive the cut: {ids:?}"
    );
    assert_eq!(ids[2], KITCHEN, "the thread follows them: {ids:?}");
    let thread = the_object(&answer, KITCHEN);
    let shown = thread["facts"].as_array().map_or(0, Vec::len);
    let left_out = answer["not_shown"]["count"]
        .as_u64()
        .unwrap_or_else(|| panic!("the answer names what it left out: {answer}"));
    assert!(
        left_out >= 1,
        "something of the research was left out: {answer}"
    );
    assert_eq!(
        shown as u64 + left_out,
        13,
        "what is shown and what is left out are the thread's thirteen findings: {answer}"
    );
    assert_eq!(
        answer["not_shown"]["offset"].as_u64(),
        Some(shown as u64),
        "and the offset that reads on starts after the last one shown: {answer}"
    );

    // Repeating the call with that offset reads the research on, and only the
    // research: the decisions were in the first part and are not sent twice.
    let (next, size) = s
        .answer_and_size(
            "recall",
            json!({"subject": KITCHEN, "facts": true, "offset": shown}),
        )
        .await;
    assert!(
        size <= ANSWER_CEILING,
        "the next part fits one ceiling: {size}"
    );
    assert_eq!(handles(&next), vec![KITCHEN], "{next}");
    assert_eq!(
        the_object(&next, KITCHEN)["facts"][0]["address"],
        format!("{KITCHEN}#f{}", shown + 1),
        "and it starts where the first part stopped: {next}"
    );
    story.finish().await;
}

/// **A decision that changes is never reworded.** The new decision is a child of
/// its own whose record names the old one; the old child is archived with its
/// reason. The thread then shows only the replacement first, says what it left
/// out, and the old decision still reads, as archived, by its own handle.
#[tokio::test]
async fn a_superseded_decision_reads_as_archived_and_its_replacement_leads() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    a_kitchen_with_two_decisions_and_ten_findings(&s).await;
    let old = s
        .call(
            "recall",
            json!({"subject": "decision:cabinets-oak", "facts": true}),
        )
        .await
        .json();
    let old_record = old["objects"][0]["facts"][0]["address"]
        .as_str()
        .expect("the old decision's record has an address")
        .to_string();

    s.add_under(KITCHEN, "decision:cabinets-walnut", "Cabinets in walnut")
        .await;
    s.call(
        "capture",
        json!({
            "subject": "decision:cabinets-walnut",
            "content": "Marge decided the cabinets are walnut after the oak went over budget",
            "provenance": "testimony",
            "derived_from": old_record,
        }),
    )
    .await;
    s.call(
        "archive_entity",
        json!({
            "handle": "decision:cabinets-oak",
            "reason": "replaced by decision:cabinets-walnut once the oak went over budget",
        }),
    )
    .await;

    let read = s.call("recall", json!({"subject": KITCHEN})).await;
    let answer = read.json();
    assert_eq!(
        handles(&answer),
        vec!["decision:cabinets-walnut", "decision:floor-tile", KITCHEN],
        "only the replacement leads, beside the decision nobody changed: {answer}"
    );
    let replacement = the_object(&answer, "decision:cabinets-walnut");
    assert_eq!(
        replacement["facts"][0]["derived_from"], old_record,
        "the replacement points at the record it replaces: {replacement}"
    );
    assert!(
        answer["decisions_archived"]
            .to_string()
            .contains("decision:cabinets-oak"),
        "the answer names the decision it left out: {answer}"
    );

    // The old decision is still there, as it was said, and reads as archived.
    let gone = s
        .call(
            "recall",
            json!({"subject": "decision:cabinets-oak", "facts": true}),
        )
        .await;
    gone.says("oak");
    gone.says("replaced by decision:cabinets-walnut");
    story.finish().await;
}

/// **Decisions that together pass the ceiling are cut like any list**: those
/// that fit lead the thread, the answer counts the rest and names the browse
/// that returns them. The thread keeps room for itself, so the first part still
/// says where its research is.
#[tokio::test]
async fn decisions_past_the_ceiling_are_counted_and_a_browse_returns_them() {
    let story = Story::begin("bot:otto").await;
    let s = story.session().await;
    s.add(KITCHEN, "The Simpsons' kitchen renovation").await;
    s.fact(KITCHEN, "finding 1: the old cabinets are rotten")
        .await;
    for (handle, name) in [
        ("decision:cabinets-oak", "Cabinets"),
        ("decision:floor-tile", "Floor"),
        ("decision:paint-cream", "Paint"),
        ("decision:sink-copper", "Sink"),
    ] {
        s.add_under(KITCHEN, handle, name).await;
        let reasons: String = (0..900)
            .map(|word| format!("{}reason{word} ", name.to_lowercase()))
            .collect();
        s.fact(handle, &format!("{name} decided, because {reasons}"))
            .await;
    }

    let (answer, size) = s
        .answer_and_size("recall", json!({"subject": KITCHEN}))
        .await;
    assert!(
        size <= ANSWER_CEILING,
        "the answer fits one ceiling: {size}"
    );
    let ids = handles(&answer);
    assert_eq!(ids.last().map(String::as_str), Some(KITCHEN), "{ids:?}");
    let led = ids.len() - 1;
    assert!(
        (1..4).contains(&led),
        "some decisions lead and some do not fit: {ids:?}"
    );
    let left = answer["decisions_not_shown"]["count"]
        .as_u64()
        .unwrap_or_else(|| panic!("the answer counts the decisions it left out: {answer}"));
    assert_eq!(led as u64 + left, 4, "{answer}");

    let rest = s
        .call(
            "recall",
            json!({"kind": "decision", "parent": KITCHEN, "facts": true}),
        )
        .await
        .json();
    let browsed = handles(&rest);
    assert!(
        ids[..led].iter().all(|id| browsed.contains(id)),
        "the browse the answer names returns the decisions: {browsed:?}"
    );
    story.finish().await;
}
