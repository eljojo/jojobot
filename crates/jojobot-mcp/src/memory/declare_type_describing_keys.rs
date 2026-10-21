//! **The text a caller reads names the keys that label a claim** (rule 466), so a
//! declaration of one is not a surprise the refusal has to explain.

use super::DeclareTypeArgs;
use jojobot_domain::memory::types::DESCRIBING_KEYS;

/// **Every key the domain fixes is in the description of the argument that
/// takes the keys**, so a seventh one added to the domain fails here until the
/// text says it. The tool's own description stays within its length.
#[test]
fn the_keys_argument_names_every_key_that_describes_a_record() {
    let schema =
        serde_json::to_value(schemars::schema_for!(DeclareTypeArgs)).expect("schema serialises");
    let said = schema["properties"]["fields"]["description"]
        .as_str()
        .expect("the keys argument has a description");
    for key in DESCRIBING_KEYS {
        assert!(said.contains(key), "{key} is not named: {said}");
    }
}
