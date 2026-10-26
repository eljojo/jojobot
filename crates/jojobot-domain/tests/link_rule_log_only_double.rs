//! **The in-memory double read while the link rule is log-only**, over values the
//! rule that is on marked (rule 646). The switch is process-wide, so the cases
//! share one test and a binary of their own, as the cases that switch it on do.
//!
//! The double is the stand-in the stories and the contract run against. Under
//! log-only it must serve what the rule that is on serves for a value that wears a
//! mark, and serve any other value as it did before the marks.

use jiff::civil::date;
use jojobot_domain::memory::testing::{InMemoryMemory, contract};
use jojobot_domain::memory::{EntityId, LinkRule, Memory, NewEntity, NewFact, set_link_rule};

async fn thing(memory: &InMemoryMemory, handle: &str, name: &str) -> EntityId {
    let id = EntityId(handle.to_string());
    memory
        .add_entity(NewEntity::new(id.clone(), name, "double-fixture"))
        .await
        .unwrap()
        .written()
        .unwrap();
    id
}

async fn state(memory: &InMemoryMemory, holder: &EntityId, fields: &[(&str, String)]) {
    let fact = NewFact {
        fields: fields
            .iter()
            .map(|(key, value)| (key.to_string(), value.clone()))
            .collect(),
        ..NewFact::about(holder.clone(), "holds some values", date(2026, 9, 1))
    };
    memory
        .capture(contract::state(fact))
        .await
        .unwrap()
        .written()
        .unwrap();
}

/// **A list that mixes words with handles, written while the rule is on, is
/// served under log-only with each mark as the handle it names and each word as
/// written**: the words keep their place, and a thing named twice is named once.
/// A list of words alone is served as written beside it.
async fn a_marked_mixed_list_is_served_as_the_handles_and_the_words() {
    let memory = InMemoryMemory::booted();
    let holder = thing(&memory, "thing:contract-links-holder", "Holder").await;
    let alpha = thing(&memory, "work:contract-links-alpha", "Alpha").await;
    let beta = thing(&memory, "work:contract-links-beta", "Beta").await;
    set_link_rule(LinkRule::On);
    state(
        &memory,
        &holder,
        &[
            (
                "labels",
                format!("kitchen, {alpha}, garden, {beta}, {alpha}"),
            ),
            ("words", "kitchen, garden".to_string()),
        ],
    )
    .await;
    set_link_rule(LinkRule::LogOnly);
    let fields = memory.fields(&holder).await.unwrap();
    assert_eq!(
        fields["labels"],
        format!("kitchen, {alpha}, garden, {beta}"),
        "log-only serves the handles of the marks, the words in their place, a thing once"
    );
    assert_eq!(
        fields["words"], "kitchen, garden",
        "a list of words is served as written"
    );
}

/// **A value that is a mark followed by a mark is served once, not composed
/// twice.** The text is one item that wears a mark, and the mark behind it is
/// part of the text: one pass serves the text without its first mark, and a
/// second pass over the answer would strip the second.
async fn a_double_mark_is_served_composed_once() {
    let memory = InMemoryMemory::booted();
    let holder = thing(&memory, "thing:contract-links-holder", "Holder").await;
    set_link_rule(LinkRule::LogOnly);
    // No write stores a value that starts with a mark: only jojobot writes one. The
    // row is a stored one, as a store an older build filled holds it.
    memory.fields_past_the_guard(&holder, &[("note", "@#@#kitchen")]);
    let fields = memory.fields(&holder).await.unwrap();
    assert_eq!(
        fields["note"], "@#kitchen",
        "one pass strips one mark, and the answer keeps the second"
    );
}

#[tokio::test]
async fn the_double_reads_a_marked_value_under_log_only_as_the_rule_that_is_on_does() {
    a_marked_mixed_list_is_served_as_the_handles_and_the_words().await;
    a_double_mark_is_served_composed_once().await;
}
