//! **The value rule of a thought's body cap, as a table.** The cap measures a
//! thought's content and details together, in characters, against the
//! container's own cap or the default one. Storage answers the same rule through
//! the contract case `a_thoughts_details_count_against_its_body_cap`.

use crate::memory::{DEFAULT_THOUGHT_BODY_CAP, EntityId, MemoryError, refuses_thought_over_cap};

/// The length and the cap the refusal names, or `None` when the thought lands.
fn refused(
    content: &str,
    details: Option<&str>,
    cap: Option<usize>,
    capacity: Option<usize>,
) -> Option<(usize, usize)> {
    match refuses_thought_over_cap(
        &EntityId("bot:gamma".into()),
        content,
        details,
        cap,
        capacity,
    ) {
        Some(MemoryError::ThoughtTooLong { len, cap, .. }) => Some((len, cap)),
        Some(other) => panic!("expected ThoughtTooLong, got {other:?}"),
        None => None,
    }
}

#[test]
fn content_alone_over_the_cap_is_refused() {
    assert_eq!(refused("abcdef", None, Some(5), None), Some((6, 5)));
}

#[test]
fn details_that_push_it_over_the_cap_are_refused() {
    assert_eq!(refused("abc", Some("def"), Some(5), None), Some((6, 5)));
}

#[test]
fn content_and_details_at_the_cap_land() {
    assert_eq!(refused("abc", Some("de"), Some(5), None), None);
}

#[test]
fn the_default_cap_binds_a_container_with_a_capacity_and_counts_details() {
    let content = "x".repeat(DEFAULT_THOUGHT_BODY_CAP);
    assert_eq!(refused(&content, None, None, Some(10)), None);
    assert_eq!(
        refused(&content, Some("y"), None, Some(10)),
        Some((DEFAULT_THOUGHT_BODY_CAP + 1, DEFAULT_THOUGHT_BODY_CAP)),
    );
}

#[test]
fn a_container_with_no_cap_and_no_capacity_is_not_capped() {
    let long = "z".repeat(500);
    assert_eq!(refused(&long, Some(&long), None, None), None);
}

#[test]
fn length_is_in_characters_and_not_bytes() {
    assert_eq!(refused("ééé", Some("éé"), Some(5), None), None);
    assert_eq!(refused("ééé", Some("ééé"), Some(5), None), Some((6, 5)));
}
