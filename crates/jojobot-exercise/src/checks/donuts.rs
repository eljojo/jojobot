//! The donut words include both spellings, singular and plural.

use serde_json::json;

/// **A claim about what each guest eats is found whichever way the donut is
/// spelled.** "Doughnut" was in the list only as a plural.
#[test]
fn both_spellings_of_donut_count_as_naming_what_each_guest_eats() {
    for said in [
        "each eats a donut",
        "three donuts each",
        "one doughnut",
        "three doughnuts",
    ] {
        assert!(
            super::says_any(&json!({"content": said}), &super::DONUTS),
            "{said:?} was not read as a claim about the donuts"
        );
    }
    assert!(!super::says_any(
        &json!({"content": "three of the dishes"}),
        &super::DONUTS
    ));
}
