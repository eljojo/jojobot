//! **Whether a field value links is read from the value alone** (rules 355 and
//! 646), by the decision of [`at_write`]: each comma item that is a handle is a
//! link under any key, loose items beside it stay text, and the stored form says
//! which items were decided to be links by wearing the mark.

use super::*;

fn handles(value: &str) -> Vec<String> {
    kinds::load_shipped();
    at_write::handles_in_value(value)
        .into_iter()
        .map(|id| id.to_string())
        .collect()
}

#[test]
fn a_handle_among_loose_items_is_a_link_and_a_sentence_is_prose() {
    assert_eq!(handles("topic:all-rules"), ["topic:all-rules"]);
    assert_eq!(
        handles("topic:all-rules, topic:widgets"),
        ["topic:all-rules", "topic:widgets"]
    );
    assert_eq!(
        handles("kitchen, topic:all-rules"),
        ["topic:all-rules"],
        "a loose item beside a handle does not unlink the handle"
    );
    assert!(handles("kitchen, garage").is_empty(), "no handle, no link");
    assert!(
        handles("read topic:all-rules before friday").is_empty(),
        "a handle inside a sentence is prose"
    );
}

#[test]
fn the_links_of_a_stored_value_are_the_items_wearing_the_mark() {
    let stored = format!(
        "kitchen, {}, topic:widgets, {}, {}",
        marked_stored_handle("abc"),
        marked_stored_handle("abc"),
        marked_stored_handle("xyz"),
    );
    assert_eq!(
        at_write::field_link_targets(&stored),
        ["abc", "xyz"],
        "marked items only, once each, in order; a handle written bare is text"
    );
    assert!(
        at_write::field_link_targets("abc, xyz").is_empty(),
        "a bare id is text"
    );
}

#[test]
fn a_repoint_changes_the_links_and_leaves_loose_text_and_repeats_of_it() {
    let stored = format!(
        "kitchen, {}, kitchen, {}",
        marked_stored_handle("old"),
        marked_stored_handle("new"),
    );
    assert_eq!(
        at_write::repoint_field_value(&stored, "old", "new"),
        Some(format!("kitchen, {}, kitchen", marked_stored_handle("new"))),
        "the list names the survivor once; the loose item twice written stays twice"
    );
    assert_eq!(at_write::repoint_field_value(&stored, "other", "new"), None);
}

#[test]
fn what_a_record_names_does_not_depend_on_what_any_type_declares() {
    kinds::load_shipped();
    let fields = BTreeMap::from([("tags".to_string(), "kitchen, topic:widgets".to_string())]);
    assert_eq!(at_write::names_in_fields(&fields), ["topic:widgets"]);
    assert!(at_write::fields_name_target(
        &fields,
        &EntityId("topic:widgets".into())
    ));
    assert!(!at_write::fields_name_target(
        &fields,
        &EntityId("topic:all-rules".into())
    ));
    assert_eq!(
        at_write::referenced_by(&fields),
        [EntityId("topic:widgets".into())],
        "the write guard asks of the same handles"
    );
}
