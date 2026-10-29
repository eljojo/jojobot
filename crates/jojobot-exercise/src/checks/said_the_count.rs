//! A count is said as a whole figure or word, never found inside another.

use serde_json::json;

/// A read of one thing holding one claim whose details say `said`.
fn read(said: &str) -> String {
    json!({"objects": [{"fields": {}, "facts": [{"content": "the pile", "details": said}]}]})
        .to_string()
}

/// **A figure inside a date or a longer number is not the count**, and a word
/// inside a longer word is not the count word.
#[test]
fn a_count_inside_a_date_or_a_longer_word_is_not_said() {
    for said in ["checked on 2026-08-13", "13 are waiting", "someone looked"] {
        let wrongly =
            super::said_the_count(&read(said), 3) || super::said_the_count(&read(said), 1);
        assert!(!wrongly, "{said:?} was taken for a count");
    }
    for (said, count) in [
        ("3 are waiting", 3),
        ("three are still waiting", 3),
        ("still waiting: 3.", 3),
        ("One is waiting", 1),
    ] {
        assert!(
            super::said_the_count(&read(said), count),
            "{said:?} was not read as {count}"
        );
    }
}
