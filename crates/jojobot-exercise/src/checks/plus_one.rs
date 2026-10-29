//! A claim that denies a plus-one does not say one is coming.

use serde_json::json;

/// **Naming a plus-one is not saying one is coming.** A denial in the same
/// sentence turns it round, in `content` or in `details`; a denial in another
/// sentence does not.
#[test]
fn a_denied_plus_one_is_not_a_plus_one_coming() {
    for said in [
        "no plus-one this time",
        "she isn't bringing a partner",
        "coming without a plus one",
    ] {
        assert!(
            !super::says_a_plus_one_is_coming(&json!({"content": said})),
            "{said:?} was read as a plus-one coming"
        );
        assert!(
            !super::says_a_plus_one_is_coming(&json!({"content": "rsvp", "details": said})),
            "{said:?} in details was read as a plus-one coming"
        );
    }
    for said in [
        "Milhouse is bringing a plus-one",
        "No dietary needs. A partner is coming too",
        "her partner will come",
    ] {
        assert!(
            super::says_a_plus_one_is_coming(&json!({"content": said})),
            "{said:?} was not read as a plus-one coming"
        );
    }
}
