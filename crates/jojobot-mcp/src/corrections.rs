//! **The claims teaching and `update_fact`'s own text name the same three
//! corrections.** Pinned on the verbs each must name, read off the `status`
//! argument `update_fact` publishes, so a correction one text drops reddens
//! this rather than leaving a bot taught two moves by one text and three by the
//! other.

use crate::teaching::{CLAIMS_TEACHING, published_arguments};

#[test]
fn the_claims_teaching_names_the_corrections_update_fact_names() {
    let published = published_arguments("update_fact").expect("update_fact is a tool");
    let status = published["status"]["description"]
        .as_str()
        .expect("status describes itself");
    for verb in ["update_fact", "archived", "retract"] {
        assert!(status.contains(verb), "update_fact's status lacks {verb}");
        assert!(CLAIMS_TEACHING.contains(verb), "the teaching lacks {verb}");
    }
}
