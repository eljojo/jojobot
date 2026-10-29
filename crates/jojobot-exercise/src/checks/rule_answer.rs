//! A rule dismissed in an answer's details is not a rule the answer names.

use serde_json::{Value, json};

fn answer(content: &str, details: &str) -> Vec<Value> {
    vec![json!({"status": "active", "content": content, "details": details})]
}

/// **Dismissing a rule by number in `details` does not convict the answer of
/// including it**, and naming it in `content` does. A rule cited only in
/// `details` still counts as the required one.
#[test]
fn a_rule_dismissed_in_the_details_is_not_included() {
    let dismissed = answer("rule 241 is the answer", "rule 129 is not this one");
    assert!(
        super::rule_answer_verdict("work:pm-state", &dismissed, &[241], &[129]).is_ok(),
        "a dismissal in details convicted the answer"
    );

    let named = answer("rules 241 and 129", "");
    assert!(
        super::rule_answer_verdict("work:pm-state", &named, &[241], &[129]).is_err(),
        "a forbidden rule named in content did not convict the answer"
    );

    let cited = answer("see the details", "rule 241 applies");
    assert!(
        super::rule_answer_verdict("work:pm-state", &cited, &[241], &[129]).is_ok(),
        "a required rule cited in details was not found"
    );
}
